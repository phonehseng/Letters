//! building bigger forms: tab order, groups, fields on several pages,
//! export and default values, reset and print buttons, field looks, and
//! form data out and back in as fdf, xfdf, csv and json.

mod support;

use letters_forms::{
    add_button, add_field, duplicate_across_pages, export_data, fields, fill_field, group_fields,
    import_data, merge_form_data, reset_form, set_default_value, set_export_value,
    set_field_appearance, set_field_logic, set_tab_order, set_value, tab_order, Aggregate,
    Alignment, BorderStyle, ButtonAction, Calculation, DataFormat, FieldAppearance, FieldFont,
    FieldLogicChanges, FieldValue, FilledCopy, NewButton, NewField, NewFieldKind, TabOrder,
    VisibleRect,
};
use lopdf::Document;
use support::{load, round_trip, sample_form, save, PageSetup, RawPdf};

fn sample() -> Document {
    load(&sample_form(&PageSetup::letter(0)))
}

fn two_pages() -> Document {
    let mut pdf = RawPdf::new();
    let catalog = pdf.add("<< /Type /Catalog /Pages 2 0 R >>");
    pdf.add("<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>");
    pdf.add("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>");
    pdf.add("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>");
    load(&pdf.finish(catalog))
}

fn text_field(name: &str, page: u32, x: f32, y: f32) -> NewField {
    NewField {
        name: name.into(),
        page,
        rect: VisibleRect::new(x, y, 100.0, 20.0),
        kind: NewFieldKind::Text {
            max_length: None,
            comb: false,
        },
        tooltip: None,
        required: false,
        read_only: false,
        font_size: 0.0,
        alignment: Alignment::Left,
    }
}

fn value_of(doc: &Document, name: &str) -> Option<String> {
    fields(doc)
        .unwrap()
        .into_iter()
        .find(|field| field.name == name)
        .unwrap()
        .value
}

#[test]
fn tab_order_follows_rows_columns_or_a_list() {
    let mut doc = two_pages();
    // a grid: top row a b, bottom row c d
    add_field(&mut doc, &text_field("d", 0, 300.0, 200.0)).unwrap();
    add_field(&mut doc, &text_field("a", 0, 72.0, 100.0)).unwrap();
    add_field(&mut doc, &text_field("c", 0, 72.0, 200.0)).unwrap();
    add_field(&mut doc, &text_field("b", 0, 300.0, 100.0)).unwrap();
    set_tab_order(&mut doc, 0, &TabOrder::Rows).unwrap();
    assert_eq!(tab_order(&doc, 0).unwrap(), vec!["a", "b", "c", "d"]);
    set_tab_order(&mut doc, 0, &TabOrder::Columns).unwrap();
    assert_eq!(tab_order(&doc, 0).unwrap(), vec!["a", "c", "b", "d"]);
    let manual = TabOrder::Manual {
        fields: vec!["d".into(), "a".into()],
    };
    set_tab_order(&mut doc, 0, &manual).unwrap();
    let doc = round_trip(&mut doc);
    assert_eq!(tab_order(&doc, 0).unwrap(), vec!["d", "a", "c", "b"]);
    let page = doc.get_dictionary(doc.get_pages()[&1]).unwrap();
    assert!(
        !page.has(b"Tabs"),
        "a manual order follows the annotation list"
    );
}

#[test]
fn grouped_fields_share_a_name() {
    let mut doc = two_pages();
    add_field(&mut doc, &text_field("street", 0, 72.0, 100.0)).unwrap();
    add_field(&mut doc, &text_field("city", 0, 72.0, 130.0)).unwrap();
    let names = group_fields(&mut doc, "home", &["street".into(), "city".into()]).unwrap();
    assert_eq!(names, vec!["home.street", "home.city"]);
    let mut doc = round_trip(&mut doc);
    set_value(&mut doc, "home.city", &FieldValue::Text("Dunedin".into())).unwrap();
    assert_eq!(value_of(&doc, "home.city").as_deref(), Some("Dunedin"));
    assert!(group_fields(&mut doc, "again", &["home.city".into()]).is_err());
}

#[test]
fn a_field_shown_on_every_page_shares_its_answer() {
    let mut doc = two_pages();
    add_field(&mut doc, &text_field("student", 0, 72.0, 40.0)).unwrap();
    assert_eq!(
        duplicate_across_pages(&mut doc, "student", &[0, 1]).unwrap(),
        vec![1]
    );
    set_value(&mut doc, "student", &FieldValue::Text("Ada".into())).unwrap();
    let doc = round_trip(&mut doc);
    let info = fields(&doc)
        .unwrap()
        .into_iter()
        .find(|f| f.name == "student")
        .unwrap();
    assert_eq!(info.widgets.len(), 2);
    assert_eq!(info.widgets[1].page, Some(1));
    assert_eq!(info.widgets[0].rect, info.widgets[1].rect);
    assert_eq!(info.value.as_deref(), Some("Ada"));
}

