//! the mac word check: does word for mac place text where word for windows does?
//!
//! all calibration so far is from word 16.0.20527 on windows. this makes a
//! kit of corpus docx files on windows, and later measures the pdfs word
//! for mac exported from them (tools/mac-word/README.md has the steps).
//!
//!   cargo run -p letters-exact --example mac_word -- kit <kit folder>
//!   cargo run -p letters-exact --example mac_word -- measure <kit folder> <mac results folder>
//!
//! the kit has three docx per document:
//! - `locked-model`: locked layout from the built-in tables, no word involved
//! - `locked`: locked layout, its lines checked through word for windows
//! - `fitted`: fitted fonts, the default mode, measured through word for windows
//!
//! locked files are measured straight against the source pdf, with the
//! same function and thresholds as the converter's own verification. a
//! fitted file can't be: each word is one composite glyph, and finding its
//! letters needs the conversion's plan. but its ink is the glyph's printed
//! origin plus offsets inside a font both words read from the same docx, so
//! comparing where the two words print each glyph tells the whole story:
//! equal origins mean equal errors, and any difference adds to the error.

use std::path::{Path, PathBuf};

use letters_exact::bridge::WordBridge;
use letters_exact::calibration::WordModel;
use letters_exact::pdf::{PageContent, PdfFile};
use letters_exact::verify::{self, GlyphStats};
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

/// the three ways each document is converted for the kit.
const VARIANTS: [&str; 3] = ["locked-model", "locked", "fitted"];

/// glyphs of the two words' pdfs are paired only this close; further apart
/// they are counted as unmatched (other lines are 8 pt or more away).
const PAIR_RADIUS_PT: f64 = 4.0;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["kit", folder] => make_kit(Path::new(folder)),
        ["measure", kit, mac] => measure(Path::new(kit), Path::new(mac)),
        _ => {
            eprintln!(
                "usage: mac_word kit <folder> | mac_word measure <kit folder> <mac results folder>"
            );
            std::process::exit(2);
        }
    }
}

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

fn options_for(variant: &str) -> ConvertOptions {
    let (mode, use_word) = match variant {
        "locked-model" => (OutputMode::LockedLayout, false),
        "locked" => (OutputMode::LockedLayout, true),
        _ => (OutputMode::FittedLayout, true),
    };
    ConvertOptions {
        mode,
        use_word,
        check_passes: use_word,
        ..ConvertOptions::default()
    }
}

/// converts every document three ways, and exports each through word for
/// windows as the reference the mac's pdfs are compared with.
fn make_kit(kit: &Path) {
    let docx_folder = kit.join("docx");
    let windows_folder = kit.join("windows-pdf");
    std::fs::create_dir_all(&docx_folder).unwrap();
    std::fs::create_dir_all(&windows_folder).unwrap();
    let mut jobs = Vec::new();
    for name in DOCUMENTS {
        for variant in VARIANTS {
            let docx = docx_folder.join(format!("{name}.{variant}.docx"));
            let pdf = corpus().join(format!("{name}.pdf"));
            convert(&pdf, &docx, &options_for(variant)).unwrap();
            jobs.push((docx, windows_folder.join(format!("{name}.{variant}.pdf"))));
            println!("converted {name} ({variant})");
        }
    }
    let bridge = WordBridge {
        seconds_per_document: 120,
        work_dir: std::env::temp_dir().join("letters-mac-word-kit"),
    };
    std::fs::create_dir_all(&bridge.work_dir).unwrap();
    let run = bridge.export(&jobs, &CancelFlag::default()).unwrap();
    let build = run.build.unwrap_or_default();
    std::fs::write(
        kit.join("windows-word.txt"),
        format!("word_build={build}\n"),
    )
    .unwrap();
    println!(
        "exported {} files with word for windows {build}",
        jobs.len()
    );
}

fn pages_of(path: &Path) -> Vec<PageContent> {
    PdfFile::open(path).unwrap().read_pages()
}

