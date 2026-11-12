//! drawingml, the shape language word shares with powerpoint and excel:
//! colours (with the theme's colour scheme and its lighter and darker
//! variants), outlines (`a:prstGeom`, `a:custGeom`), fills and lines, read
//! into the model and written back out.
//!
//! everything here is a pure function of xml elements or model values.

use std::collections::HashMap;

use crate::model::{
    Adjust, Fill, Geometry, Gradient, GradientKind, GradientStop, LineCap, LineEnd, LineJoin,
    PathCommand, ShapePath, Stroke,
};
use crate::units::emu_to_twips;
use crate::xml::{escape_attribute, Element};

/// the theme's colours by slot ("accent1", "dk1"...) and its three line
/// widths, which shapes refer to instead of naming their own.
#[derive(Clone, Debug)]
pub struct Palette {
    colors: HashMap<String, String>,
    line_widths: Vec<i64>,
}

impl Default for Palette {
    /// the office theme word has used since 2013, for files without one.
    fn default() -> Self {
        let colors = [
            ("dk1", "000000"),
            ("lt1", "FFFFFF"),
            ("dk2", "44546A"),
            ("lt2", "E7E6E6"),
            ("accent1", "4472C4"),
            ("accent2", "ED7D31"),
            ("accent3", "A5A5A5"),
            ("accent4", "FFC000"),
            ("accent5", "5B9BD5"),
            ("accent6", "70AD47"),
            ("hlink", "0563C1"),
            ("folHlink", "954F72"),
        ]
        .iter()
        .map(|(slot, hex)| (slot.to_string(), hex.to_string()))
        .collect();
        Palette {
            colors,
            line_widths: vec![6350, 12700, 19050],
        }
    }
}

impl Palette {
    /// reads `a:clrScheme` and the line styles of a theme part.
    pub fn from_theme(root: &Element) -> Palette {
        let mut palette = Palette::default();
        root.walk(&mut |element| match element.name.as_str() {
            "a:clrScheme" => {
                for slot in element.elements() {
                    let name = crate::xml::local_name(&slot.name).to_string();
                    let value =
                        slot.elements()
                            .next()
                            .and_then(|colour| match colour.name.as_str() {
                                "a:srgbClr" => colour.attr("val").map(str::to_ascii_uppercase),
                                "a:sysClr" => colour.attr("lastClr").map(str::to_ascii_uppercase),
                                _ => None,
                            });
                    if let Some(value) = value {
                        palette.colors.insert(name, value);
                    }
                }
            }
            "a:lnStyleLst" => {
                let widths: Vec<i64> = element
                    .children_named("a:ln")
                    .filter_map(|line| line.attr("w")?.parse().ok())
                    .collect();
                if !widths.is_empty() {
                    palette.line_widths = widths;
                }
            }
            _ => {}
        });
        palette
    }

    fn slot(&self, name: &str) -> Option<&str> {
        let slot = match name {
            "bg1" => "lt1",
            "tx1" => "dk1",
            "bg2" => "lt2",
            "tx2" => "dk2",
            other => other,
        };
        self.colors.get(slot).map(String::as_str)
    }

    /// the width of theme line style `index` (1 is the first).
    pub fn line_width(&self, index: usize) -> Option<i64> {
        index
            .checked_sub(1)
            .and_then(|at| self.line_widths.get(at))
            .copied()
    }
}

/// a colour as six hex digits and an opacity (1 is opaque).
pub type Colour = (String, Option<f64>);

const COLOUR_ELEMENTS: &[&str] = &[
    "a:srgbClr",
    "a:schemeClr",
    "a:prstClr",
    "a:sysClr",
    "a:scrgbClr",
    "a:hslClr",
];

/// the first colour element among `parent`'s children, resolved.
/// `placeholder` is the colour a style reference gives `phClr`.
pub fn child_colour(
    parent: &Element,
    palette: &Palette,
    placeholder: Option<&str>,
) -> Option<Colour> {
    parent
        .elements()
        .find(|child| COLOUR_ELEMENTS.contains(&child.name.as_str()))
        .and_then(|colour| read_colour(colour, palette, placeholder))
}

