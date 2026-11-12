//! the evaluation harness for font completion: how seamless is text
//! added in a font the document only has a subset of?
//!
//! for each test font, a subset holding only the letters of a first
//! sentence is made, as a pdf would embed it. the pipeline is then asked
//! for the letters of a second sentence, with only the fonts a scenario
//! allows as donors, and every completed glyph is compared with the true
//! glyph of the full font (which the pipeline never sees when the
//! scenario says it isn't there): outline distance, advance error, and
//! the whole second sentence drawn at 11 pt and 300 dpi with the true
//! font and with the completed one (ssim over inked windows, and the
//! share of inked pixels that differ).
//!
//! run from the repository root:
//!
//! ```text
//! cargo run --release -p letters-fonts --example completion_report -- core/fonts/COMPLETION_REPORT.md
//! ```
//!
//! fonts: the bundled open fonts in vendor/fonts (tools/fetch-fonts.ps1
//! fetches them), and fonts installed on this computer, used in place as
//! test subjects only (never copied). set LETTERS_DUMP to a folder to
//! also get each pair of renderings as .pgm pictures.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use letters_fonts::complete::{
    complete_font, complete_font_with, plan_generation, CompleteOptions, CompletedFont,
    FakeGenerator, Generation, GlyphGenerator, Strategy, SubsetFont,
};
use letters_fonts::matching::distance::{shape_distance, Samples};
use letters_fonts::matching::donors::{DonorFile, DonorPool};
use letters_fonts::matching::glyphs::{read_glyphs, GlyphData, GlyphSet};
use letters_fonts::matching::shape::Shape;
use letters_fonts::matching::style::measure_style;
use letters_fonts::model::Source;
use letters_fonts::outline::Segment;
use letters_fonts::raster::{fill, AlphaBitmap};

mod fonts;
mod measure;
mod render;
mod report;

// the parts share this file's imports through `use super::*`
use fonts::*;
use measure::*;
use render::*;
use report::*;

/// the letters the subset holds.
const FIRST: &str = "The annual report shows revenue rose in the third quarter.";

/// the new text: many letters the subset lacks, some it has.
const SECOND: &str = "Jack Ferguson mixed 42 big jazz kits by Friday, Bob said.";

/// for accents: a subset with some accented letters...
const FIRST_ACCENTS: &str =
    "Oh, le café crème à côté du théâtre? The quick brown fox jumps over a lazy dog - AND Paris.";
/// ...and new text whose accented letters can be built from them.
const SECOND_ACCENTS: &str = "Où êtes-vous? À la fenêtre, près du lit, déjà là.";

/// 11 pt at 300 dpi, in pixels per em.
const PIXELS_PER_EM: f64 = 11.0 / 72.0 * 300.0;

/// sans fonts unrelated to the sans targets (scenario d).
const SANS_UNRELATED: [&str; 9] = [
    "Source Sans 3",
    "Noto Sans",
    "Inter",
    "Atkinson Hyperlegible",
    "Verdana",
    "Tahoma",
    "Segoe UI",
    "Corbel",
    "Candara",
];

/// serif fonts unrelated to the serif targets (scenario d).
const SERIF_UNRELATED: [&str; 7] = [
    "Source Serif 4",
    "Noto Serif",
    "EB Garamond",
    "Georgia",
    "Constantia",
    "Caladea",
    "Cambria",
];

/// one test font, read into memory.
struct Font {
    file: PathBuf,
    bytes: Vec<u8>,
    family: String,
    style: String,
    source: Source,
    postscript: String,
}

/// makes a fake provider that answers for the characters asked.
type MakeGenerator<'a> = dyn Fn(&[char]) -> FakeGenerator + 'a;

/// how a fake provider's drawing differs from the true glyph.
type GlyphChange<'a> = dyn Fn(&GlyphData) -> Option<GlyphData> + 'a;

/// one completion to measure.
struct Case<'a> {
    scenario: &'static str,
    target: &'a Font,
    pool: DonorPool,
    /// the donors allowed, in words for the table
    pool_words: String,
    first: &'static str,
    second: &'static str,
    /// measure only these new characters (all when None)
    only: Option<&'a [char]>,
    generation: Option<&'a MakeGenerator<'a>>,
}

impl<'a> Case<'a> {
    fn new(
        scenario: &'static str,
        target: &'a Font,
        pool: DonorPool,
        pool_words: &str,
    ) -> Case<'a> {
        Case {
            scenario,
            target,
            pool,
            pool_words: pool_words.to_string(),
            first: FIRST,
            second: SECOND,
            only: None,
            generation: None,
        }
    }
}

/// one row of the report.
struct Row {
    scenario: &'static str,
    target: String,
    pool: String,
    measured: Measured,
    donors: String,
    fit: String,
    seconds: f64,
    sentence: String,
}

