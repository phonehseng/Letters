//! the back-and-forth matrix (ignored by default: it drives powerpoint
//! and libreoffice). pptx -> letters -> odp -> letters -> pptx, rendered
//! by powerpoint, against powerpoint's own pptx -> odp -> pptx; and odp
//! -> letters -> pptx -> letters -> odp, rendered by libreoffice, against
//! libreoffice's own odp -> pptx -> odp. scores go into
//! FIDELITY_REPORT.md between its back-and-forth markers.
//!
//! cargo test -p letters-slides --release --test back_and_forth -- --ignored --nocapture

#[path = "fidelity/pdf_reader.rs"]
mod pdf_reader;
#[path = "fidelity/score.rs"]
mod score;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use letters_slides::{read_odp, read_pptx, write_odp, write_pptx};
use pdf_reader::{read_pdf, Page};
use score::{score, Score};

const SOFFICE: &str = r"C:\Users\phone\AppData\Local\Programs\LibreOffice\program\soffice.com";

fn out() -> PathBuf {
    // separate joins keep the separators consistent for libreoffice
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fidelity")
        .join("out")
}

fn files(folder: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    found.retain(|p| p.extension().is_some_and(|e| e == extension));
    found.sort();
    found
}

fn stem(path: &Path) -> String {
    path.file_stem().unwrap().to_string_lossy().to_string()
}

fn powerpoint_chain(decks: &Path, folder: &Path, export: &Path) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fidelity/powerpoint_chain.ps1");
    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(script)
        .arg("-Decks")
        .arg(decks)
        .arg("-Out")
        .arg(folder)
        .arg("-Export")
        .arg(export)
        .output()
        .expect("powershell runs");
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

fn libreoffice(files: &[PathBuf], to: &str, folder: &Path) {
    if files.is_empty() {
        return;
    }
    let profile = out().join("lo_profile");
    let profile_url = format!("file:///{}", profile.to_string_lossy().replace('\\', "/"));
    println!(
        "libreoffice: {} files to {to} in {}, first {}",
        files.len(),
        folder.display(),
        files[0].display()
    );
    let status = Command::new(SOFFICE)
        .arg(format!("-env:UserInstallation={profile_url}"))
        .args(["--headless", "--norestore", "--convert-to", to, "--outdir"])
        .arg(folder)
        .args(files)
        .status()
        .expect("libreoffice runs");
    assert!(status.success());
}

struct Row {
    deck: String,
    letters: Vec<Score>,
    other: Vec<Score>,
}

fn scores(reference: &Path, candidate: &Path) -> Vec<Score> {
    let (Ok(reference), Ok(candidate)) = (read_pdf(reference), read_pdf(candidate)) else {
        return Vec::new();
    };
    let empty = Page::default();
    reference
        .iter()
        .enumerate()
        .map(|(index, page)| score(page, candidate.get(index).unwrap_or(&empty)))
        .collect()
}

#[test]
#[ignore]
fn back_and_forth_matrix() {
    let mut report = String::new();
    report.push_str(&forward());
    report.push_str(&reverse());
    write_report(&report);
    println!("{report}");
}

/// pptx made by powerpoint, judged by powerpoint.
fn forward() -> String {
    let corpus = out().join("corpus");
    let ours = out().join("chain_letters");
    let theirs = out().join("chain_powerpoint");
    let _ = std::fs::create_dir_all(&ours);
    for deck in files(&corpus, "pptx") {
        let presentation = read_pptx(&std::fs::read(&deck).unwrap()).unwrap();
        let (odp, _) = write_odp(&presentation).unwrap();
        std::fs::write(ours.join(format!("{}.odp", stem(&deck))), &odp).unwrap();
        let back = write_pptx(&read_odp(&odp).unwrap()).unwrap();
        std::fs::write(ours.join(format!("{}.pptx", stem(&deck))), back).unwrap();
    }
    powerpoint_chain(&corpus, &theirs, &theirs);
    powerpoint_chain(&out().join("chain_none"), &out().join("chain_none"), &ours);
    let rows: Vec<Row> = files(&corpus, "pptx")
        .iter()
        .map(|deck| {
            let name = stem(deck);
            let reference = corpus.join(format!("{name}.powerpoint.pdf"));
            Row {
                deck: name.clone(),
                letters: scores(&reference, &ours.join(format!("{name}.powerpoint.pdf"))),
                other: scores(&reference, &theirs.join(format!("{name}.powerpoint.pdf"))),
            }
        })
        .collect();
    table(
        "pptx -> odp -> pptx, rendered by PowerPoint (Letters' chain vs PowerPoint's own)",
        "PowerPoint",
        &rows,
    )
}

