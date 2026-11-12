//! opendocument graphic styles for drawings: fills, lines, arrowheads and
//! text areas, to and from the model. gradients, arrowheads, dash patterns
//! and fill pictures are named definitions in `office:styles`, which
//! `DrawResources` reads and `DrawDefinitions` writes.

use std::collections::{BTreeMap, HashMap};

use crate::model::{
    BoxMargins, Fill, Gradient, GradientKind, GradientStop, LineCap, LineEnd, LineJoin, Stroke,
    VerticalAlign,
};
use crate::units::{parse_length, parse_length_emu, twips_to_inches};
use crate::xml::{escape_attribute, Element};

/// a graphic style's properties, inherited ones underneath its own.
pub type GraphicProperties = HashMap<String, String>;

/// the attributes of `style:graphic-properties` the model reads; any
/// other is kept with the drawing (`DrawingPreserved::attributes`).
pub const MODELLED_GRAPHIC_ATTRIBUTES: &[&str] = &[
    "draw:stroke",
    "svg:stroke-color",
    "svg:stroke-width",
    "svg:stroke-opacity",
    "draw:stroke-dash",
    "svg:stroke-linecap",
    "draw:stroke-linejoin",
    "draw:marker-start",
    "draw:marker-start-width",
    "draw:marker-start-center",
    "draw:marker-end",
    "draw:marker-end-width",
    "draw:marker-end-center",
    "draw:fill",
    "draw:fill-color",
    "draw:opacity",
    "draw:fill-gradient-name",
    "draw:fill-image-name",
    "style:repeat",
    "draw:textarea-vertical-align",
    "draw:auto-grow-height",
    "fo:min-height",
    "fo:min-width",
    "fo:padding",
    "fo:padding-top",
    "fo:padding-bottom",
    "fo:padding-left",
    "fo:padding-right",
    "style:wrap",
    "style:run-through",
    "style:wrap-contour",
    "style:horizontal-pos",
    "style:horizontal-rel",
    "style:vertical-pos",
    "style:vertical-rel",
    "fo:background-color",
    "fo:border",
];

/// the named drawing definitions of a document's `office:styles`.
#[derive(Clone, Debug, Default)]
pub struct DrawResources {
    gradients: HashMap<String, Gradient>,
    /// arrowhead name to the model's kind of arrowhead
    markers: HashMap<String, String>,
    /// dash pattern name to the nearest drawingml dash name
    dashes: HashMap<String, String>,
    /// fill picture name to the picture's path in the package
    images: HashMap<String, String>,
}

impl DrawResources {
    /// takes in one child of `office:styles`, when it is a definition.
    pub fn add(&mut self, element: &Element) {
        let Some(name) = element.attr("draw:name") else {
            return;
        };
        let display = element.attr("draw:display-name").unwrap_or(name);
        match element.name.as_str() {
            "draw:gradient" => {
                if let Some(gradient) = read_gradient(element) {
                    self.gradients.insert(name.to_string(), gradient);
                }
            }
            "draw:marker" => {
                self.markers.insert(name.to_string(), marker_kind(display));
            }
            "draw:stroke-dash" => {
                self.dashes.insert(name.to_string(), dash_kind(element));
            }
            "draw:fill-image" => {
                if let Some(href) = element.attr("xlink:href") {
                    self.images
                        .insert(name.to_string(), href.trim_start_matches("./").to_string());
                }
            }
            _ => {}
        }
    }
}

fn hex(value: &str) -> Option<String> {
    let hex = value.trim().strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| hex.to_ascii_uppercase())
}

fn percent(value: &str) -> Option<f64> {
    value
        .trim()
        .strip_suffix('%')?
        .trim()
        .parse::<f64>()
        .ok()
        .map(|v| v / 100.0)
}

/// an opendocument angle ("90deg", "900" in tenths of a degree, "1.5rad").
fn degrees(value: &str) -> f64 {
    let value = value.trim();
    if let Some(number) = value.strip_suffix("deg") {
        return number.parse().unwrap_or(0.0);
    }
    if let Some(number) = value.strip_suffix("rad") {
        return number.parse::<f64>().unwrap_or(0.0).to_degrees();
    }
    // a bare number is tenths of a degree (opendocument 1.1)
    value
        .parse::<f64>()
        .map(|tenths| tenths / 10.0)
        .unwrap_or(0.0)
}

