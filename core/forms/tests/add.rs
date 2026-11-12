//! adding, changing and removing fields, then reading them back.

mod support;

use letters_forms::{
    add_field, fields, remove_field, set_value, update_field, Alignment, ChoiceOption,
    FieldChanges, FieldInfo, FieldKind, FieldValue, FormError, NewField, NewFieldKind, RadioButton,
    VisibleRect,
};
use lopdf::{Document, Object};
use support::{load, plain_page_pdf, render_page, round_trip, sample_form, save, PageSetup};

fn blank(rotate: i32) -> Document {
    load(&plain_page_pdf(&PageSetup::letter(rotate)))
}

fn spec(name: &str, rect: VisibleRect, kind: NewFieldKind) -> NewField {
    NewField {
        name: name.into(),
        page: 0,
        rect,
        kind,
        tooltip: None,
        required: false,
        read_only: false,
        font_size: 0.0,
        alignment: Alignment::Left,
    }
}

fn options(labels: &[&str]) -> Vec<ChoiceOption> {
    labels
        .iter()
        .map(|label| ChoiceOption {
            value: label.to_lowercase(),
            label: (*label).into(),
        })
        .collect()
}

fn field<'a>(all: &'a [FieldInfo], name: &str) -> &'a FieldInfo {
    all.iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("no field {name}"))
}

fn close(a: VisibleRect, b: VisibleRect) -> bool {
    (a.x - b.x).abs() < 0.01
        && (a.y - b.y).abs() < 0.01
        && (a.width - b.width).abs() < 0.01
        && (a.height - b.height).abs() < 0.01
}

/// one field of every kind, the way the ui would add them.
fn add_one_of_each(doc: &mut Document) {
    let mut text = spec(
        "Full name",
        VisibleRect::new(72.0, 72.0, 200.0, 20.0),
        NewFieldKind::Text {
            max_length: Some(30),
            comb: false,
        },
    );
    text.tooltip = Some("As on your student card".into());
    text.required = true;
    add_field(doc, &text).unwrap();
    add_field(
        doc,
        &spec(
            "Essay",
            VisibleRect::new(72.0, 110.0, 300.0, 100.0),
            NewFieldKind::MultilineText,
        ),
    )
    .unwrap();
    add_field(
        doc,
        &spec(
            "Agree",
            VisibleRect::new(72.0, 230.0, 14.0, 14.0),
            NewFieldKind::Checkbox {
                on_value: Some("Agreed".into()),
            },
        ),
    )
    .unwrap();
    let buttons = ["Yes", "No"]
        .iter()
        .enumerate()
        .map(|(index, value)| RadioButton {
            value: (*value).into(),
            rect: VisibleRect::new(72.0 + 40.0 * index as f32, 260.0, 14.0, 14.0),
        })
        .collect();
    add_field(
        doc,
        &spec(
            "Attending",
            VisibleRect::new(0.0, 0.0, 0.0, 0.0),
            NewFieldKind::RadioGroup { buttons },
        ),
    )
    .unwrap();
    add_field(
        doc,
        &spec(
            "Year",
            VisibleRect::new(72.0, 290.0, 120.0, 20.0),
            NewFieldKind::Dropdown {
                options: options(&["First", "Second", "Third"]),
                editable: false,
            },
        ),
    )
    .unwrap();
    add_field(
        doc,
        &spec(
            "Clubs",
            VisibleRect::new(220.0, 290.0, 120.0, 60.0),
            NewFieldKind::List {
                options: options(&["Chess", "Choir", "Rowing"]),
                multi_select: true,
            },
        ),
    )
    .unwrap();
    add_field(
        doc,
        &spec(
            "Date",
            VisibleRect::new(72.0, 360.0, 120.0, 20.0),
            NewFieldKind::Date {
                format: Some("dd.mm.yyyy".into()),
            },
        ),
    )
    .unwrap();
    add_field(
        doc,
        &spec(
            "Student number",
            VisibleRect::new(72.0, 400.0, 160.0, 20.0),
            NewFieldKind::Text {
                max_length: Some(8),
                comb: true,
            },
        ),
    )
    .unwrap();
    add_field(
        doc,
        &spec(
            "Signature",
            VisibleRect::new(300.0, 600.0, 200.0, 50.0),
            NewFieldKind::Signature,
        ),
    )
    .unwrap();
}

