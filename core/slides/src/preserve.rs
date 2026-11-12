//! putting original xml back, and rebuilding only what changed.
//!
//! the rule from letters-office applies: opening a deck, changing one
//! word and saving must not lose anything. on top of that, nothing may
//! move: every object that was not edited is written back as the exact
//! bytes it was read from. an edited object keeps the original xml of
//! every piece that was not edited (its geometry when only the text
//! changed, its text when it was only moved), so applications see the
//! same file apart from the change.

use crate::xml::{self, Element, Node};
use serde::Serialize;

use crate::model::{fingerprint_of, Source, SourceFormat};

/// a fresh source record for something just read.
pub fn source_of<T: Serialize>(
    format: SourceFormat,
    xml: &str,
    model: &T,
    pieces: Vec<(String, String)>,
) -> Source {
    Source {
        format,
        xml: xml.to_string(),
        fingerprint: fingerprint_of(model),
        pieces,
        part: None,
    }
}

/// a source record for a whole package part.
pub fn part_source_of<T: Serialize>(
    format: SourceFormat,
    part: &str,
    xml: &str,
    model: &T,
    pieces: Vec<(String, String)>,
) -> Source {
    let mut source = source_of(format, xml, model, pieces);
    source.part = Some(part.to_string());
    source
}

/// the original xml, when it is in `format` and `model` still has the
/// fingerprint it was read with.
pub fn reusable<'a, T: Serialize>(
    source: Option<&'a Source>,
    format: SourceFormat,
    model: &T,
) -> Option<&'a str> {
    let source = source?;
    (source.format == format && source.fingerprint == fingerprint_of(model))
        .then_some(source.xml.as_str())
}

/// true when the named piece of an object still has its read fingerprint.
pub fn piece_unchanged<T: Serialize>(source: &Source, name: &str, value: &T) -> bool {
    source.piece(name) == Some(fingerprint_of(value).as_str())
}

/// what to do with one kind of child while rebuilding an element.
pub struct ChildEdit<'a> {
    /// the element names this edit covers (a fill can be any of
    /// `a:noFill`, `a:solidFill`, `a:gradFill`...)
    pub names: &'a [&'a str],
    /// the new xml; `None` removes the child
    pub xml: Option<String>,
}

