//! what every object kind shares: the non-visual block (id, name, alt
//! text, placeholder) and the shape properties (`p:spPr`), read, written
//! fresh and patched.

use crate::xml::Element;

use super::super::dml::{
    attr, attr_bool, extra_attributes, find_fill, opt_attr, read_geometry, read_line, read_xfrm,
    write_fill, write_geometry, write_line, write_xfrm, ReadContext, WriteContext, FILL_NAMES,
};
use super::Changes;
use crate::model::{Fill, Geometry, Line, Placeholder, PlaceholderKind, RawChild, Transform};
use crate::preserve::{
    patch_element, rewrite_start_tag, start_tag_end, unknown_attributes, unknown_children,
    written_attributes, ChildEdit,
};

pub const GEOMETRY_NAMES: [&str; 2] = ["a:prstGeom", "a:custGeom"];

/// the `p:spPr` children the model reads; the rest are kept as raw xml
pub const SHAPE_PROPERTY_KNOWN: [&str; 9] = [
    "a:xfrm",
    "a:prstGeom",
    "a:custGeom",
    "a:noFill",
    "a:solidFill",
    "a:gradFill",
    "a:blipFill",
    "a:pattFill",
    "a:ln",
];

/// schema order of `p:spPr` children (ecma-376 part 1, 20.1.2.2.35), for
/// putting back a child that was not there before
pub const SHAPE_PROPERTY_ORDER: [&str; 15] = [
    "a:xfrm",
    "a:custGeom",
    "a:prstGeom",
    "a:noFill",
    "a:solidFill",
    "a:gradFill",
    "a:blipFill",
    "a:pattFill",
    "a:grpFill",
    "a:ln",
    "a:effectLst",
    "a:effectDag",
    "a:scene3d",
    "a:sp3d",
    "a:extLst",
];

/// the non-visual identity every object has (`p:cNvPr`).
pub struct Identity<'a> {
    pub id: u32,
    pub name: &'a str,
    pub description: &'a Option<String>,
    pub hidden: bool,
}

/// id, name, alt text and hidden flag read from `p:cNvPr`.
pub fn read_identity(nv: Option<&Element>) -> (u32, String, Option<String>, bool) {
    let common = nv.and_then(|nv| nv.child("p:cNvPr"));
    let id = common
        .and_then(|c| c.attr("id"))
        .and_then(|id| id.parse().ok())
        .unwrap_or(0);
    let name = common
        .and_then(|c| c.attr("name"))
        .unwrap_or("")
        .to_string();
    let description = common.and_then(|c| c.attr("descr")).map(str::to_string);
    let hidden = common.and_then(|c| attr_bool(c, "hidden")).unwrap_or(false);
    (id, name, description, hidden)
}

pub fn read_placeholder(nv: Option<&Element>) -> Option<Placeholder> {
    let ph = nv?.child("p:nvPr")?.child("p:ph")?;
    Some(Placeholder {
        kind: PlaceholderKind::from_pptx(ph.attr("type").unwrap_or("obj")),
        index: ph.attr("idx").and_then(|index| index.parse().ok()),
        orient: ph.attr("orient").map(str::to_string),
        size: ph.attr("sz").map(str::to_string),
        custom_prompt: attr_bool(ph, "hasCustomPrompt").unwrap_or(false),
    })
}

/// the raw attributes and unknown children of a `p:spPr`.
pub fn read_property_extras(
    properties: Option<&Element>,
    context: &ReadContext<'_>,
) -> Vec<RawChild> {
    let Some(properties) = properties else {
        return Vec::new();
    };
    let mut extra = unknown_attributes(properties, &[]);
    extra.extend(unknown_children(
        context.text,
        properties,
        &SHAPE_PROPERTY_KNOWN,
    ));
    extra
}

/// the modelled children of a `p:spPr`.
pub struct ShapeProperties {
    pub transform: Option<Transform>,
    pub geometry: Option<Geometry>,
    pub fill: Option<Fill>,
    pub line: Option<Line>,
}

pub fn read_shape_properties(
    properties: Option<&Element>,
    context: &ReadContext<'_>,
) -> ShapeProperties {
    let geometry = properties.and_then(|p| {
        p.elements()
            .find(|child| GEOMETRY_NAMES.contains(&child.name.as_str()))
            .and_then(|geometry| read_geometry(geometry, context))
    });
    ShapeProperties {
        transform: properties.and_then(|p| p.child("a:xfrm")).map(read_xfrm),
        geometry,
        fill: properties.and_then(|p| find_fill(p, context)),
        line: properties
            .and_then(|p| p.child("a:ln"))
            .map(|line| read_line(line, context)),
    }
}

pub fn identity_tag(identity: &Identity<'_>) -> String {
    format!(
        "<p:cNvPr{}{}{}{}/>",
        attr("id", identity.id),
        attr("name", identity.name),
        opt_attr("descr", identity.description.as_ref()),
        if identity.hidden {
            attr("hidden", "1")
        } else {
            String::new()
        }
    )
}

pub fn placeholder_tag(placeholder: &Placeholder) -> String {
    // "obj" is the default type, and powerpoint leaves it out
    let kind = if placeholder.kind == PlaceholderKind::Object {
        String::new()
    } else {
        attr("type", placeholder.kind.pptx_name())
    };
    let prompt = if placeholder.custom_prompt {
        attr("hasCustomPrompt", "1")
    } else {
        String::new()
    };
    format!(
        "<p:ph{kind}{}{}{}{prompt}/>",
        opt_attr("orient", placeholder.orient.as_ref()),
        opt_attr("sz", placeholder.size.as_ref()),
        opt_attr("idx", placeholder.index),
    )
}

