//! the checker: reads the document once, runs every rule family, and
//! turns what they found into a report.
//!
//! each family lives in its own module and only reads from the shared
//! `Context`, so a rule can be understood on its own.

pub mod catalog;

mod annotations;
mod content_tags;
mod contrast;
mod document;
mod figures;
mod fonts;
mod hand;
mod headings;
mod language;
mod lists;
mod reading_order;
mod tables;
mod tagging;
mod title;

pub(crate) use annotations::page_annotations;
pub(crate) use content_tags::plural;
pub(crate) use headings::short;
pub(crate) use language::is_valid_language_tag;
pub(crate) use lists::list_label;
pub(crate) use reading_order::{page_blocks, visual_order};
pub(crate) use tables::{row_cells, table_rows};

use std::collections::HashMap;

use lopdf::{Document, ObjectId};
use serde::{Deserialize, Serialize};

use crate::content::{read_page, Mark, PageContent};
use crate::error::A11yError;
use crate::geometry::{PageFrame, Rect};
use crate::objects::{ensure_readable, page_ids};
use crate::report::{
    Finding, FixKind, Location, Report, RuleOutcome, RuleStatus, Score, Severity, Summary,
};
use crate::structure::StructTree;
use catalog::{Checked, RULES};

/// pixels of part of a page, for measuring the colour behind text.
#[derive(Clone, Debug, PartialEq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// four bytes per pixel, rows from the top
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// the colour of one pixel, laid over white paper when it is see-through.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 3]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let start = ((y * self.width + x) * 4) as usize;
        let rgba = self.pixels.get(start..start + 4)?;
        let alpha = rgba[3] as u32;
        let over_white = |value: u8| ((value as u32 * alpha + 255 * (255 - alpha)) / 255) as u8;
        Some([
            over_white(rgba[0]),
            over_white(rgba[1]),
            over_white(rgba[2]),
        ])
    }
}

/// draws part of a page so the checker can see colours. the shell plugs
/// in the app's renderer; tests use pdfium through letters-pdf.
///
/// `rect` is measured on the visible page from its top left, in points;
/// `scale` is pixels per point. answer none when the page can't be drawn,
/// and the contrast rule will say it could not check that text.
pub trait RegionRenderer {
    fn render_region(&mut self, page: u32, rect: Rect, scale: f32) -> Option<RgbaImage>;
}

impl<F> RegionRenderer for F
where
    F: FnMut(u32, Rect, f32) -> Option<RgbaImage>,
{
    fn render_region(&mut self, page: u32, rect: Rect, scale: f32) -> Option<RgbaImage> {
        self(page, rect, scale)
    }
}

/// settings for a check. the defaults suit most documents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CheckOptions {
    /// documents with at least this many pages should have bookmarks
    pub bookmark_page_threshold: u32,
    /// pixels per point when measuring contrast
    pub contrast_scale: f32,
    /// at most this many pieces of text per page are measured for contrast
    pub contrast_runs_per_page: usize,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            bookmark_page_threshold: 10,
            contrast_scale: 2.0,
            contrast_runs_per_page: 400,
        }
    }
}

/// one page as the rules see it.
pub(crate) struct PageInfo {
    pub id: ObjectId,
    pub frame: PageFrame,
    /// none when the drawing instructions could not be read
    pub content: Option<PageContent>,
}

/// everything the rules share, read once.
pub(crate) struct Context<'a> {
    pub doc: &'a Document,
    pub pages: Vec<PageInfo>,
    pub page_index: HashMap<ObjectId, u32>,
    pub tree: Option<StructTree>,
    /// the document has a structure tree with at least one element
    pub tagged: bool,
    pub options: CheckOptions,
    /// where each piece of tagged content is: (page, mcid) → rectangle
    pub mcid_rects: HashMap<(u32, i64), Rect>,
    /// the text of each piece of tagged content
    pub mcid_text: HashMap<(u32, i64), String>,
}

impl<'a> Context<'a> {
    pub fn read(doc: &'a Document, options: CheckOptions) -> Result<Self, A11yError> {
        ensure_readable(doc)?;
        crate::objects::catalog(doc)?;
        let ids = page_ids(doc);
        let pages: Vec<PageInfo> = ids
            .iter()
            .map(|&id| {
                let frame = PageFrame::read(doc, id);
                let content = read_page(doc, id, &frame).ok();
                PageInfo { id, frame, content }
            })
            .collect();
        let page_index = ids
            .iter()
            .enumerate()
            .map(|(index, &id)| (id, index as u32))
            .collect();
        let tree = StructTree::read(doc);
        let tagged = tree.as_ref().is_some_and(|tree| !tree.elements.is_empty());
        let mut context = Context {
            doc,
            pages,
            page_index,
            tree,
            tagged,
            options,
            mcid_rects: HashMap::new(),
            mcid_text: HashMap::new(),
        };
        context.index_tagged_content();
        Ok(context)
    }

