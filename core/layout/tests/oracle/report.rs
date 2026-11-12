//! writes ORACLE_REPORT.md from the scores of a run, and keeps a history
//! of the headline numbers so regressions between iterations show.

use std::collections::BTreeMap;
use std::fmt::Write;

use super::score::{combine, Score};

/// one document's comparisons.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DocumentScores {
    pub name: String,
    pub class: String,
    /// never used while fitting rules
    #[serde(default)]
    pub held_out: bool,
    /// "docx" or "odt": the file every application laid out
    #[serde(default)]
    pub format: String,
    pub libreoffice_vs_word: Option<Score>,
    pub letters_vs_word: Option<Score>,
    pub word_vs_libreoffice: Option<Score>,
    pub letters_vs_libreoffice: Option<Score>,
    /// our layouts with only the open fonts that come with letters (no
    /// microsoft fonts): what a computer without office sees
    #[serde(default)]
    pub letters_vs_word_open_fonts: Option<Score>,
    #[serde(default)]
    pub letters_vs_libreoffice_open_fonts: Option<Score>,
    pub layout_ms: f64,
    /// render comparison of our pdf against each oracle's: (ssim, % pixels differing)
    #[serde(default)]
    pub render_vs_word: Option<(f64, f64)>,
    #[serde(default)]
    pub render_vs_libreoffice: Option<(f64, f64)>,
    #[serde(default)]
    pub render_libreoffice_vs_word: Option<(f64, f64)>,
    #[serde(default)]
    pub render_word_vs_libreoffice: Option<(f64, f64)>,
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
    /// set and comparison ("fit: letters vs word") -> (page breaks %, same
    /// lines %, mean baseline, documents)
    #[serde(default)]
    pub totals: BTreeMap<String, (f64, f64, f64, f64)>,
}

fn percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

/// picks one comparison out of a document's scores.
pub type Pick = fn(&DocumentScores) -> &Option<Score>;

fn classes(documents: &[DocumentScores]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for document in documents {
        if !seen.contains(&document.class) {
            seen.push(document.class.clone());
        }
    }
    seen
}

fn combined<'a>(documents: impl Iterator<Item = &'a DocumentScores>, pick: Pick) -> Option<Score> {
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
                "yes (equal lines, closer baselines)"
            } else {
                "not yet"
            }
        }
        _ => "-",
    }
}

/// a compact cell: page breaks | same lines | baseline mean / p95.
fn cell(score: &Option<Score>) -> String {
    match score {
        Some(s) => format!(
            "{} / {} / {:.2}, {:.2}",
            percent(s.page_break_share()),
            percent(s.same_line_share()),
            s.baseline.mean,
            s.baseline.p95
        ),
        None => "-".into(),
    }
}

/// the sets of documents a table can be about.
#[derive(Clone, Copy, PartialEq)]
enum Set {
    Fit,
    HeldOut,
    All,
}

impl Set {
    fn name(self) -> &'static str {
        match self {
            Set::Fit => "fit",
            Set::HeldOut => "held-out",
            Set::All => "all",
        }
    }

    fn has(self, document: &DocumentScores) -> bool {
        match self {
            Set::Fit => !document.held_out,
            Set::HeldOut => document.held_out,
            Set::All => true,
        }
    }
}

/// every comparison of the headline table, grouped by reference.
const HEADLINE: [(&str, &str, Pick); 8] = [
    ("Word (DOCX)", "LibreOffice vs Word", |d| {
        &d.libreoffice_vs_word
    }),
    ("Word (DOCX)", "**Letters (Word flavor) vs Word**", |d| {
        &d.letters_vs_word
    }),
    ("Word (DOCX)", "Letters, open fonts only, vs Word", |d| {
        &d.letters_vs_word_open_fonts
    }),
    ("LibreOffice", "Word vs LibreOffice", |d| {
        &d.word_vs_libreoffice
    }),
    (
        "LibreOffice",
        "**Letters (LibreOffice flavor) vs LibreOffice**",
        |d| &d.letters_vs_libreoffice,
    ),
    (
        "LibreOffice",
        "Letters, open fonts only, vs LibreOffice",
        |d| &d.letters_vs_libreoffice_open_fonts,
    ),
    (
        "LibreOffice (ODT LibreOffice made)",
        "Word vs LibreOffice",
        |d| &d.odt_word_vs_libreoffice,
    ),
    (
        "LibreOffice (ODT LibreOffice made)",
        "**Letters (LibreOffice flavor) vs LibreOffice**",
        |d| &d.odt_letters_vs_libreoffice,
    ),
];

