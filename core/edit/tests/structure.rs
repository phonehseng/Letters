//! edits in awkward places: form XObjects (shared and drawn twice),
//! several content streams, tagged (marked) content, kerned text, other
//! alignments, repeated edits, and long documents.

mod support;

use std::time::Instant;

use letters_edit::{insert_text, page_objects, replace_text, TextAlignment, TextEditOptions};
use support::*;

fn options() -> TextEditOptions {
    TextEditOptions::default()
}

/// a form holding one line of text, drawn twice on the page.
fn form_drawn_twice() -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    let form = pdf.stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 300 40] /Resources << /Font << /F1 3 0 R >> >>",
        b"BT /F1 14 Tf 0 10 Td (Shared label) Tj ET",
    );
    pdf.page(
        &PageSetup::letter(),
        &["q 1 0 0 1 72 700 cm /Fm1 Do Q q 1 0 0 1 72 500 cm /Fm1 Do Q"],
        &format!("/XObject << /Fm1 {form} 0 R >>"),
    );
    pdf.finish()
}

#[test]
fn text_in_a_form_drawn_twice_changes_only_where_it_was_edited() {
    let mut doc = load(&form_drawn_twice());
    let objects = page_objects(&doc, 0).unwrap();
    assert_eq!(objects.text_blocks.len(), 2);
    let top = objects
        .text_blocks
        .iter()
        .min_by(|a, b| a.rect.y.total_cmp(&b.rect.y))
        .unwrap();
    assert!(
        top.lines[0].id.contains(":c0>"),
        "the id names the form inside the page stream: {}",
        top.lines[0].id
    );
    let result = replace_text(
        &mut doc,
        0,
        &top.lines[0].id,
        "Edited label, now much longer than the form box ever was",
        &options(),
    )
    .unwrap();
    let texts: Vec<&str> = result
        .objects
        .text_blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect();
    assert!(
        texts.contains(&"Edited label, now much longer than the form box ever was")
            && texts.contains(&"Shared label"),
        "{texts:?}"
    );
    let saved = save(&mut doc);
    assert_eq!(
        find_text(&saved, 0, "Shared label").len(),
        1,
        "the other drawing still says the old text"
    );
    assert_eq!(find_text(&saved, 0, "Edited label").len(), 1);
    // the form's box grew, so the longer text isn't cut off
    let shown = render(&saved, 0);
    let edited = result
        .objects
        .text_blocks
        .iter()
        .find(|b| b.text.starts_with("Edited"))
        .unwrap();
    let tail = edited.lines[0].runs[0].char_boxes.last().unwrap();
    assert!(
        tail.right() > 72.0 + 300.0,
        "the text reaches past the form's old box"
    );
    assert!(
        shown.count(
            (
                tail.x as u32,
                tail.y as u32,
                tail.right() as u32 + 1,
                tail.bottom() as u32 + 1
            ),
            is_dark
        ) > 0,
        "and is drawn there"
    );
}

#[test]
fn text_in_a_later_content_stream_is_found_and_edited() {
    let mut pdf = PdfBuilder::new();
    pdf.page(
        &PageSetup::letter(),
        &[
            "BT /F1 12 Tf 72 700 Td (First stream) Tj ET",
            "BT /F1 12 Tf 72 600 Td (Second stream) Tj ET",
        ],
        "/Font << /F1 3 0 R >>",
    );
    let mut doc = load(&pdf.finish());
    let objects = page_objects(&doc, 0).unwrap();
    let second = objects
        .text_blocks
        .iter()
        .find(|block| block.text == "Second stream")
        .unwrap();
    assert!(second.id.contains(":c1:"), "{}", second.id);
    replace_text(
        &mut doc,
        0,
        &second.lines[0].id,
        "Changed second",
        &options(),
    )
    .unwrap();
    assert_eq!(
        page_text(&reload(&mut doc), 0),
        ["First stream", "Changed second"]
    );
}

#[test]
fn marked_content_stays_balanced_and_the_new_text_stays_inside_it() {
    let mut pdf = PdfBuilder::new();
    pdf.page(
        &PageSetup::letter(),
        &["/P <</MCID 0>> BDC BT /F1 12 Tf 72 700 Td (Tagged words) Tj ET EMC /P <</MCID 1>> BDC BT /F1 12 Tf 72 600 Td (Other) Tj ET EMC"],
        "/Font << /F1 3 0 R >>",
    );
    let mut doc = load(&pdf.finish());
    let line_id = page_objects(&doc, 0).unwrap().text_blocks[0].lines[0]
        .id
        .clone();
    replace_text(&mut doc, 0, &line_id, "Tagged and edited", &options()).unwrap();
    let content = page_content(&reload(&mut doc), 0);
    assert_eq!(
        content.matches("BDC").count(),
        content.matches("EMC").count()
    );
    let first_end = content.find("EMC").unwrap();
    let new_text_at = content.find("Tm\n[<").expect("the new text");
    assert!(
        new_text_at < first_end,
        "the new text is inside the first marked sequence:\n{content}"
    );
}

