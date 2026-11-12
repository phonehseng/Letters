//! pdf/a rules checked on every object of the file: actions and scripts,
//! additional actions, lzw compression, streams kept in other files,
//! postscript and reference xobjects, and the size limits of part 1.

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::level::PdfaLevel;
use crate::objects::{dict_of, get, get_dict, get_name, names_in};
use crate::report::{Findings, Fix};

/// actions every part forbids (19005-1 6.6.1, 19005-2 6.6.1).
const FORBIDDEN_ACTIONS: [&[u8]; 6] = [
    b"Launch",
    b"Sound",
    b"Movie",
    b"ResetForm",
    b"ImportData",
    b"JavaScript",
];
/// actions parts 2 and 3 forbid as well (19005-2 6.6.1).
const FORBIDDEN_LATER: [&[u8]; 5] = [
    b"Hide",
    b"SetOCGState",
    b"Rendition",
    b"Trans",
    b"GoTo3DView",
];
/// the only named actions allowed: moving between pages.
const ALLOWED_NAMED: [&[u8]; 4] = [b"NextPage", b"PrevPage", b"FirstPage", b"LastPage"];

/// pdf 1.4 implementation limits that part 1 keeps (19005-1 6.1.12).
const PART_ONE_MAX_INTEGER: i64 = 2_147_483_647;
const PART_ONE_MAX_REAL: f32 = 32_767.0;
const PART_ONE_MAX_STRING: usize = 65_535;
const PART_ONE_MAX_ARRAY: usize = 8_191;
const PART_ONE_MAX_DICTIONARY: usize = 4_095;
/// names are limited to 127 bytes in every part (19005-2 6.1.13 too).
const MAX_NAME: usize = 127;
/// parts 2 and 3 limit strings to 32767 bytes (19005-2 6.1.13).
const LATER_MAX_STRING: usize = 32_767;
const MAX_INDIRECT_OBJECTS: usize = 8_388_607;

pub(super) fn check_objects(doc: &Document, level: PdfaLevel, findings: &mut Findings) {
    if doc.objects.len() > MAX_INDIRECT_OBJECTS {
        findings.add(
            "limits.objects",
            &level.clause("6.1.12", "6.1.13"),
            "The file has more objects than PDF/A allows.".into(),
            Fix::NotPossible,
        );
    }
    for (id, object) in &doc.objects {
        if let Some(dict) = dict_of(object) {
            check_actions(doc, *id, dict, level, findings);
            check_xobject_kind(doc, *id, dict, level, findings);
            check_file_specification(doc, *id, dict, level, findings);
        }
        if let Object::Stream(stream) = object {
            check_stream(doc, *id, &stream.dict, level, findings);
        }
        check_limits(object, *id, level, findings, 0);
    }
    check_javascript_names(doc, level, findings);
}

fn check_actions(
    doc: &Document,
    id: ObjectId,
    dict: &Dictionary,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    if dict.has(b"AA") {
        findings
            .add(
                "actions.additional",
                &level.clause("6.6.2", "6.6.2"),
                "Something in the document runs actions on events (/AA), which PDF/A forbids."
                    .into(),
                Fix::Automatic,
            )
            .at(Some(id));
    }
    for key in [b"A".as_slice(), b"OpenAction", b"Next"] {
        let Some(action) = get(doc, dict, key).and_then(|value| value.as_dict().ok()) else {
            continue;
        };
        if let Some(problem) = forbidden_action(doc, action, level) {
            findings
                .add(
                    "actions.forbidden",
                    &level.clause("6.6.1", "6.6.1"),
                    problem,
                    Fix::Automatic,
                )
                .at(Some(id));
        }
    }
}

/// why an action is forbidden, if it is.
pub(crate) fn forbidden_action(
    doc: &Document,
    action: &Dictionary,
    level: PdfaLevel,
) -> Option<String> {
    let kind = get_name(doc, action, b"S")?;
    let forbidden = FORBIDDEN_ACTIONS.contains(&kind.as_slice())
        || (!level.is_part_one() && FORBIDDEN_LATER.contains(&kind.as_slice()));
    if forbidden {
        let kind = String::from_utf8_lossy(&kind);
        return Some(format!("A {kind} action is used, which PDF/A forbids."));
    }
    if kind == b"Named" {
        let named = get_name(doc, action, b"N").unwrap_or_default();
        if !ALLOWED_NAMED.contains(&named.as_slice()) {
            let named = String::from_utf8_lossy(&named);
            return Some(format!(
                "The named action {named} is used; PDF/A only allows moving between pages."
            ));
        }
    }
    None
}

fn check_javascript_names(doc: &Document, level: PdfaLevel, findings: &mut Findings) {
    let has_scripts = doc
        .catalog()
        .ok()
        .and_then(|catalog| get_dict(doc, catalog, b"Names"))
        .is_some_and(|names| names.has(b"JavaScript"));
    if has_scripts {
        findings.add(
            "actions.javascript",
            &level.clause("6.6.1", "6.6.1"),
            "The document has scripts that run when it opens, which PDF/A forbids.".into(),
            Fix::Automatic,
        );
    }
}