#[test]
fn added_fields_read_back_as_they_were_made() {
    let mut doc = blank(0);
    add_one_of_each(&mut doc);
    let doc = round_trip(&mut doc);
    let all = fields(&doc).unwrap();

    let kinds: Vec<FieldKind> = all.iter().map(|field| field.kind).collect();
    assert_eq!(
        kinds,
        [
            FieldKind::Text,
            FieldKind::MultilineText,
            FieldKind::Checkbox,
            FieldKind::RadioGroup,
            FieldKind::Dropdown,
            FieldKind::List,
            FieldKind::Date,
            FieldKind::Text,
            FieldKind::Signature
        ]
    );
    let name = field(&all, "Full name");
    assert!(close(
        name.widgets[0].rect,
        VisibleRect::new(72.0, 72.0, 200.0, 20.0)
    ));
    assert_eq!(name.tooltip.as_deref(), Some("As on your student card"));
    assert!(name.required);
    assert_eq!(name.max_length, Some(30));
    assert_eq!(
        field(&all, "Agree").widgets[0].on_value.as_deref(),
        Some("Agreed")
    );
    let attending = field(&all, "Attending");
    assert_eq!(
        attending
            .options
            .iter()
            .map(|o| o.value.as_str())
            .collect::<Vec<_>>(),
        ["Yes", "No"]
    );
    assert!(close(
        attending.widgets[1].rect,
        VisibleRect::new(112.0, 260.0, 14.0, 14.0)
    ));
    assert_eq!(field(&all, "Year").options[1].label, "Second");
    assert!(field(&all, "Clubs").multi_select);
    assert_eq!(
        field(&all, "Date").date_format.as_deref(),
        Some("dd.mm.yyyy")
    );
    assert!(field(&all, "Student number").comb);

    // the new form has helvetica as its default font
    let form = doc
        .catalog()
        .unwrap()
        .get(b"AcroForm")
        .unwrap()
        .as_reference()
        .unwrap();
    let form = doc.get_dictionary(form).unwrap();
    let resources = form.get(b"DR").unwrap().as_dict().unwrap();
    assert!(resources
        .get(b"Font")
        .unwrap()
        .as_dict()
        .unwrap()
        .has(b"Helv"));
}

#[test]
fn added_fields_can_be_filled() {
    let mut doc = blank(0);
    add_one_of_each(&mut doc);
    set_value(
        &mut doc,
        "Full name",
        &FieldValue::Text("Grace Hopper".into()),
    )
    .unwrap();
    set_value(&mut doc, "Agree", &FieldValue::Checked(true)).unwrap();
    set_value(&mut doc, "Attending", &FieldValue::Text("No".into())).unwrap();
    set_value(&mut doc, "Year", &FieldValue::Text("Third".into())).unwrap();
    set_value(
        &mut doc,
        "Clubs",
        &FieldValue::Selected(vec!["chess".into(), "Rowing".into()]),
    )
    .unwrap();
    set_value(&mut doc, "Date", &FieldValue::Text("2026-01-31".into())).unwrap();
    set_value(
        &mut doc,
        "Student number",
        &FieldValue::Text("12345678".into()),
    )
    .unwrap();
    let doc = round_trip(&mut doc);
    let all = fields(&doc).unwrap();
    assert_eq!(
        field(&all, "Full name").value.as_deref(),
        Some("Grace Hopper")
    );
    assert_eq!(field(&all, "Agree").value.as_deref(), Some("Agreed"));
    assert_eq!(field(&all, "Attending").value.as_deref(), Some("No"));
    assert_eq!(field(&all, "Year").value.as_deref(), Some("third"));
    assert_eq!(field(&all, "Clubs").selected, ["chess", "rowing"]);
    assert_eq!(field(&all, "Date").value.as_deref(), Some("31.01.2026"));
}

