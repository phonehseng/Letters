//! vml, the drawing language of word 97 to 2007, which older files (and
//! files made by some other programs) use alone, without drawingml.
//!
//! read here: rectangles, rounded rectangles, ovals, lines, polylines,
//! shapes with a path or a common shape type, text boxes and groups, with
//! their position, size, rotation, flips, fill, line and arrowheads.
//! anything else keeps the whole `w:pict` preserved.

use std::collections::HashMap;

use crate::model::{
    Adjust, Block, Border, BoxMargins, DrawingPreserved, Fill, FloatPosition, Geometry, Gradient,
    GradientKind, GradientStop, Group, GroupChild, LineEnd, PathCommand, Shape, ShapePath,
    ShapeText, SourceFormat, Stroke, TextBox, TextChain, Wrap,
};
use crate::units::{parse_length, twips_to_emu};
use crate::xml::Element;

use super::{DocxReader, Story};

/// a vml `style` attribute as name to value pairs.
fn style_map(element: &Element) -> HashMap<String, String> {
    element
        .attr("style")
        .unwrap_or_default()
        .split(';')
        .filter_map(|pair| {
            let (name, value) = pair.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect()
}

/// a vml length in twips. bare numbers inside a group are in the group's
/// coordinate units, which the caller scales.
fn length(value: &str) -> Option<f64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(number) = value.parse::<f64>() {
        return Some(number);
    }
    parse_length(value).map(f64::from)
}

/// a vml colour ("#3366cc", "#36c", "red", "black [3213]") as hex.
fn colour(value: &str) -> Option<String> {
    let value = value.split_whitespace().next()?.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return match hex.len() {
            6 => Some(hex.to_ascii_uppercase()),
            3 => Some(
                hex.chars()
                    .flat_map(|c| [c, c])
                    .collect::<String>()
                    .to_ascii_uppercase(),
            ),
            _ => None,
        };
    }
    Some(
        match value.to_ascii_lowercase().as_str() {
            "black" => "000000",
            "white" => "FFFFFF",
            "red" => "FF0000",
            "green" => "008000",
            "blue" => "0000FF",
            "yellow" => "FFFF00",
            "gray" | "grey" => "808080",
            "silver" => "C0C0C0",
            "navy" => "000080",
            "maroon" => "800000",
            "purple" => "800080",
            "teal" => "008080",
            "lime" => "00FF00",
            "aqua" => "00FFFF",
            "fuchsia" => "FF00FF",
            "olive" => "808000",
            _ => return None,
        }
        .to_string(),
    )
}

/// "f", "false" and "0" are off in vml; anything else is on.
fn flag(value: Option<&str>, default: bool) -> bool {
    match value {
        Some(v) => !matches!(v.trim(), "f" | "false" | "0"),
        None => default,
    }
}

/// a vml fraction: "0.5", "50%" or the fixed-point "32768f".
fn fraction(value: &str) -> Option<f64> {
    let value = value.trim();
    if let Some(fixed) = value.strip_suffix('f') {
        return fixed.parse::<f64>().ok().map(|v| v / 65536.0);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return percent.parse::<f64>().ok().map(|v| v / 100.0);
    }
    value.parse().ok()
}

/// the drawingml preset for a vml shape type number (`o:spt`).
fn preset_for_type(number: u32) -> Option<&'static str> {
    Some(match number {
        1 => "rect",
        2 => "roundRect",
        3 => "ellipse",
        4 => "diamond",
        5 => "triangle",
        6 => "rtTriangle",
        9 => "hexagon",
        10 => "octagon",
        12 => "star5",
        13 => "rightArrow",
        20 => "line",
        32 => "straightConnector1",
        56 => "pentagon",
        61 => "wedgeRectCallout",
        62 => "wedgeRoundRectCallout",
        63 => "wedgeEllipseCallout",
        66 => "leftArrow",
        67 => "downArrow",
        68 => "upArrow",
        69 => "leftRightArrow",
        74 => "heart",
        _ => return None,
    })
}

/// where a vml shape sits: its box in twips (or group units) and anchors.
struct Placed {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    horizontal_relative: String,
    vertical_relative: String,
    rotation: Option<f64>,
    flip_horizontal: bool,
    flip_vertical: bool,
    z: Option<i64>,
}

