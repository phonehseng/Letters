//! highlight, underline, strike-out and squiggly: the dictionary written,
//! and the pixels pdfium draws, on every kind of page layout.

mod support;

use letters_annotate::{
    add_text_markup, flatten, list, AnnotateError, AnnotationDetails, AnnotationKind, Color,
    FlattenScope, Rect, TextMarkupKind, TextMarkupParams,
};
use lopdf::{Dictionary, Object};
use support::*;

const YELLOW: Color = Color::HIGHLIGHT_YELLOW;
const RED: Color = Color::new(255, 0, 0);

fn markup(kind: TextMarkupKind, rects: Vec<Rect>, color: Color) -> TextMarkupParams {
    TextMarkupParams {
        kind,
        rects,
        color,
        opacity: 1.0,
        contents: Some("check this".into()),
        author: Some("Ada".into()),
    }
}

/// the first pixel inside `rect` (shrunk a little) that passes `test`.
fn find_pixel(image: &Image, rect: Rect, test: impl Fn([u8; 3]) -> bool) -> Option<(f32, f32)> {
    let mut y = rect.y + 2.0;
    while y < rect.bottom() - 2.0 {
        let mut x = rect.x + 2.0;
        while x < rect.right() - 2.0 {
            if test(image.pixel(x, y)) {
                return Some((x, y));
            }
            x += 1.0;
        }
        y += 1.0;
    }
    None
}

#[test]
fn highlights_land_on_the_selected_text_in_every_page_layout() {
    for (layout, page) in page_variants() {
        let bytes = pdf_bytes(&[page], false);
        let rects = find_text(&bytes, 0, "MMMMMMMMMM");
        assert_eq!(
            rects.len(),
            1,
            "{layout}: the test text should be found once"
        );
        let text = rects[0];
        let before = render(&bytes, 0);

        let mut doc = load(&bytes);
        let info = add_text_markup(
            &mut doc,
            0,
            &markup(TextMarkupKind::Highlight, rects.clone(), YELLOW),
        )
        .unwrap();
        let saved = save(&mut doc);
        let reloaded = load(&saved);

        // the dictionary, after a save and reload
        let annotation = annotation_by_name(&reloaded, 0, &info.id);
        assert_eq!(name_of(&annotation, b"Subtype"), "Highlight", "{layout}");
        assert_eq!(numbers_of(&annotation, b"QuadPoints").len(), 8, "{layout}");
        let color = numbers_of(&annotation, b"C");
        assert!(
            (color[0] - 1.0).abs() < 0.01 && (color[1] - 0.831).abs() < 0.01 && color[2] == 0.0
        );
        assert!(
            has_normal_appearance(&reloaded, &annotation),
            "{layout}: appearance missing"
        );
        assert_eq!(
            annotation.get(b"F").unwrap().as_i64().unwrap(),
            4,
            "{layout}: print flag"
        );
        assert!(annotation.has(b"M") && annotation.has(b"CreationDate") && annotation.has(b"P"));

        // what the app reads back
        let listed = list(&reloaded, 0).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, info.id);
        assert_eq!(listed[0].kind, AnnotationKind::Highlight);
        assert_eq!(listed[0].contents.as_deref(), Some("check this"));
        assert_eq!(listed[0].author.as_deref(), Some("Ada"));
        let shown = listed[0].rect;
        assert!(
            (shown.x - (text.x - 1.0)).abs() < 0.05,
            "{layout}: rect {shown:?} vs text {text:?}"
        );
        assert!((shown.width - (text.width + 2.0)).abs() < 0.05, "{layout}");
        let AnnotationDetails::TextMarkup { quads } = &listed[0].details else {
            panic!("{layout}: wrong details");
        };
        assert!((quads[0][0].x - text.x).abs() < 0.05 && (quads[0][0].y - text.y).abs() < 0.05);
        assert!(
            (quads[0][3].x - text.right()).abs() < 0.05
                && (quads[0][3].y - text.bottom()).abs() < 0.05
        );

        // the pixels: paper under the highlight turns yellow, the letters
        // stay dark because the highlight multiplies, and nothing outside changes
        let after = render(&saved, 0);
        let (paper_x, paper_y) =
            find_pixel(&before, text, |pixel| pixel == WHITE).expect("paper inside the text box");
        assert_near(after.pixel(paper_x, paper_y), [255, 212, 0], 12, layout);
        let (ink_x, ink_y) =
            find_pixel(&before, text, is_dark).expect("letters inside the text box");
        assert!(
            is_dark(after.pixel(ink_x, ink_y)),
            "{layout}: text under the highlight should stay dark"
        );
        let center = text.center_point();
        assert_eq!(
            after.pixel(text.x - 6.0, center.1),
            WHITE,
            "{layout}: left of the mark"
        );
        assert_eq!(
            after.pixel(center.0, text.bottom() + 6.0),
            WHITE,
            "{layout}: below the mark"
        );
        assert_eq!(
            after.pixel(center.0, text.y - 6.0),
            WHITE,
            "{layout}: above the mark"
        );
    }
}

