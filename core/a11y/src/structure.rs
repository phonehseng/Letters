//! reads the structure tree: the tags of a tagged pdf.
//!
//! the tree starts at the catalog's /StructTreeRoot. each structure
//! element has a type (/S: P, H1, Figure, Table…) and kids, which are
//! other elements, pieces of page content (named by marked-content id) or
//! whole objects such as link annotations. custom types are mapped to the
//! standard ones through the /RoleMap.
//!
//! damaged files can have loops (an element that is its own ancestor) or
//! references to objects that don't exist. the reader visits every object
//! at most once and keeps going past missing ones, noting each problem,
//! so it always finishes.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::objects::{get, get_dict, get_integer, get_name, get_reference, get_text, name_text};

/// more elements than this and the rest of the tree is not read; real
/// documents have far fewer, so this only stops runaway files.
const ELEMENT_LIMIT: usize = 200_000;

/// the standard structure types of pdf 1.7 and 2.0.
pub const STANDARD_TYPES: &[&str] = &[
    "Document",
    "DocumentFragment",
    "Part",
    "Art",
    "Sect",
    "Div",
    "Aside",
    "BlockQuote",
    "Caption",
    "TOC",
    "TOCI",
    "Index",
    "NonStruct",
    "Private",
    "Title",
    "P",
    "H",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "L",
    "LI",
    "Lbl",
    "LBody",
    "Table",
    "TR",
    "TH",
    "TD",
    "THead",
    "TBody",
    "TFoot",
    "Span",
    "Quote",
    "Note",
    "FENote",
    "Reference",
    "BibEntry",
    "Code",
    "Link",
    "Annot",
    "Ruby",
    "RB",
    "RT",
    "RP",
    "Warichu",
    "WT",
    "WP",
    "Figure",
    "Formula",
    "Form",
    "Sub",
    "Em",
    "Strong",
    "Artifact",
];

/// types that only group other elements; reading order is judged between
/// the blocks inside them.
pub const GROUPING_TYPES: &[&str] = &[
    "Document",
    "DocumentFragment",
    "Part",
    "Art",
    "Sect",
    "Div",
    "NonStruct",
    "Private",
    "Aside",
];

/// one kid of a structure element.
#[derive(Clone, Debug, PartialEq)]
pub enum Kid {
    Element(usize),
    /// a piece of page content, named by its marked-content id
    Content {
        page: Option<ObjectId>,
        mcid: i64,
        /// inside a form xobject rather than the page itself
        in_stream: bool,
    },
    /// a whole object, such as an annotation
    Object {
        page: Option<ObjectId>,
        object: ObjectId,
    },
}

/// one structure element.
#[derive(Clone, Debug)]
pub struct Element {
    /// the object holding it; none for an element written inline
    pub id: Option<ObjectId>,
    /// the type as written, which may be a custom name
    pub raw_type: String,
    /// the standard type it maps to, or the raw type when it maps to none
    pub role: String,
    pub parent: Option<usize>,
    pub kids: Vec<Kid>,
    pub page: Option<ObjectId>,
    pub alt: Option<String>,
    pub actual_text: Option<String>,
    pub lang: Option<String>,
    pub scope: Option<String>,
    pub has_headers: bool,
    pub row_span: u32,
    pub col_span: u32,
}

impl Element {
    pub fn is_heading(&self) -> Option<u8> {
        let level = self.role.strip_prefix('H')?;
        match level.parse::<u8>() {
            Ok(level @ 1..=6) => Some(level),
            _ => None,
        }
    }

