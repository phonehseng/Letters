//! pdf/a (iso 19005) parts 1, 2 and 3 at levels b and u: checking a
//! document against each rule, and converting it.
//!
//! `check_pdfa` looks at the objects of a loaded document; `check_pdfa_file`
//! also looks at the file's bytes (header, end of file, stream keywords).
//! conformance claims come from this checker only: see `CHECKER_NOTE`.

mod convert;
mod level;
mod rules_document;
mod rules_drawing;
mod rules_file;
mod rules_objects;
mod rules_pages;
pub(crate) mod xmp;

use lopdf::Document;
use serde::{Deserialize, Serialize};

pub use convert::{
    convert_to_pdfa, save_pdfa, srgb_profile, ConversionHelpers, ConversionOptions,
    ConversionReport, FontFile, FontOutcome, FontRequest, FontSource, PageRenderer, RasterisedArea,
    RenderedPage,
};
pub use level::PdfaLevel;

use crate::error::StandardsResult;
use crate::report::{Finding, Findings, Fix};

pub(crate) use rules_document::{associated_files, info_dictionary};
pub(crate) use rules_objects::forbidden_action;
pub(crate) use rules_pages::{annotation_flags, can_draw_appearance, type_allowed};

/// said with every result: these are Letters' own checks.
pub const CHECKER_NOTE: &str = "These results come from Letters’ own PDF/A checker, not from an independent validator. To confirm conformance, check the file with veraPDF (the reference validator).";

/// what a pdf/a check found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfaReport {
    pub level: PdfaLevel,
    /// no findings at all
    pub compliant: bool,
    pub findings: Vec<Finding>,
    /// how many findings `convert_to_pdfa` can fix without help
    pub automatically_fixable: usize,
    pub note: String,
}

/// checks a loaded document against a pdf/a level.
pub fn check_pdfa(doc: &Document, level: PdfaLevel) -> PdfaReport {
    let mut findings = Findings::default();
    let intent = rules_document::check_document(doc, level, &mut findings);
    rules_objects::check_objects(doc, level, &mut findings);
    rules_pages::check_pages(doc, level, &mut findings);
    rules_drawing::check_drawings(doc, level, intent, &mut findings);
    report(level, findings)
}

/// checks a file: everything `check_pdfa` checks, plus its bytes.
pub fn check_pdfa_file(bytes: &[u8], level: PdfaLevel) -> StandardsResult<PdfaReport> {
    let doc = Document::load_mem(bytes)?;
    let mut report = check_pdfa(&doc, level);
    let mut file = Findings::default();
    rules_file::check_file(bytes, level, &mut file);
    report.findings.extend(file.deduplicated());
    Ok(finish(report))
}

fn report(level: PdfaLevel, findings: Findings) -> PdfaReport {
    finish(PdfaReport {
        level,
        compliant: false,
        findings: findings.deduplicated(),
        automatically_fixable: 0,
        note: CHECKER_NOTE.to_owned(),
    })
}

fn finish(mut report: PdfaReport) -> PdfaReport {
    report.compliant = report.findings.is_empty();
    report.automatically_fixable = report
        .findings
        .iter()
        .filter(|finding| finding.fix == Fix::Automatic)
        .count();
    report
}