/// completes the case's text, through the fake provider when it has one.
fn complete_case(case: &Case, subset: &SubsetFont) -> CompletedFont {
    let options = CompleteOptions::default();
    let Some(make) = case.generation else {
        return complete_font(subset, case.second, &case.pool, &options).expect("completion");
    };
    let needed: Vec<char> = case.second.chars().filter(|c| !subset.has(*c)).collect();
    let fake = make(&needed);
    let style = measure_style(&subset.glyphs);
    let plan = plan_generation(&subset.glyphs, &style, &needed, &fake.describe());
    let acknowledgement = Some(plan.disclosure().acknowledge());
    let generation = Generation {
        generator: &fake,
        plan,
        acknowledgement,
    };
    complete_font_with(subset, case.second, &case.pool, &options, Some(&generation))
        .expect("completion")
}

/// the fit column: the fitted family and its held-out similarity, when a
/// fitted donor drew any glyph.
fn fit_words(completed: &CompletedFont) -> String {
    let used_a_fit = completed
        .glyphs
        .iter()
        .any(|g| g.strategy == Strategy::FittedDonor);
    let Some(fit) = completed.report.fit.as_ref().filter(|_| used_a_fit) else {
        return "–".into();
    };
    format!(
        "{} {:.1}% (was {:.1}%)",
        fit.family, fit.similarity, fit.unchanged_similarity
    )
}

fn run(rows: &mut Vec<Row>, case: Case) {
    // LETTERS_ONLY="c Arial" runs only the rows starting with those words
    let row_name = format!("{} {}", case.scenario, label(case.target));
    if std::env::var("LETTERS_ONLY").is_ok_and(|only| !row_name.starts_with(&only)) {
        return;
    }
    let subset = make_subset(case.target, case.first);
    let truth = read_glyphs(&case.target.bytes, 0, &[], case.second.chars()).expect("truth");
    let started = Instant::now();
    let completed = complete_case(&case, &subset);
    let seconds = started.elapsed().as_secs_f64();
    let picture_name = row_name.replace(' ', "_");
    let measured = measure(
        &completed,
        &subset,
        &truth,
        case.second,
        case.only,
        Some(&picture_name),
    );
    eprintln!("{row_name} -> {} ({seconds:.1} s)", strategy_mix(&measured));
    rows.push(Row {
        scenario: case.scenario,
        target: label(case.target),
        pool: case.pool_words.clone(),
        donors: used_donors(&completed),
        fit: fit_words(&completed),
        seconds,
        sentence: completed
            .report
            .sentences
            .first()
            .cloned()
            .unwrap_or_default(),
        measured,
    });
}

/// a fake generator whose replies are the true glyphs changed by `change`.
fn fake_from_truth(
    target: &Font,
    needed: &[char],
    change: &GlyphChange<'_>,
    malformed: bool,
) -> FakeGenerator {
    let truth = read_glyphs(&target.bytes, 0, &[], needed.iter().copied()).expect("truth");
    let glyphs: BTreeMap<char, GlyphData> = truth
        .glyphs
        .iter()
        .filter_map(|(c, g)| Some((*c, change(g)?)))
        .collect();
    let mut reply = FakeGenerator::reply_for(&glyphs);
    if malformed {
        // half the paths lose their closing Z, the rest get a stray arc
        reply = reply
            .replacen("Z\"", "\"", glyphs.len() / 2)
            .replace("Z\"", " A 1 1 0 0 1 0 0Z\"");
    }
    FakeGenerator::new(true, vec![reply])
}

/// a small, repeatable wobble of every point, like a good but imperfect
/// drawing: up to two thousandths of the em.
fn wobble(glyph: &GlyphData) -> Option<GlyphData> {
    let n = std::cell::Cell::new(0u32);
    let shape: Shape = glyph.shape.map(|x, y| {
        // a linear congruential sequence: the same wobble on every run
        n.set(n.get().wrapping_mul(1_103_515_245).wrapping_add(12_345));
        let a = f64::from((n.get() >> 16) % 1000) / 1000.0 - 0.5;
        (x + a * 0.004, y - a * 0.004)
    });
    Some(GlyphData {
        shape,
        advance: glyph.advance,
    })
}

/// a drawing 18% taller and 10% wider than the true glyph: off style.
fn taller(glyph: &GlyphData) -> Option<GlyphData> {
    Some(GlyphData {
        shape: glyph.shape.scaled(1.0, 1.18, 0.0, 0.0),
        advance: glyph.advance * 1.1,
    })
}

fn unchanged(glyph: &GlyphData) -> Option<GlyphData> {
    Some(glyph.clone())
}

/// (a) the same font is on this computer, among everything else.
fn same_font_installed(fonts: &[Font], rows: &mut Vec<Row>) {
    let targets = ["Arial", "Times New Roman", "Calibri", "Tinos", "Georgia"];
    for family in targets {
        if let Some(target) = find(fonts, family, "Regular") {
            let pool = pool_of(fonts, |_| true);
            run(rows, Case::new("a", target, pool, "all test fonts"));
        }
    }
}

