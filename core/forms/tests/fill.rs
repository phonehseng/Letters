//! reading and filling forms: values must reach the file, and the drawn
//! answers must show up on the rendered page.

mod support;

use letters_forms::{fields, set_value, FieldInfo, FieldKind, FieldValue, FormError, VisibleRect};
use lopdf::{Dictionary, Document, Object};
use support::{load, render_page, round_trip, sample_form, save, PageSetup};

fn sample(rotate: i32) -> Document {
    load(&sample_form(&PageSetup::letter(rotate)))
}

fn field<'a>(all: &'a [FieldInfo], name: &str) -> &'a FieldInfo {
    all.iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("no field {name}"))
}

/// the decoded normal appearance stream of a field's widget (the one for
/// `state` when the widget has several).
fn appearance_text(doc: &Document, field_name: &str, state: Option<&[u8]>) -> String {
    let widget = widget_dict(doc, field_name, 0);
    let ap = widget.get(b"AP").unwrap();
    let ap = match ap {
        Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
        other => other.as_dict().unwrap(),
    };
    let normal = ap.get(b"N").unwrap();
    let stream_id = match (normal, state) {
        (Object::Reference(id), None) => *id,
        (Object::Dictionary(states), Some(state)) => {
            states.get(state).unwrap().as_reference().unwrap()
        }
        other => panic!("unexpected appearance {other:?}"),
    };
    let stream = doc.get_object(stream_id).unwrap().as_stream().unwrap();
    String::from_utf8_lossy(
        &stream
            .decompressed_content()
            .unwrap_or(stream.content.clone()),
    )
    .into_owned()
}

/// the dictionary of a field's n-th widget, found through /Fields.
fn widget_dict<'a>(doc: &'a Document, field_name: &str, index: usize) -> &'a Dictionary {
    let id = widget_id(doc, field_name, index);
    doc.get_dictionary(id).unwrap()
}

fn widget_id(doc: &Document, field_name: &str, index: usize) -> lopdf::ObjectId {
    let all = all_widgets(doc);
    all.into_iter()
        .filter(|(name, _)| name == field_name)
        .nth(index)
        .map(|(_, id)| id)
        .unwrap_or_else(|| panic!("no widget {index} for {field_name}"))
}

/// every widget with its field's full name, walking /Kids by hand.
fn all_widgets(doc: &Document) -> Vec<(String, lopdf::ObjectId)> {
    fn walk(
        doc: &Document,
        id: lopdf::ObjectId,
        prefix: &str,
        out: &mut Vec<(String, lopdf::ObjectId)>,
    ) {
        let dict = doc.get_dictionary(id).unwrap();
        let name = match dict.get(b"T") {
            Ok(Object::String(bytes, _)) => {
                let part = String::from_utf8_lossy(bytes).into_owned();
                if prefix.is_empty() {
                    part
                } else {
                    format!("{prefix}.{part}")
                }
            }
            _ => prefix.to_owned(),
        };
        match dict.get(b"Kids") {
            Ok(Object::Array(kids)) => {
                for kid in kids {
                    walk(doc, kid.as_reference().unwrap(), &name, out);
                }
            }
            _ => out.push((name, id)),
        }
    }
    let catalog = doc.catalog().unwrap();
    let form_id = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
    let form = doc.get_dictionary(form_id).unwrap();
    let mut out = Vec::new();
    for field in form.get(b"Fields").unwrap().as_array().unwrap() {
        walk(doc, field.as_reference().unwrap(), "", &mut out);
    }
    out
}

fn stored_value<'a>(doc: &'a Document, field_name: &str) -> &'a Object {
    let id = field_dict_id(doc, field_name);
    doc.get_dictionary(id).unwrap().get(b"V").unwrap()
}

/// the field dictionary (not the widget) for radio groups, which keep /V on the parent.
fn field_dict_id(doc: &Document, field_name: &str) -> lopdf::ObjectId {
    let widget = widget_id(doc, field_name, 0);
    let dict = doc.get_dictionary(widget).unwrap();
    if dict.has(b"T") {
        widget
    } else {
        dict.get(b"Parent").unwrap().as_reference().unwrap()
    }
}