/// a `draw:gradient`. opendocument measures the angle counterclockwise
/// from top-to-bottom; drawingml clockwise from left-to-right, so a
/// drawingml angle is 90 minus the opendocument one. a radial gradient's
/// start colour is at the outside in opendocument and at the centre in
/// drawingml.
fn read_gradient(element: &Element) -> Option<Gradient> {
    let start = element
        .attr("draw:start-color")
        .and_then(hex)
        .unwrap_or_else(|| "000000".into());
    let end = element
        .attr("draw:end-color")
        .and_then(hex)
        .unwrap_or_else(|| "FFFFFF".into());
    let radial = matches!(
        element.attr("draw:style"),
        Some("radial" | "ellipsoid" | "square" | "rectangular")
    );
    let mut stops: Vec<GradientStop> = element
        .children_named("loext:gradient-stop")
        .filter_map(|stop| {
            Some(GradientStop {
                position: stop.attr("svg:offset")?.parse().ok()?,
                color: stop.attr("loext:color-value").and_then(hex)?,
                opacity: None,
            })
        })
        .collect();
    if stops.len() < 2 {
        stops = vec![
            GradientStop {
                position: 0.0,
                color: start,
                opacity: None,
            },
            GradientStop {
                position: 1.0,
                color: end,
                opacity: None,
            },
        ];
    }
    if radial {
        stops.reverse();
        for stop in &mut stops {
            stop.position = 1.0 - stop.position;
        }
    }
    let angle = element.attr("draw:angle").map(degrees).unwrap_or(0.0);
    Some(Gradient {
        kind: if radial {
            GradientKind::Radial
        } else {
            GradientKind::Linear
        },
        angle: (90.0 - angle).rem_euclid(360.0),
        stops,
    })
}

/// the kind of arrowhead a marker draws, from its name (libreoffice and
/// word's converted markers are named after their look).
fn marker_kind(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    let kind = if name.contains("oval") || name.contains("circle") {
        "oval"
    } else if name.contains("diamond") || name.contains("square 45") {
        "diamond"
    } else if name.contains("stealth") {
        "stealth"
    } else if name.contains("open") || name.contains("line arrow") {
        "arrow"
    } else {
        "triangle"
    };
    kind.to_string()
}

/// the nearest drawingml dash for a `draw:stroke-dash`.
fn dash_kind(element: &Element) -> String {
    if let Some(own) = element
        .attr("draw:name")
        .and_then(|name| name.strip_prefix("Letters_20_"))
    {
        return own.to_string();
    }
    let length = |name: &str| element.attr(name).and_then(percent).unwrap_or(1.0);
    let has_second = element.attr("draw:dots2").is_some_and(|d| d != "0");
    let first = length("draw:dots1-length");
    let kind = match (has_second, first) {
        (true, l) if l >= 7.0 => "longDashDot",
        (true, _) => "dashDot",
        (false, l) if l >= 7.0 => "longDash",
        (false, l) if l >= 2.0 => "dash",
        _ => "sysDot",
    };
    kind.to_string()
}

/// the fill a graphic style gives. `picture` turns a package path into a
/// media id.
pub fn read_fill(
    properties: &GraphicProperties,
    resources: &DrawResources,
    picture: &mut dyn FnMut(&str) -> Option<String>,
) -> Fill {
    let opacity = properties
        .get("draw:opacity")
        .and_then(|v| percent(v))
        .filter(|o| (*o - 1.0).abs() > 1e-9);
    match properties.get("draw:fill").map(String::as_str) {
        Some("none") | None => Fill::None,
        Some("gradient") => properties
            .get("draw:fill-gradient-name")
            .and_then(|name| resources.gradients.get(name))
            .cloned()
            .map(|mut gradient| {
                for stop in &mut gradient.stops {
                    stop.opacity = opacity;
                }
                Fill::Gradient(gradient)
            })
            .unwrap_or(Fill::None),
        Some("bitmap") => properties
            .get("draw:fill-image-name")
            .and_then(|name| resources.images.get(name))
            .and_then(|path| picture(path))
            .map(|media_id| Fill::Picture {
                media_id,
                tile: properties
                    .get("style:repeat")
                    .is_some_and(|r| r == "repeat"),
            })
            .unwrap_or(Fill::None),
        // a hatch is drawn over its background colour; the colour is kept
        Some(_) => match properties.get("draw:fill-color").and_then(|c| hex(c)) {
            Some(color) => Fill::Solid { color, opacity },
            None => Fill::None,
        },
    }
}

