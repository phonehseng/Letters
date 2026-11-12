//! vector drawings: reading paths, moving, reshaping, restyling and
//! restacking them, each checked in the saved file and in pixels.

mod support;

use letters_edit::{
    bring_paths_to_front, delete_paths, duplicate_paths, move_paths, page_objects, resize_paths,
    rotate_paths, set_path_points, set_path_style, Color, EditConsent, GroupReason, NewPaint,
    PathShape, PathStyle, Point, Rect,
};
use support::{load, render, save, PageSetup, PdfBuilder};

/// a page with a "logo" of three paths in one q block, a red triangle, a
/// blue line, a cmyk rectangle and some text.
const DRAWING: &str = "\
q 0 0 1 rg 72 650 40 40 re f 1 0 0 rg 116 650 40 40 re f 0 1 0 rg 160 650 40 40 re f Q
1 0 0 rg 300 600 m 360 600 l 330 660 l h f
0 0 1 RG 4 w 72 500 m 300 500 l S
0 1 1 0 k 72 300 100 60 re f
BT /F1 12 Tf 72 200 Td (Some text) Tj ET";

fn drawing_pdf(setup: PageSetup) -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    pdf.page(&setup, &[DRAWING], "/Font << /F1 3 0 R >>");
    pdf.finish()
}

fn ids_of(objects: &letters_edit::PageObjects, shape: PathShape) -> Vec<String> {
    objects
        .paths
        .iter()
        .filter(|path| path.shape == shape)
        .map(|path| path.id.clone())
        .collect()
}

fn consent() -> EditConsent {
    EditConsent::default()
}

#[test]
fn paths_are_read_with_shape_colour_and_line_style() {
    let doc = load(&drawing_pdf(PageSetup::letter()));
    let objects = page_objects(&doc, 0).unwrap();
    assert_eq!(objects.paths.len(), 6, "{:#?}", objects.paths);
    let shapes: Vec<PathShape> = objects.paths.iter().map(|path| path.shape).collect();
    assert_eq!(
        shapes,
        [
            PathShape::Rectangle,
            PathShape::Rectangle,
            PathShape::Rectangle,
            PathShape::Free,
            PathShape::Line,
            PathShape::Rectangle
        ]
    );
    let line = &objects.paths[4];
    let stroke = line.stroke.as_ref().unwrap();
    assert_eq!(stroke.width, 4.0);
    assert_eq!(stroke.paint.color, Color::new(0, 0, 255));
    assert_eq!(
        line.points,
        [Point::new(72.0, 292.0), Point::new(300.0, 292.0)]
    );
    assert!(line.fill.is_none() && line.point_editable);
    // the line's box includes half its width
    assert_eq!(line.rect, Rect::new(70.0, 290.0, 232.0, 4.0));
    let cmyk = objects.paths[5].fill.as_ref().unwrap();
    assert_eq!(cmyk.color_space, "DeviceCMYK");
    assert_eq!(cmyk.components, [0.0, 1.0, 1.0, 0.0]);
    assert!(
        !objects.paths[3].point_editable,
        "free shapes are not point-editable"
    );
    // the three logo squares are suggested as one object, twice over:
    // they share a q block and they touch one after another
    let logo: Vec<String> = objects.paths[..3].iter().map(|p| p.id.clone()).collect();
    let block = objects
        .path_groups
        .iter()
        .find(|group| group.reason == GroupReason::SameBlock)
        .expect("a q-block group");
    assert_eq!(block.path_ids, logo);
    assert_eq!(block.rect, Rect::new(72.0, 102.0, 128.0, 40.0));
}