#[test]
fn reads_every_kind_of_field() {
    let doc = sample(0);
    let all = fields(&doc).unwrap();
    let kinds: Vec<(&str, FieldKind)> = all
        .iter()
        .map(|field| (field.name.as_str(), field.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("name", FieldKind::Text),
            ("notes", FieldKind::MultilineText),
            ("agree", FieldKind::Checkbox),
            ("colour", FieldKind::RadioGroup),
            ("country", FieldKind::Dropdown),
            ("subjects", FieldKind::List),
            ("born", FieldKind::Date),
            ("code", FieldKind::Text),
            ("locked", FieldKind::Text),
            ("must", FieldKind::Text),
            ("address.street", FieldKind::Text),
            ("address.city", FieldKind::Text),
            ("sign_here", FieldKind::Signature),
        ]
    );
    let name = field(&all, "name");
    assert_eq!(
        name.widgets[0].rect,
        VisibleRect::new(100.0, 72.0, 200.0, 20.0)
    );
    assert_eq!(name.widgets[0].page, Some(0));
    assert_eq!(name.tooltip.as_deref(), Some("Your full name"));
    assert_eq!(name.max_length, Some(20));
    assert_eq!(name.font_size, 12.0);

    let colour = field(&all, "colour");
    let options: Vec<&str> = colour
        .options
        .iter()
        .map(|option| option.value.as_str())
        .collect();
    assert_eq!(options, ["Red", "Green", "Blue"]);
    assert_eq!(colour.widgets.len(), 3);
    assert_eq!(colour.widgets[1].on_value.as_deref(), Some("Green"));

    let country = field(&all, "country");
    assert_eq!(country.options[2].value, "nz");
    assert_eq!(country.options[2].label, "New Zealand");
    assert!(field(&all, "subjects").multi_select);
    assert_eq!(
        field(&all, "born").date_format.as_deref(),
        Some("dd/mm/yyyy")
    );
    assert!(field(&all, "code").comb);
    assert!(field(&all, "locked").read_only);
    assert_eq!(field(&all, "locked").value.as_deref(), Some("fixed"));
    assert!(field(&all, "must").required);
    assert_eq!(field(&all, "agree").checked, Some(false));
    assert!(!field(&all, "sign_here").signed);
}

#[test]
fn widget_rectangles_follow_the_page_turn() {
    let doc = sample(90);
    let all = fields(&doc).unwrap();
    // turned clockwise: the field's user-space x becomes the visible y
    assert_eq!(
        field(&all, "name").widgets[0].rect,
        VisibleRect::new(700.0, 100.0, 20.0, 200.0)
    );

    let mut page = PageSetup::letter(0);
    page.crop_box = Some([50.0, 50.0, 562.0, 742.0]);
    let doc = load(&sample_form(&page));
    let all = fields(&doc).unwrap();
    assert_eq!(
        field(&all, "name").widgets[0].rect,
        VisibleRect::new(50.0, 22.0, 200.0, 20.0)
    );
}

#[test]
fn text_reaches_the_file_and_its_drawing() {
    let mut doc = sample(0);
    set_value(
        &mut doc,
        "name",
        &FieldValue::Text("Ada (A.) Lovelace".into()),
    )
    .unwrap();
    let doc = round_trip(&mut doc);

    let all = fields(&doc).unwrap();
    assert_eq!(
        field(&all, "name").value.as_deref(),
        Some("Ada (A.) Lovelace")
    );
    let drawing = appearance_text(&doc, "name", None);
    assert!(drawing.contains("(Ada \\(A.\\) Lovelace) Tj"), "{drawing}");
    assert!(drawing.contains("/Helv 12 Tf"), "{drawing}");
}

#[test]
fn multiline_text_wraps_inside_the_box() {
    let mut doc = sample(0);
    let long = "This answer is long enough that it has to wrap onto more than one line of the box.";
    set_value(&mut doc, "notes", &FieldValue::Text(long.into())).unwrap();
    let doc = round_trip(&mut doc);
    let drawing = appearance_text(&doc, "notes", None);
    assert!(drawing.matches(" Tj").count() >= 2, "{drawing}");
}

#[test]
fn inherited_settings_are_used() {
    let mut doc = sample(0);
    set_value(
        &mut doc,
        "address.city",
        &FieldValue::Text("Wellington".into()),
    )
    .unwrap();
    let doc = round_trip(&mut doc);
    assert_eq!(
        field(&fields(&doc).unwrap(), "address.city")
            .value
            .as_deref(),
        Some("Wellington")
    );
    let drawing = appearance_text(&doc, "address.city", None);
    // the colour and size come from the parent's /DA
    assert!(drawing.contains("/Helv 11 Tf"), "{drawing}");
    assert!(drawing.contains("0 0 1 rg"), "{drawing}");
}

