//! the structure tree: the outline of headings, paragraphs and figures
//! that screen readers use. its elements can carry text of their own that
//! stands for the content: /Alt (a description of a figure), /ActualText
//! (the real text of something drawn oddly) and /E (an abbreviation
//! spelled out). if the content an element points to lost something to
//! redaction, that text could still spell it out, so it is removed from
//! the element and from the elements above it.
//!
//! elements also point at annotations (OBJR). those pointers are removed
//! when the annotation is removed.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

use crate::objects::{as_dict, resolve};
use crate::redact::walker::TEXT_ALTERNATIVE_KEYS;

/// what to look for: marked content ids per page, and removed objects.
pub struct Touched<'a> {
    pub content: &'a HashSet<(ObjectId, i64)>,
    pub objects: &'a HashSet<ObjectId>,
}

/// changes the structure tree and returns how many elements lost text.
pub fn clean_structure(doc: &mut Document, touched: &Touched) -> usize {
    let Some(root) = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"StructTreeRoot").ok())
        .and_then(|root| root.as_reference().ok())
    else {
        return 0;
    };
    let mut findings = Findings::default();
    let mut seen = HashSet::new();
    visit(
        doc,
        root,
        None,
        &mut Vec::new(),
        touched,
        &mut findings,
        &mut seen,
    );
    for id in &findings.strip {
        if let Ok(dict) = doc.get_dictionary_mut(*id) {
            for key in TEXT_ALTERNATIVE_KEYS {
                dict.remove(key);
            }
        }
    }
    for id in &findings.drop_object_kids {
        remove_object_kids(doc, *id, touched.objects);
    }
    findings.strip.len()
}

#[derive(Default)]
struct Findings {
    /// elements whose text alternatives must go
    strip: HashSet<ObjectId>,
    /// elements with kids pointing at removed annotations
    drop_object_kids: HashSet<ObjectId>,
}

/// how deep the tree is followed. a looping tree is also stopped by `seen`.
const MAX_DEPTH: usize = 256;

fn visit(
    doc: &Document,
    id: ObjectId,
    inherited_page: Option<ObjectId>,
    ancestors: &mut Vec<ObjectId>,
    touched: &Touched,
    findings: &mut Findings,
    seen: &mut HashSet<ObjectId>,
) {
    if ancestors.len() > MAX_DEPTH || !seen.insert(id) {
        return;
    }
    let Ok(element) = doc.get_dictionary(id) else {
        return;
    };
    let page = element
        .get(b"Pg")
        .ok()
        .and_then(|p| p.as_reference().ok())
        .or(inherited_page);
    let kids: Vec<Object> = match element.get(b"K").ok().map(|k| resolve_kids(doc, k)) {
        Some(kids) => kids,
        None => return,
    };
    let mut hit = false;
    ancestors.push(id);
    for kid in &kids {
        match kid {
            Object::Integer(mcid) => {
                hit |= page.is_some_and(|page| touched.content.contains(&(page, *mcid)))
            }
            Object::Reference(child) => {
                if as_dict(resolve(doc, kid)).is_some_and(is_struct_element) {
                    visit(doc, *child, page, ancestors, touched, findings, seen);
                } else {
                    hit |= kid_is_hit(resolve(doc, kid), page, touched, id, findings);
                }
            }
            other => hit |= kid_is_hit(other, page, touched, id, findings),
        }
    }
    ancestors.pop();
    if hit {
        findings.strip.insert(id);
        // the elements above (excluding the root) may describe it too
        findings.strip.extend(ancestors.iter().skip(1).copied());
    }
}

fn resolve_kids(doc: &Document, kids: &Object) -> Vec<Object> {
    match resolve(doc, kids) {
        Object::Array(items) => items.clone(),
        // a single kid that is an element is kept as a reference so it
        // can be visited by id
        _ => vec![kids.clone()],
    }
}

fn is_struct_element(dict: &lopdf::Dictionary) -> bool {
    dict.has(b"S") || dict.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"StructElem")
}

/// a marked content reference (MCR) or object reference (OBJR) kid.
fn kid_is_hit(
    kid: &Object,
    page: Option<ObjectId>,
    touched: &Touched,
    owner: ObjectId,
    findings: &mut Findings,
) -> bool {
    let Some(dict) = as_dict(kid) else {
        return false;
    };
    let kid_page = dict
        .get(b"Pg")
        .ok()
        .and_then(|p| p.as_reference().ok())
        .or(page);
    if let Some(mcid) = dict.get(b"MCID").ok().and_then(|m| m.as_i64().ok()) {
        return kid_page.is_some_and(|page| touched.content.contains(&(page, mcid)));
    }
    let target = dict.get(b"Obj").ok().and_then(|o| o.as_reference().ok());
    if target.is_some_and(|target| touched.objects.contains(&target)) {
        findings.drop_object_kids.insert(owner);
        return true;
    }
    false
}

/// removes kids that point at removed objects.
fn remove_object_kids(doc: &mut Document, element: ObjectId, removed: &HashSet<ObjectId>) {
    let points_at_removed = |doc: &Document, kid: &Object| {
        as_dict(resolve(doc, kid))
            .and_then(|dict| dict.get(b"Obj").ok())
            .and_then(|o| o.as_reference().ok())
            .is_some_and(|target| removed.contains(&target))
    };
    let Ok(dict) = doc.get_dictionary(element) else {
        return;
    };
    let kept = match dict.get(b"K").ok().map(|k| resolve(doc, k)) {
        Some(Object::Array(kids)) => Object::Array(
            kids.iter()
                .filter(|kid| !points_at_removed(doc, kid))
                .cloned()
                .collect(),
        ),
        Some(kid) if points_at_removed(doc, kid) => Object::Array(Vec::new()),
        _ => return,
    };
    if let Ok(dict) = doc.get_dictionary_mut(element) {
        dict.set("K", kept);
    }
}
