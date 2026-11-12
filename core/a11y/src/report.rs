//! the shape of a check's answer: findings, counts and an honest score.
//!
//! everything here is sent to the ui as json with camelCase names.

use serde::{Deserialize, Serialize};

use crate::colour::Rgb;
use crate::geometry::Rect;
use crate::objects::ObjectRef;

/// how urgent a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// breaks pdf/ua or blocks a screen reader user
    MustFix,
    /// makes the document harder to use; worth fixing
    ShouldFix,
    /// a person has to look; no program can decide it
    CheckByHand,
}

impl Severity {
    pub fn label(&self) -> &'static str {
        match self {
            Severity::MustFix => "Must fix",
            Severity::ShouldFix => "Should fix",
            Severity::CheckByHand => "Check by hand",
        }
    }
}

/// where a finding is: a page and a rectangle on it (measured on the
/// visible page from its top left, in points, y down), a place in the
/// tags, or the document as a whole when every field is empty.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// 0-based page index
    pub page: Option<u32>,
    pub rect: Option<Rect>,
    /// a path through the tags people can read, like "Document › Figure 2"
    pub structure_path: Option<String>,
    pub element: Option<ObjectRef>,
    pub annotation: Option<ObjectRef>,
}

impl Location {
    pub fn document() -> Self {
        Self::default()
    }

    pub fn page(page: u32, rect: Option<Rect>) -> Self {
        Self {
            page: Some(page),
            rect,
            ..Self::default()
        }
    }
}

/// a picture found in the page content, by its place in the content.
/// `opIndex` is the index of the operator that draws it, as the checker
/// counts them; it stays valid until the page's content changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentRef {
    pub page: u32,
    pub op_index: usize,
}

/// what a fix would work on: a structure element or a picture in the
/// page content that has no tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Element { element: ObjectRef },
    Image { image: ContentRef },
}

/// what can be done about a finding, automatically or with one answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FixKind {
    /// build tags for the whole document (`auto_tag`)
    AddTags,
    /// set the "this file is tagged" flag (`mark_as_tagged`)
    MarkAsTagged,
    /// ask for the language (`set_language`)
    SetLanguage { suggested: Option<String> },
    /// ask for or confirm a title (`set_title`)
    SetTitle { suggested: Option<String> },
    /// show the title instead of the file name (`show_title`)
    ShowTitle,
    /// ask for alt text, or "this is decoration" (`set_alt_text`, `mark_as_artifact`)
    SetAltText { target: Target },
    /// renumber headings so none are skipped (`fix_heading_levels`)
    FixHeadingLevels,
    /// choose header rows and columns (`set_table_headers`)
    SetTableHeaders {
        table: ObjectRef,
        suggested_header_rows: u32,
        suggested_header_columns: u32,
    },
    /// accept or adjust a proposed order (`set_reading_order`)
    SetReadingOrder { page: u32, proposal: Vec<ObjectRef> },
    /// ask for a tooltip (`set_field_tooltip`)
    SetFieldTooltip {
        field: ObjectRef,
        suggested: Option<String>,
    },
    /// make tab follow the tags (`set_tab_order`)
    SetTabOrder,
    /// ask for a description of a link (`add_link_text`)
    AddLinkText {
        annotation: ObjectRef,
        suggested: Option<String>,
    },
    /// ask for a description of a note or other annotation (`set_annotation_contents`)
    SetAnnotationContents { annotation: ObjectRef },
    /// give links, notes and fields their tags (`tag_annotations`)
    TagAnnotations,
    /// tag the untagged content of a page (`tag_untagged_content`)
    TagUntaggedContent { page: u32 },
    /// change these pieces of text to a colour that passes (`apply_text_colour`)
    ChangeTextColour {
        runs: Vec<ContentRef>,
        suggested: Rgb,
    },
    /// add bookmarks made from the headings (`add_bookmarks_from_headings`)
    AddBookmarksFromHeadings,
    /// run text recognition on these pages (the ocr tool, outside this crate)
    RecogniseText { pages: Vec<u32> },
}

/// the measured contrast behind a contrast finding.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContrastMeasurement {
    /// the lowest ratio among the text this finding covers
    pub ratio: f64,
    pub required: f64,
    pub text_colour: Rgb,
    pub background_colour: Rgb,
    /// the nearest colour that passes against this background
    pub suggested_colour: Rgb,
}

/// one problem, or one thing a person needs to check.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// a stable rule id like "missing-alt-text"
    pub id: String,
    pub severity: Severity,
    pub title: String,
    /// what the rule is about, in plain sentences
    pub explanation: String,
    /// why it matters to someone using a screen reader or similar
    pub why_it_matters: String,
    /// what the fix does to the file, or how to fix it by hand
    pub fix_description: String,
    /// what was found in this particular place
    pub details: String,
    #[serde(rename = "where")]
    pub location: Location,
    pub fix: Option<FixKind>,
    /// where pdf/ua (iso 14289-1) or wcag asks for this
    pub pdfua: String,
    pub contrast: Option<ContrastMeasurement>,
}

/// how many findings there are of each severity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub must_fix: usize,
    pub should_fix: usize,
    pub check_by_hand: usize,
    pub total: usize,
    /// findings that have an automatic or one-answer fix
    pub fixable: usize,
}

/// what happened to one rule during the check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleStatus {
    Passed,
    Failed,
    /// nothing in the document this rule applies to (no tables, no links…)
    NotApplicable,
    /// the rule could not be run; the reason says why
    NotChecked,
    /// a person has to decide; listed as "check by hand" findings
    ByHand,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleOutcome {
    pub id: String,
    pub title: String,
    pub status: RuleStatus,
    pub reason: Option<String>,
}

/// the score: how many of the machine-checkable rules passed, said
/// plainly together with what was not checked at all.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    /// 0 to 100: passed out of passed plus failed. rules that were not
    /// checked are left out rather than counted as passes
    pub percent: u32,
    pub passed: usize,
    pub failed: usize,
    pub not_applicable: usize,
    pub not_checked: usize,
    /// things only a person can judge
    pub by_hand: usize,
    /// one or two sentences that say what the number does and does not mean
    pub statement: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub page_count: u32,
    pub findings: Vec<Finding>,
    pub summary: Summary,
    pub score: Score,
    pub rules: Vec<RuleOutcome>,
}

impl Report {
    /// the findings with one rule id.
    pub fn with_id<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Finding> + 'a {
        self.findings.iter().filter(move |finding| finding.id == id)
    }

    pub fn has(&self, id: &str) -> bool {
        self.with_id(id).next().is_some()
    }

    /// true when no rule that a program can check left a "must fix".
    pub fn machine_checks_pass(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::MustFix)
            && !self.rules.iter().any(|rule| {
                rule.status == RuleStatus::NotChecked
                    && crate::rules::catalog::blocks_pdfua(&rule.id)
            })
    }
}

/// what a fix changed, in sentences for the person.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    pub changes: Vec<String>,
}

impl FixResult {
    pub fn one(change: impl Into<String>) -> Self {
        Self {
            changes: vec![change.into()],
        }
    }

    pub fn push(&mut self, change: impl Into<String>) {
        self.changes.push(change.into());
    }
}