#[test]
fn a_moved_path_moves_alone_and_exactly() {
    for (name, setup) in support::layouts() {
        let bytes = drawing_pdf(setup);
        let before = render(&bytes, 0);
        let mut doc = load(&bytes);
        let objects = page_objects(&doc, 0).unwrap();
        let triangle = objects.paths[3].clone();
        let result = move_paths(
            &mut doc,
            0,
            std::slice::from_ref(&triangle.id),
            30.0,
            20.0,
            &consent(),
        )
        .unwrap();
        let moved = result
            .objects
            .paths
            .iter()
            .find(|p| p.id == result.object_ids[0])
            .unwrap();
        assert!(
            (moved.rect.x - triangle.rect.x - 30.0).abs() < 0.01,
            "{name}"
        );
        assert!(
            (moved.rect.y - triangle.rect.y - 20.0).abs() < 0.01,
            "{name}"
        );
        assert_eq!(result.summary.paths_moved, 1);
        let saved = save(&mut doc);
        let after = render(&saved, 0);
        // the pixels of the triangle are found 30 right and 20 down
        // the middle of a triangle's box is inside it, however it is turned
        let centre = Point::new(
            triangle.rect.x + triangle.rect.width / 2.0,
            triangle.rect.y + triangle.rect.height / 2.0,
        );
        assert_eq!(before.pixel(centre.x, centre.y), [255, 0, 0], "{name}");
        assert_eq!(
            after.pixel(centre.x + 30.0, centre.y + 20.0),
            [255, 0, 0],
            "{name}"
        );
        // and nothing outside the old and new places changed
        let changed = triangle.rect.union(&moved.rect);
        let outside = differences_outside(&before, &after, changed);
        assert_eq!(outside, 0, "{name}: {outside} pixels changed elsewhere");
    }
}

