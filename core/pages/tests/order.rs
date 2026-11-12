//! reordering, moving and deleting pages, and keeping bookmarks, links
//! and form fields valid while doing it.

mod support;

use letters_pages::{
    delete, move_pages, reorder, rotate, DeleteParams, MovePagesParams, PagesError, ReorderParams,
    RotateParams,
};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use support::*;

fn reorder_to(doc: &mut Document, new_order: &[u32]) -> Result<(), PagesError> {
    reorder(
        doc,
        &ReorderParams {
            new_order: new_order.to_vec(),
        },
    )
}

#[test]
fn reorder_puts_pages_in_the_new_order() {
    let mut doc = tagged(4);
    reorder_to(&mut doc, &[3, 0, 2, 1]).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(order(&doc), [3, 0, 2, 1]);
    // the text drawn on each page moved with it
    assert!(content_text(&doc, page_ids(&doc)[0]).contains("(Page 3)"));
}

#[test]
fn reorder_refuses_anything_but_a_full_permutation() {
    let mut doc = tagged(3);
    let before = fingerprint(&doc);
    assert_eq!(
        reorder_to(&mut doc, &[0, 1]),
        Err(PagesError::NotAPermutation)
    );
    assert_eq!(
        reorder_to(&mut doc, &[0, 1, 1]),
        Err(PagesError::NotAPermutation)
    );
    assert_eq!(
        reorder_to(&mut doc, &[0, 1, 3]),
        Err(PagesError::NoSuchPage(4))
    );
    assert_eq!(
        fingerprint(&doc),
        before,
        "a refused reorder changes nothing"
    );
}

#[test]
fn moving_pages_works_like_dragging_thumbnails() {
    let cases: [(&[u32], u32, [usize; 5]); 5] = [
        (&[3], 0, [3, 0, 1, 2, 4]),
        (&[0], 5, [1, 2, 3, 4, 0]),
        (&[4, 1], 3, [0, 2, 1, 4, 3]),
        // dropped onto one of the moved pages: they land before the next page that stays
        (&[1, 2], 2, [0, 1, 2, 3, 4]),
        (&[0, 4], 2, [1, 0, 4, 2, 3]),
    ];
    for (pages, before_index, expected) in cases {
        let mut doc = tagged(5);
        move_pages(
            &mut doc,
            &MovePagesParams {
                pages: pages.to_vec(),
                before_index,
            },
        )
        .unwrap();
        assert_eq!(
            order(&reload(&mut doc)),
            expected,
            "moving {pages:?} before {before_index}"
        );
    }
}

#[test]
fn moving_checks_its_settings() {
    let mut doc = tagged(3);
    let params = |pages: Vec<u32>, before_index| MovePagesParams {
        pages,
        before_index,
    };
    assert_eq!(
        move_pages(&mut doc, &params(vec![0], 4)),
        Err(PagesError::NoSuchPosition)
    );
    assert_eq!(
        move_pages(&mut doc, &params(vec![], 0)),
        Err(PagesError::NoPagesChosen)
    );
    assert_eq!(
        move_pages(&mut doc, &params(vec![7], 0)),
        Err(PagesError::NoSuchPage(8))
    );
}

#[test]
fn delete_removes_pages_and_what_only_they_used() {
    let mut doc = tagged(4);
    let only_on_page_2 = doc.get_page_contents(page_ids(&doc)[2])[0];
    delete(&mut doc, &DeleteParams { pages: vec![2, 0] }).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(order(&doc), [1, 3]);
    assert_eq!(doc.get_pages().len(), 2);
    assert!(
        !doc.objects.contains_key(&only_on_page_2),
        "the deleted page's drawing is dropped"
    );
    assert_no_dangling_references(&doc);
}

#[test]
fn delete_refuses_to_remove_every_page() {
    let mut doc = tagged(2);
    let before = fingerprint(&doc);
    let result = delete(&mut doc, &DeleteParams { pages: vec![1, 0] });
    assert_eq!(result, Err(PagesError::CannotDeleteEveryPage));
    assert_eq!(
        delete(&mut doc, &DeleteParams { pages: vec![2] }),
        Err(PagesError::NoSuchPage(3))
    );
    assert_eq!(
        delete(&mut doc, &DeleteParams { pages: vec![0, 0] }),
        Err(PagesError::PageChosenTwice(1))
    );
    assert_eq!(fingerprint(&doc), before);
}

