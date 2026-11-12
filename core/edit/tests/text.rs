//! reading text as blocks and lines, and editing it in place.

mod support;

use letters_edit::{
    delete_chars, delete_text, insert_text, page_objects, replace_text, set_text_style, Color,
    EditError, FallbackKind, TextAlignment, TextEditOptions, TextStyleChange,
};
use support::*;

const HELVETICA_RESOURCES: &str = "/Font << /F1 3 0 R >>";

/// a heading, then a three-line paragraph, then a footer.
const SAMPLE: &str = "BT /F1 20 Tf 72 700 Td (Heading) Tj ET\n\
BT /F1 12 Tf 14 TL 72 650 Td (The first line of text) Tj T* (and a second line here) Tj T* (ends the paragraph.) Tj ET\n\
BT /F1 10 Tf 72 100 Td (Footer stays put) Tj ET";

fn sample(setup: PageSetup) -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    pdf.page(&setup, &[SAMPLE], HELVETICA_RESOURCES);
    pdf.finish()
}

fn options() -> TextEditOptions {
    TextEditOptions::default()
}

#[test]
fn text_is_grouped_into_blocks_lines_and_runs() {
    let doc = load(&sample(PageSetup::letter()));
    let objects = page_objects(&doc, 0).unwrap();
    let texts: Vec<&str> = objects
        .text_blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect();
    assert_eq!(
        texts,
        [
            "Heading",
            "The first line of text\nand a second line here\nends the paragraph.",
            "Footer stays put"
        ]
    );
    let paragraph = &objects.text_blocks[1];
    assert_eq!(paragraph.lines.len(), 3);
    assert_eq!(paragraph.alignment, TextAlignment::Left);
    assert!((paragraph.lines[0].baseline[0].x - 72.0).abs() < 0.01);
    assert!(
        (paragraph.lines[0].baseline[0].y - 142.0).abs() < 0.01,
        "792 - 650 = 142 from the top"
    );
    assert!((paragraph.lines[1].baseline[0].y - 156.0).abs() < 0.01);
    let run = &paragraph.lines[0].runs[0];
    assert_eq!(run.font.name, "Helvetica");
    assert!(!run.font.embedded && run.font.editable_chars.contains('é'));
    assert!((run.size - 12.0).abs() < 1e-3);
    assert_eq!(run.char_boxes.len(), run.text.chars().count());
    // ids name the stream and operator
    assert!(
        paragraph.lines[1].id.starts_with("line:") && paragraph.lines[1].id.ends_with(":c0:11:0"),
        "{}",
        paragraph.lines[1].id
    );
}

#[test]
fn word_gaps_made_by_moving_become_spaces() {
    let mut pdf = PdfBuilder::new();
    pdf.page(
        &PageSetup::letter(),
        &["BT /F1 12 Tf 72 700 Td [(Hello) -278 (world)] TJ ET"],
        HELVETICA_RESOURCES,
    );
    let doc = load(&pdf.finish());
    assert_eq!(page_text(&doc, 0), ["Hello world"]);
}