#[test]
fn checkboxes_store_their_on_state() {
    let mut doc = sample(0);
    set_value(&mut doc, "agree", &FieldValue::Checked(true)).unwrap();
    let mut doc = round_trip(&mut doc);
    assert_eq!(stored_value(&doc, "agree"), &Object::Name(b"Yes".to_vec()));
    assert_eq!(
        widget_dict(&doc, "agree", 0).get(b"AS").unwrap(),
        &Object::Name(b"Yes".to_vec())
    );
    assert_eq!(field(&fields(&doc).unwrap(), "agree").checked, Some(true));

    set_value(&mut doc, "agree", &FieldValue::Checked(false)).unwrap();
    let doc = round_trip(&mut doc);
    assert_eq!(
        widget_dict(&doc, "agree", 0).get(b"AS").unwrap(),
        &Object::Name(b"Off".to_vec())
    );
}

#[test]
fn radio_groups_turn_on_exactly_one_button() {
    let mut doc = sample(0);
    set_value(&mut doc, "colour", &FieldValue::Text("Green".into())).unwrap();
    let doc = round_trip(&mut doc);
    assert_eq!(
        stored_value(&doc, "colour"),
        &Object::Name(b"Green".to_vec())
    );
    let states: Vec<Object> = (0..3)
        .map(|index| {
            widget_dict(&doc, "colour", index)
                .get(b"AS")
                .unwrap()
                .clone()
        })
        .collect();
    assert_eq!(
        states,
        vec![
            Object::Name(b"Off".to_vec()),
            Object::Name(b"Green".to_vec()),
            Object::Name(b"Off".to_vec())
        ]
    );
    assert_eq!(
        field(&fields(&doc).unwrap(), "colour").value.as_deref(),
        Some("Green")
    );
}

#[test]
fn dropdowns_store_the_value_and_show_the_label() {
    let mut doc = sample(0);
    set_value(&mut doc, "country", &FieldValue::Text("New Zealand".into())).unwrap();
    let doc = round_trip(&mut doc);
    assert_eq!(
        field(&fields(&doc).unwrap(), "country").value.as_deref(),
        Some("nz")
    );
    assert!(appearance_text(&doc, "country", None).contains("(New Zealand) Tj"));
}

#[test]
fn lists_take_several_choices_when_allowed() {
    let mut doc = sample(0);
    set_value(
        &mut doc,
        "subjects",
        &FieldValue::Selected(vec!["Music".into(), "Art".into()]),
    )
    .unwrap();
    let doc = round_trip(&mut doc);
    let subjects = field(&fields(&doc).unwrap(), "subjects").clone();
    assert_eq!(subjects.selected, ["Music", "Art"]);
    let indices = widget_dict(&doc, "subjects", 0).get(b"I").unwrap().clone();
    assert_eq!(
        indices,
        Object::Array(vec![Object::Integer(1), Object::Integer(3)])
    );
    let drawing = appearance_text(&doc, "subjects", None);
    assert!(
        drawing.contains("(History) Tj") && drawing.contains("0.6 0.75 0.86 rg"),
        "{drawing}"
    );
}

#[test]
fn dates_are_written_in_the_fields_pattern() {
    let mut doc = sample(0);
    set_value(&mut doc, "born", &FieldValue::Text("2026-11-03".into())).unwrap();
    let doc = round_trip(&mut doc);
    assert_eq!(
        field(&fields(&doc).unwrap(), "born").value.as_deref(),
        Some("03/11/2026")
    );
    assert!(appearance_text(&doc, "born", None).contains("(03/11/2026) Tj"));

    let mut doc = sample(0);
    let error = set_value(&mut doc, "born", &FieldValue::Text("someday".into())).unwrap_err();
    assert_eq!(error, FormError::BadDate("someday".into()));
}

#[test]
fn comb_fields_draw_each_character_in_its_cell() {
    let mut doc = sample(0);
    set_value(&mut doc, "code", &FieldValue::Text("AB1234".into())).unwrap();
    let doc = round_trip(&mut doc);
    let drawing = appearance_text(&doc, "code", None);
    assert_eq!(drawing.matches(" Tj").count(), 6, "{drawing}");

    let mut doc = sample(0);
    let error = set_value(&mut doc, "code", &FieldValue::Text("AB12345".into())).unwrap_err();
    assert_eq!(
        error,
        FormError::TooLong {
            field: "code".into(),
            max: 6
        }
    );
}

