//! the comparison report, as pdf, plain text or html.
//!
//! all three say the same things in the same order, built once here:
//! a title, the two versions, a summary table of counts, then for each page
//! pair with changes a heading and its changes grouped by category, one
//! plain sentence each. the pdf adds thumbnails of both pages with the
//! changed areas marked (see pdf.rs).

mod pdf;
mod winansi;

use serde::{Deserialize, Serialize};

use crate::result::{Category, Change, ComparisonResult, PagePair};

pub use pdf::report_pdf;

/// what the report says about the documents. the ui fills in names and
/// dates; the crate itself never reads the clock, so reports are repeatable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReportOptions {
    pub title: String,
    /// the old version's file name, as people know it
    pub old_name: String,
    pub new_name: String,
    /// when each version was last changed, already worded ("3 November 2026")
    pub old_date: Option<String>,
    pub new_date: Option<String>,
    /// when the comparison was made, already worded
    pub made_on: Option<String>,
    /// put pictures of both pages, with the changes marked, in the pdf
    pub thumbnails: bool,
    /// the report's language, as a language tag ("en", "en-GB")
    pub language: String,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            title: "Comparison report".to_owned(),
            old_name: "Old version".to_owned(),
            new_name: "New version".to_owned(),
            old_date: None,
            new_date: None,
            made_on: None,
            thumbnails: true,
            language: "en".to_owned(),
        }
    }
}

/// one page pair's part of the report.
pub(crate) struct Section<'a> {
    pub heading: String,
    pub pair: &'a PagePair,
    /// changes by category, in the summary's order; empty categories left out
    pub groups: Vec<(Category, Vec<&'a Change>)>,
}

/// the lines that describe the two versions, under the title.
pub(crate) fn version_lines(options: &ReportOptions) -> Vec<String> {
    let line = |label: &str, name: &str, date: &Option<String>| match date {
        Some(date) => format!("{label}: {name} ({date})"),
        None => format!("{label}: {name}"),
    };
    let mut lines = vec![
        line("Old version", &options.old_name, &options.old_date),
        line("New version", &options.new_name, &options.new_date),
    ];
    if let Some(made_on) = &options.made_on {
        lines.push(format!("Compared on {made_on}"));
    }
    lines
}

/// the rows of the summary table, the total last.
pub(crate) fn summary_rows(result: &ComparisonResult) -> Vec<(String, u32)> {
    let mut rows: Vec<(String, u32)> = Category::ALL
        .iter()
        .map(|category| (category.label().to_owned(), result.summary.count(*category)))
        .collect();
    rows.push(("Total".to_owned(), result.summary.total));
    rows
}

/// the sentences under the summary table.
pub(crate) fn summary_sentences(result: &ComparisonResult) -> Vec<String> {
    let summary = &result.summary;
    let mut sentences = vec![if summary.total == 0 {
        "The two versions are the same: no changes were found.".to_owned()
    } else {
        format!(
            "{} of {} page {} changes.",
            summary.page_pairs_with_changes,
            summary.page_pairs,
            if summary.page_pairs == 1 {
                "pair has"
            } else {
                "pairs have"
            }
        )
    }];
    let unchanged = unchanged_pages(result);
    if !unchanged.is_empty() && summary.total > 0 {
        sentences.push(format!("No changes on {unchanged}."));
    }
    if summary.ignored > 0 {
        sentences.push(format!(
            "{} {} only in spacing or hyphenation {} not listed.",
            summary.ignored,
            if summary.ignored == 1 {
                "difference"
            } else {
                "differences"
            },
            if summary.ignored == 1 { "is" } else { "are" }
        ));
    }
    sentences.extend(
        result
            .notes
            .iter()
            .filter(|note| !note.starts_with("Differences only in spacing"))
            .cloned(),
    );
    sentences
}

/// "new pages 1–3 and 5", the unchanged pages counted on the new version.
fn unchanged_pages(result: &ComparisonResult) -> String {
    let pages: Vec<u32> = result
        .page_pairs
        .iter()
        .filter(|pair| !pair.has_changes())
        .filter_map(|pair| pair.right_page.map(|page| page + 1))
        .collect();
    if pages.is_empty() {
        return String::new();
    }
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for page in pages {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == page => *end = page,
            _ => runs.push((page, page)),
        }
    }
    let parts: Vec<String> = runs
        .iter()
        .map(|(start, end)| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}–{end}")
            }
        })
        .collect();
    let single = runs.len() == 1 && runs[0].0 == runs[0].1;
    format!(
        "new {} {}",
        if single { "page" } else { "pages" },
        crate::sentences::join_parts(&parts)
    )
}

/// the heading of a page pair's section.
pub(crate) fn pair_heading(pair: &PagePair) -> String {
    match (pair.left_page, pair.right_page) {
        (Some(old), Some(new)) if old == new => format!("Page {}", new + 1),
        (Some(old), Some(new)) => format!("Old page {} and new page {}", old + 1, new + 1),
        (Some(old), None) => format!("Old page {} (removed)", old + 1),
        (None, Some(new)) => format!("New page {} (added)", new + 1),
        (None, None) => "Page".to_owned(),
    }
}