/// the line a graphic style gives, or `None` for no line.
pub fn read_stroke(properties: &GraphicProperties, resources: &DrawResources) -> Option<Stroke> {
    let kind = properties
        .get("draw:stroke")
        .map(String::as_str)
        .unwrap_or("solid");
    if kind == "none" {
        return None;
    }
    let color = properties
        .get("svg:stroke-color")
        .and_then(|c| hex(c))
        .unwrap_or_else(|| "000000".into());
    // opendocument's 0 width is a hairline; drawingml's thinnest is about
    // a quarter point
    let width_emu = properties
        .get("svg:stroke-width")
        .and_then(|w| parse_length_emu(w))
        .filter(|w| *w > 0)
        .unwrap_or(3175);
    let mut stroke = Stroke::solid(&color, crate::units::emu_to_twips(width_emu));
    stroke.width_emu = Some(width_emu);
    stroke.opacity = properties
        .get("svg:stroke-opacity")
        .and_then(|v| percent(v))
        .filter(|o| (*o - 1.0).abs() > 1e-9);
    if kind == "dash" {
        stroke.dash = properties
            .get("draw:stroke-dash")
            .and_then(|name| resources.dashes.get(name))
            .cloned()
            .unwrap_or_else(|| "dash".into());
    }
    stroke.cap = match properties.get("svg:stroke-linecap").map(String::as_str) {
        Some("round") => Some(LineCap::Round),
        Some("square") => Some(LineCap::Square),
        Some("butt") => Some(LineCap::Flat),
        _ => None,
    };
    stroke.join = match properties.get("draw:stroke-linejoin").map(String::as_str) {
        Some("round") => Some(LineJoin::Round),
        Some("bevel") => Some(LineJoin::Bevel),
        Some("miter") => Some(LineJoin::Miter),
        _ => None,
    };
    let end = |name: &str| {
        let marker = properties.get(name)?;
        Some(LineEnd {
            kind: resources
                .markers
                .get(marker)
                .cloned()
                .unwrap_or_else(|| marker_kind(marker)),
            width: None,
            length: None,
        })
    };
    stroke.head = end("draw:marker-start");
    stroke.tail = end("draw:marker-end");
    Some(stroke)
}

/// the inner margins, vertical placement and growing of a shape's text.
pub fn read_text_area(properties: &GraphicProperties) -> (BoxMargins, Option<VerticalAlign>, bool) {
    let all = properties.get("fo:padding").and_then(|p| parse_length(p));
    let side = |name: &str| properties.get(name).and_then(|p| parse_length(p)).or(all);
    let defaults = BoxMargins::default();
    let margins = BoxMargins {
        left: side("fo:padding-left").unwrap_or(defaults.left),
        top: side("fo:padding-top").unwrap_or(defaults.top),
        right: side("fo:padding-right").unwrap_or(defaults.right),
        bottom: side("fo:padding-bottom").unwrap_or(defaults.bottom),
    };
    let align = match properties
        .get("draw:textarea-vertical-align")
        .map(String::as_str)
    {
        Some("middle") => Some(VerticalAlign::Center),
        Some("bottom") => Some(VerticalAlign::Bottom),
        _ => None,
    };
    let grows = properties
        .get("draw:auto-grow-height")
        .is_some_and(|g| g == "true");
    (margins, align, grows)
}

// writing -------------------------------------------------------------------

/// named definitions a written document needs in `office:styles`, made
/// once each.
#[derive(Clone, Debug, Default)]
pub struct DrawDefinitions {
    by_name: BTreeMap<String, String>,
    /// names the document already defines (in its kept styles), which
    /// new numbered definitions must not reuse and fixed ones need not repeat
    taken: std::collections::BTreeSet<String>,
    gradients: usize,
    images: usize,
}

