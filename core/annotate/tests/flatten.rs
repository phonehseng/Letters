//! flattening: marks become part of the page, look the same, and the
//! annotation objects are gone.

mod support;

use letters_annotate::{
    add_image, add_ink, add_note, add_reply, add_shape, add_text_box, add_text_markup, flatten,
    list, AnnotateError, Color, FlattenScope, FlattenSummary, ImageParams, InkParams, NoteIcon,
    NoteParams, Point, Rect, ReplyParams, ShapeKind, ShapeParams, StandardFont, StrokeStyle,
    TextAlign, TextBoxParams, TextMarkupKind, TextMarkupParams,
};
use lopdf::{Document, Object};
use support::*;

/// puts one of every kind of mark on page `page`, returning the note's id.
fn mark_everything(doc: &mut Document, page: usize) -> String {
    add_text_markup(
        doc,
        page,
        &TextMarkupParams {
            kind: TextMarkupKind::Highlight,
            rects: vec![Rect::new(60.0, 80.0, 250.0, 30.0)],
            color: Color::HIGHLIGHT_YELLOW,
            opacity: 1.0,
            contents: None,
            author: None,
        },
    )
    .unwrap();
    let note = add_note(
        doc,
        page,
        &NoteParams {
            position: Point::new(400.0, 60.0),
            icon: NoteIcon::Note,
            color: Color::new(255, 160, 0),
            contents: "a note".into(),
            author: None,
            opacity: 1.0,
            open: false,
        },
    )
    .unwrap();
    add_reply(
        doc,
        page,
        &note.id,
        &ReplyParams {
            contents: "a reply".into(),
            author: None,
        },
    )
    .unwrap();
    add_text_box(
        doc,
        page,
        &TextBoxParams {
            rect: Rect::new(60.0, 150.0, 200.0, 60.0),
            text: "Flattened text stays put".into(),
            font: StandardFont::TimesRoman,
            font_size: 14.0,
            color: Color::new(0, 0, 160),
            align: TextAlign::Left,
            border: None,
            fill: Some(Color::new(230, 255, 230)),
            opacity: 1.0,
            author: None,
        },
    )
    .unwrap();
    add_ink(
        doc,
        page,
        &InkParams {
            strokes: vec![vec![
                Point::new(300.0, 250.0),
                Point::new(350.0, 280.0),
                Point::new(400.0, 250.0),
            ]],
            color: Color::new(200, 0, 100),
            width: 3.0,
            opacity: 0.7,
            contents: None,
            author: None,
        },
    )
    .unwrap();
    add_shape(
        doc,
        page,
        &ShapeParams {
            kind: ShapeKind::Circle,
            rect: Rect::new(60.0, 300.0, 120.0, 80.0),
            stroke: StrokeStyle {
                color: Color::new(0, 0, 255),
                width: 3.0,
                dash: vec![4.0, 2.0],
            },
            fill: Some(Color::new(255, 255, 200)),
            opacity: 1.0,
            contents: None,
            author: None,
        },
    )
    .unwrap();
    add_image(
        doc,
        page,
        &ImageParams {
            rect: Rect::new(300.0, 320.0, 60.0, 30.0),
            image: solid_png([0, 128, 255]),
            opacity: 1.0,
            contents: None,
            author: None,
        },
    )
    .unwrap();
    note.id
}

fn solid_png(color: [u8; 3]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 4, 4);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&color.repeat(16)).unwrap();
    writer.finish().unwrap();
    bytes
}

/// the subtypes of the annotations left on a page, form fields included.
fn subtypes_left(doc: &Document, page: usize) -> Vec<String> {
    page_annotations(doc, page)
        .iter()
        .map(|annotation| name_of(annotation, b"Subtype"))
        .collect()
}

#[test]
fn flattening_keeps_every_pixel_and_removes_the_annotations() {
    for (layout, page) in [
        ("upright", text_page()),
        ("turned 90", text_page().rotated(90)),
        (
            "turned 270 and cropped",
            text_page().rotated(270).cropped([40.0, 30.0, 580.0, 760.0]),
        ),
    ] {
        let page = page.with_annotation("<< /Type /Annot /Subtype /Link /Rect [400 700 500 720] /Border [0 0 0] /A << /S /URI /URI (https://example.org) >> >>");
        let mut doc = load(&pdf_bytes(&[page], true));
        mark_everything(&mut doc, 0);
        let before = render(&save(&mut doc), 0);
        let names: Vec<String> = list(&doc, 0)
            .unwrap()
            .into_iter()
            .map(|annotation| annotation.id)
            .collect();

        let summary = flatten(&mut doc, &FlattenScope::AllPages).unwrap();
        assert_eq!(
            summary,
            FlattenSummary {
                flattened: 7,
                pages: vec![0]
            },
            "{layout}"
        );
        let reloaded = reload(&mut doc);
        assert_eq!(
            subtypes_left(&reloaded, 0),
            ["Link", "Widget"],
            "{layout}: links and form fields stay"
        );
        // nothing that was flattened survives anywhere in the file
        for object in reloaded.objects.values() {
            if let Ok(dict) = object.as_dict() {
                if let Ok(name) = dict.get(b"NM").and_then(Object::as_str) {
                    assert!(
                        !names.iter().any(|id| id.as_bytes() == name),
                        "{layout}: an annotation object is left"
                    );
                }
                assert_ne!(
                    dict.get(b"Subtype").ok(),
                    Some(&Object::Name(b"Popup".to_vec())),
                    "{layout}: a pop-up is left"
                );
            }
        }
        let after = render(&save(&mut doc), 0);
        let changed = after.differences(&before, 8);
        assert!(
            changed < 40,
            "{layout}: flattening changed {changed} pixels"
        );
        // and the marks really are there
        assert_near(
            after.pixel(70.0, 95.0),
            [255, 212, 0],
            12,
            &format!("{layout}: highlight"),
        );
        assert_near(
            after.pixel(120.0, 340.0),
            [255, 255, 200],
            6,
            &format!("{layout}: ellipse fill"),
        );
        assert_near(
            after.pixel(330.0, 335.0),
            [0, 128, 255],
            6,
            &format!("{layout}: picture"),
        );
    }
}