/// a page tree four levels deep, with settings inherited from three
/// different levels:
///
/// root (paper 300x200, resources with F1)
///   branch a (rotate 90)
///     page 0
///     branch b (paper 400x200)
///       page 1
///       page 2
///   page 3 (own paper 500x200)
fn deep_tree() -> Document {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object({
        let mut font = Dictionary::new();
        font.set("Type", Object::Name(b"Font".to_vec()));
        font.set("Subtype", Object::Name(b"Type1".to_vec()));
        font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
        font
    });
    let [root, branch_a, branch_b] = [
        doc.new_object_id(),
        doc.new_object_id(),
        doc.new_object_id(),
    ];
    let page = |doc: &mut Document, parent: ObjectId, label: usize, own_paper: Option<f32>| {
        let text = format!("BT /F1 12 Tf 10 10 Td (Page {label}) Tj ET");
        let content = doc.add_object(Stream::new(Dictionary::new(), text.into_bytes()));
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"Page".to_vec()));
        dict.set("Parent", Object::Reference(parent));
        dict.set("Contents", Object::Reference(content));
        if let Some(width) = own_paper {
            dict.set("MediaBox", rect(0.0, 0.0, width, 200.0));
        }
        doc.add_object(dict)
    };
    let pages = [
        page(&mut doc, branch_a, 0, None),
        page(&mut doc, branch_b, 1, None),
        page(&mut doc, branch_b, 2, None),
        page(&mut doc, root, 3, Some(500.0)),
    ];
    let node = |kids: Vec<ObjectId>, parent: Option<ObjectId>, extra: &[(&str, Object)]| {
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"Pages".to_vec()));
        dict.set(
            "Kids",
            Object::Array(kids.into_iter().map(Object::Reference).collect()),
        );
        if let Some(parent) = parent {
            dict.set("Parent", Object::Reference(parent));
        }
        for (key, value) in extra {
            dict.set(*key, value.clone());
        }
        dict
    };
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    let root_node = node(
        vec![branch_a, pages[3]],
        None,
        &[
            ("Count", Object::Integer(4)),
            ("MediaBox", rect(0.0, 0.0, 300.0, 200.0)),
            ("Resources", Object::Dictionary(resources)),
        ],
    );
    let a = node(
        vec![pages[0], branch_b],
        Some(root),
        &[
            ("Count", Object::Integer(3)),
            ("Rotate", Object::Integer(90)),
        ],
    );
    let b = node(
        vec![pages[1], pages[2]],
        Some(branch_a),
        &[
            ("Count", Object::Integer(2)),
            ("MediaBox", rect(0.0, 0.0, 400.0, 200.0)),
        ],
    );
    doc.objects.insert(root, Object::Dictionary(root_node));
    doc.objects.insert(branch_a, Object::Dictionary(a));
    doc.objects.insert(branch_b, Object::Dictionary(b));
    finish_catalog(&mut doc, root);
    doc
}

/// (width, rotation, has the font) of every page, in order.
fn looks(doc: &Document) -> Vec<(f32, i64, bool)> {
    page_ids(doc)
        .into_iter()
        .map(|page| {
            let [_, _, width, _] = page_box(doc, page, b"MediaBox").unwrap();
            let resources = inherited(doc, page, b"Resources").unwrap();
            let has_font = resources.as_dict().unwrap().has(b"Font");
            (width, rotation(doc, page), has_font)
        })
        .collect()
}

#[test]
fn a_deep_inherited_tree_keeps_every_setting_when_reordered() {
    let mut doc = deep_tree();
    let before = looks(&doc);
    assert_eq!(
        before,
        [
            (300.0, 90, true),
            (400.0, 90, true),
            (400.0, 90, true),
            (500.0, 0, true)
        ]
    );
    reorder_to(&mut doc, &[3, 2, 1, 0]).unwrap();
    let doc = reload(&mut doc);
    let after = looks(&doc);
    assert_eq!(after, before.into_iter().rev().collect::<Vec<_>>());
    assert!(content_text(&doc, page_ids(&doc)[1]).contains("(Page 2)"));
    assert_no_dangling_references(&doc);
}

#[test]
fn a_deep_tree_can_have_pages_deleted_and_rotated() {
    let mut doc = deep_tree();
    delete(&mut doc, &DeleteParams { pages: vec![1] }).unwrap();
    rotate(
        &mut doc,
        &RotateParams {
            pages: vec![0, 2],
            quarter_turns: 1,
        },
    )
    .unwrap();
    let doc = reload(&mut doc);
    // page 0 inherited 90 and gains a quarter; page 3 had none
    assert_eq!(
        looks(&doc),
        [(300.0, 180, true), (400.0, 90, true), (500.0, 90, true)]
    );
}

