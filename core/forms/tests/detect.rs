//! finding fields on flat forms drawn by hand, upright and on turned pages.

mod support;

use letters_forms::{
    add_field, detect_fields, Alignment, DetectedField, DetectedKind, NewField, NewFieldKind,
    VisibleRect,
};
use support::{load, plain_page_pdf, PageSetup};

/// a circle of radius r around (cx, cy), drawn with four curves.
fn circle(cx: f32, cy: f32, r: f32) -> String {
    let k = 0.5523 * r;
    format!(
        "{} {cy} m {} {} {} {} {cx} {} c {} {} {} {} {} {cy} c {} {} {} {} {cx} {} c {} {} {} {} {} {cy} c S\n",
        cx + r,
        cx + r, cy + k, cx + k, cy + r, cy + r,
        cx - k, cy + r, cx - r, cy + k, cx - r,
        cx - r, cy - k, cx - k, cy - r, cy - r,
        cx + k, cy - r, cx + r, cy - k, cx + r,
    )
}

fn text(x: f32, baseline: f32, size: f32, words: &str) -> String {
    format!("BT /F1 {size} Tf {x} {baseline} Td ({words}) Tj ET\n")
}

/// a small flat form. every position is given as the distance from the
/// top of the page (`top` is the page's visible height), so the same
/// layout can be drawn upright on a turned page.
fn flat_form(top: f32) -> String {
    let y = |from_top: f32| top - from_top;
    let mut out = String::new();
    // a line to write on, with its label on the left
    out += &text(72.0, y(94.0), 11.0, "Name:");
    out += &format!("0.8 w 110 {} m 300 {} l S\n", y(96.0), y(96.0));
    // underscores typed as text
    out += &text(72.0, y(130.0), 11.0, "Email: ____________________");
    // a checkbox with its label on the right
    out += &format!("72 {} 10 10 re S\n", y(170.0));
    out += &text(88.0, y(168.0), 10.0, "I agree to the rules");
    // two radio buttons in a row
    out += &circle(80.0, y(200.0), 5.0);
    out += &text(90.0, y(203.0), 10.0, "Yes");
    out += &circle(160.0, y(200.0), 5.0);
    out += &text(170.0, y(203.0), 10.0, "No");
    // a comb of eight boxes with a label above
    out += &text(72.0, y(232.0), 10.0, "Student ID");
    for cell in 0..8 {
        out += &format!("{} {} 14 18 re S\n", 72 + 14 * cell, y(258.0));
    }
    // a two row table: labels on the left, empty cells on the right
    out += &format!("72 {} 468 60 re S\n", y(340.0));
    out += &format!("72 {} m 540 {} l S\n", y(310.0), y(310.0));
    out += &format!("200 {} m 200 {} l S\n", y(280.0), y(340.0));
    out += &text(78.0, y(300.0), 11.0, "Course");
    out += &text(78.0, y(330.0), 11.0, "Teacher");
    // underlined words, which are not a field
    out += &text(72.0, y(400.0), 11.0, "Important notice");
    out += &format!("0.5 w 72 {} m 150.8 {} l S\n", y(402.0), y(402.0));
    // a box with its label inside
    out += &format!("72 {} 328 24 re S\n", y(450.0));
    out += &text(78.0, y(442.0), 11.0, "City");
    out
}

/// the page content drawn turned so it reads upright on a page with
/// /Rotate `rotate` (the design is in the visible page's own frame).
fn upright_on(rotate: i32, content: &str) -> String {
    let matrix = match rotate {
        90 => "0 1 -1 0 612 0",
        180 => "-1 0 0 -1 612 792",
        270 => "0 -1 1 0 0 792",
        _ => "1 0 0 1 0 0",
    };
    format!("q {matrix} cm\n{content}Q\n")
}

fn detect(rotate: i32) -> Vec<DetectedField> {
    let top = if rotate % 180 == 0 { 792.0 } else { 612.0 };
    let mut page = PageSetup::letter(rotate);
    page.content = upright_on(rotate, &flat_form(top));
    let doc = load(&plain_page_pdf(&page));
    detect_fields(&doc, 0).unwrap()
}

