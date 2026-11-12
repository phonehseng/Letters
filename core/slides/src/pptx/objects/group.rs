//! groups (`p:grpSp`): a box with its own child coordinate space and the
//! objects inside it, each written by its own rules.

use crate::xml::Element;

use super::super::dml::{
    find_fill, read_xfrm, write_fill, write_xfrm, ReadContext, WriteContext, FILL_NAMES,
};
use super::common::{
    identity_tag, patched_non_visual, read_identity, Identity, SHAPE_PROPERTY_ORDER,
};
use super::{piece, read_objects, write_objects, Changes};
use crate::model::Group;
use crate::preserve::{patch_element, start_tag_end, ChildEdit};

pub fn read_group(element: &Element, context: &ReadContext<'_>) -> Group {
    let nv = element.child("p:nvGrpSpPr");
    let (id, name, description, hidden) = read_identity(nv);
    let properties = element.child("p:grpSpPr");
    Group {
        id,
        name,
        description,
        hidden,
        transform: properties
            .and_then(|p| p.child("a:xfrm"))
            .map(read_xfrm)
            .unwrap_or_default(),
        fill: properties.and_then(|p| find_fill(p, context)),
        objects: read_objects(element, context),
        source: None,
    }
}

pub fn pieces(group: &Group) -> Vec<(String, String)> {
    let identity = (group.id, &group.name, &group.description, group.hidden);
    vec![
        piece("nv", &identity),
        piece("xfrm", &group.transform),
        piece("fill", &group.fill),
    ]
}

fn identity(group: &Group) -> Identity<'_> {
    Identity {
        id: group.id,
        name: &group.name,
        description: &group.description,
        hidden: group.hidden,
    }
}

fn fresh_group_properties(group: &Group, context: &mut WriteContext<'_>) -> String {
    let mut out = String::from("<p:grpSpPr>");
    out.push_str(&write_xfrm(&group.transform, "a:xfrm"));
    if let Some(fill) = &group.fill {
        out.push_str(&write_fill(fill, context));
    }
    out.push_str("</p:grpSpPr>");
    out
}

/// a group: its children are always written one by one, so an edited
/// child leaves its siblings byte-identical.
pub fn write_group(
    group: &Group,
    original: Option<(&str, &Element)>,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    let children = write_objects(&group.objects, context);
    let Some((source, element)) = original else {
        return format!(
            "<p:grpSp><p:nvGrpSpPr>{}<p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>{}{children}</p:grpSp>",
            identity_tag(&identity(group)),
            fresh_group_properties(group, context)
        );
    };
    let raw = &source[element.start..element.end];
    let mut out = raw[..start_tag_end(raw)].to_string();
    for child in element.elements() {
        match child.name.as_str() {
            "p:nvGrpSpPr" if changes.changed("nv") => out.push_str(&patched_non_visual(
                source,
                child,
                &identity(group),
                &None,
                None,
            )),
            "p:grpSpPr" if changes.any(&["xfrm", "fill"]) => out.push_str(
                &patched_group_properties(group, source, child, changes, context),
            ),
            "p:nvGrpSpPr" | "p:grpSpPr" => out.push_str(&source[child.start..child.end]),
            _ => {}
        }
    }
    out.push_str(&children);
    for extensions in element.children_named("p:extLst") {
        out.push_str(&source[extensions.start..extensions.end]);
    }
    out.push_str("</p:grpSp>");
    out
}

fn patched_group_properties(
    group: &Group,
    source: &str,
    element: &Element,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    let xfrm_names = ["a:xfrm"];
    let mut edits = Vec::new();
    if changes.changed("xfrm") {
        edits.push(ChildEdit {
            names: &xfrm_names,
            xml: Some(write_xfrm(&group.transform, "a:xfrm")),
        });
    }
    if changes.changed("fill") {
        edits.push(ChildEdit {
            names: &FILL_NAMES,
            xml: group.fill.as_ref().map(|fill| write_fill(fill, context)),
        });
    }
    patch_element(source, element, None, &edits, &SHAPE_PROPERTY_ORDER)
}
