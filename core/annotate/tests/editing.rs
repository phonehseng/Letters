//! moving, resizing, restyling, retyping and deleting annotations.

mod support;

use letters_annotate::{
    add_ink, add_line, add_shape, add_text_box, add_text_markup, delete, get, list, update,
    AnnotateError, AnnotationChanges, AnnotationDetails, Color, InkParams, LineEnding, LineParams,
    Point, Rect, ShapeKind, ShapeParams, StandardFont, StrokeStyle, TextAlign, TextBoxParams,
    TextMarkupKind, TextMarkupParams,
};
use lopdf::{Document, ObjectId};
use support::*;

const BLUE: Color = Color::new(0, 0, 255);
const GREEN: Color = Color::new(0, 160, 0);

fn appearance_of(doc: &Document, id: &str) -> ObjectId {
    let annotation = annotation_by_name(doc, 0, id);
    annotation
        .get(b"AP")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"N")
        .unwrap()
        .as_reference()
        .unwrap()
}

fn square(doc: &mut Document, rect: Rect) -> String {
    let params = ShapeParams {
        kind: ShapeKind::Square,
        rect,
        stroke: StrokeStyle {
            color: BLUE,
            width: 4.0,
            dash: Vec::new(),
        },
        fill: None,
        opacity: 1.0,
        contents: None,
        author: None,
    };
    add_shape(doc, 0, &params).unwrap().id
}

fn move_to(rect: Rect) -> AnnotationChanges {
    AnnotationChanges {
        rect: Some(rect),
        ..Default::default()
    }
}

#[test]
fn moving_a_highlight_moves_its_quads_and_keeps_its_drawing() {
    for (layout, page) in [
        ("upright", TestPage::letter()),
        ("turned 90", TestPage::letter().rotated(90)),
    ] {
        let mut doc = load(&pdf_bytes(&[page], false));
        let params = TextMarkupParams {
            kind: TextMarkupKind::Highlight,
            rects: vec![Rect::new(100.0, 100.0, 200.0, 20.0)],
            color: Color::HIGHLIGHT_YELLOW,
            opacity: 1.0,
            contents: None,
            author: None,
        };
        let info = add_text_markup(&mut doc, 0, &params).unwrap();
        let drawing = appearance_of(&doc, &info.id);
        let target = Rect::new(
            info.rect.x + 50.0,
            info.rect.y + 200.0,
            info.rect.width,
            info.rect.height,
        );
        let moved = update(&mut doc, 0, &info.id, &move_to(target)).unwrap();

        assert!(
            (moved.rect.x - target.x).abs() < 0.01 && (moved.rect.y - target.y).abs() < 0.01,
            "{layout}"
        );
        let AnnotationDetails::TextMarkup { quads } = &moved.details else {
            panic!("wrong details");
        };
        assert!(
            (quads[0][0].x - 150.0).abs() < 0.01 && (quads[0][0].y - 300.0).abs() < 0.01,
            "{layout}"
        );
        assert_eq!(
            appearance_of(&doc, &info.id),
            drawing,
            "{layout}: a move keeps the drawing"
        );

        let image = render(&save(&mut doc), 0);
        assert_near(
            image.pixel(250.0, 310.0),
            [255, 212, 0],
            12,
            &format!("{layout}: new place"),
        );
        assert_eq!(image.pixel(150.0, 110.0), WHITE, "{layout}: old place");
    }
}

#[test]
fn resizing_ink_stretches_the_strokes_to_the_new_box() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter().rotated(180)], false));
    let params = InkParams {
        strokes: vec![vec![Point::new(100.0, 100.0), Point::new(200.0, 150.0)]],
        color: BLUE,
        width: 2.0,
        opacity: 1.0,
        contents: None,
        author: None,
    };
    let info = add_ink(&mut doc, 0, &params).unwrap();
    // the frame is the strokes plus 2 points of margin (half the width and one more)
    assert!((info.rect.x - 98.0).abs() < 0.01 && (info.rect.width - 104.0).abs() < 0.01);
    let bigger = Rect::new(298.0, 298.0, 204.0, 104.0);
    let resized = update(&mut doc, 0, &info.id, &move_to(bigger)).unwrap();
    for (got, wanted) in [
        (resized.rect.x, 298.0),
        (resized.rect.y, 298.0),
        (resized.rect.width, 204.0),
        (resized.rect.height, 104.0),
    ] {
        assert!((got - wanted).abs() < 0.05, "rect {:?}", resized.rect);
    }
    let AnnotationDetails::Ink { strokes, .. } = &resized.details else {
        panic!("wrong details");
    };
    let ends = (strokes[0][0], *strokes[0].last().unwrap());
    assert!(
        (ends.0.x - 300.0).abs() < 0.05 && (ends.0.y - 300.0).abs() < 0.05,
        "{ends:?}"
    );
    assert!(
        (ends.1.x - 500.0).abs() < 0.05 && (ends.1.y - 400.0).abs() < 0.05,
        "{ends:?}"
    );

    let image = render(&save(&mut doc), 0);
    assert_near(
        image.pixel(400.0, 350.0),
        [0, 0, 255],
        60,
        "the middle of the stretched stroke",
    );
    assert_eq!(image.pixel(150.0, 125.0), WHITE, "old place");
}

