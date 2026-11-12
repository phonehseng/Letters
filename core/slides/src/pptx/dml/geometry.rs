//! transforms (`a:xfrm`), preset and custom geometry, and theme style
//! references (`p:style`), read and written.

use crate::xml::Element;

use super::color::{find_color, write_color};
use super::{attr, attr_bool, attr_i32, attr_i64, bool_text, extra_after, opt_attr, ReadContext};
use crate::model::{Geometry, PathCommand, PathPoint, ShapePath, ShapeStyle, StyleRef, Transform};
use crate::preserve::unknown_children;

// ---------------------------------------------------------------- transforms

/// `a:xfrm` (or `p:xfrm` of a graphic frame).
pub fn read_xfrm(element: &Element) -> Transform {
    let offset = element.child("a:off");
    let extent = element.child("a:ext");
    let child_offset = element.child("a:chOff");
    let child_extent = element.child("a:chExt");
    Transform {
        x: offset.and_then(|o| attr_i64(o, "x")).unwrap_or(0),
        y: offset.and_then(|o| attr_i64(o, "y")).unwrap_or(0),
        width: extent.and_then(|e| attr_i64(e, "cx")).unwrap_or(0),
        height: extent.and_then(|e| attr_i64(e, "cy")).unwrap_or(0),
        rotation: attr_i32(element, "rot").unwrap_or(0),
        flip_h: attr_bool(element, "flipH").unwrap_or(false),
        flip_v: attr_bool(element, "flipV").unwrap_or(false),
        child_x: child_offset.and_then(|o| attr_i64(o, "x")),
        child_y: child_offset.and_then(|o| attr_i64(o, "y")),
        child_width: child_extent.and_then(|e| attr_i64(e, "cx")),
        child_height: child_extent.and_then(|e| attr_i64(e, "cy")),
    }
}

pub fn write_xfrm(transform: &Transform, name: &str) -> String {
    let mut out = format!(
        "<{name}{}{}{}>",
        if transform.rotation != 0 {
            attr("rot", transform.rotation)
        } else {
            String::new()
        },
        if transform.flip_h {
            attr("flipH", "1")
        } else {
            String::new()
        },
        if transform.flip_v {
            attr("flipV", "1")
        } else {
            String::new()
        },
    );
    out.push_str(&format!(
        "<a:off{}{}/><a:ext{}{}/>",
        attr("x", transform.x),
        attr("y", transform.y),
        attr("cx", transform.width),
        attr("cy", transform.height)
    ));
    if let (Some(x), Some(y)) = (transform.child_x, transform.child_y) {
        out.push_str(&format!("<a:chOff{}{}/>", attr("x", x), attr("y", y)));
    }
    if let (Some(width), Some(height)) = (transform.child_width, transform.child_height) {
        out.push_str(&format!(
            "<a:chExt{}{}/>",
            attr("cx", width),
            attr("cy", height)
        ));
    }
    out.push_str(&format!("</{name}>"));
    out
}

// ---------------------------------------------------------------- geometry