    fn index_tagged_content(&mut self) {
        for (page, info) in self.pages.iter().enumerate() {
            let Some(content) = &info.content else {
                continue;
            };
            let page = page as u32;
            for run in content.texts.iter().filter(|run| !run.in_form) {
                if let Mark::Mcid(mcid) = run.mark {
                    grow(&mut self.mcid_rects, (page, mcid), run.rect);
                    let text = self.mcid_text.entry((page, mcid)).or_default();
                    text.push_str(&run.text);
                }
            }
            for image in content.images.iter().filter(|image| !image.in_form) {
                if let Mark::Mcid(mcid) = image.mark {
                    grow(&mut self.mcid_rects, (page, mcid), image.rect);
                }
            }
        }
    }

    pub fn page_count(&self) -> u32 {
        self.pages.len() as u32
    }

    pub fn page_of(&self, page: Option<ObjectId>) -> Option<u32> {
        page.and_then(|id| self.page_index.get(&id).copied())
    }

    /// the content under an element, as (page index, mcid).
    pub fn element_content(&self, index: usize) -> Vec<(u32, i64)> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        tree.content_under(index)
            .into_iter()
            .filter_map(|(page, mcid)| Some((self.page_of(page)?, mcid)))
            .collect()
    }

    /// the text an element holds, as a screen reader would get it.
    pub fn element_text(&self, index: usize) -> String {
        let mut text = String::new();
        for key in self.element_content(index) {
            if let Some(piece) = self.mcid_text.get(&key) {
                if !text.is_empty() && !text.ends_with(' ') && !piece.starts_with(' ') {
                    text.push(' ');
                }
                text.push_str(piece);
            }
        }
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// where an element is: its page, the rectangle around its content on
    /// that page, and its path in the tags.
    pub fn element_location(&self, index: usize) -> Location {
        let Some(tree) = &self.tree else {
            return Location::document();
        };
        let content = self.element_content(index);
        let page = content
            .first()
            .map(|(page, _)| *page)
            .or_else(|| self.page_of(tree.elements[index].page));
        let rect = page.and_then(|page| {
            Rect::union_all(
                content
                    .iter()
                    .filter(|(p, _)| *p == page)
                    .filter_map(|key| self.mcid_rects.get(key)),
            )
        });
        Location {
            page,
            rect,
            structure_path: Some(tree.path(index)),
            element: tree.elements[index].id.map(Into::into),
            annotation: None,
        }
    }

    pub fn contents(&self) -> impl Iterator<Item = (u32, &PageContent)> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(page, info)| Some((page as u32, info.content.as_ref()?)))
    }
}

fn grow(map: &mut HashMap<(u32, i64), Rect>, key: (u32, i64), rect: Rect) {
    map.entry(key)
        .and_modify(|sum| *sum = sum.union(&rect))
        .or_insert(rect);
}

/// gathers findings and rule outcomes while the rules run.
#[derive(Default)]
pub(crate) struct Collector {
    findings: Vec<Finding>,
    outcomes: HashMap<&'static str, (RuleStatus, String)>,
}

impl Collector {
    /// adds a finding, taking its title and explanations from the catalog.
    pub fn report(
        &mut self,
        id: &'static str,
        details: impl Into<String>,
        location: Location,
        fix: Option<FixKind>,
    ) -> &mut Finding {
        let rule = catalog::info(id);
        self.findings.push(Finding {
            id: id.to_owned(),
            severity: rule.severity,
            title: rule.title.to_owned(),
            explanation: rule.explanation.to_owned(),
            why_it_matters: rule.why.to_owned(),
            fix_description: rule.fix.to_owned(),
            details: details.into(),
            location,
            fix,
            pdfua: rule.pdfua.to_owned(),
            contrast: None,
        });
        self.findings.last_mut().unwrap()
    }

    pub fn not_checked(&mut self, id: &'static str, reason: impl Into<String>) {
        self.outcomes
            .insert(id, (RuleStatus::NotChecked, reason.into()));
    }