#[test]
fn export_and_default_values_and_reset() {
    let mut doc = sample();
    set_export_value(&mut doc, "agree", 0, "I agree").unwrap();
    set_value(&mut doc, "agree", &FieldValue::Checked(true)).unwrap();
    assert_eq!(value_of(&doc, "agree").as_deref(), Some("I agree"));
    set_export_value(&mut doc, "colour", 1, "Verde").unwrap();
    set_value(&mut doc, "colour", &FieldValue::Text("Verde".into())).unwrap();
    assert_eq!(value_of(&doc, "colour").as_deref(), Some("Verde"));

    set_default_value(
        &mut doc,
        "name",
        Some(&FieldValue::Text("Your name".into())),
    )
    .unwrap();
    set_value(&mut doc, "name", &FieldValue::Text("Ada".into())).unwrap();
    let reset = reset_form(&mut doc, None).unwrap();
    assert!(reset.contains(&"name".to_owned()));
    let doc = round_trip(&mut doc);
    assert_eq!(value_of(&doc, "name").as_deref(), Some("Your name"));
    assert_eq!(value_of(&doc, "agree"), None);
    assert_eq!(value_of(&doc, "colour"), None);
}

#[test]
fn reset_and_print_buttons_use_standard_actions() {
    let mut doc = sample();
    let reset = NewButton {
        name: "Clear".into(),
        page: 0,
        rect: VisibleRect::new(400.0, 700.0, 80.0, 24.0),
        label: "Clear form".into(),
        action: ButtonAction::ResetForm { fields: None },
    };
    let print = NewButton {
        name: "Print".into(),
        label: "Print".into(),
        action: ButtonAction::Print,
        ..reset.clone()
    };
    assert_eq!(add_button(&mut doc, &reset).unwrap(), "Clear");
    add_button(&mut doc, &print).unwrap();
    let bytes = save(&mut doc);
    let file = String::from_utf8_lossy(&bytes);
    assert!(file.contains("/ResetForm"));
    assert!(file.contains("/Named") && file.contains("/Print"));
    assert!(!file.contains("/SubmitForm"));
    let doc = load(&bytes);
    let clear = fields(&doc)
        .unwrap()
        .into_iter()
        .find(|f| f.name == "Clear")
        .unwrap();
    assert_eq!(clear.kind, letters_forms::FieldKind::Button);
}

#[test]
fn field_looks_change_the_drawing() {
    let mut doc = sample();
    let look = FieldAppearance {
        border_color: Some(vec![0.0, 0.0, 1.0]),
        fill_color: Some(vec![1.0, 1.0, 0.8]),
        border_width: Some(2.0),
        border_style: Some(BorderStyle::Dashed),
        font: Some(FieldFont::Courier),
        font_size: Some(9.0),
        text_color: Some(vec![0.5]),
        alignment: Some(Alignment::Right),
        multiline: Some(true),
        password: None,
        comb: None,
        rich_text_off: true,
    };
    set_field_appearance(&mut doc, "name", &look).unwrap();
    set_value(&mut doc, "name", &FieldValue::Text("Ada".into())).unwrap();
    let doc = round_trip(&mut doc);
    let info = fields(&doc)
        .unwrap()
        .into_iter()
        .find(|f| f.name == "name")
        .unwrap();
    assert_eq!(info.kind, letters_forms::FieldKind::MultilineText);
    assert_eq!(info.alignment, Alignment::Right);
    assert_eq!(info.font_size, 9.0);
    let bytes = save(&mut doc.clone());
    let file = String::from_utf8_lossy(&bytes);
    assert!(
        file.contains("/Cour 9 Tf 0.5 g"),
        "the field asks for courier"
    );
    assert!(file.contains("/Courier"), "courier is in the form's fonts");
    let bad = FieldAppearance {
        comb: Some(true),
        ..Default::default()
    };
    let mut doc = sample();
    assert!(
        set_field_appearance(&mut doc, "notes", &bad).is_err(),
        "a comb needs a length"
    );
}

fn filled_sample() -> Document {
    let mut doc = sample();
    set_value(
        &mut doc,
        "name",
        &FieldValue::Text("Ada, \"the first\"".into()),
    )
    .unwrap();
    set_value(
        &mut doc,
        "notes",
        &FieldValue::Text("line one\nline two".into()),
    )
    .unwrap();
    set_value(&mut doc, "agree", &FieldValue::Checked(true)).unwrap();
    set_value(&mut doc, "colour", &FieldValue::Text("Green".into())).unwrap();
    set_value(&mut doc, "country", &FieldValue::Text("nz".into())).unwrap();
    set_value(
        &mut doc,
        "subjects",
        &FieldValue::Selected(vec!["Art".into(), "Music".into()]),
    )
    .unwrap();
    set_value(&mut doc, "born", &FieldValue::Text("2001-02-03".into())).unwrap();
    set_value(
        &mut doc,
        "address.city",
        &FieldValue::Text("Ōtautahi & Dunedin".into()),
    )
    .ok();
    set_value(
        &mut doc,
        "address.street",
        &FieldValue::Text("1 <Main> St".into()),
    )
    .unwrap();
    doc
}