#[test]
fn replacing_a_line_keeps_its_start_and_baseline_on_every_layout() {
    for (layout, setup) in layouts() {
        let bytes = sample(setup);
        let mut doc = load(&bytes);
        let before = page_objects(&doc, 0).unwrap();
        let line = &before.text_blocks[1].lines[0];
        let result = replace_text(
            &mut doc,
            0,
            &line.id,
            "A brand new first line, longer",
            &options(),
        )
        .unwrap();
        assert!(
            result.notices.is_empty(),
            "{layout}: helvetica can draw all of it"
        );
        let saved = save(&mut doc);
        let reloaded = load(&saved);

        let after = page_objects(&reloaded, 0).unwrap();
        let new_line = &after.text_blocks[1].lines[0];
        assert_eq!(new_line.text, "A brand new first line, longer", "{layout}");
        assert!(
            new_line.baseline[0].minus_distance(line.baseline[0]) < 0.05,
            "{layout}: {:?} vs {:?}",
            new_line.baseline[0],
            line.baseline[0]
        );
        assert_eq!(
            after.text_blocks[1].lines[1].text, "and a second line here",
            "{layout}: the next line is untouched"
        );
        assert_eq!(after.text_blocks[2].text, "Footer stays put");

        // pdfium reads the new text too, and nothing else on the page moved
        assert_eq!(
            find_text(&saved, 0, "brand new first line").len(),
            1,
            "{layout}"
        );
        assert!(
            find_text(&saved, 0, "The first line").is_empty(),
            "{layout}"
        );
        let old_render = render(&bytes, 0);
        let new_render = render(&saved, 0);
        let footer = before.text_blocks[2].rect;
        let heading = before.text_blocks[0].rect;
        for rect in [footer, heading] {
            let region = (
                rect.x as u32,
                rect.y as u32,
                rect.right() as u32 + 1,
                rect.bottom() as u32 + 1,
            );
            assert_eq!(
                new_render.differences_in(&old_render, region),
                0,
                "{layout}: untouched text changed"
            );
        }
    }
}

trait Distance {
    fn minus_distance(&self, other: letters_edit::Point) -> f32;
}