trait Center {
    fn center_point(&self) -> (f32, f32);
}

impl Center for Rect {
    fn center_point(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

#[test]
fn several_lines_become_several_quads() {
    let bytes = pdf_bytes(&[TestPage::letter()], false);
    let mut doc = load(&bytes);
    let lines = vec![
        Rect::new(100.0, 100.0, 300.0, 14.0),
        Rect::new(100.0, 116.0, 120.0, 14.0),
    ];
    let info = add_text_markup(
        &mut doc,
        0,
        &markup(TextMarkupKind::Highlight, lines, YELLOW),
    )
    .unwrap();
    let reloaded = reload(&mut doc);
    let annotation = annotation_by_name(&reloaded, 0, &info.id);
    let quads = numbers_of(&annotation, b"QuadPoints");
    assert_eq!(quads.len(), 16);
    // first quad, top left then top right, in pdf space (y up)
    assert_eq!(&quads[0..4], &[100.0, 692.0, 400.0, 692.0]);

    let image = render(&save(&mut doc.clone()), 0);
    assert_near(image.pixel(380.0, 107.0), [255, 212, 0], 12, "first line");
    assert_near(image.pixel(200.0, 123.0), [255, 212, 0], 12, "second line");
    assert_eq!(
        image.pixel(380.0, 123.0),
        WHITE,
        "beyond the short second line"
    );
}

#[test]
fn underline_strike_out_and_squiggly_sit_at_their_own_heights() {
    let line = Rect::new(100.0, 100.0, 200.0, 20.0);
    let red = |pixel: [u8; 3]| pixel[0] > 180 && pixel[1] < 120 && pixel[2] < 120;
    let red_rows = |image: &Image| -> Vec<u32> {
        (95..125)
            .filter(|y| (150..250).any(|x| red(image.pixel(x as f32, *y as f32))))
            .collect()
    };
    for (kind, lowest, highest) in [
        (TextMarkupKind::Underline, 116, 119),
        (TextMarkupKind::StrikeOut, 110, 113),
        (TextMarkupKind::Squiggly, 114, 119),
    ] {
        let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
        let info = add_text_markup(&mut doc, 0, &markup(kind, vec![line], RED)).unwrap();
        let reloaded = reload(&mut doc);
        let annotation = annotation_by_name(&reloaded, 0, &info.id);
        assert_eq!(name_of(&annotation, b"Subtype"), kind_name(kind));
        assert!(has_normal_appearance(&reloaded, &annotation));

        let rows = red_rows(&render(&save(&mut doc), 0));
        assert!(!rows.is_empty(), "{kind:?}: nothing drawn");
        assert!(
            rows.iter().all(|row| (lowest..=highest).contains(row)),
            "{kind:?}: drawn on rows {rows:?}, expected {lowest}..={highest}"
        );
    }
}

fn kind_name(kind: TextMarkupKind) -> &'static str {
    match kind {
        TextMarkupKind::Highlight => "Highlight",
        TextMarkupKind::Underline => "Underline",
        TextMarkupKind::StrikeOut => "StrikeOut",
        TextMarkupKind::Squiggly => "Squiggly",
    }
}

#[test]
fn opacity_lets_the_paper_show_through() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let mut params = markup(
        TextMarkupKind::Highlight,
        vec![Rect::new(100.0, 100.0, 100.0, 20.0)],
        YELLOW,
    );
    params.opacity = 0.5;
    add_text_markup(&mut doc, 0, &params).unwrap();
    let image = render(&save(&mut doc), 0);
    // half of white and half of white-times-yellow
    assert_near(
        image.pixel(150.0, 110.0),
        [255, 233, 128],
        12,
        "half see-through yellow",
    );
}

