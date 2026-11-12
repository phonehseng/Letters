//! lines and connectors (`p:cxnSp`), with the objects their ends are
//! glued to (`a:stCxn`, `a:endCxn`).

use crate::xml::{escape_attribute, Element};

use super::super::dml::{attr, read_style, write_style, ReadContext, WriteContext};
use super::common::{
    fresh_shape_properties, identity_tag, patched_non_visual, patched_shape_properties,
    read_identity, read_property_extras, read_shape_properties, Identity, ShapeProperties,
};
use super::{piece, Changes};
use crate::model::{Connection, Connector};
use crate::preserve::{patch_element, written_attributes, ChildEdit};

const CONNECTOR_ORDER: [&str; 4] = ["p:nvCxnSpPr", "p:spPr", "p:style", "p:extLst"];

fn read_connection(element: Option<&Element>) -> Option<Connection> {
    let element = element?;
    Some(Connection {
        id: element.attr("id")?.parse().ok()?,
        site: element
            .attr("idx")
            .and_then(|i| i.parse().ok())
            .unwrap_or(0),
    })
}

pub fn read_connector(element: &Element, context: &ReadContext<'_>) -> Connector {
    let nv = element.child("p:nvCxnSpPr");
    let (id, name, description, hidden) = read_identity(nv);
    let glue = nv.and_then(|nv| nv.child("p:cNvCxnSpPr"));
    let properties_element = element.child("p:spPr");
    let properties = read_shape_properties(properties_element, context);
    Connector {
        id,
        name,
        description,
        hidden,
        transform: properties.transform,
        geometry: properties.geometry,
        line: properties.line,
        style: element.child("p:style").map(read_style),
        start: read_connection(glue.and_then(|g| g.child("a:stCxn"))),
        end: read_connection(glue.and_then(|g| g.child("a:endCxn"))),
        extra: read_property_extras(properties_element, context),
        source: None,
    }
}

pub fn pieces(connector: &Connector) -> Vec<(String, String)> {
    let identity = (
        connector.id,
        &connector.name,
        &connector.description,
        connector.hidden,
        &connector.start,
        &connector.end,
    );
    vec![
        piece("nv", &identity),
        piece("xfrm", &connector.transform),
        piece("geometry", &connector.geometry),
        piece("line", &connector.line),
        piece("style", &connector.style),
    ]
}

fn identity(connector: &Connector) -> Identity<'_> {
    Identity {
        id: connector.id,
        name: &connector.name,
        description: &connector.description,
        hidden: connector.hidden,
    }
}

fn properties(connector: &Connector) -> ShapeProperties {
    ShapeProperties {
        transform: connector.transform.clone(),
        geometry: connector.geometry.clone(),
        fill: None,
        line: connector.line.clone(),
    }
}

fn connection_tag(name: &str, connection: &Option<Connection>) -> String {
    connection
        .as_ref()
        .map(|c| format!("<a:{name}{}{}/>", attr("id", c.id), attr("idx", c.site)))
        .unwrap_or_default()
}

fn glue_tags(connector: &Connector) -> String {
    format!(
        "{}{}",
        connection_tag("stCxn", &connector.start),
        connection_tag("endCxn", &connector.end)
    )
}

pub fn write_connector(
    connector: &Connector,
    original: Option<(&str, &Element)>,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    match original {
        Some((source, element)) => patch_connector(connector, source, element, changes, context),
        None => fresh_connector(connector, context),
    }
}

fn fresh_connector(connector: &Connector, context: &mut WriteContext<'_>) -> String {
    let mut out = String::from("<p:cxnSp><p:nvCxnSpPr>");
    out.push_str(&identity_tag(&identity(connector)));
    let glue = glue_tags(connector);
    if glue.is_empty() {
        out.push_str("<p:cNvCxnSpPr/>");
    } else {
        out.push_str(&format!("<p:cNvCxnSpPr>{glue}</p:cNvCxnSpPr>"));
    }
    out.push_str("<p:nvPr/></p:nvCxnSpPr>");
    out.push_str(&fresh_shape_properties(
        &properties(connector),
        &connector.extra,
        context,
    ));
    if let Some(style) = &connector.style {
        out.push_str(&write_style(style, "p"));
    }
    out.push_str("</p:cxnSp>");
    out
}

/// `p:cNvCxnSpPr` with new glue and its locks kept.
fn patched_glue(connector: &Connector, source: &str, element: &Element) -> String {
    let raw = &source[element.start..element.end];
    let locks: String = element
        .elements()
        .filter(|e| e.name != "a:stCxn" && e.name != "a:endCxn")
        .map(|e| source[e.start..e.end].to_string())
        .collect();
    let attributes: String = written_attributes(raw)
        .iter()
        .map(|(key, value)| format!(" {key}=\"{}\"", escape_attribute(value)))
        .collect();
    format!(
        "<p:cNvCxnSpPr{attributes}>{locks}{}</p:cNvCxnSpPr>",
        glue_tags(connector)
    )
}

fn patch_connector(
    connector: &Connector,
    source: &str,
    element: &Element,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    let non_visual_names = ["p:nvCxnSpPr"];
    let properties_names = ["p:spPr"];
    let style_names = ["p:style"];
    let mut edits = Vec::new();
    if changes.changed("nv") {
        if let Some(nv) = element.child("p:nvCxnSpPr") {
            let kind = nv
                .child("p:cNvCxnSpPr")
                .map(|glue| ("p:cNvCxnSpPr", patched_glue(connector, source, glue)));
            let xml = patched_non_visual(source, nv, &identity(connector), &None, kind);
            edits.push(ChildEdit {
                names: &non_visual_names,
                xml: Some(xml),
            });
        }
    }
    if changes.any(&["xfrm", "geometry", "line"]) {
        if let Some(properties_element) = element.child("p:spPr") {
            let xml = patched_shape_properties(
                source,
                properties_element,
                &properties(connector),
                changes,
                context,
            );
            edits.push(ChildEdit {
                names: &properties_names,
                xml: Some(xml),
            });
        }
    }
    if changes.changed("style") {
        edits.push(ChildEdit {
            names: &style_names,
            xml: connector.style.as_ref().map(|s| write_style(s, "p")),
        });
    }
    patch_element(source, element, None, &edits, &CONNECTOR_ORDER)
}
