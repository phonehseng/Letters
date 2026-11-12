//! new text in standard and embedded fonts, composite (CID) fonts, and
//! copying a missing glyph from the same font installed on the computer.

mod support;

use letters_edit::{
    add_text, page_objects, replace_text, Color, EditError, FallbackKind, FontKind, NewText,
    NewTextFont, Point, StandardFont, TextEditOptions,
};
use support::*;

fn blank() -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    pdf.page(
        &PageSetup::letter(),
        &["BT /F1 10 Tf 72 72 Td (Footer) Tj ET"],
        "/Font << /F1 3 0 R >>",
    );
    pdf.finish()
}

fn new_text(text: &str, font: NewTextFont) -> NewText {
    NewText {
        position: Point::new(100.0, 100.0),
        text: text.into(),
        font,
        size: 24.0,
        color: Color::new(0, 0, 160),
    }
}

#[test]
fn new_text_in_a_standard_font_is_real_page_text() {
    for (layout, setup) in layouts() {
        let mut pdf = PdfBuilder::new();
        pdf.page(
            &setup,
            &["BT /F1 10 Tf 72 72 Td (Footer) Tj ET"],
            "/Font << /F1 3 0 R >>",
        );
        let mut doc = load(&pdf.finish());
        let result = add_text(
            &mut doc,
            0,
            &new_text(
                "Added text\nsecond line",
                NewTextFont::Standard {
                    font: StandardFont::TimesBold,
                },
            ),
        )
        .unwrap();
        let block = result
            .objects
            .text_blocks
            .iter()
            .find(|block| block.text.starts_with("Added"))
            .expect("the new text is listed");
        assert_eq!(block.text, "Added text\nsecond line", "{layout}");
        assert!(block.rotation.abs() < 0.01, "{layout}: upright on screen");
        // the top of the first line box is where it was asked to be
        let top_line = &block.lines[0];
        assert!((top_line.baseline[0].x - 100.0).abs() < 0.05, "{layout}");
        assert!(
            (top_line.baseline[0].y - (100.0 + 0.676 * 24.0)).abs() < 0.05,
            "{layout}: {:?}",
            top_line.baseline[0]
        );
        assert_eq!(top_line.runs[0].font.name, "Times-Bold");
        let saved = save(&mut doc);
        assert_eq!(find_text(&saved, 0, "Added text").len(), 1, "{layout}");
        let shown = render(&saved, 0);
        assert!(
            shown.count((100, 100, 220, 125), |p| p[2] > 100 && p[0] < 60) > 50,
            "{layout}: drawn in dark blue"
        );
    }
}

#[test]
fn an_embedded_font_becomes_a_subset_cid_font_that_copies_and_edits() {
    let font_file = font::box_font("LettersBoxes", &font::hello_glyphs(), 0);
    let mut doc = load(&blank());
    let result = add_text(
        &mut doc,
        0,
        &new_text("Hello", NewTextFont::Embedded { bytes: font_file }),
    )
    .unwrap();
    let block = result
        .objects
        .text_blocks
        .iter()
        .find(|block| block.text == "Hello")
        .expect("copyable text");
    let font = &block.lines[0].runs[0].font;
    assert_eq!(font.kind, FontKind::Composite);
    assert!(font.subset && font.embedded);
    assert_eq!(font.name, "LettersBoxes");
    assert_eq!(
        font.editable_chars, "Helo",
        "only the glyphs the text used are copied in"
    );
    let saved = save(&mut doc);
    assert_eq!(
        find_text(&saved, 0, "Hello").len(),
        1,
        "/ToUnicode makes it readable"
    );
    // the boxes are drawn: H is a 700 unit tall box at 24 points
    let shown = render(&saved, 0);
    assert!(shown.count((100, 100, 200, 125), |p| p[2] > 100 && p[0] < 60) > 100);

    // editing it keeps the font for letters the subset has
    let mut doc = load(&saved);
    let line_id = page_objects(&doc, 0)
        .unwrap()
        .text_blocks
        .iter()
        .find(|b| b.text == "Hello")
        .unwrap()
        .lines[0]
        .id
        .clone();
    let edited = replace_text(&mut doc, 0, &line_id, "Hole", &TextEditOptions::default()).unwrap();
    assert!(edited.notices.is_empty());
    let line = &edited
        .objects
        .text_blocks
        .iter()
        .find(|b| b.text == "Hole")
        .unwrap()
        .lines[0];
    assert_eq!(line.runs.len(), 1);
    assert_eq!(line.runs[0].font.kind, FontKind::Composite);
    assert_eq!(find_text(&save(&mut doc), 0, "Hole").len(), 1);
}