    pub fn not_applicable(&mut self, id: &'static str, reason: impl Into<String>) {
        self.outcomes
            .entry(id)
            .or_insert((RuleStatus::NotApplicable, reason.into()));
    }

    fn into_report(self, page_count: u32) -> Report {
        let rules: Vec<RuleOutcome> = RULES
            .iter()
            .map(|rule| {
                let found = self.findings.iter().any(|f| f.id == rule.id);
                let (status, reason) = match (rule.checked, found, self.outcomes.get(rule.id)) {
                    (Checked::ByHand, true, _) => (RuleStatus::ByHand, None),
                    (Checked::ByHand, false, _) => (
                        RuleStatus::NotApplicable,
                        Some("Nothing in this document needs this check.".to_owned()),
                    ),
                    (Checked::Machine, true, _) => (RuleStatus::Failed, None),
                    (Checked::Machine, false, Some((status, reason))) => {
                        (*status, Some(reason.clone()))
                    }
                    (Checked::Machine, false, None) => (RuleStatus::Passed, None),
                };
                RuleOutcome {
                    id: rule.id.to_owned(),
                    title: rule.title.to_owned(),
                    status,
                    reason,
                }
            })
            .collect();
        let mut findings = self.findings;
        // the most urgent first, then by page, so the list reads top down
        findings.sort_by_key(|f| (f.severity, f.location.page.unwrap_or(0)));
        let summary = summarise(&findings);
        let score = score(&rules);
        Report {
            page_count,
            findings,
            summary,
            score,
            rules,
        }
    }
}

fn summarise(findings: &[Finding]) -> Summary {
    let count = |severity: Severity| findings.iter().filter(|f| f.severity == severity).count();
    Summary {
        must_fix: count(Severity::MustFix),
        should_fix: count(Severity::ShouldFix),
        check_by_hand: count(Severity::CheckByHand),
        total: findings.len(),
        fixable: findings.iter().filter(|f| f.fix.is_some()).count(),
    }
}

fn score(rules: &[RuleOutcome]) -> Score {
    let count = |status: RuleStatus| rules.iter().filter(|r| r.status == status).count();
    let passed = count(RuleStatus::Passed);
    let failed = count(RuleStatus::Failed);
    let not_applicable = count(RuleStatus::NotApplicable);
    let not_checked = count(RuleStatus::NotChecked);
    let by_hand = count(RuleStatus::ByHand);
    let percent = (passed * 100).checked_div(passed + failed).unwrap_or(0) as u32;
    let mut statement = format!(
        "{passed} of {} checks a program can make passed.",
        passed + failed
    );
    if not_checked > 0 {
        statement.push_str(&format!(
            " {not_checked} could not be checked, so the score says nothing about them."
        ));
    }
    if by_hand > 0 {
        statement.push_str(&format!(
            " {by_hand} more need a person to look, because no program can judge them."
        ));
    }
    statement.push_str(" A full score does not by itself make a document accessible.");
    Score {
        percent,
        passed,
        failed,
        not_applicable,
        not_checked,
        by_hand,
        statement,
    }
}

/// checks the document with the default settings.
pub fn check(
    doc: &Document,
    renderer: Option<&mut dyn RegionRenderer>,
) -> Result<Report, A11yError> {
    check_with_options(doc, renderer, &CheckOptions::default())
}

/// checks the document. `renderer` draws page regions for the contrast
/// rule; without one, contrast is reported as not checked.
pub fn check_with_options(
    doc: &Document,
    renderer: Option<&mut dyn RegionRenderer>,
    options: &CheckOptions,
) -> Result<Report, A11yError> {
    let context = Context::read(doc, options.clone())?;
    let mut out = Collector::default();
    tagging::check(&context, &mut out);
    language::check(&context, &mut out);
    title::check(&context, &mut out);
    figures::check(&context, &mut out);
    headings::check(&context, &mut out);
    reading_order::check(&context, &mut out);
    tables::check(&context, &mut out);
    lists::check(&context, &mut out);
    annotations::check(&context, &mut out);
    content_tags::check(&context, &mut out);
    fonts::check(&context, &mut out);
    contrast::check(&context, renderer, &mut out);
    document::check(&context, &mut out);
    hand::check(&context, &mut out);
    Ok(out.into_report(context.page_count()))
}

/// marks every rule that needs tags as not checked, for untagged documents.
pub(crate) fn skip_untagged(out: &mut Collector, ids: &[&'static str]) {
    for id in ids {
        out.not_checked(
            id,
            "The document has no tags yet; see “The document has no tags”.",
        );
    }
}
