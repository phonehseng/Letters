//! damaged files: loops, missing objects, nonsense content. the checker
//! and the fixes must never panic or hang on them.

mod support;

use std::time::{Duration, Instant};

use letters_a11y::{check, RuleStatus};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use support::*;

fn element(role: &str, parent: ObjectId, kids: Vec<Object>) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", name("StructElem"));
    dict.set("S", name(role));
    dict.set("P", Object::Reference(parent));
    dict.set("K", Object::Array(kids));
    dict
}

/// a document whose tag tree is whatever `build` makes under the root.
fn with_tree(build: impl FnOnce(&mut Document, ObjectId, ObjectId) -> Object) -> Document {
    let mut builder = Builder::new();
    builder.page(&marked("P", 0, &text("F1", 12.0, 72.0, 100.0, "Text")));
    let page = builder.pages[0];
    let mut doc = builder.build();
    let root = doc.new_object_id();
    let kids = build(&mut doc, root, page);
    let mut root_dict = Dictionary::new();
    root_dict.set("Type", name("StructTreeRoot"));
    root_dict.set("K", kids);
    doc.objects.insert(root, Object::Dictionary(root_dict));
    let catalog = doc.catalog_mut().unwrap();
    catalog.set("StructTreeRoot", Object::Reference(root));
    let mut info = Dictionary::new();
    info.set("Marked", Object::Boolean(true));
    catalog.set("MarkInfo", Object::Dictionary(info));
    add_metadata(&mut doc, "en", "Damaged");
    doc
}

/// runs the checker and the heading and reading order fixes, quickly.
fn survives(doc: Document) -> letters_a11y::Report {
    let started = Instant::now();
    let report = check(&doc, None).unwrap();
    let mut copy = doc.clone();
    let _ = letters_a11y::fix_heading_levels(&mut copy);
    let _ = letters_a11y::auto_reading_order(&doc, 0);
    let _ = letters_a11y::add_bookmarks_from_headings(&mut copy);
    assert!(started.elapsed() < Duration::from_secs(10), "took too long");
    report
}

#[test]
fn a_loop_in_the_tags_is_reported_not_followed_forever() {
    let doc = with_tree(|doc, root, page| {
        let a = doc.new_object_id();
        let b = doc.new_object_id();
        let mut first = element("Sect", root, vec![Object::Reference(b)]);
        first.set("Pg", Object::Reference(page));
        doc.objects.insert(a, Object::Dictionary(first));
        // b holds a again: a loop
        doc.objects.insert(
            b,
            Object::Dictionary(element(
                "H1",
                a,
                vec![Object::Integer(0), Object::Reference(a)],
            )),
        );
        Object::Reference(a)
    });
    let report = survives(doc);
    assert!(report.has("structure-tree-damaged"));
    assert!(report
        .with_id("structure-tree-damaged")
        .next()
        .unwrap()
        .details
        .contains("loop"));
}

#[test]
fn an_element_that_is_its_own_kid_is_survived() {
    let doc = with_tree(|doc, root, _| {
        let a = doc.new_object_id();
        doc.objects.insert(
            a,
            Object::Dictionary(element("Div", root, vec![Object::Reference(a)])),
        );
        Object::Array(vec![Object::Reference(a), Object::Reference(a)])
    });
    survives(doc);
}

#[test]
fn references_to_missing_objects_are_reported() {
    let doc = with_tree(|doc, root, page| {
        let paragraph = doc.new_object_id();
        let mut dict = element(
            "P",
            root,
            vec![Object::Integer(0), Object::Reference((9998, 0))],
        );
        dict.set("Pg", Object::Reference(page));
        doc.objects.insert(paragraph, Object::Dictionary(dict));
        Object::Array(vec![
            Object::Reference(paragraph),
            Object::Reference((9999, 0)),
        ])
    });
    let report = survives(doc);
    let finding = report.with_id("structure-tree-damaged").next().unwrap();
    assert!(finding.details.contains("doesn’t exist"));
    // the part that could be read is still checked
    assert!(!report.has("untagged-content"));

    // a tree with nothing readable at all counts as no tags
    let empty = with_tree(|_, _, _| Object::Array(vec![Object::Reference((9999, 0))]));
    assert!(survives(empty).has("not-tagged"));
}

#[test]
fn the_root_pointing_at_itself_is_survived() {
    let doc = with_tree(|_, root, _| Object::Reference(root));
    survives(doc);
}