#[test]
fn highlights_flatten_into_identical_pixels() {
    let bytes = pdf_bytes(&[text_page().rotated(90)], false);
    let rects = find_text(&bytes, 0, "MMMMMMMMMM");
    let mut doc = load(&bytes);
    add_text_markup(
        &mut doc,
        0,
        &markup(TextMarkupKind::Highlight, rects, YELLOW),
    )
    .unwrap();
    let with_annotation = render(&save(&mut doc), 0);

    let summary = flatten(&mut doc, &FlattenScope::AllPages).unwrap();
    assert_eq!(summary.flattened, 1);
    let flattened = reload(&mut doc);
    assert!(page_annotations(&flattened, 0).is_empty());
    assert!(list(&flattened, 0).unwrap().is_empty());
    let after = render(&save(&mut doc), 0);
    assert!(
        after.differences(&with_annotation, 8) < 20,
        "flattening changed {} pixels",
        after.differences(&with_annotation, 8)
    );
}

#[test]
fn nothing_to_mark_is_refused() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let empty = markup(TextMarkupKind::Highlight, vec![], YELLOW);
    assert_eq!(
        add_text_markup(&mut doc, 0, &empty).unwrap_err(),
        AnnotateError::EmptyArea
    );
    let flat = markup(
        TextMarkupKind::Highlight,
        vec![Rect::new(10.0, 10.0, 50.0, 0.0)],
        YELLOW,
    );
    assert_eq!(
        add_text_markup(&mut doc, 0, &flat).unwrap_err(),
        AnnotateError::EmptyArea
    );
    let broken = markup(
        TextMarkupKind::Highlight,
        vec![Rect::new(f32::NAN, 10.0, 50.0, 10.0)],
        YELLOW,
    );
    assert_eq!(
        add_text_markup(&mut doc, 0, &broken).unwrap_err(),
        AnnotateError::EmptyArea
    );
    assert!(
        page_annotations(&doc, 0).is_empty(),
        "a refused mark leaves nothing behind"
    );
}

#[test]
fn pages_that_do_not_exist_are_reported_in_page_numbers_people_use() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter(), TestPage::letter()], false));
    let params = markup(
        TextMarkupKind::Highlight,
        vec![Rect::new(10.0, 10.0, 50.0, 10.0)],
        YELLOW,
    );
    let error = add_text_markup(&mut doc, 2, &params).unwrap_err();
    assert_eq!(error, AnnotateError::UnknownPage(2));
    assert_eq!(error.to_string(), "Page 3 doesn’t exist in this document.");
    assert!(matches!(list(&doc, 7), Err(AnnotateError::UnknownPage(7))));
}

#[test]
fn encrypted_documents_must_be_unlocked_first() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let mut encryption = Dictionary::new();
    encryption.set("Filter", Object::Name(b"Standard".to_vec()));
    let encryption_id = doc.add_object(encryption);
    doc.trailer.set("Encrypt", Object::Reference(encryption_id));

    let params = markup(
        TextMarkupKind::Highlight,
        vec![Rect::new(10.0, 10.0, 50.0, 10.0)],
        YELLOW,
    );
    let error = add_text_markup(&mut doc, 0, &params).unwrap_err();
    assert_eq!(error, AnnotateError::Locked);
    assert!(error.to_string().contains("Unlock it first"));
    assert_eq!(list(&doc, 0).unwrap_err(), AnnotateError::Locked);
    assert_eq!(
        flatten(&mut doc, &FlattenScope::AllPages).unwrap_err(),
        AnnotateError::Locked
    );
}

#[test]
fn parameters_and_results_use_camel_case_json() {
    let params: TextMarkupParams = serde_json::from_str(
        r#"{"kind":"strikeOut","rects":[{"x":10,"y":20,"width":30,"height":12}],"color":{"r":255,"g":0,"b":0}}"#,
    )
    .unwrap();
    assert_eq!(params.kind, TextMarkupKind::StrikeOut);
    assert_eq!(params.opacity, 1.0, "opacity defaults to solid");

    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let info = add_text_markup(&mut doc, 0, &params).unwrap();
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["kind"], "strikeOut");
    assert_eq!(json["subtype"], "StrikeOut");
    assert_eq!(json["details"]["type"], "textMarkup");
    assert!(json.get("replyTo").is_some());
    assert!(json["modified"].as_str().unwrap().ends_with('Z'));
}
