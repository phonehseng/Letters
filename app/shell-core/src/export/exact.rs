//! the adapter to the "exact match" word converter (core/exact).
//!
//! the export dialog only talks to this module, so the engine can change
//! its insides without the dialog noticing. the engine starts and ends its
//! own private copy of word when word is installed; nothing here starts
//! word itself.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use letters_exact::{convert, CancelFlag, ConversionReport, ConvertOptions, RenderFn};

use super::request::{Closeness, ExportOutcome, ExportProgress};

/// what the dialog says about the exact match entry on this computer.
pub struct ExactStatus {
    pub available: bool,
    pub how: &'static str,
    pub note: &'static str,
}

/// it always works; with word installed it is also measured.
pub fn status() -> ExactStatus {
    if crate::writer::export::word_installed() {
        ExactStatus {
            available: true,
            how: "Letters, checked with Microsoft Word",
            note: "Every letter is placed where Word will draw it, then Word itself checks the result, which takes from about ten seconds to a minute and a half.",
        }
    } else {
        ExactStatus {
            available: true,
            how: "Letters",
            note: "Every letter is placed where Word will draw it, but without Word on this computer the result can’t be checked here.",
        }
    }
}

/// where the engine keeps what it measured about this computer's word,
/// and its working files.
pub struct ExactPlaces {
    pub cache_dir: PathBuf,
    pub work_dir: PathBuf,
}

/// converts `pdf` to `docx`. `use_word` false never starts word (the
/// tests, and computers without it).
pub fn convert_exact(
    pdf: &Path,
    docx: &Path,
    places: &ExactPlaces,
    use_word: bool,
    render: Option<Arc<RenderFn>>,
    progress: Arc<dyn Fn(ExportProgress) + Send + Sync>,
    cancel: CancelFlag,
) -> Result<ExportOutcome, String> {
    let options = ConvertOptions {
        cache_dir: Some(places.cache_dir.clone()),
        verify: use_word,
        use_word,
        work_dir: Some(places.work_dir.clone()),
        render,
        progress: Some(Arc::new(move |step: letters_exact::Progress| {
            progress(ExportProgress {
                fraction: step.fraction.clamp(0.0, 1.0),
                page: None,
            })
        })),
        cancel,
        ..ConvertOptions::default()
    };
    let _ = std::fs::create_dir_all(&places.work_dir);
    let report = convert(pdf, docx, &options).map_err(|error| error.to_string())?;
    Ok(outcome(&report, docx))
}

/// the dialog's words for a finished conversion.
pub fn outcome(report: &ConversionReport, docx: &Path) -> ExportOutcome {
    let pages = report.pages.len();
    let pictures: usize = report.pages.iter().map(|page| page.pictures).sum();
    let mut summary = format!(
        "{} with {} lines of real text",
        count(pages, "page"),
        report.lines
    );
    if pictures > 0 {
        summary.push_str(&format!(" and {}", count(pictures, "picture")));
    }
    summary.push('.');
    let verification = report.verification.as_ref();
    let checked = match verification {
        Some(check) => check.summary.clone(),
        None => non_empty(&report.word.note)
            .unwrap_or("Not checked against Word on this computer.")
            .to_string(),
    };
    ExportOutcome {
        files: vec![docx.to_path_buf()],
        summary,
        notes: report.warnings.clone(),
        verification: Some(checked),
        closeness: verification.map(|check| Closeness {
            glyphs: check.glyphs.glyphs,
            max_pt: check.glyphs.max_pt,
            mean_pt: check.glyphs.mean_pt,
            p99_pt: check.glyphs.p99_pt,
            within_0_025: check.glyphs.within_0_025,
            within_0_06: check.glyphs.within_0_06,
            within_0_125: check.glyphs.within_0_125,
            pass: check.pass,
        }),
        stand_ins: stand_ins(report),
    }
}

/// "“Minion Pro”: it doesn’t allow embedding" for every face that got a
/// similar font instead of its own.
fn stand_ins(report: &ConversionReport) -> Vec<String> {
    report
        .fonts
        .iter()
        .filter_map(|font| {
            let substitute = font.substitute.as_ref()?;
            let why = font
                .not_used_because
                .as_deref()
                .unwrap_or("the PDF doesn’t include it");
            Some(format!("“{}” is shown in {substitute}: {why}", font.family))
        })
        .collect()
}

fn non_empty(text: &str) -> Option<&str> {
    (!text.trim().is_empty()).then_some(text)
}

fn count(number: usize, noun: &str) -> String {
    if number == 1 {
        format!("1 {noun}")
    } else {
        format!("{number} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_pdf_converts_without_word() {
        let dir = std::env::temp_dir().join(format!("letters-exact-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("letter.pdf");
        std::fs::write(
            &pdf,
            crate::export::test_pdfs::one_line_pdf("Hello exact match"),
        )
        .unwrap();
        let docx = dir.join("letter.docx");
        let places = ExactPlaces {
            cache_dir: dir.join("cache"),
            work_dir: dir.join("work"),
        };
        let outcome = convert_exact(
            &pdf,
            &docx,
            &places,
            false,
            None,
            Arc::new(|_| {}),
            CancelFlag::new(),
        )
        .unwrap();
        assert!(docx.exists());
        assert!(
            outcome.summary.starts_with("1 page with"),
            "{}",
            outcome.summary
        );
        // without word nobody measured it, and the dialog must say so
        assert!(outcome.closeness.is_none());
        assert!(outcome.verification.unwrap().contains("Not checked"));
        // the office engine can read what came out
        let bytes = std::fs::read(&docx).unwrap();
        assert!(letters_office::read_docx(&bytes).is_ok());
    }

    #[test]
    fn a_cancelled_conversion_stops() {
        let dir = std::env::temp_dir().join(format!("letters-exact-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("letter.pdf");
        std::fs::write(&pdf, crate::export::test_pdfs::one_line_pdf("Stop")).unwrap();
        let cancel = CancelFlag::new();
        cancel.cancel();
        let places = ExactPlaces {
            cache_dir: dir.join("cache"),
            work_dir: dir.join("work"),
        };
        let result = convert_exact(
            &pdf,
            &dir.join("out.docx"),
            &places,
            false,
            None,
            Arc::new(|_| {}),
            cancel,
        );
        assert!(result.is_err());
    }
}
