//! rectangles, ellipses, lines with arrowheads, polygons and polylines.

mod support;

use letters_annotate::{
    add_line, add_polygon, add_shape, list, AnnotateError, AnnotationDetails, Color, LineEnding,
    LineParams, Point, PolygonParams, Rect, ShapeKind, ShapeParams, StrokeStyle,
};
use support::*;

const BLUE: Color = Color::new(0, 0, 255);
const RED: Color = Color::new(255, 0, 0);
const PALE: Color = Color::new(255, 240, 180);

fn stroke(color: Color, width: f32) -> StrokeStyle {
    StrokeStyle {
        color,
        width,
        dash: Vec::new(),
    }
}

fn shape(kind: ShapeKind, rect: Rect) -> ShapeParams {
    ShapeParams {
        kind,
        rect,
        stroke: stroke(BLUE, 4.0),
        fill: Some(PALE),
        opacity: 1.0,
        contents: None,
        author: None,
    }
}

fn line(start: Point, end: Point) -> LineParams {
    LineParams {
        start,
        end,
        stroke: stroke(RED, 2.0),
        start_ending: LineEnding::None,
        end_ending: LineEnding::ClosedArrow,
        fill: None,
        opacity: 1.0,
        contents: None,
        author: None,
    }
}

#[test]
fn rectangles_keep_their_outline_inside_the_box_on_every_page_layout() {
    let rect = Rect::new(100.0, 100.0, 150.0, 80.0);
    for (layout, page) in page_variants() {
        let mut doc = load(&pdf_bytes(&[page.with_content("")], false));
        let info = add_shape(&mut doc, 0, &shape(ShapeKind::Square, rect)).unwrap();
        let saved = save(&mut doc);
        let reloaded = load(&saved);
        let annotation = annotation_by_name(&reloaded, 0, &info.id);
        assert_eq!(name_of(&annotation, b"Subtype"), "Square", "{layout}");
        assert_eq!(numbers_of(&annotation, b"C"), [0.0, 0.0, 1.0]);
        assert_eq!(numbers_of(&annotation, b"IC").len(), 3);
        assert!(has_normal_appearance(&reloaded, &annotation));
        let listed = &list(&reloaded, 0).unwrap()[0];
        assert!(
            (listed.rect.x - 100.0).abs() < 0.01 && (listed.rect.width - 150.0).abs() < 0.01,
            "{layout}"
        );

        let image = render(&saved, 0);
        assert_near(
            image.pixel(101.5, 140.0),
            [0, 0, 255],
            30,
            &format!("{layout}: left edge"),
        );
        assert_near(
            image.pixel(175.0, 178.5),
            [0, 0, 255],
            30,
            &format!("{layout}: bottom edge"),
        );
        assert_near(
            image.pixel(175.0, 140.0),
            [255, 240, 180],
            4,
            &format!("{layout}: fill"),
        );
        assert_eq!(image.pixel(98.0, 140.0), WHITE, "{layout}: outside");
        assert_eq!(image.pixel(175.0, 182.0), WHITE, "{layout}: below");
    }
}

#[test]
fn ellipses_leave_the_corners_of_their_box_empty() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter().rotated(270)], false));
    add_shape(
        &mut doc,
        0,
        &shape(ShapeKind::Circle, Rect::new(100.0, 100.0, 200.0, 100.0)),
    )
    .unwrap();
    let image = render(&save(&mut doc), 0);
    assert_near(image.pixel(200.0, 150.0), [255, 240, 180], 4, "centre");
    assert_near(image.pixel(101.5, 150.0), [0, 0, 255], 40, "leftmost point");
    assert_near(image.pixel(200.0, 101.5), [0, 0, 255], 40, "top point");
    assert_eq!(image.pixel(106.0, 106.0), WHITE, "the corner of the box");
}

#[test]
fn shapes_without_fill_show_the_page_through() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let mut params = shape(ShapeKind::Square, Rect::new(100.0, 100.0, 100.0, 100.0));
    params.fill = None;
    let info = add_shape(&mut doc, 0, &params).unwrap();
    assert!(!annotation_by_name(&doc, 0, &info.id).has(b"IC"));
    let image = render(&save(&mut doc), 0);
    assert_eq!(image.pixel(150.0, 150.0), WHITE);
    assert_near(image.pixel(101.5, 150.0), [0, 0, 255], 30, "outline");
}

