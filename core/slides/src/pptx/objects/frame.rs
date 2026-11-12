//! graphic frames (`p:graphicFrame`): tables are modelled; charts,
//! smartart and ole objects are named and carried with their parts.

use crate::xml::Element;

use super::super::dml::{read_xfrm, write_xfrm, ReadContext, WriteContext};
use super::common::{
    application_properties, identity_tag, patched_non_visual, read_identity, read_placeholder,
    Identity,
};
use super::table::{read_table, write_table};
use super::{piece, Changes};
use crate::model::{GraphicFrame, GraphicKind, Table};
use crate::preserve::{patch_element, ChildEdit};

const TABLE_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/table";

pub fn read_frame(element: &Element, context: &ReadContext<'_>) -> GraphicFrame {
    let nv = element.child("p:nvGraphicFramePr");
    let (id, name, description, hidden) = read_identity(nv);
    let data = element
        .child("a:graphic")
        .and_then(|graphic| graphic.child("a:graphicData"));
    // an ole object's stand-in picture is a blip inside it; charts and
    // diagrams have none in pptx (the application draws them)
    let fallback_media_id = data
        .and_then(|d| find_descendant(d, "a:blip"))
        .and_then(|blip| blip.attr("r:embed"))
        .and_then(|rel| context.media_id(rel));
    GraphicFrame {
        id,
        name,
        description,
        hidden,
        placeholder: read_placeholder(nv),
        transform: element.child("p:xfrm").map(read_xfrm),
        kind: data
            .map(|d| read_kind(d, context))
            .unwrap_or(GraphicKind::Other { uri: String::new() }),
        fallback_media_id,
        source: None,
    }
}

fn read_kind(data: &Element, context: &ReadContext<'_>) -> GraphicKind {
    let uri = data.attr("uri").unwrap_or("").to_string();
    let target = |id: Option<&str>| {
        id.and_then(|id| context.rels.get(id))
            .map(|rel| rel.resolved(context.part))
    };
    let first = data.elements().next();
    if let Some(table) = data.child("a:tbl") {
        return GraphicKind::Table(read_table(table, context));
    }
    if uri.ends_with("/chart") {
        return GraphicKind::Chart {
            part: target(first.and_then(|c| c.attr("r:id"))).unwrap_or_default(),
        };
    }
    if uri.ends_with("/diagram") {
        let parts = ["r:dm", "r:lo", "r:qs", "r:cs"]
            .iter()
            .filter_map(|name| target(first.and_then(|ids| ids.attr(name))))
            .collect();
        return GraphicKind::Diagram { parts };
    }
    if uri.ends_with("/ole") {
        let ole = find_descendant(data, "p:oleObj");
        return GraphicKind::Ole {
            prog_id: ole.and_then(|o| o.attr("progId")).map(str::to_string),
            part: target(ole.and_then(|o| o.attr("r:id"))),
        };
    }
    GraphicKind::Other { uri }
}

pub fn find_descendant<'a>(element: &'a Element, name: &str) -> Option<&'a Element> {
    for child in element.elements() {
        if child.name == name {
            return Some(child);
        }
        if let Some(found) = find_descendant(child, name) {
            return Some(found);
        }
    }
    None
}

pub fn pieces(frame: &GraphicFrame) -> Vec<(String, String)> {
    let identity = (
        frame.id,
        &frame.name,
        &frame.description,
        frame.hidden,
        &frame.placeholder,
    );
    vec![
        piece("nv", &identity),
        piece("xfrm", &frame.transform),
        piece("graphic", &frame.kind),
    ]
}

fn identity(frame: &GraphicFrame) -> Identity<'_> {
    Identity {
        id: frame.id,
        name: &frame.name,
        description: &frame.description,
        hidden: frame.hidden,
    }
}

pub fn write_frame(
    frame: &GraphicFrame,
    original: Option<(&str, &Element)>,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    match (original, &frame.kind) {
        (Some((source, element)), _) => patch_frame(frame, source, element, changes, context),
        (None, GraphicKind::Table(table)) => fresh_table_frame(frame, table, context),
        // only tables can be made from nothing; other kinds need their parts
        (None, _) => String::new(),
    }
}

fn fresh_table_frame(
    frame: &GraphicFrame,
    table: &Table,
    context: &mut WriteContext<'_>,
) -> String {
    let transform = frame.transform.clone().unwrap_or_default();
    format!(
        "<p:graphicFrame><p:nvGraphicFramePr>{}<p:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></p:cNvGraphicFramePr>{}</p:nvGraphicFramePr>{}<a:graphic><a:graphicData uri=\"{TABLE_URI}\">{}</a:graphicData></a:graphic></p:graphicFrame>",
        identity_tag(&identity(frame)),
        application_properties(&frame.placeholder),
        write_xfrm(&transform, "p:xfrm"),
        write_table(table, None, context)
    )
}

fn patch_frame(
    frame: &GraphicFrame,
    source: &str,
    element: &Element,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    let non_visual_names = ["p:nvGraphicFramePr"];
    let xfrm_names = ["p:xfrm"];
    let graphic_names = ["a:graphic"];
    let mut edits = Vec::new();
    if changes.changed("nv") {
        if let Some(nv) = element.child("p:nvGraphicFramePr") {
            let xml = patched_non_visual(source, nv, &identity(frame), &frame.placeholder, None);
            edits.push(ChildEdit {
                names: &non_visual_names,
                xml: Some(xml),
            });
        }
    }
    if changes.changed("xfrm") {
        let transform = frame.transform.clone().unwrap_or_default();
        edits.push(ChildEdit {
            names: &xfrm_names,
            xml: Some(write_xfrm(&transform, "p:xfrm")),
        });
    }
    let graphic = element.child("a:graphic");
    if let (true, GraphicKind::Table(table), Some(graphic)) =
        (changes.changed("graphic"), &frame.kind, graphic)
    {
        edits.push(ChildEdit {
            names: &graphic_names,
            xml: Some(patched_table_graphic(table, source, graphic, context)),
        });
    }
    patch_element(source, element, None, &edits, &[])
}

/// `a:graphic` with its table rebuilt (unchanged rows and cells reused).
fn patched_table_graphic(
    table: &Table,
    source: &str,
    graphic: &Element,
    context: &mut WriteContext<'_>,
) -> String {
    let Some(data) = graphic.child("a:graphicData") else {
        return source[graphic.start..graphic.end].to_string();
    };
    let original_table = data.child("a:tbl").map(|t| (source, t));
    let table_names = ["a:tbl"];
    let data_names = ["a:graphicData"];
    let table_edit = [ChildEdit {
        names: &table_names,
        xml: Some(write_table(table, original_table, context)),
    }];
    let inner = patch_element(source, data, None, &table_edit, &[]);
    let data_edit = [ChildEdit {
        names: &data_names,
        xml: Some(inner),
    }];
    patch_element(source, graphic, None, &data_edit, &[])
}
