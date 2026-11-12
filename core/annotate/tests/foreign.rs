//! annotations and form fields made by other apps: listed, kept intact,
//! movable when understood, deletable, and never broken.

mod support;

use letters_annotate::{
    add_text_markup, delete, get, list, update, AnnotateError, AnnotationChanges, AnnotationKind,
    Color, Rect, TextMarkupKind, TextMarkupParams,
};
use lopdf::{Dictionary, Document, Object, ObjectId};
use support::*;

/// a page as another pdf app might leave it: a highlight with no id and
/// no appearance, a square with its own id, a link and a caret.
fn foreign_page() -> TestPage {
    TestPage::letter()
        .with_annotation("<< /Type /Annot /Subtype /Highlight /Rect [100 680 300 700] /QuadPoints [100 700 300 700 100 680 300 680] /C [0 1 0] /T (Other App) /Contents (theirs) >>")
        .with_annotation("<< /Type /Annot /Subtype /Square /Rect [100 500 200 600] /C [1 0 0] /BS << /W 3 >> /NM (acrobat-square-1) /P {page} >>")
        .with_annotation("<< /Type /Annot /Subtype /Link /Rect [400 700 500 720] /Border [0 0 0] /A << /S /URI /URI (https://example.org) >> >>")
        .with_annotation("<< /Type /Annot /Subtype /Caret /Rect [50 50 60 60] /Contents (insert here) >>")
}

fn foreign_doc() -> Document {
    load(&pdf_bytes(&[foreign_page()], true))
}

/// every annotation object on page 0, by object number.
fn annotation_objects(doc: &Document) -> Vec<(ObjectId, Dictionary)> {
    let page = doc.get_pages()[&1];
    doc.get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let id = entry.as_reference().unwrap();
            (id, doc.get_dictionary(id).unwrap().clone())
        })
        .collect()
}

fn widget(doc: &Document) -> (ObjectId, Dictionary) {
    annotation_objects(doc)
        .into_iter()
        .find(|(_, annotation)| name_of(annotation, b"Subtype") == "Widget")
        .expect("the form field is still on the page")
}

fn find(doc: &Document, subtype: &str) -> letters_annotate::AnnotationInfo {
    list(doc, 0)
        .unwrap()
        .into_iter()
        .find(|annotation| annotation.subtype == subtype)
        .unwrap()
}

#[test]
fn annotations_from_other_apps_are_listed() {
    let doc = foreign_doc();
    let listed = list(&doc, 0).unwrap();
    let kinds: Vec<(AnnotationKind, &str)> = listed
        .iter()
        .map(|annotation| (annotation.kind, annotation.subtype.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (AnnotationKind::Highlight, "Highlight"),
            (AnnotationKind::Square, "Square"),
            (AnnotationKind::Other, "Link"),
            (AnnotationKind::Other, "Caret"),
        ],
        "form fields and pop-ups are not listed"
    );
    let highlight = &listed[0];
    assert!(
        highlight.id.starts_with("ref:"),
        "no /NM, so the object number is the id: {}",
        highlight.id
    );
    assert_eq!(highlight.author.as_deref(), Some("Other App"));
    assert_eq!(highlight.color, Some(Color::new(0, 255, 0)));
    assert_eq!(highlight.rect, Rect::new(100.0, 92.0, 200.0, 20.0));
    assert_eq!(listed[1].id, "acrobat-square-1");
    assert!(listed[1].editable && !listed[2].editable && !listed[3].editable);
    assert_eq!(get(&doc, 0, &highlight.id).unwrap(), *highlight);
}

#[test]
fn adding_a_mark_leaves_everything_else_exactly_as_it_was() {
    let mut doc = foreign_doc();
    let before = annotation_objects(&doc);
    let catalog_before = doc.catalog().unwrap().clone();
    let params = TextMarkupParams {
        kind: TextMarkupKind::Underline,
        rects: vec![Rect::new(100.0, 300.0, 100.0, 14.0)],
        color: Color::new(0, 0, 255),
        opacity: 1.0,
        contents: None,
        author: None,
    };
    add_text_markup(&mut doc, 0, &params).unwrap();
    let reloaded = reload(&mut doc);
    let after = annotation_objects(&reloaded);
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(
        &after[..before.len()],
        &before[..],
        "existing annotations and the form field are untouched"
    );
    assert_eq!(
        reloaded.catalog().unwrap().get(b"AcroForm").unwrap(),
        catalog_before.get(b"AcroForm").unwrap()
    );
}

