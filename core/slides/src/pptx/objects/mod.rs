//! the objects of a shape tree (`p:spTree`), read and written; one file
//! per object kind.
//!
//! writing follows one rule: an object whose model still has the
//! fingerprint it was read with is its original xml, byte for byte. an
//! edited object is rebuilt from its original xml, replacing only the
//! pieces whose own fingerprints changed (see `pieces_of`).

mod common;
mod connector;
mod frame;
mod group;
mod picture;
mod shape;
mod table;

use crate::xml::{Element, XmlDocument};
use serde::Serialize;

use super::dml::{ReadContext, WriteContext};
use crate::model::{fingerprint_of, Object, Source, SourceFormat};
use crate::preserve::{parse_fragment, reusable, source_of};

/// the elements of a shape tree that are the tree's own properties, not objects
const TREE_PROPERTIES: [&str; 3] = ["p:nvGrpSpPr", "p:grpSpPr", "p:extLst"];

/// every object of a shape tree (or a group), back to front.
pub fn read_objects(tree: &Element, context: &ReadContext<'_>) -> Vec<Object> {
    tree.elements()
        .filter(|child| !TREE_PROPERTIES.contains(&child.name.as_str()))
        .map(|child| read_object(child, context))
        .collect()
}

fn read_object(element: &Element, context: &ReadContext<'_>) -> Object {
    let object = match element.name.as_str() {
        "p:sp" => Object::Shape(shape::read_shape(element, context)),
        "p:pic" => Object::Picture(picture::read_picture(element, context)),
        "p:grpSp" => Object::Group(group::read_group(element, context)),
        "p:cxnSp" => Object::Connector(connector::read_connector(element, context)),
        "p:graphicFrame" => Object::Frame(frame::read_frame(element, context)),
        // ink, unknown alternate content and the like stay exactly as written
        _ => Object::Preserved {
            xml: context.raw(element),
        },
    };
    remember(object, &context.raw(element))
}

/// records the original xml and the fingerprints of the object and its
/// pieces, as read.
fn remember(mut object: Object, xml: &str) -> Object {
    object.set_source(None);
    if matches!(object, Object::Preserved { .. }) {
        return object;
    }
    let pieces = pieces_of(&object);
    let source = source_of(SourceFormat::Pptx, xml, &object, pieces);
    object.set_source(Some(source));
    object
}

fn piece<T: Serialize>(name: &str, value: &T) -> (String, String) {
    (name.to_string(), fingerprint_of(value))
}

/// the fingerprints of the separately rebuildable pieces of an object.
fn pieces_of(object: &Object) -> Vec<(String, String)> {
    match object {
        Object::Shape(shape) => shape::pieces(shape),
        Object::Picture(picture) => picture::pieces(picture),
        Object::Group(group) => group::pieces(group),
        Object::Connector(connector) => connector::pieces(connector),
        Object::Frame(frame) => frame::pieces(frame),
        Object::Preserved { .. } => Vec::new(),
    }
}

/// which pieces of an edited object differ from what was read. without a
/// source every piece counts as changed, so the object is written whole.
pub(crate) struct Changes<'a> {
    source: Option<&'a Source>,
    current: Vec<(String, String)>,
}

impl Changes<'_> {
    pub fn changed(&self, name: &str) -> bool {
        let Some(source) = self.source else {
            return true;
        };
        let now = self
            .current
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str());
        source.piece(name) != now
    }

    pub fn any(&self, names: &[&str]) -> bool {
        names.iter().any(|name| self.changed(name))
    }
}

pub fn write_objects(objects: &[Object], context: &mut WriteContext<'_>) -> String {
    objects
        .iter()
        .map(|object| write_object(object, context))
        .collect()
}

pub fn write_object(object: &Object, context: &mut WriteContext<'_>) -> String {
    if let Object::Preserved { xml } = object {
        // only xml that came from a pptx file can go back into one
        let from_pptx = xml.starts_with("<p:") || xml.starts_with("<mc:");
        return if from_pptx {
            xml.clone()
        } else {
            String::new()
        };
    }
    let mut plain = object.clone();
    plain.set_source(None);
    if let Some(xml) = reusable(object.source(), SourceFormat::Pptx, &plain) {
        return xml.to_string();
    }
    let source = object
        .source()
        .filter(|source| source.format == SourceFormat::Pptx);
    let changes = Changes {
        source,
        current: pieces_of(&plain),
    };
    let original: Option<(&Source, XmlDocument)> =
        source.and_then(|source| parse_fragment(&source.xml).map(|document| (source, document)));
    write_changed(object, original, &changes, context)
}

fn write_changed(
    object: &Object,
    original: Option<(&Source, XmlDocument)>,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    let original = original
        .as_ref()
        .map(|(source, document)| (source.xml.as_str(), &document.root));
    match object {
        Object::Shape(item) => shape::write_shape(item, original, changes, context),
        Object::Picture(item) => picture::write_picture(item, original, changes, context),
        Object::Group(item) => group::write_group(item, original, changes, context),
        Object::Connector(item) => connector::write_connector(item, original, changes, context),
        Object::Frame(item) => frame::write_frame(item, original, changes, context),
        Object::Preserved { xml } => xml.clone(),
    }
}