/// the expected proposals: kind, rect and label.
fn expected() -> Vec<(DetectedKind, VisibleRect, &'static str)> {
    vec![
        (
            DetectedKind::Text,
            VisibleRect::new(110.0, 78.0, 190.0, 18.0),
            "Name",
        ),
        (
            DetectedKind::Text,
            VisibleRect::new(105.6, 118.3, 122.3, 14.0),
            "Email",
        ),
        (
            DetectedKind::Checkbox,
            VisibleRect::new(72.0, 160.0, 10.0, 10.0),
            "I agree to the rules",
        ),
        (
            DetectedKind::Radio,
            VisibleRect::new(75.0, 195.0, 10.0, 10.0),
            "Yes",
        ),
        (
            DetectedKind::Radio,
            VisibleRect::new(155.0, 195.0, 10.0, 10.0),
            "No",
        ),
        (
            DetectedKind::Comb,
            VisibleRect::new(72.0, 240.0, 112.0, 18.0),
            "Student ID",
        ),
        (
            DetectedKind::Text,
            VisibleRect::new(201.0, 281.0, 338.0, 28.0),
            "Course",
        ),
        (
            DetectedKind::Text,
            VisibleRect::new(201.0, 311.0, 338.0, 28.0),
            "Teacher",
        ),
        (
            DetectedKind::Text,
            VisibleRect::new(99.9, 427.0, 299.1, 22.0),
            "City",
        ),
    ]
}

/// within two points on every side: close enough to snap a field to.
fn near(a: &VisibleRect, b: &VisibleRect) -> bool {
    let tolerance = 2.0;
    (a.x - b.x).abs() <= tolerance
        && (a.y - b.y).abs() <= tolerance
        && (a.right() - b.right()).abs() <= tolerance
        && (a.bottom() - b.bottom()).abs() <= tolerance
}

fn assert_finds_the_flat_form(found: &[DetectedField], rotate: i32) {
    let summary: Vec<String> = found
        .iter()
        .map(|field| format!("{:?} {:?} {:?}", field.kind, field.rect, field.label))
        .collect();
    assert_eq!(
        found.len(),
        expected().len(),
        "at {rotate} degrees found:\n{}",
        summary.join("\n")
    );
    for (kind, rect, label) in expected() {
        let hit = found
            .iter()
            .find(|field| field.kind == kind && near(&field.rect, &rect))
            .unwrap_or_else(|| {
                panic!(
                    "at {rotate} degrees, no {kind:?} near {rect:?}; found:\n{}",
                    summary.join("\n")
                )
            });
        assert_eq!(hit.label.as_deref(), Some(label), "label of {rect:?}");
    }
}

#[test]
fn finds_lines_boxes_circles_combs_and_cells() {
    let found = detect(0);
    assert_finds_the_flat_form(&found, 0);
    let comb = found
        .iter()
        .find(|field| field.kind == DetectedKind::Comb)
        .unwrap();
    assert_eq!(comb.max_length, Some(8));
    let radios: Vec<&DetectedField> = found
        .iter()
        .filter(|field| field.kind == DetectedKind::Radio)
        .collect();
    assert_eq!(radios[0].group, radios[1].group);
    assert_eq!(found[0].suggested_name, "Name");
}

#[test]
fn finds_the_same_fields_on_turned_pages() {
    for rotate in [90, 180, 270] {
        assert_finds_the_flat_form(&detect(rotate), rotate);
    }
}

/// the same kinds of field drawn the other ways pdf writers draw them:
/// a rule as a thin filled rectangle, a checkbox as four lines, a comb as
/// one box with dividers, and a table from loose lines with a header row.
fn other_drawings(top: f32) -> String {
    let y = |from_top: f32| top - from_top;
    let mut out = String::new();
    out += &text(72.0, y(98.0), 11.0, "Address");
    out += &format!("120 {} 200 0.75 re f\n", y(100.0));
    out += &format!(
        "72 {0} m 84 {0} l 84 {1} l 72 {1} l h S\n",
        y(140.0),
        y(128.0)
    );
    out += &text(90.0, y(138.0), 10.0, "Subscribe");
    out += &text(72.0, y(194.0), 11.0, "Code");
    out += &format!("140 {} 120 20 re S\n", y(200.0));
    for divider in 1..10 {
        let x = 140 + 12 * divider;
        out += &format!("{x} {} m {x} {} l S\n", y(180.0), y(200.0));
    }
    for line_y in [240.0, 260.0, 280.0] {
        out += &format!("72 {0} m 372 {0} l S\n", y(line_y));
    }
    for x in [72.0, 172.0, 372.0] {
        out += &format!("{x} {} m {x} {} l S\n", y(240.0), y(280.0));
    }
    out += &text(78.0, y(254.0), 11.0, "Item");
    out += &text(178.0, y(254.0), 11.0, "Notes");
    out
}

