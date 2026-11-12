//! freehand ink: strokes land where they were drawn on every page
//! layout, smoothly, and a wide translucent stroke works as a highlighter.

mod support;

use letters_annotate::{add_ink, list, AnnotateError, AnnotationDetails, Color, InkParams, Point};
use support::*;

const RED: Color = Color::new(255, 0, 0);

fn ink(strokes: Vec<Vec<Point>>, width: f32) -> InkParams {
    InkParams {
        strokes,
        color: RED,
        width,
        opacity: 1.0,
        contents: None,
        author: Some("Ada".into()),
    }
}

fn zigzag() -> Vec<Point> {
    vec![
        Point::new(100.0, 300.0),
        Point::new(150.0, 330.0),
        Point::new(200.0, 300.0),
        Point::new(250.0, 330.0),
    ]
}

#[test]
fn ink_lands_where_it_was_drawn_on_every_page_layout() {
    for (layout, page) in page_variants() {
        let blank = page.with_content("");
        let mut doc = load(&pdf_bytes(&[blank], false));
        let info = add_ink(&mut doc, 0, &ink(vec![zigzag()], 4.0)).unwrap();
        let saved = save(&mut doc);
        let reloaded = load(&saved);

        let annotation = annotation_by_name(&reloaded, 0, &info.id);
        assert_eq!(name_of(&annotation, b"Subtype"), "Ink", "{layout}");
        let strokes = annotation.get(b"InkList").unwrap().as_array().unwrap();
        assert_eq!(strokes.len(), 1);
        assert_eq!(strokes[0].as_array().unwrap().len(), 8);
        let border = annotation.get(b"BS").unwrap().as_dict().unwrap();
        assert_eq!(border.get(b"W").unwrap().as_float().unwrap(), 4.0);
        assert!(has_normal_appearance(&reloaded, &annotation));

        let listed = &list(&reloaded, 0).unwrap()[0];
        let AnnotationDetails::Ink { strokes, width } = &listed.details else {
            panic!("{layout}: wrong details");
        };
        assert_eq!(*width, 4.0);
        for (read, drawn) in strokes[0].iter().zip(zigzag()) {
            assert!(
                read.distance_to(drawn) < 0.01,
                "{layout}: {read:?} vs {drawn:?}"
            );
        }

        let image = render(&saved, 0);
        // the curve passes through every point the pen passed
        for point in zigzag() {
            assert_near(
                image.pixel(point.x, point.y),
                [255, 0, 0],
                40,
                &format!("{layout}: at {point:?}"),
            );
        }
        assert_eq!(
            image.pixel(175.0, 360.0),
            WHITE,
            "{layout}: below the stroke"
        );
        assert_eq!(
            image.pixel(150.0, 290.0),
            WHITE,
            "{layout}: above the stroke"
        );
    }
}

trait Distance {
    fn distance_to(&self, other: Point) -> f32;
}

impl Distance for Point {
    fn distance_to(&self, other: Point) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

#[test]
fn strokes_are_smooth_curves_not_straight_steps() {
    // the appearance is made of bezier curves ("c"), one per gap between
    // points, rather than straight lines ("l")
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    add_ink(&mut doc, 0, &ink(vec![zigzag()], 2.0)).unwrap();
    let reloaded = reload(&mut doc);
    let annotation = &page_annotations(&reloaded, 0)[0];
    let normal = annotation
        .get(b"AP")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"N")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = reloaded.get_object(normal).unwrap().as_stream().unwrap();
    let drawing = String::from_utf8(
        stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()),
    )
    .unwrap();
    assert_eq!(
        drawing.matches(" c\n").count(),
        3,
        "one curve per gap between points:\n{drawing}"
    );
    assert!(drawing.contains("1 J 1 j"), "round pen ends");
}

#[test]
fn a_wide_translucent_stroke_is_a_highlighter() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let mut params = ink(
        vec![vec![Point::new(100.0, 200.0), Point::new(300.0, 200.0)]],
        16.0,
    );
    params.color = Color::HIGHLIGHT_YELLOW;
    params.opacity = 0.4;
    add_ink(&mut doc, 0, &params).unwrap();
    let image = render(&save(&mut doc), 0);
    // 40% yellow over white paper
    assert_near(
        image.pixel(200.0, 200.0),
        [255, 240, 153],
        12,
        "pale yellow",
    );
    assert_near(
        image.pixel(200.0, 206.0),
        [255, 240, 153],
        12,
        "the stroke is wide",
    );
    assert_eq!(image.pixel(200.0, 212.0), WHITE);
}

#[test]
fn a_single_tap_leaves_a_dot() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    add_ink(&mut doc, 0, &ink(vec![vec![Point::new(200.0, 200.0)]], 6.0)).unwrap();
    let image = render(&save(&mut doc), 0);
    assert_near(image.pixel(200.0, 200.0), [255, 0, 0], 40, "dot");
    assert_eq!(image.pixel(210.0, 200.0), WHITE);
}

#[test]
fn several_strokes_are_kept_apart() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let strokes = vec![
        vec![Point::new(100.0, 100.0), Point::new(200.0, 100.0)],
        vec![Point::new(100.0, 150.0), Point::new(200.0, 150.0)],
    ];
    add_ink(&mut doc, 0, &ink(strokes, 3.0)).unwrap();
    let image = render(&save(&mut doc), 0);
    assert_near(image.pixel(150.0, 100.0), [255, 0, 0], 40, "first stroke");
    assert_near(image.pixel(150.0, 150.0), [255, 0, 0], 40, "second stroke");
    assert_eq!(
        image.pixel(150.0, 125.0),
        WHITE,
        "no line joins the strokes"
    );
}

#[test]
fn empty_drawings_and_bad_widths_are_refused() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    assert_eq!(
        add_ink(&mut doc, 0, &ink(vec![], 2.0)).unwrap_err(),
        AnnotateError::EmptyArea
    );
    assert_eq!(
        add_ink(&mut doc, 0, &ink(vec![vec![]], 2.0)).unwrap_err(),
        AnnotateError::EmptyArea
    );
    let nowhere = vec![vec![Point::new(f32::INFINITY, 3.0)]];
    assert_eq!(
        add_ink(&mut doc, 0, &ink(nowhere, 2.0)).unwrap_err(),
        AnnotateError::EmptyArea
    );
    assert!(matches!(
        add_ink(&mut doc, 0, &ink(vec![zigzag()], 0.0)),
        Err(AnnotateError::InvalidValue(_))
    ));
    assert!(matches!(
        add_ink(&mut doc, 9, &ink(vec![zigzag()], 2.0)),
        Err(AnnotateError::UnknownPage(9))
    ));
}