#[test]
fn restyling_redraws_and_cleans_up_the_old_drawing() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let id = square(&mut doc, Rect::new(100.0, 100.0, 100.0, 100.0));
    let old_drawing = appearance_of(&doc, &id);
    let changes = AnnotationChanges {
        color: Some(GREEN),
        stroke_width: Some(8.0),
        dash: Some(vec![]),
        fill: Some(Some(Color::new(250, 250, 200))),
        ..Default::default()
    };
    let changed = update(&mut doc, 0, &id, &changes).unwrap();
    assert_eq!(changed.color, Some(GREEN));
    let AnnotationDetails::Shape {
        stroke_width, fill, ..
    } = changed.details
    else {
        panic!("wrong details");
    };
    assert_eq!((stroke_width, fill), (8.0, Some(Color::new(250, 250, 200))));
    assert_ne!(appearance_of(&doc, &id), old_drawing);
    assert!(
        !doc.objects.contains_key(&old_drawing),
        "the replaced drawing is removed"
    );

    let image = render(&save(&mut doc), 0);
    assert_near(
        image.pixel(106.0, 150.0),
        [0, 160, 0],
        30,
        "a thicker green outline",
    );
    assert_near(
        image.pixel(150.0, 150.0),
        [250, 250, 200],
        4,
        "the new fill",
    );

    // and null takes the fill away again
    let no_fill: AnnotationChanges = serde_json::from_str(r#"{"fill":null}"#).unwrap();
    assert_eq!(no_fill.fill, Some(None));
    let cleared = update(&mut doc, 0, &id, &no_fill).unwrap();
    let AnnotationDetails::Shape { fill, .. } = cleared.details else {
        panic!("wrong details");
    };
    assert_eq!(fill, None);
    assert_eq!(render(&save(&mut doc), 0).pixel(150.0, 150.0), WHITE);
}

#[test]
fn changing_a_text_box_rewraps_its_text() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter().rotated(90)], false));
    let params = TextBoxParams {
        rect: Rect::new(100.0, 100.0, 150.0, 100.0),
        text: "first".into(),
        font: StandardFont::Helvetica,
        font_size: 12.0,
        color: Color::BLACK,
        align: TextAlign::Left,
        border: None,
        fill: None,
        opacity: 1.0,
        author: None,
    };
    let info = add_text_box(&mut doc, 0, &params).unwrap();
    let changes = AnnotationChanges {
        text: Some("MMMMMMMM MMMMMMMM".into()),
        font: Some(StandardFont::TimesBold),
        font_size: Some(18.0),
        color: Some(BLUE),
        ..Default::default()
    };
    let changed = update(&mut doc, 0, &info.id, &changes).unwrap();
    let AnnotationDetails::TextBox {
        text,
        font,
        font_size,
        text_color,
        ..
    } = &changed.details
    else {
        panic!("wrong details");
    };
    assert_eq!(
        (text.as_str(), *font, *font_size, *text_color),
        ("MMMMMMMM MMMMMMMM", StandardFont::TimesBold, 18.0, BLUE)
    );
    assert_eq!(
        changed.contents.as_deref(),
        Some("MMMMMMMM MMMMMMMM"),
        "a text box's text is its comment"
    );

    let image = render(&save(&mut doc), 0);
    let blue = |pixel: [u8; 3]| pixel[2] > 150 && pixel[0] < 100;
    let in_band = |top: u32, bottom: u32| {
        (top..bottom)
            .flat_map(|y| (100..250).map(move |x| (x, y)))
            .filter(|(x, y)| blue(image.pixel(*x as f32, *y as f32)))
            .count()
    };
    // two words of 8 bold Ms (8 x 16.7 points each) can't share a 146 point line
    assert!(in_band(102, 120) > 100, "first line");
    assert!(in_band(124, 142) > 100, "second line");

    let bad = AnnotationChanges {
        text: Some("日本".into()),
        ..Default::default()
    };
    assert!(matches!(
        update(&mut doc, 0, &info.id, &bad),
        Err(AnnotateError::UnsupportedCharacters(_))
    ));
    assert_eq!(
        get(&doc, 0, &info.id).unwrap().contents.as_deref(),
        Some("MMMMMMMM MMMMMMMM"),
        "a refused change changes nothing"
    );
}