#[test]
fn finds_fields_drawn_in_other_ways() {
    for rotate in [0, 90] {
        let top = if rotate == 0 { 792.0 } else { 612.0 };
        let mut page = PageSetup::letter(rotate);
        page.content = upright_on(rotate, &other_drawings(top));
        let found = detect_fields(&load(&plain_page_pdf(&page)), 0).unwrap();
        let summary: Vec<String> = found
            .iter()
            .map(|f| format!("{:?} {:?} {:?}", f.kind, f.rect, f.label))
            .collect();
        let wanted = [
            (
                DetectedKind::Text,
                VisibleRect::new(120.0, 81.6, 200.0, 18.0),
                "Address",
            ),
            (
                DetectedKind::Checkbox,
                VisibleRect::new(72.0, 128.0, 12.0, 12.0),
                "Subscribe",
            ),
            (
                DetectedKind::Comb,
                VisibleRect::new(140.0, 180.0, 120.0, 20.0),
                "Code",
            ),
            (
                DetectedKind::Text,
                VisibleRect::new(73.0, 261.0, 98.0, 18.0),
                "Item",
            ),
            (
                DetectedKind::Text,
                VisibleRect::new(173.0, 261.0, 198.0, 18.0),
                "Notes",
            ),
        ];
        assert_eq!(
            found.len(),
            wanted.len(),
            "at {rotate}:\n{}",
            summary.join("\n")
        );
        for (kind, rect, label) in wanted {
            let hit = found
                .iter()
                .find(|field| field.kind == kind && near(&field.rect, &rect))
                .unwrap_or_else(|| {
                    panic!(
                        "no {kind:?} near {rect:?} at {rotate}:\n{}",
                        summary.join("\n")
                    )
                });
            assert_eq!(hit.label.as_deref(), Some(label));
        }
    }
}

/// labelled rules, a comb of separate boxes, and rows of checkboxes that
/// must stay checkboxes. positions are distances from the top.
fn rules_and_box_rows(top: f32) -> String {
    let y = |from_top: f32| top - from_top;
    let mut out = String::new();
    // a rule under the whole row, label included, drawn at the baseline
    out += &text(72.0, y(94.0), 11.0, "Parent or guardian:");
    out += &format!("0.5 w 72 {0} m 250 {0} l S\n", y(94.0));
    // a rule that starts right after the colon
    out += &text(72.0, y(140.0), 11.0, "Signature:");
    out += &format!("124 {0} m 300 {0} l S\n", y(140.0));
    // typed underscores after a colon
    out += &text(72.0, y(180.0), 11.0, "Phone: ______");
    // six separate boxes with even gaps: one comb
    out += &text(72.0, y(226.0), 11.0, "Postcode");
    for cell in 0..6 {
        out += &format!("{} {} 14 16 re S\n", 140 + 20 * cell, y(230.0));
    }
    // checkboxes with a one letter label between each
    for (index, letter) in ["a", "b", "c", "d"].iter().enumerate() {
        let x = 72.0 + 24.0 * index as f32;
        out += &format!("{x} {} 12 12 re S\n", y(272.0));
        out += &text(x + 15.0, y(270.0), 10.0, letter);
    }
    // a rating scale: numbers over evenly spaced boxes
    for index in 0..5 {
        let x = 72.0 + 22.0 * index as f32;
        out += &text(x + 3.5, y(300.0), 9.0, &(index + 1).to_string());
        out += &format!("{x} {} 12 12 re S\n", y(318.0));
    }
    // evenly spaced boxes with a word right after the last one
    for index in 0..4 {
        out += &format!("{} {} 12 12 re S\n", 72 + 20 * index, y(352.0));
    }
    out += &text(148.0, y(350.0), 10.0, "Weekly");
    out
}