fn placed(style: &HashMap<String, String>, in_group: bool) -> Placed {
    let get = |name: &str| style.get(name).and_then(|v| length(v));
    let (x, y) = if in_group {
        (get("left").unwrap_or(0.0), get("top").unwrap_or(0.0))
    } else {
        (
            get("margin-left").or_else(|| get("left")).unwrap_or(0.0),
            get("margin-top").or_else(|| get("top")).unwrap_or(0.0),
        )
    };
    let relative = |name: &str, vertical: bool| {
        match style.get(name).map(String::as_str) {
            Some("page") => "page",
            Some("margin") => "margin",
            Some("char") => "character",
            Some("line") => "line",
            Some("left-margin-area") => "leftMargin",
            Some("right-margin-area") => "rightMargin",
            Some("top-margin-area") => "topMargin",
            Some("bottom-margin-area") => "bottomMargin",
            _ if vertical => "paragraph",
            _ => "column",
        }
        .to_string()
    };
    let flip = style.get("flip").cloned().unwrap_or_default();
    Placed {
        x,
        y,
        width: get("width").unwrap_or(0.0),
        height: get("height").unwrap_or(0.0),
        horizontal_relative: relative("mso-position-horizontal-relative", false),
        vertical_relative: relative("mso-position-vertical-relative", true),
        rotation: style
            .get("rotation")
            .and_then(|r| r.trim_end_matches("fd").parse::<f64>().ok())
            .filter(|r| *r != 0.0),
        flip_horizontal: flip.contains('x'),
        flip_vertical: flip.contains('y'),
        z: style.get("z-index").and_then(|z| z.parse().ok()),
    }
}

fn wrap_of(element: &Element, z: Option<i64>) -> Wrap {
    let wrap = element
        .children_named("w10:wrap")
        .next()
        .and_then(|w| w.attr("type"));
    match wrap {
        Some("square") => Wrap::Square,
        Some("tight") => Wrap::Tight,
        Some("through") => Wrap::Through,
        Some("topAndBottom") => Wrap::TopAndBottom,
        _ if z.is_some_and(|z| z < 0) => Wrap::BehindText,
        _ => Wrap::InFrontOfText,
    }
}

fn fill_of(element: &Element) -> Fill {
    if !flag(element.attr("filled"), true) {
        return Fill::None;
    }
    let base = element
        .attr("fillcolor")
        .and_then(colour)
        .unwrap_or_else(|| "FFFFFF".into());
    let fill = element.children_named("v:fill").next();
    let opacity = fill
        .and_then(|f| f.attr("opacity"))
        .and_then(fraction)
        .filter(|o| (*o - 1.0).abs() > 1e-9);
    match fill.and_then(|f| f.attr("type")) {
        Some(kind @ ("gradient" | "gradientRadial")) => {
            let second = fill
                .and_then(|f| f.attr("color2"))
                .and_then(colour)
                .unwrap_or_else(|| "FFFFFF".into());
            let angle = fill
                .and_then(|f| f.attr("angle"))
                .and_then(|a| a.parse::<f64>().ok())
                .unwrap_or(0.0);
            Fill::Gradient(Gradient {
                kind: if kind == "gradient" {
                    GradientKind::Linear
                } else {
                    GradientKind::Radial
                },
                // vml measures counterclockwise from top to bottom
                angle: (90.0 - angle).rem_euclid(360.0),
                stops: vec![
                    GradientStop {
                        position: 0.0,
                        color: base,
                        opacity,
                    },
                    GradientStop {
                        position: 1.0,
                        color: second,
                        opacity,
                    },
                ],
            })
        }
        _ => Fill::Solid {
            color: base,
            opacity,
        },
    }
}

