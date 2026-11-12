//! the report as plain text (for a summary or a copy to the clipboard)
//! and as an html page that is itself accessible: real headings and
//! lists, a language, and severity said in words, never only by colour.

use crate::report::{Finding, Location, Report, RuleStatus, Severity};

const SEVERITIES: [Severity; 3] = [
    Severity::MustFix,
    Severity::ShouldFix,
    Severity::CheckByHand,
];

/// "page 3", "page 3, in Document › Figure 2", or "the whole document".
fn place(location: &Location) -> String {
    let mut parts = Vec::new();
    if let Some(page) = location.page {
        parts.push(format!("page {}", page + 1));
    }
    if let Some(path) = &location.structure_path {
        parts.push(format!("in the tags at {path}"));
    }
    if parts.is_empty() {
        "the whole document".to_owned()
    } else {
        parts.join(", ")
    }
}

fn opening(report: &Report) -> String {
    let summary = &report.summary;
    if summary.must_fix == 0 && summary.should_fix == 0 {
        return format!(
            "No problems a program can find. {} things still need a person to check.",
            summary.check_by_hand
        );
    }
    format!(
        "{} must be fixed, {} should be fixed, and {} need checking by hand. {} of them Letters can fix for you or with one answer from you.",
        summary.must_fix, summary.should_fix, summary.check_by_hand, summary.fixable
    )
}

/// a plain-language summary of the report.
pub fn report_text(report: &Report) -> String {
    let mut text = String::from("Accessibility check\n\n");
    text.push_str(&opening(report));
    text.push_str(&format!(
        "\nScore: {}%. {}\n",
        report.score.percent, report.score.statement
    ));
    for severity in SEVERITIES {
        let findings: Vec<&Finding> = report
            .findings
            .iter()
            .filter(|f| f.severity == severity)
            .collect();
        if findings.is_empty() {
            continue;
        }
        text.push_str(&format!("\n{} ({})\n", severity.label(), findings.len()));
        for (number, finding) in findings.iter().enumerate() {
            text.push_str(&format!(
                "{}. {} ({})\n",
                number + 1,
                finding.title,
                place(&finding.location)
            ));
            if !finding.details.is_empty() {
                text.push_str(&format!("   {}\n", finding.details));
            }
            text.push_str(&format!("   Why it matters: {}\n", finding.why_it_matters));
            text.push_str(&format!("   What to do: {}\n", finding.fix_description));
        }
    }
    let not_checked: Vec<_> = report
        .rules
        .iter()
        .filter(|r| r.status == RuleStatus::NotChecked)
        .collect();
    if !not_checked.is_empty() {
        text.push_str("\nNot checked\n");
        for rule in not_checked {
            text.push_str(&format!(
                "- {}: {}\n",
                rule.title,
                rule.reason.as_deref().unwrap_or("")
            ));
        }
    }
    text
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// the report as a complete, accessible html page.
pub fn report_html(report: &Report) -> String {
    let mut html = String::from(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Accessibility report</title>\n<style>\n\
         body { font-family: system-ui, sans-serif; line-height: 1.5; max-width: 46rem; margin: 2rem auto; padding: 0 1rem; color: #1a1a1a; background: #fff; }\n\
         h2 { border-bottom: 2px solid #1a1a1a; }\n\
         li { margin-bottom: 1rem; }\n\
         .label { font-weight: bold; }\n\
         </style>\n</head>\n<body>\n<main>\n<h1>Accessibility report</h1>\n",
    );
    html.push_str(&format!("<p>{}</p>\n", escape(&opening(report))));
    html.push_str(&format!(
        "<p><span class=\"label\">Score:</span> {}%. {}</p>\n",
        report.score.percent,
        escape(&report.score.statement)
    ));
    for severity in SEVERITIES {
        let findings: Vec<&Finding> = report
            .findings
            .iter()
            .filter(|f| f.severity == severity)
            .collect();
        if findings.is_empty() {
            continue;
        }
        html.push_str(&format!(
            "<h2>{} ({})</h2>\n<ol>\n",
            severity.label(),
            findings.len()
        ));
        for finding in findings {
            html.push_str("<li>\n");
            html.push_str(&format!("<h3>{}</h3>\n", escape(&finding.title)));
            html.push_str(&format!(
                "<p><span class=\"label\">Where:</span> {}</p>\n",
                escape(&place(&finding.location))
            ));
            if !finding.details.is_empty() {
                html.push_str(&format!("<p>{}</p>\n", escape(&finding.details)));
            }
            html.push_str(&format!("<p>{}</p>\n", escape(&finding.explanation)));
            html.push_str(&format!(
                "<p><span class=\"label\">Why it matters:</span> {}</p>\n",
                escape(&finding.why_it_matters)
            ));
            html.push_str(&format!(
                "<p><span class=\"label\">What to do:</span> {}</p>\n",
                escape(&finding.fix_description)
            ));
            html.push_str(&format!(
                "<p><span class=\"label\">Standard:</span> {}</p>\n",
                escape(&finding.pdfua)
            ));
            html.push_str("</li>\n");
        }
        html.push_str("</ol>\n");
    }
    let not_checked: Vec<_> = report
        .rules
        .iter()
        .filter(|r| r.status == RuleStatus::NotChecked)
        .collect();
    if !not_checked.is_empty() {
        html.push_str("<h2>Not checked</h2>\n<ul>\n");
        for rule in not_checked {
            html.push_str(&format!(
                "<li>{}: {}</li>\n",
                escape(&rule.title),
                escape(rule.reason.as_deref().unwrap_or(""))
            ));
        }
        html.push_str("</ul>\n");
    }
    html.push_str("</main>\n</body>\n</html>\n");
    html
}