impl Distance for letters_edit::Point {
    fn minus_distance(&self, other: letters_edit::Point) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

#[test]
fn the_new_text_starts_where_the_old_text_started() {
    let bytes = sample(PageSetup::letter());
    let mut doc = load(&bytes);
    let line_id = page_objects(&doc, 0).unwrap().text_blocks[0].lines[0]
        .id
        .clone();
    replace_text(&mut doc, 0, &line_id, "Headline", &options()).unwrap();
    let saved = save(&mut doc);
    // "Heading" and "Headline" both start with an H at x = 72
    let band = (72, 100);
    assert_eq!(
        render(&saved, 0).first_dark_column(band.0, band.1),
        render(&bytes, 0).first_dark_column(band.0, band.1)
    );
    assert_eq!(page_text(&load(&saved), 0)[0], "Headline");
}

#[test]
fn inserting_and_deleting_characters_moves_the_rest_of_the_line() {
    let mut doc = load(&sample(PageSetup::letter()));
    let line = page_objects(&doc, 0).unwrap().text_blocks[1].lines[0].clone();
    let result = insert_text(&mut doc, 0, &line.id, 4, "very ", &options()).unwrap();
    let edited = &result.objects.text_blocks[1].lines[0];
    assert_eq!(edited.text, "The very first line of text");
    // the end of the line moved right by the width of "very "
    assert!(edited.baseline[1].x > line.baseline[1].x + 20.0);

    let result = delete_chars(&mut doc, 0, &edited.id, 4, 9).unwrap();
    assert_eq!(
        result.objects.text_blocks[1].lines[0].text,
        "The first line of text"
    );
    let back = &result.objects.text_blocks[1].lines[0];
    assert!(
        (back.baseline[1].x - line.baseline[1].x).abs() < 0.05,
        "the line is as long as it was"
    );
    let saved = save(&mut doc);
    assert_eq!(find_text(&saved, 0, "The first line of text").len(), 1);
}

#[test]
fn deleting_a_line_leaves_the_rest() {
    let mut doc = load(&sample(PageSetup::letter()));
    let line_id = page_objects(&doc, 0).unwrap().text_blocks[1].lines[1]
        .id
        .clone();
    let result = delete_text(&mut doc, 0, &line_id).unwrap();
    let texts: Vec<&str> = result
        .objects
        .text_blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect();
    assert!(
        texts.iter().all(|text| !text.contains("second line")),
        "{texts:?}"
    );
    let saved = save(&mut doc);
    assert!(find_text(&saved, 0, "second line").is_empty());
    assert_eq!(find_text(&saved, 0, "ends the paragraph").len(), 1);
}

#[test]
fn colour_and_size_can_change_on_part_of_a_line() {
    let mut doc = load(&sample(PageSetup::letter()));
    let line_id = page_objects(&doc, 0).unwrap().text_blocks[1].lines[0]
        .id
        .clone();
    let red = TextStyleChange {
        color: Some(Color::new(255, 0, 0)),
        ..Default::default()
    };
    let result = set_text_style(&mut doc, 0, &line_id, Some((4, 9)), &red, &options()).unwrap();
    let runs = &result.objects.text_blocks[1].lines[0].runs;
    let red_run = runs
        .iter()
        .find(|run| run.color == Color::new(255, 0, 0))
        .expect("a red run");
    assert_eq!(red_run.text.trim(), "first");
    let saved = save(&mut doc);
    let image = render(&saved, 0);
    let box_ = red_run.char_boxes[0];
    let reds = image.count(
        (
            box_.x as u32,
            box_.y as u32,
            box_.right() as u32 + 1,
            box_.bottom() as u32 + 1,
        ),
        |p| p[0] > 180 && p[1] < 100,
    );
    assert!(reds > 3, "the f is drawn in red");
    // the text after the red word keeps its colour
    let rest = runs.last().unwrap();
    assert_eq!(rest.color, Color::BLACK);
    assert_eq!(
        page_text(&load(&saved), 0)[1].lines().next(),
        Some("The first line of text")
    );

    let bigger = TextStyleChange {
        size: Some(18.0),
        ..Default::default()
    };
    let line_id = result.objects.text_blocks[0].lines[0].id.clone();
    let result = set_text_style(&mut doc, 0, &line_id, None, &bigger, &options()).unwrap();
    assert!((result.objects.text_blocks[0].lines[0].runs[0].size - 18.0).abs() < 0.01);
}

#[test]
fn reflowing_a_paragraph_keeps_it_inside_its_width() {
    let mut doc = load(&sample(PageSetup::letter()));
    let before = page_objects(&doc, 0).unwrap();
    let block = &before.text_blocks[1];
    let long = "This replacement paragraph is a good deal longer than the one it replaces, so it needs more lines to fit.";
    let result = replace_text(&mut doc, 0, &block.id, long, &options()).unwrap();
    let after = result
        .objects
        .text_blocks
        .iter()
        .find(|b| b.text.starts_with("This replacement"))
        .expect("the new paragraph");
    assert!(after.lines.len() > 3);
    for line in &after.lines {
        assert!(
            line.rect.right() <= block.rect.right() + 1.0,
            "{} runs past the block",
            line.text
        );
        assert!((line.baseline[0].x - 72.0).abs() < 0.05, "left aligned");
    }
    let joined = after.text.replace('\n', " ");
    assert_eq!(joined, long);
    let overflow = result.overflow.expect("more lines than before");
    assert_eq!(overflow.lines_before, 3);
    assert_eq!(overflow.lines_after, after.lines.len());
    // lines keep the paragraph's spacing
    assert!((after.lines[1].baseline[0].y - after.lines[0].baseline[0].y - 14.0).abs() < 0.05);
    let saved = save(&mut doc);
    assert_eq!(find_text(&saved, 0, "good deal longer").len(), 1);
    assert_eq!(find_text(&saved, 0, "Footer stays put").len(), 1);
}

#[test]
fn characters_the_font_lacks_fall_back_and_are_reported() {
    // a subset TrueType font with a custom encoding: only H, e, l, o and space
    let font_file = font::box_font("LettersBoxes", &font::hello_glyphs(), 0);
    let mut pdf = PdfBuilder::new();
    let file = pdf.stream(&format!("/Length1 {}", font_file.len()), &font_file);
    let descriptor = pdf.add(format!(
        "<< /Type /FontDescriptor /FontName /ABCDEF+LettersBoxes /Flags 32 /FontBBox [0 0 700 700] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 {file} 0 R >>"
    ));
    let font_object = pdf.add(format!(
        "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+LettersBoxes /FirstChar 1 /LastChar 5 /Widths [300 700 550 250 550] \
         /Encoding << /Type /Encoding /Differences [1 /space /H /e /l /o] >> /FontDescriptor {descriptor} 0 R >>"
    ));
    pdf.page(
        &PageSetup::letter(),
        &["BT /F2 24 Tf 72 700 Td <0203040405> Tj ET"],
        &format!("/Font << /F2 {font_object} 0 R >>"),
    );
    let mut doc = load(&pdf.finish());

    let objects = page_objects(&doc, 0).unwrap();
    let line = &objects.text_blocks[0].lines[0];
    assert_eq!(line.text, "Hello");
    let font_info = &line.runs[0].font;
    assert!(font_info.subset && font_info.embedded);
    assert_eq!(
        font_info.editable_chars, " Helo",
        "exactly the glyphs in the subset"
    );

    // only letters the subset has: the font is kept
    let kept = replace_text(&mut doc, 0, &line.id, "Hole", &options()).unwrap();
    assert!(kept.notices.is_empty());
    assert_eq!(kept.objects.text_blocks[0].lines[0].runs.len(), 1);
    assert_eq!(
        kept.objects.text_blocks[0].lines[0].runs[0].font.name,
        "LettersBoxes"
    );

    // a letter it lacks: a standard font draws it, and the person is told
    let line_id = kept.objects.text_blocks[0].lines[0].id.clone();
    let fallback = replace_text(&mut doc, 0, &line_id, "Help", &options()).unwrap();
    assert_eq!(fallback.notices.len(), 1);
    let notice = &fallback.notices[0];
    assert_eq!(notice.kind, FallbackKind::StandardFont);
    assert_eq!(notice.characters, "p");
    assert_eq!(
        notice.message,
        "The original font can’t draw “p”, so Helvetica was used for the new text."
    );
    let saved = save(&mut doc);
    assert_eq!(
        find_text(&saved, 0, "Help").len(),
        1,
        "the mixed-font text reads as one word"
    );

    // refusing fallbacks leaves the document as it was
    let strict = TextEditOptions {
        allow_standard_fonts: false,
        allow_installed_fonts: false,
        ..options()
    };
    let line_id = fallback.objects.text_blocks[0].lines[0].id.clone();
    let before = save(&mut doc);
    let error = replace_text(&mut doc, 0, &line_id, "Hop", &strict).unwrap_err();
    assert_eq!(error, EditError::UnsupportedCharacters("p".into()));
    assert_eq!(save(&mut doc), before);
}

#[test]
fn stale_and_unknown_ids_are_refused() {
    let mut doc = load(&sample(PageSetup::letter()));
    let line_id = page_objects(&doc, 0).unwrap().text_blocks[0].lines[0]
        .id
        .clone();
    replace_text(&mut doc, 0, &line_id, "Changed", &options()).unwrap();
    // the page changed, so the old id no longer names anything
    assert_eq!(
        replace_text(&mut doc, 0, &line_id, "Again", &options()).unwrap_err(),
        EditError::UnknownObject
    );
    assert_eq!(
        replace_text(&mut doc, 0, "line:nothing", "x", &options()).unwrap_err(),
        EditError::UnknownObject
    );
    assert_eq!(
        replace_text(&mut doc, 3, "line:nothing", "x", &options()).unwrap_err(),
        EditError::UnknownPage(3)
    );
    let fresh = page_objects(&doc, 0).unwrap().text_blocks[0].lines[0]
        .id
        .clone();
    assert_eq!(
        insert_text(&mut doc, 0, &fresh, 99, "x", &options()).unwrap_err(),
        EditError::OutOfRange
    );
}

#[test]
fn encrypted_documents_must_be_unlocked_first() {
    let mut doc = load(&sample(PageSetup::letter()));
    let encryption = doc.add_object(lopdf::Dictionary::new());
    doc.trailer
        .set("Encrypt", lopdf::Object::Reference(encryption));
    assert_eq!(page_objects(&doc, 0).unwrap_err(), EditError::Locked);
    let error = replace_text(&mut doc, 0, "line:x", "y", &options()).unwrap_err();
    assert!(error.to_string().contains("Unlock it first"));
}

#[test]
fn a_hand_made_composite_font_is_read_and_edited_with_two_byte_codes() {
    // a Type0 font with Identity-H codes (glyph numbers), /W widths and a
    // /ToUnicode map for H e l o (glyphs 2 to 5); the full font is embedded
    let font_file = font::box_font("LettersBoxes", &font::hello_glyphs(), 0);
    let mut pdf = PdfBuilder::new();
    let file = pdf.stream(&format!("/Length1 {}", font_file.len()), &font_file);
    let descriptor = pdf.add(format!(
        "<< /Type /FontDescriptor /FontName /LettersBoxes /Flags 32 /FontBBox [0 0 700 700] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 {file} 0 R >>"
    ));
    let cid_font = pdf.add(format!(
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /LettersBoxes /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
         /FontDescriptor {descriptor} 0 R /DW 1000 /W [2 [700 550 250 550]] /CIDToGIDMap /Identity >>"
    ));
    let to_unicode = pdf.stream(
        "",
        b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap 1 begincodespacerange <0000> <FFFF> endcodespacerange \
          4 beginbfchar <0002> <0048> <0003> <0065> <0004> <006C> <0005> <006F> endbfchar endcmap end end",
    );
    let type0 = pdf.add(format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /LettersBoxes /Encoding /Identity-H /DescendantFonts [{cid_font} 0 R] /ToUnicode {to_unicode} 0 R >>"
    ));
    pdf.page(
        &PageSetup::letter(),
        &["BT /F5 24 Tf 72 700 Td <00020003000400040005> Tj ET"],
        &format!("/Font << /F5 {type0} 0 R >>"),
    );
    let mut doc = load(&pdf.finish());

