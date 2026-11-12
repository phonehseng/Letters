//! page geometry and the edge cases: every rotation and a crop box with an
//! odd origin, overlapping boxes, a box over nothing, locked files,
//! unreadable pages, and redacting twice.

mod support;

use letters_redact::lopdf::Document;
use letters_redact::{
    encrypt, find_text_areas, redact, save_clean, FindOptions, LockOptions, PageRect, RedactError,
    RedactOptions, RedactionArea, TextQuery,
};
use support::*;

const SECRET: &str = "SECRETWORD";

fn found_areas(doc: &Document, text: &str) -> Vec<RedactionArea> {
    let query = TextQuery::Phrase {
        text: text.to_owned(),
        match_case: true,
    };
    find_text_areas(doc, &query, &FindOptions::default())
        .unwrap()
        .into_iter()
        .map(|found| RedactionArea {
            page: found.page,
            rects: found.rects,
        })
        .collect()
}

/// a turned page whose crop box does not start at the origin.
fn turned_pdf(rotate: u32) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let content = format!("BT /F1 14 Tf 72 600 Td (left {SECRET} right) Tj ET\nBT /F1 14 Tf 72 560 Td (second line) Tj ET");
    let page = PageSpec::new(&format!("/Font << /F1 {font} 0 R >>"), &content)
        .with_extra(&format!("/Rotate {rotate} /CropBox [36 50 576 742]"));
    let (catalog, _) = add_pages(&mut pdf, vec![page], "");
    pdf.finish(catalog, "")
}

#[test]
fn every_rotation_and_a_crop_box_put_the_box_in_the_right_place() {
    let worker = start_worker();
    for rotate in [0, 90, 180, 270] {
        let dir = temp_dir(&format!("rotate-{rotate}"));
        let original = turned_pdf(rotate);
        let mut doc = Document::load_mem(&original).unwrap();
        let areas = found_areas(&doc, SECRET);
        assert_eq!(areas.len(), 1, "rotate {rotate}");
        let ours = areas[0].rects[0];

        // pdfium's own box for the word must sit inside ours
        let before = worker
            .open(&write(&dir, "before.pdf", &original), None)
            .unwrap();
        let theirs = worker.search(before.id, SECRET).unwrap().matches[0].rects[0];
        let (x, y, width, height) = (
            theirs.x as f64,
            theirs.y as f64,
            theirs.width as f64,
            theirs.height as f64,
        );
        assert!(
            x >= ours.x - 1.0
                && y >= ours.y - 1.0
                && x + width <= ours.x + ours.width + 1.0
                && y + height <= ours.y + ours.height + 1.0,
            "rotate {rotate}: pdfium's box {theirs:?} is not inside ours {ours:?}"
        );

        redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
        let saved = save_clean(&mut doc).unwrap();
        assert_secret_gone(&worker, &dir, &saved, SECRET);
        let after = worker
            .open(&write(&dir, "after.pdf", &saved), None)
            .unwrap();
        for word in ["left", "right", "second line"] {
            let old = worker.search(before.id, word).unwrap().matches[0]
                .rects
                .clone();
            let new = worker.search(after.id, word).unwrap().matches[0]
                .rects
                .clone();
            assert_eq!(old, new, "rotate {rotate}: {word} moved");
        }
        let size = before.page_sizes[0];
        let size = (size.width_pt, size.height_pt);
        let old = render_page(&worker, before.id, 0, 2.0, size);
        let new = render_page(&worker, after.id, 0, 2.0, size);
        assert_pixels(&old, &new, &areas[0].rects, 2.0, [0, 0, 0]);
    }
}

fn plain_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let content = format!("BT /F1 14 Tf 72 600 Td (left {SECRET} right) Tj ET");
    let (catalog, _) = add_pages(
        &mut pdf,
        vec![PageSpec::new(
            &format!("/Font << /F1 {font} 0 R >>"),
            &content,
        )],
        "",
    );
    pdf.finish(catalog, "")
}

#[test]
fn overlapping_boxes_remove_everything_under_either() {
    let dir = temp_dir("overlap");
    let worker = start_worker();
    let mut doc = Document::load_mem(&plain_pdf()).unwrap();
    let word = found_areas(&doc, SECRET)[0].rects[0];
    let half = word.width / 2.0;
    let areas = vec![RedactionArea {
        page: 0,
        rects: vec![
            PageRect::new(word.x, word.y, half + 10.0, word.height),
            PageRect::new(
                word.x + half - 10.0,
                word.y - 2.0,
                half + 10.0,
                word.height + 4.0,
            ),
        ],
    }];
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].glyphs_removed, SECRET.len());
    let saved = save_clean(&mut doc).unwrap();
    assert_secret_gone(&worker, &dir, &saved, SECRET);
}

