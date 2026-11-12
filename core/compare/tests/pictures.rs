//! pictures and drawings: found by comparing pixels, and shown in the overlay.

mod support;

use letters_compare::*;
use support::*;

fn with_picture(colour: [u8; 3]) -> PageSpec {
    let mut items = lines(
        72.0,
        100.0,
        12.0,
        16.0,
        &[
            "A caption above the picture stays the same.",
            "So does this second line of text.",
        ],
    );
    items.push(Item::Image {
        x: 72.0,
        top: 300.0,
        width: 100.0,
        height: 100.0,
        picture: Picture::plain(20, 20, colour),
    });
    page(items)
}

#[test]
fn a_replaced_picture_is_found_in_the_right_place() {
    let (old, new) = write_pair(
        "picture",
        &pdf(vec![with_picture([220, 30, 30])]),
        &pdf(vec![with_picture([30, 30, 220])]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let changes = &result.page_pairs[0].changes;
    assert_eq!(changes.len(), 1, "{changes:#?}");
    let change = &changes[0];
    assert_eq!(change.kind, ChangeKind::Redrawn);
    assert_eq!(change.category, Category::Image);
    assert_eq!(change.detail, "Picture or drawing changed in this area");
    let expected = Rect::new(72.0, 300.0, 100.0, 100.0);
    assert!(
        lands_on(&change.left_rects, expected, 7.0),
        "{:?}",
        change.left_rects
    );
    assert!(
        lands_on(&change.right_rects, expected, 7.0),
        "{:?}",
        change.right_rects
    );
    assert_eq!(result.summary.image, 1);
}

#[test]
fn identical_pictures_are_not_changes() {
    let spec = pdf(vec![with_picture([10, 120, 10])]);
    let mut variant = spec.clone();
    variant.variant = true;
    let (old, new) = write_pair("same-picture", &spec, &variant);
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    assert_eq!(
        result.summary.total, 0,
        "{:#?}",
        result.page_pairs[0].changes
    );
}

fn boxes(moving_top: f32) -> PageSpec {
    page(vec![
        filled_box(100.0, moving_top, 50.0, 50.0, [0.0, 0.0, 0.0]),
        filled_box(300.0, 100.0, 50.0, 50.0, [0.0, 0.0, 0.0]),
    ])
}

#[test]
fn pages_without_text_are_compared_as_pictures_and_say_so() {
    let (old, new) = write_pair(
        "scanned",
        &pdf(vec![boxes(100.0)]),
        &pdf(vec![boxes(400.0)]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let pair = &result.page_pairs[0];
    assert!(
        pair.notes
            .iter()
            .any(|note| note.contains("compared as a picture")),
        "{:?}",
        pair.notes
    );
    let redrawn: Vec<&Change> = pair
        .changes
        .iter()
        .filter(|change| change.kind == ChangeKind::Redrawn)
        .collect();
    assert!(!redrawn.is_empty());
    // the box's old place and new place are both marked; the box that
    // stayed is not
    let all_left: Vec<Rect> = redrawn
        .iter()
        .flat_map(|change| change.left_rects.clone())
        .collect();
    assert!(
        covers(&all_left, Rect::new(100.0, 100.0, 50.0, 50.0)),
        "{all_left:?}"
    );
    assert!(
        covers(&all_left, Rect::new(100.0, 400.0, 50.0, 50.0)),
        "{all_left:?}"
    );
    assert!(
        !covers(&all_left, Rect::new(300.0, 100.0, 50.0, 50.0)),
        "{all_left:?}"
    );
}

#[test]
fn the_overlay_shows_shared_content_grey_removed_red_and_added_green() {
    let (old, new) = write_pair(
        "overlay",
        &pdf(vec![boxes(100.0)]),
        &pdf(vec![boxes(400.0)]),
    );
    let (old_source, new_source) = (old.source(), new.source());
    let result = compare_documents(&old_source, &new_source).unwrap();
    let pair = &result.page_pairs[0];
    let overlay = overlay_image(&old_source, &new_source, pair, 1.0).unwrap();
    assert_eq!((overlay.width, overlay.height), (612, 792));

    let removed = overlay.pixel(125, 125);
    assert_eq!(removed[0], 255, "only on the old page: red {removed:?}");
    assert!(removed[1] < 100 && removed[2] < 100, "{removed:?}");
    let added = overlay.pixel(125, 425);
    assert!(
        added[1] > added[0] + 80 && added[1] > added[2] + 80,
        "only on the new page: green {added:?}"
    );
    let shared = overlay.pixel(325, 125);
    assert!(
        shared[0] == shared[1] && shared[1] == shared[2] && shared[0] < 150,
        "on both: grey {shared:?}"
    );
    assert_eq!(
        overlay.pixel(50, 50),
        [255, 255, 255, 255],
        "paper stays white"
    );

    // the blink pair: same size, each page as it is
    let aligned = aligned_images(&old_source, &new_source, pair, 1.0).unwrap();
    assert_eq!(
        (aligned.left.width, aligned.left.height),
        (aligned.right.width, aligned.right.height)
    );
    assert_eq!(aligned.left.rgb_on_white(125, 125), [0, 0, 0]);
    assert_eq!(aligned.right.rgb_on_white(125, 125), [255, 255, 255]);
    assert_eq!(aligned.right.rgb_on_white(125, 425), [0, 0, 0]);
}

#[test]
fn the_overlay_lines_up_text_that_moved_as_a_whole() {
    // everything moved down 30 points: aligned, the pages match again
    let old = page(lines(
        72.0,
        100.0,
        12.0,
        16.0,
        &["Header text moves down", "with the rest of the page."],
    ));
    let new = page(lines(
        72.0,
        130.0,
        12.0,
        16.0,
        &["Header text moves down", "with the rest of the page."],
    ));
    let (old, new) = write_pair("overlay-offset", &pdf(vec![old]), &pdf(vec![new]));
    let (old_source, new_source) = (old.source(), new.source());
    let result = compare_documents(&old_source, &new_source).unwrap();
    let pair = &result.page_pairs[0];
    assert!((pair.offset.dy - 30.0).abs() < 0.5, "{:?}", pair.offset);
    let aligned = aligned_images(&old_source, &new_source, pair, 1.0).unwrap();
    let header = old.word_rect(0, "Header");
    let (x, y) = (header.x as u32 + 2, header.center_y() as u32);
    let ink = |image: &RgbaImage| {
        (0..20)
            .map(|dx| image.rgb_on_white(x + dx, y)[0] as u32)
            .sum::<u32>()
    };
    let difference = ink(&aligned.left).abs_diff(ink(&aligned.right));
    assert!(
        difference < 200,
        "the aligned pictures should match: {difference}"
    );
}

#[test]
fn an_added_page_overlays_all_green() {
    let (old, new) = write_pair(
        "overlay-added",
        &pdf(vec![boxes(100.0)]),
        &pdf(vec![boxes(100.0), boxes(400.0)]),
    );
    let (old_source, new_source) = (old.source(), new.source());
    let result = compare_documents(&old_source, &new_source).unwrap();
    let added = result
        .page_pairs
        .iter()
        .find(|pair| pair.left_page.is_none())
        .unwrap();
    let overlay = overlay_image(&old_source, &new_source, added, 0.5).unwrap();
    let pixel = overlay.pixel(62, 212);
    assert!(pixel[1] > pixel[0] + 80, "{pixel:?}");
}
