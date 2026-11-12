//! extending a simple (one-byte) embedded TrueType font with characters
//! its subset lacks: new codes, widths, encoding names and cmap entries,
//! with every existing code drawn exactly as before.

mod support;

use letters_edit::{page_objects, reflow_paragraph, ReflowOptions};
use support::subset;
use support::{load, render, save, PageSetup, PdfBuilder};

const ORIGINAL: &str = "Hello world from the office";

fn arial() -> Option<Vec<u8>> {
    std::fs::read("C:/Windows/Fonts/arial.ttf").ok()
}

/// a page in a WinAnsi subset of Arial, written as a simple TrueType font
/// with widths for every code from 32 to 126.
fn simple_font_pdf(font: &[u8]) -> Vec<u8> {
    let program = subset::subset(font, ORIGINAL);
    let glyphs = subset::glyphs_for(font, &(32u8..=126).map(char::from).collect::<String>());
    let widths: Vec<String> = (32u8..=126)
        .map(|code| {
            format!(
                "{:.0}",
                glyphs
                    .get(&char::from(code))
                    .map_or(0.0, |glyph| subset::advance(font, *glyph))
            )
        })
        .collect();
    let mut pdf = PdfBuilder::new();
    let file = pdf.stream(&format!("/Length1 {}", program.len()), &program);
    let descriptor = pdf.add(format!(
        "<< /Type /FontDescriptor /FontName /ABCDEF+ArialMT /Flags 32 /FontBBox [-665 -325 2000 1006] \
         /ItalicAngle 0 /Ascent 905 /Descent -212 /CapHeight 716 /StemV 80 /FontFile2 {file} 0 R >>"
    ));
    let font_object = pdf.add(format!(
        "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+ArialMT /FirstChar 32 /LastChar 126 \
         /Widths [{}] /Encoding /WinAnsiEncoding /FontDescriptor {descriptor} 0 R >>",
        widths.join(" ")
    ));
    let content = format!("BT /F1 12 Tf 72 700 Td ({ORIGINAL}) Tj ET");
    pdf.page(
        &PageSetup::letter(),
        &[&content],
        &format!("/Font << /F1 {font_object} 0 R >>"),
    );
    pdf.finish()
}

#[test]
fn a_simple_truetype_subset_gains_the_characters_it_lacks() {
    let Some(font) = arial() else {
        println!("skipped: Arial isn't installed here");
        return;
    };
    let mut doc = load(&simple_font_pdf(&font));
    let before = page_objects(&doc, 0).unwrap();
    let block = &before.text_blocks[0];
    let font_id = block.lines[0].runs[0].font.id.clone();
    let widths_before = font_widths(&doc);
    let text = "Hello wörld from the café, 5 €";
    let result = reflow_paragraph(&mut doc, 0, &block.id, text, &ReflowOptions::default()).unwrap();
    assert!(
        result.applied && result.notices.is_empty(),
        "{:?}",
        result.notices
    );
    assert_eq!(result.summary.fonts_extended.len(), 1);
    for run in result.objects.text_blocks[0]
        .lines
        .iter()
        .flat_map(|line| &line.runs)
    {
        assert_eq!(run.font.id, font_id, "one font draws all of it");
    }
    let widths_after = font_widths(&doc);
    assert_eq!(
        &widths_after[..widths_before.len()],
        &widths_before[..],
        "existing codes keep their widths"
    );
    let saved = save(&mut doc);
    assert!(
        !support::find_text(&saved, 0, "wörld from the café").is_empty(),
        "the new characters copy and search"
    );
    // the ö is drawn: ink inside its box
    let objects = page_objects(&load(&saved), 0).unwrap();
    let run = &objects.text_blocks[0].lines[0].runs[0];
    let index = run.text.chars().position(|c| c == 'ö').unwrap();
    let place = run.char_boxes[index];
    let pixels = render(&saved, 0);
    let region = (
        place.x as u32,
        place.y as u32,
        place.right() as u32 + 1,
        place.bottom() as u32 + 1,
    );
    assert!(
        pixels.count(region, support::is_dark) > 5,
        "the added glyph has ink"
    );
}

fn font_widths(doc: &lopdf::Document) -> Vec<f32> {
    let page = doc.get_pages()[&1];
    let fonts = doc.get_page_fonts(page).unwrap();
    let font = fonts.values().next().unwrap();
    font.get(b"Widths")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|width| width.as_float().unwrap())
        .collect()
}