impl DrawDefinitions {
    /// definitions for a document whose kept styles already name `taken`.
    pub fn avoiding(taken: std::collections::BTreeSet<String>) -> DrawDefinitions {
        DrawDefinitions {
            taken,
            ..Default::default()
        }
    }

    /// the definitions to add to `office:styles`: the ones the document
    /// does not have yet.
    pub fn xml(&self) -> String {
        self.by_name
            .iter()
            .filter(|(name, _)| !self.taken.contains(*name))
            .map(|(_, xml)| xml.as_str())
            .collect()
    }

    fn gradient(&mut self, gradient: &Gradient) -> String {
        let mut name;
        loop {
            self.gradients += 1;
            name = format!("Letters_20_Gradient_20_{}", self.gradients);
            if !self.taken.contains(&name) {
                break;
            }
        }
        let mut stops = gradient.stops.clone();
        let radial = gradient.kind == GradientKind::Radial;
        if radial {
            stops.reverse();
            for stop in &mut stops {
                stop.position = 1.0 - stop.position;
            }
        }
        let first = stops.first().map(|s| s.color.as_str()).unwrap_or("000000");
        let last = stops.last().map(|s| s.color.as_str()).unwrap_or("FFFFFF");
        let angle = (90.0 - gradient.angle).rem_euclid(360.0);
        let mut xml = format!(
            "<draw:gradient draw:name=\"{name}\" draw:style=\"{}\" draw:start-color=\"#{}\" draw:end-color=\"#{}\" \
             draw:start-intensity=\"100%\" draw:end-intensity=\"100%\" draw:angle=\"{}deg\" draw:border=\"0%\"",
            if radial { "radial" } else { "linear" },
            first.to_ascii_lowercase(),
            last.to_ascii_lowercase(),
            (angle * 10.0).round() / 10.0,
        );
        if radial {
            xml.push_str(" draw:cx=\"50%\" draw:cy=\"50%\"");
        }
        if stops.len() > 2 {
            xml.push('>');
            for stop in &stops {
                xml.push_str(&format!(
                    "<loext:gradient-stop svg:offset=\"{}\" loext:color-type=\"rgb\" loext:color-value=\"#{}\"/>",
                    stop.position,
                    stop.color.to_ascii_lowercase()
                ));
            }
            xml.push_str("</draw:gradient>");
        } else {
            xml.push_str("/>");
        }
        self.by_name.insert(name.clone(), xml);
        name
    }

    fn image(&mut self, path: &str) -> String {
        if let Some((name, _)) = self
            .by_name
            .iter()
            .find(|(_, xml)| xml.contains(&format!("xlink:href=\"{}\"", escape_attribute(path))))
        {
            return name.clone();
        }
        let mut name;
        loop {
            self.images += 1;
            name = format!("Letters_20_Picture_20_{}", self.images);
            if !self.taken.contains(&name) {
                break;
            }
        }
        let xml = format!(
            "<draw:fill-image draw:name=\"{name}\" xlink:href=\"{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>",
            escape_attribute(path)
        );
        self.by_name.insert(name.clone(), xml);
        name
    }

    fn marker(&mut self, kind: &str) -> String {
        let (name, path) = match kind {
            "oval" => (
                "Letters_20_Oval",
                "M150 0c83 0 150 67 150 150s-67 150-150 150-150-67-150-150 67-150 150-150z",
            ),
            "diamond" => ("Letters_20_Diamond", "M150 0l150 150-150 150-150-150z"),
            "stealth" => ("Letters_20_Stealth", "M150 0l150 300-150-75-150 75z"),
            "arrow" => (
                "Letters_20_Open",
                "M150 0l150 270-30 30-120-210-120 210-30-30z",
            ),
            _ => ("Letters_20_Triangle", "M150 0l150 300h-300z"),
        };
        self.by_name.entry(name.to_string()).or_insert_with(|| {
            format!(
                "<draw:marker draw:name=\"{name}\" svg:viewBox=\"0 0 300 300\" svg:d=\"{path}\"/>"
            )
        });
        name.to_string()
    }

