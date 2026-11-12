//! pdf/a rules about the document as a whole: encryption, the file
//! identifier, xmp metadata and its agreement with the info dictionary,
//! output intents, forms (xfa, NeedAppearances), optional content and
//! embedded files.

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::level::PdfaLevel;
use super::xmp::{read_xmp, same_date, DocumentInfo};
use crate::objects::{get, get_array, get_dict, get_name, resolve, stream_bytes};
use crate::report::{Findings, Fix};

/// what the document's output intent says, for the colour rules.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutputIntent {
    pub present: bool,
    /// the profile's colour space from its header: b"RGB ", b"CMYK" or b"GRAY"
    pub space: [u8; 4],
}

pub(super) fn check_document(
    doc: &Document,
    level: PdfaLevel,
    findings: &mut Findings,
) -> OutputIntent {
    if doc.trailer.has(b"Encrypt") || doc.is_encrypted() {
        findings.add(
            "document.encrypted",
            &level.clause("6.1.3", "6.1.3"),
            "The file is encrypted, which PDF/A forbids. Unlock it first.".into(),
            Fix::NotPossible,
        );
    }
    if !doc.trailer.has(b"ID") {
        findings.add(
            "document.fileId",
            &level.clause("6.1.3", "6.1.3"),
            "The file has no identifier (/ID) in its trailer.".into(),
            Fix::Automatic,
        );
    }
    let Ok(catalog) = doc.catalog() else {
        return OutputIntent::default();
    };
    check_metadata(doc, catalog, level, findings);
    check_forms(doc, catalog, level, findings);
    check_optional_content(doc, catalog, level, findings);
    check_embedded_files(doc, catalog, level, findings);
    output_intent(doc, catalog, level, findings)
}

fn check_metadata(doc: &Document, catalog: &Dictionary, level: PdfaLevel, findings: &mut Findings) {
    let clause = level.clause("6.7.2", "6.6.2.1");
    let Some(Object::Stream(stream)) = get(doc, catalog, b"Metadata") else {
        findings.add(
            "metadata.missing",
            &clause,
            "The document has no XMP metadata, which PDF/A needs.".into(),
            Fix::Automatic,
        );
        return;
    };
    if level.is_part_one() && stream.dict.has(b"Filter") {
        findings.add(
            "metadata.filtered",
            "ISO 19005-1 6.7.2",
            "The XMP metadata is compressed; PDF/A-1 asks for it uncompressed.".into(),
            Fix::Automatic,
        );
    }
    let text = String::from_utf8_lossy(&stream_bytes(stream)).into_owned();
    let facts = read_xmp(&text);
    if !facts.has_packet_header {
        findings.add(
            "metadata.packet",
            &level.clause("6.7.5", "6.6.2.2"),
            "The XMP metadata doesn’t start with an xpacket header.".into(),
            Fix::Automatic,
        );
    }
    let part_matches = facts.part.as_deref() == Some(level.part().to_string().as_str());
    let conformance_matches = facts
        .conformance
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case(level.conformance()));
    if !part_matches || !conformance_matches {
        findings.add(
            "metadata.identification",
            &level.clause("6.7.11", "6.6.4"),
            format!(
                "The metadata doesn’t identify the file as {}.",
                level.name()
            ),
            Fix::Automatic,
        );
    }
    check_info_agrees(doc, &facts.info, level, findings);
}

/// the document information the info dictionary holds, decoded.
pub(crate) fn info_dictionary(doc: &Document) -> DocumentInfo {
    let Some(info) = doc
        .trailer
        .get(b"Info")
        .ok()
        .map(|value| resolve(doc, value))
        .and_then(|value| value.as_dict().ok())
    else {
        return DocumentInfo::default();
    };
    let text = |key: &[u8]| match get(doc, info, key) {
        Some(Object::String(bytes, _)) => Some(decode_text(bytes)),
        _ => None,
    };
    DocumentInfo {
        title: text(b"Title"),
        author: text(b"Author"),
        subject: text(b"Subject"),
        keywords: text(b"Keywords"),
        creator: text(b"Creator"),
        producer: text(b"Producer"),
        creation_date: text(b"CreationDate"),
        modification_date: text(b"ModDate"),
    }
}

/// a pdf text string: utf-16 with a byte order mark, or pdfdocencoding
/// (read as latin-1, which agrees for every letter people type).
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|pair| pair.len() == 2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    bytes.iter().map(|byte| *byte as char).collect()
}

