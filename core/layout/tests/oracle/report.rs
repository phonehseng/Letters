//! writes ORACLE_REPORT.md from the scores of a run, and keeps a history
//! of the headline numbers so regressions between iterations show.

use std::collections::BTreeMap;
use std::fmt::Write;

use super::score::{combine, Score};

/// one document's four comparisons.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DocumentScores {
    pub name: String,
    pub class: String,
    pub libreoffice_vs_word: Option<Score>,
    pub letters_vs_word: Option<Score>,
    pub word_vs_libreoffice: Option<Score>,
    pub letters_vs_libreoffice: Option<Score>,
    pub layout_ms: f64,
    /// render comparison of our pdf against each oracle's: (ssim, % pixels differing)
    #[serde(default)]
    pub render_vs_word: Option<(f64, f64)>,
    #[serde(default)]
    pub render_vs_libreoffice: Option<(f64, f64)>,
    #[serde(default)]
    pub render_libreoffice_vs_word: Option<(f64, f64)>,
    /// the same document converted to odt by libreoffice, then laid out by
    /// word and by letters, both scored against libreoffice's layout of it
    #[serde(default)]
    pub odt_word_vs_libreoffice: Option<Score>,
    #[serde(default)]
    pub odt_letters_vs_libreoffice: Option<Score>,
}

/// one run's headline numbers per class, for the history table.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub iteration: String,
    /// class -> (letters vs word same-line %, letters vs word mean baseline,
    /// letters vs lo same-line %, page breaks vs word %)
    pub classes: BTreeMap<String, (f64, f64, f64, f64)>,
}

fn percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn cell(score: &Option<Score>) -> String {
    match score {
        Some(s) => format!(
            "{} | {} | {:.2} / {:.2} | {:.2} / {:.2}",
            percent(s.page_break_share()),
            percent(s.same_line_share()),
            s.baseline.mean,
            s.baseline.max,
            s.line_x.mean,
            s.line_x.max
        ),
        None => "- | - | - | -".into(),
    }
}

fn classes(documents: &[DocumentScores]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for document in documents {
        if !seen.contains(&document.class) {
            seen.push(document.class.clone());
        }
    }
    seen
}

fn combined<'a>(
    documents: impl Iterator<Item = &'a DocumentScores>,
    pick: fn(&DocumentScores) -> &Option<Score>,
) -> Option<Score> {
    let scores: Vec<&Score> = documents.filter_map(|d| pick(d).as_ref()).collect();
    (!scores.is_empty()).then(|| combine(&scores))
}

/// "yes" when ours is closer to the oracle than the other application:
/// more lines broken the same, or as many and a smaller baseline error.
fn verdict(ours: &Option<Score>, other_app: &Option<Score>) -> &'static str {
    match (ours, other_app) {
        (Some(ours), Some(other)) => {
            let ours_share = ours.same_line_share();
            let other_share = other.same_line_share();
            if ours_share > other_share + 0.001 {
                "yes"
            } else if (ours_share - other_share).abs() <= 0.001
                && ours.baseline.mean < other.baseline.mean
            {
                "yes (equal breaks, closer baselines)"
            } else {
                "not yet"
            }
        }
        _ => "-",
    }
}

/// picks one comparison out of a document's scores.
type Pick = fn(&DocumentScores) -> &Option<Score>;

