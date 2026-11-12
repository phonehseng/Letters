//! end to end through microsoft word. these start a private, invisible
//! copy of word, so they only run when asked:
//!
//!     cargo test -p letters-exact --test word -- --ignored --nocapture --test-threads 1
//!
//! every test checks afterwards that the word processes running before it
//! are still running and that no new one was left behind.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use letters_exact::bridge::{find_word, WordBridge};
use letters_exact::{convert, CancelFlag, ConvertOptions, ExactError, OutputMode};

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

/// the processes that were there before are untouched, and none is new.
fn assert_word_left_as_it_was(before: &BTreeSet<u32>) {
    std::thread::sleep(Duration::from_secs(1));
    let after = word_processes();
    assert!(
        before.is_subset(&after),
        "a word process that was running before is gone: {:?}",
        before.difference(&after).collect::<Vec<_>>()
    );
    let new: Vec<_> = after.difference(before).collect();
    assert!(new.is_empty(), "word processes left running: {new:?}");
}

#[test]
#[ignore = "starts microsoft word"]
fn the_bridge_exports_with_its_own_copy_and_ends_it() {
    assert!(find_word().is_some(), "word is not installed");
    let before = word_processes();
    let dir = out_dir("bridge");
    let docx = dir.join("form.docx");
    convert(
        &corpus().join("form.pdf"),
        &docx,
        &ConvertOptions {
            use_word: false,
            ..ConvertOptions::default()
        },
    )
    .unwrap();
    let bridge = WordBridge {
        seconds_per_document: 60,
        work_dir: dir.join("word"),
    };
    let pdf = dir.join("form.word.pdf");
    let run = bridge.export_one(&docx, &pdf, &CancelFlag::new()).unwrap();
    println!(
        "word build {:?}, process {:?}, {:.1} s",
        run.build, run.word_process, run.seconds
    );
    assert!(pdf.is_file());
    let ours = run.word_process.expect("the script names its word process");
    assert!(!before.contains(&ours), "the copy used was already running");
    assert!(run.word_ended);
    assert_word_left_as_it_was(&before);
}

#[test]
#[ignore = "starts microsoft word"]
fn a_call_that_takes_too_long_is_stopped_and_word_ended() {
    let before = word_processes();
    let dir = out_dir("timeout");
    let docx = dir.join("form.docx");
    convert(
        &corpus().join("form.pdf"),
        &docx,
        &ConvertOptions {
            use_word: false,
            ..ConvertOptions::default()
        },
    )
    .unwrap();
    let bridge = WordBridge {
        seconds_per_document: 60,
        work_dir: dir.join("word"),
    };
    let result = bridge.export_within(
        &[(docx, dir.join("form.word.pdf"))],
        &CancelFlag::new(),
        Duration::from_secs(1),
    );
    assert!(
        matches!(result, Err(ExactError::WordTimedOut(1))),
        "{result:?}"
    );
    assert_word_left_as_it_was(&before);
}

#[test]
#[ignore = "starts microsoft word"]
fn a_cancelled_conversion_ends_word() {
    let before = word_processes();
    let dir = out_dir("cancel");
    let cancel = CancelFlag::new();
    let stopper = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(4));
        stopper.cancel();
    });
    let options = ConvertOptions {
        cancel,
        verify: true,
        cache_dir: Some(dir.join("cache")),
        ..ConvertOptions::default()
    };
    let result = convert(
        &corpus().join("journal.pdf"),
        &dir.join("journal.docx"),
        &options,
    );
    assert!(
        matches!(result, Err(ExactError::Cancelled)),
        "{:?}",
        result.map(|r| r.seconds)
    );
    assert_word_left_as_it_was(&before);
}

/// measures page sizes again with the probe exports, with the sections
/// marked portrait (as the prototype's probes were) and landscape (as the
/// converter writes wide pages), and compares them with the built-in tables.
/// set LETTERS_EXACT_SIZES to "w,h;w,h" for other sizes. the measured
/// tables are written to target-exact/word-tests/probe/measured.json.
#[test]
#[ignore = "starts microsoft word (a few minutes per page size)"]
fn measured_step_tables_agree_with_the_built_in_ones() {
    use letters_exact::calibration::probe::measure_page_steps;
    use letters_exact::calibration::{Axis, WordModel};
    use letters_exact::word::writer_model::page_pixels;
    let before = word_processes();
    let dir = out_dir("probe");
    let sizes: Vec<(f64, f64)> = std::env::var("LETTERS_EXACT_SIZES")
        .unwrap_or_else(|_| "720,405".into())
        .split(';')
        .map(|pair| {
            let (w, h) = pair.split_once(',').unwrap();
            (w.trim().parse().unwrap(), h.trim().parse().unwrap())
        })
        .collect();
    let word = find_word().expect("word is installed");
    let bridge = WordBridge {
        seconds_per_document: 300,
        work_dir: dir.join("word"),
    };
    let mut built_in = WordModel::built_in();
    let mut measured = serde_json::Map::new();
    let mut portrait_differ = Vec::new();
    for (width, height) in sizes {
        let (wx, hy) = (page_pixels(width), page_pixels(height));
        for (mark, name) in [(false, "portrait"), (true, "landscape")] {
            let work = dir.join(format!("{wx}x{hy}-{name}"));
            let (x, y, seconds) = measure_page_steps(
                &bridge,
                width,
                height,
                Some(mark),
                &work,
                &CancelFlag::new(),
            )
            .unwrap();
            let shipped = (
                built_in.table(Axis::X, wx, false).steps,
                built_in.table(Axis::Y, hy, false).steps,
            );
            println!(
                "word {}, {width} x {height} pt marked {name} ({seconds:.0} s): x {x:?}, y {y:?}; built in x {:?}, y {:?}",
                word.build, shipped.0, shipped.1
            );
            measured.insert(format!("x:{wx}:{name}"), serde_json::json!(x));
            measured.insert(format!("y:{hy}:{name}"), serde_json::json!(y));
            if !mark && (x != shipped.0 || y != shipped.1) {
                portrait_differ.push(format!("{width} x {height}"));
            }
        }
    }
    std::fs::write(
        dir.join("measured.json"),
        serde_json::to_string_pretty(&measured).unwrap(),
    )
    .unwrap();
    assert_word_left_as_it_was(&before);
    assert!(
        portrait_differ.is_empty(),
        "portrait-marked tables differ from the built-in ones: {portrait_differ:?}"
    );
}

