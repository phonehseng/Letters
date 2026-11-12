//! a list of changes worked out in full before any of them is made.
//!
//! every operation promises to either succeed or leave the document as it
//! was. the simplest way to keep that promise is to do all the reading and
//! checking first, collect what should change in one of these, and only
//! then apply it. applying is just inserting objects and setting keys, so
//! it can't fail halfway.

use lopdf::{Document, Object, ObjectId};

pub(crate) struct Changes {
    next_number: u32,
    new_objects: Vec<(ObjectId, Object)>,
    key_updates: Vec<(ObjectId, &'static str, Option<Object>)>,
}

impl Changes {
    pub fn new(doc: &Document) -> Self {
        Self {
            next_number: doc.max_id + 1,
            new_objects: Vec::new(),
            key_updates: Vec::new(),
        }
    }

    /// reserves an id for a new object that will be added on `apply`.
    pub fn add(&mut self, object: impl Into<Object>) -> ObjectId {
        let id = (self.next_number, 0);
        self.next_number += 1;
        self.new_objects.push((id, object.into()));
        id
    }

    /// sets a key on an existing dictionary (or stream dictionary).
    pub fn set(&mut self, target: ObjectId, key: &'static str, value: impl Into<Object>) {
        self.key_updates.push((target, key, Some(value.into())));
    }

    /// removes a key from an existing dictionary.
    pub fn remove(&mut self, target: ObjectId, key: &'static str) {
        self.key_updates.push((target, key, None));
    }

    pub fn apply(self, doc: &mut Document) {
        doc.max_id = doc.max_id.max(self.next_number - 1);
        for (id, object) in self.new_objects {
            doc.objects.insert(id, object);
        }
        for (target, key, value) in self.key_updates {
            let dict = match doc.objects.get_mut(&target) {
                Some(Object::Dictionary(dict)) => dict,
                Some(Object::Stream(stream)) => &mut stream.dict,
                _ => continue,
            };
            match value {
                Some(value) => dict.set(key, value),
                None => {
                    dict.remove(key.as_bytes());
                }
            }
        }
    }
}