#[test]
fn understood_kinds_from_other_apps_can_be_moved() {
    let mut doc = foreign_doc();
    let before = annotation_objects(&doc);
    let square = find(&doc, "Square");
    let moved = update(
        &mut doc,
        0,
        &square.id,
        &AnnotationChanges {
            rect: Some(Rect::new(
                300.0,
                300.0,
                square.rect.width,
                square.rect.height,
            )),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(moved.id, "acrobat-square-1", "an existing id is kept");
    assert_eq!(moved.rect, Rect::new(300.0, 300.0, 100.0, 100.0));
    let after = annotation_objects(&doc);
    // only the square changed
    for ((id, old), (_, new)) in before.iter().zip(&after) {
        if name_of(old, b"Subtype") != "Square" {
            assert_eq!(old, new, "object {id:?} changed");
        }
    }
    let image = render(&save(&mut doc), 0);
    assert_near(
        image.pixel(301.0, 350.0),
        [255, 0, 0],
        60,
        "the square in its new place",
    );
}

#[test]
fn restyling_a_mark_from_another_app_gives_it_a_drawing_and_a_lasting_id() {
    let mut doc = foreign_doc();
    let highlight = find(&doc, "Highlight");
    let changed = update(
        &mut doc,
        0,
        &highlight.id,
        &AnnotationChanges {
            color: Some(Color::HIGHLIGHT_YELLOW),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        changed.id.starts_with("letters-"),
        "it now has a /NM: {}",
        changed.id
    );
    // the id the ui already holds keeps working
    assert_eq!(get(&doc, 0, &highlight.id).unwrap().id, changed.id);
    let reloaded = reload(&mut doc);
    let annotation = annotation_by_name(&reloaded, 0, &changed.id);
    assert!(has_normal_appearance(&reloaded, &annotation));
    assert_eq!(
        annotation.get(b"T").unwrap().as_str().unwrap(),
        b"Other App",
        "its author is kept"
    );
    let image = render(&save(&mut doc), 0);
    assert_near(
        image.pixel(200.0, 102.0),
        [255, 212, 0],
        12,
        "drawn in the new colour",
    );
}

#[test]
fn unknown_kinds_are_left_alone_but_can_be_deleted() {
    let mut doc = foreign_doc();
    let link = find(&doc, "Link");
    let before = annotation_objects(&doc);
    let error = update(
        &mut doc,
        0,
        &link.id,
        &AnnotationChanges {
            contents: Some(Some("x".into())),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error, AnnotateError::CannotChange);
    assert_eq!(
        annotation_objects(&doc),
        before,
        "a refused change changes nothing"
    );

    assert_eq!(
        delete(&mut doc, 0, &link.id).unwrap(),
        std::slice::from_ref(&link.id)
    );
    let caret = find(&doc, "Caret");
    delete(&mut doc, 0, &caret.id).unwrap();
    let left: Vec<String> = list(&doc, 0)
        .unwrap()
        .into_iter()
        .map(|annotation| annotation.subtype)
        .collect();
    assert_eq!(left, ["Highlight", "Square"]);
}

#[test]
fn form_fields_are_never_changed_here() {
    let mut doc = foreign_doc();
    let (widget_id, widget_before) = widget(&doc);
    let widget_ref = format!("ref:{}-{}", widget_id.0, widget_id.1);
    let move_it = AnnotationChanges {
        rect: Some(Rect::new(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    };
    assert_eq!(
        update(&mut doc, 0, &widget_ref, &move_it).unwrap_err(),
        AnnotateError::FormField
    );
    assert_eq!(
        delete(&mut doc, 0, &widget_ref).unwrap_err(),
        AnnotateError::FormField
    );

    // deleting everything else still leaves the field and the form intact
    for annotation in list(&doc, 0).unwrap() {
        delete(&mut doc, 0, &annotation.id).unwrap();
    }
    let reloaded = reload(&mut doc);
    assert!(list(&reloaded, 0).unwrap().is_empty());
    let (_, widget_after) = widget(&reloaded);
    assert_eq!(widget_after, widget_before);
    let fields = reloaded
        .catalog()
        .unwrap()
        .get(b"AcroForm")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Fields")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(fields[0].as_reference().unwrap(), widget_id);
}

#[test]
fn annotations_written_straight_into_the_page_list_work_too() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let page = doc.get_pages()[&1];
    let mut inline = Dictionary::new();
    inline.set("Type", Object::Name(b"Annot".to_vec()));
    inline.set("Subtype", Object::Name(b"Square".to_vec()));
    inline.set(
        "Rect",
        Object::Array(vec![100.into(), 100.into(), 200.into(), 200.into()]),
    );
    inline.set("C", Object::Array(vec![0.into(), 0.into(), 1.into()]));
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("Annots", Object::Array(vec![Object::Dictionary(inline)]));

    let listed = list(&doc, 0).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "at:0");
    let moved = update(
        &mut doc,
        0,
        "at:0",
        &AnnotationChanges {
            rect: Some(Rect::new(300.0, 300.0, 100.0, 100.0)),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(moved.id.starts_with("letters-"));
    assert_eq!(list(&doc, 0).unwrap()[0].id, moved.id);
    delete(&mut doc, 0, &moved.id).unwrap();
    assert!(list(&doc, 0).unwrap().is_empty());
}

#[test]
fn locked_annotations_are_not_moved() {
    let page = TestPage::letter().with_annotation(
        "<< /Type /Annot /Subtype /Square /Rect [100 500 200 600] /C [1 0 0] /F 132 /NM (locked-1) >>",
    );
    let mut doc = load(&pdf_bytes(&[page], false));
    assert!(!list(&doc, 0).unwrap()[0].editable);
    let error = update(
        &mut doc,
        0,
        "locked-1",
        &AnnotationChanges {
            rect: Some(Rect::new(0.0, 0.0, 100.0, 100.0)),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error, AnnotateError::CannotChange);
}

#[test]
fn a_change_that_cannot_be_drawn_is_undone_completely() {
    // a text box from another app in japanese, which the built-in fonts can't draw
    let page = TestPage::letter().with_annotation(
        "<< /Type /Annot /Subtype /FreeText /Rect [100 600 300 650] /DA (/Helv 12 Tf 0 g) /Contents <FEFF65E5672C> /NM (theirs-1) >>",
    );
    let mut doc = load(&pdf_bytes(&[page], false));
    let before = annotation_objects(&doc);
    let listed = list(&doc, 0).unwrap();
    assert_eq!(listed[0].contents.as_deref(), Some("日本"));
    let error = update(
        &mut doc,
        0,
        "theirs-1",
        &AnnotationChanges {
            color: Some(Color::new(255, 0, 0)),
            rect: Some(Rect::new(50.0, 50.0, 100.0, 100.0)),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error, AnnotateError::UnsupportedCharacters("日本".into()));
    assert_eq!(annotation_objects(&doc), before);
}

#[test]
fn dangling_entries_in_the_page_list_are_tidied_away() {
    let mut doc = foreign_doc();
    let page = doc.get_pages()[&1];
    let missing = (9999, 0);
    if let Ok(Object::Array(entries)) = doc.get_dictionary_mut(page).unwrap().get_mut(b"Annots") {
        entries.push(Object::Reference(missing));
    }
    assert_eq!(
        list(&doc, 0).unwrap().len(),
        4,
        "a missing object is skipped when listing"
    );
    let square = find(&doc, "Square");
    update(
        &mut doc,
        0,
        &square.id,
        &AnnotationChanges {
            author: Some("me".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let entries = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(
        !entries.contains(&Object::Reference(missing)),
        "the dangling entry is gone"
    );
    assert_eq!(entries.len(), 5);
}
