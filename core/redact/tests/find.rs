//! finding text to offer for redaction: phrases and the built in patterns.

mod support;

use letters_redact::lopdf::Document;
use letters_redact::{find_text_areas, FindOptions, RedactError, TextQuery};
use support::*;

fn sample() -> Document {
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let lines = [
        "Contact: jane.doe@school.example.org or call +1 (555) 123-4567",
        "Student number 2024 5566 7788, room 101, year 2024",
        "Card 4111-1111-1111-1111 expires 12/30",
        "Mixed Case Phrase here and mixed case phrase there",
        "a phrase that breaks",
        "across two lines",
    ];
    let content: String = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                "BT /F1 11 Tf 72 {} Td ({}) Tj ET\n",
                700 - index * 20,
                literal(line)
            )
        })
        .collect();
    let (catalog, _) = add_pages(
        &mut pdf,
        vec![PageSpec::new(
            &format!("/Font << /F1 {font} 0 R >>"),
            &content,
        )],
        "",
    );
    Document::load_mem(&pdf.finish(catalog, "")).unwrap()
}

fn texts(query: TextQuery) -> Vec<String> {
    find_text_areas(&sample(), &query, &FindOptions::default())
        .unwrap()
        .into_iter()
        .map(|found| found.text)
        .collect()
}

#[test]
fn finds_email_addresses() {
    assert_eq!(
        texts(TextQuery::EmailAddresses),
        vec!["jane.doe@school.example.org"]
    );
}

#[test]
fn finds_phone_numbers() {
    let found = texts(TextQuery::PhoneNumbers);
    assert!(found.contains(&"+1 (555) 123-4567".to_owned()), "{found:?}");
    // a long number spaced like a phone number is offered too; offering
    // one box too many is safer than missing one
    assert!(
        !found
            .iter()
            .any(|text| text.contains("101") || text.contains("12/30")),
        "{found:?}"
    );
}

#[test]
fn finds_long_numbers_but_not_short_ones() {
    assert_eq!(
        texts(TextQuery::LongNumbers),
        vec!["2024 5566 7788", "4111-1111-1111-1111"]
    );
}

#[test]
fn phrases_can_ignore_case_and_cross_lines() {
    let any_case = TextQuery::Phrase {
        text: "mixed case phrase".into(),
        match_case: false,
    };
    assert_eq!(texts(any_case).len(), 2);
    let exact = TextQuery::Phrase {
        text: "mixed case phrase".into(),
        match_case: true,
    };
    assert_eq!(texts(exact).len(), 1);
    let across = TextQuery::Phrase {
        text: "breaks across".into(),
        match_case: true,
    };
    let found = find_text_areas(&sample(), &across, &FindOptions::default()).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rects.len(), 2, "one box per line");
}

#[test]
fn boxes_sit_on_the_text() {
    let query = TextQuery::Phrase {
        text: "Contact:".into(),
        match_case: true,
    };
    let found = find_text_areas(&sample(), &query, &FindOptions::default()).unwrap();
    let rect = found[0].rects[0];
    // the line's baseline is 92 points from the top; helvetica reaches
    // about 0.72 of the size above it and 0.21 below
    assert!((rect.x - 72.0).abs() < 0.01);
    assert!((rect.y - (92.0 - 0.718 * 11.0)).abs() < 0.01, "{rect:?}");
    assert!((rect.height - 0.925 * 11.0).abs() < 0.01, "{rect:?}");
}

#[test]
fn a_broken_pattern_is_explained() {
    let query = TextQuery::Pattern {
        pattern: "(unclosed".into(),
        match_case: false,
    };
    let error = find_text_areas(&sample(), &query, &FindOptions::default()).unwrap_err();
    assert!(matches!(error, RedactError::InvalidPattern(_)));
}

#[test]
fn queries_read_from_the_app_as_json() {
    let query: TextQuery =
        serde_json::from_str(r#"{"kind":"phrase","text":"room","matchCase":false}"#).unwrap();
    assert_eq!(texts(query), vec!["room"]);
    let query: TextQuery = serde_json::from_str(r#"{"kind":"longNumbers"}"#).unwrap();
    assert_eq!(texts(query).len(), 2);
}