#[test]
fn fonts_that_forbid_embedding_are_refused() {
    // fsType 2: "restricted licence embedding"
    let restricted = font::box_font("NoCopies", &font::hello_glyphs(), 2);
    let mut doc = load(&blank());
    let before = save(&mut doc);
    let error = add_text(
        &mut doc,
        0,
        &new_text("Hello", NewTextFont::Embedded { bytes: restricted }),
    )
    .unwrap_err();
    assert_eq!(error, EditError::EmbeddingNotAllowed("NoCopies".into()));
    assert!(error
        .to_string()
        .contains("doesn’t allow being copied into a PDF"));
    assert_eq!(save(&mut doc), before);

    // a font that lacks a character says which
    let boxes = font::box_font("LettersBoxes", &font::hello_glyphs(), 0);
    let error = add_text(
        &mut doc,
        0,
        &new_text("Help", NewTextFont::Embedded { bytes: boxes }),
    )
    .unwrap_err();
    assert_eq!(error, EditError::UnsupportedCharacters("p".into()));
    let error = add_text(
        &mut doc,
        0,
        &new_text(
            "日本",
            NewTextFont::Standard {
                font: StandardFont::Helvetica,
            },
        ),
    )
    .unwrap_err();
    assert_eq!(error, EditError::UnsupportedCharacters("日本".into()));
}

/// arial ships with windows; elsewhere this test explains why it skips.
fn installed_arial() -> Option<Vec<u8>> {
    let windows = std::env::var_os("WINDIR")?;
    std::fs::read(
        std::path::Path::new(&windows)
            .join("Fonts")
            .join("arial.ttf"),
    )
    .ok()
}

#[test]
fn a_missing_glyph_is_copied_from_the_same_font_installed_on_the_computer() {
    let Some(arial) = installed_arial() else {
        eprintln!("skipped: Arial (C:\\Windows\\Fonts\\arial.ttf) is not installed here");
        return;
    };
    // a pdf whose Arial subset holds only H, e, l and o
    let mut doc = load(&blank());
    add_text(
        &mut doc,
        0,
        &new_text("Hello", NewTextFont::Embedded { bytes: arial }),
    )
    .unwrap();
    let objects = page_objects(&doc, 0).unwrap();
    let line = &objects
        .text_blocks
        .iter()
        .find(|b| b.text == "Hello")
        .unwrap()
        .lines[0];
    assert_eq!(line.runs[0].font.name, "ArialMT");
    assert_eq!(line.runs[0].font.editable_chars, "Helo");

    let result = replace_text(&mut doc, 0, &line.id, "Héllo", &TextEditOptions::default()).unwrap();
    assert_eq!(result.notices.len(), 1);
    let notice = &result.notices[0];
    assert_eq!(notice.kind, FallbackKind::InstalledFont);
    assert_eq!(notice.characters, "é");
    assert_eq!(notice.used_font, "Arial");
    assert!(
        notice
            .message
            .contains("copied from Arial on this computer"),
        "{}",
        notice.message
    );
    let saved = save(&mut doc);
    assert_eq!(find_text(&saved, 0, "Héllo").len(), 1);
    let line = &page_objects(&load(&saved), 0)
        .unwrap()
        .text_blocks
        .iter()
        .find(|b| b.text == "Héllo")
        .unwrap()
        .lines[0]
        .clone();
    assert!(
        line.runs.iter().all(|run| run.font.name == "ArialMT"),
        "every part is Arial"
    );
}
