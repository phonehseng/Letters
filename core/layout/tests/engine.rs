//! whole-engine behaviour on small documents: failure paths, the model to
//! page map, footnotes, spacing and the pdf writer. these use the fonts
//! installed on the machine (any font will do; times new roman is assumed
//! where the numbers depend on it, and the tests return early without it).

use letters_layout::fonts::{FontLibrary, SubstitutionReason};
use letters_layout::unsupported::UnsupportedKind;
use letters_layout::{layout, LayoutOptions, ModelPosition, Story};
use letters_office::model::*;

fn has_times() -> bool {
    let mut library = FontLibrary::system();
    library
        .resolve("Times New Roman", false, false)
        .is_some_and(|id| library.font(id).face.family == "Times New Roman")
}

fn document_with(blocks: Vec<Block>) -> Document {
    let mut document = Document::new();
    document.styles.defaults.run.font = Some(FontRef {
        name: "Times New Roman".into(),
        theme: None,
        extra: Vec::new(),
    });
    document.styles.defaults.run.size = Some(24);
    document.blocks = blocks;
    document
}

#[test]
fn a_missing_font_is_substituted_and_reported() {
    let mut document = document_with(vec![Block::Paragraph(Paragraph {
        content: vec![Inline::styled(
            "no such font",
            vec![Mark::FontFamily {
                name: "Letters Font That Does Not Exist".into(),
                theme: None,
                extra: Vec::new(),
            }],
        )],
        ..Default::default()
    })]);
    document.blocks.push(Block::text("plain"));
    let result = layout(&document, &LayoutOptions::default());
    assert_eq!(result.pages.len(), 1);
    let found = result
        .substitutions
        .iter()
        .find(|s| s.requested == "Letters Font That Does Not Exist")
        .expect("the substitution is reported");
    assert_eq!(found.reason, SubstitutionReason::NotInstalled);
}

#[test]
fn unsupported_content_is_listed_and_the_rest_is_laid_out() {
    let mut paragraph = Paragraph::with_text("before the equation ");
    paragraph.content.push(Inline::Preserved {
        xml: "<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>".into(),
    });
    paragraph.content.push(Inline::text(" after"));
    let document = document_with(vec![
        Block::Paragraph(paragraph),
        Block::Preserved {
            xml: "<w:sdt><w:sdtContent/></w:sdt>".into(),
        },
        Block::text("still laid out"),
    ]);
    let result = layout(&document, &LayoutOptions::default());
    let kinds: Vec<UnsupportedKind> = result.unsupported.iter().map(|u| u.kind).collect();
    assert!(kinds.contains(&UnsupportedKind::Equation), "{kinds:?}");
    assert!(
        kinds.contains(&UnsupportedKind::PreservedBlock),
        "{kinds:?}"
    );
    let locations: Vec<&str> = result
        .unsupported
        .iter()
        .map(|u| u.location.as_str())
        .collect();
    assert!(locations.contains(&"body 1"), "{locations:?}");
    let lines: Vec<&letters_layout::result::Line> = result.pages[0].lines.iter().collect();
    assert_eq!(lines.len(), 2);
}

#[test]
fn an_empty_document_still_has_a_page() {
    let result = layout(&Document::default(), &LayoutOptions::default());
    assert_eq!(result.pages.len(), 1);
}

#[test]
fn carets_and_hit_testing_map_both_ways() {
    let document = document_with(vec![Block::text("Hello world, this is a test.")]);
    let result = layout(&document, &LayoutOptions::default());
    let position = ModelPosition {
        story: Story::Body,
        block: vec![0],
        offset: 6,
    };
    let caret = result.locate(&position).expect("caret");
    assert_eq!(caret.page, 0);
    assert!(caret.x > 72.0 && caret.x < 150.0, "{caret:?}");
    let back = result
        .hit_test(0, caret.x + 0.1, (caret.top + caret.bottom) / 2.0)
        .expect("hit");
    assert_eq!(back, position);
    let boxes = result.selection_boxes(
        &position,
        &ModelPosition {
            offset: 11,
            ..position.clone()
        },
    );
    assert_eq!(boxes.len(), 1);
    assert!(boxes[0].1.width > 10.0);
}

