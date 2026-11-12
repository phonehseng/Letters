//! autoshapes, text boxes and placeholders (`p:sp`).

use crate::xml::Element;

use super::super::dml::{attr_bool, read_style, write_style, ReadContext, WriteContext};
use super::super::text::{
    read_text_body, write_body, write_paragraph, write_text_body, write_text_style,
};
use super::common::{
    application_properties, fresh_shape_properties, identity_tag, patched_non_visual,
    read_identity, read_placeholder, read_property_extras, read_shape_properties, retagged,
    shape_properties_xml, Identity, ShapeProperties,
};
use super::{piece, Changes};
use crate::model::{Shape, TextBody};
use crate::preserve::{patch_element, start_tag_end, ChildEdit};

/// schema order of `p:sp` children
const SHAPE_ORDER: [&str; 5] = ["p:nvSpPr", "p:spPr", "p:style", "p:txBody", "p:extLst"];

/// what an empty text body holds: a text body must have a paragraph
const EMPTY_PARAGRAPH: &str = "<a:p><a:endParaRPr lang=\"en-US\" dirty=\"0\"/></a:p>";

pub fn read_shape(element: &Element, context: &ReadContext<'_>) -> Shape {
    let nv = element.child("p:nvSpPr");
    let (id, name, description, hidden) = read_identity(nv);
    let properties_element = element.child("p:spPr");
    let properties = read_shape_properties(properties_element, context);
    let text_box = nv
        .and_then(|nv| nv.child("p:cNvSpPr"))
        .and_then(|c| attr_bool(c, "txBox"))
        .unwrap_or(false);
    Shape {
        id,
        name,
        description,
        hidden,
        text_box,
        placeholder: read_placeholder(nv),
        transform: properties.transform,
        geometry: properties.geometry,
        fill: properties.fill,
        line: properties.line,
        style: element.child("p:style").map(read_style),
        text: element
            .child("p:txBody")
            .map(|body| read_text_body(body, context)),
        extra: read_property_extras(properties_element, context),
        source: None,
    }
}

pub fn pieces(shape: &Shape) -> Vec<(String, String)> {
    let identity = (
        shape.id,
        &shape.name,
        &shape.description,
        shape.hidden,
        shape.text_box,
        &shape.placeholder,
    );
    vec![
        piece("nv", &identity),
        piece("xfrm", &shape.transform),
        piece("geometry", &shape.geometry),
        piece("fill", &shape.fill),
        piece("line", &shape.line),
        piece("extra", &shape.extra),
        piece("style", &shape.style),
        piece("text", &shape.text),
        piece("body", &shape.text.as_ref().map(|text| &text.body)),
        piece(
            "lstStyle",
            &shape.text.as_ref().map(|text| &text.list_style),
        ),
    ]
}

fn identity(shape: &Shape) -> Identity<'_> {
    Identity {
        id: shape.id,
        name: &shape.name,
        description: &shape.description,
        hidden: shape.hidden,
    }
}

fn properties(shape: &Shape) -> ShapeProperties {
    ShapeProperties {
        transform: shape.transform.clone(),
        geometry: shape.geometry.clone(),
        fill: shape.fill.clone(),
        line: shape.line.clone(),
    }
}

pub fn write_shape(
    shape: &Shape,
    original: Option<(&str, &Element)>,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    match original {
        // effects and 3d are kept as raw xml; when those were edited the
        // whole shape is rebuilt so the raw pieces are the new ones
        Some((source, element)) if !changes.changed("extra") => {
            patch_shape(shape, source, element, changes, context)
        }
        _ => fresh_shape(shape, context),
    }
}

