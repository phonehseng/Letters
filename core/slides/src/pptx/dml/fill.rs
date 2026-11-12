//! drawingml fills (solid, gradient, picture, pattern) and lines,
//! read and written.

use crate::xml::Element;

use super::color::{find_color, write_color};
use super::{
    attr, attr_bool, attr_i32, attr_i64, bool_text, extra_after, extra_attributes, opt_attr,
    ReadContext, WriteContext,
};
use crate::model::{Fill, GradientFill, GradientStop, Line, LineEnd, RawChild};
use crate::preserve::{unknown_attributes, unknown_children};

// ---------------------------------------------------------------- fills

pub const FILL_NAMES: [&str; 6] = [
    "a:noFill",
    "a:solidFill",
    "a:gradFill",
    "a:blipFill",
    "a:pattFill",
    "a:grpFill",
];

/// the fill among `parent`'s children, if any.
pub fn find_fill(parent: &Element, context: &ReadContext<'_>) -> Option<Fill> {
    parent
        .elements()
        .find(|child| FILL_NAMES.contains(&child.name.as_str()))
        .and_then(|child| read_fill(child, context))
}

pub fn read_fill(element: &Element, context: &ReadContext<'_>) -> Option<Fill> {
    Some(match element.name.as_str() {
        "a:noFill" => Fill::None,
        "a:solidFill" => Fill::Solid {
            color: find_color(element)?,
        },
        "a:gradFill" => Fill::Gradient(read_gradient(element, context)),
        "a:blipFill" => Fill::Picture {
            media_id: element
                .child("a:blip")
                .and_then(|blip| blip.attr("r:embed"))
                .and_then(|id| context.media_id(id)),
            xml: Some(context.raw(element)),
        },
        "a:pattFill" => Fill::Pattern {
            preset: element.attr("prst").unwrap_or("pct5").to_string(),
            foreground: element.child("a:fgClr").and_then(find_color),
            background: element.child("a:bgClr").and_then(find_color),
        },
        "a:grpFill" => Fill::Group,
        _ => return None,
    })
}