#[test]
fn form_data_round_trips_in_every_format() {
    let source = filled_sample();
    let expected: Vec<(String, Option<String>, Vec<String>)> = fields(&source)
        .unwrap()
        .into_iter()
        .map(|f| (f.name, f.value, f.selected))
        .collect();
    for format in [
        DataFormat::Fdf,
        DataFormat::Xfdf,
        DataFormat::Csv,
        DataFormat::Json,
    ] {
        let data = export_data(&source, format).unwrap();
        let mut empty = sample();
        let report = import_data(&mut empty, &data, format).unwrap();
        // the read-only field keeps its own value and is reported
        assert!(
            report.skipped.iter().any(|s| s.field == "locked"),
            "{format:?}: {:?}",
            report.skipped
        );
        let got: Vec<(String, Option<String>, Vec<String>)> = fields(&empty)
            .unwrap()
            .into_iter()
            .map(|f| (f.name, f.value, f.selected))
            .collect();
        assert_eq!(got, expected, "{format:?}");
    }
    let fdf = String::from_utf8_lossy(&export_data(&source, DataFormat::Fdf).unwrap()).into_owned();
    assert!(fdf.starts_with("%FDF-1.2"));
    assert!(fdf.contains("/Kids"), "address.city is written as a tree");
    let xfdf = String::from_utf8(export_data(&source, DataFormat::Xfdf).unwrap()).unwrap();
    assert!(xfdf.contains("<field name=\"street\">"));
    assert!(xfdf.contains("1 &lt;Main&gt; St"));
}

#[test]
fn imported_answers_pass_the_fields_checks() {
    let mut doc = sample();
    add_field(&mut doc, &text_field("a", 0, 400.0, 100.0)).unwrap();
    add_field(&mut doc, &text_field("b", 0, 400.0, 130.0)).unwrap();
    add_field(&mut doc, &text_field("sum", 0, 400.0, 160.0)).unwrap();
    set_field_logic(
        &mut doc,
        "sum",
        &FieldLogicChanges {
            calculation: Some(Calculation::Aggregate {
                function: Aggregate::Sum,
                fields: vec!["a".into(), "b".into()],
            }),
            ..Default::default()
        },
    )
    .unwrap();
    let data = br#"{"a": "2", "b": "40", "sum": "999", "nope": "x", "name": "this name is far too long for twenty"}"#;
    let report = import_data(&mut doc, data, DataFormat::Json).unwrap();
    assert_eq!(value_of(&doc, "sum").as_deref(), Some("42"));
    let reasons: Vec<(&str, &str)> = report
        .skipped
        .iter()
        .map(|s| (s.field.as_str(), s.reason.as_str()))
        .collect();
    assert!(reasons.contains(&("sum", "It is calculated from other fields.")));
    assert!(reasons.contains(&("nope", "This form has no field with that name.")));
    assert!(reasons.contains(&("name", "The field “name” takes at most 20 characters.")));
    fill_field(&mut doc, "a", &FieldValue::Text("3".into())).unwrap();
    assert_eq!(value_of(&doc, "sum").as_deref(), Some("43"));
}

#[test]
fn many_filled_copies_merge_into_one_table() {
    let mut first = filled_sample();
    let mut second = sample();
    set_value(&mut second, "name", &FieldValue::Text("Grace".into())).unwrap();
    let copies = vec![
        FilledCopy {
            label: "ada.pdf".into(),
            bytes: save(&mut first),
        },
        FilledCopy {
            label: "grace.pdf".into(),
            bytes: save(&mut second),
        },
        FilledCopy {
            label: "broken.pdf".into(),
            bytes: b"not a pdf".to_vec(),
        },
    ];
    let merged = merge_form_data(&copies);
    assert_eq!(merged.columns[0], "File");
    assert_eq!(merged.rows.len(), 2);
    assert_eq!(merged.skipped.len(), 1);
    let name_column = merged.columns.iter().position(|c| c == "name").unwrap();
    assert_eq!(merged.rows[0][name_column], "Ada, \"the first\"");
    assert_eq!(merged.rows[1][name_column], "Grace");
    assert!(merged.csv.starts_with("File,name,"));
    assert!(merged.csv.contains("\"Ada, \"\"the first\"\"\""));
    let subjects = merged.columns.iter().position(|c| c == "subjects").unwrap();
    assert_eq!(merged.rows[0][subjects], "Art; Music");
    // one row of the table fills a fresh copy again (the File column is ignored)
    let header = merged.csv.lines().next().unwrap();
    // (ada's notes hold a line break inside quotes, so rows are found by label)
    let row = merged
        .csv
        .lines()
        .find(|line| line.starts_with("grace.pdf"))
        .unwrap();
    let mut fresh = sample();
    import_data(
        &mut fresh,
        format!("{header}\r\n{row}\r\n").as_bytes(),
        DataFormat::Csv,
    )
    .unwrap();
    assert_eq!(value_of(&fresh, "name").as_deref(), Some("Grace"));
}