#[test]
fn a_box_over_nothing_removes_nothing() {
    let dir = temp_dir("nothing");
    let worker = start_worker();
    let original = plain_pdf();
    let mut doc = Document::load_mem(&original).unwrap();
    let areas = vec![RedactionArea {
        page: 0,
        rects: vec![PageRect::new(300.0, 500.0, 100.0, 50.0)],
    }];
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    let page = &report.pages[0];
    assert_eq!(
        (
            page.glyphs_removed,
            page.images_removed,
            page.paths_removed,
            page.annotations_removed
        ),
        (0, 0, 0, 0)
    );
    let saved = save_clean(&mut doc).unwrap();
    let before = worker
        .open(&write(&dir, "before.pdf", &original), None)
        .unwrap();
    let after = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    assert_eq!(
        worker
            .search(after.id, &format!("left {SECRET} right"))
            .unwrap()
            .matches
            .len(),
        1
    );
    let old = render_page(&worker, before.id, 0, 1.0, (612.0, 792.0));
    let new = render_page(&worker, after.id, 0, 1.0, (612.0, 792.0));
    assert_pixels(&old, &new, &areas[0].rects, 1.0, [0, 0, 0]);
}

#[test]
fn a_locked_file_must_be_unlocked_first() {
    let mut doc = Document::load_mem(&plain_pdf()).unwrap();
    let options = LockOptions {
        open_password: "open sesame".into(),
        ..Default::default()
    };
    encrypt(&mut doc, &options).unwrap();
    let locked = save_clean(&mut doc).unwrap();
    let mut still_locked = Document::load_mem(&locked).unwrap();
    let areas = vec![RedactionArea {
        page: 0,
        rects: vec![PageRect::new(10.0, 10.0, 10.0, 10.0)],
    }];
    let error = redact(&mut still_locked, &areas, &RedactOptions::default()).unwrap_err();
    assert_eq!(error, RedactError::Locked);
    assert!(error.to_string().contains("Unlock it first"));
}

#[test]
fn a_page_that_cannot_be_read_is_refused_and_nothing_changes() {
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let content = format!("BT /F1 14 Tf 72 600 Td ({SECRET} Tj ET");
    let (catalog, _) = add_pages(
        &mut pdf,
        vec![PageSpec::new(
            &format!("/Font << /F1 {font} 0 R >>"),
            &content,
        )],
        "",
    );
    let mut doc = Document::load_mem(&pdf.finish(catalog, "")).unwrap();
    let before = doc.objects.clone();
    let areas = vec![RedactionArea {
        page: 0,
        rects: vec![PageRect::new(60.0, 170.0, 200.0, 40.0)],
    }];
    let error = redact(&mut doc, &areas, &RedactOptions::default()).unwrap_err();
    assert!(
        matches!(error, RedactError::UnreadablePage(1, _)),
        "{error:?}"
    );
    assert_eq!(doc.objects, before);
}

#[test]
fn boxes_with_no_size_or_on_missing_pages_are_refused() {
    let mut doc = Document::load_mem(&plain_pdf()).unwrap();
    let empty = vec![RedactionArea {
        page: 0,
        rects: vec![PageRect::new(10.0, 10.0, 0.0, 10.0)],
    }];
    assert_eq!(
        redact(&mut doc, &empty, &RedactOptions::default()).unwrap_err(),
        RedactError::EmptyArea(1)
    );
    let missing = vec![RedactionArea {
        page: 3,
        rects: vec![PageRect::new(10.0, 10.0, 10.0, 10.0)],
    }];
    assert_eq!(
        redact(&mut doc, &missing, &RedactOptions::default()).unwrap_err(),
        RedactError::UnknownPage(4)
    );
}

#[test]
fn redacting_twice_gives_the_same_file_as_once() {
    let mut doc = Document::load_mem(&plain_pdf()).unwrap();
    let areas = found_areas(&doc, SECRET);
    let options = RedactOptions {
        overlay_text: Some("Redacted".into()),
        ..Default::default()
    };
    redact(&mut doc, &areas, &options).unwrap();
    let once = save_clean(&mut doc).unwrap();
    let mut again = Document::load_mem(&once).unwrap();
    let report = redact(&mut again, &areas, &options).unwrap();
    assert_eq!(report.pages[0].glyphs_removed, 0);
    let twice = save_clean(&mut again).unwrap();
    assert!(once == twice, "the second redaction changed the file");
}