/// every entry of the info dictionary must agree with its xmp
/// equivalent (19005-1 6.7.3; parts 2 and 3 keep the rule in 6.6.2.3).
fn check_info_agrees(
    doc: &Document,
    xmp: &DocumentInfo,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let info = info_dictionary(doc);
    let pairs = [
        ("title", &info.title, &xmp.title, false),
        ("author", &info.author, &xmp.author, false),
        ("subject", &info.subject, &xmp.subject, false),
        ("keywords", &info.keywords, &xmp.keywords, false),
        ("creator", &info.creator, &xmp.creator, false),
        ("producer", &info.producer, &xmp.producer, false),
        (
            "creation date",
            &info.creation_date,
            &xmp.creation_date,
            true,
        ),
        (
            "modification date",
            &info.modification_date,
            &xmp.modification_date,
            true,
        ),
    ];
    for (label, in_info, in_xmp, is_date) in pairs {
        let Some(in_info) = in_info else { continue };
        let agrees = match in_xmp {
            Some(in_xmp) if is_date => same_date(in_info, in_xmp),
            Some(in_xmp) => in_info == in_xmp,
            None => false,
        };
        if !agrees {
            findings.add(
                "metadata.infoMismatch",
                &level.clause("6.7.3", "6.6.2.3"),
                format!("The document’s {label} in its information dictionary doesn’t match its XMP metadata."),
                Fix::Automatic,
            );
        }
    }
}

fn check_forms(doc: &Document, catalog: &Dictionary, level: PdfaLevel, findings: &mut Findings) {
    let Some(form) = get_dict(doc, catalog, b"AcroForm") else {
        return;
    };
    if form.has(b"XFA") {
        findings.add(
            "forms.xfa",
            &level.clause("6.9", "6.4.2"),
            "The form carries an XFA copy, which PDF/A forbids.".into(),
            Fix::Automatic,
        );
    }
    let needs_appearances = matches!(
        get(doc, form, b"NeedAppearances"),
        Some(Object::Boolean(true))
    );
    if needs_appearances {
        findings.add(
            "forms.needAppearances",
            &level.clause("6.9", "6.4.1"),
            "The form asks readers to draw its fields themselves (NeedAppearances), which PDF/A forbids.".into(),
            Fix::Automatic,
        );
    }
}

fn check_optional_content(
    doc: &Document,
    catalog: &Dictionary,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let Some(properties) = get_dict(doc, catalog, b"OCProperties") else {
        return;
    };
    if level.is_part_one() {
        findings.add(
            "layers.partOne",
            "ISO 19005-1 6.1.13",
            "The document has layers (optional content), which PDF/A-1 doesn’t allow.".into(),
            Fix::NotPossible,
        );
        return;
    }
    let mut configurations: Vec<&Dictionary> =
        get_dict(doc, properties, b"D").into_iter().collect();
    for config in get_array(doc, properties, b"Configs").into_iter().flatten() {
        if let Ok(config) = resolve(doc, config).as_dict() {
            configurations.push(config);
        }
    }
    for config in configurations {
        if !config.has(b"Name") || config.has(b"AS") {
            findings.add(
                "layers.configuration",
                "ISO 19005-2 6.9",
                "A layer configuration has no name, or switches layers automatically (/AS); PDF/A needs names and forbids /AS.".into(),
                Fix::Automatic,
            );
        }
    }
}

/// the embedded files of the document, with their file specifications.
pub(crate) fn embedded_files(
    doc: &Document,
    catalog: &Dictionary,
) -> Vec<(Option<ObjectId>, Dictionary)> {
    let tree =
        get_dict(doc, catalog, b"Names").and_then(|names| get_dict(doc, names, b"EmbeddedFiles"));
    let mut out = Vec::new();
    if let Some(tree) = tree {
        collect_name_tree(doc, tree, &mut out, 0);
    }
    out
}

fn collect_name_tree(
    doc: &Document,
    node: &Dictionary,
    out: &mut Vec<(Option<ObjectId>, Dictionary)>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    for pair in get_array(doc, node, b"Names")
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .chunks(2)
    {
        if let [_, value] = pair {
            if let Ok(spec) = resolve(doc, value).as_dict() {
                out.push((value.as_reference().ok(), spec.clone()));
            }
        }
    }
    for kid in get_array(doc, node, b"Kids").into_iter().flatten() {
        if let Ok(kid) = resolve(doc, kid).as_dict() {
            collect_name_tree(doc, kid, out, depth + 1);
        }
    }
}