    let line = page_objects(&doc, 0).unwrap().text_blocks[0].lines[0].clone();
    assert_eq!(line.text, "Hello");
    assert_eq!(line.runs[0].font.kind, letters_edit::FontKind::Composite);
    // the font's own cmap also has a space, which /ToUnicode didn't list
    assert_eq!(line.runs[0].font.editable_chars, " Helo");

    let result = replace_text(&mut doc, 0, &line.id, "Hello Hole", &options()).unwrap();
    assert!(
        result.notices.is_empty(),
        "the original font draws all of it"
    );
    assert_eq!(result.objects.text_blocks[0].lines[0].runs.len(), 1);
    let saved = save(&mut doc);
    // the space was added to /ToUnicode, so the words copy out correctly
    assert_eq!(find_text(&saved, 0, "Hello Hole").len(), 1);
    assert!(
        page_content(&load(&saved), 0).contains("<0002000300040004000500010002000500040003>"),
        "two-byte codes, the space from the font's own cmap"
    );
}

#[test]
fn options_and_results_use_camel_case_json() {
    let options: TextEditOptions = serde_json::from_str("{}").unwrap();
    assert_eq!(options, TextEditOptions::default());
    assert!(options.reflow && options.allow_installed_fonts && options.allow_standard_fonts);
    let options: TextEditOptions = serde_json::from_str(r#"{"allowStandardFonts":false}"#).unwrap();
    assert!(!options.allow_standard_fonts && options.reflow);
    let font: letters_edit::NewTextFont =
        serde_json::from_str(r#"{"type":"standard","font":"timesBoldItalic"}"#).unwrap();
    assert_eq!(
        font,
        letters_edit::NewTextFont::Standard {
            font: letters_edit::StandardFont::TimesBoldItalic
        }
    );
    let objects = page_objects(&load(&sample(PageSetup::letter())), 0).unwrap();
    let json = serde_json::to_value(&objects).unwrap();
    assert!(json["textBlocks"][1]["lines"][0]["runs"][0]["font"]["editableChars"].is_string());
    assert_eq!(json["textBlocks"][1]["alignment"], "left");
}