/// odp made by libreoffice, judged by libreoffice.
fn reverse() -> String {
    let decks = out().join("lo_odp");
    let ours = out().join("chain_letters_odp");
    let theirs_pptx = out().join("chain_libreoffice_pptx");
    let theirs = out().join("chain_libreoffice");
    let _ = std::fs::create_dir_all(&ours);
    let originals = files(&decks, "odp");
    for deck in &originals {
        let presentation = read_odp(&std::fs::read(deck).unwrap()).unwrap();
        let pptx = write_pptx(&presentation).unwrap();
        std::fs::write(ours.join(format!("{}.pptx", stem(deck))), &pptx).unwrap();
        let (back, _) = write_odp(&read_pptx(&pptx).unwrap()).unwrap();
        std::fs::write(ours.join(format!("{}.odp", stem(deck))), back).unwrap();
    }
    libreoffice(
        &originals,
        "pptx:Impress MS PowerPoint 2007 XML",
        &theirs_pptx,
    );
    libreoffice(&files(&theirs_pptx, "pptx"), "odp", &theirs);
    libreoffice(&files(&theirs, "odp"), "pdf", &theirs);
    libreoffice(&files(&ours, "odp"), "pdf", &ours);
    let rows: Vec<Row> = originals
        .iter()
        .map(|deck| {
            let name = stem(deck);
            let reference = out().join("lo_odp_pdf").join(format!("{name}.pdf"));
            Row {
                deck: name.clone(),
                letters: scores(&reference, &ours.join(format!("{name}.pdf"))),
                other: scores(&reference, &theirs.join(format!("{name}.pdf"))),
            }
        })
        .collect();
    table(
        "odp -> pptx -> odp, rendered by LibreOffice (Letters' chain vs LibreOffice's own)",
        "LibreOffice",
        &rows,
    )
}

fn table(title: &str, other: &str, rows: &[Row]) -> String {
    let mut out = format!("\n### {title}\n\n| deck | slides | lines agree (Letters / {other}) | baseline Δ pt | box Δ pt | SSIM | closer |\n|---|---|---|---|---|---|---|\n");
    let mut closer = 0;
    for row in rows {
        let summary = |list: &[Score]| {
            let count = list.len().max(1) as f64;
            let matched: usize = list.iter().map(|s| s.matched_lines).sum();
            let total: usize = list.iter().map(|s| s.reference_lines).sum();
            (
                matched as f64 / total.max(1) as f64 * 100.0,
                list.iter().map(|s| s.baseline_difference).sum::<f64>() / count,
                list.iter().map(|s| s.box_difference).sum::<f64>() / count,
                list.iter().map(|s| s.ssim).sum::<f64>() / count,
            )
        };
        let (ours, theirs) = (summary(&row.letters), summary(&row.other));
        let ours_better = !row.letters.is_empty() && (row.other.is_empty() || ours.3 >= theirs.3);
        if ours_better {
            closer += 1;
        }
        let _ = writeln!(
            out,
            "| {} | {} | {:.0} % / {:.0} % | {:.2} / {:.2} | {:.1} / {:.1} | {:.3} / {:.3} | {} |",
            row.deck,
            row.letters.len(),
            ours.0,
            theirs.0,
            ours.1,
            theirs.1,
            ours.2,
            theirs.2,
            ours.3,
            theirs.3,
            if ours_better { "Letters" } else { other }
        );
    }
    let _ = writeln!(out, "\nLetters' chain is closer to the original on {closer} of {} decks (by SSIM; a missing pdf counts against it).", rows.len());
    out
}

fn write_report(tables: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("FIDELITY_REPORT.md");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let (start, end) = ("<!-- back-and-forth -->", "<!-- /back-and-forth -->");
    let new = match (text.find(start), text.find(end)) {
        (Some(a), Some(b)) if a < b => format!("{}{start}\n{tables}\n{}", &text[..a], &text[b..]),
        _ => format!("{text}\n## Back and forth\n\n{start}\n{tables}\n{end}\n"),
    };
    let _ = std::fs::write(path, new);
}