/// prints one table: each variant of each document, measured on the mac.
fn measure(kit: &Path, mac: &Path) {
    // the producer says which export made the pdf: word's own ("best for
    // printing") or microsoft's online service, which lays out differently
    let first_mac_pdf = mac.join(format!("{}.{}.pdf", DOCUMENTS[0], VARIANTS[0]));
    println!("mac pdf producer: {}\n", producer_of(&first_mac_pdf));
    println!("| document | variant | mac vs source max / mean (pt) | windows vs source max / mean (pt) | mac vs windows origins max / mean (pt) | unpaired |");
    println!("|---|---|---|---|---|---|");
    for name in DOCUMENTS {
        let source = pages_of(&corpus().join(format!("{name}.pdf")));
        for variant in VARIANTS {
            let file = format!("{name}.{variant}.pdf");
            let mac_pdf = mac.join(&file);
            let windows_pdf = kit.join("windows-pdf").join(&file);
            if !mac_pdf.exists() {
                println!("| {name} | {variant} | missing | | | |");
                continue;
            }
            let against_source = |pdf: &Path| -> String {
                if variant == "fitted" {
                    return "see origins".into();
                }
                let report = verify::measure(
                    &source,
                    pdf,
                    &mut WordModel::built_in(),
                    Vec::new(),
                    None,
                    None,
                )
                .unwrap();
                stats_cell(&report.glyphs, report.positions_pass)
            };
            let origins = compare_origins(&pages_of(&windows_pdf), &pages_of(&mac_pdf));
            println!(
                "| {name} | {variant} | {} | {} | {:.4} / {:.4} | {} |",
                against_source(&mac_pdf),
                against_source(&windows_pdf),
                origins.max_pt,
                origins.mean_pt,
                origins.unpaired,
            );
        }
    }
}

/// the text after `/Producer` in a pdf, as far as it is plain ascii.
/// good enough to tell word's own export from the online service's.
fn producer_of(pdf: &Path) -> String {
    let Ok(bytes) = std::fs::read(pdf) else {
        return "unknown (the file is missing)".into();
    };
    let Some(start) = bytes.windows(9).position(|window| window == b"/Producer") else {
        return "not recorded".into();
    };
    bytes[start + 9..]
        .iter()
        .take(120)
        .map(|byte| *byte as char)
        .take_while(|character| *character != '/' && *character != '>')
        .filter(|character| character.is_ascii_graphic() || *character == ' ')
        .collect::<String>()
        .trim()
        .to_string()
}

fn stats_cell(glyphs: &GlyphStats, pass: bool) -> String {
    let verdict = if pass { "pass" } else { "FAIL" };
    format!("{:.4} / {:.4} {verdict}", glyphs.max_pt, glyphs.mean_pt)
}

/// how far apart the two words printed the same glyphs.
struct OriginDifference {
    max_pt: f64,
    mean_pt: f64,
    /// glyphs of either pdf with no partner within `PAIR_RADIUS_PT`
    unpaired: usize,
}

/// pairs each glyph windows printed with the mac's glyph of the same text
/// on the same page nearest it, and measures the distance between origins.
fn compare_origins(windows: &[PageContent], mac: &[PageContent]) -> OriginDifference {
    let mut distances = Vec::new();
    let mut unpaired = 0;
    let mac_glyphs: Vec<_> = mac.iter().flat_map(|page| &page.glyphs).collect();
    let mut taken = vec![false; mac_glyphs.len()];
    for glyph in windows.iter().flat_map(|page| &page.glyphs) {
        let nearest = mac_glyphs
            .iter()
            .enumerate()
            .filter(|(index, other)| {
                !taken[*index] && other.page == glyph.page && other.text == glyph.text
            })
            .map(|(index, other)| (index, (other.x - glyph.x).hypot(other.y - glyph.y)))
            .filter(|(_, distance)| *distance <= PAIR_RADIUS_PT)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match nearest {
            Some((index, distance)) => {
                taken[index] = true;
                distances.push(distance);
            }
            None => unpaired += 1,
        }
    }
    unpaired += taken.iter().filter(|was_taken| !**was_taken).count();
    let max_pt = distances.iter().copied().fold(0.0, f64::max);
    let mean_pt = if distances.is_empty() {
        0.0
    } else {
        distances.iter().sum::<f64>() / distances.len() as f64
    };
    OriginDifference {
        max_pt,
        mean_pt,
        unpaired,
    }
}
