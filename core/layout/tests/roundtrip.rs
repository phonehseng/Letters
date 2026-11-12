//! exact-position content, round trip: a pdf page read into positioned
//! runs, laid out (nothing moves), written with our pdf writer and read
//! back with the same reader. every glyph must come back within 0.01 pt.
//!
//! the source pdfs are the fidelity corpus (read only). fonts are looked up
//! by family here, so glyph shapes may differ from the source when its
//! font is not installed; positions are what this test is about.

#[path = "oracle/pdf_glyphs.rs"]
mod pdf_glyphs;

use std::path::Path;

use letters_layout::{layout_exact, ExactFont, ExactGlyph, ExactPage, LayoutOptions};
use pdf_glyphs::{read_pdf, Glyph, PdfPage};

const CORPUS: &str = "C:/Letters-fidelity/tools/fidelity/corpus";

/// "ABCDEF+CrimsonText-Bold" -> ("CrimsonText", bold, italic)
fn family_of(font: &str) -> ExactFont {
    let name = font.split_once('+').map_or(font, |(_, rest)| rest);
    let lower = name.to_lowercase();
    let family = name.split(['-', ',']).next().unwrap_or(name);
    ExactFont::Family {
        family: spaced(family),
        bold: lower.contains("bold"),
        italic: lower.contains("italic") || lower.contains("oblique"),
    }
}

/// "TimesNewRomanPSMT" -> "Times New Roman"; "CrimsonText" -> "Crimson Text"
fn spaced(name: &str) -> String {
    let name = name
        .trim_end_matches("PSMT")
        .trim_end_matches("MT")
        .trim_end_matches("PS");
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
        if index > 0 && ch.is_uppercase() && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// one exact page from a source page: a run per font and size, in order.
fn exact_page(page: &PdfPage) -> ExactPage {
    let mut exact = ExactPage::new(page.width, page.height);
    let mut current: Option<(String, f64, Vec<ExactGlyph>)> = None;
    for glyph in &page.glyphs {
        let same = current
            .as_ref()
            .is_some_and(|(font, size, _)| *font == glyph.font && *size == glyph.size);
        if !same {
            if let Some((font, size, glyphs)) = current.take() {
                exact.push_run(family_of(&font), size, None, glyphs);
            }
            current = Some((glyph.font.clone(), glyph.size, Vec::new()));
        }
        current.as_mut().expect("set above").2.push(ExactGlyph {
            text: glyph.ch.to_string(),
            glyph: None,
            x: glyph.x,
            y: glyph.y,
        });
    }
    if let Some((font, size, glyphs)) = current {
        exact.push_run(family_of(&font), size, None, glyphs);
    }
    exact
}

fn max_error(source: &[PdfPage], ours: &[PdfPage]) -> (usize, f64) {
    let mut count = 0;
    let mut worst: f64 = 0.0;
    for (a, b) in source.iter().zip(ours) {
        let mut theirs: Vec<&Glyph> = a.glyphs.iter().collect();
        let mut mine: Vec<&Glyph> = b.glyphs.iter().collect();
        // compare in a fixed order: by baseline, then x
        let key = |g: &&Glyph| ((g.y * 100.0).round() as i64, (g.x * 100.0).round() as i64);
        theirs.sort_by_key(key);
        mine.sort_by_key(key);
        assert_eq!(theirs.len(), mine.len(), "glyph count per page");
        for (g, h) in theirs.iter().zip(&mine) {
            assert_eq!(g.ch, h.ch);
            worst = worst.max((g.x - h.x).abs()).max((g.y - h.y).abs());
            count += 1;
        }
    }
    (count, worst)
}

#[test]
#[ignore = "needs the fidelity corpus pdfs and vendor/pdfium"]
fn corpus_pdfs_round_trip_within_a_hundredth_of_a_point() {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/out/roundtrip");
    std::fs::create_dir_all(&out).expect("out folder");
    for name in [
        "syllabus",
        "research_paper",
        "lab_report",
        "journal",
        "form",
    ] {
        let source_path = Path::new(CORPUS).join(format!("{name}.pdf"));
        let Ok(source) = read_pdf(&source_path) else {
            println!("{name}: not found, skipped");
            continue;
        };
        let pages: Vec<ExactPage> = source.iter().map(exact_page).collect();
        let result = layout_exact(&pages, &LayoutOptions::default());
        let bytes = letters_layout::pdf::write_pdf_with(&result, &|_| None).expect("pdf");
        let ours_path = out.join(format!("{name}.pdf"));
        std::fs::write(&ours_path, bytes).expect("write");
        let ours = read_pdf(&ours_path).expect("read ours");
        let (count, worst) = max_error(&source, &ours);
        println!(
            "{name}: {count} glyphs, worst position error {worst:.5} pt, substitutions {}",
            result.substitutions.len()
        );
        assert!(worst <= 0.01, "{name}: a glyph moved {worst} pt");
    }
}
