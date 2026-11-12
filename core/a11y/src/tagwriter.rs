//! adds to and changes the structure tree.
//!
//! two things must stay in step when content is tagged:
//! - the structure element lists the content among its kids, by page and
//!   marked-content id (mcid)
//! - the parent tree answers the reverse question, "which element owns
//!   mcid 3 on this page?": each page has a /StructParents key, and the
//!   parent tree maps that key to an array whose n-th entry is the element
//!   owning mcid n. annotations get their own key the same way.
//!
//! this writer keeps the parent tree in memory while a fix runs and writes
//! it back as one flat number tree when it finishes.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::A11yError;
use crate::numtree;
use crate::objects::{
    as_list, catalog_id, dict_mut, get, get_dict, get_integer, get_reference, name, resolve,
};

pub(crate) struct TagWriter<'d> {
    pub doc: &'d mut Document,
    pub root: ObjectId,
    parent_tree: BTreeMap<i64, Object>,
    next_key: i64,
}

impl<'d> TagWriter<'d> {
    /// opens the existing tree. the root is moved into its own object if
    /// it was written inline in the catalog, so it can be referred to.
    pub fn open(doc: &'d mut Document) -> Result<Self, A11yError> {
        let catalog = catalog_id(doc)?;
        let root = match doc
            .get_dictionary(catalog)
            .ok()
            .and_then(|c| c.get(b"StructTreeRoot").ok())
        {
            Some(Object::Reference(id)) if doc.get_dictionary(*id).is_ok() => *id,
            Some(Object::Dictionary(inline)) => {
                let inline = inline.clone();
                let id = doc.add_object(inline);
                dict_mut(doc, catalog)?.set("StructTreeRoot", Object::Reference(id));
                id
            }
            _ => return Err(A11yError::NotTagged),
        };
        let root_dict = doc
            .get_dictionary(root)
            .map_err(|e| A11yError::Internal(e.to_string()))?;
        let parent_tree = get_dict(doc, root_dict, b"ParentTree")
            .map(|tree| numtree::read(doc, tree))
            .unwrap_or_default();
        let stored_next = get_integer(doc, root_dict, b"ParentTreeNextKey").unwrap_or(0);
        let next_key = stored_next.max(parent_tree.keys().last().map_or(0, |last| last + 1));
        Ok(Self {
            doc,
            root,
            parent_tree,
            next_key,
        })
    }

    /// makes a new, empty tree, replacing any old one, and marks the
    /// document as tagged.
    pub fn create(doc: &'d mut Document) -> Result<Self, A11yError> {
        let catalog = catalog_id(doc)?;
        let mut root = Dictionary::new();
        root.set("Type", name("StructTreeRoot"));
        root.set("K", Object::Array(Vec::new()));
        // standard types only are written, so the role map starts empty;
        // it is there for the person to map custom types later
        root.set("RoleMap", Object::Dictionary(Dictionary::new()));
        let root = doc.add_object(root);
        let catalog_dict = dict_mut(doc, catalog)?;
        catalog_dict.set("StructTreeRoot", Object::Reference(root));
        let mut mark_info = Dictionary::new();
        mark_info.set("Marked", Object::Boolean(true));
        catalog_dict.set("MarkInfo", Object::Dictionary(mark_info));
        // old keys would point into the old tree
        for page in crate::objects::page_ids(doc) {
            if let Ok(page) = dict_mut(doc, page) {
                page.remove(b"StructParents");
            }
        }
        Ok(Self {
            doc,
            root,
            parent_tree: BTreeMap::new(),
            next_key: 0,
        })
    }

    /// a new element appended to `parent`'s kids.
    pub fn new_element(
        &mut self,
        role: &str,
        parent: ObjectId,
        page: Option<ObjectId>,
    ) -> ObjectId {
        let mut element = Dictionary::new();
        element.set("Type", name("StructElem"));
        element.set("S", name(role));
        element.set("P", Object::Reference(parent));
        if let Some(page) = page {
            element.set("Pg", Object::Reference(page));
        }
        element.set("K", Object::Array(Vec::new()));
        let id = self.doc.add_object(element);
        self.append_kid(parent, Object::Reference(id));
        id
    }

    /// adds a kid at the end of an element's (or the root's) /K.
    pub fn append_kid(&mut self, parent: ObjectId, kid: Object) {
        let mut kids = self.kids_of(parent);
        kids.push(kid);
        self.set_kids(parent, kids);
    }

    /// the kids of an element as a list, however they were written.
    pub fn kids_of(&self, element: ObjectId) -> Vec<Object> {
        let Ok(dict) = self.doc.get_dictionary(element) else {
            return Vec::new();
        };
        match dict.get(b"K").ok() {
            Some(Object::Reference(id)) => match self.doc.get_object(*id) {
                // an array stored as its own object
                Ok(Object::Array(items)) => items.clone(),
                _ => vec![Object::Reference(*id)],
            },
            other => as_list(other),
        }
    }

    pub fn set_kids(&mut self, element: ObjectId, kids: Vec<Object>) {
        if let Ok(dict) = dict_mut(self.doc, element) {
            dict.set("K", Object::Array(kids));
        }
    }