/// pixels that differ anywhere except inside `except` (plus a pixel of
/// anti-aliasing around it).
fn differences_outside(a: &support::Image, b: &support::Image, except: Rect) -> usize {
    let mut count = 0;
    for y in 0..a.height {
        for x in 0..a.width {
            let (fx, fy) = (x as f32, y as f32);
            let inside = fx >= except.x - 2.0
                && fx <= except.right() + 2.0
                && fy >= except.y - 2.0
                && fy <= except.bottom() + 2.0;
            if !inside && a.pixel(fx, fy) != b.pixel(fx, fy) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn a_logo_moves_as_one_object() {
    let bytes = drawing_pdf(PageSetup::letter());
    let mut doc = load(&bytes);
    let objects = page_objects(&doc, 0).unwrap();
    let logo = objects.path_groups[0].path_ids.clone();
    let result = move_paths(&mut doc, 0, &logo, 0.0, 300.0, &consent()).unwrap();
    assert_eq!(result.object_ids.len(), 3);
    let after = render(&save(&mut doc), 0);
    // each square's colour is found 300 points lower
    assert_eq!(after.pixel(92.0, 422.0), [0, 0, 255]);
    assert_eq!(after.pixel(136.0, 422.0), [255, 0, 0]);
    assert_eq!(after.pixel(180.0, 422.0), [0, 255, 0]);
    assert_eq!(after.pixel(92.0, 122.0), support::WHITE);
}

#[test]
fn restyling_changes_only_that_path() {
    let bytes = drawing_pdf(PageSetup::letter());
    let before = render(&bytes, 0);
    let mut doc = load(&bytes);
    let objects = page_objects(&doc, 0).unwrap();
    let cmyk = objects.paths[5].clone();
    let style = PathStyle {
        fill: Some(NewPaint::Rgb {
            color: Color::new(0, 128, 0),
        }),
        stroke: Some(NewPaint::Gray { value: 0.0 }),
        stroke_width: Some(2.0),
        ..PathStyle::default()
    };
    let result = set_path_style(
        &mut doc,
        0,
        std::slice::from_ref(&cmyk.id),
        &style,
        &consent(),
    )
    .unwrap();
    let restyled = &result.objects.paths[5];
    assert_eq!(restyled.fill.as_ref().unwrap().color, Color::new(0, 128, 0));
    assert_eq!(restyled.stroke.as_ref().unwrap().width, 2.0);
    let after = render(&save(&mut doc), 0);
    assert_eq!(after.pixel(120.0, 462.0), [0, 128, 0]);
    // an outline now runs around it
    assert!(support::is_dark(after.pixel(72.0, 462.0)));
    assert_eq!(differences_outside(&before, &after, cmyk.rect), 0);
    // the other paths kept their colours
    assert_eq!(after.pixel(330.0, 170.0), [255, 0, 0]);
}

#[test]
fn removing_a_fill_keeps_the_outline() {
    let mut doc = load(&drawing_pdf(PageSetup::letter()));
    let triangle = page_objects(&doc, 0).unwrap().paths[3].clone();
    let style = PathStyle {
        fill: Some(NewPaint::None),
        stroke: Some(NewPaint::Rgb {
            color: Color::new(255, 0, 0),
        }),
        ..PathStyle::default()
    };
    let result = set_path_style(&mut doc, 0, &[triangle.id], &style, &consent()).unwrap();
    let path = &result.objects.paths[3];
    assert!(path.fill.is_none());
    assert!(path.stroke.is_some());
    let content = support::page_content(&doc, 0);
    // the triangle ends with h, so a plain S strokes it closed
    assert!(
        content.contains("h S"),
        "the path is now stroked: {content}"
    );
}

#[test]
fn line_ends_and_rectangle_corners_can_be_edited() {
    let mut doc = load(&drawing_pdf(PageSetup::letter()));
    let objects = page_objects(&doc, 0).unwrap();
    let line = objects.paths[4].id.clone();
    let result = set_path_points(
        &mut doc,
        0,
        &line,
        &[Point::new(100.0, 250.0), Point::new(400.0, 250.0)],
        &consent(),
    )
    .unwrap();
    let edited = result
        .objects
        .paths
        .iter()
        .find(|p| p.id == result.object_ids[0])
        .unwrap();
    assert_eq!(
        edited.points,
        [Point::new(100.0, 250.0), Point::new(400.0, 250.0)]
    );
    assert_eq!(
        edited.stroke.as_ref().unwrap().width,
        4.0,
        "the width stays"
    );
    let rect = result.objects.paths[5].id.clone();
    let corners = Rect::new(50.0, 400.0, 200.0, 100.0).corners();
    let result = set_path_points(&mut doc, 0, &rect, &corners, &consent()).unwrap();
    assert_eq!(
        result.objects.paths[5].rect,
        Rect::new(50.0, 400.0, 200.0, 100.0)
    );
    let free = result.objects.paths[3].id.clone();
    let refused = set_path_points(&mut doc, 0, &free, &corners, &consent());
    assert!(refused.unwrap_err().to_string().contains("Only the points"));
    let after = render(&save(&mut doc), 0);
    assert!(
        support::is_dark(after.pixel(250.0, 250.0)) || after.pixel(250.0, 250.0) == [0, 0, 255]
    );
}

#[test]
fn resizing_and_turning_follow_the_box() {
    let mut doc = load(&drawing_pdf(PageSetup::letter()));
    let id = page_objects(&doc, 0).unwrap().paths[5].id.clone();
    let result = resize_paths(
        &mut doc,
        0,
        &[id],
        Rect::new(300.0, 400.0, 50.0, 30.0),
        &consent(),
    )
    .unwrap();
    let id = result.object_ids[0].clone();
    assert_eq!(
        result.objects.paths[5].rect,
        Rect::new(300.0, 400.0, 50.0, 30.0)
    );
    let result = rotate_paths(&mut doc, 0, &[id], 90.0, &consent()).unwrap();
    let turned = result.objects.paths[5].rect;
    assert!(
        (turned.width - 30.0).abs() < 0.01 && (turned.height - 50.0).abs() < 0.01,
        "{turned:?}"
    );
    assert!((turned.x + turned.width / 2.0 - 325.0).abs() < 0.01);
}

#[test]
fn deleting_duplicating_and_restacking() {
    let bytes = drawing_pdf(PageSetup::letter());
    let mut doc = load(&bytes);
    let objects = page_objects(&doc, 0).unwrap();
    let triangle = objects.paths[3].id.clone();
    let result = duplicate_paths(
        &mut doc,
        0,
        std::slice::from_ref(&triangle),
        100.0,
        0.0,
        &consent(),
    )
    .unwrap();
    assert_eq!(result.objects.paths.len(), 7);
    let after = render(&save(&mut doc), 0);
    assert_eq!(after.pixel(430.0, 170.0), [255, 0, 0], "the copy is drawn");
    assert_eq!(after.pixel(330.0, 170.0), [255, 0, 0], "the original stays");

    // the blue line is drawn under the red triangle's copy once it is sent
    // to the front over a new red square covering it
    let line = result.objects.paths[4].id.clone();
    let result = bring_paths_to_front(&mut doc, 0, &[line], &consent()).unwrap();
    assert_eq!(result.objects.paths.last().unwrap().shape, PathShape::Line);

    let triangle = result.objects.paths[3].id.clone();
    let result = delete_paths(&mut doc, 0, &[triangle], &consent()).unwrap();
    assert_eq!(result.objects.paths.len(), 6);
    let after = render(&save(&mut doc), 0);
    assert_eq!(
        after.pixel(330.0, 170.0),
        support::WHITE,
        "the original is gone"
    );
    assert_eq!(after.pixel(430.0, 170.0), [255, 0, 0], "the copy stays");
}

#[test]
fn a_path_in_a_form_drawn_twice_changes_only_where_it_was_edited() {
    let mut pdf = PdfBuilder::new();
    let form = pdf.stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 50 50]",
        b"1 0 0 rg 0 0 50 50 re f",
    );
    pdf.page(
        &PageSetup::letter(),
        &["q 1 0 0 1 100 600 cm /X1 Do Q q 1 0 0 1 300 600 cm /X1 Do Q"],
        &format!("/XObject << /X1 {form} 0 R >>"),
    );
    let mut doc = load(&pdf.finish());
    let objects = page_objects(&doc, 0).unwrap();
    assert_eq!(objects.paths.len(), 2);
    let first = objects.paths[0].id.clone();
    move_paths(&mut doc, 0, &[first], 0.0, 100.0, &consent()).unwrap();
    let after = render(&save(&mut doc), 0);
    assert_eq!(
        after.pixel(125.0, 267.0),
        [255, 0, 0],
        "the first drawing moved down"
    );
    assert_eq!(after.pixel(125.0, 167.0), support::WHITE);
    assert_eq!(after.pixel(325.0, 167.0), [255, 0, 0], "the second stayed");
}

#[test]
fn a_pattern_fill_moves_with_its_path() {
    let mut pdf = PdfBuilder::new();
    let shading = pdf.add(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [100 0 200 0] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> /Extend [true true] >>",
    );
    let pattern = pdf.add(format!(
        "<< /Type /Pattern /PatternType 2 /Shading {shading} 0 R >>"
    ));
    pdf.page(
        &PageSetup::letter(),
        &["/Pattern cs /P1 scn 100 500 100 50 re f"],
        &format!("/Pattern << /P1 {pattern} 0 R >>"),
    );
    let bytes = pdf.finish();
    let before = render(&bytes, 0);
    let mut doc = load(&bytes);
    let objects = page_objects(&doc, 0).unwrap();
    assert_eq!(
        objects.paths[0].fill.as_ref().unwrap().color_space,
        "Pattern"
    );
    move_paths(
        &mut doc,
        0,
        &[objects.paths[0].id.clone()],
        200.0,
        0.0,
        &consent(),
    )
    .unwrap();
    let after = render(&save(&mut doc), 0);
    for x in [105.0, 150.0, 195.0] {
        let old = before.pixel(x, 267.0);
        let new = after.pixel(x + 200.0, 267.0);
        let close = old
            .iter()
            .zip(new)
            .all(|(a, b)| (i32::from(*a) - i32::from(b)).abs() <= 3);
        assert!(close, "at {x}: {old:?} became {new:?}");
    }
}

#[test]
fn shapes_on_a_turned_page_are_still_upright_rectangles() {
    let doc = load(&drawing_pdf(PageSetup::letter().rotated(90)));
    let objects = page_objects(&doc, 0).unwrap();
    assert_eq!(ids_of(&objects, PathShape::Rectangle).len(), 4);
    assert_eq!(objects.paths[4].shape, PathShape::Line);
}