    /// a dash pattern for a drawingml dash name, in multiples of the line
    /// width as both applications draw them.
    fn dash(&mut self, dash: &str) -> String {
        let (first, second, distance): (f64, Option<f64>, f64) = match dash {
            "dot" => (1.0, None, 3.0),
            "sysDot" => (1.0, None, 1.0),
            "sysDash" => (3.0, None, 1.0),
            "dashDot" => (4.0, Some(1.0), 3.0),
            "sysDashDot" => (3.0, Some(1.0), 1.0),
            "longDash" => (8.0, None, 3.0),
            "longDashDot" | "longDashDotDot" | "sysDashDotDot" => (8.0, Some(1.0), 3.0),
            _ => (4.0, None, 3.0),
        };
        let name = format!("Letters_20_{dash}");
        let mut xml = format!(
            "<draw:stroke-dash draw:name=\"{name}\" draw:style=\"rect\" draw:dots1=\"1\" draw:dots1-length=\"{}%\"",
            first * 100.0
        );
        if let Some(second) = second {
            let dots = if dash.ends_with("DotDot") { 2 } else { 1 };
            xml.push_str(&format!(
                " draw:dots2=\"{dots}\" draw:dots2-length=\"{}%\"",
                second * 100.0
            ));
        }
        xml.push_str(&format!(" draw:distance=\"{}%\"/>", distance * 100.0));
        self.by_name.entry(name.clone()).or_insert(xml);
        name
    }
}

fn colour(color: &str) -> String {
    format!("#{}", color.to_ascii_lowercase())
}

fn opacity(value: f64) -> String {
    format!("{}%", (value * 1000.0).round() / 10.0)
}

/// graphic property attributes for a fill. `picture` gives a media item's
/// path in the package.
pub fn fill_attributes(
    fill: &Fill,
    definitions: &mut DrawDefinitions,
    picture: &dyn Fn(&str) -> Option<String>,
) -> String {
    match fill {
        Fill::None => " draw:fill=\"none\"".into(),
        Fill::Solid {
            color,
            opacity: alpha,
        } => {
            let mut xml = format!(" draw:fill=\"solid\" draw:fill-color=\"{}\"", colour(color));
            if let Some(alpha) = alpha {
                xml.push_str(&format!(" draw:opacity=\"{}\"", opacity(*alpha)));
            }
            xml
        }
        Fill::Gradient(gradient) => {
            let name = definitions.gradient(gradient);
            let mut xml = format!(" draw:fill=\"gradient\" draw:fill-gradient-name=\"{name}\"");
            // opendocument has one opacity per fill: the stops' average
            let alphas: Vec<f64> = gradient.stops.iter().filter_map(|s| s.opacity).collect();
            if !alphas.is_empty() {
                let average = alphas.iter().sum::<f64>() / alphas.len() as f64;
                xml.push_str(&format!(" draw:opacity=\"{}\"", opacity(average)));
            }
            xml
        }
        Fill::Picture { media_id, tile } => match picture(media_id) {
            Some(path) => format!(
                " draw:fill=\"bitmap\" draw:fill-image-name=\"{}\" style:repeat=\"{}\"",
                definitions.image(&path),
                if *tile { "repeat" } else { "stretch" }
            ),
            None => " draw:fill=\"none\"".into(),
        },
    }
}