#[test]
fn flattening_chosen_annotations_takes_their_threads_and_leaves_the_rest() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let note = mark_everything(&mut doc, 0);
    let before = list(&doc, 0).unwrap();
    assert_eq!(before.len(), 7);
    let circle = before
        .iter()
        .find(|annotation| annotation.subtype == "Circle")
        .unwrap()
        .id
        .clone();

    let summary = flatten(
        &mut doc,
        &FlattenScope::Annotations {
            page: 0,
            ids: vec![note.clone(), circle.clone()],
        },
    )
    .unwrap();
    assert_eq!(summary.flattened, 3, "the note, its reply and the circle");
    let left: Vec<String> = list(&doc, 0)
        .unwrap()
        .into_iter()
        .map(|annotation| annotation.subtype)
        .collect();
    assert_eq!(left, ["Highlight", "FreeText", "Ink", "Stamp"]);
    assert!(!subtypes_left(&doc, 0).contains(&"Popup".to_string()));
    let image = render(&save(&mut doc), 0);
    assert_near(
        image.pixel(120.0, 340.0),
        [255, 255, 200],
        6,
        "the circle is now page content",
    );
}

#[test]
fn flattening_some_pages_leaves_the_others() {
    let mut doc = load(&pdf_bytes(
        &[TestPage::letter(), TestPage::letter(), TestPage::letter()],
        false,
    ));
    mark_everything(&mut doc, 0);
    mark_everything(&mut doc, 2);
    let summary = flatten(&mut doc, &FlattenScope::Pages { pages: vec![2] }).unwrap();
    assert_eq!(summary.pages, [2]);
    assert_eq!(list(&doc, 0).unwrap().len(), 7);
    assert!(list(&doc, 2).unwrap().is_empty());

    // again is harmless
    assert_eq!(
        flatten(&mut doc, &FlattenScope::Pages { pages: vec![2] })
            .unwrap()
            .flattened,
        0
    );
    // a page that doesn't exist changes nothing at all
    let error = flatten(&mut doc, &FlattenScope::Pages { pages: vec![0, 7] }).unwrap_err();
    assert_eq!(error, AnnotateError::UnknownPage(7));
    assert_eq!(list(&doc, 0).unwrap().len(), 7);
}

#[test]
fn marks_from_other_apps_without_a_drawing_are_drawn_before_flattening() {
    let page = TestPage::letter().with_annotation(
        "<< /Type /Annot /Subtype /Square /Rect [100 500 200 600] /C [1 0 0] /IC [0 1 0] /BS << /W 4 >> >>",
    );
    let mut doc = load(&pdf_bytes(&[page], false));
    flatten(&mut doc, &FlattenScope::AllPages).unwrap();
    assert!(page_annotations(&doc, 0).is_empty());
    let image = render(&save(&mut doc), 0);
    assert_near(image.pixel(150.0, 242.0), [0, 255, 0], 6, "its fill");
    assert_near(image.pixel(101.5, 242.0), [255, 0, 0], 40, "its border");
}

#[test]
fn form_fields_and_links_cannot_be_flattened_by_id() {
    let page = TestPage::letter()
        .with_annotation("<< /Type /Annot /Subtype /Link /Rect [400 700 500 720] >>");
    let mut doc = load(&pdf_bytes(&[page], true));
    let link = list(&doc, 0).unwrap()[0].id.clone();
    let scope = FlattenScope::Annotations {
        page: 0,
        ids: vec![link],
    };
    assert_eq!(
        flatten(&mut doc, &scope).unwrap_err(),
        AnnotateError::CannotChange
    );
    let widget = format!("ref:{}-0", 4 + 2 + 1);
    let scope = FlattenScope::Annotations {
        page: 0,
        ids: vec![widget],
    };
    assert_eq!(
        flatten(&mut doc, &scope).unwrap_err(),
        AnnotateError::FormField
    );
}

#[test]
fn flatten_scope_reads_from_json() {
    let scope: FlattenScope =
        serde_json::from_str(r#"{"scope":"annotations","page":2,"ids":["a"]}"#).unwrap();
    assert_eq!(
        scope,
        FlattenScope::Annotations {
            page: 2,
            ids: vec!["a".into()]
        }
    );
    let scope: FlattenScope = serde_json::from_str(r#"{"scope":"allPages"}"#).unwrap();
    assert_eq!(scope, FlattenScope::AllPages);
}
