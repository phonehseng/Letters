//! the "exact, easy to edit" mode end to end through microsoft word: the
//! corpus, the edit tests and the back and forth loop. these start a
//! private, invisible copy of word, so they only run when asked:
//!
//!     cargo test -p letters-exact --release --test word_hybrid -- --ignored --nocapture --test-threads 1
//!
//! LETTERS_EXACT_ONLY=syllabus,journal limits the corpus. every test checks
//! afterwards that the word processes running before it still run and
//! that none of its own is left.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use letters_exact::bridge::WordBridge;
use letters_exact::hybrid::edits::{judge_edit, plan_edits, EditKind, EditOutcome};
use letters_exact::verify::{compare_glyphs, placed_glyphs};
use letters_exact::{convert, CancelFlag, ConversionReport, ConvertOptions, OutputMode};

pub const DOCUMENTS: [(&str, &str); 13] = [
    ("syllabus", "syllabus"),
    ("research_paper", "research paper"),
    ("lab_report", "lab report"),
    ("slides", "slides"),
    ("form", "form"),
    ("journal", "journal page"),
    ("cid_subset", "cid subset > 256 glyphs"),
    ("type3_font", "Type 3 font"),
    ("mixed_styles", "mixed styles"),
    ("tj_kerning", "justified TJ kerning"),
    ("cff_font", "CFF font"),
    ("type1_font", "Type 1 font"),
    ("restricted_font", "restricted fsType"),
];

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

fn out_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target-exact")
        .join("word-tests")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn word_processes() -> BTreeSet<u32> {
    let output = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq WINWORD.EXE", "/FO", "CSV", "/NH"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            line.split("\",\"")
                .nth(1)
                .and_then(|pid| pid.trim_matches('"').parse().ok())
        })
        .collect()
}

/// the processes that were there before still run, and none is new. other
/// programs on this machine start their own private words now and then,
/// so a new process gets 30 s to go away before it counts.
fn assert_word_left_as_it_was(before: &BTreeSet<u32>) {
    let mut new: Vec<u32> = Vec::new();
    for _ in 0..30 {
        std::thread::sleep(Duration::from_secs(1));
        let after = word_processes();
        assert!(
            before.is_subset(&after),
            "a word process that was running before is gone: {:?}",
            before.difference(&after).collect::<Vec<_>>()
        );
        new = after.difference(before).copied().collect();
        if new.is_empty() {
            return;
        }
    }
    panic!("word processes left running: {new:?}");
}

fn selected() -> Vec<(&'static str, &'static str)> {
    let only: Option<String> = std::env::var("LETTERS_EXACT_ONLY").ok();
    DOCUMENTS
        .into_iter()
        .filter(|(name, _)| {
            only.as_deref()
                .is_none_or(|o| o.split(',').any(|n| n == *name))
        })
        .collect()
}

fn convert_mode(
    pdf: &Path,
    docx: &Path,
    mode: OutputMode,
    dir: &Path,
    name: &str,
) -> ConversionReport {
    let cache = dir.join("cache");
    // a cold run: the hybrid decisions of an earlier run are not reused
    for entry in std::fs::read_dir(&cache).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with("hybrid-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let options = ConvertOptions {
        verify: true,
        mode,
        cache_dir: Some(cache),
        work_dir: Some(dir.join(format!("{name}-work"))),
        ..ConvertOptions::default()
    };
    convert(pdf, docx, &options).unwrap()
}

/// the fidelity and editability row of one document.
fn row(class: &str, report: &ConversionReport) -> String {
    let v = report.verification.as_ref().expect("word is installed");
    let g = &v.glyphs;
    let h = report.hybrid.as_ref();
    let share = |n: usize| match h {
        Some(h) => format!("{:.1}%", 100.0 * n as f64 / h.glyphs.max(1) as f64),
        None => "-".into(),
    };
    format!(
        "| {class} | {} | {:.3} | {:.4} | {:.3} | {:.1}% | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.0} |",
        g.glyphs,
        g.max_pt,
        g.mean_pt,
        g.p99_pt,
        100.0 * g.within_0_125,
        if v.positions_pass { "pass" } else { "FAIL" },
        h.map(|h| share(h.glyphs_in_multi_line_paragraphs)).unwrap_or_default(),
        h.map(|h| share(h.glyphs_in_single_line_paragraphs)).unwrap_or_default(),
        h.map(|h| share(h.glyphs_in_positioned_lines)).unwrap_or_default(),
        h.map(|h| format!("{}/{}", h.multi_line_paragraphs_promoted, h.multi_line_paragraphs_detected))
            .unwrap_or_default(),
        h.map(|h| format!("{:?}", h.promoted_by_level)).unwrap_or_default(),
        h.map(|h| format!("{} / {} / {}", h.pins, h.splits, h.vertical_cuts)).unwrap_or_default(),
        h.map(|h| format!("{} ({})", h.shared_frames, h.paragraphs_in_shared_frames))
            .unwrap_or_default(),
        h.map(|h| h.exports.to_string()).unwrap_or_default(),
        report.seconds,
    )
}