#[test]
fn spacing_between_paragraphs_is_the_larger_of_after_and_before() {
    if !has_times() {
        return;
    }
    let mut first = Paragraph::with_text("first");
    first.props.spacing.after = Some(240);
    let mut second = Paragraph::with_text("second");
    second.props.spacing.before = Some(240);
    let mut third = Paragraph::with_text("third");
    third.props.spacing.before = Some(480);
    let document = document_with(vec![
        Block::Paragraph(first),
        Block::Paragraph(second),
        Block::Paragraph(third),
    ]);
    let result = layout(&document, &LayoutOptions::default());
    let lines = &result.pages[0].lines;
    let pitch = lines[1].top - lines[0].top - lines[0].height;
    assert!(
        (pitch - 12.0).abs() < 1e-6,
        "12 after and 12 before leave 12, got {pitch}"
    );
    let pitch = lines[2].top - lines[1].top - lines[1].height;
    assert!(
        (pitch - 24.0).abs() < 1e-6,
        "0 after and 24 before leave 24, got {pitch}"
    );
}

#[test]
fn a_long_footnote_splits_across_pages() {
    if !has_times() {
        return;
    }
    let mut blocks = Vec::new();
    // fill most of the first page, then refer to a note far too long for it
    for _ in 0..40 {
        blocks.push(Block::text("A line of body text that stays short."));
    }
    let mut referring = Paragraph::with_text("The reference");
    referring.content.push(Inline::NoteReference {
        note_id: "n1".into(),
        marks: vec![Mark::Superscript],
    });
    blocks.push(Block::Paragraph(referring));
    blocks.push(Block::text("After the reference."));
    let mut document = document_with(blocks);
    let words = "note text that goes on and on ".repeat(120);
    document.notes.push(Note {
        id: "n1".into(),
        kind: NoteKind::Footnote,
        special: None,
        blocks: vec![Block::text(&words)],
    });
    let result = layout(&document, &LayoutOptions::default());
    let note_pages: Vec<u32> = result
        .pages
        .iter()
        .filter(|page| {
            page.lines
                .iter()
                .any(|line| matches!(line.story, Story::Footnote { .. }))
        })
        .map(|page| page.index)
        .collect();
    assert!(
        note_pages.len() >= 2,
        "the note continues on a later page: {note_pages:?}"
    );
    // the reference and the note's first lines share a page
    let reference_page = result
        .pages
        .iter()
        .find(|page| page.lines.iter().any(|line| line.block == vec![40]))
        .map(|page| page.index)
        .unwrap();
    assert_eq!(note_pages[0], reference_page);
}

#[test]
fn the_pdf_holds_real_text_in_embedded_fonts() {
    let document = document_with(vec![Block::text("Searchable text")]);
    let result = layout(&document, &LayoutOptions::default());
    let bytes = letters_layout::pdf::write_pdf(&result, &document).expect("pdf");
    let pdf = lopdf::Document::load_mem(&bytes).expect("a valid pdf");
    assert_eq!(pdf.get_pages().len(), 1);
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("%PDF-1.7"));
    // the font is a composite truetype font with a unicode map
    let fonts: Vec<_> = pdf
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| {
            dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Type0".as_slice())
        })
        .collect();
    assert!(!fonts.is_empty());
    assert!(fonts[0].get(b"ToUnicode").is_ok());
}

#[test]
fn page_breaks_and_sections_make_pages() {
    let mut document = document_with(vec![
        Block::text("one"),
        Block::PageBreak,
        Block::text("two"),
        Block::SectionBreak,
        Block::text("three"),
    ]);
    let mut landscape = Section::default();
    landscape.page.width = 15840;
    landscape.page.height = 12240;
    document.sections = vec![Section::default(), landscape];
    let result = layout(&document, &LayoutOptions::default());
    assert_eq!(result.pages.len(), 3);
    assert_eq!(result.pages[2].width, 792.0);
}