pub fn read_geometry(element: &Element, context: &ReadContext<'_>) -> Option<Geometry> {
    match element.name.as_str() {
        "a:prstGeom" => Some(Geometry::Preset {
            name: element.attr("prst").unwrap_or("rect").to_string(),
            adjust: element
                .child("a:avLst")
                .map(|list| {
                    list.children_named("a:gd")
                        .filter_map(|guide| {
                            let name = guide.attr("name")?.to_string();
                            let value = guide.attr("fmla")?.strip_prefix("val ")?.trim();
                            Some((name, value.parse().ok()?))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "a:custGeom" => Some(read_custom(element, context)),
        _ => None,
    }
}

fn guides_of(element: Option<&Element>) -> Vec<(String, String)> {
    element
        .map(|list| {
            list.children_named("a:gd")
                .filter_map(|guide| {
                    Some((
                        guide.attr("name")?.to_string(),
                        guide.attr("fmla").unwrap_or("val 0").to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn read_custom(element: &Element, context: &ReadContext<'_>) -> Geometry {
    let paths = element
        .child("a:pathLst")
        .map(|list| list.children_named("a:path").map(read_path).collect())
        .unwrap_or_default();
    Geometry::Custom {
        adjust: guides_of(element.child("a:avLst")),
        guides: guides_of(element.child("a:gdLst")),
        paths,
        extra: unknown_children(context.text, element, &["a:avLst", "a:gdLst", "a:pathLst"]),
    }
}

fn point_of(element: Option<&Element>) -> PathPoint {
    let point = element.and_then(|e| e.child("a:pt").or(Some(e)));
    PathPoint {
        x: point.and_then(|p| p.attr("x")).unwrap_or("0").to_string(),
        y: point.and_then(|p| p.attr("y")).unwrap_or("0").to_string(),
    }
}

fn read_path(path: &Element) -> ShapePath {
    let commands = path
        .elements()
        .filter_map(|step| {
            let points: Vec<&Element> = step.children_named("a:pt").collect();
            let point = |index: usize| point_of(points.get(index).copied());
            Some(match step.name.as_str() {
                "a:moveTo" => PathCommand::Move { to: point(0) },
                "a:lnTo" => PathCommand::Line { to: point(0) },
                "a:arcTo" => PathCommand::Arc {
                    width_radius: step.attr("wR").unwrap_or("0").to_string(),
                    height_radius: step.attr("hR").unwrap_or("0").to_string(),
                    start: step.attr("stAng").unwrap_or("0").to_string(),
                    swing: step.attr("swAng").unwrap_or("0").to_string(),
                },
                "a:quadBezTo" => PathCommand::Quadratic {
                    control: point(0),
                    to: point(1),
                },
                "a:cubicBezTo" => PathCommand::Cubic {
                    control1: point(0),
                    control2: point(1),
                    to: point(2),
                },
                "a:close" => PathCommand::Close,
                _ => return None,
            })
        })
        .collect();
    ShapePath {
        width: attr_i64(path, "w"),
        height: attr_i64(path, "h"),
        fill: path.attr("fill").map(str::to_string),
        stroke: attr_bool(path, "stroke").unwrap_or(true),
        extrusion_ok: attr_bool(path, "extrusionOk"),
        commands,
    }
}

pub fn write_geometry(geometry: &Geometry) -> String {
    match geometry {
        Geometry::Preset { name, adjust } => {
            // powerpoint refuses a file naming a preset it does not define
            let name = if crate::geometry::is_preset_name(name) {
                name.as_str()
            } else {
                "rect"
            };
            let mut out = format!("<a:prstGeom{}>", attr("prst", name));
            if adjust.is_empty() {
                out.push_str("<a:avLst/>");
            } else {
                out.push_str("<a:avLst>");
                for (guide, value) in adjust {
                    out.push_str(&format!(
                        "<a:gd{}{}/>",
                        attr("name", guide),
                        attr("fmla", format!("val {value}"))
                    ));
                }
                out.push_str("</a:avLst>");
            }
            out.push_str("</a:prstGeom>");
            out
        }
        Geometry::Custom {
            adjust,
            guides,
            paths,
            extra,
        } => {
            let list = |name: &str, guides: &[(String, String)]| {
                if guides.is_empty() {
                    return format!("<a:{name}/>");
                }
                let mut out = format!("<a:{name}>");
                for (guide, formula) in guides {
                    out.push_str(&format!(
                        "<a:gd{}{}/>",
                        attr("name", guide),
                        attr("fmla", formula)
                    ));
                }
                out.push_str(&format!("</a:{name}>"));
                out
            };
            let mut out = String::from("<a:custGeom>");
            out.push_str(&list("avLst", adjust));
            out.push_str(&list("gdLst", guides));
            out.push_str(&extra_after(extra, Some("a:gdLst")));
            out.push_str(&extra_after(extra, Some("a:avLst")));
            out.push_str("<a:pathLst>");
            for path in paths {
                out.push_str(&write_path(path));
            }
            out.push_str("</a:pathLst>");
            out.push_str(&extra_after(extra, Some("a:pathLst")));
            out.push_str("</a:custGeom>");
            out
        }
    }
}

fn write_point(point: &PathPoint) -> String {
    format!("<a:pt{}{}/>", attr("x", &point.x), attr("y", &point.y))
}

fn write_path(path: &ShapePath) -> String {
    let mut out = format!(
        "<a:path{}{}{}{}{}>",
        opt_attr("w", path.width),
        opt_attr("h", path.height),
        opt_attr("fill", path.fill.as_ref()),
        if path.stroke {
            String::new()
        } else {
            attr("stroke", "0")
        },
        opt_attr("extrusionOk", path.extrusion_ok.map(bool_text)),
    );
    for command in &path.commands {
        out.push_str(&match command {
            PathCommand::Move { to } => format!("<a:moveTo>{}</a:moveTo>", write_point(to)),
            PathCommand::Line { to } => format!("<a:lnTo>{}</a:lnTo>", write_point(to)),
            PathCommand::Arc {
                width_radius,
                height_radius,
                start,
                swing,
            } => format!(
                "<a:arcTo{}{}{}{}/>",
                attr("wR", width_radius),
                attr("hR", height_radius),
                attr("stAng", start),
                attr("swAng", swing)
            ),
            PathCommand::Quadratic { control, to } => format!(
                "<a:quadBezTo>{}{}</a:quadBezTo>",
                write_point(control),
                write_point(to)
            ),
            PathCommand::Cubic {
                control1,
                control2,
                to,
            } => format!(
                "<a:cubicBezTo>{}{}{}</a:cubicBezTo>",
                write_point(control1),
                write_point(control2),
                write_point(to)
            ),
            PathCommand::Close => "<a:close/>".into(),
        });
    }
    out.push_str("</a:path>");
    out
}

// ---------------------------------------------------------------- style refs

pub fn read_style(element: &Element) -> ShapeStyle {
    let reference = |name: &str| {
        let element = element.child(name);
        StyleRef {
            index: element
                .and_then(|e| e.attr("idx"))
                .map(|index| match index {
                    "major" => 1,
                    "minor" => 2,
                    "none" => 0,
                    other => other.parse().unwrap_or(0),
                })
                .unwrap_or(0),
            color: element.and_then(find_color),
        }
    };
    ShapeStyle {
        line: reference("a:lnRef"),
        fill: reference("a:fillRef"),
        effect: reference("a:effectRef"),
        font: reference("a:fontRef"),
    }
}

pub fn write_style(style: &ShapeStyle, prefix: &str) -> String {
    let reference = |name: &str, reference: &StyleRef, font: bool| {
        let index = if font {
            match reference.index {
                1 => "major".to_string(),
                2 => "minor".to_string(),
                _ => "none".to_string(),
            }
        } else {
            reference.index.to_string()
        };
        match &reference.color {
            Some(color) => format!(
                "<a:{name}{}>{}</a:{name}>",
                attr("idx", index),
                write_color(color)
            ),
            None => format!("<a:{name}{}/>", attr("idx", index)),
        }
    };
    format!(
        "<{prefix}:style>{}{}{}{}</{prefix}:style>",
        reference("lnRef", &style.line, false),
        reference("fillRef", &style.fill, false),
        reference("effectRef", &style.effect, false),
        reference("fontRef", &style.font, true)
    )
}