fn read_gradient(element: &Element, context: &ReadContext<'_>) -> GradientFill {
    let stops = element
        .child("a:gsLst")
        .map(|list| {
            list.children_named("a:gs")
                .filter_map(|stop| {
                    Some(GradientStop {
                        position: attr_i32(stop, "pos").unwrap_or(0),
                        color: find_color(stop)?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let linear = element.child("a:lin");
    let path = element.child("a:path");
    let focus = path
        .and_then(|path| path.child("a:fillToRect"))
        .map(|rect| {
            [
                attr_i32(rect, "l").unwrap_or(0),
                attr_i32(rect, "t").unwrap_or(0),
                attr_i32(rect, "r").unwrap_or(0),
                attr_i32(rect, "b").unwrap_or(0),
            ]
        });
    let mut extra = unknown_attributes(element, &["rotWithShape"]);
    extra.extend(unknown_children(
        context.text,
        element,
        &["a:gsLst", "a:lin", "a:path"],
    ));
    GradientFill {
        stops,
        angle: linear.and_then(|lin| attr_i32(lin, "ang")),
        scaled: linear
            .and_then(|lin| attr_bool(lin, "scaled"))
            .unwrap_or(false),
        path: path.map(|path| path.attr("path").unwrap_or("circle").to_string()),
        focus,
        rotate_with_shape: attr_bool(element, "rotWithShape").unwrap_or(false),
        extra,
    }
}

/// writes a fill. a picture fill needs its media relationship, so the
/// writer context comes along; without one the original xml is used.
pub fn write_fill(fill: &Fill, context: &mut WriteContext<'_>) -> String {
    match fill {
        Fill::None => "<a:noFill/>".into(),
        Fill::Solid { color } => format!("<a:solidFill>{}</a:solidFill>", write_color(color)),
        Fill::Gradient(gradient) => write_gradient(gradient),
        Fill::Picture { media_id, xml } => match (xml, media_id) {
            (Some(xml), _) => xml.clone(),
            (None, Some(media_id)) => match context.media_rel(media_id, crate::pptx::rels::IMAGE) {
                Some(rel) => format!(
                    "<a:blipFill><a:blip r:embed=\"{rel}\"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"
                ),
                None => "<a:noFill/>".into(),
            },
            (None, None) => "<a:noFill/>".into(),
        },
        Fill::Pattern {
            preset,
            foreground,
            background,
        } => {
            let mut out = format!("<a:pattFill{}>", attr("prst", preset));
            if let Some(color) = foreground {
                out.push_str(&format!("<a:fgClr>{}</a:fgClr>", write_color(color)));
            }
            if let Some(color) = background {
                out.push_str(&format!("<a:bgClr>{}</a:bgClr>", write_color(color)));
            }
            out.push_str("</a:pattFill>");
            out
        }
        Fill::Group => "<a:grpFill/>".into(),
    }
}

fn write_gradient(gradient: &GradientFill) -> String {
    let mut out = format!(
        "<a:gradFill{}{}>",
        if gradient.rotate_with_shape {
            attr("rotWithShape", "1")
        } else {
            String::new()
        },
        extra_attributes(&gradient.extra)
    );
    out.push_str("<a:gsLst>");
    for stop in &gradient.stops {
        out.push_str(&format!(
            "<a:gs{}>{}</a:gs>",
            attr("pos", stop.position),
            write_color(&stop.color)
        ));
    }
    out.push_str("</a:gsLst>");
    if let Some(path) = &gradient.path {
        out.push_str(&format!("<a:path{}>", attr("path", path)));
        if let Some([l, t, r, b]) = gradient.focus {
            out.push_str(&format!(
                "<a:fillToRect{}{}{}{}/>",
                attr("l", l),
                attr("t", t),
                attr("r", r),
                attr("b", b)
            ));
        }
        out.push_str("</a:path>");
    } else {
        out.push_str(&format!(
            "<a:lin{}{}/>",
            attr("ang", gradient.angle.unwrap_or(0)),
            attr("scaled", bool_text(gradient.scaled))
        ));
    }
    out.push_str(&extra_after(&gradient.extra, Some("a:gsLst")));
    out.push_str(&extra_after(&gradient.extra, Some("a:lin")));
    out.push_str(&extra_after(&gradient.extra, Some("a:path")));
    out.push_str("</a:gradFill>");
    out
}

// ---------------------------------------------------------------- lines

const LINE_KNOWN: [&str; 10] = [
    "a:noFill",
    "a:solidFill",
    "a:gradFill",
    "a:pattFill",
    "a:prstDash",
    "a:round",
    "a:bevel",
    "a:miter",
    "a:headEnd",
    "a:tailEnd",
];

pub fn read_line(element: &Element, context: &ReadContext<'_>) -> Line {
    let end = |name: &str| {
        element.child(name).map(|end| LineEnd {
            kind: end.attr("type").unwrap_or("none").to_string(),
            width: end.attr("w").map(str::to_string),
            length: end.attr("len").map(str::to_string),
        })
    };
    let join = ["a:round", "a:bevel", "a:miter"]
        .iter()
        .find(|name| element.child(name).is_some())
        .map(|name| name.trim_start_matches("a:").to_string());
    // a miter limit is the one join detail worth keeping exactly
    let mut extra = unknown_attributes(element, &["w", "cap", "cmpd"]);
    if let Some(miter) = element.child("a:miter") {
        if miter.attr("lim").is_some() {
            extra.push(RawChild::element(Some("a:miter"), &context.raw(miter)));
        }
    }
    extra.extend(unknown_children(context.text, element, &LINE_KNOWN));
    Line {
        width: attr_i64(element, "w"),
        fill: find_fill(element, context),
        dash: element
            .child("a:prstDash")
            .and_then(|dash| dash.attr("val"))
            .map(str::to_string),
        cap: element.attr("cap").map(str::to_string),
        compound: element.attr("cmpd").map(str::to_string),
        join,
        head: end("a:headEnd"),
        tail: end("a:tailEnd"),
        extra,
    }
}

/// `a:ln` (or `a:lnL`... in table cells when `name` says so).
pub fn write_line(line: &Line, name: &str, context: &mut WriteContext<'_>) -> String {
    let mut out = format!(
        "<{name}{}{}{}{}>",
        opt_attr("w", line.width),
        opt_attr("cap", line.cap.as_ref()),
        opt_attr("cmpd", line.compound.as_ref()),
        extra_attributes(&line.extra)
    );
    if let Some(fill) = &line.fill {
        out.push_str(&write_fill(fill, context));
    }
    if let Some(dash) = &line.dash {
        out.push_str(&format!("<a:prstDash{}/>", attr("val", dash)));
    }
    out.push_str(&extra_after(&line.extra, Some("a:prstDash")));
    let custom_miter = line
        .extra
        .iter()
        .any(|child| child.after.as_deref() == Some("a:miter"));
    match line.join.as_deref() {
        Some("miter") if custom_miter => {}
        Some(join) => out.push_str(&format!("<a:{join}/>")),
        None => {}
    }
    out.push_str(&extra_after(&line.extra, Some("a:miter")));
    for (end, element) in [(&line.head, "a:headEnd"), (&line.tail, "a:tailEnd")] {
        if let Some(end) = end {
            out.push_str(&format!(
                "<{element}{}{}{}/>",
                attr("type", &end.kind),
                opt_attr("w", end.width.as_ref()),
                opt_attr("len", end.length.as_ref())
            ));
        }
    }
    for after in [
        None,
        Some("a:noFill"),
        Some("a:solidFill"),
        Some("a:gradFill"),
        Some("a:pattFill"),
        Some("a:round"),
        Some("a:bevel"),
        Some("a:headEnd"),
        Some("a:tailEnd"),
    ] {
        out.push_str(&extra_after(&line.extra, after));
    }
    out.push_str(&format!("</{name}>"));
    out
}