#[test]
fn kerning_and_spacing_of_the_rest_of_the_line_are_kept() {
    let mut pdf = PdfBuilder::new();
    // kerned "AVAV" after a word, with character spacing set
    pdf.page(
        &PageSetup::letter(),
        &["BT /F1 20 Tf 1 Tc 72 700 Td [(Start ) (A) 120 (V) 120 (A) 120 (V)] TJ ET"],
        "/Font << /F1 3 0 R >>",
    );
    let mut doc = load(&pdf.finish());
    let before = page_objects(&doc, 0).unwrap();
    let line = &before.text_blocks[0].lines[0];
    assert_eq!(line.text, "Start AVAV");
    let gaps = |line: &letters_edit::TextLine| -> Vec<f32> {
        let boxes: Vec<_> = line
            .runs
            .iter()
            .flat_map(|run| run.char_boxes.clone())
            .collect();
        let n = boxes.len();
        (n - 3..n).map(|i| boxes[i].x - boxes[i - 1].x).collect()
    };
    let old_gaps = gaps(line);
    // deleting "Start": the kerned word keeps its spacing, moved left
    let result = letters_edit::delete_chars(&mut doc, 0, &line.id, 0, 5).unwrap();
    let edited = &result.objects.text_blocks[0].lines[0];
    assert_eq!(edited.text.trim_start(), "AVAV");
    for (old, new) in old_gaps.iter().zip(gaps(edited)) {
        assert!(
            (old - new).abs() < 0.01,
            "kerned gaps {old_gaps:?} became {:?}",
            gaps(edited)
        );
    }
    assert!(
        edited.baseline[0].x < line.baseline[0].x + 80.0 && edited.rect.x < 80.0,
        "the rest closed up"
    );
}

#[test]
fn centred_and_right_aligned_blocks_keep_their_alignment_when_reflowed() {
    let mut pdf = PdfBuilder::new();
    // three centred lines around x = 306 (helvetica widths: 50.03, 72.04 and 54.68)
    let content = "BT /F1 12 Tf 1 0 0 1 280.985 700 Tm (a centred) Tj 1 0 0 1 269.98 686 Tm (block of three) Tj 1 0 0 1 278.66 672 Tm (short lines) Tj ET";
    pdf.page(&PageSetup::letter(), &[content], "/Font << /F1 3 0 R >>");
    let mut doc = load(&pdf.finish());
    let block = page_objects(&doc, 0).unwrap().text_blocks[0].clone();
    assert_eq!(block.alignment, TextAlignment::Center);
    let result = replace_text(
        &mut doc,
        0,
        &block.id,
        "new centred text that wraps",
        &options(),
    )
    .unwrap();
    let after = &result.objects.text_blocks[0];
    for line in &after.lines {
        let centre = (line.baseline[0].x + line.baseline[1].x) / 2.0;
        let block_centre = block.rect.x + block.rect.width / 2.0;
        assert!(
            (centre - block_centre).abs() < 2.0,
            "{} is off centre: {centre} vs {block_centre}",
            line.text
        );
    }
}

#[test]
fn repeated_edits_keep_working_and_converge() {
    let mut pdf = PdfBuilder::new();
    pdf.page(
        &PageSetup::letter(),
        &["BT /F1 12 Tf 72 700 Td (Version 1) Tj ET"],
        "/Font << /F1 3 0 R >>",
    );
    let mut doc = load(&pdf.finish());
    for version in 2..8 {
        let line_id = page_objects(&doc, 0).unwrap().text_blocks[0].lines[0]
            .id
            .clone();
        let result = replace_text(
            &mut doc,
            0,
            &line_id,
            &format!("Version {version}"),
            &options(),
        )
        .unwrap();
        assert_eq!(
            result.objects.text_blocks[0].text,
            format!("Version {version}")
        );
        assert!(
            (result.objects.text_blocks[0].lines[0].baseline[0].x - 72.0).abs() < 0.01,
            "the start never drifts"
        );
    }
    // the same text again changes what is drawn not at all
    let saved = save(&mut doc);
    let line_id = page_objects(&doc, 0).unwrap().text_blocks[0].lines[0]
        .id
        .clone();
    replace_text(&mut doc, 0, &line_id, "Version 7", &options()).unwrap();
    let again = save(&mut doc);
    assert_eq!(
        render(&again, 0).differences_in(&render(&saved, 0), (0, 0, 612, 792)),
        0
    );
    let line_id = page_objects(&doc, 0).unwrap().text_blocks[0].lines[0]
        .id
        .clone();
    insert_text(&mut doc, 0, &line_id, 0, "Final ", &options()).unwrap();
    assert_eq!(find_text(&save(&mut doc), 0, "Final Version 7").len(), 1);
}

#[test]
fn editing_one_page_of_a_long_document_is_quick() {
    let mut pdf = PdfBuilder::new();
    let paragraph: String = (0..40)
        .map(|line| format!("({line}: a line of text on this page) Tj T* "))
        .collect();
    let content = format!("BT /F1 10 Tf 12 TL 72 750 Td {paragraph} ET");
    for _ in 0..100 {
        pdf.page(&PageSetup::letter(), &[&content], "/Font << /F1 3 0 R >>");
    }
    let mut doc = load(&pdf.finish());
    let started = Instant::now();
    let objects = page_objects(&doc, 50).unwrap();
    let line_id = objects.text_blocks[0].lines[20].id.clone();
    replace_text(
        &mut doc,
        50,
        &line_id,
        "20: this line was edited",
        &options(),
    )
    .unwrap();
    let took = started.elapsed();
    assert!(
        took.as_millis() < 1500,
        "reading and editing one page took {took:?}"
    );
    assert!(page_text(&doc, 50)[0].contains("20: this line was edited"));
    assert!(
        page_text(&doc, 49)[0].contains("20: a line of text"),
        "other pages are untouched"
    );
}