    /// alternative text or replacement text that isn't empty.
    pub fn description(&self) -> Option<&str> {
        [self.alt.as_deref(), self.actual_text.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .find(|text| !text.is_empty())
    }
}

/// the whole tree, read once.
#[derive(Clone, Debug, Default)]
pub struct StructTree {
    pub root: Option<ObjectId>,
    pub elements: Vec<Element>,
    /// elements directly under the root, in order
    pub top: Vec<usize>,
    pub role_map: HashMap<String, String>,
    /// loops, missing objects and other damage, worded for people
    pub problems: Vec<String>,
}

impl StructTree {
    /// reads the tree, or answers none when the document has no tree.
    pub fn read(doc: &Document) -> Option<StructTree> {
        let catalog = doc.catalog().ok()?;
        let root_dict = get_dict(doc, catalog, b"StructTreeRoot")?;
        let mut tree = StructTree {
            root: get_reference(catalog, b"StructTreeRoot"),
            role_map: read_role_map(doc, root_dict),
            ..StructTree::default()
        };
        let mut reader = TreeReader {
            doc,
            seen: HashSet::new(),
            pending: Vec::new(),
        };
        if let Some(root) = tree.root {
            reader.seen.insert(root);
        }
        let top_kids = reader.read_kids(&mut tree, root_dict, None, None);
        tree.top = top_kids
            .into_iter()
            .filter_map(|kid| match kid {
                Kid::Element(index) => Some(index),
                _ => None,
            })
            .collect();
        // breadth first, with an explicit list, so a very deep tree can't
        // overflow the stack
        while let Some(index) = reader.pending.pop() {
            if tree.elements.len() > ELEMENT_LIMIT {
                tree.problems
                    .push("The tags are too large to read completely.".into());
                break;
            }
            let Some(dict) = reader.element_dict(&tree.elements[index]) else {
                continue;
            };
            let page = tree.elements[index].page;
            let kids = reader.read_kids(&mut tree, dict, Some(index), page);
            tree.elements[index].kids = kids;
        }
        Some(tree)
    }

    /// every element in reading order: each element before its kids.
    pub fn preorder(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.elements.len());
        let mut stack: Vec<usize> = self.top.iter().rev().copied().collect();
        let mut visited = vec![false; self.elements.len()];
        while let Some(index) = stack.pop() {
            if std::mem::replace(&mut visited[index], true) {
                continue;
            }
            order.push(index);
            for kid in self.elements[index].kids.iter().rev() {
                if let Kid::Element(child) = kid {
                    stack.push(*child);
                }
            }
        }
        order
    }

    pub fn children(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        self.elements[index]
            .kids
            .iter()
            .filter_map(|kid| match kid {
                Kid::Element(child) => Some(*child),
                _ => None,
            })
    }

    /// the pieces of page content under an element and all its
    /// descendants, in order: (page, mcid).
    pub fn content_under(&self, index: usize) -> Vec<(Option<ObjectId>, i64)> {
        let mut found = Vec::new();
        let mut stack = vec![index];
        let mut visited = HashSet::new();
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            let mut children = Vec::new();
            for kid in &self.elements[current].kids {
                match kid {
                    Kid::Content {
                        page,
                        mcid,
                        in_stream: false,
                    } => found.push((*page, *mcid)),
                    Kid::Element(child) => children.push(*child),
                    _ => {}
                }
            }
            stack.extend(children.into_iter().rev());
        }
        found
    }

    /// which element owns each piece of page content.
    pub fn content_owners(&self) -> HashMap<(ObjectId, i64), usize> {
        let mut owners = HashMap::new();
        for (index, element) in self.elements.iter().enumerate() {
            for kid in &element.kids {
                if let Kid::Content {
                    page: Some(page),
                    mcid,
                    in_stream: false,
                } = kid
                {
                    owners.entry((*page, *mcid)).or_insert(index);
                }
            }
        }
        owners
    }

    /// which element owns each whole object (annotations).
    pub fn object_owners(&self) -> HashMap<ObjectId, usize> {
        let mut owners = HashMap::new();
        for (index, element) in self.elements.iter().enumerate() {
            for kid in &element.kids {
                if let Kid::Object { object, .. } = kid {
                    owners.entry(*object).or_insert(index);
                }
            }
        }
        owners
    }

    /// the element and its ancestors, nearest first.
    pub fn ancestors(&self, index: usize) -> Vec<usize> {
        let mut chain = vec![index];
        let mut current = index;
        while let Some(parent) = self.elements[current].parent {
            if chain.contains(&parent) {
                break;
            }
            chain.push(parent);
            current = parent;
        }
        chain
    }

    /// the language that applies to an element: its own or the nearest
    /// ancestor's.
    pub fn language_of(&self, index: usize) -> Option<&str> {
        self.ancestors(index)
            .into_iter()
            .find_map(|i| self.elements[i].lang.as_deref())
    }

    /// a path people can read, like "Document › Sect 2 › Figure 1": each
    /// step counts elements of the same type among its siblings.
    pub fn path(&self, index: usize) -> String {
        let mut steps: Vec<String> = self
            .ancestors(index)
            .into_iter()
            .map(|i| self.step_name(i))
            .collect();
        steps.reverse();
        steps.join(" › ")
    }

    fn step_name(&self, index: usize) -> String {
        let element = &self.elements[index];
        let siblings: Vec<usize> = match element.parent {
            Some(parent) => self.children(parent).collect(),
            None => self.top.clone(),
        };
        let same_type: Vec<usize> = siblings
            .into_iter()
            .filter(|&s| self.elements[s].raw_type == element.raw_type)
            .collect();
        if same_type.len() <= 1 {
            element.raw_type.clone()
        } else {
            let position = same_type.iter().position(|&s| s == index).unwrap_or(0);
            format!("{} {}", element.raw_type, position + 1)
        }
    }

    /// the block an element belongs to for reading order: its highest
    /// ancestor that is not a grouping element.
    pub fn block_of(&self, index: usize) -> usize {
        let mut block = index;
        for ancestor in self.ancestors(index) {
            if GROUPING_TYPES.contains(&self.elements[ancestor].role.as_str()) {
                break;
            }
            block = ancestor;
        }
        block
    }

    pub fn find(&self, id: ObjectId) -> Option<usize> {
        self.elements
            .iter()
            .position(|element| element.id == Some(id))
    }
}

