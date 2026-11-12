//! flattening keeps the page looking the same and removes the fields.

mod support;

use letters_forms::{
    add_field, fields, flatten, redraw_fields, set_value, Alignment, FieldValue, FlattenOptions,
    NewField, NewFieldKind, VisibleRect,
};
use support::{load, plain_page_pdf, render_page, round_trip, sample_form, save, PageSetup};

fn filled_sample(rotate: i32) -> lopdf::Document {
    let mut doc = load(&sample_form(&PageSetup::letter(rotate)));
    let text = |value: &str| FieldValue::Text(value.into());
    set_value(&mut doc, "name", &text("Mary Somerville")).unwrap();
    set_value(
        &mut doc,
        "notes",
        &text("Two lines of notes, long enough to wrap inside the box."),
    )
    .unwrap();
    set_value(&mut doc, "agree", &FieldValue::Checked(true)).unwrap();
    set_value(&mut doc, "colour", &text("Red")).unwrap();
    set_value(&mut doc, "country", &text("fr")).unwrap();
    set_value(
        &mut doc,
        "subjects",
        &FieldValue::Selected(vec!["Art".into()]),
    )
    .unwrap();
    set_value(&mut doc, "born", &text("1780-12-26")).unwrap();
    set_value(&mut doc, "code", &text("XY12")).unwrap();
    set_value(&mut doc, "address.city", &text("Jedburgh")).unwrap();
    // "locked" has a value but no drawing; without one each reader guesses
    assert_eq!(
        redraw_fields(&mut doc, true).unwrap(),
        ["locked", "must", "address.street", "sign_here"]
    );
    doc
}

/// renders, flattens, renders again: the pages must match pixel for
/// pixel (allowing a handful of anti-aliased edge pixels).
fn assert_flatten_keeps_the_look(mut doc: lopdf::Document, test_name: &str) {
    let before_bytes = save(&mut doc);
    let before = render_page(&before_bytes, 0, 1000, &format!("{test_name}-before"));
    let report = flatten(&mut doc, &FlattenOptions::default()).unwrap();
    assert!(!report.flattened.is_empty());
    let after_bytes = save(&mut doc);
    let after = render_page(&after_bytes, 0, 1000, &format!("{test_name}-after"));
    let differing = before.differing_pixels(&after);
    let sample: Vec<(u32, u32)> = before
        .differences(&after)
        .into_iter()
        .step_by(7)
        .take(20)
        .collect();
    assert!(
        differing <= 20,
        "{differing} pixels changed when flattening, such as {sample:?}"
    );
    // and the answers really are on the page: it differs from a blank one
    let blank = render_page(
        &plain_page_pdf(&PageSetup::letter(0)),
        0,
        1000,
        &format!("{test_name}-blank"),
    );
    if (blank.width, blank.height) == (after.width, after.height) {
        assert!(after.differing_pixels(&blank) > 500);
    }
}

#[test]
fn flattening_keeps_the_page_looking_the_same() {
    assert_flatten_keeps_the_look(filled_sample(0), "flatten-upright");
}

#[test]
fn flattening_keeps_turned_pages_looking_the_same() {
    assert_flatten_keeps_the_look(filled_sample(90), "flatten-turned");
    // fields letters added itself carry a turn (/MK /R) in their drawing
    let mut doc = load(&plain_page_pdf(&PageSetup::letter(270)));
    let field = NewField {
        name: "answer".into(),
        page: 0,
        rect: VisibleRect::new(100.0, 100.0, 250.0, 30.0),
        kind: NewFieldKind::Text {
            max_length: None,
            comb: false,
        },
        tooltip: None,
        required: false,
        read_only: false,
        font_size: 14.0,
        alignment: Alignment::Center,
    };
    add_field(&mut doc, &field).unwrap();
    set_value(
        &mut doc,
        "answer",
        &FieldValue::Text("Turned but upright".into()),
    )
    .unwrap();
    assert_flatten_keeps_the_look(doc, "flatten-added-270");
}

#[test]
fn flattened_fields_are_gone_and_answers_stay_on_the_page() {
    let mut doc = filled_sample(0);
    flatten(&mut doc, &FlattenOptions::default()).unwrap();
    let mut doc = round_trip(&mut doc);
    let left: Vec<String> = fields(&doc)
        .unwrap()
        .into_iter()
        .map(|field| field.name)
        .collect();
    // the empty signature field is kept so the document can still be signed
    assert_eq!(left, ["sign_here"]);
    let page = doc.get_dictionary(doc.page_iter().next().unwrap()).unwrap();
    assert_eq!(page.get(b"Annots").unwrap().as_array().unwrap().len(), 1);
    let content = String::from_utf8_lossy(&doc.get_page_content(doc.page_iter().next().unwrap()))
        .into_owned();
    assert!(content.contains("/LettersFlat1 Do"), "{content}");

    let everything = FlattenOptions {
        fields: None,
        include_signatures: true,
    };
    flatten(&mut doc, &everything).unwrap();
    let doc = round_trip(&mut doc);
    assert!(fields(&doc).unwrap().is_empty());
    assert!(
        !doc.catalog().unwrap().has(b"AcroForm"),
        "an empty form is removed"
    );
}

#[test]
fn flattening_only_some_fields_keeps_the_rest_fillable() {
    let mut doc = filled_sample(0);
    let options = FlattenOptions {
        fields: Some(vec!["name".into(), "colour".into(), "address.city".into()]),
        include_signatures: false,
    };
    let report = flatten(&mut doc, &options).unwrap();
    assert_eq!(report.flattened, ["name", "colour", "address.city"]);
    let mut doc = round_trip(&mut doc);
    let left: Vec<String> = fields(&doc)
        .unwrap()
        .into_iter()
        .map(|field| field.name)
        .collect();
    assert!(left.contains(&"notes".to_owned()) && left.contains(&"address.street".to_owned()));
    assert!(!left.contains(&"name".to_owned()) && !left.contains(&"colour".to_owned()));
    set_value(
        &mut doc,
        "address.street",
        &FieldValue::Text("1 High St".into()),
    )
    .unwrap();
}