fn check_xobject_kind(
    doc: &Document,
    id: ObjectId,
    dict: &Dictionary,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let subtype = get_name(doc, dict, b"Subtype").unwrap_or_default();
    let postscript = subtype == b"PS" || get_name(doc, dict, b"Subtype2").as_deref() == Some(b"PS");
    if postscript && get_name(doc, dict, b"Type").as_deref() != Some(b"Annot") {
        findings
            .add(
                "xobjects.postscript",
                &level.clause("6.2.7", "6.2.9"),
                "The document contains PostScript, which PDF/A forbids.".into(),
                Fix::Automatic,
            )
            .at(Some(id));
    }
    if subtype == b"Form" && dict.has(b"Ref") {
        findings
            .add(
                "xobjects.reference",
                &level.clause("6.2.6", "6.2.9"),
                "A drawing refers to a page of another file, which PDF/A forbids.".into(),
                Fix::Automatic,
            )
            .at(Some(id));
    }
    if dict.has(b"OPI") {
        findings
            .add(
                "xobjects.opi",
                &level.clause("6.2.6", "6.2.9"),
                "A picture refers to a print-shop replacement image (/OPI), which PDF/A forbids."
                    .into(),
                Fix::Automatic,
            )
            .at(Some(id));
    }
}

/// part 1: no file specification may carry an embedded file, even one
/// nothing lists (19005-1 6.1.11; veraPDF checks every object for it).
fn check_file_specification(
    doc: &Document,
    id: ObjectId,
    dict: &Dictionary,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let is_specification = get_name(doc, dict, b"Type").as_deref() == Some(b"Filespec");
    if level.is_part_one() && is_specification && dict.has(b"EF") {
        findings
            .add(
                "files.partOne",
                "ISO 19005-1 6.1.11",
                "A file is embedded in the document; PDF/A-1 doesn’t allow attachments.".into(),
                Fix::Automatic,
            )
            .at(Some(id));
    }
}

fn check_stream(
    doc: &Document,
    id: ObjectId,
    dict: &Dictionary,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let external = dict.has(b"F") || dict.has(b"FFilter") || dict.has(b"FDecodeParms");
    if external {
        findings
            .add(
                "streams.external",
                &level.clause("6.1.7", "6.1.7.1"),
                "A stream’s data is kept in another file, which PDF/A forbids.".into(),
                Fix::NotPossible,
            )
            .at(Some(id));
    }
    let filters = names_in(doc, dict.get(b"Filter").ok());
    if filters
        .iter()
        .any(|filter| filter == b"LZWDecode" || filter == b"LZW")
    {
        findings
            .add(
                "streams.lzw",
                &level.clause("6.1.10", "6.1.7.2"),
                "A stream is compressed with LZW, which PDF/A forbids.".into(),
                Fix::Automatic,
            )
            .at(Some(id));
    }
    if level.is_part_one() && filters.iter().any(|filter| filter == b"JPXDecode") {
        findings
            .add(
                "images.jpeg2000",
                "ISO 19005-1 6.1.10",
                "A picture is JPEG 2000, which PDF/A-1 doesn’t allow.".into(),
                Fix::NotPossible,
            )
            .at(Some(id));
    }
}

/// values beyond the size limits (checked through arrays and dictionaries).
fn check_limits(
    object: &Object,
    id: ObjectId,
    level: PdfaLevel,
    findings: &mut Findings,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    let part_one = level.is_part_one();
    let clause = level.clause("6.1.12", "6.1.13");
    let mut report = |message: &str| {
        findings
            .add(
                "limits.value",
                &clause,
                message.to_owned(),
                Fix::NotPossible,
            )
            .at(Some(id));
    };
    match object {
        Object::Integer(value) if part_one && value.abs() > PART_ONE_MAX_INTEGER => {
            report("A number is larger than PDF/A-1 allows.")
        }
        Object::Real(value) if part_one && value.abs() > PART_ONE_MAX_REAL => {
            report("A decimal number is larger than PDF/A-1 allows.")
        }
        Object::String(bytes, _) => {
            let limit = if part_one {
                PART_ONE_MAX_STRING
            } else {
                LATER_MAX_STRING
            };
            if bytes.len() > limit {
                report("A piece of text in the file is longer than PDF/A allows.");
            }
        }
        Object::Name(name) if name.len() > MAX_NAME => {
            report("A name in the file is longer than 127 bytes.")
        }
        Object::Array(items) => {
            if part_one && items.len() > PART_ONE_MAX_ARRAY {
                report("A list in the file is longer than PDF/A-1 allows.");
            }
            for item in items {
                check_limits(item, id, level, findings, depth + 1);
            }
        }
        Object::Dictionary(dict) => check_dictionary_limits(dict, id, level, findings, depth),
        Object::Stream(stream) => check_dictionary_limits(&stream.dict, id, level, findings, depth),
        _ => {}
    }
}

fn check_dictionary_limits(
    dict: &Dictionary,
    id: ObjectId,
    level: PdfaLevel,
    findings: &mut Findings,
    depth: usize,
) {
    if level.is_part_one() && dict.len() > PART_ONE_MAX_DICTIONARY {
        findings
            .add(
                "limits.value",
                &level.clause("6.1.12", "6.1.13"),
                "A dictionary in the file has more entries than PDF/A-1 allows.".into(),
                Fix::NotPossible,
            )
            .at(Some(id));
    }
    for (key, value) in dict.iter() {
        if key.len() > MAX_NAME {
            findings
                .add(
                    "limits.value",
                    &level.clause("6.1.12", "6.1.13"),
                    "A name in the file is longer than 127 bytes.".into(),
                    Fix::NotPossible,
                )
                .at(Some(id));
        }
        check_limits(value, id, level, findings, depth + 1);
    }
}