/// graphic property attributes for a line.
pub fn stroke_attributes(stroke: Option<&Stroke>, definitions: &mut DrawDefinitions) -> String {
    let Some(stroke) = stroke else {
        return " draw:stroke=\"none\"".into();
    };
    let width = crate::units::emu_to_inches(stroke.width_emu());
    let mut xml = if stroke.dash == "solid" {
        String::from(" draw:stroke=\"solid\"")
    } else {
        format!(
            " draw:stroke=\"dash\" draw:stroke-dash=\"{}\"",
            definitions.dash(&stroke.dash)
        )
    };
    xml.push_str(&format!(
        " svg:stroke-width=\"{width}\" svg:stroke-color=\"{}\"",
        colour(&stroke.color)
    ));
    if let Some(alpha) = stroke.opacity {
        xml.push_str(&format!(" svg:stroke-opacity=\"{}\"", opacity(alpha)));
    }
    if let Some(cap) = stroke.cap {
        let value = match cap {
            LineCap::Flat => "butt",
            LineCap::Round => "round",
            LineCap::Square => "square",
        };
        xml.push_str(&format!(" svg:stroke-linecap=\"{value}\""));
    }
    if let Some(join) = stroke.join {
        let value = match join {
            LineJoin::Miter => "miter",
            LineJoin::Round => "round",
            LineJoin::Bevel => "bevel",
        };
        xml.push_str(&format!(" draw:stroke-linejoin=\"{value}\""));
    }
    for (side, end) in [("start", &stroke.head), ("end", &stroke.tail)] {
        if let Some(end) = end {
            let name = definitions.marker(&end.kind);
            let size = arrow_width(stroke.width_emu(), end.width.as_deref());
            xml.push_str(&format!(
                " draw:marker-{side}=\"{name}\" draw:marker-{side}-width=\"{}\" draw:marker-{side}-center=\"{}\"",
                crate::units::emu_to_inches(size),
                end.kind == "oval" || end.kind == "diamond"
            ));
        }
    }
    xml
}

/// an arrowhead's width: drawingml sizes it against the line (small,
/// medium, large); about 2.5, 4 and 6 line widths match word's drawing,
/// with a floor so thin lines still show a head.
fn arrow_width(line_emu: i64, size: Option<&str>) -> i64 {
    let factor = match size {
        Some("sm") => 2.5,
        Some("lg") => 6.0,
        _ => 4.0,
    };
    ((line_emu as f64 * factor).round() as i64).max(54_864)
}

/// graphic property attributes for a shape's text area.
pub fn text_area_attributes(
    margins: &BoxMargins,
    align: Option<VerticalAlign>,
    grows: bool,
) -> String {
    format!(
        " draw:textarea-vertical-align=\"{}\" draw:auto-grow-height=\"{grows}\" fo:padding-top=\"{}\" fo:padding-bottom=\"{}\" fo:padding-left=\"{}\" fo:padding-right=\"{}\"",
        match align {
            Some(VerticalAlign::Center) => "middle",
            Some(VerticalAlign::Bottom) => "bottom",
            _ => "top",
        },
        twips_to_inches(margins.top),
        twips_to_inches(margins.bottom),
        twips_to_inches(margins.left),
        twips_to_inches(margins.right),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    #[test]
    fn gradients_turn_their_angle_between_formats() {
        let gradient = Gradient {
            kind: GradientKind::Linear,
            angle: 90.0,
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: "FFC800".into(),
                    opacity: None,
                },
                GradientStop {
                    position: 1.0,
                    color: "C82828".into(),
                    opacity: None,
                },
            ],
        };
        let mut definitions = DrawDefinitions::default();
        let name = definitions.gradient(&gradient);
        let xml = definitions.xml();
        assert!(xml.contains("draw:angle=\"0deg\""), "{xml}");
        let element = parse(&xml.replace(
            "<draw:gradient",
            "<draw:gradient xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\"",
        ))
        .unwrap()
        .root;
        let mut resources = DrawResources::default();
        resources.add(&element);
        assert_eq!(resources.gradients.get(&name), Some(&gradient));
    }

    #[test]
    fn lines_with_dashes_and_arrowheads() {
        let mut stroke = Stroke::solid("008000", 60);
        stroke.dash = "dash".into();
        stroke.tail = Some(LineEnd {
            kind: "triangle".into(),
            width: None,
            length: None,
        });
        let mut definitions = DrawDefinitions::default();
        let attributes = stroke_attributes(Some(&stroke), &mut definitions);
        let properties: GraphicProperties = parse(&format!(
            "<p xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\"{attributes}/>"
        ))
        .unwrap()
        .root
        .attributes
        .into_iter()
        .collect();
        let mut resources = DrawResources::default();
        let defs = format!("<x xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\">{}</x>", definitions.xml());
        for child in parse(&defs).unwrap().root.elements() {
            resources.add(child);
        }
        let back = read_stroke(&properties, &resources).unwrap();
        assert_eq!(back.dash, "dash");
        assert_eq!(back.width, 60);
        assert_eq!(back.tail.unwrap().kind, "triangle");
    }
}
