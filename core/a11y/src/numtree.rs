//! number trees: pdf's sorted maps from whole numbers to values, used by
//! the parent tree (which tag owns which piece of content) and by page
//! labels.
//!
//! a number tree is either one node with a flat /Nums list or nodes with
//! /Kids. this reads either kind and always writes the flat kind, which
//! every reader understands.

use std::collections::{BTreeMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::objects::{get_array, resolve};

/// more nodes than this and the rest of the tree is ignored.
const NODE_LIMIT: usize = 100_000;

/// every entry of a number tree, sorted by key.
pub fn read(doc: &Document, root: &Dictionary) -> BTreeMap<i64, Object> {
    let mut entries = BTreeMap::new();
    let mut seen: HashSet<ObjectId> = HashSet::new();
    let mut stack: Vec<&Dictionary> = vec![root];
    let mut visited = 0;
    while let Some(node) = stack.pop() {
        visited += 1;
        if visited > NODE_LIMIT {
            break;
        }
        if let Some(nums) = get_array(doc, node, b"Nums") {
            for pair in nums.chunks(2) {
                if let [key, value] = pair {
                    if let Ok(key) = resolve(doc, key).as_i64() {
                        entries.insert(key, value.clone());
                    }
                }
            }
        }
        for kid in get_array(doc, node, b"Kids").unwrap_or_default() {
            if let Ok(id) = kid.as_reference() {
                if !seen.insert(id) {
                    continue;
                }
            }
            if let Ok(dict) = resolve(doc, kid).as_dict() {
                stack.push(dict);
            }
        }
    }
    entries
}

/// a flat number tree node holding the entries.
pub fn flat(entries: &BTreeMap<i64, Object>) -> Dictionary {
    let mut nums = Vec::with_capacity(entries.len() * 2);
    for (key, value) in entries {
        nums.push(Object::Integer(*key));
        nums.push(value.clone());
    }
    let mut dict = Dictionary::new();
    dict.set("Nums", Object::Array(nums));
    dict
}
