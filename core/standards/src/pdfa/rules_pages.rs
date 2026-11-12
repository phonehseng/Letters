//! pdf/a rules about pages and their annotations: page size, page groups
//! in part 1, annotation types, flags, opacity and appearance streams.

use lopdf::{Dictionary, Document, ObjectId};

use super::level::PdfaLevel;
use crate::objects::{
    dict_of, get, get_array, get_dict, get_name, get_number, inherited, page_ids, rect_of, resolve,
};
use crate::report::{Findings, Fix};

/// page sizes pdf readers handle: 3 to 14400 units (iso 32000-1 annex c,
/// kept by 19005-2 6.1.13).
const MIN_PAGE_SIDE: f64 = 3.0;
const MAX_PAGE_SIDE: f64 = 14_400.0;

/// annotation flag bits (iso 32000 table 165).
pub(crate) mod annotation_flags {
    pub const INVISIBLE: i64 = 1;
    pub const HIDDEN: i64 = 2;
    pub const PRINT: i64 = 4;
    pub const NO_VIEW: i64 = 32;
    pub const TOGGLE_NO_VIEW: i64 = 256;
}

/// the annotation types each part allows (19005-1 6.5.2, 19005-2 6.3.1:
/// the types of the pdf version they are based on, minus media and 3d).
const PART_ONE_TYPES: [&[u8]; 17] = [
    b"Text",
    b"Link",
    b"FreeText",
    b"Line",
    b"Square",
    b"Circle",
    b"Polygon",
    b"PolyLine",
    b"Highlight",
    b"Underline",
    b"Squiggly",
    b"StrikeOut",
    b"Stamp",
    b"Ink",
    b"Popup",
    b"Widget",
    b"PrinterMark",
];
const LATER_EXTRA_TYPES: [&[u8]; 6] = [
    b"Caret",
    b"FileAttachment",
    b"TrapNet",
    b"Watermark",
    b"Redact",
    b"Projection",
];

pub(crate) fn type_allowed(subtype: &[u8], level: PdfaLevel) -> bool {
    if PART_ONE_TYPES.contains(&subtype) {
        return true;
    }
    if level.is_part_one() {
        return false;
    }
    // 19005-2 allows file attachments only when the attached file is itself
    // allowed, which the embedded file rule checks
    LATER_EXTRA_TYPES.contains(&subtype)
}

pub(super) fn check_pages(doc: &Document, level: PdfaLevel, findings: &mut Findings) {
    for (index, page_id) in page_ids(doc).into_iter().enumerate() {
        let page = Some(index as u32);
        check_page_size(doc, page_id, page, level, findings);
        let Ok(dict) = doc.get_dictionary(page_id) else {
            continue;
        };
        if level.is_part_one() && get_dict(doc, dict, b"Group").is_some() {
            findings
                .add(
                    "transparency.pageGroup",
                    "ISO 19005-1 6.4",
                    "The page is set up for transparency (a page group), which PDF/A-1 forbids."
                        .into(),
                    Fix::Automatic,
                )
                .on_page(page);
        }
        for annotation in get_array(doc, dict, b"Annots").into_iter().flatten() {
            let id = annotation.as_reference().ok();
            if let Some(annotation) = dict_of(resolve(doc, annotation)) {
                check_annotation(doc, annotation, id, page, level, findings);
            }
        }
    }
}

fn check_page_size(
    doc: &Document,
    page_id: ObjectId,
    page: Option<u32>,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let Some([left, bottom, right, top]) =
        inherited(doc, page_id, b"MediaBox").and_then(|value| rect_of(doc, value))
    else {
        findings
            .add(
                "pages.mediaBox",
                &level.clause("6.1.12", "6.1.13"),
                "The page has no size (MediaBox).".into(),
                Fix::NotPossible,
            )
            .on_page(page);
        return;
    };
    let sides = [(right - left).abs(), (top - bottom).abs()];
    if sides
        .iter()
        .any(|side| *side < MIN_PAGE_SIDE || *side > MAX_PAGE_SIDE)
    {
        findings
            .add(
                "pages.size",
                &level.clause("6.1.12", "6.1.13"),
                "The page is smaller than 3 or larger than 14,400 units on a side.".into(),
                Fix::NotPossible,
            )
            .on_page(page);
    }
}