/// every line's text, for checking that an edit's needle is unique.
fn document_text(report: &ConversionReport) -> String {
    let lines: Vec<&str> = report.line_plans.iter().map(|p| p.text.as_str()).collect();
    lines.join(" ")
}

fn pages_of(pdf: &Path) -> Vec<letters_exact::pdf::PageContent> {
    letters_exact::pdf::PdfFile::open(pdf).unwrap().read_pages()
}

/// the edit tests on one converted document: one word session exports the
/// document as it is and once per edit.
fn edit_tests(
    name: &str,
    source_pdf: &Path,
    docx: &Path,
    report: &ConversionReport,
    dir: &Path,
) -> Vec<EditOutcome> {
    let hybrid = report.hybrid.as_ref().unwrap();
    let planned = plan_edits(hybrid, &document_text(report));
    if planned.is_empty() {
        return Vec::new();
    }
    let base = dir.join(format!("{name}.edit-none.word.pdf"));
    let mut jobs = vec![(docx.to_path_buf(), base.clone(), None)];
    for (index, edit) in planned.iter().enumerate() {
        let pdf = dir.join(format!("{name}.edit-{index}.word.pdf"));
        jobs.push((docx.to_path_buf(), pdf, Some(edit.edit.clone())));
    }
    let bridge = WordBridge {
        seconds_per_document: 120,
        work_dir: dir.join("edit-word"),
    };
    let run = bridge.export_edited(&jobs, &CancelFlag::new()).unwrap();
    let source = pages_of(source_pdf);
    let before = pages_of(&base);
    planned
        .iter()
        .zip(jobs.iter().skip(1))
        .zip(run.results.iter().skip(1))
        .map(|((edit, (_, pdf, _)), result)| match result {
            Ok(()) => judge_edit(edit, hybrid, &source, &before, &pages_of(pdf)),
            Err(why) => {
                println!("    {name}: {:?} failed in word: {why}", edit.kind);
                EditOutcome {
                    kind: Some(edit.kind),
                    needle: edit.edit.needle.clone(),
                    ..EditOutcome::default()
                }
            }
        })
        .collect()
}

fn edit_row(class: &str, outcomes: &[EditOutcome]) -> String {
    let cell = |kind: EditKind| {
        outcomes
            .iter()
            .find(|o| o.kind == Some(kind))
            .map(|o| {
                format!(
                    "{} (grew {}, below {}/{}, outside moved {}/{}, new {} in {})",
                    if o.pass { "pass" } else { "FAIL" },
                    o.growth_lines,
                    o.below_moved_right,
                    o.below_glyphs,
                    o.outside_moved,
                    o.outside_glyphs,
                    o.new_glyphs,
                    o.new_text_fonts.join("+")
                )
            })
            .unwrap_or_else(|| "not run".into())
    };
    format!(
        "| {class} | {} | {} | {} | {} |",
        cell(EditKind::InsertSentence),
        cell(EditKind::DeleteSentence),
        cell(EditKind::TypeAtEnd),
        cell(EditKind::TypeAndUndo)
    )
}

/// the second generation: word's pdf of the converted docx converted again.
fn second_generation(
    name: &str,
    class: &str,
    report: &ConversionReport,
    original: &Path,
    dir: &Path,
) -> String {
    let first_pdf = dir.join(format!("{name}.word.pdf"));
    let docx = dir.join(format!("{name}.gen2.docx"));
    let second = convert_mode(
        &first_pdf,
        &docx,
        OutputMode::EasyToEdit,
        dir,
        &format!("{name}-gen2"),
    );
    let second_pdf = docx.with_extension("word.pdf");
    let original_glyphs = placed_glyphs(&pages_of(original));
    let drift = compare_glyphs(&original_glyphs, &placed_glyphs(&pages_of(&second_pdf)));
    let first = compare_glyphs(&original_glyphs, &placed_glyphs(&pages_of(&first_pdf)));
    let (h1, h2) = (
        report.hybrid.as_ref().unwrap(),
        second.hybrid.as_ref().unwrap(),
    );
    let v2 = second.verification.as_ref().unwrap();
    format!(
        "| {class} | {:.3} / {:.4} | {:.3} / {:.4} | {:.3} / {:.4} | {} / {} | {} / {} | {:.1}% / {:.1}% | {} |",
        first.max_pt,
        first.mean_pt,
        v2.glyphs.max_pt,
        v2.glyphs.mean_pt,
        drift.max_pt,
        drift.mean_pt,
        first.matched,
        drift.matched,
        h1.multi_line_paragraphs_promoted,
        h2.multi_line_paragraphs_promoted,
        100.0 * h1.multi_line_share(),
        100.0 * h2.multi_line_share(),
        if v2.substituted_by_word.is_empty() { "embedded" } else { "Word substituted" },
    )
}