#[test]
fn bookmarks_follow_their_pages_when_reordered() {
    let mut doc = tagged(3);
    add_outline(
        &mut doc,
        &[
            bookmark("One", 0, vec![bookmark("One a", 1, vec![])]),
            bookmark("Three", 2, vec![]),
        ],
    );
    reorder_to(&mut doc, &[2, 0, 1]).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(
        read_outline(&doc),
        [
            (0, "One".to_owned(), Some(1)),
            (1, "One a".to_owned(), Some(2)),
            (0, "Three".to_owned(), Some(0)),
        ]
    );
}

#[test]
fn deleting_a_page_removes_its_bookmarks_and_keeps_their_children() {
    let mut doc = tagged(4);
    add_outline(
        &mut doc,
        &[
            bookmark("Intro", 0, vec![]),
            bookmark(
                "Chapter",
                1,
                vec![
                    bookmark("Section", 2, vec![]),
                    bookmark("Gone too", 1, vec![]),
                ],
            ),
            bookmark("End", 3, vec![]),
        ],
    );
    delete(&mut doc, &DeleteParams { pages: vec![1] }).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(
        read_outline(&doc),
        [
            (0, "Intro".to_owned(), Some(0)),
            (0, "Section".to_owned(), Some(1)),
            (0, "End".to_owned(), Some(2)),
        ]
    );
    assert_no_dangling_references(&doc);
}

#[test]
fn links_to_deleted_pages_are_removed_and_others_still_lead_to_the_right_page() {
    let mut doc = tagged(4);
    add_link(&mut doc, 0, 2);
    add_link(&mut doc, 0, 3);
    add_link(&mut doc, 3, 0);
    delete(&mut doc, &DeleteParams { pages: vec![2] }).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(
        link_targets(&doc, 0),
        [Some(2)],
        "only the link to the old page 4 is left"
    );
    assert_eq!(link_targets(&doc, 2), [Some(0)]);
    assert_no_dangling_references(&doc);
}

#[test]
fn named_destinations_to_deleted_pages_are_removed() {
    let mut doc = tagged(3);
    let pages = page_ids(&doc);
    let dest = |page: ObjectId| {
        Object::Array(vec![Object::Reference(page), Object::Name(b"Fit".to_vec())])
    };
    let mut leaf = Dictionary::new();
    leaf.set(
        "Names",
        Object::Array(vec![
            Object::string_literal("first"),
            dest(pages[0]),
            Object::string_literal("second"),
            dest(pages[1]),
        ]),
    );
    let mut names = Dictionary::new();
    names.set("Dests", Object::Dictionary(leaf));
    doc.catalog_mut()
        .unwrap()
        .set("Names", Object::Dictionary(names));
    // a link that uses a name instead of a written-out destination
    let link = add_link(&mut doc, 2, 0);
    doc.get_dictionary_mut(link)
        .unwrap()
        .set("Dest", Object::string_literal("second"));

    delete(&mut doc, &DeleteParams { pages: vec![1] }).unwrap();
    let doc = reload(&mut doc);

    let names = doc
        .catalog()
        .unwrap()
        .get(b"Names")
        .unwrap()
        .as_dict()
        .unwrap();
    let leaf = names.get(b"Dests").unwrap().as_dict().unwrap();
    let entries = leaf.get(b"Names").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].as_str().unwrap(), b"first");
    assert!(
        link_targets(&doc, 1).is_empty(),
        "the link through the removed name is gone"
    );
    assert_no_dangling_references(&doc);
}

#[test]
fn form_fields_lose_boxes_on_deleted_pages_and_vanish_with_their_last_box() {
    let mut doc = tagged(3);
    add_text_field(&mut doc, "name", &[0, 1]);
    add_text_field(&mut doc, "only_on_two", &[1]);
    add_text_field(&mut doc, "signature", &[2]);
    delete(&mut doc, &DeleteParams { pages: vec![1] }).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(
        fields(&doc),
        [("name".to_owned(), 1), ("signature".to_owned(), 1)]
    );
    assert_no_dangling_references(&doc);
}

#[test]
fn deleting_the_page_the_file_opens_at_clears_that_setting() {
    let mut doc = tagged(2);
    let second = page_ids(&doc)[1];
    doc.catalog_mut().unwrap().set(
        "OpenAction",
        Object::Array(vec![
            Object::Reference(second),
            Object::Name(b"Fit".to_vec()),
        ]),
    );
    delete(&mut doc, &DeleteParams { pages: vec![1] }).unwrap();
    let doc = reload(&mut doc);
    assert!(!doc.catalog().unwrap().has(b"OpenAction"));
}
