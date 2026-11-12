//! inheritance, autofit and rendering without any office application:
//! effective values come from the right place in the chain, slides draw
//! to pdf with real text, exports have the pages they promise, missing
//! fonts are replaced and reported, and a 300-slide deck is fast enough.

use std::path::Path;
use std::time::Instant;

use letters_slides::edit::{apply, Operation};
use letters_slides::export::{export_pdf, PdfLayout};
use letters_slides::fonts::FontLibrary;
use letters_slides::inherit::{
    effective_paragraph, effective_run, effective_transform, resolve_typeface, Chain,
};
use letters_slides::model::{Autofit, Object, Presentation, Typeface};
use letters_slides::render::render_pdf;
use letters_slides::{read_pptx, write_pptx};

fn deck(name: &str) -> Presentation {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/decks")
        .join(name);
    read_pptx(&std::fs::read(path).unwrap()).unwrap()
}

fn page_count(pdf: &[u8]) -> usize {
    lopdf::Document::load_mem(pdf).unwrap().get_pages().len()
}

#[test]
fn placeholders_inherit_position_and_text_style_from_layout_and_master() {
    let presentation = deck("pp_title_bullets.pptx");
    let slide = &presentation.slides[1];
    let chain = Chain::of_slide(&presentation, slide);
    let body = slide
        .objects
        .iter()
        .find(|o| o.placeholder().is_some_and(|p| !p.kind.is_title()))
        .unwrap();
    // the slide's body placeholder has no position of its own
    assert!(body.transform().is_none());
    let placed = effective_transform(&chain, body).unwrap();
    assert_eq!((placed.x, placed.y), (838_200, 1_825_625));
    let text = body.text().unwrap();
    let second_level = text.paragraphs.iter().find(|p| p.level() == 1).unwrap();
    let paragraph = effective_paragraph(&chain, body, text, second_level);
    let run = effective_run(&paragraph, second_level.runs[0].props());
    // powerpoint's office theme: 24 pt at the second level, 28 pt at the first
    assert_eq!(run.size, Some(2_400));
    let latin = run.latin.unwrap_or_else(|| Typeface::named("+mn-lt"));
    assert_eq!(
        resolve_typeface(chain.theme, &latin.typeface),
        presentation.themes[0].fonts.minor_latin
    );
}

#[test]
fn stored_autofit_scale_is_read() {
    let presentation = deck("pp_autofit_notes.pptx");
    let shrunk = presentation.slides[0]
        .objects
        .iter()
        .filter_map(Object::text)
        .any(|t| matches!(t.body.autofit, Some(Autofit::Shrink { .. })));
    assert!(shrunk, "the text box that shrinks on overflow says so");
}

#[test]
fn every_deck_renders_one_page_per_slide_with_its_text() {
    let mut fonts = FontLibrary::system();
    for name in [
        "pp_title_bullets.pptx",
        "pp_table.pptx",
        "pp_groups_connectors.pptx",
        "lo_two_content.pptx",
    ] {
        let presentation = deck(name);
        let (pdf, _) = render_pdf(&presentation, None, &mut fonts).unwrap();
        assert_eq!(page_count(&pdf), presentation.slides.len(), "{name}");
        let document = lopdf::Document::load_mem(&pdf).unwrap();
        let text = document.extract_text(&[1]).unwrap_or_default();
        assert!(
            !text.trim().is_empty(),
            "{name}: the first page has real text"
        );
    }
}

#[test]
fn notes_pages_and_handouts_have_the_promised_pages() {
    let mut fonts = FontLibrary::system();
    let mut presentation = deck("pp_title_bullets.pptx");
    for _ in 0..5 {
        apply(&mut presentation, &Operation::DuplicateSlide { index: 0 }).unwrap();
    }
    let slides = presentation.slides.len();
    assert_eq!(slides, 7);
    let (notes, _) = export_pdf(&presentation, PdfLayout::NotesPages, &mut fonts).unwrap();
    assert_eq!(page_count(&notes), 7);
    for (per_page, pages) in [(2u8, 4), (3, 3), (4, 2), (6, 2)] {
        let (handout, _) =
            export_pdf(&presentation, PdfLayout::Handouts(per_page), &mut fonts).unwrap();
        assert_eq!(page_count(&handout), pages, "{per_page} a page");
    }
    assert!(export_pdf(&presentation, PdfLayout::Handouts(5), &mut fonts).is_err());
}

#[test]
fn missing_fonts_are_replaced_and_reported() {
    let mut presentation = deck("pp_shapes.pptx");
    for object in &mut presentation.slides[0].objects {
        if let Some(text) = object.text_mut() {
            for paragraph in &mut text.paragraphs {
                for run in &mut paragraph.runs {
                    run.props_mut().latin = Some(Typeface::named("No Such Font Anywhere"));
                }
            }
        }
    }
    let mut fonts = FontLibrary::system();
    let (_, report) = render_pdf(&presentation, None, &mut fonts).unwrap();
    assert!(report
        .substitutions
        .iter()
        .any(|s| s.requested == "No Such Font Anywhere"));
}

/// 300 slides: open, render every slide, save. the times are printed
/// (run with --release --nocapture for the numbers in the readme) and
/// held to generous limits so a slow machine does not fail.
#[test]
fn a_300_slide_deck_opens_renders_and_saves_in_time() {
    let mut presentation = deck("pp_title_bullets.pptx");
    while presentation.slides.len() < 300 {
        let index = presentation.slides.len() % 2;
        apply(&mut presentation, &Operation::DuplicateSlide { index }).unwrap();
    }
    let bytes = write_pptx(&presentation).unwrap();
    let started = Instant::now();
    let opened = read_pptx(&bytes).unwrap();
    let open_time = started.elapsed();
    let started = Instant::now();
    let mut fonts = FontLibrary::system();
    let (pdf, _) = render_pdf(&opened, None, &mut fonts).unwrap();
    let render_time = started.elapsed();
    let started = Instant::now();
    let saved = write_pptx(&opened).unwrap();
    let save_time = started.elapsed();
    println!("300 slides: open {open_time:?}, render all {render_time:?} ({} kB pdf), save {save_time:?} ({} kB)", pdf.len() / 1024, saved.len() / 1024);
    assert_eq!(opened.slides.len(), 300);
    assert!(open_time.as_secs() < 30 && render_time.as_secs() < 60 && save_time.as_secs() < 30);
}
