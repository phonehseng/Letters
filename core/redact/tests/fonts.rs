//! text in every kind of font and container: two byte cid fonts, type 3
//! fonts, text inside a form, text split over several content streams,
//! and a font whose widths are missing.

mod support;

use letters_redact::lopdf::Document;
use letters_redact::{
    find_text_areas, redact, save_clean, FindOptions, RedactOptions, RedactionArea, TextQuery,
};
use support::*;

const SECRET: &str = "SECRETWORD";

fn find(doc: &Document, text: &str) -> Vec<RedactionArea> {
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

fn redact_and_save(original: &[u8], text: &str) -> (Vec<u8>, letters_redact::RedactionReport) {
    let mut doc = Document::load_mem(original).unwrap();
    let areas = find(&doc, text);
    assert!(!areas.is_empty(), "{text:?} is found before redacting");
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    (save_clean(&mut doc).unwrap(), report)
}

/// two byte codes that are the unicode values of the letters, so the
/// secret is in the file as utf-16, which the leak checks look for.
fn utf16_hex(text: &str) -> String {
    text.encode_utf16()
        .map(|unit| format!("{unit:04X}"))
        .collect()
}

#[test]
fn cid_font_text_is_removed() {
    let dir = temp_dir("cid-font");
    let worker = start_worker();
    let mut pdf = Pdf::new();
    let to_unicode = pdf.add_stream(
        "",
        b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
          1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
          1 beginbfrange <0020> <007E> <0020> endbfrange\n\
          endcmap CMapName currentdict /CMap defineresource pop end end",
    );
    let descendant = pdf.add(
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Arial /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /DW 600 /W [32 [278] 65 90 667] /CIDToGIDMap /Identity >>",
    );
    let font = pdf.add(format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /Arial /Encoding /Identity-H /DescendantFonts [{descendant} 0 R] /ToUnicode {to_unicode} 0 R >>"
    ));
    let content = format!(
        "BT /C1 14 Tf 72 700 Td <{}> Tj ET\nBT /C1 14 Tf 72 650 Td [<{}> -300 <{}>] TJ ET",
        utf16_hex(&format!("KEEP {SECRET} KEEP")),
        utf16_hex("LEFT"),
        utf16_hex(SECRET),
    );
    let (catalog, _) = add_pages(
        &mut pdf,
        vec![PageSpec::new(
            &format!("/Font << /C1 {font} 0 R >>"),
            &content,
        )],
        "",
    );
    let original = pdf.finish(catalog, "");
    let (saved, report) = redact_and_save(&original, SECRET);
    assert_eq!(report.pages[0].glyphs_removed, 2 * SECRET.len());
    assert_secret_gone(&worker, &dir, &saved, SECRET);
    let info = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    assert_eq!(worker.search(info.id, "KEEP").unwrap().matches.len(), 2);
    assert_eq!(worker.search(info.id, "LEFT").unwrap().matches.len(), 1);
}

#[test]
fn type3_font_text_is_removed() {
    let dir = temp_dir("type3-font");
    let worker = start_worker();
    let mut pdf = Pdf::new();
    let glyph = pdf.add_stream("", b"800 0 0 0 700 700 d1 0 0 700 700 re f");
    let letters: Vec<char> = ('A'..='Z').collect();
    let procs: String = letters
        .iter()
        .map(|letter| format!("/{letter} {glyph} 0 R "))
        .collect();
    let names: String = letters.iter().map(|letter| format!("/{letter} ")).collect();
    let widths: String = letters.iter().map(|_| "800 ").collect();
    let font = pdf.add(format!(
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 800 700] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << {procs} >> /Encoding << /Type /Encoding /Differences [65 {names}] >> /FirstChar 65 /LastChar 90 /Widths [{widths}] /Resources << >> >>"
    ));
    let content = format!("BT /T3 12 Tf 72 700 Td (KEEP{SECRET}KEEP) Tj ET");
    let (catalog, _) = add_pages(
        &mut pdf,
        vec![PageSpec::new(
            &format!("/Font << /T3 {font} 0 R >>"),
            &content,
        )],
        "",
    );
    let original = pdf.finish(catalog, "");
    let (saved, report) = redact_and_save(&original, SECRET);
    assert_eq!(report.pages[0].glyphs_removed, SECRET.len());
    assert_secret_gone(&worker, &dir, &saved, SECRET);
    let before = worker
        .open(&write(&dir, "before.pdf", &original), None)
        .unwrap();
    let after = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    let old = worker.search(before.id, "KEEP").unwrap();
    let new = worker.search(after.id, "KEEP").unwrap();
    assert_eq!(old.matches.len(), 2);
    assert_eq!(
        old.matches[1].rects, new.matches[1].rects,
        "the second KEEP moved"
    );
}

