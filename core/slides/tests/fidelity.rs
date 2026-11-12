//! the fidelity harness (ignored by default: it needs the corpus that
//! tests/fidelity/*.ps1 build with powerpoint and libreoffice).
//!
//! for every slide it compares letters' rendering and the other
//! application's rendering with the reference application's pdf, and
//! writes the tables into FIDELITY_REPORT.md between its markers.
//!
//! cargo test -p letters-slides --test fidelity -- --ignored --nocapture

#[path = "fidelity/pdf_reader.rs"]
mod pdf_reader;
#[path = "fidelity/score.rs"]
mod score;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use letters_slides::fonts::FontLibrary;
use letters_slides::render::render_pdf;
use letters_slides::{read_odp, read_pptx};
use pdf_reader::{read_pdf, save_png, Page};
use score::{score, Score};

fn out() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fidelity/out")
}

/// one comparison set: decks, the reference application's pdfs and the
/// other application's pdfs of the same decks.
struct Set {
    title: &'static str,
    decks: PathBuf,
    reference: Box<dyn Fn(&str) -> PathBuf>,
    other: Box<dyn Fn(&str) -> PathBuf>,
    reference_name: &'static str,
    other_name: &'static str,
    letters_folder: &'static str,
    /// "pptx" or "odp"
    extension: &'static str,
}

fn sets() -> Vec<Set> {
    vec![
        Set {
            title: "Decks made by PowerPoint, reference PowerPoint",
            decks: out().join("corpus"),
            reference: Box::new(|name| out().join("corpus").join(format!("{name}.powerpoint.pdf"))),
            other: Box::new(|name| out().join("lo_pdf").join(format!("{name}.pdf"))),
            reference_name: "PowerPoint",
            other_name: "LibreOffice",
            letters_folder: "letters",
            extension: "pptx",
        },
        Set {
            // powerpoint refuses libreoffice 26.8's pptx saves of the
            // corpus, so libreoffice's own decks are its odp saves
            title: "Decks saved by LibreOffice (odp), reference LibreOffice",
            decks: out().join("lo_odp"),
            reference: Box::new(|name| out().join("lo_odp_pdf").join(format!("{name}.pdf"))),
            other: Box::new(|name| {
                out()
                    .join("lo_odp_pp")
                    .join(format!("{name}.powerpoint.pdf"))
            }),
            reference_name: "LibreOffice",
            other_name: "PowerPoint",
            letters_folder: "letters_lo",
            extension: "odp",
        },
    ]
}

struct SlideRow {
    deck: String,
    slide: usize,
    letters: Score,
    other: Score,
}

#[test]
#[ignore = "needs Microsoft PowerPoint on windows and LibreOffice"]
fn fidelity_harness() {
    let mut tables = String::new();
    let mut fonts = FontLibrary::system();
    for set in sets() {
        let rows = run_set(&set, &mut fonts);
        tables.push_str(&table(&set, &rows));
    }
    let substitutions: Vec<String> = fonts
        .substitutions
        .iter()
        .map(|s| format!("{} -> {} ({:?})", s.requested, s.used, s.reason))
        .collect();
    let _ = writeln!(
        tables,
        "\nFont substitutions while rendering: {}\n",
        if substitutions.is_empty() {
            "none".into()
        } else {
            substitutions.join("; ")
        }
    );
    write_report(&tables);
    println!("{tables}");
}

fn run_set(set: &Set, fonts: &mut FontLibrary) -> Vec<SlideRow> {
    let letters_folder = out().join(set.letters_folder);
    let pictures = out().join("png");
    let _ = std::fs::create_dir_all(&letters_folder);
    let _ = std::fs::create_dir_all(&pictures);
    let mut rows = Vec::new();
    let mut decks: Vec<PathBuf> = std::fs::read_dir(&set.decks)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    decks.retain(|path| path.extension().is_some_and(|e| e == set.extension));
    decks.sort();
    for deck in decks {
        let name = deck.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&deck).unwrap();
        let read = if set.extension == "odp" {
            read_odp(&bytes)
        } else {
            read_pptx(&bytes)
        };
        let Ok(presentation) = read else {
            println!("{name}: could not be read");
            continue;
        };
        let ours = letters_folder.join(format!("{name}.pdf"));
        match render_pdf(&presentation, None, fonts) {
            Ok((bytes, _)) => std::fs::write(&ours, bytes).unwrap(),
            Err(error) => {
                println!("{name}: {error}");
                continue;
            }
        }
        let (Ok(reference), Ok(letters), Ok(other)) = (
            read_pdf(&(set.reference)(&name)),
            read_pdf(&ours),
            read_pdf(&(set.other)(&name)),
        ) else {
            println!("{name}: a pdf is missing");
            continue;
        };
        save_first_pages(
            &pictures,
            &format!("{}_{name}", set.letters_folder),
            &reference,
            &letters,
            &other,
        );
        for (index, reference_page) in reference.iter().enumerate() {
            let empty = Page::default();
            rows.push(SlideRow {
                deck: name.clone(),
                slide: index + 1,
                letters: score(reference_page, letters.get(index).unwrap_or(&empty)),
                other: score(reference_page, other.get(index).unwrap_or(&empty)),
            });
        }
    }
    rows
}