/// part 1 allows no embedded files, part 2 only pdf/a files, part 3 any
/// file with its relationship and media type (19005-3 6.8).
fn check_embedded_files(
    doc: &Document,
    catalog: &Dictionary,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let associated = associated_files(doc);
    for (id, spec) in embedded_files(doc, catalog) {
        let name = get(doc, &spec, b"UF")
            .or_else(|| get(doc, &spec, b"F"))
            .and_then(|value| value.as_str().ok())
            .map(decode_text)
            .unwrap_or_else(|| "an attachment".into());
        let file = get_dict(doc, &spec, b"EF")
            .and_then(|ef| get(doc, ef, b"F"))
            .and_then(|value| value.as_stream().ok());
        match level.part() {
            1 => {
                findings.add(
                    "files.partOne",
                    "ISO 19005-1 6.1.11",
                    format!("“{name}” is attached; PDF/A-1 doesn’t allow attachments."),
                    Fix::Automatic,
                );
            }
            2 => {
                let is_pdfa = file.is_some_and(|stream| {
                    let bytes = stream_bytes(stream);
                    bytes.starts_with(b"%PDF-")
                        && bytes.windows(7).any(|window| window == b"pdfaid:")
                });
                if !is_pdfa {
                    findings.add(
                        "files.notPdfa",
                        "ISO 19005-2 6.8",
                        format!("“{name}” is attached but isn’t a PDF/A file; PDF/A-2 only allows PDF/A attachments."),
                        Fix::Automatic,
                    );
                }
            }
            _ => {
                let has_relationship = get_name(doc, &spec, b"AFRelationship").is_some();
                let has_type = file.is_some_and(|stream| stream.dict.has(b"Subtype"));
                let is_associated = id.is_some_and(|id| associated.contains(&id));
                if !has_relationship || !has_type || !spec.has(b"UF") || !is_associated {
                    findings.add(
                        "files.description",
                        "ISO 19005-3 6.8",
                        format!("“{name}” is attached without its relationship to the document, its association (/AF), its media type or its unicode name."),
                        Fix::Automatic,
                    );
                }
            }
        }
    }
}

/// reads the pdf/a output intent (/S /GTS_PDFA1 with a profile), and
/// reports one with no profile, or several different ones (19005-2 6.2.3).
fn output_intent(
    doc: &Document,
    catalog: &Dictionary,
    level: PdfaLevel,
    findings: &mut Findings,
) -> OutputIntent {
    let mut found = OutputIntent::default();
    let mut profiles: Vec<Vec<u8>> = Vec::new();
    for intent in get_array(doc, catalog, b"OutputIntents")
        .into_iter()
        .flatten()
    {
        let Ok(intent) = resolve(doc, intent).as_dict() else {
            continue;
        };
        if get_name(doc, intent, b"S").as_deref() != Some(b"GTS_PDFA1") {
            continue;
        }
        let Some(Object::Stream(profile)) = get(doc, intent, b"DestOutputProfile") else {
            findings.add(
                "color.intentProfile",
                &level.clause("6.2.2", "6.2.3"),
                "The PDF/A output intent has no colour profile.".into(),
                Fix::Automatic,
            );
            continue;
        };
        let bytes = stream_bytes(profile);
        if bytes.len() >= 20 {
            found.present = true;
            found.space.copy_from_slice(&bytes[16..20]);
        }
        if !profiles.contains(&bytes) {
            profiles.push(bytes);
        }
    }
    if profiles.len() > 1 {
        findings.add(
            "color.severalIntents",
            &level.clause("6.2.2", "6.2.3"),
            "The document has more than one different output intent profile.".into(),
            Fix::Automatic,
        );
    }
    found
}

/// every file specification some /AF list refers to (19005-3 6.8 asks
/// each embedded file to be associated with the document or a part of it).
pub(crate) fn associated_files(doc: &Document) -> Vec<ObjectId> {
    let mut found = Vec::new();
    for object in doc.objects.values() {
        let Some(dict) = crate::objects::dict_of(object) else {
            continue;
        };
        if let Some(Object::Array(items)) = dict.get(b"AF").ok().map(|value| resolve(doc, value)) {
            found.extend(items.iter().filter_map(|item| item.as_reference().ok()));
        }
    }
    found
}