#[test]
fn text_inside_a_form_is_removed_on_the_redacted_page_only() {
    let dir = temp_dir("form-text");
    let worker = start_worker();
    let mut pdf = Pdf::new();
    let helvetica = add_helvetica(&mut pdf);
    let form = pdf.add_stream(
        &format!("/Type /XObject /Subtype /Form /BBox [0 0 400 100] /Matrix [1 0 0 1 50 500] /Resources << /Font << /F1 {helvetica} 0 R >> >>"),
        format!("BT /F1 12 Tf 10 40 Td (shared {SECRET} form) Tj ET").as_bytes(),
    );
    let resources = format!("/XObject << /Fm1 {form} 0 R >>");
    let pages = vec![
        PageSpec::new(&resources, "q /Fm1 Do Q"),
        PageSpec::new(&resources, "q /Fm1 Do Q"),
    ];
    let (catalog, _) = add_pages(&mut pdf, pages, "");
    let original = pdf.finish(catalog, "");
    let mut doc = Document::load_mem(&original).unwrap();
    let areas: Vec<RedactionArea> = find(&doc, SECRET)
        .into_iter()
        .filter(|area| area.page == 0)
        .collect();
    assert_eq!(areas.len(), 1);
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].glyphs_removed, SECRET.len());
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("page 2")),
        "{:?}",
        report.warnings
    );
    let saved = save_clean(&mut doc).unwrap();
    let info = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    let pages: Vec<u32> = worker
        .search(info.id, SECRET)
        .unwrap()
        .matches
        .iter()
        .map(|m| m.page)
        .collect();
    assert_eq!(pages, vec![1], "page 2 keeps the shared form as it was");
    assert_eq!(worker.search(info.id, "shared").unwrap().matches.len(), 2);
}

#[test]
fn text_inside_a_form_used_on_one_page_is_gone_from_the_file() {
    let dir = temp_dir("form-text-only");
    let worker = start_worker();
    let mut pdf = Pdf::new();
    let helvetica = add_helvetica(&mut pdf);
    let form = pdf.add_flate_stream(
        &format!("/Type /XObject /Subtype /Form /BBox [0 0 400 100] /Resources << /Font << /F1 {helvetica} 0 R >> >>"),
        format!("BT /F1 12 Tf 10 40 Td (inner {SECRET} form) Tj ET").as_bytes(),
    );
    let page = PageSpec::new(
        &format!("/XObject << /Fm1 {form} 0 R >>"),
        "q 1 0 0 1 100 400 cm /Fm1 Do Q",
    );
    let (catalog, _) = add_pages(&mut pdf, vec![page], "");
    let (saved, _) = redact_and_save(&pdf.finish(catalog, ""), SECRET);
    assert_secret_gone(&worker, &dir, &saved, SECRET);
    let info = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    assert_eq!(worker.search(info.id, "inner").unwrap().matches.len(), 1);
}

#[test]
fn text_split_over_content_streams_is_removed() {
    let dir = temp_dir("split-streams");
    let worker = start_worker();
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let mut page = PageSpec::new(
        &format!("/Font << /F1 {font} 0 R >>"),
        "BT /F1 12 Tf 72 700 Td (before ) Tj (SECR) Tj",
    );
    page.contents.push(b"(ETWORD) Tj ( after) Tj ET".to_vec());
    let (catalog, _) = add_pages(&mut pdf, vec![page], "");
    let (saved, report) = redact_and_save(&pdf.finish(catalog, ""), SECRET);
    assert_eq!(report.pages[0].glyphs_removed, SECRET.len());
    assert_secret_gone(&worker, &dir, &saved, SECRET);
    assert_not_in_bytes(&saved, "SECR");
    assert_not_in_bytes(&saved, "ETWORD");
}

#[test]
fn text_in_a_font_without_widths_goes_with_the_rest_of_its_line() {
    let dir = temp_dir("no-widths");
    let worker = start_worker();
    let mut pdf = Pdf::new();
    let font = pdf.add("<< /Type /Font /Subtype /TrueType /BaseFont /MadeUpSans >>");
    let helvetica = add_helvetica(&mut pdf);
    let content = format!("BT /U1 12 Tf 72 700 Td (start {SECRET} end) Tj ET\nBT /F1 12 Tf 72 650 Td (another line) Tj ET");
    let resources = format!("/Font << /U1 {font} 0 R /F1 {helvetica} 0 R >>");
    let (catalog, _) = add_pages(&mut pdf, vec![PageSpec::new(&resources, &content)], "");
    let original = pdf.finish(catalog, "");
    let mut doc = Document::load_mem(&original).unwrap();
    // the box is drawn by hand: guessed widths can't place the word exactly
    let areas = vec![RedactionArea {
        page: 0,
        rects: vec![letters_redact::PageRect::new(110.0, 80.0, 60.0, 16.0)],
    }];
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    assert!(!report.pages[0].removed_wholesale.is_empty());
    let saved = save_clean(&mut doc).unwrap();
    assert_secret_gone(&worker, &dir, &saved, SECRET);
    let info = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    assert_eq!(
        worker
            .search(info.id, "another line")
            .unwrap()
            .matches
            .len(),
        1
    );
}