/// rebuilds `element` (whose text is `source[element.start..element.end]`)
/// replacing the children named by `edits` and copying every other byte.
/// a replacement whose child was not there is inserted before the first
/// child that comes later in `order` (the schema order of child names),
/// or at the end. `start_tag` replaces the opening tag when given.
pub fn patch_element(
    source: &str,
    element: &Element,
    start_tag: Option<&str>,
    edits: &[ChildEdit<'_>],
    order: &[&str],
) -> String {
    let raw = &source[element.start..element.end];
    let tag_end = start_tag_end(raw);
    let self_closing = raw[..tag_end].ends_with("/>");
    let opening = match start_tag {
        Some(tag) => tag.to_string(),
        None if self_closing => format!("{}>", raw[..tag_end - 2].trim_end()),
        None => raw[..tag_end].to_string(),
    };
    let closing = if self_closing {
        format!("</{}>", written_name(raw))
    } else {
        raw[raw.rfind("</").unwrap_or(raw.len())..].to_string()
    };
    let inner_start = element.start + tag_end;
    let inner_end = element.start + raw.len() - if self_closing { 0 } else { closing.len() };

    let mut applied = vec![false; edits.len()];
    let mut out = opening;
    let mut cursor = inner_start;
    let rank = |name: &str| order.iter().position(|known| *known == name);
    let mut last_rank: Option<usize> = None;
    for child in element.elements() {
        // text between children (indentation) is copied as it was
        out.push_str(&source[cursor.min(inner_end)..child.start.max(cursor).min(inner_end)]);
        cursor = child.end;
        let child_rank = rank(&child.name);
        if let Some(child_rank) = child_rank {
            insert_due(
                &mut out,
                edits,
                &mut applied,
                &rank,
                last_rank,
                Some(child_rank),
            );
            last_rank = Some(child_rank);
        }
        match edits
            .iter()
            .position(|edit| edit.names.contains(&child.name.as_str()))
        {
            Some(index) => {
                if !applied[index] {
                    applied[index] = true;
                    if let Some(new) = &edits[index].xml {
                        out.push_str(new);
                    }
                }
            }
            None => out.push_str(&source[child.start..child.end]),
        }
    }
    insert_due(&mut out, edits, &mut applied, &rank, last_rank, None);
    if cursor < inner_end {
        out.push_str(&source[cursor..inner_end]);
    }
    out.push_str(&closing);
    out
}

/// inserts the edits whose child was missing and whose place in the
/// schema is before `next` (or anywhere, when `next` is `None`).
fn insert_due(
    out: &mut String,
    edits: &[ChildEdit<'_>],
    applied: &mut [bool],
    rank: &dyn Fn(&str) -> Option<usize>,
    _previous: Option<usize>,
    next: Option<usize>,
) {
    for (index, edit) in edits.iter().enumerate() {
        if applied[index] || edit.xml.is_none() {
            continue;
        }
        let edit_rank = edit.names.iter().filter_map(|name| rank(name)).min();
        let due = match (edit_rank, next) {
            (_, None) => true,
            (Some(own), Some(next)) => own < next,
            (None, Some(_)) => false,
        };
        if due {
            applied[index] = true;
            out.push_str(edit.xml.as_deref().unwrap_or(""));
        }
    }
}

/// where the opening tag of an element's text ends (after its `>`),
/// skipping `>` inside quoted attribute values.
pub fn start_tag_end(raw: &str) -> usize {
    let mut quote: Option<u8> = None;
    for (index, byte) in raw.bytes().enumerate() {
        match (quote, byte) {
            (Some(open), b) if b == open => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(byte),
            (None, b'>') => return index + 1,
            _ => {}
        }
    }
    raw.len()
}

/// the element name as written in its tag.
fn written_name(raw: &str) -> &str {
    let rest = raw.trim_start_matches('<');
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(rest.len());
    &rest[..end]
}

/// an opening tag rebuilt with some attributes changed: values are
/// replaced where they were (keeping attribute order), `None` removes an
/// attribute, and new ones are added at the end.
pub fn rewrite_start_tag(
    raw_tag_name: &str,
    attributes: &[(String, String)],
    updates: &[(&str, Option<String>)],
    self_closing: bool,
) -> String {
    let mut out = format!("<{raw_tag_name}");
    let mut done = vec![false; updates.len()];
    for (key, value) in attributes {
        match updates.iter().position(|(name, _)| name == key) {
            Some(index) => {
                done[index] = true;
                if let Some(new) = &updates[index].1 {
                    push_attribute(&mut out, key, new);
                }
            }
            None => push_attribute(&mut out, key, value),
        }
    }
    for (index, (name, value)) in updates.iter().enumerate() {
        if !done[index] {
            if let Some(value) = value {
                push_attribute(&mut out, name, value);
            }
        }
    }
    out.push_str(if self_closing { "/>" } else { ">" });
    out
}

fn push_attribute(out: &mut String, key: &str, value: &str) {
    out.push(' ');
    out.push_str(key);
    out.push_str("=\"");
    out.push_str(&xml::escape_attribute(value));
    out.push('"');
}

/// the attributes of an element as written in its source (prefixes as
/// the file wrote them, values unescaped).
pub fn written_attributes(raw: &str) -> Vec<(String, String)> {
    let tag = &raw[..start_tag_end(raw)];
    let wrapped = if tag.ends_with("/>") {
        tag.to_string()
    } else {
        format!("{}/>", &tag[..tag.len() - 1])
    };
    match xml::parse(&wrapped) {
        Ok(document) => document.root_attributes,
        Err(_) => Vec::new(),
    }
}

/// parses a fragment kept as text (an object's original xml). prefixes
/// are not declared inside it, so names stay as the file wrote them,
/// which is what the original part used too.
pub fn parse_fragment(text: &str) -> Option<xml::XmlDocument> {
    xml::parse(text).ok()
}

/// the text of the children of `element` that are not in `known`, with
/// the name of the known child before each, for `RawChild` lists.
pub fn unknown_children(
    source: &str,
    element: &Element,
    known: &[&str],
) -> Vec<crate::model::RawChild> {
    let mut extra = Vec::new();
    let mut after: Option<&str> = None;
    for child in element.elements() {
        if known.contains(&child.name.as_str()) {
            after = Some(child.name.as_str());
        } else {
            extra.push(crate::model::RawChild::element(
                after,
                &source[child.start..child.end],
            ));
        }
    }
    extra
}

/// the attributes of `element` that are not in `known`, as raw children
/// whose xml is `name="value"` (they are told apart by not starting
/// with `<`).
pub fn unknown_attributes(element: &Element, known: &[&str]) -> Vec<crate::model::RawChild> {
    element
        .attributes
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()) && !key.starts_with("xmlns"))
        .map(|(key, value)| crate::model::RawChild {
            after: None,
            xml: format!("{key}=\"{}\"", xml::escape_attribute(value)),
        })
        .collect()
}