fn headline_row(name: &str, score: &Score) -> String {
    format!(
        "| {name} | {} of {} | {} | {} ({} of {}) | {:.2} / {:.2} / {:.1} | {:.2} / {:.2} / {:.1} | {:.2} / {:.2} ({}) | {:.2} / {:.1} ({}) |",
        score.pages_match,
        score.documents,
        percent(score.page_break_share()),
        percent(score.same_line_share()),
        score.same_lines,
        score.reference_lines,
        score.baseline.mean,
        score.baseline.p95,
        score.baseline.max,
        score.line_x.mean,
        score.line_x.p95,
        score.line_x.max,
        score.rules.mean,
        score.rules.p95,
        score.rules.count,
        score.pictures.mean,
        score.pictures.max,
        score.pictures.count,
    )
}

fn write_headline(out: &mut String, documents: &[DocumentScores], set: Set) {
    let in_set: Vec<&DocumentScores> = documents.iter().filter(|d| set.has(d)).collect();
    let _ = writeln!(
        out,
        "\n### {} set ({} documents)\n",
        match set {
            Set::Fit => "Fit",
            Set::HeldOut => "Held-out",
            Set::All => "Whole",
        },
        in_set.len()
    );
    let _ = writeln!(out, "| reference / comparison | same page count | page breaks | same lines | baseline mean / p95 / max (pt) | line start mean / p95 / max (pt) | vertical rules mean / p95 (count) | pictures mean / max (count) |");
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
    let mut last_group = "";
    for (group, name, pick) in HEADLINE {
        if group != last_group {
            let _ = writeln!(out, "| *against {group}* | | | | | | | |");
            last_group = group;
        }
        if let Some(score) = combined(in_set.iter().copied(), pick) {
            let _ = writeln!(out, "{}", headline_row(name, &score));
        }
    }
}

fn write_class_table(
    out: &mut String,
    documents: &[DocumentScores],
    title: &str,
    other_name: &str,
    picks: (Pick, Pick, Option<Pick>),
) {
    let (other_pick, ours_pick, open_pick) = picks;
    let _ = writeln!(out, "\n### {title}\n");
    let _ = writeln!(
        out,
        "Each cell: page breaks / same lines / baseline mean, p95 (pt).\n"
    );
    let open_header = if open_pick.is_some() {
        " Letters, open fonts only |"
    } else {
        ""
    };
    let _ = writeln!(
        out,
        "| class | set | docs | {other_name} (the baseline to beat) | Letters |{open_header} closer than {other_name}? |"
    );
    let _ = writeln!(
        out,
        "|---|---|---|---|---|{}---|",
        if open_pick.is_some() { "---|" } else { "" }
    );
    for class in classes(documents) {
        for set in [Set::Fit, Set::HeldOut] {
            let in_class = || documents.iter().filter(|d| d.class == class && set.has(d));
            let count = in_class().count();
            let other = combined(in_class(), other_pick);
            let ours = combined(in_class(), ours_pick);
            if other.is_none() && ours.is_none() {
                continue;
            }
            let open = open_pick.map_or(String::new(), |pick| {
                format!(" {} |", cell(&combined(in_class(), pick)))
            });
            let _ = writeln!(
                out,
                "| {class} | {} | {count} | {} | {} |{open} {} |",
                set.name(),
                cell(&other),
                cell(&ours),
                verdict(&ours, &other)
            );
        }
    }
}