pub fn application_properties(placeholder: &Option<Placeholder>) -> String {
    match placeholder {
        Some(placeholder) => format!("<p:nvPr>{}</p:nvPr>", placeholder_tag(placeholder)),
        None => "<p:nvPr/>".into(),
    }
}

/// an element's original text with some attributes of its opening tag
/// changed and its children kept.
pub fn retagged(
    source: &str,
    element: &Element,
    name: &str,
    updates: &[(&str, Option<String>)],
) -> String {
    let raw = &source[element.start..element.end];
    let tag_end = start_tag_end(raw);
    let self_closing = raw[..tag_end].ends_with("/>");
    let tag = rewrite_start_tag(name, &written_attributes(raw), updates, self_closing);
    format!("{tag}{}", &raw[tag_end..])
}

/// the schema order of `p:nvPr` children, for putting a placeholder back
const APPLICATION_ORDER: [&str; 8] = [
    "p:ph",
    "a:audioCd",
    "a:wavAudioFile",
    "a:audioFile",
    "a:videoFile",
    "a:quickTimeFile",
    "p:custDataLst",
    "p:extLst",
];

/// patches a non-visual block (`p:nvSpPr`, `p:nvPicPr`...): the identity,
/// optionally the kind-specific child (`p:cNvSpPr` with its `txBox`), and
/// the placeholder. hyperlinks, locks and extensions are copied.
pub fn patched_non_visual(
    source: &str,
    nv: &Element,
    identity: &Identity<'_>,
    placeholder: &Option<Placeholder>,
    kind_child: Option<(&str, String)>,
) -> String {
    let identity_names = ["p:cNvPr"];
    let kind_names = [kind_child.as_ref().map(|(name, _)| *name).unwrap_or("")];
    let application_names = ["p:nvPr"];
    let placeholder_names = ["p:ph"];
    let mut edits = Vec::new();
    if let Some(common) = nv.child("p:cNvPr") {
        let updates = [
            ("id", Some(identity.id.to_string())),
            ("name", Some(identity.name.to_string())),
            ("descr", identity.description.clone()),
            ("hidden", identity.hidden.then(|| "1".to_string())),
        ];
        edits.push(ChildEdit {
            names: &identity_names,
            xml: Some(retagged(source, common, "p:cNvPr", &updates)),
        });
    }
    if let Some((_, xml)) = kind_child {
        edits.push(ChildEdit {
            names: &kind_names,
            xml: Some(xml),
        });
    }
    if let Some(application) = nv.child("p:nvPr") {
        let placeholder_edit = [ChildEdit {
            names: &placeholder_names,
            xml: placeholder.as_ref().map(placeholder_tag),
        }];
        let patched = patch_element(
            source,
            application,
            None,
            &placeholder_edit,
            &APPLICATION_ORDER,
        );
        edits.push(ChildEdit {
            names: &application_names,
            xml: Some(patched),
        });
    }
    patch_element(source, nv, None, &edits, &[])
}

/// a whole `p:spPr` written from the model.
pub fn fresh_shape_properties(
    properties: &ShapeProperties,
    extra: &[RawChild],
    context: &mut WriteContext<'_>,
) -> String {
    let mut out = format!("<p:spPr{}>", extra_attributes(extra));
    if let Some(transform) = &properties.transform {
        out.push_str(&write_xfrm(transform, "a:xfrm"));
    }
    if let Some(geometry) = &properties.geometry {
        out.push_str(&write_geometry(geometry));
    }
    if let Some(fill) = &properties.fill {
        out.push_str(&write_fill(fill, context));
    }
    if let Some(line) = &properties.line {
        out.push_str(&write_line(line, "a:ln", context));
    }
    for child in extra.iter().filter(|child| child.xml.starts_with('<')) {
        out.push_str(&child.xml);
    }
    out.push_str("</p:spPr>");
    out
}

/// patches `p:spPr`: the transform, geometry, fill and line are replaced
/// when their pieces changed, everything else is copied.
pub fn patched_shape_properties(
    source: &str,
    element: &Element,
    properties: &ShapeProperties,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    let xfrm_names = ["a:xfrm"];
    let line_names = ["a:ln"];
    let mut edits = Vec::new();
    if changes.changed("xfrm") {
        edits.push(ChildEdit {
            names: &xfrm_names,
            xml: properties
                .transform
                .as_ref()
                .map(|t| write_xfrm(t, "a:xfrm")),
        });
    }
    if changes.changed("geometry") {
        edits.push(ChildEdit {
            names: &GEOMETRY_NAMES,
            xml: properties.geometry.as_ref().map(write_geometry),
        });
    }
    if changes.changed("fill") {
        edits.push(ChildEdit {
            names: &FILL_NAMES,
            xml: properties.fill.as_ref().map(|f| write_fill(f, context)),
        });
    }
    if changes.changed("line") {
        edits.push(ChildEdit {
            names: &line_names,
            xml: properties
                .line
                .as_ref()
                .map(|l| write_line(l, "a:ln", context)),
        });
    }
    patch_element(source, element, None, &edits, &SHAPE_PROPERTY_ORDER)
}

/// `p:spPr` for an edited object: patched when there was one, else fresh.
pub fn shape_properties_xml(
    original: Option<(&str, &Element)>,
    properties: &ShapeProperties,
    extra: &[RawChild],
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    match original.and_then(|(source, root)| root.child("p:spPr").map(|e| (source, e))) {
        Some((source, element)) => {
            patched_shape_properties(source, element, properties, changes, context)
        }
        None => fresh_shape_properties(properties, extra, context),
    }
}