fn fresh_shape(shape: &Shape, context: &mut WriteContext<'_>) -> String {
    let mut out = String::from("<p:sp><p:nvSpPr>");
    out.push_str(&identity_tag(&identity(shape)));
    out.push_str(if shape.text_box {
        "<p:cNvSpPr txBox=\"1\"/>"
    } else {
        "<p:cNvSpPr/>"
    });
    out.push_str(&application_properties(&shape.placeholder));
    out.push_str("</p:nvSpPr>");
    out.push_str(&fresh_shape_properties(
        &properties(shape),
        &shape.extra,
        context,
    ));
    if let Some(style) = &shape.style {
        out.push_str(&write_style(style, "p"));
    }
    if let Some(text) = &shape.text {
        out.push_str(&write_text_body(text, "p:txBody", context));
    }
    out.push_str("</p:sp>");
    out
}

fn patch_shape(
    shape: &Shape,
    source: &str,
    element: &Element,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    let non_visual_names = ["p:nvSpPr"];
    let properties_names = ["p:spPr"];
    let style_names = ["p:style"];
    let text_names = ["p:txBody"];
    let mut edits = Vec::new();
    if changes.changed("nv") {
        if let Some(nv) = element.child("p:nvSpPr") {
            edits.push(ChildEdit {
                names: &non_visual_names,
                xml: Some(patched_shape_non_visual(shape, source, nv)),
            });
        }
    }
    if changes.any(&["xfrm", "geometry", "fill", "line"]) {
        let original = Some((source, element));
        let xml =
            shape_properties_xml(original, &properties(shape), &shape.extra, changes, context);
        edits.push(ChildEdit {
            names: &properties_names,
            xml: Some(xml),
        });
    }
    if changes.changed("style") {
        edits.push(ChildEdit {
            names: &style_names,
            xml: shape.style.as_ref().map(|style| write_style(style, "p")),
        });
    }
    if changes.changed("text") {
        let original_body = element.child("p:txBody");
        let body = shape
            .text
            .as_ref()
            .map(|body| text_body_xml(body, source, original_body, changes, context));
        edits.push(ChildEdit {
            names: &text_names,
            xml: body,
        });
    }
    patch_element(source, element, None, &edits, &SHAPE_ORDER)
}

fn patched_shape_non_visual(shape: &Shape, source: &str, nv: &Element) -> String {
    let kind = nv.child("p:cNvSpPr").map(|element| {
        let updates = [("txBox", shape.text_box.then(|| "1".to_string()))];
        (
            "p:cNvSpPr",
            retagged(source, element, "p:cNvSpPr", &updates),
        )
    });
    patched_non_visual(source, nv, &identity(shape), &shape.placeholder, kind)
}

fn text_body_xml(
    body: &TextBody,
    source: &str,
    original: Option<&Element>,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    match original {
        Some(original) => patched_text_body(body, source, original, changes, context),
        None => write_text_body(body, "p:txBody", context),
    }
}

/// a text body with its original start tag, `a:bodyPr` and `a:lstStyle`
/// kept when they did not change; paragraphs are reused one by one.
fn patched_text_body(
    body: &TextBody,
    source: &str,
    original: &Element,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    let raw = &source[original.start..original.end];
    let tag_end = start_tag_end(raw);
    let self_closing = raw[..tag_end].ends_with("/>");
    let mut out = if self_closing {
        format!("{}>", raw[..tag_end - 2].trim_end())
    } else {
        raw[..tag_end].to_string()
    };
    match original.child("a:bodyPr") {
        Some(element) if !changes.changed("body") => {
            out.push_str(&source[element.start..element.end])
        }
        _ => out.push_str(&write_body(&body.body)),
    }
    match original.child("a:lstStyle") {
        Some(element) if !changes.changed("lstStyle") => {
            out.push_str(&source[element.start..element.end])
        }
        _ => out.push_str(&write_text_style(&body.list_style, "a:lstStyle", context)),
    }
    for paragraph in &body.paragraphs {
        out.push_str(&write_paragraph(paragraph, context));
    }
    if body.paragraphs.is_empty() {
        out.push_str(EMPTY_PARAGRAPH);
    }
    match raw.rfind("</") {
        Some(close) if !self_closing => out.push_str(&raw[close..]),
        _ => out.push_str(&format!("</{}>", original.name)),
    }
    out
}