fn check_annotation(
    doc: &Document,
    annotation: &Dictionary,
    id: Option<ObjectId>,
    page: Option<u32>,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let subtype = get_name(doc, annotation, b"Subtype").unwrap_or_default();
    let kind = String::from_utf8_lossy(&subtype).into_owned();
    if !type_allowed(&subtype, level) {
        findings
            .add(
                "annotations.type",
                &level.clause("6.5.2", "6.3.1"),
                format!(
                    "A {kind} annotation is on the page, which {} doesn’t allow.",
                    level.name()
                ),
                Fix::Automatic,
            )
            .on_page(page)
            .at(id);
        return;
    }
    check_flags(doc, annotation, &subtype, id, page, level, findings);
    if level.is_part_one() {
        let opacity = get_number(doc, annotation, b"CA").unwrap_or(1.0);
        if opacity < 1.0 {
            findings
                .add(
                    "annotations.opacity",
                    "ISO 19005-1 6.5.3",
                    format!("A {kind} annotation is see-through, which PDF/A-1 forbids."),
                    Fix::Automatic,
                )
                .on_page(page)
                .at(id);
        }
    }
    check_appearance(doc, annotation, &subtype, id, page, level, findings);
}

fn check_flags(
    doc: &Document,
    annotation: &Dictionary,
    subtype: &[u8],
    id: Option<ObjectId>,
    page: Option<u32>,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    use annotation_flags::*;
    if subtype == b"Popup" {
        return;
    }
    let flags = get(doc, annotation, b"F")
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0);
    let mut hiding = INVISIBLE | HIDDEN | NO_VIEW;
    if !level.is_part_one() {
        hiding |= TOGGLE_NO_VIEW;
    }
    if flags & PRINT == 0 || flags & hiding != 0 {
        findings
            .add(
                "annotations.flags",
                &level.clause("6.5.3", "6.3.2"),
                "An annotation doesn’t print, or is hidden; PDF/A needs annotations to print and show.".into(),
                Fix::Automatic,
            )
            .on_page(page)
            .at(id);
    }
}

/// parts 2 and 3 need a normal appearance for every annotation except
/// popups and links; every part allows only /N in it (19005-2 6.3.3).
fn check_appearance(
    doc: &Document,
    annotation: &Dictionary,
    subtype: &[u8],
    id: Option<ObjectId>,
    page: Option<u32>,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let appearance = get_dict(doc, annotation, b"AP");
    let exempt = subtype == b"Popup" || subtype == b"Link";
    let rect_is_empty = get(doc, annotation, b"Rect")
        .and_then(|rect| rect_of(doc, rect))
        .is_some_and(|[left, bottom, right, top]| left == right || bottom == top);
    let Some(appearance) = appearance else {
        if !exempt && !level.is_part_one() && !rect_is_empty {
            let fix = appearance_fix(subtype);
            findings
                .add(
                    "annotations.appearance",
                    "ISO 19005-2 6.3.3",
                    format!("A {} annotation has no appearance stream, so readers may draw it differently.", String::from_utf8_lossy(subtype)),
                    fix,
                )
                .on_page(page)
                .at(id);
        }
        return;
    };
    let only_normal = appearance.iter().all(|(key, _)| key.as_slice() == b"N");
    if !only_normal {
        findings
            .add(
                "annotations.appearanceKeys",
                &level.clause("6.5.3", "6.3.3"),
                "An annotation has rollover or pressed appearances; PDF/A allows only the normal one.".into(),
                Fix::Automatic,
            )
            .on_page(page)
            .at(id);
    }
}

/// the annotation kinds `convert_to_pdfa` can draw appearances for.
pub(crate) fn can_draw_appearance(subtype: &[u8]) -> bool {
    matches!(
        subtype,
        b"Widget"
            | b"Square"
            | b"Circle"
            | b"Line"
            | b"Ink"
            | b"Highlight"
            | b"Underline"
            | b"StrikeOut"
            | b"Squiggly"
            | b"Text"
            | b"FreeText"
            | b"Polygon"
            | b"PolyLine"
    )
}

fn appearance_fix(subtype: &[u8]) -> Fix {
    if can_draw_appearance(subtype) {
        Fix::Automatic
    } else {
        Fix::NotPossible
    }
}