fn stroke_of(element: &Element) -> Option<Stroke> {
    if !flag(element.attr("stroked"), true) {
        return None;
    }
    let color = element
        .attr("strokecolor")
        .and_then(colour)
        .unwrap_or_else(|| "000000".into());
    let width = element
        .attr("strokeweight")
        .and_then(length)
        .unwrap_or(15.0)
        .round() as i32;
    let mut stroke = Stroke::solid(&color, width);
    stroke.width_emu = Some(twips_to_emu(width));
    if let Some(details) = element.children_named("v:stroke").next() {
        stroke.dash = match details.attr("dashstyle") {
            Some("dash") => "dash",
            Some("shortdash") => "sysDash",
            Some("dot") | Some("shortdot") => "sysDot",
            Some("dashdot") | Some("shortdashdot") => "dashDot",
            Some("longdash") => "longDash",
            Some("longdashdot") => "longDashDot",
            Some("longdashdotdot") => "longDashDotDot",
            _ => "solid",
        }
        .into();
        let end = |name: &str| {
            let kind = match details.attr(name)? {
                "block" => "triangle",
                "classic" => "stealth",
                "diamond" => "diamond",
                "oval" => "oval",
                "open" => "arrow",
                _ => return None,
            };
            Some(LineEnd {
                kind: kind.into(),
                width: None,
                length: None,
            })
        };
        stroke.head = end("startarrow");
        stroke.tail = end("endarrow");
    }
    Some(stroke)
}

/// a vml `path` ("m0,0l100,0,100,100xe") in the shape's coordinate space.
fn path_commands(path: &str) -> Option<Vec<PathCommand>> {
    let mut commands = Vec::new();
    let mut numbers: Vec<f64> = Vec::new();
    let mut verb = ' ';
    let flush = |verb: char, numbers: &mut Vec<f64>, out: &mut Vec<PathCommand>| -> Option<()> {
        match verb {
            'm' => {
                let [x, y] = numbers.get(..2)?.try_into().ok()?;
                out.push(PathCommand::Move { x, y });
            }
            'l' => {
                for pair in numbers.chunks(2) {
                    if let [x, y] = pair {
                        out.push(PathCommand::Line { x: *x, y: *y });
                    }
                }
            }
            'c' => {
                for six in numbers.chunks(6) {
                    if let [x1, y1, x2, y2, x, y] = six {
                        out.push(PathCommand::Cubic {
                            x1: *x1,
                            y1: *y1,
                            x2: *x2,
                            y2: *y2,
                            x: *x,
                            y: *y,
                        });
                    }
                }
            }
            ' ' | 'e' | 'n' | 's' | 'f' | 'x' => {}
            _ => return None,
        }
        numbers.clear();
        Some(())
    };
    let mut token = String::new();
    // an empty number in vml is a zero ("m,l100,"): a comma, or a verb
    // straight after a comma, ends a number even when nothing was written
    let mut after_comma = false;
    for c in path.chars().chain(std::iter::once(' ')) {
        let starts_negative = c == '-' && !token.is_empty();
        if c.is_ascii_digit() || c == '.' || (c == '-' && token.is_empty()) {
            token.push(c);
            after_comma = false;
            continue;
        }
        if !token.is_empty() {
            numbers.push(token.parse().ok()?);
            token.clear();
        } else if (c == ',' || c.is_ascii_alphabetic())
            && (after_comma || c == ',' && verb_takes_numbers(verb) && numbers.is_empty())
        {
            numbers.push(0.0);
        }
        after_comma = c == ',';
        if starts_negative {
            token.push('-');
            continue;
        }
        if c.is_ascii_alphabetic() {
            flush(verb, &mut numbers, &mut commands)?;
            verb = c;
            if c == 'x' {
                commands.push(PathCommand::Close);
            }
        }
    }
    flush(verb, &mut numbers, &mut commands)?;
    (!commands.is_empty()).then_some(commands)
}

fn verb_takes_numbers(verb: char) -> bool {
    matches!(verb, 'm' | 'l' | 'c')
}