/// one colour element with its transforms (lighter, darker, see-through).
pub fn read_colour(
    element: &Element,
    palette: &Palette,
    placeholder: Option<&str>,
) -> Option<Colour> {
    let base = match element.name.as_str() {
        "a:srgbClr" => element.attr("val")?.to_ascii_uppercase(),
        "a:sysClr" => element
            .attr("lastClr")
            .map(str::to_ascii_uppercase)
            .unwrap_or_else(|| "000000".into()),
        "a:schemeClr" => match element.attr("val")? {
            "phClr" => placeholder?.to_string(),
            slot => palette.slot(slot)?.to_string(),
        },
        "a:prstClr" => preset_colour(element.attr("val")?)?.to_string(),
        "a:scrgbClr" => {
            let channel = |name: &str| {
                let linear = element
                    .attr(name)
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(0.0)
                    / 100_000.0;
                (gamma(linear) * 255.0).round().clamp(0.0, 255.0) as u8
            };
            format!(
                "{:02X}{:02X}{:02X}",
                channel("r"),
                channel("g"),
                channel("b")
            )
        }
        _ => return None,
    };
    let mut rgb = hex_rgb(&base)?;
    let mut opacity = None;
    for transform in element.elements() {
        let value = transform
            .attr("val")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
            / 100_000.0;
        match transform.name.as_str() {
            "a:alpha" => opacity = Some(value),
            "a:shade" => rgb = rgb.map(|c| c * value),
            "a:tint" => rgb = rgb.map(|c| c * value + (1.0 - value)),
            "a:lumMod" | "a:lumOff" => {
                let (h, s, mut l) = to_hsl(rgb);
                if transform.name == "a:lumMod" {
                    l *= value;
                } else {
                    l += value;
                }
                rgb = from_hsl(h, s, l.clamp(0.0, 1.0));
            }
            _ => {}
        }
    }
    Some((rgb_hex(rgb), opacity.filter(|o| (*o - 1.0).abs() > 1e-9)))
}

/// srgb from linear light, for `a:scrgbClr`.
fn gamma(linear: f64) -> f64 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

fn preset_colour(name: &str) -> Option<&'static str> {
    Some(match name {
        "black" => "000000",
        "white" => "FFFFFF",
        "red" => "FF0000",
        "green" => "008000",
        "blue" => "0000FF",
        "yellow" => "FFFF00",
        "gray" | "grey" => "808080",
        "darkGray" => "A9A9A9",
        "lightGray" => "D3D3D3",
        "orange" => "FFA500",
        "purple" => "800080",
        "cyan" => "00FFFF",
        "magenta" => "FF00FF",
        _ => return None,
    })
}

pub(crate) fn hex_rgb(hex: &str) -> Option<[f64; 3]> {
    if hex.len() != 6 {
        return None;
    }
    let channel = |at: usize| {
        u8::from_str_radix(hex.get(at..at + 2)?, 16)
            .ok()
            .map(|c| f64::from(c) / 255.0)
    };
    Some([channel(0)?, channel(2)?, channel(4)?])
}

pub(crate) fn rgb_hex(rgb: [f64; 3]) -> String {
    let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "{:02X}{:02X}{:02X}",
        byte(rgb[0]),
        byte(rgb[1]),
        byte(rgb[2])
    )
}

fn to_hsl([r, g, b]: [f64; 3]) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-12 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn from_hsl(h: f64, s: f64, l: f64) -> [f64; 3] {
    if s == 0.0 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0)]
}

/// a shape's style reference (`wps:style`): which theme fill and line it
/// uses and the colour each is drawn in, plus the default text colour.
#[derive(Clone, Debug, Default)]
pub struct StyleRef {
    pub fill: Option<(usize, Option<Colour>)>,
    pub line: Option<(usize, Option<Colour>)>,
    pub font_colour: Option<String>,
}

impl StyleRef {
    pub fn read(style: &Element, palette: &Palette) -> StyleRef {
        let reference = |name: &str| {
            let element = style.child(name)?;
            let index = element.attr("idx")?.parse::<usize>().ok()?;
            Some((index, child_colour(element, palette, None)))
        };
        StyleRef {
            fill: reference("a:fillRef"),
            line: reference("a:lnRef"),
            font_colour: style
                .child("a:fontRef")
                .and_then(|font| child_colour(font, palette, None))
                .map(|(hex, _)| hex),
        }
    }
}