pub fn write_report(
    documents: &[DocumentScores],
    history: &[HistoryEntry],
    header: &str,
    analysis: &str,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Oracle report\n");
    let _ = writeln!(out, "{header}\n");
    let _ = writeln!(out, "## Headline\n");
    let _ = writeln!(
        out,
        "\"Same lines\": lines of the reference that the other layout breaks at the same words. \
         \"Page breaks\": pages whose first and last body text agree. Baselines and line starts are \
         compared on lines that match and sit on the same page. Vertical rules are table column \
         edges, page borders and column separators; pictures compare their top-left corners. \
         The other application on the same file is the baseline to beat. \"Open fonts only\" is our \
         engine with only the open fonts that come with Letters (vendor/fonts), what a computer \
         without Microsoft Office sees."
    );
    for set in [Set::Fit, Set::HeldOut, Set::All] {
        write_headline(&mut out, documents, set);
    }
    let _ = writeln!(out, "\n## Per class\n");
    write_class_table(
        &mut out,
        documents,
        "Against Word",
        "LibreOffice",
        (
            |d| &d.libreoffice_vs_word,
            |d| &d.letters_vs_word,
            Some(|d| &d.letters_vs_word_open_fonts),
        ),
    );
    write_class_table(
        &mut out,
        documents,
        "Against LibreOffice",
        "Word",
        (
            |d| &d.word_vs_libreoffice,
            |d| &d.letters_vs_libreoffice,
            Some(|d| &d.letters_vs_libreoffice_open_fonts),
        ),
    );
    write_class_table(
        &mut out,
        documents,
        "Against LibreOffice, on ODT files LibreOffice made from each DOCX",
        "Word",
        (
            |d| &d.odt_word_vs_libreoffice,
            |d| &d.odt_letters_vs_libreoffice,
            None,
        ),
    );
    write_rendering(&mut out, documents);
    write_per_document(&mut out, documents);
    let _ = writeln!(out, "\n## Layout time\n");
    let _ = writeln!(
        out,
        "Our layout of each document, both flavors with system fonts (release build).\n"
    );
    let _ = writeln!(out, "| document | ms |");
    let _ = writeln!(out, "|---|---|");
    for d in documents {
        let _ = writeln!(out, "| {} | {:.1} |", d.name, d.layout_ms);
    }
    write_history(&mut out, documents, history);
    let _ = writeln!(out, "\n{analysis}");
    out
}

fn write_rendering(out: &mut String, documents: &[DocumentScores]) {
    if !documents.iter().any(|d| d.render_vs_word.is_some()) {
        return;
    }
    let _ = writeln!(out, "\n## Rendering at 150 dpi\n");
    let _ = writeln!(
        out,
        "Mean SSIM over the pages both have / share of pixels differing by more than a quarter of the grey range.\n"
    );
    let show = |value: &Option<(f64, f64)>| {
        value.map_or("-".to_string(), |(ssim, diff)| {
            format!("{ssim:.3} / {:.2}%", diff * 100.0)
        })
    };
    let mean = |pick: fn(&DocumentScores) -> Option<(f64, f64)>, set: Set| {
        let values: Vec<(f64, f64)> = documents
            .iter()
            .filter(|d| set.has(d))
            .filter_map(pick)
            .collect();
        (!values.is_empty()).then(|| {
            let n = values.len() as f64;
            (
                values.iter().map(|v| v.0).sum::<f64>() / n,
                values.iter().map(|v| v.1).sum::<f64>() / n,
            )
        })
    };
    let _ = writeln!(out, "| set | LibreOffice vs Word | Letters vs Word | Word vs LibreOffice | Letters vs LibreOffice |");
    let _ = writeln!(out, "|---|---|---|---|---|");
    for set in [Set::Fit, Set::HeldOut, Set::All] {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            set.name(),
            show(&mean(|d| d.render_libreoffice_vs_word, set)),
            show(&mean(|d| d.render_vs_word, set)),
            show(&mean(|d| d.render_word_vs_libreoffice, set)),
            show(&mean(|d| d.render_vs_libreoffice, set)),
        );
    }
    let _ = writeln!(out, "\n| document | LibreOffice vs Word | Letters vs Word | Word vs LibreOffice | Letters vs LibreOffice |");
    let _ = writeln!(out, "|---|---|---|---|---|");
    for d in documents {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            d.name,
            show(&d.render_libreoffice_vs_word),
            show(&d.render_vs_word),
            show(&d.render_word_vs_libreoffice),
            show(&d.render_vs_libreoffice)
        );
    }
}