#[test]
#[ignore = "starts microsoft word (about twenty minutes for the corpus)"]
fn the_corpus_in_easy_to_edit_mode_through_word() {
    let before = word_processes();
    let dir = out_dir("hybrid-corpus");
    let mut rows = Vec::new();
    let mut edit_rows = Vec::new();
    let mut generation_rows = Vec::new();
    let mut failures = Vec::new();
    let with_edits = std::env::var("LETTERS_EXACT_NO_EDITS").is_err();
    let with_generations = std::env::var("LETTERS_EXACT_NO_GENERATIONS").is_err();
    for (name, class) in selected() {
        let pdf = corpus().join(format!("{name}.pdf"));
        let docx = dir.join(format!("{name}.docx"));
        let report = convert_mode(&pdf, &docx, OutputMode::EasyToEdit, &dir, name);
        let line = row(class, &report);
        println!("{line}");
        rows.push(line);
        if with_edits {
            let outcomes = edit_tests(name, &pdf, &docx, &report, &dir);
            let line = edit_row(class, &outcomes);
            println!("{line}");
            edit_rows.push(line);
            std::fs::write(
                dir.join(format!("{name}.edits.json")),
                serde_json::to_string_pretty(&outcomes).unwrap(),
            )
            .unwrap();
        }
        if with_generations {
            let line = second_generation(name, class, &report, &pdf, &dir);
            println!("{line}");
            generation_rows.push(line);
        }
        std::fs::write(
            dir.join(format!("{name}.report.json")),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        let hybrid = report.hybrid.as_ref().unwrap();
        assert!(
            hybrid.checked_in_word,
            "{name}: the hybrid result was not checked"
        );
        if !report.verification.as_ref().unwrap().positions_pass {
            failures.push(name);
        }
    }
    println!("| class | glyphs | max | mean | p99 | <=0.125 | positions | text in multi-line paragraphs | single-line paragraphs | positioned lines | multi-line promoted | by level | pins / splits / vertical cuts | shared frames (paragraphs) | exports | seconds |");
    for line in &rows {
        println!("{line}");
    }
    println!("| class | insert a sentence | delete a sentence | type at the end | type one char and undo |");
    for line in &edit_rows {
        println!("{line}");
    }
    println!("| class | gen 1 vs pdf max / mean | gen 2 vs gen 1 max / mean | gen 2 vs pdf max / mean | glyphs matched gen 1 / gen 2 | multi-line paragraphs promoted gen 1 / gen 2 | text in them gen 1 / gen 2 | fonts gen 2 |");
    for line in &generation_rows {
        println!("{line}");
    }
    assert_word_left_as_it_was(&before);
    println!("positions failing in hybrid mode: {failures:?}");
}

/// converting the same pdf again reuses the proof decisions: no proof
/// rounds, only the final checking export (a pass is never reported
/// without one).
#[test]
#[ignore = "starts microsoft word"]
fn a_second_conversion_reuses_the_decisions() {
    let before = word_processes();
    let dir = out_dir("hybrid-cache");
    let pdf = corpus().join("tj_kerning.pdf");
    let first = convert_mode(
        &pdf,
        &dir.join("first.docx"),
        OutputMode::EasyToEdit,
        &dir,
        "first",
    );
    let options = ConvertOptions {
        verify: true,
        mode: OutputMode::EasyToEdit,
        cache_dir: Some(dir.join("cache")),
        work_dir: Some(dir.join("second-work")),
        ..ConvertOptions::default()
    };
    let second = convert(&pdf, &dir.join("second.docx"), &options).unwrap();
    let (a, b) = (first.hybrid.unwrap(), second.hybrid.unwrap());
    println!(
        "first: {} exports, {:.0} s; second: cache {}, {} exports, {:.0} s",
        a.exports, first.seconds, b.cache, b.exports, second.seconds
    );
    assert_eq!(b.cache, "hit");
    assert_eq!(
        b.multi_line_paragraphs_promoted,
        a.multi_line_paragraphs_promoted
    );
    assert!(b.exports < a.exports);
    assert!(second.verification.unwrap().positions_pass);
    assert_word_left_as_it_was(&before);
}