/// the prototype's results through word (corpus_report.md, round 4):
/// name, class, glyphs, max, mean, positions pass, fonts pass.
const PROTOTYPE: [(&str, &str, usize, f64, f64, bool, bool); 13] = [
    ("syllabus", "syllabus", 2567, 0.074, 0.0278, true, true),
    (
        "research_paper",
        "research paper",
        5747,
        0.063,
        0.0312,
        true,
        true,
    ),
    ("lab_report", "lab report", 1628, 0.063, 0.0285, true, true),
    ("slides", "slides", 389, 0.102, 0.0494, true, true),
    ("form", "form", 517, 0.085, 0.0447, true, false),
    ("journal", "journal page", 5109, 0.086, 0.0496, true, true),
    (
        "cid_subset",
        "cid subset > 256 glyphs",
        972,
        0.078,
        0.0427,
        true,
        true,
    ),
    ("type3_font", "Type 3 font", 972, 0.078, 0.0416, true, true),
    (
        "mixed_styles",
        "mixed styles",
        387,
        0.077,
        0.0403,
        true,
        true,
    ),
    (
        "tj_kerning",
        "justified TJ kerning",
        1230,
        0.084,
        0.0479,
        true,
        true,
    ),
    ("cff_font", "CFF font", 1338, 0.085, 0.0485, true, true),
    ("type1_font", "Type 1 font", 1255, 0.085, 0.0490, true, true),
    (
        "restricted_font",
        "restricted fsType",
        785,
        0.085,
        0.0471,
        true,
        false,
    ),
];

#[test]
#[ignore = "starts microsoft word (about ten minutes for the corpus)"]
fn the_corpus_matches_the_prototype_through_word() {
    let before = word_processes();
    let dir = out_dir("corpus");
    let only: Option<String> = std::env::var("LETTERS_EXACT_ONLY").ok();
    let mut rows = Vec::new();
    let mut differences = Vec::new();
    for (name, class, glyphs, max, mean, positions, fonts) in PROTOTYPE {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|n| n == name))
        {
            continue;
        }
        let options = ConvertOptions {
            verify: true,
            mode: OutputMode::LockedLayout,
            cache_dir: Some(dir.join("cache")),
            work_dir: Some(dir.join(name)),
            ..ConvertOptions::default()
        };
        let report = convert(
            &corpus().join(format!("{name}.pdf")),
            &dir.join(format!("{name}.docx")),
            &options,
        )
        .unwrap();
        let v = report
            .verification
            .clone()
            .expect("word is installed, so the result is verified");
        let g = &v.glyphs;
        let check = report
            .check
            .as_ref()
            .map(|c| c.glyphs_off_plan_final)
            .unwrap_or(0);
        let row = format!(
            "| {class} | {} | {:.3} | {:.4} | {:.3} | {:.1}% | {:.1}% | {:.1}% | {} | {} | {} | {:.3} / {:.4} | {:.3} / {:.4} | {check} | {:.1} |",
            g.glyphs,
            g.max_pt,
            g.mean_pt,
            g.p99_pt,
            100.0 * g.within_0_025,
            100.0 * g.within_0_06,
            100.0 * g.within_0_125,
            if v.positions_pass { "pass" } else { "FAIL" },
            if v.fonts_pass { "pass" } else { "FAIL" },
            if v.pass { "pass" } else { "FAIL" },
            max,
            mean,
            v.floor.max_pt,
            v.floor.mean_pt,
            report.seconds,
        );
        println!("{row}");
        rows.push(row);
        std::fs::write(
            dir.join(format!("{name}.report.json")),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        // "within noise": the same verdicts, the same glyph count, max and mean close to the prototype's
        if v.positions_pass != positions || v.fonts_pass != fonts {
            differences.push(format!(
                "{name}: verdict positions {} fonts {}",
                v.positions_pass, v.fonts_pass
            ));
        }
        if g.glyphs != glyphs {
            differences.push(format!(
                "{name}: {} glyphs measured, the prototype measured {glyphs}",
                g.glyphs
            ));
        }
        if (g.max_pt - max).abs() > 0.012 || (g.mean_pt - mean).abs() > 0.002 {
            differences.push(format!(
                "{name}: max {:.3} mean {:.4} against {max:.3} / {mean:.4}",
                g.max_pt, g.mean_pt
            ));
        }
    }
    println!("| class | glyphs | max | mean | p99 | <=0.025 | <=0.06 | <=0.125 | positions | fonts | overall | prototype max / mean | floor max / mean | glyphs off plan | convert s |");
    for row in &rows {
        println!("{row}");
    }
    assert_word_left_as_it_was(&before);
    assert!(differences.is_empty(), "{differences:#?}");
}