fn coordinate_size(element: &Element) -> (f64, f64) {
    element
        .attr("coordsize")
        .and_then(|size| {
            let (w, h) = size.split_once(',')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or((21600.0, 21600.0))
}

impl DocxReader<'_> {
    /// the first drawing in a `w:pict`.
    pub(super) fn read_vml(&mut self, pict: &Element, story: &mut Story) -> Option<Block> {
        let element = pict.elements().find(|child| child.name != "v:shapetype")?;
        let shapetypes: HashMap<String, u32> = pict
            .children_named("v:shapetype")
            .filter_map(|t| Some((t.attr("id")?.to_string(), t.attr("o:spt")?.parse().ok()?)))
            .collect();
        let child = self.read_vml_element(element, &shapetypes, story, None)?;
        Some(match child {
            GroupChild::Shape(shape) => Block::Shape(shape),
            GroupChild::TextBox(text_box) => Block::TextBox(text_box),
            GroupChild::Group(group) => Block::Group(group),
            GroupChild::Picture(_) => return None,
        })
    }

    /// one vml element. `group` is the scale from the parent group's
    /// coordinate units to emu, for members of a group.
    fn read_vml_element(
        &mut self,
        element: &Element,
        shapetypes: &HashMap<String, u32>,
        story: &mut Story,
        group: Option<(f64, f64)>,
    ) -> Option<GroupChild> {
        let style = style_map(element);
        let at = placed(&style, group.is_some());
        // outside a group lengths are twips; inside, group units scaled
        let (scale_x, scale_y) = group.unwrap_or((635.0, 635.0));
        let emu = |value: f64, scale: f64| (value * scale).round() as i64;
        let position = |value: f64, relative: &str, scale: f64| {
            let exact = emu(value, scale);
            FloatPosition {
                relative_to: if group.is_some() {
                    "group".into()
                } else {
                    relative.to_string()
                },
                offset: crate::units::emu_to_twips(exact),
                align: None,
                offset_emu: Some(exact),
            }
        };
        let horizontal = position(at.x, &at.horizontal_relative, scale_x);
        let vertical = position(at.y, &at.vertical_relative, scale_y);
        let (cx, cy) = (emu(at.width, scale_x), emu(at.height, scale_y));
        let wrap = wrap_of(element, at.z);
        let z_order = at.z.map(i64::abs);
        let name = element.attr("id").map(str::to_string);
        if element.name == "v:group" {
            let (units_x, units_y) = coordinate_size(element);
            let child_scale = (
                if units_x > 0.0 {
                    cx as f64 / units_x
                } else {
                    1.0
                },
                if units_y > 0.0 {
                    cy as f64 / units_y
                } else {
                    1.0
                },
            );
            let mut children = Vec::new();
            for member in element.elements() {
                if member.name == "v:shapetype"
                    || member.name.starts_with("w10:")
                    || member.name.starts_with("o:")
                {
                    continue;
                }
                children.push(self.read_vml_element(
                    member,
                    shapetypes,
                    story,
                    Some(child_scale),
                )?);
            }
            return Some(GroupChild::Group(Group {
                name,
                description: element.attr("alt").map(str::to_string),
                title: None,
                horizontal,
                vertical,
                width: crate::units::emu_to_twips(cx),
                height: crate::units::emu_to_twips(cy),
                width_emu: Some(cx),
                height_emu: Some(cy),
                wrap,
                z_order,
                rotation: at.rotation,
                flip_horizontal: at.flip_horizontal,
                flip_vertical: at.flip_vertical,
                child_offset_x: 0,
                child_offset_y: 0,
                child_width: cx,
                child_height: cy,
                children,
                preserved: DrawingPreserved::default(),
                anchor: None,
                original: None,
            }));
        }
        let shape_type = element
            .attr("type")
            .map(|t| t.trim_start_matches('#'))
            .and_then(|t| {
                shapetypes
                    .get(t)
                    .copied()
                    .or_else(|| t.strip_prefix("_x0000_t")?.parse().ok())
            })
            .or_else(|| element.attr("o:spt").and_then(|n| n.parse().ok()));
        let text_element = element.children_named("v:textbox").next();
        let blocks = match text_element.and_then(|t| t.child("w:txbxContent")) {
            Some(content) => {
                let mut ignored_sections = Vec::new();
                Some(self.read_blocks(content, story, &mut ignored_sections))
            }
            None => None,
        };
        let margins = text_element
            .and_then(|t| t.attr("inset"))
            .map(insets)
            .unwrap_or_default();
        let next_box = text_element
            .map(style_map)
            .and_then(|s| s.get("mso-next-textbox").cloned());
        let fill = fill_of(element);
        let stroke = stroke_of(element);
        let (geometry, (x, y, cx, cy), flips) = match element.name.as_str() {
            "v:rect" => (
                Geometry::preset("rect"),
                (horizontal, vertical, cx, cy),
                (false, false),
            ),
            "v:oval" => (
                Geometry::preset("ellipse"),
                (horizontal, vertical, cx, cy),
                (false, false),
            ),
            "v:roundrect" => {
                let arc = element.attr("arcsize").and_then(fraction).unwrap_or(0.2);
                (
                    Geometry::Preset {
                        name: "roundRect".into(),
                        adjust: vec![Adjust {
                            name: "adj".into(),
                            value: (arc * 50000.0).round() as i64,
                        }],
                    },
                    (horizontal, vertical, cx, cy),
                    (false, false),
                )
            }
            "v:line" => {
                let point = |name: &str| -> Option<(f64, f64)> {
                    let (a, b) = element.attr(name)?.split_once(',')?;
                    Some((length(a).unwrap_or(0.0), length(b).unwrap_or(0.0)))
                };
                let (from, to) = (point("from")?, point("to")?);
                let left = from.0.min(to.0);
                let top = from.1.min(to.1);
                (
                    Geometry::preset("line"),
                    (
                        position(left, &at.horizontal_relative, scale_x),
                        position(top, &at.vertical_relative, scale_y),
                        emu((to.0 - from.0).abs(), scale_x),
                        emu((to.1 - from.1).abs(), scale_y),
                    ),
                    (to.0 < from.0, to.1 < from.1),
                )
            }
            "v:polyline" => {
                let numbers: Vec<f64> = element
                    .attr("points")?
                    .split([',', ' '])
                    .filter(|s| !s.is_empty())
                    .map(|s| length(s).unwrap_or(0.0))
                    .collect();
                let points: Vec<(f64, f64)> = numbers
                    .chunks(2)
                    .filter_map(|p| Some((*p.first()?, *p.get(1)?)))
                    .collect();
                let (min_x, min_y) = points.iter().fold((f64::MAX, f64::MAX), |(a, b), (x, y)| {
                    (a.min(*x), b.min(*y))
                });
                let (max_x, max_y) = points.iter().fold((f64::MIN, f64::MIN), |(a, b), (x, y)| {
                    (a.max(*x), b.max(*y))
                });
                let mut commands = Vec::new();
                for (index, (px, py)) in points.iter().enumerate() {
                    let (x, y) = (px - min_x, py - min_y);
                    commands.push(if index == 0 {
                        PathCommand::Move { x, y }
                    } else {
                        PathCommand::Line { x, y }
                    });
                }
                (
                    Geometry::Path {
                        paths: vec![ShapePath {
                            width: (max_x - min_x).max(1.0),
                            height: (max_y - min_y).max(1.0),
                            fill: flag(element.attr("filled"), true),
                            stroke: true,
                            commands,
                        }],
                    },
                    (
                        position(min_x, &at.horizontal_relative, scale_x),
                        position(min_y, &at.vertical_relative, scale_y),
                        emu(max_x - min_x, scale_x),
                        emu(max_y - min_y, scale_y),
                    ),
                    (false, false),
                )
            }
            "v:shape" => {
                let geometry = match (element.attr("path"), shape_type) {
                    (_, Some(202)) | (None, Some(1)) => Geometry::preset("rect"),
                    (Some(path), _) => {
                        let (width, height) = coordinate_size(element);
                        Geometry::Path {
                            paths: vec![ShapePath {
                                width,
                                height,
                                fill: true,
                                stroke: true,
                                commands: path_commands(path)?,
                            }],
                        }
                    }
                    (None, Some(number)) => Geometry::preset(preset_for_type(number)?),
                    (None, None) => return None,
                };
                (geometry, (horizontal, vertical, cx, cy), (false, false))
            }
            _ => return None,
        };
        let (flip_horizontal, flip_vertical) =
            (at.flip_horizontal ^ flips.0, at.flip_vertical ^ flips.1);
        let is_text_box = shape_type == Some(202)
            || (blocks.is_some()
                && matches!(&geometry, Geometry::Preset { name, adjust } if name == "rect" && adjust.is_empty()));
        if is_text_box && !flip_horizontal && !flip_vertical && !matches!(fill, Fill::Gradient(_)) {
            let mut text_box = TextBox::new(
                0,
                0,
                crate::units::emu_to_twips(cx),
                crate::units::emu_to_twips(cy),
                blocks.unwrap_or_default(),
            );
            text_box.name = name;
            text_box.horizontal = x;
            text_box.vertical = y;
            text_box.width_emu = Some(cx);
            text_box.height_emu = Some(cy);
            text_box.wrap = wrap;
            text_box.z_order = z_order;
            text_box.rotation = at.rotation;
            text_box.margins = margins;
            text_box.auto_height = style.get("mso-fit-shape-to-text").is_some_and(|v| v == "t");
            text_box.border = stroke.map(|stroke| Border {
                style: "single".into(),
                size: ((stroke.width * 8) / 20).max(1) as u32,
                color: Some(stroke.color),
            });
            text_box.fill = match fill {
                Fill::Solid { color, .. } => Some(color),
                _ => None,
            };
            if let Some(next) = next_box {
                // the chain's id is the first box's name; the reader of
                // the whole body numbers the rest (see `link_vml_chains`)
                text_box.chain = Some(TextChain {
                    id: format!("vml:{}", next.trim_start_matches('#')),
                    sequence: 0,
                });
            }
            return Some(GroupChild::TextBox(text_box));
        }
        Some(GroupChild::Shape(Shape {
            name,
            description: element.attr("alt").map(str::to_string),
            title: None,
            horizontal: x,
            vertical: y,
            width: crate::units::emu_to_twips(cx),
            height: crate::units::emu_to_twips(cy),
            width_emu: Some(cx),
            height_emu: Some(cy),
            wrap,
            z_order,
            rotation: at.rotation,
            flip_horizontal,
            flip_vertical,
            geometry,
            fill,
            stroke,
            text: blocks.map(|blocks| ShapeText {
                margins,
                ..ShapeText::new(blocks)
            }),
            connector: None,
            preserved: DrawingPreserved {
                format: Some(SourceFormat::Docx),
                ..Default::default()
            },
            anchor: None,
            original: None,
        }))
    }
}

/// joins vml text boxes into chains. vml names the next box of a chain
/// (`mso-next-textbox`) on each box; the model numbers every box of the
/// chain from the first one, whose name becomes the chain's id.
pub(super) fn link_vml_chains(blocks: &mut [Block]) {
    let mut next_of: HashMap<String, String> = HashMap::new();
    for block in blocks.iter() {
        if let Block::TextBox(TextBox {
            name: Some(name),
            chain: Some(chain),
            ..
        }) = block
        {
            if let Some(next) = chain.id.strip_prefix("vml:") {
                next_of.insert(name.clone(), next.to_string());
            }
        }
    }
    if next_of.is_empty() {
        return;
    }
    let targets: std::collections::HashSet<&String> = next_of.values().collect();
    let mut placed: HashMap<String, TextChain> = HashMap::new();
    for head in next_of.keys().filter(|name| !targets.contains(name)) {
        let mut name = head.clone();
        let mut sequence = 0;
        while sequence < 1000 {
            placed.insert(
                name.clone(),
                TextChain {
                    id: head.clone(),
                    sequence,
                },
            );
            match next_of.get(&name) {
                Some(next) => name = next.clone(),
                None => break,
            }
            sequence += 1;
        }
    }
    for block in blocks.iter_mut() {
        if let Block::TextBox(text_box) = block {
            if let Some(chain) = text_box.name.as_ref().and_then(|name| placed.get(name)) {
                text_box.chain = Some(chain.clone());
                // the box is as it was read, so its original still applies
                if let Some(original) = text_box.original.take() {
                    crate::model::remember_original(text_box, original.format, original.xml);
                }
            }
        }
    }
}

/// a vml text box inset ("7.2pt,3.6pt,7.2pt,3.6pt"); missing sides keep
/// the defaults.
fn insets(value: &str) -> BoxMargins {
    let mut margins = BoxMargins::default();
    let sides: Vec<Option<i32>> = value
        .split(',')
        .map(|side| length(side).map(|v| v.round() as i32))
        .collect();
    let slots = [
        &mut margins.left,
        &mut margins.top,
        &mut margins.right,
        &mut margins.bottom,
    ];
    for (slot, side) in slots.into_iter().zip(sides) {
        if let Some(side) = side {
            *slot = side;
        }
    }
    margins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vml_paths_and_colours() {
        let commands = path_commands("m,l21600,,21600,21600xe").unwrap();
        assert_eq!(commands.len(), 4, "{commands:?}");
        assert_eq!(commands[0], PathCommand::Move { x: 0.0, y: 0.0 });
        assert_eq!(commands[3], PathCommand::Close);
        assert_eq!(colour("#36c").as_deref(), Some("3366CC"));
        assert_eq!(colour("red [3]").as_deref(), Some("FF0000"));
        assert_eq!(fraction("32768f"), Some(0.5));
        assert_eq!(insets("0,0,0,0").left, 0);
    }
}