pub fn write_report(
    documents: &[DocumentScores],
    history: &[HistoryEntry],
    header: &str,
    analysis: &str,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Oracle report\n");
    let _ = writeln!(out, "{header}\n");
    let _ = writeln!(
        out,
        "## Headline: are we closer to each application than the other application is?\n"
    );
    let _ = writeln!(
        out,
        "Each cell: page breaks agreeing | lines breaking at the same words | baseline difference mean / max (pt) | line start difference mean / max (pt). \
         \"LibreOffice vs Word\" is what a Word user sees when the same file is opened in LibreOffice: the baseline to beat.\n"
    );
    let _ = writeln!(out, "### Against Word (DOCX)\n");
    let _ = writeln!(
        out,
        "| class | LibreOffice vs Word | Letters (Word flavor) vs Word | beats LibreOffice? |"
    );
    let _ = writeln!(out, "|---|---|---|---|");
    for class in classes(documents) {
        let in_class = || documents.iter().filter(|d| d.class == class);
        let other = combined(in_class(), |d| &d.libreoffice_vs_word);
        let ours = combined(in_class(), |d| &d.letters_vs_word);
        let _ = writeln!(
            out,
            "| {class} | {} | {} | {} |",
            cell(&other),
            cell(&ours),
            verdict(&ours, &other)
        );
    }
    let _ = writeln!(out, "\n### Against LibreOffice\n");
    let _ = writeln!(out, "| class | Word vs LibreOffice | Letters (LibreOffice flavor) vs LibreOffice | beats Word? |");
    let _ = writeln!(out, "|---|---|---|---|");
    for class in classes(documents) {
        let in_class = || documents.iter().filter(|d| d.class == class);
        let other = combined(in_class(), |d| &d.word_vs_libreoffice);
        let ours = combined(in_class(), |d| &d.letters_vs_libreoffice);
        let _ = writeln!(
            out,
            "| {class} | {} | {} | {} |",
            cell(&other),
            cell(&ours),
            verdict(&ours, &other)
        );
    }
    let _ = writeln!(
        out,
        "\n### Against LibreOffice, on ODT files LibreOffice made\n"
    );
    let _ = writeln!(out, "Each corpus document converted to ODT by LibreOffice, then laid out by LibreOffice (the reference), by Word, and by Letters (LibreOffice flavor) from the ODT.\n");
    let _ = writeln!(out, "| class | Word vs LibreOffice (ODT) | Letters (LibreOffice flavor) vs LibreOffice (ODT) | beats Word? |");
    let _ = writeln!(out, "|---|---|---|---|");
    for class in classes(documents) {
        let in_class = || documents.iter().filter(|d| d.class == class);
        let other = combined(in_class(), |d| &d.odt_word_vs_libreoffice);
        let ours = combined(in_class(), |d| &d.odt_letters_vs_libreoffice);
        if other.is_none() && ours.is_none() {
            continue;
        }
        let _ = writeln!(
            out,
            "| {class} | {} | {} | {} |",
            cell(&other),
            cell(&ours),
            verdict(&ours, &other)
        );
    }
    let all = || documents.iter();
    let _ = writeln!(out, "\n### All documents together\n");
    let _ = writeln!(out, "| comparison | page breaks | same lines | baseline mean / max | line start mean / max | glyph x in line mean / p95 / max |");
    let _ = writeln!(out, "|---|---|---|---|---|---|");
    let rows: [(&str, Pick); 6] = [
        ("LibreOffice vs Word", |d| &d.libreoffice_vs_word),
        ("Letters (Word) vs Word", |d| &d.letters_vs_word),
        ("Word vs LibreOffice", |d| &d.word_vs_libreoffice),
        ("Letters (LibreOffice) vs LibreOffice", |d| {
            &d.letters_vs_libreoffice
        }),
        ("ODT: Word vs LibreOffice", |d| &d.odt_word_vs_libreoffice),
        ("ODT: Letters (LibreOffice) vs LibreOffice", |d| {
            &d.odt_letters_vs_libreoffice
        }),
    ];
    for (name, pick) in rows {
        if let Some(s) = combined(all(), pick) {
            let _ = writeln!(
                out,
                "| {name} | {} | {} ({} of {}) | {:.2} / {:.2} | {:.2} / {:.2} | {:.3} / {:.3} / {:.2} |",
                percent(s.page_break_share()),
                percent(s.same_line_share()),
                s.same_lines,
                s.reference_lines,
                s.baseline.mean,
                s.baseline.max,
                s.line_x.mean,
                s.line_x.max,
                s.glyph_x.mean,
                s.glyph_x.p95,
                s.glyph_x.max
            );
        }
    }
    let renders = documents.iter().any(|d| d.render_vs_word.is_some());
    if renders {
        let _ = writeln!(
            out,
            "\n### Rendering, 300 dpi (SSIM / share of pixels that differ)\n"
        );
        let _ = writeln!(
            out,
            "| document | LibreOffice vs Word | Letters vs Word | Letters vs LibreOffice |"
        );
        let _ = writeln!(out, "|---|---|---|---|");
        let show = |value: &Option<(f64, f64)>| {
            value.map_or("-".to_string(), |(ssim, diff)| {
                format!("{ssim:.3} / {:.2}%", diff * 100.0)
            })
        };
        for d in documents {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                d.name,
                show(&d.render_libreoffice_vs_word),
                show(&d.render_vs_word),
                show(&d.render_vs_libreoffice)
            );
        }
    }
    for (title, pick_other, pick_ours, other_name) in [
        (
            "Word",
            (|d| &d.libreoffice_vs_word) as fn(&DocumentScores) -> &Option<Score>,
            (|d| &d.letters_vs_word) as fn(&DocumentScores) -> &Option<Score>,
            "LibreOffice",
        ),
        (
            "LibreOffice",
            |d| &d.word_vs_libreoffice,
            |d| &d.letters_vs_libreoffice,
            "Word",
        ),
    ] {
        let _ = writeln!(out, "\n## Per document, against {title}\n");
        let _ = writeln!(out, "| document | pages (oracle / Letters / {other_name}) | Letters: breaks / same lines / baseline mean, max / start mean, max / glyph x mean | {other_name}: same lines / baseline mean |");
        let _ = writeln!(out, "|---|---|---|---|");
        for d in documents {
            let ours = pick_ours(d);
            let other = pick_other(d);
            let pages = format!(
                "{} / {} / {}",
                ours.as_ref().map_or(0, |s| s.reference_pages),
                ours.as_ref().map_or(0, |s| s.candidate_pages),
                other.as_ref().map_or(0, |s| s.candidate_pages)
            );
            let ours_text = ours.as_ref().map_or("-".into(), |s| {
                format!(
                    "{} / {} / {:.2}, {:.2} / {:.2}, {:.2} / {:.3}",
                    percent(s.page_break_share()),
                    percent(s.same_line_share()),
                    s.baseline.mean,
                    s.baseline.max,
                    s.line_x.mean,
                    s.line_x.max,
                    s.glyph_x.mean
                )
            });
            let other_text = other.as_ref().map_or("-".into(), |s| {
                format!("{} / {:.2}", percent(s.same_line_share()), s.baseline.mean)
            });
            let _ = writeln!(out, "| {} | {pages} | {ours_text} | {other_text} |", d.name);
        }
    }
    let _ = writeln!(out, "\n## Layout time\n");
    let _ = writeln!(out, "| document | our layout (ms, both flavors, debug-independent: release build when run by run.ps1) |");
    let _ = writeln!(out, "|---|---|");
    for d in documents {
        let _ = writeln!(out, "| {} | {:.1} |", d.name, d.layout_ms);
    }
    let _ = writeln!(out, "\n## Progress across iterations\n");
    let _ = writeln!(out, "Letters vs Word same lines %, mean baseline difference (pt); Letters vs LibreOffice same lines %; page breaks vs Word %.\n");
    let class_names = classes(documents);
    let _ = write!(out, "| iteration |");
    for class in &class_names {
        let _ = write!(out, " {class} |");
    }
    let _ = writeln!(out);
    let _ = write!(out, "|---|");
    for _ in &class_names {
        let _ = write!(out, "---|");
    }
    let _ = writeln!(out);
    for entry in history {
        let _ = write!(out, "| {} |", entry.iteration);
        for class in &class_names {
            match entry.classes.get(class) {
                Some((same, baseline, same_lo, breaks)) => {
                    let _ = write!(
                        out,
                        " {:.1}% / {baseline:.2} / {:.1}% / {:.0}% |",
                        same * 100.0,
                        same_lo * 100.0,
                        breaks * 100.0
                    );
                }
                None => {
                    let _ = write!(out, " - |");
                }
            }
        }
        let _ = writeln!(out);
    }
    let _ = writeln!(out, "\n{analysis}");
    out
}

/// the headline numbers of this run, for the history.
pub fn history_entry(iteration: &str, documents: &[DocumentScores]) -> HistoryEntry {
    let mut entry = HistoryEntry {
        iteration: iteration.to_string(),
        classes: BTreeMap::new(),
    };
    for class in classes(documents) {
        let in_class = || documents.iter().filter(|d| d.class == class);
        let ours = combined(in_class(), |d| &d.letters_vs_word);
        let ours_lo = combined(in_class(), |d| &d.letters_vs_libreoffice);
        if let Some(ours) = ours {
            entry.classes.insert(
                class,
                (
                    ours.same_line_share(),
                    ours.baseline.mean,
                    ours_lo.map_or(0.0, |s| s.same_line_share()),
                    ours.page_break_share(),
                ),
            );
        }
    }
    entry
}