#[test]
fn names_already_in_use_get_a_number() {
    let mut doc = load(&sample_form(&PageSetup::letter(0)));
    let rect = VisibleRect::new(400.0, 400.0, 100.0, 20.0);
    let text = || NewFieldKind::Text {
        max_length: None,
        comb: false,
    };
    assert_eq!(
        add_field(&mut doc, &spec("name", rect, text())).unwrap(),
        "name_2"
    );
    assert_eq!(
        add_field(&mut doc, &spec("name", rect, text())).unwrap(),
        "name_3"
    );
    // "address" is a parent field, so its name is taken too
    assert_eq!(
        add_field(&mut doc, &spec("address", rect, text())).unwrap(),
        "address_2"
    );
}

#[test]
fn bad_requests_are_refused() {
    let mut doc = blank(0);
    let text = NewFieldKind::Text {
        max_length: None,
        comb: false,
    };
    let rect = VisibleRect::new(10.0, 10.0, 50.0, 20.0);
    assert_eq!(
        add_field(&mut doc, &spec("a.b", rect, text.clone())).unwrap_err(),
        FormError::BadName
    );
    assert_eq!(
        add_field(&mut doc, &spec(" ", rect, text.clone())).unwrap_err(),
        FormError::BadName
    );
    assert_eq!(
        add_field(
            &mut doc,
            &spec("a", VisibleRect::new(10.0, 10.0, 0.0, 20.0), text.clone())
        )
        .unwrap_err(),
        FormError::EmptyRect
    );
    assert_eq!(
        add_field(
            &mut doc,
            &spec(
                "a",
                rect,
                NewFieldKind::Dropdown {
                    options: vec![],
                    editable: false
                }
            )
        )
        .unwrap_err(),
        FormError::NoOptions
    );
    let mut on_page_two = spec("a", rect, text);
    on_page_two.page = 1;
    assert_eq!(
        add_field(&mut doc, &on_page_two).unwrap_err(),
        FormError::UnknownPage(2)
    );
}

#[test]
fn widgets_are_listed_in_reading_order() {
    let mut doc = blank(0);
    let text = || NewFieldKind::Text {
        max_length: None,
        comb: false,
    };
    // added out of order: bottom, top right, top left
    add_field(
        &mut doc,
        &spec("bottom", VisibleRect::new(72.0, 500.0, 100.0, 20.0), text()),
    )
    .unwrap();
    add_field(
        &mut doc,
        &spec(
            "top right",
            VisibleRect::new(300.0, 100.0, 100.0, 20.0),
            text(),
        ),
    )
    .unwrap();
    add_field(
        &mut doc,
        &spec(
            "top left",
            VisibleRect::new(72.0, 102.0, 100.0, 20.0),
            text(),
        ),
    )
    .unwrap();
    let doc = round_trip(&mut doc);
    let page = doc.get_dictionary(doc.page_iter().next().unwrap()).unwrap();
    let names: Vec<String> = page
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|annot| {
            let dict = doc.get_dictionary(annot.as_reference().unwrap()).unwrap();
            String::from_utf8_lossy(dict.get(b"T").unwrap().as_str().unwrap()).into_owned()
        })
        .collect();
    assert_eq!(names, ["top left", "top right", "bottom"]);
    assert_eq!(page.get(b"Tabs").unwrap(), &Object::Name(b"R".to_vec()));
}

#[test]
fn fields_can_be_renamed_moved_and_changed() {
    let mut doc = blank(0);
    add_one_of_each(&mut doc);
    let changes = FieldChanges {
        new_name: Some("Name".into()),
        rect: Some(VisibleRect::new(100.0, 80.0, 250.0, 24.0)),
        required: Some(false),
        read_only: Some(true),
        max_length: Some(0),
        tooltip: Some(String::new()),
        alignment: Some(Alignment::Center),
        font_size: Some(14.0),
        ..FieldChanges::default()
    };
    assert_eq!(
        update_field(&mut doc, "Full name", &changes).unwrap(),
        "Name"
    );
    let new_options = FieldChanges {
        options: Some(options(&["First", "Fourth"])),
        ..FieldChanges::default()
    };
    set_value(&mut doc, "Year", &FieldValue::Text("Second".into())).unwrap();
    update_field(&mut doc, "Year", &new_options).unwrap();

    let doc = round_trip(&mut doc);
    let all = fields(&doc).unwrap();
    assert!(all.iter().all(|field| field.name != "Full name"));
    let name = field(&all, "Name");
    assert!(close(
        name.widgets[0].rect,
        VisibleRect::new(100.0, 80.0, 250.0, 24.0)
    ));
    assert!(!name.required && name.read_only);
    assert_eq!(name.max_length, None);
    assert_eq!(name.tooltip, None);
    assert_eq!(name.alignment, Alignment::Center);
    assert_eq!(name.font_size, 14.0);
    let year = field(&all, "Year");
    assert_eq!(year.options.len(), 2);
    // "second" is no longer offered, so the answer was cleared
    assert_eq!(year.value, None);

    let mut doc = doc;
    let taken = FieldChanges {
        new_name: Some("Essay".into()),
        ..FieldChanges::default()
    };
    assert_eq!(
        update_field(&mut doc, "Name", &taken).unwrap_err(),
        FormError::NameTaken("Essay".into())
    );
}