/// true for a raw child that is an attribute.
pub fn is_attribute(child: &crate::model::RawChild) -> bool {
    !child.xml.starts_with('<')
}

/// the text inside an element, decoded, with nothing else (for the few
/// elements whose text matters: `a:t`, `text:span`).
pub fn element_text(element: &Element) -> String {
    let mut text = String::new();
    for node in &element.children {
        if let Node::Text(part) = node {
            text.push_str(part);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patching_replaces_one_child_and_copies_the_rest() {
        let source = "<p:spPr>\n  <a:xfrm><a:off x=\"1\" y=\"2\"/></a:xfrm>\n  <a:prstGeom prst=\"rect\"/>\n  <a:effectLst odd = 'x'/>\n</p:spPr>";
        let document = xml::parse(source).unwrap();
        let edits = [ChildEdit {
            names: &["a:xfrm"],
            xml: Some("<a:xfrm><a:off x=\"5\" y=\"6\"/></a:xfrm>".into()),
        }];
        let patched = patch_element(source, &document.root, None, &edits, &[]);
        assert_eq!(
            patched,
            "<p:spPr>\n  <a:xfrm><a:off x=\"5\" y=\"6\"/></a:xfrm>\n  <a:prstGeom prst=\"rect\"/>\n  <a:effectLst odd = 'x'/>\n</p:spPr>"
        );
    }

    #[test]
    fn missing_children_go_in_schema_order() {
        let source = "<p:spPr><a:prstGeom prst=\"rect\"/><a:ln/></p:spPr>";
        let document = xml::parse(source).unwrap();
        let edits = [ChildEdit {
            names: &["a:solidFill", "a:noFill"],
            xml: Some("<a:noFill/>".into()),
        }];
        let order = ["a:xfrm", "a:prstGeom", "a:noFill", "a:solidFill", "a:ln"];
        let patched = patch_element(source, &document.root, None, &edits, &order);
        assert_eq!(
            patched,
            "<p:spPr><a:prstGeom prst=\"rect\"/><a:noFill/><a:ln/></p:spPr>"
        );
    }

    #[test]
    fn self_closing_elements_open_up() {
        let source = "<p:spPr/>";
        let document = xml::parse(source).unwrap();
        let edits = [ChildEdit {
            names: &["a:noFill"],
            xml: Some("<a:noFill/>".into()),
        }];
        assert_eq!(
            patch_element(source, &document.root, None, &edits, &[]),
            "<p:spPr><a:noFill/></p:spPr>"
        );
    }

    #[test]
    fn start_tags_keep_attribute_order() {
        let attributes = vec![
            ("rot".to_string(), "5".to_string()),
            ("flipH".to_string(), "1".to_string()),
        ];
        let tag = rewrite_start_tag(
            "a:xfrm",
            &attributes,
            &[("rot", Some("7".into())), ("flipV", Some("1".into()))],
            false,
        );
        assert_eq!(tag, "<a:xfrm rot=\"7\" flipH=\"1\" flipV=\"1\">");
        assert_eq!(start_tag_end("<a b=\">\">x</a>"), 9);
    }
}