    /// the /StructParents key of a page, given one if it has none.
    pub fn page_key(&mut self, page: ObjectId) -> i64 {
        if let Ok(dict) = self.doc.get_dictionary(page) {
            if let Some(key) = get_integer(self.doc, dict, b"StructParents") {
                return key;
            }
        }
        let key = self.take_key();
        if let Ok(dict) = dict_mut(self.doc, page) {
            dict.set("StructParents", Object::Integer(key));
        }
        key
    }

    fn take_key(&mut self) -> i64 {
        let key = self.next_key;
        self.next_key += 1;
        key
    }

    /// records that `element` owns content `mcid` on `page`: in the
    /// element's kids and in the parent tree.
    pub fn add_content(&mut self, element: ObjectId, page: ObjectId, mcid: i64) {
        let element_page = self
            .doc
            .get_dictionary(element)
            .ok()
            .and_then(|dict| get_reference(dict, b"Pg"));
        let kid = if element_page == Some(page) {
            Object::Integer(mcid)
        } else {
            let mut reference = Dictionary::new();
            reference.set("Type", name("MCR"));
            reference.set("Pg", Object::Reference(page));
            reference.set("MCID", Object::Integer(mcid));
            Object::Dictionary(reference)
        };
        self.append_kid(element, kid);
        let key = self.page_key(page);
        let mut owners = self.page_owners(key);
        let slot = mcid.max(0) as usize;
        if owners.len() <= slot {
            owners.resize(slot + 1, Object::Null);
        }
        owners[slot] = Object::Reference(element);
        self.parent_tree.insert(key, Object::Array(owners));
    }

    /// the parent tree array of a page key, as a list.
    fn page_owners(&self, key: i64) -> Vec<Object> {
        match self.parent_tree.get(&key) {
            Some(value) => match resolve(self.doc, value) {
                Object::Array(items) => items.clone(),
                _ => Vec::new(),
            },
            None => Vec::new(),
        }
    }

    /// records that `element` owns a whole object, such as an annotation.
    pub fn add_object(&mut self, element: ObjectId, page: ObjectId, object: ObjectId) {
        let mut reference = Dictionary::new();
        reference.set("Type", name("OBJR"));
        reference.set("Pg", Object::Reference(page));
        reference.set("Obj", Object::Reference(object));
        self.append_kid(element, Object::Dictionary(reference));
        let existing = self
            .doc
            .get_dictionary(object)
            .ok()
            .and_then(|dict| get_integer(self.doc, dict, b"StructParent"));
        let key = existing.unwrap_or_else(|| self.take_key());
        if let Ok(dict) = dict_mut(self.doc, object) {
            dict.set("StructParent", Object::Integer(key));
        }
        self.parent_tree.insert(key, Object::Reference(element));
    }

    /// clears every parent tree entry that names one of `elements`, so
    /// nothing points at elements that were taken out.
    pub fn forget_owners(&mut self, elements: &[ObjectId]) {
        let named =
            |object: &Object| matches!(object, Object::Reference(id) if elements.contains(id));
        let keys: Vec<i64> = self.parent_tree.keys().copied().collect();
        for key in keys {
            let value = self.parent_tree[&key].clone();
            match resolve(self.doc, &value) {
                Object::Array(items) => {
                    let cleared: Vec<Object> = items
                        .iter()
                        .map(|item| {
                            if named(item) {
                                Object::Null
                            } else {
                                item.clone()
                            }
                        })
                        .collect();
                    self.parent_tree.insert(key, Object::Array(cleared));
                }
                _ if named(&value) => {
                    self.parent_tree.remove(&key);
                }
                _ => {}
            }
        }
    }

    /// the element new top-level content should go into: the single
    /// grouping element under the root (usually Document), or else the root.
    pub fn top_container(&mut self) -> ObjectId {
        let kids = self.kids_of(self.root);
        let elements: Vec<ObjectId> = kids
            .iter()
            .filter_map(|kid| kid.as_reference().ok())
            .collect();
        if let [only] = elements.as_slice() {
            let is_group = self
                .doc
                .get_dictionary(*only)
                .ok()
                .and_then(|dict| get(self.doc, dict, b"S"))
                .and_then(|s| s.as_name().ok())
                .is_some_and(|s| {
                    crate::structure::GROUPING_TYPES.contains(&String::from_utf8_lossy(s).as_ref())
                });
            if is_group {
                return *only;
            }
        }
        self.root
    }

    /// writes the parent tree back.
    pub fn finish(self) -> Result<(), A11yError> {
        let tree = numtree::flat(&self.parent_tree);
        let existing = self
            .doc
            .get_dictionary(self.root)
            .ok()
            .and_then(|root| get_reference(root, b"ParentTree"));
        // the old tree object is reused so nothing else is left pointing at it
        let tree = match existing {
            Some(id) => {
                self.doc.objects.insert(id, Object::Dictionary(tree));
                id
            }
            None => self.doc.add_object(tree),
        };
        let root = dict_mut(self.doc, self.root)?;
        root.set("ParentTree", Object::Reference(tree));
        root.set("ParentTreeNextKey", Object::Integer(self.next_key));
        Ok(())
    }
}