#[test]
fn a_very_deep_tree_does_not_overflow_the_stack() {
    let doc = with_tree(|doc, root, page| {
        let mut parent = root;
        let mut first = None;
        let mut previous: Option<ObjectId> = None;
        for depth in 0..5000 {
            let id = doc.new_object_id();
            let kids = if depth == 4999 {
                vec![Object::Integer(0)]
            } else {
                Vec::new()
            };
            let mut dict = element(if depth == 4999 { "P" } else { "Div" }, parent, kids);
            dict.set("Pg", Object::Reference(page));
            doc.objects.insert(id, Object::Dictionary(dict));
            if let Some(previous) = previous {
                doc.get_dictionary_mut(previous)
                    .unwrap()
                    .set("K", Object::Array(vec![Object::Reference(id)]));
            }
            first.get_or_insert(id);
            previous = Some(id);
            parent = id;
        }
        Object::Reference(first.unwrap())
    });
    survives(doc);
}

#[test]
fn nonsense_values_in_the_tags_are_ignored() {
    let doc = with_tree(|doc, root, _| {
        let a = doc.new_object_id();
        let mut dict = element(
            "Figure",
            root,
            vec![Object::Real(f32::NAN), Object::Boolean(true), string("x")],
        );
        dict.set("Pg", Object::Integer(7));
        dict.set("A", Object::Integer(3));
        dict.set("Alt", Object::Integer(5));
        doc.objects.insert(a, Object::Dictionary(dict));
        Object::Reference(a)
    });
    let report = survives(doc);
    assert!(report.has("missing-alt-text"));
}

#[test]
fn a_looping_role_map_is_survived() {
    let mut doc = with_tree(|doc, root, page| {
        let a = doc.new_object_id();
        let mut dict = element("Mine", root, vec![Object::Integer(0)]);
        dict.set("Pg", Object::Reference(page));
        doc.objects.insert(a, Object::Dictionary(dict));
        Object::Reference(a)
    });
    let root = doc
        .catalog()
        .unwrap()
        .get(b"StructTreeRoot")
        .unwrap()
        .as_reference()
        .unwrap();
    let mut map = Dictionary::new();
    map.set("Mine", name("Yours"));
    map.set("Yours", name("Mine"));
    doc.get_dictionary_mut(root)
        .unwrap()
        .set("RoleMap", Object::Dictionary(map));
    survives(doc);
}

#[test]
fn unreadable_page_content_is_survived() {
    let mut builder = Builder::new();
    builder.page("BT /F1 12 Tf ( unclosed string [[[ <<<< Tj");
    builder.page("EMC EMC Q Q Q ET 1 2 3 4 5 6 7 cm BDC");
    let mut doc = builder.build();
    add_metadata(&mut doc, "en", "Broken");
    let report = survives(doc);
    assert!(report.has("not-tagged"));
}

#[test]
fn a_page_tree_loop_is_survived() {
    let mut doc = good_document();
    // the pages node lists itself as one of its kids
    let pages = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let dict = doc.get_dictionary_mut(pages).unwrap();
    let mut kids = dict.get(b"Kids").unwrap().as_array().unwrap().clone();
    kids.push(Object::Reference(pages));
    dict.set("Kids", Object::Array(kids));
    let _ = check(&doc, None);
}

#[test]
fn a_form_that_draws_itself_is_survived() {
    let mut builder = Builder::new();
    builder.page("/Fx Do");
    let page = builder.pages[0];
    let mut doc = builder.build();
    let form = doc.new_object_id();
    let mut dict = Dictionary::new();
    dict.set("Type", name("XObject"));
    dict.set("Subtype", name("Form"));
    dict.set("BBox", numbers(&[0.0, 0.0, 100.0, 100.0]));
    let mut xobjects = Dictionary::new();
    xobjects.set("Fx", Object::Reference(form));
    let mut resources = Dictionary::new();
    resources.set("XObject", Object::Dictionary(xobjects.clone()));
    dict.set("Resources", Object::Dictionary(resources.clone()));
    doc.objects
        .insert(form, Object::Stream(Stream::new(dict, b"/Fx Do".to_vec())));
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("Resources", Object::Dictionary(resources));
    let _ = check(&doc, None).unwrap();
}

#[test]
fn a_document_with_no_pages_is_checked() {
    let mut doc = Document::with_version("1.7");
    let mut pages = Dictionary::new();
    pages.set("Type", name("Pages"));
    pages.set("Count", Object::Integer(0));
    pages.set("Kids", Object::Array(Vec::new()));
    let pages = doc.add_object(pages);
    let mut catalog = Dictionary::new();
    catalog.set("Type", name("Catalog"));
    catalog.set("Pages", Object::Reference(pages));
    let catalog = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog));
    let report = check(&doc, None).unwrap();
    assert_eq!(report.page_count, 0);
    let fonts = report
        .rules
        .iter()
        .find(|r| r.id == "font-not-embedded")
        .unwrap();
    assert_eq!(fonts.status, RuleStatus::NotApplicable);
}