fn save_first_pages(
    folder: &Path,
    stem: &str,
    reference: &[Page],
    letters: &[Page],
    other: &[Page],
) {
    for (label, pages) in [
        ("reference", reference),
        ("letters", letters),
        ("other", other),
    ] {
        if let Some(page) = pages.first() {
            save_png(page, &folder.join(format!("{stem}_{label}.png")));
        }
    }
}

fn table(set: &Set, rows: &[SlideRow]) -> String {
    let mut out = format!(
        "\n### {}\n\n| deck | slide | lines agree (Letters / {other}) | baseline Δ pt (L / {o}) | left Δ pt (L / {o}) | box Δ pt (L / {o}) | SSIM (L / {o}) | closer |\n|---|---|---|---|---|---|---|---|\n",
        set.title,
        other = set.other_name,
        o = &set.other_name[..1]
    );
    let mut closer = 0;
    for row in rows {
        let ours_better = row.letters.ssim >= row.other.ssim;
        if ours_better {
            closer += 1;
        }
        let _ = writeln!(
            out,
            "| {} | {} | {:.0} % / {:.0} % | {:.2} / {:.2} | {:.2} / {:.2} | {:.1} / {:.1} | {:.3} / {:.3} | {} |",
            row.deck,
            row.slide,
            row.letters.line_agreement() * 100.0,
            row.other.line_agreement() * 100.0,
            row.letters.baseline_difference,
            row.other.baseline_difference,
            row.letters.left_difference,
            row.other.left_difference,
            row.letters.box_difference,
            row.other.box_difference,
            row.letters.ssim,
            row.other.ssim,
            if ours_better { "Letters" } else { set.other_name }
        );
    }
    out.push_str(&summary(set, rows, closer));
    out
}

fn summary(set: &Set, rows: &[SlideRow], closer: usize) -> String {
    let count = rows.len().max(1) as f64;
    let mean = |pick: &dyn Fn(&SlideRow) -> f64| rows.iter().map(pick).sum::<f64>() / count;
    let lines = |pick: &dyn Fn(&SlideRow) -> &Score| {
        let matched: usize = rows.iter().map(|r| pick(r).matched_lines).sum();
        let total: usize = rows.iter().map(|r| pick(r).reference_lines).sum();
        matched as f64 / total.max(1) as f64 * 100.0
    };
    format!(
        "\n**All {} slides:** lines agree {:.1} % (Letters) vs {:.1} % ({}); mean SSIM {:.3} vs {:.3}; mean baseline Δ {:.2} pt vs {:.2} pt; Letters is closer to {} on {} of {} slides (by SSIM).\n",
        rows.len(),
        lines(&|r| &r.letters),
        lines(&|r| &r.other),
        set.other_name,
        mean(&|r| r.letters.ssim),
        mean(&|r| r.other.ssim),
        mean(&|r| r.letters.baseline_difference),
        mean(&|r| r.other.baseline_difference),
        set.reference_name,
        closer,
        rows.len()
    )
}

/// replaces the generated part of FIDELITY_REPORT.md.
fn write_report(tables: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("FIDELITY_REPORT.md");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let (start, end) = ("<!-- tables -->", "<!-- /tables -->");
    let new = match (text.find(start), text.find(end)) {
        (Some(a), Some(b)) if a < b => format!("{}{start}\n{tables}\n{}", &text[..a], &text[b..]),
        _ => format!("{text}\n{start}\n{tables}\n{end}\n"),
    };
    let _ = std::fs::write(path, new);
}

/// prints each reference line with the matching letters line, for the
/// deck named in FIDELITY_DECK (a diagnosis aid for the report's causes).
#[test]
#[ignore = "diagnostic"]
fn fidelity_lines() {
    let name = std::env::var("FIDELITY_DECK").unwrap_or_else(|_| "pp_title_bullets".into());
    let reference = read_pdf(&out().join("corpus").join(format!("{name}.powerpoint.pdf"))).unwrap();
    let letters = read_pdf(&out().join("letters").join(format!("{name}.pdf"))).unwrap();
    for (index, page) in reference.iter().enumerate() {
        let ours = score::lines(&letters[index].glyphs);
        for line in score::lines(&page.glyphs) {
            let found = ours.iter().find(|l| l.text == line.text);
            let size = page
                .glyphs
                .iter()
                .find(|g| (g.y - line.baseline).abs() < 0.6)
                .map_or(0.0, |g| g.size);
            println!(
                "slide {} size {size:.1} ref {:.2},{:.2} ours {:?} {}",
                index + 1,
                line.left,
                line.baseline,
                found.map(|l| (format!("{:.2}", l.left), format!("{:.2}", l.baseline))),
                line.text.chars().take(30).collect::<String>()
            );
        }
    }
}