#[test]
fn line_ends_and_arrowheads_can_be_changed() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let params = LineParams {
        start: Point::new(100.0, 100.0),
        end: Point::new(200.0, 100.0),
        stroke: StrokeStyle {
            color: BLUE,
            width: 2.0,
            dash: Vec::new(),
        },
        start_ending: LineEnding::None,
        end_ending: LineEnding::None,
        fill: None,
        opacity: 1.0,
        contents: None,
        author: None,
    };
    let info = add_line(&mut doc, 0, &params).unwrap();
    let changes = AnnotationChanges {
        points: Some(vec![Point::new(100.0, 300.0), Point::new(300.0, 300.0)]),
        start_ending: Some(LineEnding::OpenArrow),
        ..Default::default()
    };
    let changed = update(&mut doc, 0, &info.id, &changes).unwrap();
    let AnnotationDetails::Line {
        start,
        end,
        start_ending,
        end_ending,
        ..
    } = changed.details
    else {
        panic!("wrong details");
    };
    assert_eq!(
        (start_ending, end_ending),
        (LineEnding::OpenArrow, LineEnding::None)
    );
    assert!((start.y - 300.0).abs() < 0.01 && (end.x - 300.0).abs() < 0.01);
    let image = render(&save(&mut doc), 0);
    assert_near(
        image.pixel(200.0, 300.0),
        [0, 0, 255],
        60,
        "the line in its new place",
    );
    assert_eq!(image.pixel(150.0, 100.0), WHITE, "not in the old one");
}

#[test]
fn deleting_removes_the_annotation_and_its_drawing() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let keep = square(&mut doc, Rect::new(300.0, 300.0, 50.0, 50.0));
    let id = square(&mut doc, Rect::new(100.0, 100.0, 100.0, 100.0));
    let drawing = appearance_of(&doc, &id);
    assert_eq!(delete(&mut doc, 0, &id).unwrap(), std::slice::from_ref(&id));
    assert!(!doc.objects.contains_key(&drawing));
    let reloaded = reload(&mut doc);
    let left: Vec<String> = list(&reloaded, 0)
        .unwrap()
        .into_iter()
        .map(|annotation| annotation.id)
        .collect();
    assert_eq!(left, [keep]);
    assert_eq!(
        get(&reloaded, 0, &id).unwrap_err(),
        AnnotateError::UnknownAnnotation
    );
    assert_eq!(render(&save(&mut doc), 0).pixel(101.0, 150.0), WHITE);
}

#[test]
fn bad_changes_are_refused_and_change_nothing() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let id = square(&mut doc, Rect::new(100.0, 100.0, 100.0, 100.0));
    let before = get(&doc, 0, &id).unwrap();
    for changes in [
        AnnotationChanges {
            stroke_width: Some(0.0),
            ..Default::default()
        },
        AnnotationChanges {
            rect: Some(Rect::new(0.0, 0.0, -5.0, 10.0)),
            ..Default::default()
        },
        AnnotationChanges {
            opacity: Some(f32::NAN),
            ..Default::default()
        },
    ] {
        assert!(update(&mut doc, 0, &id, &changes).is_err(), "{changes:?}");
    }
    assert_eq!(get(&doc, 0, &id).unwrap(), before);
    assert_eq!(
        update(
            &mut doc,
            0,
            "letters-missing",
            &move_to(Rect::new(1.0, 1.0, 5.0, 5.0))
        )
        .unwrap_err(),
        AnnotateError::UnknownAnnotation
    );
    assert!(matches!(
        update(&mut doc, 4, &id, &AnnotationChanges::default()),
        Err(AnnotateError::UnknownPage(4))
    ));
}

#[test]
fn changes_update_the_modified_date() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let id = square(&mut doc, Rect::new(100.0, 100.0, 100.0, 100.0));
    // pretend it was last changed long ago
    let page_id = doc.get_pages()[&1];
    let annots = doc
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap();
    let annotation_id = annots[0].as_reference().unwrap();
    let old_date = lopdf::Object::string_literal("D:20000101000000+00'00'");
    doc.get_dictionary_mut(annotation_id)
        .unwrap()
        .set("M", old_date);
    assert_eq!(
        get(&doc, 0, &id).unwrap().modified.as_deref(),
        Some("2000-01-01T00:00:00Z")
    );

    let changed = update(
        &mut doc,
        0,
        &id,
        &AnnotationChanges {
            author: Some("Grace".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(changed.author.as_deref(), Some("Grace"));
    assert!(
        changed.modified.unwrap().as_str() > "2020",
        "the modified date moves to now"
    );
    assert_eq!(changed.created, get(&doc, 0, &id).unwrap().created);
}
