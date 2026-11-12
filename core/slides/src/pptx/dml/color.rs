//! drawingml colours (`a:srgbClr`, `a:schemeClr`...) with their
//! transforms, read and written.

use crate::xml::{local_name, Element};

use super::{attr, attr_i32, opt_attr};
use crate::model::{ColorKind, ColorMod, ColorRef};
// ---------------------------------------------------------------- colours

const COLOR_ELEMENTS: [&str; 6] = [
    "srgbClr",
    "schemeClr",
    "sysClr",
    "prstClr",
    "hslClr",
    "scrgbClr",
];

/// the first colour element among `parent`'s children.
pub fn find_color(parent: &Element) -> Option<ColorRef> {
    parent.elements().find_map(read_color)
}

pub fn read_color(element: &Element) -> Option<ColorRef> {
    let local = local_name(&element.name);
    if !COLOR_ELEMENTS.contains(&local) {
        return None;
    }
    let value = |name: &str| element.attr(name).unwrap_or("").to_string();
    let number = |name: &str| attr_i32(element, name).unwrap_or(0);
    let kind = match local {
        "srgbClr" => ColorKind::Rgb {
            hex: value("val").to_ascii_uppercase(),
        },
        "schemeClr" => ColorKind::Scheme { slot: value("val") },
        "sysClr" => ColorKind::System {
            name: value("val"),
            last: element.attr("lastClr").map(str::to_string),
        },
        "prstClr" => ColorKind::Preset { name: value("val") },
        "hslClr" => ColorKind::Hsl {
            hue: number("hue"),
            sat: number("sat"),
            lum: number("lum"),
        },
        _ => ColorKind::ScRgb {
            r: number("r"),
            g: number("g"),
            b: number("b"),
        },
    };
    let mods = element
        .elements()
        .map(|child| ColorMod {
            name: local_name(&child.name).to_string(),
            value: attr_i32(child, "val").unwrap_or(0),
        })
        .collect();
    Some(ColorRef { kind, mods })
}

pub fn write_color(color: &ColorRef) -> String {
    let (name, attributes) = match &color.kind {
        ColorKind::Rgb { hex } => ("a:srgbClr", attr("val", hex)),
        ColorKind::Scheme { slot } => ("a:schemeClr", attr("val", slot)),
        ColorKind::System { name, last } => (
            "a:sysClr",
            format!(
                "{}{}",
                attr("val", name),
                opt_attr("lastClr", last.as_ref())
            ),
        ),
        ColorKind::Preset { name } => ("a:prstClr", attr("val", name)),
        ColorKind::Hsl { hue, sat, lum } => (
            "a:hslClr",
            format!(
                "{}{}{}",
                attr("hue", hue),
                attr("sat", sat),
                attr("lum", lum)
            ),
        ),
        ColorKind::ScRgb { r, g, b } => (
            "a:scrgbClr",
            format!("{}{}{}", attr("r", r), attr("g", g), attr("b", b)),
        ),
    };
    if color.mods.is_empty() {
        return format!("<{name}{attributes}/>");
    }
    let mut out = format!("<{name}{attributes}>");
    for modifier in &color.mods {
        // a few transforms take no value at all
        if matches!(
            modifier.name.as_str(),
            "comp" | "inv" | "gray" | "gamma" | "invGamma"
        ) {
            out.push_str(&format!("<a:{}/>", modifier.name));
        } else {
            out.push_str(&format!(
                "<a:{}{}/>",
                modifier.name,
                attr("val", modifier.value)
            ));
        }
    }
    out.push_str(&format!("</{name}>"));
    out
}
