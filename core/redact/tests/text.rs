//! text redaction: the secret is taken out of every way text can be shown,
//! and the text around it stays exactly where it was.

mod support;

use letters_redact::lopdf::Document;
use letters_redact::{
    find_text_areas, preview, redact, save_clean, FindOptions, PageRect, RedactOptions,
    RedactionArea, TextQuery,
};
use support::*;

const SECRET: &str = "SECRETWORD";

fn phrase(text: &str) -> TextQuery {
    TextQuery::Phrase {
        text: text.to_owned(),
        match_case: true,
    }
}

/// the boxes find_text_areas gives for a phrase, as redaction areas.
fn areas_for(doc: &Document, text: &str) -> Vec<RedactionArea> {
    let found = find_text_areas(doc, &phrase(text), &FindOptions::default()).unwrap();
    found
        .into_iter()
        .map(|found| RedactionArea {
            page: found.page,
            rects: found.rects,
        })
        .collect()
}

/// the secret shown by Tj, TJ with kerning, ' and ", with other words
/// on the same lines.
fn simple_font_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let content = format!(
        "BT /F1 12 Tf 72 700 Td (Name: ) Tj ({SECRET}) Tj ( end) Tj ET\n\
         BT /F1 12 Tf 72 670 Td [(Kerned ) -120 (SECR) 30 (ETWORD) -50 ( after)] TJ ET\n\
         BT /F1 12 Tf 14 TL 72 640 Td (first line) Tj ({SECRET} here) ' ET\n\
         BT /F1 12 Tf 16 TL 72 590 Td (x) Tj 2 1 (beside {SECRET} too) \" ET\n"
    );
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
fn text_shown_every_way_is_gone_after_redaction() {
    let dir = temp_dir("simple-text");
    let worker = start_worker();
    let mut doc = Document::load_mem(&simple_font_pdf()).unwrap();
    let areas = areas_for(&doc, SECRET);
    assert_eq!(areas.len(), 4, "the secret is found once per line");
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].glyphs_removed, 4 * SECRET.len());
    let saved = save_clean(&mut doc).unwrap();
    assert_secret_gone(&worker, &dir, &saved, SECRET);
    // the words around the secret survived
    let path = write(&dir, "after.pdf", &saved);
    let info = worker.open(&path, None).unwrap();
    for word in [
        "Name:",
        "end",
        "Kerned",
        "after",
        "first line",
        "here",
        "beside",
        "too",
    ] {
        assert_eq!(
            worker.search(info.id, word).unwrap().matches.len(),
            1,
            "{word} should survive"
        );
    }
}

#[test]
fn surviving_text_does_not_move() {
    let dir = temp_dir("text-stays");
    let worker = start_worker();
    let original = simple_font_pdf();
    let mut doc = Document::load_mem(&original).unwrap();
    let areas = areas_for(&doc, SECRET);
    redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    let saved = save_clean(&mut doc).unwrap();

    let before = worker
        .open(&write(&dir, "before.pdf", &original), None)
        .unwrap();
    let after = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    for word in ["end", "after", "here", "too"] {
        let old = worker.search(before.id, word).unwrap().matches[0]
            .rects
            .clone();
        let new = worker.search(after.id, word).unwrap().matches[0]
            .rects
            .clone();
        assert_eq!(old, new, "{word} moved");
    }
    let size = (612.0, 792.0);
    let old = render_page(&worker, before.id, 0, 2.0, size);
    let new = render_page(&worker, after.id, 0, 2.0, size);
    let rects: Vec<PageRect> = areas.iter().flat_map(|area| area.rects.clone()).collect();
    assert_pixels(&old, &new, &rects, 2.0, [0, 0, 0]);
}

#[test]
fn only_the_covered_letters_of_a_word_go() {
    let dir = temp_dir("partial-word");
    let worker = start_worker();
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let content = "BT /F1 20 Tf 72 700 Td (ABCDEFGH) Tj ET";
    let (catalog, _) = add_pages(
        &mut pdf,
        vec![PageSpec::new(
            &format!("/Font << /F1 {font} 0 R >>"),
            content,
        )],
        "",
    );
    let original = pdf.finish(catalog, "");
    let mut doc = Document::load_mem(&original).unwrap();
    let areas = areas_for(&doc, "CDE");
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].glyphs_removed, 3);
    let saved = save_clean(&mut doc).unwrap();
    assert_not_in_bytes(&saved, "CDE");
    let before = worker
        .open(&write(&dir, "before.pdf", &original), None)
        .unwrap();
    let after = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    assert!(worker.search(after.id, "C").unwrap().matches.is_empty());
    for kept in ["AB", "FGH"] {
        let old = worker.search(before.id, kept).unwrap().matches[0]
            .rects
            .clone();
        let new = worker.search(after.id, kept).unwrap().matches[0]
            .rects
            .clone();
        assert_eq!(old, new, "{kept} moved");
    }
}

#[test]
fn preview_lists_the_text_that_would_go_and_changes_nothing() {
    let doc = Document::load_mem(&simple_font_pdf()).unwrap();
    let areas = areas_for(&doc, SECRET);
    let before = doc.clone();
    let preview = preview(&doc, &areas).unwrap();
    assert_eq!(preview.areas.len(), 4);
    for area in &preview.areas {
        assert_eq!(area.texts, vec![SECRET.to_owned()]);
    }
    assert_eq!(doc.objects, before.objects);
}