// geometry ------------------------------------------------------------------

/// the outline in `a:prstGeom` or `a:custGeom`, or `None` when it uses
/// formulas the model cannot follow.
pub fn read_geometry(properties: &Element, size: (i64, i64)) -> Option<Geometry> {
    if let Some(preset) = properties.child("a:prstGeom") {
        let name = preset.attr("prst")?.to_string();
        let adjust = preset
            .child("a:avLst")
            .map(|list| {
                list.children_named("a:gd")
                    .filter_map(|guide| {
                        let value = guide
                            .attr("fmla")?
                            .strip_prefix("val ")?
                            .trim()
                            .parse()
                            .ok()?;
                        Some(Adjust {
                            name: guide.attr("name")?.to_string(),
                            value,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Some(Geometry::Preset { name, adjust });
    }
    let custom = properties.child("a:custGeom")?;
    let mut paths = Vec::new();
    for path in custom.child("a:pathLst")?.children_named("a:path") {
        paths.push(read_path(path, size)?);
    }
    (!paths.is_empty()).then_some(Geometry::Path { paths })
}

fn number(element: &Element, name: &str) -> Option<f64> {
    element.attr(name)?.trim().parse().ok()
}

fn point(element: &Element) -> Option<(f64, f64)> {
    Some((number(element, "x")?, number(element, "y")?))
}

fn read_path(path: &Element, size: (i64, i64)) -> Option<ShapePath> {
    let width = number(path, "w").unwrap_or(size.0 as f64);
    let height = number(path, "h").unwrap_or(size.1 as f64);
    let mut commands = Vec::new();
    let mut current = (0.0, 0.0);
    let mut start = (0.0, 0.0);
    for step in path.elements() {
        let points: Vec<(f64, f64)> = step
            .children_named("a:pt")
            .map(point)
            .collect::<Option<_>>()?;
        match step.name.as_str() {
            "a:moveTo" => {
                let (x, y) = *points.first()?;
                commands.push(PathCommand::Move { x, y });
                current = (x, y);
                start = current;
            }
            "a:lnTo" => {
                let (x, y) = *points.first()?;
                commands.push(PathCommand::Line { x, y });
                current = (x, y);
            }
            "a:cubicBezTo" if points.len() == 3 => {
                let [(x1, y1), (x2, y2), (x, y)] = [points[0], points[1], points[2]];
                commands.push(PathCommand::Cubic {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                });
                current = (x, y);
            }
            "a:quadBezTo" if points.len() == 2 => {
                let [(x1, y1), (x, y)] = [points[0], points[1]];
                commands.push(PathCommand::Quadratic { x1, y1, x, y });
                current = (x, y);
            }
            "a:arcTo" => {
                let arc = (
                    number(step, "wR")?,
                    number(step, "hR")?,
                    number(step, "stAng")? / 60000.0,
                    number(step, "swAng")? / 60000.0,
                );
                current = arc_to_cubics(current, arc, &mut commands);
            }
            "a:close" => {
                commands.push(PathCommand::Close);
                current = start;
            }
            _ => return None,
        }
    }
    Some(ShapePath {
        width,
        height,
        fill: path.attr("fill") != Some("none"),
        stroke: !matches!(path.attr("stroke"), Some("0" | "false")),
        commands,
    })
}

/// drawingml's `arcTo` (an elliptical arc that starts at the current
/// point) as cubic curves of at most a quarter turn each, which draw the
/// same arc to well within a hundredth of its radius.
fn arc_to_cubics(
    from: (f64, f64),
    (rx, ry, start, sweep): (f64, f64, f64, f64),
    out: &mut Vec<PathCommand>,
) -> (f64, f64) {
    let radians = |degrees: f64| degrees.to_radians();
    let a0 = radians(start);
    let centre = (from.0 - rx * a0.cos(), from.1 - ry * a0.sin());
    let pieces = (sweep.abs() / 90.0).ceil().max(1.0) as usize;
    let step = radians(sweep) / pieces as f64;
    let mut angle = a0;
    let mut end = from;
    for _ in 0..pieces {
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let (s0, c0) = angle.sin_cos();
        let (s1, c1) = (angle + step).sin_cos();
        let p0 = (centre.0 + rx * c0, centre.1 + ry * s0);
        end = (centre.0 + rx * c1, centre.1 + ry * s1);
        out.push(PathCommand::Cubic {
            x1: p0.0 - k * rx * s0,
            y1: p0.1 + k * ry * c0,
            x2: end.0 + k * rx * s1,
            y2: end.1 - k * ry * c1,
            x: end.0,
            y: end.1,
        });
        angle += step;
    }
    end
}

/// `a:prstGeom` or `a:custGeom` for a geometry.
pub fn geometry_xml(geometry: &Geometry) -> String {
    match geometry {
        Geometry::Preset { name, adjust } => {
            let mut list = String::new();
            for handle in adjust {
                list.push_str(&format!(
                    "<a:gd name=\"{}\" fmla=\"val {}\"/>",
                    escape_attribute(&handle.name),
                    handle.value
                ));
            }
            let list = if list.is_empty() {
                "<a:avLst/>".to_string()
            } else {
                format!("<a:avLst>{list}</a:avLst>")
            };
            format!(
                "<a:prstGeom prst=\"{}\">{list}</a:prstGeom>",
                escape_attribute(name)
            )
        }
        Geometry::Path { paths } => {
            let mut xml = String::from(
                "<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l=\"0\" t=\"0\" r=\"r\" b=\"b\"/><a:pathLst>",
            );
            for path in paths {
                xml.push_str(&path_xml(path));
            }
            xml.push_str("</a:pathLst></a:custGeom>");
            xml
        }
    }
}

/// coordinates are written as whole numbers, as drawingml requires.
fn coordinate(value: f64) -> i64 {
    value.round() as i64
}

fn pt(x: f64, y: f64) -> String {
    format!("<a:pt x=\"{}\" y=\"{}\"/>", coordinate(x), coordinate(y))
}

fn path_xml(path: &ShapePath) -> String {
    let mut attributes = format!(
        " w=\"{}\" h=\"{}\"",
        coordinate(path.width),
        coordinate(path.height)
    );
    if !path.fill {
        attributes.push_str(" fill=\"none\"");
    }
    if !path.stroke {
        attributes.push_str(" stroke=\"0\"");
    }
    let mut xml = format!("<a:path{attributes}>");
    for command in &path.commands {
        xml.push_str(&match command {
            PathCommand::Move { x, y } => format!("<a:moveTo>{}</a:moveTo>", pt(*x, *y)),
            PathCommand::Line { x, y } => format!("<a:lnTo>{}</a:lnTo>", pt(*x, *y)),
            PathCommand::Cubic {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => format!(
                "<a:cubicBezTo>{}{}{}</a:cubicBezTo>",
                pt(*x1, *y1),
                pt(*x2, *y2),
                pt(*x, *y)
            ),
            PathCommand::Quadratic { x1, y1, x, y } => {
                format!("<a:quadBezTo>{}{}</a:quadBezTo>", pt(*x1, *y1), pt(*x, *y))
            }
            PathCommand::Close => "<a:close/>".into(),
        });
    }
    xml.push_str("</a:path>");
    xml
}

// fills ---------------------------------------------------------------------

/// the fill a shape's properties name, or `None` when they name none (the
/// style reference then decides). `picture` turns a relationship id into
/// a media id.
pub fn read_fill(
    properties: &Element,
    palette: &Palette,
    placeholder: Option<&str>,
    picture: &mut dyn FnMut(&str) -> Option<String>,
) -> Option<Fill> {
    for child in properties.elements() {
        match child.name.as_str() {
            "a:noFill" => return Some(Fill::None),
            "a:solidFill" => {
                let (color, opacity) = child_colour(child, palette, placeholder)?;
                return Some(Fill::Solid { color, opacity });
            }
            "a:gradFill" => return read_gradient(child, palette, placeholder).map(Fill::Gradient),
            "a:blipFill" => {
                let id = child.child("a:blip")?.attr("r:embed")?;
                return Some(Fill::Picture {
                    media_id: picture(id)?,
                    tile: child.child("a:tile").is_some(),
                });
            }
            "a:pattFill" => {
                // a pattern is shown as its foreground colour
                let colour = child
                    .child("a:fgClr")
                    .and_then(|fg| child_colour(fg, palette, placeholder))?;
                return Some(Fill::Solid {
                    color: colour.0,
                    opacity: colour.1,
                });
            }
            _ => {}
        }
    }
    None
}

fn read_gradient(
    element: &Element,
    palette: &Palette,
    placeholder: Option<&str>,
) -> Option<Gradient> {
    let mut stops = Vec::new();
    for stop in element.child("a:gsLst")?.children_named("a:gs") {
        let (color, opacity) = child_colour(stop, palette, placeholder)?;
        stops.push(GradientStop {
            position: number(stop, "pos").unwrap_or(0.0) / 100_000.0,
            color,
            opacity,
        });
    }
    stops.sort_by(|a, b| a.position.total_cmp(&b.position));
    let (kind, angle) = match (element.child("a:lin"), element.child("a:path")) {
        (Some(linear), _) => (
            GradientKind::Linear,
            number(linear, "ang").unwrap_or(0.0) / 60000.0,
        ),
        (None, Some(_)) => (GradientKind::Radial, 0.0),
        (None, None) => (GradientKind::Linear, 0.0),
    };
    Some(Gradient { kind, angle, stops })
}

fn colour_xml(color: &str, opacity: Option<f64>) -> String {
    match opacity {
        Some(opacity) if (opacity - 1.0).abs() > 1e-9 => format!(
            "<a:srgbClr val=\"{}\"><a:alpha val=\"{}\"/></a:srgbClr>",
            escape_attribute(color),
            (opacity * 100_000.0).round() as i64
        ),
        _ => format!("<a:srgbClr val=\"{}\"/>", escape_attribute(color)),
    }
}

/// the fill element for a model fill. `picture` gives the relationship id
/// of a media item, or `None` when its bytes are missing.
pub fn fill_xml(fill: &Fill, picture: &mut dyn FnMut(&str) -> Option<String>) -> String {
    match fill {
        Fill::None => "<a:noFill/>".into(),
        Fill::Solid { color, opacity } => {
            format!("<a:solidFill>{}</a:solidFill>", colour_xml(color, *opacity))
        }
        Fill::Gradient(gradient) => {
            let mut xml = String::from("<a:gradFill rotWithShape=\"1\"><a:gsLst>");
            for stop in &gradient.stops {
                xml.push_str(&format!(
                    "<a:gs pos=\"{}\">{}</a:gs>",
                    (stop.position * 100_000.0).round() as i64,
                    colour_xml(&stop.color, stop.opacity)
                ));
            }
            xml.push_str("</a:gsLst>");
            match gradient.kind {
                GradientKind::Linear => xml.push_str(&format!(
                    "<a:lin ang=\"{}\" scaled=\"1\"/>",
                    (gradient.angle * 60000.0).round() as i64
                )),
                GradientKind::Radial => xml.push_str(
                    "<a:path path=\"circle\"><a:fillToRect l=\"50000\" t=\"50000\" r=\"50000\" b=\"50000\"/></a:path>",
                ),
            }
            xml.push_str("</a:gradFill>");
            xml
        }
        Fill::Picture { media_id, tile } => match picture(media_id) {
            Some(rel) => format!(
                "<a:blipFill><a:blip r:embed=\"{rel}\"/>{}</a:blipFill>",
                if *tile {
                    "<a:tile tx=\"0\" ty=\"0\" sx=\"100000\" sy=\"100000\" flip=\"none\" algn=\"tl\"/>"
                } else {
                    "<a:stretch><a:fillRect/></a:stretch>"
                }
            ),
            None => "<a:noFill/>".into(),
        },
    }
}

// lines ---------------------------------------------------------------------

/// what `a:ln` says, with the style reference filling in what it leaves
/// out. `Ok(None)` is "no line"; `Err(())` means the properties name no
/// line at all, so the style reference decides.
pub fn read_stroke(
    properties: &Element,
    palette: &Palette,
    style: &StyleRef,
) -> Result<Option<Stroke>, ()> {
    let Some(line) = properties.child("a:ln") else {
        return Err(());
    };
    if line.child("a:noFill").is_some() {
        return Ok(None);
    }
    let reference = style.line.as_ref();
    let placeholder = reference.and_then(|(_, colour)| colour.as_ref().map(|c| c.0.as_str()));
    let colour = line
        .child("a:solidFill")
        .and_then(|fill| child_colour(fill, palette, placeholder))
        .or_else(|| reference.and_then(|(_, colour)| colour.clone()));
    let Some((color, opacity)) = colour else {
        // a line with neither its own colour nor a style is not drawn
        return Ok(None);
    };
    let width = line
        .attr("w")
        .and_then(|w| w.parse::<i64>().ok())
        .or_else(|| reference.and_then(|(index, _)| palette.line_width(*index)))
        .unwrap_or(9525);
    Ok(Some(stroke_from(line, color, opacity, width)))
}

/// the line a style reference alone gives.
pub fn style_stroke(palette: &Palette, style: &StyleRef) -> Option<Stroke> {
    let (index, colour) = style.line.as_ref()?;
    if *index == 0 {
        return None;
    }
    let (color, opacity) = colour.clone()?;
    let width = palette.line_width(*index).unwrap_or(9525);
    let mut stroke = Stroke::solid(&color, emu_to_twips(width));
    stroke.width_emu = Some(width);
    stroke.opacity = opacity;
    Some(stroke)
}

/// the fill a style reference alone gives.
pub fn style_fill(style: &StyleRef) -> Fill {
    match &style.fill {
        Some((index, Some((color, opacity)))) if *index > 0 => Fill::Solid {
            color: color.clone(),
            opacity: *opacity,
        },
        _ => Fill::None,
    }
}

fn stroke_from(line: &Element, color: String, opacity: Option<f64>, width: i64) -> Stroke {
    let end = |name: &str| {
        let element = line.child(name)?;
        let kind = element.attr("type")?;
        (kind != "none").then(|| LineEnd {
            kind: kind.to_string(),
            width: element.attr("w").map(str::to_string),
            length: element.attr("len").map(str::to_string),
        })
    };
    Stroke {
        color,
        width: emu_to_twips(width),
        width_emu: Some(width),
        opacity,
        dash: line
            .child("a:prstDash")
            .and_then(|dash| dash.attr("val"))
            .unwrap_or("solid")
            .to_string(),
        cap: match line.attr("cap") {
            Some("rnd") => Some(LineCap::Round),
            Some("sq") => Some(LineCap::Square),
            Some("flat") => Some(LineCap::Flat),
            _ => None,
        },
        join: if line.child("a:round").is_some() {
            Some(LineJoin::Round)
        } else if line.child("a:bevel").is_some() {
            Some(LineJoin::Bevel)
        } else if line.child("a:miter").is_some() {
            Some(LineJoin::Miter)
        } else {
            None
        },
        head: end("a:headEnd"),
        tail: end("a:tailEnd"),
    }
}

/// `a:ln` for a stroke, or a line with no fill.
pub fn stroke_xml(stroke: Option<&Stroke>) -> String {
    let Some(stroke) = stroke else {
        return "<a:ln><a:noFill/></a:ln>".into();
    };
    let cap = match stroke.cap {
        Some(LineCap::Round) => " cap=\"rnd\"",
        Some(LineCap::Square) => " cap=\"sq\"",
        Some(LineCap::Flat) => " cap=\"flat\"",
        None => "",
    };
    let mut xml = format!(
        "<a:ln w=\"{}\"{cap}><a:solidFill>{}</a:solidFill>",
        stroke.width_emu(),
        colour_xml(&stroke.color, stroke.opacity)
    );
    if stroke.dash != "solid" {
        xml.push_str(&format!(
            "<a:prstDash val=\"{}\"/>",
            escape_attribute(&stroke.dash)
        ));
    }
    xml.push_str(match stroke.join {
        Some(LineJoin::Round) => "<a:round/>",
        Some(LineJoin::Bevel) => "<a:bevel/>",
        Some(LineJoin::Miter) => "<a:miter lim=\"800000\"/>",
        None => "",
    });
    for (name, end) in [("a:headEnd", &stroke.head), ("a:tailEnd", &stroke.tail)] {
        if let Some(end) = end {
            let mut attributes = format!(" type=\"{}\"", escape_attribute(&end.kind));
            if let Some(width) = &end.width {
                attributes.push_str(&format!(" w=\"{}\"", escape_attribute(width)));
            }
            if let Some(length) = &end.length {
                attributes.push_str(&format!(" len=\"{}\"", escape_attribute(length)));
            }
            xml.push_str(&format!("<{name}{attributes}/>"));
        }
    }
    xml.push_str("</a:ln>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    fn element(xml: &str) -> Element {
        parse(xml).unwrap().root
    }

    const A: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"";

    #[test]
    fn theme_colours_and_their_variants() {
        let palette = Palette::default();
        let colour = element(&format!(
            "<a:schemeClr {A} val=\"accent1\"><a:shade val=\"50000\"/></a:schemeClr>"
        ));
        assert_eq!(read_colour(&colour, &palette, None).unwrap().0, "223962");
        let colour = element(&format!(
            "<a:schemeClr {A} val=\"accent1\"><a:lumMod val=\"60000\"/><a:lumOff val=\"40000\"/></a:schemeClr>"
        ));
        assert_eq!(read_colour(&colour, &palette, None).unwrap().0, "8FAADC");
        let colour = element(&format!(
            "<a:srgbClr {A} val=\"783ca0\"><a:alpha val=\"50000\"/></a:srgbClr>"
        ));
        assert_eq!(
            read_colour(&colour, &palette, None).unwrap(),
            ("783CA0".to_string(), Some(0.5))
        );
    }

    #[test]
    fn custom_paths_and_arcs() {
        let properties = element(&format!(
            "<wps:spPr {A} xmlns:wps=\"x\"><a:custGeom><a:pathLst><a:path w=\"100\" h=\"100\">\
             <a:moveTo><a:pt x=\"0\" y=\"50\"/></a:moveTo>\
             <a:arcTo wR=\"50\" hR=\"50\" stAng=\"10800000\" swAng=\"10800000\"/><a:close/>\
             </a:path></a:pathLst></a:custGeom></wps:spPr>"
        ));
        let Some(Geometry::Path { paths }) = read_geometry(&properties, (100, 100)) else {
            panic!("no path")
        };
        let commands = &paths[0].commands;
        assert_eq!(commands.len(), 4, "{commands:?}");
        let PathCommand::Cubic { x, y, .. } = commands[2] else {
            panic!()
        };
        assert!(
            (x - 100.0).abs() < 1e-9 && (y - 50.0).abs() < 1e-9,
            "{x} {y}"
        );
        let written = geometry_xml(&Geometry::Path {
            paths: paths.clone(),
        });
        assert!(written.contains("<a:cubicBezTo>") && written.contains("<a:close/>"));
    }

    #[test]
    fn fills_and_lines_round_trip() {
        let fill = Fill::Gradient(Gradient {
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
                    opacity: Some(0.5),
                },
            ],
        });
        let mut no_pictures = |_: &str| None;
        let xml = format!(
            "<wps:spPr {A} xmlns:wps=\"x\">{}{}</wps:spPr>",
            fill_xml(&fill, &mut no_pictures),
            stroke_xml(Some(&Stroke {
                dash: "dash".into(),
                head: Some(LineEnd {
                    kind: "oval".into(),
                    width: None,
                    length: None
                }),
                cap: Some(LineCap::Round),
                ..Stroke::solid("008000", 60)
            }))
        );
        let properties = element(&xml);
        let palette = Palette::default();
        assert_eq!(
            read_fill(&properties, &palette, None, &mut no_pictures),
            Some(fill)
        );
        let stroke = read_stroke(&properties, &palette, &StyleRef::default())
            .unwrap()
            .unwrap();
        assert_eq!(stroke.width, 60);
        assert_eq!(stroke.dash, "dash");
        assert_eq!(stroke.head.unwrap().kind, "oval");
        assert_eq!(stroke.cap, Some(LineCap::Round));
    }
}
