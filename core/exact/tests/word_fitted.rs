//! the fitted fonts mode end to end through microsoft word: the corpus
//! measured by `verify` (every piece's glyph read as its components), next
//! to the locked layout's numbers, and a one-word edit. only runs when asked:
//!
//!     cargo test -p letters-exact --release --test word_fitted -- --ignored --nocapture --test-threads 1
//!
//! LETTERS_EXACT_ONLY=syllabus,journal limits the corpus.

use std::path::{Path, PathBuf};

use letters_exact::bridge::{Edit, EditAction, WordBridge};
use letters_exact::pdf::PdfFile;
use letters_exact::verify::{compare_glyphs, placed_glyphs};
use letters_exact::{convert, CancelFlag, ConvertOptions, OutputMode};

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

fn out_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target-exact")
        .join("word-tests")
        .join("fitted");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// replaces one word of the docx in word and counts the glyphs that moved
/// on lines other than the edited one (`(moved, compared)`).
fn one_word_edit(docx: &Path, dir: &Path, name: &str, needle: &str) -> Option<(usize, usize)> {
    let before = dir.join(format!("{name}.edit-before.word.pdf"));
    let after = dir.join(format!("{name}.edit-after.word.pdf"));
    let edit = Edit {
        action: EditAction::Insert,
        needle: needle.to_string(),
        text: "s".into(),
    };
    let bridge = WordBridge {
        seconds_per_document: 120,
        work_dir: dir.join("edit-word"),
    };
    let jobs = vec![
        (docx.to_path_buf(), before.clone(), None),
        (docx.to_path_buf(), after.clone(), Some(edit)),
    ];
    let run = bridge.export_edited(&jobs, &CancelFlag::new()).ok()?;
    if run.results.iter().any(|r| r.is_err()) {
        return None;
    }
    let a = placed_glyphs(&PdfFile::open(&before).ok()?.read_pages());
    let b = placed_glyphs(&PdfFile::open(&after).ok()?.read_pages());
    let texts_a: Vec<&str> = a.iter().map(|g| g.text.as_str()).collect();
    let texts_b: Vec<&str> = b.iter().map(|g| g.text.as_str()).collect();
    let pairs = letters_exact::verify::matching::matched_pairs(&texts_a, &texts_b);
    // the edited line: the first glyph that moved, and everything on its baseline
    let first_moved = pairs
        .iter()
        .find(|&&(i, j)| (a[i].x - b[j].x).abs() > 0.002 || (a[i].y - b[j].y).abs() > 0.002);
    let line_y = first_moved.map(|&(i, _)| (a[i].page, a[i].y));
    let other: Vec<&(usize, usize)> = pairs
        .iter()
        .filter(|&&(i, _)| {
            line_y.is_none_or(|(page, y)| a[i].page != page || (a[i].y - y).abs() > 0.5)
        })
        .collect();
    let moved = other
        .iter()
        .filter(|&&&(i, j)| (a[i].x - b[j].x).abs() > 0.0001 || (a[i].y - b[j].y).abs() > 0.0001)
        .count();
    Some((moved, other.len()))
}

#[test]
#[ignore = "starts microsoft word"]
fn the_corpus_with_fitted_fonts_through_word() {
    let dir = out_dir();
    let only: Option<String> = std::env::var("LETTERS_EXACT_ONLY").ok();
    let mut rows = Vec::new();
    for name in DOCUMENTS {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|n| n == name))
        {
            continue;
        }
        let pdf = corpus().join(format!("{name}.pdf"));
        let docx = dir.join(format!("{name}.docx"));
        let options = ConvertOptions {
            verify: true,
            mode: OutputMode::FittedLayout,
            cache_dir: Some(dir.join("cache")),
            work_dir: Some(dir.join(format!("{name}-work"))),
            ..ConvertOptions::default()
        };
        let report = convert(&pdf, &docx, &options).unwrap();
        std::fs::write(
            dir.join(format!("{name}.report.json")),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        let v = report.verification.as_ref().expect("word is installed");
        let fitted = report.fitted.as_ref().expect("fitted mode with word");
        // the plain reading of the same pdf, for the record
        let plain = compare_glyphs(
            &placed_glyphs(&PdfFile::open(&pdf).unwrap().read_pages()),
            &placed_glyphs(
                &PdfFile::open(&docx.with_extension("word.pdf"))
                    .unwrap()
                    .read_pages(),
            ),
        );
        let needle = report
            .line_plans
            .iter()
            .filter(|p| !p.rotated)
            .map(|p| p.text.split(' ').next().unwrap_or("").to_string())
            .find(|w| w.len() >= 4 && w.chars().all(|c| c.is_ascii_alphabetic()));
        let edit = needle.and_then(|n| one_word_edit(&docx, &dir, name, &n));
        let row = format!(
            "| {name} | {} | {:.4} | {:.4} | {:.4} | {:.1}% | {} | {}/{} ({} on round one's lines) | {} | {:.0} | {} |",
            v.glyphs.glyphs,
            v.glyphs.max_pt,
            v.glyphs.mean_pt,
            v.glyphs.p99_pt,
            100.0 * v.glyphs.within_0_025,
            if v.positions_pass { "pass" } else { "FAIL" },
            fitted.pieces_found_final,
            fitted.pieces,
            fitted.lines_left_on_round_one,
            fitted.exports,
            report.seconds,
            edit.map(|(moved, of)| format!("{moved} of {of} moved")).unwrap_or_else(|| "not run".into()),
        );
        println!(
            "{row}    (plain reading of word's pdf: {} glyphs paired)",
            plain.glyphs
        );
        rows.push(row);
    }
    println!("| document | glyphs | max | mean | p99 | <=0.025 | positions | pieces found | exports | seconds | one-word edit: glyphs on other lines that moved |");
    for row in rows {
        println!("{row}");
    }
}