#[test]
fn finds_labelled_rules_and_combs_with_gaps_but_keeps_checkbox_rows() {
    for rotate in [0, 90] {
        let top = if rotate == 0 { 792.0 } else { 612.0 };
        let mut page = PageSetup::letter(rotate);
        page.content = upright_on(rotate, &rules_and_box_rows(top));
        let found = detect_fields(&load(&plain_page_pdf(&page)), 0).unwrap();
        let summary: Vec<String> = found
            .iter()
            .map(|f| format!("{:?} {:?} {:?} {:?}", f.kind, f.rect, f.label, f.max_length))
            .collect();
        let summary = summary.join("\n");
        let wanted = [
            (
                DetectedKind::Text,
                VisibleRect::new(169.2, 76.0, 80.8, 18.0),
                "Parent or guardian",
            ),
            (
                DetectedKind::Text,
                VisibleRect::new(124.0, 122.0, 176.0, 18.0),
                "Signature",
            ),
            (
                DetectedKind::Text,
                VisibleRect::new(109.9, 168.3, 36.7, 14.0),
                "Phone",
            ),
            (
                DetectedKind::Comb,
                VisibleRect::new(137.0, 214.0, 120.0, 16.0),
                "Postcode",
            ),
        ];
        for (kind, rect, label) in wanted {
            let hit = found
                .iter()
                .find(|field| field.kind == kind && near(&field.rect, &rect))
                .unwrap_or_else(|| panic!("no {kind:?} near {rect:?} at {rotate}:\n{summary}"));
            assert_eq!(hit.label.as_deref(), Some(label), "at {rotate}:\n{summary}");
        }
        let comb = found
            .iter()
            .find(|field| field.kind == DetectedKind::Comb)
            .unwrap();
        assert_eq!(comb.max_length, Some(6));
        let combs = found
            .iter()
            .filter(|field| field.kind == DetectedKind::Comb)
            .count();
        assert_eq!(
            combs, 1,
            "only the postcode is a comb at {rotate}:\n{summary}"
        );
        let checkboxes: Vec<&DetectedField> = found
            .iter()
            .filter(|field| field.kind == DetectedKind::Checkbox)
            .collect();
        assert_eq!(checkboxes.len(), 4 + 5 + 4, "at {rotate}:\n{summary}");
        let lettered: Vec<&str> = checkboxes
            .iter()
            .filter(|field| (field.rect.y - 260.0).abs() < 1.0)
            .filter_map(|field| field.label.as_deref())
            .collect();
        assert_eq!(lettered, ["a", "b", "c", "d"]);
        assert_eq!(found.len(), 4 + 13, "at {rotate}:\n{summary}");
    }
}

#[test]
fn proposes_nothing_on_a_page_of_plain_text() {
    let mut page = PageSetup::letter(0);
    let mut content = text(72.0, 740.0, 18.0, "A History of Letters");
    for line in 0..30 {
        content += &text(
            72.0,
            700.0 - 16.0 * line as f32,
            11.0,
            "Students wrote letters by hand for centuries, and some still do (in ink).",
        );
    }
    page.content = content;
    let doc = load(&plain_page_pdf(&page));
    assert!(detect_fields(&doc, 0).unwrap().is_empty());
}

#[test]
fn ignores_places_that_already_have_fields() {
    let mut page = PageSetup::letter(0);
    page.content = flat_form(792.0);
    let mut doc = load(&plain_page_pdf(&page));
    let name_field = NewField {
        name: "Name".into(),
        page: 0,
        rect: VisibleRect::new(110.0, 78.0, 190.0, 18.0),
        kind: NewFieldKind::Text {
            max_length: None,
            comb: false,
        },
        tooltip: None,
        required: false,
        read_only: false,
        font_size: 0.0,
        alignment: Alignment::Left,
    };
    add_field(&mut doc, &name_field).unwrap();
    let found = detect_fields(&doc, 0).unwrap();
    assert_eq!(found.len(), expected().len() - 1);
    assert!(found
        .iter()
        .all(|field| field.label.as_deref() != Some("Name")));
}

#[test]
fn proposals_serialise_for_the_ui() {
    let found = detect(0);
    let json = serde_json::to_value(&found[5]).unwrap();
    assert_eq!(json["kind"], "comb");
    assert_eq!(json["maxLength"], 8);
    assert_eq!(json["suggestedName"], "Student ID");
}
