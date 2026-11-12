//! claiming pdf/ua: the identifier in the xmp metadata that says "this
//! file follows iso 14289-1".
//!
//! the identifier is only written when every rule a program can check
//! passes. that is necessary but not enough: pdf/ua also needs a person
//! to confirm the things no program can judge, listed in
//! `PDFUA_HUMAN_CHECKS`. the report lists them as "check by hand" items,
//! and the wizard should ask the person to go through them before
//! calling this.

use lopdf::Document;

use crate::error::A11yError;
use crate::objects::ensure_editable;
use crate::report::FixResult;
use crate::rules::{check, RegionRenderer};
use crate::xmp::{self, XmpUpdate};

/// what pdf/ua needs that this tool cannot check, for the ui to show
/// before the person confirms.
pub const PDFUA_HUMAN_CHECKS: &[&str] = &[
    "Each picture’s alt text says what the picture shows and why it is there.",
    "The reading order makes sense when the page is read aloud from start to end.",
    "Headings are used for section titles, not just to make text big or bold.",
    "Colour is never the only way something is shown.",
    "Phrases in other languages are marked with their language.",
    "Nothing flashes, and nothing only works with scripts.",
    "Decoration is marked as decoration, and nothing meaningful is.",
];

/// writes the pdf/ua-1 identifier (pdfuaid:part 1) into the xmp metadata,
/// but only when the checker finds nothing that must be fixed. the
/// renderer is optional and only adds the contrast check, which pdf/ua
/// leaves to wcag.
pub fn mark_pdfua(
    doc: &mut Document,
    renderer: Option<&mut dyn RegionRenderer>,
) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let report = check(doc, renderer)?;
    if !report.machine_checks_pass() {
        return Err(A11yError::NotReadyForPdfUa);
    }
    xmp::update(
        doc,
        &XmpUpdate {
            title: None,
            pdfua_part: Some(1),
        },
    )?;
    Ok(FixResult {
        changes: vec![
            "Marked the document as following PDF/UA-1.".into(),
            "Every rule a program can check passes. PDF/UA also needs the checks by hand in the report; the mark is only true once a person has done them.".into(),
        ],
    })
}