#[test]
fn lines_end_in_arrowheads_where_asked() {
    for (layout, page) in [
        ("upright", TestPage::letter()),
        ("turned 90", TestPage::letter().rotated(90)),
    ] {
        let mut doc = load(&pdf_bytes(&[page], false));
        let info = add_line(
            &mut doc,
            0,
            &line(Point::new(100.0, 300.0), Point::new(300.0, 300.0)),
        )
        .unwrap();
        let saved = save(&mut doc);
        let reloaded = load(&saved);
        let annotation = annotation_by_name(&reloaded, 0, &info.id);
        assert_eq!(name_of(&annotation, b"Subtype"), "Line");
        assert_eq!(numbers_of(&annotation, b"L").len(), 4);
        let endings = annotation.get(b"LE").unwrap().as_array().unwrap();
        assert_eq!(endings[0].as_name().unwrap(), b"None");
        assert_eq!(endings[1].as_name().unwrap(), b"ClosedArrow");
        let listed = &list(&reloaded, 0).unwrap()[0];
        let AnnotationDetails::Line {
            start,
            end,
            end_ending,
            stroke_width,
            ..
        } = &listed.details
        else {
            panic!("wrong details");
        };
        assert!(
            (start.x - 100.0).abs() < 0.01
                && (end.x - 300.0).abs() < 0.01
                && (end.y - 300.0).abs() < 0.01
        );
        assert_eq!((*end_ending, *stroke_width), (LineEnding::ClosedArrow, 2.0));

        let image = render(&saved, 0);
        assert_near(
            image.pixel(200.0, 300.0),
            [255, 0, 0],
            40,
            &format!("{layout}: the line"),
        );
        // the arrowhead is 13 points long and opens 30 degrees each side
        assert_near(
            image.pixel(292.0, 303.0),
            [255, 0, 0],
            40,
            &format!("{layout}: arrowhead"),
        );
        assert_near(
            image.pixel(292.0, 297.0),
            [255, 0, 0],
            40,
            &format!("{layout}: arrowhead"),
        );
        assert_eq!(
            image.pixel(108.0, 303.5),
            WHITE,
            "{layout}: no arrowhead at the start"
        );
        assert_eq!(
            image.pixel(200.0, 306.0),
            WHITE,
            "{layout}: beside the line"
        );
    }
}

#[test]
fn dashed_lines_have_gaps() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let mut params = line(Point::new(100.0, 300.0), Point::new(300.0, 300.0));
    params.stroke.dash = vec![6.0, 6.0];
    params.end_ending = LineEnding::None;
    let info = add_line(&mut doc, 0, &params).unwrap();
    let border = annotation_by_name(&doc, 0, &info.id)
        .get(b"BS")
        .unwrap()
        .as_dict()
        .unwrap()
        .clone();
    assert_eq!(border.get(b"S").unwrap().as_name().unwrap(), b"D");
    let image = render(&save(&mut doc), 0);
    // 6 on and 6 off, and each dash's round ends add half the width (1
    // point) on both sides: 8 of every 12 points are red
    let red = (110..290)
        .filter(|x| image.pixel(*x as f32, 300.0)[1] < 100)
        .count();
    assert!(
        (100..140).contains(&red),
        "about two thirds of the line is drawn: {red} of 180 pixels"
    );
}

#[test]
fn polygons_fill_and_polylines_stay_open() {
    let triangle = vec![
        Point::new(100.0, 300.0),
        Point::new(200.0, 300.0),
        Point::new(150.0, 200.0),
    ];
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let polygon = add_polygon(
        &mut doc,
        0,
        &PolygonParams {
            points: triangle.clone(),
            closed: true,
            stroke: stroke(BLUE, 2.0),
            fill: Some(PALE),
            opacity: 1.0,
            contents: None,
            author: None,
        },
    )
    .unwrap();
    let shifted: Vec<Point> = triangle
        .iter()
        .map(|point| Point::new(point.x + 200.0, point.y))
        .collect();
    let polyline = add_polygon(
        &mut doc,
        0,
        &PolygonParams {
            points: shifted,
            closed: false,
            stroke: stroke(BLUE, 2.0),
            fill: Some(PALE),
            opacity: 1.0,
            contents: None,
            author: None,
        },
    )
    .unwrap();
    let reloaded = reload(&mut doc);
    assert_eq!(
        name_of(&annotation_by_name(&reloaded, 0, &polygon.id), b"Subtype"),
        "Polygon"
    );
    assert_eq!(
        name_of(&annotation_by_name(&reloaded, 0, &polyline.id), b"Subtype"),
        "PolyLine"
    );
    assert_eq!(
        numbers_of(&annotation_by_name(&reloaded, 0, &polygon.id), b"Vertices").len(),
        6
    );

    let image = render(&save(&mut doc), 0);
    assert_near(
        image.pixel(150.0, 280.0),
        [255, 240, 180],
        4,
        "inside the polygon",
    );
    // the closing edge runs from the last corner (150, 200) back to the first (100, 300)
    assert_near(
        image.pixel(125.0, 250.0),
        [0, 0, 255],
        60,
        "its closing edge",
    );
    assert_eq!(
        image.pixel(350.0, 280.0),
        WHITE,
        "a polyline is never filled"
    );
    assert_eq!(image.pixel(325.0, 250.0), WHITE, "and has no closing edge");
    assert_near(
        image.pixel(375.0, 250.0),
        [0, 0, 255],
        60,
        "but it has its sides",
    );
}

#[test]
fn shapes_with_nothing_to_draw_are_refused() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let point = Point::new(100.0, 100.0);
    assert_eq!(
        add_line(&mut doc, 0, &line(point, point)).unwrap_err(),
        AnnotateError::EmptyArea
    );
    let two_points = PolygonParams {
        points: vec![point, Point::new(200.0, 100.0)],
        closed: true,
        stroke: stroke(BLUE, 1.0),
        fill: None,
        opacity: 1.0,
        contents: None,
        author: None,
    };
    assert_eq!(
        add_polygon(&mut doc, 0, &two_points).unwrap_err(),
        AnnotateError::EmptyArea
    );
    let mut too_thick = shape(ShapeKind::Square, Rect::new(10.0, 10.0, 10.0, 10.0));
    too_thick.stroke.width = -1.0;
    assert!(matches!(
        add_shape(&mut doc, 0, &too_thick),
        Err(AnnotateError::InvalidValue(_))
    ));
    assert!(page_annotations(&doc, 0).is_empty());
}