#[test]
fn refuses_answers_the_field_cannot_take() {
    let mut doc = sample(0);
    let text = |value: &str| FieldValue::Text(value.into());
    assert_eq!(
        set_value(&mut doc, "nope", &text("x")).unwrap_err(),
        FormError::UnknownField("nope".into())
    );
    assert_eq!(
        set_value(&mut doc, "locked", &text("x")).unwrap_err(),
        FormError::ReadOnly("locked".into())
    );
    assert_eq!(
        set_value(&mut doc, "must", &text("  ")).unwrap_err(),
        FormError::Required {
            field: "must".into()
        }
    );
    assert_eq!(
        set_value(&mut doc, "country", &text("Mars")).unwrap_err(),
        FormError::NotAnOption {
            field: "country".into(),
            value: "Mars".into()
        }
    );
    assert_eq!(
        set_value(&mut doc, "colour", &text("Purple")).unwrap_err(),
        FormError::NotAnOption {
            field: "colour".into(),
            value: "Purple".into()
        }
    );
    assert!(matches!(
        set_value(&mut doc, "name", &text("日本語")).unwrap_err(),
        FormError::UnsupportedCharacters(_)
    ));
    assert!(matches!(
        set_value(&mut doc, "sign_here", &text("me")).unwrap_err(),
        FormError::WrongValueKind { .. }
    ));
    // a failed fill changes nothing
    assert_eq!(field(&fields(&doc).unwrap(), "country").value, None);
    // every message is a sentence for the person using the app
    assert_eq!(
        FormError::ReadOnly("locked".into()).to_string(),
        "The field “locked” is read-only, so it can’t be changed."
    );
}

#[test]
fn locked_and_xfa_documents_are_refused_clearly() {
    let mut doc = sample(0);
    let encrypt = doc.add_object(Dictionary::new());
    doc.trailer.set("Encrypt", Object::Reference(encrypt));
    assert_eq!(fields(&doc).unwrap_err(), FormError::Encrypted);
    assert!(FormError::Encrypted.to_string().contains("Unlock it first"));

    let mut doc = sample(0);
    let form_id = doc
        .catalog()
        .unwrap()
        .get(b"AcroForm")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(form_id)
        .unwrap()
        .set("XFA", Object::Array(vec![]));
    assert_eq!(fields(&doc).unwrap_err(), FormError::XfaUnsupported);
    assert_eq!(
        set_value(&mut doc, "name", &FieldValue::Text("x".into())).unwrap_err(),
        FormError::XfaUnsupported
    );
}

#[test]
fn a_document_without_a_form_has_no_fields() {
    let doc = load(&support::plain_page_pdf(&PageSetup::letter(0)));
    assert!(fields(&doc).unwrap().is_empty());
}

#[test]
fn values_round_trip_as_json() {
    let value: FieldValue = serde_json::from_str(r#"{"kind":"selected","value":["Art"]}"#).unwrap();
    assert_eq!(value, FieldValue::Selected(vec!["Art".into()]));
    let info = &fields(&sample(0)).unwrap()[0];
    let json = serde_json::to_value(info).unwrap();
    assert_eq!(json["kind"], "text");
    assert_eq!(json["maxLength"], 20);
    assert_eq!(json["widgets"][0]["rect"]["width"], 200.0);
}

/// fills a few fields, renders the page, and checks ink lands inside each
/// widget's visible rectangle and not just anywhere.
fn assert_answers_are_drawn_in_place(rotate: i32, test_name: &str) {
    let mut doc = sample(rotate);
    set_value(&mut doc, "name", &FieldValue::Text("WWWWWWWW".into())).unwrap();
    set_value(&mut doc, "agree", &FieldValue::Checked(true)).unwrap();
    set_value(&mut doc, "colour", &FieldValue::Text("Blue".into())).unwrap();
    let bytes = save(&mut doc);
    let doc = load(&bytes);
    let all = fields(&doc).unwrap();
    let image = render_page(&bytes, 0, 2000, test_name);

    let name_rect = field(&all, "name").widgets[0].rect;
    assert!(
        image.ink_in(name_rect) > 200,
        "no text drawn in {name_rect:?}"
    );
    let agree_rect = field(&all, "agree").widgets[0].rect;
    assert!(
        image.ink_in(agree_rect) > 100,
        "no check mark in {agree_rect:?}"
    );
    let colour = &field(&all, "colour").widgets;
    assert!(image.ink_in(colour[2].rect) > image.ink_in(colour[0].rect) + 50);

    // the unfilled notes box stays blank
    assert_eq!(image.ink_in(field(&all, "notes").widgets[0].rect), 0);
}

#[test]
fn filled_answers_show_up_on_the_page() {
    assert_answers_are_drawn_in_place(0, "render-upright");
}

#[test]
fn filled_answers_show_up_on_turned_pages() {
    assert_answers_are_drawn_in_place(90, "render-turned-90");
    assert_answers_are_drawn_in_place(270, "render-turned-270");
}
