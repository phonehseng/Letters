//! changes collected before they touch the document.
//!
//! every edit first works out all of its changes against a read-only
//! document, writing them into a draft. only when everything has worked
//! is the draft committed. so an edit that fails part way leaves the
//! document exactly as it was, without copying the whole file first.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Document, Object, ObjectId};

/// the objects an edit changes or adds, on top of a document it can't change.
pub(crate) struct Draft<'a> {
    pub doc: &'a Document,
    changed: BTreeMap<ObjectId, Object>,
    next_number: u32,
}

/// a finished draft, ready to be written into the document.
pub(crate) struct Changes {
    changed: BTreeMap<ObjectId, Object>,
    last_number: u32,
}

impl<'a> Draft<'a> {
    pub fn new(doc: &'a Document) -> Self {
        Self {
            doc,
            changed: BTreeMap::new(),
            next_number: doc.max_id + 1,
        }
    }

    /// an object as the draft sees it: changed, added, or as in the document.
    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        self.changed.get(&id).or_else(|| self.doc.objects.get(&id))
    }

    pub fn dictionary(&self, id: ObjectId) -> Option<&Dictionary> {
        match self.object(id)? {
            Object::Dictionary(dict) => Some(dict),
            Object::Stream(stream) => Some(&stream.dict),
            _ => None,
        }
    }

    /// follows a reference through the draft.
    pub fn resolve<'b>(&'b self, object: &'b Object) -> &'b Object {
        match object {
            Object::Reference(id) => self.object(*id).unwrap_or(object),
            other => other,
        }
    }

    pub fn set(&mut self, id: ObjectId, object: Object) {
        self.changed.insert(id, object);
    }

    pub fn add(&mut self, object: Object) -> ObjectId {
        let id = (self.next_number, 0);
        self.next_number += 1;
        self.changed.insert(id, object);
        id
    }

    pub fn finish(self) -> Changes {
        Changes {
            changed: self.changed,
            last_number: self.next_number - 1,
        }
    }
}

impl Changes {
    pub fn commit(self, doc: &mut Document) {
        for (id, object) in self.changed {
            doc.objects.insert(id, object);
        }
        doc.max_id = doc.max_id.max(self.last_number);
    }
}
