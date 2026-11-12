//! forms that calculate, format and check: hand-built forms using each
//! standard action, the order they run in, circles, formats with local
//! separators, refused answers, scripts left alone, and rules set up in
//! Letters read back unchanged after saving.

mod support;

use letters_forms::{
    add_field, fill_field, form_logic, recalculate, set_calculation_order, set_field_logic,
    Aggregate, Calculation, CalculationIssue, FieldFormat, FieldLogicChanges, FieldValue,
    FormError, NegativeStyle, NewField, NewFieldKind, SeparatorStyle, SpecialFormat, Validation,
    VisibleRect,
};
use lopdf::{Document, Object};
use support::{load, round_trip, RawPdf};

/// a one page form with a text field per entry, stacked down the page.
/// each entry is (name, extra dictionary entries such as /AA).
/// `order` names the fields listed in /CO.
fn form(entries: &[(&str, &str)], order: &[&str]) -> Document {
    let mut pdf = RawPdf::new();
    let catalog = pdf.add("<< /Type /Catalog /Pages 2 0 R /AcroForm 6 0 R >>");
    pdf.add("<< /Type /Pages /Count 1 /Kids [3 0 R] >>");
    let page = pdf.reserve(); // 3
    pdf.add_stream("", ""); // 4
    pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"); // 5
    let acroform = pdf.reserve(); // 6
    let mut numbers = Vec::new();
    for (index, (name, extra)) in entries.iter().enumerate() {
        let top = 760 - index as i32 * 30;
        numbers.push(pdf.add(format!(
            "<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /FT /Tx /T ({name}) /DA (/Helv 10 Tf 0 g) /Rect [100 {} 300 {top}] {extra} >>",
            top - 20
        )));
    }
    let refs = |list: &[usize]| {
        list.iter()
            .map(|number| format!("{number} 0 R"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let order_refs: Vec<usize> = order
        .iter()
        .map(|wanted| numbers[entries.iter().position(|(name, _)| name == wanted).unwrap()])
        .collect();
    pdf.set(
        page,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Annots [{}] >>",
            refs(&numbers)
        ),
    );
    pdf.set(
        acroform,
        format!(
            "<< /Fields [{}] /CO [{}] /DR << /Font << /Helv 5 0 R >> >> /DA (/Helv 0 Tf 0 g) >>",
            refs(&numbers),
            refs(&order_refs)
        ),
    );
    load(&pdf.finish(catalog))
}

fn js(key: &str, script: &str) -> String {
    format!("/{key} << /S /JavaScript /JS ({script}) >>")
}

fn aa(parts: &[String]) -> String {
    format!("/AA << {} >>", parts.join(" "))
}

fn calc(script: &str) -> String {
    aa(&[js("C", script)])
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_owned())
}

fn value_of(doc: &Document, name: &str) -> String {
    letters_forms::fields(doc)
        .unwrap()
        .into_iter()
        .find(|field| field.name == name)
        .unwrap()
        .value
        .unwrap_or_default()
}

/// the drawing of a field's (first) widget, as text.
fn drawing(doc: &Document, name: &str) -> String {
    let form = doc
        .catalog()
        .unwrap()
        .get(b"AcroForm")
        .unwrap()
        .as_reference()
        .unwrap();
    let fields = doc
        .get_dictionary(form)
        .unwrap()
        .get(b"Fields")
        .unwrap()
        .as_array()
        .unwrap();
    for field in fields {
        let dict = doc.get_dictionary(field.as_reference().unwrap()).unwrap();
        let own = match dict.get(b"T").unwrap() {
            Object::String(bytes, _) => String::from_utf8_lossy(bytes).into_owned(),
            _ => continue,
        };
        if own == name {
            let ap = dict.get(b"AP").unwrap().as_dict().unwrap();
            let stream = doc
                .get_object(ap.get(b"N").unwrap().as_reference().unwrap())
                .unwrap();
            let bytes = stream
                .as_stream()
                .unwrap()
                .decompressed_content()
                .unwrap_or_else(|_| stream.as_stream().unwrap().content.clone());
            return String::from_utf8_lossy(&bytes).into_owned();
        }
    }
    panic!("no field {name}");
}

#[test]
fn every_simple_calculation_works() {
    let numbers = r#"new Array("a", "b", "c")"#;
    let mut doc = form(
        &[
            ("a", ""),
            ("b", ""),
            ("c", ""),
            (
                "sum",
                &calc(&format!(r#"AFSimple_Calculate("SUM", {numbers});"#)),
            ),
            (
                "product",
                &calc(&format!(r#"AFSimple_Calculate("PRD", {numbers});"#)),
            ),
            (
                "average",
                &calc(&format!(r#"AFSimple_Calculate("AVG", {numbers});"#)),
            ),
            (
                "smallest",
                &calc(&format!(r#"AFSimple_Calculate("MIN", {numbers});"#)),
            ),
            ("largest", &calc(r#"AFSimple_Calculate("MAX", "a, b, c");"#)),
        ],
        &["sum", "product", "average", "smallest", "largest"],
    );
    let logic = form_logic(&doc).unwrap();
    assert_eq!(logic.calculation_order.len(), 5);
    assert!(logic.issues.is_empty(), "{:?}", logic.issues);
    assert_eq!(
        logic.fields[3].calculation,
        Some(Calculation::Aggregate {
            function: Aggregate::Sum,
            fields: vec!["a".into(), "b".into(), "c".into()]
        })
    );
    fill_field(&mut doc, "a", &text("2")).unwrap();
    fill_field(&mut doc, "b", &text("3.5")).unwrap();
    let report = fill_field(&mut doc, "c", &text("-1")).unwrap();
    assert_eq!(report.changed[0].name, "c");
    let doc = round_trip(&mut doc);
    assert_eq!(value_of(&doc, "sum"), "4.5");
    assert_eq!(value_of(&doc, "product"), "-7");
    assert_eq!(value_of(&doc, "average"), "1.5");
    assert_eq!(value_of(&doc, "smallest"), "-1");
    assert_eq!(value_of(&doc, "largest"), "3.5");
    // the drawing shows the calculated value too
    assert!(
        drawing(&doc, "sum").contains("(4.5) Tj"),
        "{}",
        drawing(&doc, "sum")
    );
}

#[test]
fn field_arithmetic_scripts_are_run() {
    let mut doc = form(
        &[
            ("price", ""),
            ("qty", ""),
            (
                "line",
                &calc(
                    r#"event.value = this.getField("price").value * this.getField("qty").value;"#,
                ),
            ),
            (
                "note",
                &calc(
                    r#"event.value = this.getField("line").value > 100 ? "Big order" : "Small order";"#,
                ),
            ),
            (
                "rounded",
                &calc(
                    r#"var t = AFMakeNumber(getField("line").value); if (t > 0) event.value = Math.round(t / 3); else event.value = "";"#,
                ),
            ),
        ],
        &["line", "note", "rounded"],
    );
    let logic = form_logic(&doc).unwrap();
    assert_eq!(
        logic.fields[2].calculation,
        Some(Calculation::Formula {
            formula: "price * qty".into()
        })
    );
    assert!(matches!(
        logic.fields[4].calculation,
        Some(Calculation::Script { .. })
    ));
    fill_field(&mut doc, "price", &text("12.5")).unwrap();
    let report = fill_field(&mut doc, "qty", &text("10")).unwrap();
    let changed: Vec<&str> = report.changed.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(changed, vec!["qty", "line", "note", "rounded"]);
    assert_eq!(value_of(&doc, "line"), "125");
    assert_eq!(value_of(&doc, "note"), "Big order");
    assert_eq!(value_of(&doc, "rounded"), "42");
}

#[test]
fn calculations_run_in_the_forms_order() {
    let entries = [
        ("a", String::new()),
        (
            "subtotal",
            calc(r#"AFSimple_Calculate("SUM", new Array("a"));"#),
        ),
        (
            "total",
            calc(r#"event.value = this.getField("subtotal").value * 2;"#),
        ),
    ];
    let entries: Vec<(&str, &str)> = entries.iter().map(|(n, e)| (*n, e.as_str())).collect();

    // the wrong order: total first, so it sees the old subtotal (as other apps do)
    let mut wrong = form(&entries, &["total", "subtotal"]);
    let logic = form_logic(&wrong).unwrap();
    assert_eq!(logic.calculation_order, vec!["total", "subtotal"]);
    assert!(logic.issues.iter().any(|issue| matches!(
        issue,
        CalculationIssue::OutOfOrder { field, reads, .. } if field == "total" && reads == "subtotal"
    )));
    fill_field(&mut wrong, "a", &text("5")).unwrap();
    assert_eq!(value_of(&wrong, "subtotal"), "5");
    assert_eq!(value_of(&wrong, "total"), "0");
    // the order can be fixed in Letters
    set_calculation_order(&mut wrong, &["subtotal".into(), "total".into()]).unwrap();
    recalculate(&mut wrong).unwrap();
    assert_eq!(value_of(&wrong, "total"), "10");
    assert!(form_logic(&wrong).unwrap().issues.is_empty());

    let mut right = form(&entries, &["subtotal", "total"]);
    fill_field(&mut right, "a", &text("5")).unwrap();
    assert_eq!(value_of(&right, "total"), "10");
}

#[test]
fn circles_are_found_and_left_alone() {
    let mut doc = form(
        &[
            ("x", &calc(r#"event.value = this.getField("y").value + 1;"#)),
            ("y", &calc(r#"event.value = this.getField("x").value + 1;"#)),
            (
                "self",
                &calc(r#"event.value = this.getField("self").value + 1;"#),
            ),
            ("a", ""),
            (
                "fine",
                &calc(r#"event.value = this.getField("a").value * 3;"#),
            ),
        ],
        &["x", "y", "self", "fine"],
    );
    let logic = form_logic(&doc).unwrap();
    let cycles: Vec<&Vec<String>> = logic
        .issues
        .iter()
        .filter_map(|issue| match issue {
            CalculationIssue::Cycle { fields, message } => {
                assert!(message.contains("in a circle"));
                Some(fields)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        cycles,
        vec![
            &vec!["x".to_owned(), "y".to_owned()],
            &vec!["self".to_owned()]
        ]
    );
    fill_field(&mut doc, "a", &text("2")).unwrap();
    assert_eq!(value_of(&doc, "fine"), "6");
    assert_eq!(value_of(&doc, "x"), "");
    assert_eq!(value_of(&doc, "self"), "");
}

#[test]
fn formats_keep_raw_values_and_show_formatted_ones() {
    let number = aa(&[
        js("F", r#"AFNumber_Format(2, 2, 0, 0, " EUR", false);"#),
        js("K", r#"AFNumber_Keystroke(2, 2, 0, 0, " EUR", false);"#),
    ]);
    let red = aa(&[js("F", r#"AFNumber_Format(2, 0, 3, 0, "$", true);"#)]);
    let percent = aa(&[
        js("F", "AFPercent_Format(1, 0);"),
        js("K", "AFPercent_Keystroke(1, 0);"),
    ]);
    let phone = aa(&[
        js("F", "AFSpecial_Format(2);"),
        js("K", "AFSpecial_Keystroke(2);"),
    ]);
    let mask = aa(&[js("K", r#"AFSpecial_KeystrokeEx("AA-9999");"#)]);
    let time = aa(&[
        js("F", "AFTime_Format(1);"),
        js("K", "AFTime_Keystroke(1);"),
    ]);
    let date = aa(&[js("F", r#"AFDate_FormatEx("dd.mm.yyyy");"#)]);
    let mut doc = form(
        &[
            ("amount", &number),
            ("balance", &red),
            ("rate", &percent),
            ("phone", &phone),
            ("code", &mask),
            ("start", &time),
            ("day", &date),
        ],
        &[],
    );
    let logic = form_logic(&doc).unwrap();
    assert_eq!(
        logic.fields[0].format,
        Some(FieldFormat::Number {
            decimals: 2,
            separator: SeparatorStyle::DotComma,
            negative: NegativeStyle::MinusSign,
            currency: " EUR".into(),
            currency_before: false
        })
    );
    let shown = |report: letters_forms::FillReport| report.changed[0].shown.clone();
    assert_eq!(
        shown(fill_field(&mut doc, "amount", &text("1.234,5")).unwrap()),
        "1.234,50 EUR"
    );
    assert_eq!(
        shown(fill_field(&mut doc, "balance", &text("-42")).unwrap()),
        "($42.00)"
    );
    assert_eq!(
        shown(fill_field(&mut doc, "rate", &text("12.5%")).unwrap()),
        "12.5%"
    );
    assert_eq!(
        shown(fill_field(&mut doc, "phone", &text("021 555 1234")).unwrap()),
        "(021) 555-1234"
    );
    assert_eq!(
        shown(fill_field(&mut doc, "code", &text("nz4021")).unwrap()),
        "nz-4021"
    );
    assert_eq!(
        shown(fill_field(&mut doc, "start", &text("14:30")).unwrap()),
        "2:30 pm"
    );
    assert_eq!(
        shown(fill_field(&mut doc, "day", &text("2026-11-12")).unwrap()),
        "12.11.2026"
    );

    let doc = round_trip(&mut doc);
    // the file keeps the raw values; the drawings show them formatted
    assert_eq!(value_of(&doc, "amount"), "1234.5");
    assert_eq!(value_of(&doc, "rate"), "0.125");
    assert_eq!(value_of(&doc, "phone"), "0215551234");
    assert!(
        drawing(&doc, "amount").contains("(1.234,50 EUR) Tj"),
        "{}",
        drawing(&doc, "amount")
    );
    assert!(
        drawing(&doc, "phone").contains("((021\\) 555-1234) Tj")
            || drawing(&doc, "phone").contains("(\\(021\\) 555-1234) Tj"),
        "{}",
        drawing(&doc, "phone")
    );
    // red parentheses are drawn red
    assert!(
        drawing(&doc, "balance").contains("1 0 0 rg"),
        "{}",
        drawing(&doc, "balance")
    );
    assert!(!drawing(&doc, "amount").contains("1 0 0 rg"));
}

#[test]
fn answers_that_fail_a_check_are_refused() {
    let range = aa(&[
        js("K", "AFNumber_Keystroke(0, 1, 0, 0, \"\", true);"),
        js("V", "AFRange_Validate(true, 1, true, 10);"),
    ]);
    let custom = aa(&[js(
        "V",
        r#"if (event.value > this.getField("budget").value) { app.alert("That is more than the budget."); event.rc = false; }"#,
    )]);
    let mut doc = form(
        &[("score", &range), ("budget", ""), ("spend", &custom)],
        &[],
    );
    let refused = fill_field(&mut doc, "score", &text("11")).unwrap_err();
    assert_eq!(refused.to_string(), "“score” must be between 1 and 10.");
    assert!(matches!(refused, FormError::AnswerRefused { .. }));
    let not_a_number = fill_field(&mut doc, "score", &text("lots")).unwrap_err();
    assert_eq!(
        not_a_number.to_string(),
        "“score” takes a number, such as 1234.50."
    );
    fill_field(&mut doc, "score", &text("7")).unwrap();
    assert_eq!(value_of(&doc, "score"), "7");

    fill_field(&mut doc, "budget", &text("100")).unwrap();
    let over = fill_field(&mut doc, "spend", &text("150")).unwrap_err();
    assert_eq!(over.to_string(), "That is more than the budget.");
    assert_eq!(value_of(&doc, "spend"), "", "nothing changes when refused");
    fill_field(&mut doc, "spend", &text("80")).unwrap();
    assert_eq!(value_of(&doc, "spend"), "80");
}

#[test]
fn scripts_outside_the_subset_are_reported_and_kept() {
    let hidden = calc(r#"this.getField("a").display = display.hidden; event.value = 1;"#);
    let looping = aa(&[js("V", "for (var i = 0; i < 10; i++) { app.beep(); }")]);
    let mut doc = form(&[("a", ""), ("odd", &hidden), ("loop", &looping)], &["odd"]);
    let logic = form_logic(&doc).unwrap();
    assert_eq!(logic.fields_with_custom_scripts, vec!["odd", "loop"]);
    assert_eq!(
        logic.custom_script_note.as_deref(),
        Some("This form has a custom script Letters doesn’t run: odd, loop.")
    );
    let report = fill_field(&mut doc, "loop", &text("anything")).unwrap();
    assert!(report.custom_script_note.is_some());
    let doc = round_trip(&mut doc);
    assert_eq!(value_of(&doc, "odd"), "", "the script was not run");
    // the scripts are still in the file, untouched
    let mut bytes = Vec::new();
    doc.clone().save_to(&mut bytes).unwrap();
    let file = String::from_utf8_lossy(&bytes);
    assert!(file.contains("display.hidden"));
    assert!(file.contains("app.beep"));
}

fn new_text(doc: &mut Document, name: &str, y: f32) {
    add_field(
        doc,
        &NewField {
            name: name.into(),
            page: 0,
            rect: VisibleRect::new(72.0, y, 150.0, 20.0),
            kind: NewFieldKind::Text {
                max_length: None,
                comb: false,
            },
            tooltip: None,
            required: false,
            read_only: false,
            font_size: 0.0,
            alignment: letters_forms::Alignment::Left,
        },
    )
    .unwrap();
}

#[test]
fn rules_set_up_in_letters_read_back_the_same() {
    let mut doc = form(&[("seed", "")], &[]);
    for (index, name) in [
        "price", "qty", "Line 1", "subtotal", "tax", "total", "rate", "phone", "when", "code",
        "budget", "spend",
    ]
    .iter()
    .enumerate()
    {
        new_text(&mut doc, name, 100.0 + index as f32 * 25.0);
    }
    let money = FieldFormat::Number {
        decimals: 2,
        separator: SeparatorStyle::CommaDot,
        negative: NegativeStyle::Parentheses,
        currency: "$".into(),
        currency_before: true,
    };
    let rules: Vec<(&str, FieldLogicChanges)> = vec![
        (
            "Line 1",
            FieldLogicChanges {
                calculation: Some(Calculation::Formula {
                    formula: "price*qty".into(),
                }),
                format: Some(money.clone()),
                validations: vec![],
            },
        ),
        (
            "subtotal",
            FieldLogicChanges {
                calculation: Some(Calculation::Aggregate {
                    function: Aggregate::Sum,
                    fields: vec!["Line 1".into()],
                }),
                format: Some(money.clone()),
                validations: vec![],
            },
        ),
        (
            "tax",
            FieldLogicChanges {
                calculation: Some(Calculation::Formula {
                    formula: "round(subtotal * 15%)".into(),
                }),
                ..Default::default()
            },
        ),
        (
            "total",
            FieldLogicChanges {
                calculation: Some(Calculation::Formula {
                    formula: "subtotal + tax".into(),
                }),
                format: Some(money.clone()),
                validations: vec![],
            },
        ),
        (
            "qty",
            FieldLogicChanges {
                validations: vec![
                    Validation::Range {
                        min: Some(1.0),
                        max: Some(99.0),
                    },
                    Validation::Required,
                    Validation::MaxLength { max: 2 },
                ],
                ..Default::default()
            },
        ),
        (
            "rate",
            FieldLogicChanges {
                format: Some(FieldFormat::Percent {
                    decimals: 1,
                    separator: SeparatorStyle::DotComma,
                }),
                ..Default::default()
            },
        ),
        (
            "phone",
            FieldLogicChanges {
                format: Some(FieldFormat::Special {
                    kind: SpecialFormat::Phone,
                }),
                ..Default::default()
            },
        ),
        (
            "when",
            FieldLogicChanges {
                format: Some(FieldFormat::Time {
                    pattern: "HH:MM".into(),
                }),
                ..Default::default()
            },
        ),
        (
            "code",
            FieldLogicChanges {
                format: Some(FieldFormat::Mask {
                    mask: "999-AAA".into(),
                }),
                ..Default::default()
            },
        ),
        (
            "spend",
            FieldLogicChanges {
                validations: vec![Validation::Condition {
                    condition: "value <= budget".into(),
                    message: Some("Keep it within the \"budget\".".into()),
                }],
                ..Default::default()
            },
        ),
    ];
    let mut written = Vec::new();
    for (name, changes) in &rules {
        written.push(set_field_logic(&mut doc, name, changes).unwrap());
    }
    // formulas come back in their tidy form
    assert_eq!(
        written[0].calculation,
        Some(Calculation::Formula {
            formula: "price * qty".into()
        })
    );
    assert_eq!(
        written[2].calculation,
        Some(Calculation::Formula {
            formula: "round(subtotal * 0.15)".into()
        })
    );

    let saved = round_trip(&mut doc);
    let logic = form_logic(&saved).unwrap();
    for model in &written {
        let read = logic
            .fields
            .iter()
            .find(|field| field.field == model.field)
            .unwrap();
        assert_eq!(read, model, "{}", model.field);
        assert!(read.unsupported.is_empty(), "{read:?}");
    }
    assert_eq!(
        logic.calculation_order,
        vec!["Line 1", "subtotal", "tax", "total"]
    );
    assert!(logic.issues.is_empty(), "{:?}", logic.issues);

    // the file holds the standard actions other apps run
    let mut bytes = Vec::new();
    saved.clone().save_to(&mut bytes).unwrap();
    let file = String::from_utf8_lossy(&bytes);
    for expected in [
        "AFSimple_Calculate",
        "AFNumber_Format(2, 0, 2, 0",
        "AFNumber_Keystroke",
        "AFPercent_Format(1, 2)",
        "AFSpecial_Format(2)",
        "AFTime_Format(0)",
        "AFSpecial_KeystrokeEx",
        "AFRange_Validate(true, 1, true, 99)",
        "BVCALC price * qty EVCALC",
        "event.rc = false",
    ] {
        assert!(file.contains(expected), "{expected} missing");
    }

    // and the form works
    let mut doc = saved;
    fill_field(&mut doc, "price", &text("19.99")).unwrap();
    let report = fill_field(&mut doc, "qty", &text("3")).unwrap();
    assert_eq!(value_of(&doc, "Line 1"), "59.97");
    assert_eq!(value_of(&doc, "tax"), "9");
    assert_eq!(value_of(&doc, "total"), "68.97");
    let total = report.changed.iter().find(|c| c.name == "total").unwrap();
    assert_eq!(total.shown, "$68.97");
    assert!(fill_field(&mut doc, "qty", &text("100")).is_err());
    fill_field(&mut doc, "budget", &text("10")).unwrap();
    assert_eq!(
        fill_field(&mut doc, "spend", &text("11"))
            .unwrap_err()
            .to_string(),
        "Keep it within the \"budget\"."
    );

    // removing the rules removes the actions and the place in /CO
    set_field_logic(&mut doc, "tax", &FieldLogicChanges::default()).unwrap();
    let logic = form_logic(&doc).unwrap();
    assert_eq!(logic.calculation_order, vec!["Line 1", "subtotal", "total"]);
    assert!(logic.issues.is_empty(), "{:?}", logic.issues);
}

#[test]
fn bad_rules_are_refused_with_sentences() {
    let mut doc = form(&[("a", "")], &[]);
    let unknown = set_field_logic(
        &mut doc,
        "a",
        &FieldLogicChanges {
            calculation: Some(Calculation::Formula {
                formula: "b * 2".into(),
            }),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        unknown.to_string(),
        "There is no field called “b” in this form."
    );
    let broken = set_field_logic(
        &mut doc,
        "a",
        &FieldLogicChanges {
            calculation: Some(Calculation::Formula {
                formula: "2 *".into(),
            }),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(
        broken
            .to_string()
            .starts_with("Letters can’t read this formula"),
        "{broken}"
    );
    let two = set_field_logic(
        &mut doc,
        "a",
        &FieldLogicChanges {
            validations: vec![
                Validation::Range {
                    min: None,
                    max: Some(1.0),
                },
                Validation::Condition {
                    condition: "value > 0".into(),
                    message: None,
                },
            ],
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(two, FormError::TooManyChecks);
}