struct TreeReader<'a> {
    doc: &'a Document,
    seen: HashSet<ObjectId>,
    /// elements whose kids have not been read yet
    pending: Vec<usize>,
}

impl<'a> TreeReader<'a> {
    fn element_dict(&self, element: &Element) -> Option<&'a Dictionary> {
        // inline elements are re-read through their parent; only indirect
        // ones are looked up here
        self.doc.get_dictionary(element.id?).ok()
    }

    /// reads the /K entry of a dictionary into kids, creating elements as
    /// they are found.
    fn read_kids(
        &mut self,
        tree: &mut StructTree,
        dict: &'a Dictionary,
        parent: Option<usize>,
        page: Option<ObjectId>,
    ) -> Vec<Kid> {
        let Ok(value) = dict.get(b"K") else {
            return Vec::new();
        };
        let items: Vec<&'a Object> = match self.follow(tree, value) {
            Some(Object::Array(items)) => items.iter().collect(),
            Some(_) => vec![value],
            None => Vec::new(),
        };
        items
            .into_iter()
            .filter_map(|item| self.read_kid(tree, item, parent, page))
            .collect()
    }

    /// follows a reference, noting a missing object.
    fn follow(&self, tree: &mut StructTree, value: &'a Object) -> Option<&'a Object> {
        match value {
            Object::Reference(id) => match self.doc.get_object(*id) {
                Ok(object) => Some(object),
                Err(_) => {
                    tree.problems.push(format!(
                        "The tags point to object {} {}, which doesn’t exist.",
                        id.0, id.1
                    ));
                    None
                }
            },
            other => Some(other),
        }
    }

    fn read_kid(
        &mut self,
        tree: &mut StructTree,
        item: &'a Object,
        parent: Option<usize>,
        page: Option<ObjectId>,
    ) -> Option<Kid> {
        let reference = item.as_reference().ok();
        let resolved = self.follow(tree, item)?;
        match resolved {
            Object::Integer(mcid) => Some(Kid::Content {
                page,
                mcid: *mcid,
                in_stream: false,
            }),
            Object::Dictionary(dict) => {
                let kind = get_name(self.doc, dict, b"Type");
                let own_page = get_reference(dict, b"Pg").or(page);
                match kind {
                    Some(b"MCR") => Some(Kid::Content {
                        page: own_page,
                        mcid: get_integer(self.doc, dict, b"MCID")?,
                        in_stream: dict.has(b"Stm"),
                    }),
                    Some(b"OBJR") => Some(Kid::Object {
                        page: own_page,
                        object: get_reference(dict, b"Obj")?,
                    }),
                    _ if dict.has(b"S") => {
                        self.new_element(tree, reference, dict, parent, own_page)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn new_element(
        &mut self,
        tree: &mut StructTree,
        reference: Option<ObjectId>,
        dict: &'a Dictionary,
        parent: Option<usize>,
        page: Option<ObjectId>,
    ) -> Option<Kid> {
        if let Some(id) = reference {
            if !self.seen.insert(id) {
                tree.problems.push(format!(
                    "The tags loop back on themselves at object {} {}.",
                    id.0, id.1
                ));
                return None;
            }
        }
        if tree.elements.len() >= ELEMENT_LIMIT {
            return None;
        }
        let doc = self.doc;
        let raw_type = get_name(doc, dict, b"S").map(name_text).unwrap_or_default();
        let role = map_role(&tree.role_map, &raw_type);
        let table = table_attributes(doc, dict);
        tree.elements.push(Element {
            id: reference,
            raw_type,
            role,
            parent,
            kids: Vec::new(),
            page,
            alt: get_text(doc, dict, b"Alt"),
            actual_text: get_text(doc, dict, b"ActualText"),
            lang: get_text(doc, dict, b"Lang"),
            scope: table.scope,
            has_headers: table.has_headers,
            row_span: table.row_span,
            col_span: table.col_span,
        });
        let index = tree.elements.len() - 1;
        if reference.is_some() {
            self.pending.push(index);
        } else {
            // an inline element: its kids are read right away, since it
            // can't be looked up again later
            let kids = self.read_kids(tree, dict, Some(index), page);
            tree.elements[index].kids = kids;
        }
        Some(Kid::Element(index))
    }
}

fn read_role_map(doc: &Document, root: &Dictionary) -> HashMap<String, String> {
    get_dict(doc, root, b"RoleMap")
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    let target = crate::objects::resolve(doc, value).as_name().ok()?;
                    Some((name_text(key), name_text(target)))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// follows the role map until a standard type is reached. a chain that
/// loops or runs past ten steps stays at its own name.
pub fn map_role(role_map: &HashMap<String, String>, raw: &str) -> String {
    let mut current = raw.to_owned();
    for _ in 0..10 {
        if STANDARD_TYPES.contains(&current.as_str()) {
            return current;
        }
        match role_map.get(&current) {
            Some(next) if *next != current => current = next.clone(),
            _ => break,
        }
    }
    if STANDARD_TYPES.contains(&current.as_str()) {
        current
    } else {
        raw.to_owned()
    }
}

#[derive(Default)]
struct TableAttributes {
    scope: Option<String>,
    has_headers: bool,
    row_span: u32,
    col_span: u32,
}

/// the table attributes of an element (/A with /O /Table).
fn table_attributes(doc: &Document, dict: &Dictionary) -> TableAttributes {
    let mut found = TableAttributes {
        row_span: 1,
        col_span: 1,
        ..TableAttributes::default()
    };
    let lists: Vec<&Dictionary> = match get(doc, dict, b"A") {
        Some(Object::Dictionary(single)) => vec![single],
        Some(Object::Array(items)) => items
            .iter()
            .filter_map(|item| crate::objects::resolve(doc, item).as_dict().ok())
            .collect(),
        _ => Vec::new(),
    };
    for attributes in lists {
        if get_name(doc, attributes, b"O") != Some(b"Table") {
            continue;
        }
        if let Some(scope) = get_name(doc, attributes, b"Scope") {
            found.scope = Some(name_text(scope));
        }
        if attributes.has(b"Headers") {
            found.has_headers = true;
        }
        let span = |key: &[u8]| get_integer(doc, attributes, key).map(|n| n.clamp(1, 1000) as u32);
        if let Some(rows) = span(b"RowSpan") {
            found.row_span = rows;
        }
        if let Some(columns) = span(b"ColSpan") {
            found.col_span = columns;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_types_follow_the_role_map() {
        let map: HashMap<String, String> = [
            ("Heading1".to_owned(), "H1".to_owned()),
            ("Body".to_owned(), "Para".to_owned()),
            ("Para".to_owned(), "P".to_owned()),
        ]
        .into_iter()
        .collect();
        assert_eq!(map_role(&map, "Heading1"), "H1");
        assert_eq!(map_role(&map, "Body"), "P");
        assert_eq!(map_role(&map, "Mystery"), "Mystery");
    }

    #[test]
    fn a_looping_role_map_stops() {
        let map: HashMap<String, String> = [
            ("A".to_owned(), "B".to_owned()),
            ("B".to_owned(), "A".to_owned()),
        ]
        .into_iter()
        .collect();
        assert_eq!(map_role(&map, "A"), "A");
    }
}