/// the sections of the report: page pairs with changes or notes.
pub(crate) fn sections(result: &ComparisonResult) -> Vec<Section<'_>> {
    result
        .page_pairs
        .iter()
        .filter(|pair| pair.has_changes() || !pair.notes.is_empty())
        .map(|pair| Section {
            heading: pair_heading(pair),
            pair,
            groups: Category::ALL
                .iter()
                .map(|category| {
                    let changes: Vec<&Change> = pair
                        .changes
                        .iter()
                        .filter(|change| change.category == *category)
                        .collect();
                    (*category, changes)
                })
                .filter(|(_, changes)| !changes.is_empty())
                .collect(),
        })
        .collect()
}

/// one change as a sentence with a full stop.
pub(crate) fn change_sentence(change: &Change) -> String {
    format!("{}.", change.detail)
}

/// the report as plain text, for copying into an email or a note.
pub fn report_text(result: &ComparisonResult, options: &ReportOptions) -> String {
    let mut out = String::new();
    out.push_str(&options.title);
    out.push('\n');
    for line in version_lines(options) {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("\nSummary\n");
    for (label, count) in summary_rows(result) {
        out.push_str(&format!("  {label}: {count}\n"));
    }
    for sentence in summary_sentences(result) {
        out.push_str(&sentence);
        out.push('\n');
    }
    for section in sections(result) {
        out.push('\n');
        out.push_str(&section.heading);
        out.push('\n');
        for note in &section.pair.notes {
            out.push_str(&format!("  {note}\n"));
        }
        for (category, changes) in &section.groups {
            out.push_str(&format!("  {}\n", category.label()));
            for change in changes {
                out.push_str(&format!("    - {}\n", change_sentence(change)));
            }
        }
    }
    out
}

/// the report as a small, self-contained html page with headings, a
/// summary table and lists, so screen readers can move around it.
pub fn report_html(result: &ComparisonResult, options: &ReportOptions) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n",
        escape(&options.language),
        escape(&options.title)
    ));
    out.push_str(&format!("<h1>{}</h1>\n", escape(&options.title)));
    for line in version_lines(options) {
        out.push_str(&format!("<p>{}</p>\n", escape(&line)));
    }
    out.push_str("<h2>Summary</h2>\n<table>\n<thead><tr><th scope=\"col\">What changed</th><th scope=\"col\">Changes</th></tr></thead>\n<tbody>\n");
    for (label, count) in summary_rows(result) {
        out.push_str(&format!(
            "<tr><th scope=\"row\">{}</th><td>{count}</td></tr>\n",
            escape(&label)
        ));
    }
    out.push_str("</tbody>\n</table>\n");
    for sentence in summary_sentences(result) {
        out.push_str(&format!("<p>{}</p>\n", escape(&sentence)));
    }
    for section in sections(result) {
        out.push_str(&format!("<h2>{}</h2>\n", escape(&section.heading)));
        for note in &section.pair.notes {
            out.push_str(&format!("<p>{}</p>\n", escape(note)));
        }
        for (category, changes) in &section.groups {
            out.push_str(&format!("<h3>{}</h3>\n<ul>\n", escape(category.label())));
            for change in changes {
                out.push_str(&format!(
                    "<li id=\"change-{}\">{}</li>\n",
                    change.id,
                    escape(&change_sentence(change))
                ));
            }
            out.push_str("</ul>\n");
        }
    }
    out.push_str("</body>\n</html>\n");
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::{ChangeKind, Summary};

    fn result_with_one_change() -> ComparisonResult {
        let change = Change {
            kind: ChangeKind::Replaced,
            category: Category::Text,
            detail: "Changed “fox” to “cat”".to_owned(),
            ..Change::default()
        };
        let pairs = vec![
            PagePair {
                left_page: Some(0),
                right_page: Some(0),
                changes: vec![change],
                ..PagePair::default()
            },
            PagePair {
                left_page: Some(1),
                right_page: Some(1),
                ..PagePair::default()
            },
            PagePair {
                left_page: Some(2),
                right_page: Some(2),
                ..PagePair::default()
            },
        ];
        ComparisonResult {
            summary: Summary::of(&pairs, 0),
            page_pairs: pairs,
            ..ComparisonResult::default()
        }
    }

    #[test]
    fn text_reports_list_counts_and_sentences() {
        let text = report_text(&result_with_one_change(), &ReportOptions::default());
        assert!(text.contains("Text: 1"), "{text}");
        assert!(text.contains("Total: 1"), "{text}");
        assert!(text.contains("Changed “fox” to “cat”."), "{text}");
        assert!(text.contains("No changes on new pages 2–3."), "{text}");
    }

    #[test]
    fn html_reports_are_escaped_and_have_a_language() {
        let options = ReportOptions {
            old_name: "a<b>.pdf".to_owned(),
            ..ReportOptions::default()
        };
        let html = report_html(&result_with_one_change(), &options);
        assert!(html.contains("<html lang=\"en\">"));
        assert!(html.contains("a&lt;b&gt;.pdf"));
        assert!(html.contains("<h3>Text</h3>"));
    }
}