#[test]
fn removed_fields_leave_the_form_and_the_page() {
    let mut doc = load(&sample_form(&PageSetup::letter(0)));
    remove_field(&mut doc, "colour").unwrap();
    remove_field(&mut doc, "address.street").unwrap();
    remove_field(&mut doc, "address.city").unwrap();
    remove_field(&mut doc, "name").unwrap();
    let doc = round_trip(&mut doc);
    let names: Vec<String> = fields(&doc)
        .unwrap()
        .into_iter()
        .map(|field| field.name)
        .collect();
    assert!(!names
        .iter()
        .any(|name| name == "colour" || name.starts_with("address") || name == "name"));
    assert!(names.iter().any(|name| name == "notes"));
    let page = doc.get_dictionary(doc.page_iter().next().unwrap()).unwrap();
    // 13 widgets in the sample; colour had 3, address 2, name 1
    assert_eq!(
        page.get(b"Annots").unwrap().as_array().unwrap().len(),
        15 - 6
    );
    assert_eq!(
        remove_field(&mut doc.clone(), "colour").unwrap_err(),
        FormError::UnknownField("colour".into())
    );
}

#[test]
fn added_fields_on_turned_pages_show_their_answers_in_place() {
    for rotate in [90, 180, 270] {
        let mut doc = blank(rotate);
        let rect = VisibleRect::new(100.0, 150.0, 220.0, 30.0);
        add_field(
            &mut doc,
            &spec(
                "answer",
                rect,
                NewFieldKind::Text {
                    max_length: None,
                    comb: false,
                },
            ),
        )
        .unwrap();
        let check = VisibleRect::new(400.0, 150.0, 20.0, 20.0);
        add_field(
            &mut doc,
            &spec("tick", check, NewFieldKind::Checkbox { on_value: None }),
        )
        .unwrap();
        set_value(&mut doc, "answer", &FieldValue::Text("MMMMMMMM".into())).unwrap();
        set_value(&mut doc, "tick", &FieldValue::Checked(true)).unwrap();
        let bytes = save(&mut doc);
        let all = fields(&load(&bytes)).unwrap();
        assert!(
            close(field(&all, "answer").widgets[0].rect, rect),
            "{rotate}"
        );

        let image = render_page(&bytes, 0, 2000, &format!("added-turned-{rotate}"));
        assert!(
            image.ink_in(rect) > 300,
            "no answer drawn at {rotate} degrees"
        );
        // the text is upright: its ink is wider than it is tall, as a line of text is
        let (left, right) = ink_extent_x(&image, rect);
        assert!(
            right - left > 60.0,
            "text at {rotate} degrees spans only {left}..{right}"
        );
        assert!(
            image.ink_in(check) > 50,
            "no tick drawn at {rotate} degrees"
        );
        // nothing spills outside the field
        let outside = VisibleRect::new(rect.x, rect.bottom() + 2.0, rect.width, 40.0);
        assert_eq!(image.ink_in(outside), 0);
    }
}

/// the leftmost and rightmost columns (in points) with ink inside a rect.
fn ink_extent_x(image: &support::PageImage, rect: VisibleRect) -> (f32, f32) {
    let mut columns = Vec::new();
    let mut x = rect.x;
    while x < rect.right() {
        if image.ink_in(VisibleRect::new(x, rect.y, 1.0, rect.height)) > 0 {
            columns.push(x);
        }
        x += 1.0;
    }
    (
        *columns.first().unwrap_or(&0.0),
        *columns.last().unwrap_or(&0.0),
    )
}
