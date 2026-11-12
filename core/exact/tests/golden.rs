//! golden comparison with the prototype: for every corpus document, the
//! rust converter's plan (frame positions, per-glyph targets, advances and
//! spacing) must match the plan the python prototype made, which was
//! measured through word (corpus_report.md). word is not used here: the
//! built-in tables are the ones the prototype measured.
//!
//! the golden files in tests/corpus/golden were extracted from the
//! prototype's output (`<name>.convert.json` for the line plans, the docx
//! for the frames). rotated lines are left out: their plan depends on
//! measuring exports through word.

use std::path::{Path, PathBuf};

use letters_exact::{convert, ConvertOptions};
use serde_json::Value;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

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

struct Differences {
    lines: usize,
    lines_differing: Vec<String>,
    frames_differing: Vec<String>,
}

fn as_i64s(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

/// the slides are wider than tall, so the docx marks them landscape, and
/// word prints a landscape page on another lattice than the portrait-marked
/// probe pages the prototype measured (README, "departures"). to compare
/// the port itself, this cache gives landscape pages the prototype's tables.
fn prototype_tables_for(name: &str) -> Option<PathBuf> {
    if name != "slides" {
        return None;
    }
    let dir =
        std::env::temp_dir().join(format!("letters-exact-golden-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let facts = r#"{"steps": {"x:6000:landscape": [633, 1982, 3331, 4680], "y:3375:landscape": [331, 1036, 1741, 2446, 3151]}}"#;
    std::fs::write(dir.join("word-16.0.20527.20150-model.json"), facts).unwrap();
    Some(dir)
}

fn compare(name: &str) -> Differences {
    let golden: Value = serde_json::from_str(
        &std::fs::read_to_string(corpus().join("golden").join(format!("{name}.json"))).unwrap(),
    )
    .unwrap();
    let out = std::env::temp_dir().join(format!(
        "letters-exact-golden-{}-{name}.docx",
        std::process::id()
    ));
    let options = ConvertOptions {
        use_word: false,
        cache_dir: prototype_tables_for(name),
        ..ConvertOptions::default()
    };
    let report = convert(&corpus().join(format!("{name}.pdf")), &out, &options).unwrap();
    let _ = std::fs::remove_file(&out);
    let ours: Vec<_> = report.line_plans.iter().filter(|p| !p.rotated).collect();
    let theirs: Vec<&Value> = golden["plans"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| !p["rotated"].as_bool().unwrap())
        .collect();
    let mut lines_differing = Vec::new();
    if ours.len() != theirs.len() {
        lines_differing.push(format!(
            "{} lines planned, the prototype planned {}",
            ours.len(),
            theirs.len()
        ));
    }
    for (index, (mine, gold)) in ours.iter().zip(&theirs).enumerate() {
        let mut what = Vec::new();
        if mine.text != gold["text"].as_str().unwrap() {
            what.push(format!(
                "text {:?} vs {:?}",
                mine.text,
                gold["text"].as_str().unwrap()
            ));
        }
        if mine.targets_px != as_i64s(&gold["targets_px"]) {
            what.push("targets".to_string());
        }
        if mine.advances_px != as_i64s(&gold["advances_px"]) {
            what.push(format!(
                "advances {:?} vs {:?}",
                mine.advances_px,
                as_i64s(&gold["advances_px"])
            ));
        }
        if mine.spacing_twips != as_i64s(&gold["spacing_twips"]) {
            what.push("spacing".to_string());
        }
        if mine.start_pixel != gold["start_pixel"].as_i64() {
            what.push(format!(
                "start {:?} vs {:?}",
                mine.start_pixel, gold["start_pixel"]
            ));
        }
        let word_y = gold["word_y"].as_f64().unwrap_or(f64::NAN);
        if mine.word_y.is_none_or(|y| (y - word_y).abs() > 1e-9) {
            what.push(format!("baseline {:?} vs {word_y}", mine.word_y));
        }
        if !what.is_empty() {
            lines_differing.push(format!(
                "line {index} ({:?}): {}",
                gold["text"].as_str().unwrap_or(""),
                what.join(", ")
            ));
        }
    }
    let mut frames_differing = Vec::new();
    let golden_frames = golden["frames"].as_array().unwrap();
    if golden_frames.len() != report.frames.len() {
        frames_differing.push(format!(
            "{} frames, the prototype wrote {}",
            report.frames.len(),
            golden_frames.len()
        ));
    }
    for (index, (mine, gold)) in report.frames.iter().zip(golden_frames).enumerate() {
        let theirs = (
            gold["x"].as_i64().unwrap(),
            gold["y"].as_i64().unwrap(),
            gold["w"].as_i64().unwrap(),
            gold["line"].as_i64().unwrap(),
        );
        let ours = (
            mine.x_twips,
            mine.y_twips,
            mine.width_twips,
            mine.line_twips,
        );
        if ours != theirs {
            frames_differing.push(format!("frame {index}: {ours:?} vs {theirs:?}"));
        }
    }
    let count = |f: fn(&letters_exact::PageReport) -> usize| {
        report.pages.iter().map(f).sum::<usize>() as i64
    };
    let substitutes: Vec<&str> = report
        .fonts
        .iter()
        .filter(|f| f.substitute.is_some())
        .map(|f| f.pdf_font.as_str())
        .collect();
    let golden_substitutes: Vec<&str> = golden["fonts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| !f["substitute"].is_null())
        .map(|f| f["pdf_font"].as_str().unwrap())
        .collect();
    let totals = [
        (
            "glyphs",
            report.glyphs as i64,
            golden["glyphs"].as_i64().unwrap(),
        ),
        (
            "lines",
            report.lines as i64,
            golden["lines"].as_i64().unwrap(),
        ),
        (
            "rotated lines",
            count(|p| p.rotated_lines),
            golden["rotated_lines"].as_i64().unwrap(),
        ),
        (
            "hidden lines",
            count(|p| p.hidden_lines),
            golden["hidden_lines"].as_i64().unwrap(),
        ),
        (
            "pictures",
            count(|p| p.pictures),
            golden["images"].as_i64().unwrap(),
        ),
        (
            "jpegs kept",
            count(|p| p.jpeg_kept),
            golden["jpeg_kept"].as_i64().unwrap(),
        ),
        (
            "shapes",
            count(|p| p.shapes),
            golden["shapes"].as_i64().unwrap(),
        ),
        (
            "vector items for pictures",
            count(|p| p.vectors_left_out),
            golden["rasterised_vector_items"].as_i64().unwrap(),
        ),
        (
            "glyphs without char",
            report.glyph_counts.glyphs_without_char as i64,
            golden["glyphs_without_char"].as_i64().unwrap(),
        ),
        (
            "grid fonts",
            report.grid_fonts as i64,
            golden["grid_fonts"].as_i64().unwrap(),
        ),
        (
            "faces",
            report.fonts.len() as i64,
            golden["fonts"].as_array().unwrap().len() as i64,
        ),
    ];
    for (what, ours, theirs) in totals {
        if ours != theirs {
            frames_differing.push(format!("{what}: {ours} vs {theirs}"));
        }
    }
    if substitutes != golden_substitutes {
        frames_differing.push(format!(
            "stand-ins {substitutes:?} vs {golden_substitutes:?}"
        ));
    }
    Differences {
        lines: theirs.len(),
        lines_differing,
        frames_differing,
    }
}

#[test]
fn plans_match_the_prototype() {
    let mut failures = Vec::new();
    for name in DOCUMENTS {
        let differences = compare(name);
        println!(
            "{name:<16} {} lines: {} differ in plan, {} frames differ",
            differences.lines,
            differences.lines_differing.len(),
            differences.frames_differing.len()
        );
        for line in differences
            .lines_differing
            .iter()
            .chain(&differences.frames_differing)
            .take(6)
        {
            println!("    {line}");
        }
        if !differences.lines_differing.is_empty() || !differences.frames_differing.is_empty() {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "plans differ from the prototype's for {failures:?}"
    );
}