/// (b) only other weights or versions of the family.
fn other_weights_only(fonts: &[Font], rows: &mut Vec<Row>) {
    let targets = ["Arial", "Times New Roman", "Calibri", "Segoe UI", "Verdana"];
    for family in targets {
        let Some(target) = find(fonts, family, "Regular") else {
            continue;
        };
        let is_other_weight = |f: &Font| f.family == target.family && f.style != target.style;
        let others: Vec<String> = fonts
            .iter()
            .filter(|f| is_other_weight(f))
            .map(|f| f.style.clone())
            .collect();
        let words = format!("{family} {}", others.join("/"));
        let pool = pool_of(fonts, is_other_weight);
        run(rows, Case::new("b", target, pool, &words));
    }
}

/// (c) only the metric-compatible open stand-in.
fn metric_twin_only(fonts: &[Font], rows: &mut Vec<Row>) {
    let targets = [
        ("Arial", "Regular", "Arimo"),
        ("Arial", "Bold", "Arimo"),
        ("Calibri", "Regular", "Carlito"),
        ("Times New Roman", "Regular", "Tinos"),
    ];
    for (family, style, twin) in targets {
        if let Some(target) = find(fonts, family, style) {
            let pool = pool_of(fonts, |f| f.family == twin);
            run(rows, Case::new("c", target, pool, twin));
        }
    }
}

/// the unrelated fonts of a target's kind, and their name in the table.
fn unrelated(serif: bool) -> (&'static [&'static str], &'static str) {
    if serif {
        (&SERIF_UNRELATED, "unrelated serifs")
    } else {
        (&SANS_UNRELATED, "unrelated sans")
    }
}

/// (d) only unrelated fonts of the same kind.
fn unrelated_only(fonts: &[Font], rows: &mut Vec<Row>) {
    let targets = [
        ("Arial", false),
        ("Calibri", false),
        ("Times New Roman", true),
        ("Georgia", true),
        ("Segoe UI", false),
    ];
    for (family, serif) in targets {
        let Some(target) = find(fonts, family, "Regular") else {
            continue;
        };
        let (class, words) = unrelated(serif);
        let pool = pool_of(fonts, |f| {
            class.contains(&f.family.as_str()) && f.family != target.family
        });
        run(rows, Case::new("d", target, pool, words));
    }
}

/// (e) no donor at all: accents built from the subset's own glyphs.
fn accents_only(fonts: &[Font], rows: &mut Vec<Row>) {
    let accented: Vec<char> = SECOND_ACCENTS
        .chars()
        .filter(|c| !c.is_ascii() && !FIRST_ACCENTS.contains(*c))
        .collect();
    for family in ["Arial", "Times New Roman", "Calibri", "Georgia"] {
        if let Some(target) = find(fonts, family, "Regular") {
            let case = Case {
                first: FIRST_ACCENTS,
                second: SECOND_ACCENTS,
                only: Some(&accented),
                ..Case::new("e", target, DonorPool::default(), "none")
            };
            run(rows, case);
        }
    }
}

/// (f) the fake generated-glyph provider on top of unrelated donors.
fn fake_provider(fonts: &[Font], rows: &mut Vec<Row>) {
    let cases: [(&'static str, &GlyphChange<'_>, bool); 3] = [
        ("f good", &wobble, false),
        ("f off-style", &taller, false),
        ("f malformed", &unchanged, true),
    ];
    for (family, serif) in [("Arial", false), ("Times New Roman", true)] {
        let Some(target) = find(fonts, family, "Regular") else {
            continue;
        };
        let (class, words) = unrelated(serif);
        for (scenario, change, malformed) in cases {
            let pool = pool_of(fonts, |f| class.contains(&f.family.as_str()));
            let make = |needed: &[char]| fake_from_truth(target, needed, change, malformed);
            let case = Case {
                generation: Some(&make),
                ..Case::new(scenario, target, pool, &format!("{words} + fake provider"))
            };
            run(rows, case);
        }
    }
}

fn main() {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "core/fonts/COMPLETION_REPORT.md".into());
    let fonts = all_fonts();
    eprintln!("{} test fonts", fonts.len());
    let mut rows = Vec::new();
    same_font_installed(&fonts, &mut rows);
    other_weights_only(&fonts, &mut rows);
    metric_twin_only(&fonts, &mut rows);
    unrelated_only(&fonts, &mut rows);
    accents_only(&fonts, &mut rows);
    fake_provider(&fonts, &mut rows);
    let report = write_report(&rows, fonts.len());
    std::fs::write(&output, report).expect("report written");
    eprintln!("wrote {output}");
}
