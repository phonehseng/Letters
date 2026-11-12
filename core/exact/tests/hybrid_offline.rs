//! the easy to edit mode without word: promotions are predicted by the
//! measured pixel rules alone, the report says so, and the fonts are
//! completed for new text. runs in a few seconds, no word needed.

use std::path::{Path, PathBuf};

use letters_exact::hybrid::PREDICTED;
use letters_exact::{convert, ConvertOptions, OutputMode};

const DOCUMENTS: [&str; 13] = [
    "syllabus",
    "research_paper",
    "lab_report",
    "slides",
    "form",
    "journal",
    "cid_subset",
    "type3_font",
    "mixed_styles",
    "tj_kerning",
    "cff_font",
    "type1_font",
    "restricted_font",
];

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

#[test]
fn without_word_promotions_are_predicted_and_said_to_be() {
    let mut total_glyphs = 0;
    let mut in_paragraphs = 0;
    for name in DOCUMENTS {
        let out = std::env::temp_dir().join(format!(
            "letters-exact-hybrid-offline-{}-{name}.docx",
            std::process::id()
        ));
        let options = ConvertOptions {
            use_word: false,
            mode: OutputMode::EasyToEdit,
            ..ConvertOptions::default()
        };
        let report = convert(&corpus().join(format!("{name}.pdf")), &out, &options).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert!(bytes.starts_with(b"PK"), "{name}: not a zip");
        let hybrid = report.hybrid.as_ref().expect("easy to edit mode reports");
        assert!(!hybrid.checked_in_word);
        assert_eq!(hybrid.note, PREDICTED);
        assert_eq!(hybrid.exports, 0);
        assert!(report.verification.is_none());
        // every face covers what a person may type
        for completion in &report.font_completion {
            assert!(
                completion.unavailable.len() < 40,
                "{name}: {} can't be completed: {:?}",
                completion.pdf_font,
                completion.unavailable
            );
        }
        total_glyphs += hybrid.glyphs;
        in_paragraphs += hybrid.glyphs_in_multi_line_paragraphs;
        println!(
            "{name:<16} {:>5} glyphs, {} vertical cuts: {:>5.1}% in multi-line paragraphs ({}/{} promoted), {:>5.1}% single lines, {:>5.1}% positioned; fonts: {}",
            hybrid.glyphs,
            hybrid.vertical_cuts,
            100.0 * hybrid.multi_line_share(),
            hybrid.multi_line_paragraphs_promoted,
            hybrid.multi_line_paragraphs_detected,
            100.0 * hybrid.glyphs_in_single_line_paragraphs as f64 / hybrid.glyphs.max(1) as f64,
            100.0 * hybrid.glyphs_in_positioned_lines as f64 / hybrid.glyphs.max(1) as f64,
            report
                .font_completion
                .iter()
                .map(|c| format!(
                    "{} +{} composed +{} borrowed from {} ({:.0}% alike)",
                    c.pdf_font, c.composed, c.borrowed, c.donor, 100.0 * c.similarity.unwrap_or(f64::NAN)
                ))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    assert!(in_paragraphs > 0 && in_paragraphs < total_glyphs);
}
