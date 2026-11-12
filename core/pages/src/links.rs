//! where bookmarks and links lead.
//!
//! a "destination" names a page and how to show it (for example "page 4,
//! scrolled to the top"). it can be written out in full, or as a name
//! that is looked up in the document's table of named destinations. a
//! bookmark or link gets one either directly (/Dest) or through a "go to"
//! action (/A). this module reads all of those forms in one place.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};

/// a name tree is a few levels deep; this stops a looping one.
const MAX_NAME_TREE_DEPTH: usize = 32;

/// a page to go to and how to show it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Destination {
    pub page: ObjectId,
    /// the rest of the destination, such as `/XYZ 0 792 0` or `/Fit`
    pub view: Vec<Object>,
}

impl Destination {
    /// the destination written out in full, pointing at `page`.
    pub fn to_object(&self, page: ObjectId) -> Object {
        let mut array = vec![Object::Reference(page)];
        array.extend(self.view.iter().cloned());
        Object::Array(array)
    }
}

/// where something with a /Dest or /A entry leads.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Jump {
    /// it isn't a jump within this document (a web link, or nothing)
    NotInternal,
    /// a jump to a page of this document
    To(Destination),
    /// a jump within the document whose target can't be found
    Broken,
}

/// the document's named destinations, read once so that looking up many
/// links stays quick.
pub(crate) struct NamedDestinations {
    by_name: HashMap<Vec<u8>, Object>,
}

impl NamedDestinations {
    pub fn read(doc: &Document) -> Self {
        let mut by_name = HashMap::new();
        if let Ok(catalog) = doc.catalog() {
            // the older form: a plain dictionary in the catalog
            if let Some(dests) = dictionary_at(doc, catalog, b"Dests") {
                for (name, value) in dests.iter() {
                    by_name.insert(name.clone(), value.clone());
                }
            }
            // the newer form: a name tree under /Names
            if let Some(tree) =
                dictionary_at(doc, catalog, b"Names").and_then(|names| names.get(b"Dests").ok())
            {
                for (name, value) in name_tree_entries(doc, tree) {
                    by_name.insert(name, value);
                }
            }
        }
        Self { by_name }
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// what a destination object (written out, or a name) points at.
    pub fn resolve(&self, doc: &Document, dest: &Object) -> Option<Destination> {
        let (_, dest) = doc.dereference(dest).ok()?;
        match dest {
            Object::Array(array) => explicit_destination(doc, array),
            Object::Name(name) | Object::String(name, _) => {
                let named = self.by_name.get(name)?;
                let (_, named) = doc.dereference(named).ok()?;
                // a named destination is an array, or a dictionary holding one
                let array = match named {
                    Object::Dictionary(dict) => doc.dereference(dict.get(b"D").ok()?).ok()?.1,
                    other => other,
                };
                explicit_destination(doc, array.as_array().ok()?)
            }
            _ => None,
        }
    }

    /// where a bookmark or link annotation leads.
    pub fn jump_of(&self, doc: &Document, item: &Dictionary) -> Jump {
        let target = match item.get(b"Dest") {
            Ok(dest) => Some(dest),
            Err(_) => go_to_target(doc, item),
        };
        match target {
            None => Jump::NotInternal,
            Some(dest) => match self.resolve(doc, dest) {
                Some(destination) => Jump::To(destination),
                None => Jump::Broken,
            },
        }
    }

    /// every name with the page it leads to, for rewriting the table.
    pub fn entries(&self) -> impl Iterator<Item = (&Vec<u8>, &Object)> {
        self.by_name.iter()
    }
}

/// the target of a "go to" action, if the item has one.
fn go_to_target<'a>(doc: &'a Document, item: &'a Dictionary) -> Option<&'a Object> {
    let (_, action) = doc.dereference(item.get(b"A").ok()?).ok()?;
    let action = action.as_dict().ok()?;
    if action.get(b"S").and_then(Object::as_name).ok()? != b"GoTo" {
        return None;
    }
    action.get(b"D").ok()
}

/// reads `[page /View ...]`. the page must be a reference to an object
/// that exists; remote links name pages by number and don't count.
fn explicit_destination(doc: &Document, array: &[Object]) -> Option<Destination> {
    let page = array.first()?.as_reference().ok()?;
    doc.get_dictionary(page).ok()?;
    Some(Destination {
        page,
        view: array[1..].to_vec(),
    })
}

/// a dictionary stored under `key`, following a reference if needed.
pub(crate) fn dictionary_at<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Dictionary> {
    let (_, value) = doc.dereference(dict.get(key).ok()?).ok()?;
    value.as_dict().ok()
}

/// every (name, value) pair in a name tree, in tree order.
pub(crate) fn name_tree_entries(doc: &Document, root: &Object) -> Vec<(Vec<u8>, Object)> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut to_visit = vec![(root.clone(), 0)];
    while let Some((node, depth)) = to_visit.pop() {
        if depth > MAX_NAME_TREE_DEPTH {
            continue;
        }
        if let Object::Reference(id) = node {
            if !seen.insert(id) {
                continue;
            }
        }
        let Ok((_, node)) = doc.dereference(&node) else {
            continue;
        };
        let Ok(node) = node.as_dict() else {
            continue;
        };
        if let Some(pairs) = node
            .get(b"Names")
            .ok()
            .and_then(|names| doc.dereference(names).ok())
            .and_then(|(_, names)| names.as_array().ok())
        {
            // the leaf lists name, value, name, value...
            for [name, value] in pairs.as_chunks::<2>().0 {
                if let Ok(name) = name.as_str() {
                    entries.push((name.to_vec(), value.clone()));
                }
            }
        }
        if let Some(kids) = node
            .get(b"Kids")
            .ok()
            .and_then(|kids| doc.dereference(kids).ok())
            .and_then(|(_, kids)| kids.as_array().ok())
        {
            for kid in kids.iter().rev() {
                to_visit.push((kid.clone(), depth + 1));
            }
        }
    }
    entries
}

/// a pdf text string: plain when the text is plain ascii, otherwise
/// utf-16 with a byte order mark, which every reader understands.
pub(crate) fn text_string(text: &str) -> Object {
    if text.chars().all(|c| (' '..='~').contains(&c)) {
        return Object::string_literal(text.as_bytes().to_vec());
    }
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, lopdf::StringFormat::Hexadecimal)
}

/// reads a pdf text string back into text.
pub(crate) fn read_text_string(bytes: &[u8]) -> String {
    if let Some(utf16) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = utf16
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_be_bytes(pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    // close enough to pdf's own single byte encoding for titles
    bytes.iter().map(|&byte| byte as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_strings_round_trip() {
        for text in ["Chapter 1", "Café – über", "日本"] {
            let object = text_string(text);
            assert_eq!(read_text_string(object.as_str().unwrap()), text);
        }
        assert!(matches!(
            text_string("plain"),
            Object::String(_, lopdf::StringFormat::Literal)
        ));
    }
}