fn write_per_document(out: &mut String, documents: &[DocumentScores]) {
    for (title, pick_other, pick_ours, other_name) in [
        (
            "Word",
            (|d| &d.libreoffice_vs_word) as Pick,
            (|d| &d.letters_vs_word) as Pick,
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
        let _ = writeln!(out, "| document | set | pages (oracle / Letters / {other_name}) | Letters: breaks / same lines / baseline mean, p95, max / start mean, p95 / rules mean / pictures max | {other_name}: breaks / same lines / baseline mean |");
        let _ = writeln!(out, "|---|---|---|---|---|");
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
                    "{} / {} / {:.2}, {:.2}, {:.1} / {:.2}, {:.2} / {:.2} / {:.1}",
                    percent(s.page_break_share()),
                    percent(s.same_line_share()),
                    s.baseline.mean,
                    s.baseline.p95,
                    s.baseline.max,
                    s.line_x.mean,
                    s.line_x.p95,
                    s.rules.mean,
                    s.pictures.max
                )
            });
            let other_text = other.as_ref().map_or("-".into(), |s| {
                format!(
                    "{} / {} / {:.2}",
                    percent(s.page_break_share()),
                    percent(s.same_line_share()),
                    s.baseline.mean
                )
            });
            let set = if d.held_out { "held-out" } else { "fit" };
            let _ = writeln!(
                out,
                "| {} | {set} | {pages} | {ours_text} | {other_text} |",
                d.name
            );
        }
    }
}

fn write_history(out: &mut String, documents: &[DocumentScores], history: &[HistoryEntry]) {
    let _ = writeln!(out, "\n## Progress across iterations\n");
    let _ = writeln!(out, "Totals: page breaks % / same lines % / mean baseline (pt) / documents. Earlier iterations had only the first 30 documents (all fit).\n");
    let keys: Vec<String> = {
        let mut keys: Vec<String> = history
            .iter()
            .flat_map(|entry| entry.totals.keys().cloned())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    };
    if !keys.is_empty() {
        let _ = write!(out, "| iteration |");
        for key in &keys {
            let _ = write!(out, " {key} |");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "|---|{}", "---|".repeat(keys.len()));
        for entry in history {
            let _ = write!(out, "| {} |", entry.iteration);
            for key in &keys {
                match entry.totals.get(key) {
                    Some((breaks, same, baseline, count)) => {
                        let _ = write!(
                            out,
                            " {:.1} / {:.1} / {baseline:.2} / {count} |",
                            breaks * 100.0,
                            same * 100.0
                        );
                    }
                    None => {
                        let _ = write!(out, " - |");
                    }
                }
            }
            let _ = writeln!(out);
        }
    }
    let _ = writeln!(out, "\nPer class: Letters vs Word same lines % / mean baseline (pt) / Letters vs LibreOffice same lines % / page breaks vs Word %.\n");
    let class_names = classes(documents);
    let _ = write!(out, "| iteration |");
    for class in &class_names {
        let _ = write!(out, " {class} |");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "|---|{}", "---|".repeat(class_names.len()));
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
}

/// the headline numbers of this run, for the history.
pub fn history_entry(iteration: &str, documents: &[DocumentScores]) -> HistoryEntry {
    let mut entry = HistoryEntry {
        iteration: iteration.to_string(),
        ..HistoryEntry::default()
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
    let totals: [(&str, Pick); 7] = [
        ("letters vs word", |d| &d.letters_vs_word),
        ("libreoffice vs word", |d| &d.libreoffice_vs_word),
        ("letters vs libreoffice", |d| &d.letters_vs_libreoffice),
        ("word vs libreoffice", |d| &d.word_vs_libreoffice),
        ("odt: letters vs libreoffice", |d| {
            &d.odt_letters_vs_libreoffice
        }),
        ("open fonts: letters vs word", |d| {
            &d.letters_vs_word_open_fonts
        }),
        ("open fonts: letters vs libreoffice", |d| {
            &d.letters_vs_libreoffice_open_fonts
        }),
    ];
    for set in [Set::Fit, Set::HeldOut] {
        for (name, pick) in totals {
            if let Some(score) = combined(documents.iter().filter(|d| set.has(d)), pick) {
                entry.totals.insert(
                    format!("{}: {name}", set.name()),
                    (
                        score.page_break_share(),
                        score.same_line_share(),
                        score.baseline.mean,
                        score.documents as f64,
                    ),
                );
            }
        }
    }
    entry
}
