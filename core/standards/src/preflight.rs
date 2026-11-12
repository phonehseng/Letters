//! "Check This PDF": structural soundness (cross-reference offsets, stream
//! lengths, references to missing objects), fonts, picture resolutions,
//! page boxes, what makes the file big, and features that some viewers
//! don't show the same way, so "looks identical everywhere" has an honest
//! checklist behind it. it reports; it changes nothing.

use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::error::StandardsResult;
use crate::fonts::font_facts;
use crate::images::{placed_images, PlacedImage};
use crate::objects::{dict_of, get, get_dict, get_name, inherited, page_ids, rect_of};
use crate::report::{Finding, Findings, Fix};

/// pictures below this many pixels per inch look soft when printed.
pub const LOW_RESOLUTION_PPI: f64 = 150.0;

/// one font as the report lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontLine {
    pub name: String,
    pub embedded: bool,
    pub kind: String,
}

/// the boxes of one page, left bottom right top.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageBoxes {
    pub page: u32,
    pub media: Option<[f64; 4]>,
    pub crop: Option<[f64; 4]>,
    pub trim: Option<[f64; 4]>,
    pub bleed: Option<[f64; 4]>,
    pub rotate: i64,
}

/// what part of the file the bytes go to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeLine {
    /// "pictures", "fonts", "page content", "other streams", "objects"
    pub part: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    /// problems with the file's structure
    pub structure: Vec<Finding>,
    pub fonts: Vec<FontLine>,
    pub images: Vec<PlacedImage>,
    pub pages: Vec<PageBoxes>,
    /// largest first
    pub size: Vec<SizeLine>,
    /// features that viewers or printers may show differently
    pub portability: Vec<Finding>,
    pub summary: String,
}

/// checks a file. it is read, never changed.
pub fn preflight(bytes: &[u8]) -> StandardsResult<PreflightReport> {
    let doc = Document::load_mem(bytes)?;
    let mut structure = Findings::default();
    check_xref_offsets(bytes, &mut structure);
    check_dangling_references(&doc, &mut structure);
    let images = placed_images(&doc);
    let mut portability = Findings::default();
    check_portability(&doc, &images, &mut portability);
    let structure = structure.deduplicated();
    let portability = portability.deduplicated();
    let summary = summary(&structure, &portability);
    Ok(PreflightReport {
        fonts: fonts(&doc),
        pages: page_boxes(&doc),
        size: size_lines(&doc),
        images,
        structure,
        portability,
        summary,
    })
}

/// every in-use entry of a classic cross-reference table must point at
/// its object's "n g obj" (iso 32000 7.5.4). files with cross-reference
/// streams are checked by being loaded.
fn check_xref_offsets(bytes: &[u8], findings: &mut Findings) {
    let mut from = 0;
    while let Some(at) = find(bytes, from, b"\nxref") {
        from = at + 5;
        let mut lines = bytes[from..]
            .split(|byte| *byte == b'\n')
            .map(|line| line.strip_suffix(b"\r").unwrap_or(line));
        let mut number = 0u32;
        for line in lines.by_ref() {
            let text = String::from_utf8_lossy(line);
            let parts: Vec<&str> = text.split_whitespace().collect();
            match parts.as_slice() {
                [] => continue,
                ["trailer", ..] => break,
                // a subsection header: its first object number and a count
                [first, _] => number = first.parse().unwrap_or(0),
                [offset, generation, "n"] => {
                    let offset: usize = offset.parse().unwrap_or(usize::MAX);
                    let expected = format!(
                        "{number} {} obj",
                        generation
                            .trim_start_matches('0')
                            .parse::<u32>()
                            .unwrap_or(0)
                    );
                    let found = bytes
                        .get(offset..)
                        .is_some_and(|rest| rest.starts_with(expected.as_bytes()));
                    if !found {
                        findings.add(
                            "structure.xrefOffset",
                            "ISO 32000-1 7.5.4",
                            format!("The cross-reference table points object {number} to the wrong place."),
                            Fix::Automatic,
                        );
                    }
                    number += 1;
                }
                [_, _, "f"] => number += 1,
                _ => break,
            }
        }
    }
}

fn find(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| from + position)
}

/// references to objects the file doesn't have (iso 32000 7.3.10 treats
/// them as null, which readers handle differently).
fn check_dangling_references(doc: &Document, findings: &mut Findings) {
    let mut missing: Vec<(ObjectId, ObjectId)> = Vec::new();
    for (id, object) in &doc.objects {
        let mut references = Vec::new();
        collect_references(object, &mut references, 0);
        for target in references {
            if !doc.objects.contains_key(&target) {
                missing.push((*id, target));
            }
        }
    }
    for (holder, target) in missing {
        findings
            .add(
                "structure.danglingReference",
                "ISO 32000-1 7.3.10",
                format!(
                    "An object refers to object {} {}, which isn’t in the file.",
                    target.0, target.1
                ),
                Fix::Automatic,
            )
            .at(Some(holder));
    }
}

pub(crate) fn collect_references(object: &Object, out: &mut Vec<ObjectId>, depth: usize) {
    if depth > 32 {
        return;
    }
    match object {
        Object::Reference(id) => out.push(*id),
        Object::Array(items) => items
            .iter()
            .for_each(|item| collect_references(item, out, depth + 1)),
        Object::Dictionary(dict) => dict
            .iter()
            .for_each(|(_, value)| collect_references(value, out, depth + 1)),
        Object::Stream(stream) => stream
            .dict
            .iter()
            .for_each(|(_, value)| collect_references(value, out, depth + 1)),
        _ => {}
    }
}

/// the fonts the pages draw with (written in place or as objects).
fn fonts(doc: &Document) -> Vec<FontLine> {
    let mut out: Vec<FontLine> = Vec::new();
    crate::walk::each_drawing(doc, &mut |drawing| {
        let Some(fonts) = get_dict(doc, &drawing.resources, b"Font") else {
            return;
        };
        for (_, value) in fonts.iter() {
            let Ok(dict) = crate::objects::resolve(doc, value).as_dict() else {
                continue;
            };
            let facts = font_facts(doc, dict);
            let kind = get_name(doc, dict, b"Subtype")
                .map(|subtype| String::from_utf8_lossy(&subtype).into_owned())
                .unwrap_or_default();
            out.push(FontLine {
                name: facts.base_font,
                embedded: facts.embedded || facts.is_type3,
                kind,
            });
        }
    });
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup();
    out
}

fn page_boxes(doc: &Document) -> Vec<PageBoxes> {
    page_ids(doc)
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            let rect = |key: &[u8]| inherited(doc, id, key).and_then(|value| rect_of(doc, value));
            let own = |key: &[u8]| {
                doc.get_dictionary(id)
                    .ok()
                    .and_then(|page| get(doc, page, key))
                    .and_then(|value| rect_of(doc, value))
            };
            PageBoxes {
                page: index as u32,
                media: rect(b"MediaBox"),
                crop: rect(b"CropBox"),
                trim: own(b"TrimBox"),
                bleed: own(b"BleedBox"),
                rotate: inherited(doc, id, b"Rotate")
                    .and_then(|value| value.as_i64().ok())
                    .unwrap_or(0),
            }
        })
        .collect()
}

/// sums stream bytes (as stored) by what they hold.
fn size_lines(doc: &Document) -> Vec<SizeLine> {
    let mut totals = [0u64; 5];
    const PARTS: [&str; 5] = [
        "pictures",
        "fonts",
        "page content",
        "other streams",
        "objects",
    ];
    let font_files: Vec<ObjectId> = doc
        .objects
        .values()
        .filter_map(dict_of)
        .filter(|dict| get_name(doc, dict, b"Type").as_deref() == Some(b"FontDescriptor"))
        .flat_map(|dict| {
            [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
                .into_iter()
                .filter_map(|key| {
                    dict.get(key)
                        .ok()
                        .and_then(|value| value.as_reference().ok())
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let contents: Vec<ObjectId> = page_ids(doc)
        .into_iter()
        .flat_map(|page| doc.get_page_contents(page))
        .collect();
    for (id, object) in &doc.objects {
        let Object::Stream(stream) = object else {
            totals[4] += 32;
            continue;
        };
        let length = stream.content.len() as u64;
        let index = if get_name(doc, &stream.dict, b"Subtype").as_deref() == Some(b"Image") {
            0
        } else if font_files.contains(id) {
            1
        } else if contents.contains(id)
            || get_name(doc, &stream.dict, b"Subtype").as_deref() == Some(b"Form")
        {
            2
        } else {
            3
        };
        totals[index] += length;
    }
    let mut lines: Vec<SizeLine> = PARTS
        .iter()
        .zip(totals)
        .map(|(part, bytes)| SizeLine {
            part: (*part).to_owned(),
            bytes,
        })
        .collect();
    lines.sort_by_key(|line| std::cmp::Reverse(line.bytes));
    lines
}

/// features that viewers, phones or print shops may show differently.
fn check_portability(doc: &Document, images: &[PlacedImage], findings: &mut Findings) {
    let note = "Viewers and printers";
    for font in fonts(doc).into_iter().filter(|font| !font.embedded) {
        findings.add(
            "portability.fontNotEmbedded",
            note,
            format!("The font {} isn’t embedded, so other computers may show the text in a different font.", font.name),
            Fix::NeedsFont,
        );
    }
    for image in images
        .iter()
        .filter(|image| image.pixels_per_inch < LOW_RESOLUTION_PPI)
    {
        findings
            .add(
                "portability.lowResolution",
                note,
                format!("A picture is only {:.0} pixels per inch as placed, so it may look soft when printed.", image.pixels_per_inch),
                Fix::NotPossible,
            )
            .on_page(Some(image.page));
    }
    check_catalog_features(doc, findings);
    check_object_features(doc, findings);
    check_graphics_states(doc, findings);
}

/// graphics states may sit inside a page's resources rather than as
/// objects of their own, so they are read from every drawing too.
fn check_graphics_states(doc: &Document, findings: &mut Findings) {
    let mut overprint = false;
    let mut blending = false;
    crate::walk::each_drawing(doc, &mut |drawing| {
        let Some(states) = get_dict(doc, &drawing.resources, b"ExtGState") else {
            return;
        };
        for (_, state) in states.iter() {
            let Ok(state) = crate::objects::resolve(doc, state).as_dict() else {
                continue;
            };
            let sets = |key: &[u8]| matches!(get(doc, state, key), Some(Object::Boolean(true)));
            overprint |= sets(b"OP") || sets(b"op");
            blending |= get_name(doc, state, b"BM")
                .is_some_and(|mode| mode != b"Normal" && mode != b"Compatible");
        }
    });
    let note = "Viewers and printers";
    if overprint {
        findings.add(
            "portability.overprint",
            note,
            "Some colours are set to overprint, which screens usually don’t show but printers do."
                .into(),
            Fix::NotPossible,
        );
    }
    if blending {
        findings.add(
            "portability.blendModes",
            note,
            "Blend modes are used; older printers and some viewers flatten them differently."
                .into(),
            Fix::NotPossible,
        );
    }
}

fn check_catalog_features(doc: &Document, findings: &mut Findings) {
    let Ok(catalog) = doc.catalog() else { return };
    let form = get_dict(doc, catalog, b"AcroForm");
    let mut add = |rule: &str, message: &str| {
        findings.add(
            rule,
            "Viewers and printers",
            message.to_owned(),
            Fix::NotPossible,
        );
    };
    if form.is_some_and(|form| form.has(b"XFA")) {
        add(
            "portability.xfa",
            "The form is an XFA form, which most viewers other than Adobe’s can’t show.",
        );
    }
    if form.is_some_and(|form| {
        matches!(
            get(doc, form, b"NeedAppearances"),
            Some(Object::Boolean(true))
        )
    }) {
        add("portability.needAppearances", "The form asks each viewer to draw its fields, so they look different from viewer to viewer.");
    }
    let has_scripts = get_dict(doc, catalog, b"Names")
        .is_some_and(|names| names.has(b"JavaScript"))
        || catalog.has(b"AA");
    if has_scripts {
        add(
            "portability.scripts",
            "The document runs scripts, which phones and many viewers ignore.",
        );
    }
    if catalog.has(b"OCProperties") {
        add(
            "portability.layers",
            "The document has layers; some viewers and printers show hidden layers or ignore them.",
        );
    }
    if doc.trailer.has(b"Encrypt") {
        add(
            "portability.encrypted",
            "The document is encrypted; some tools can’t open or print it.",
        );
    }
}

fn check_object_features(doc: &Document, findings: &mut Findings) {
    let mut seen: Vec<&str> = Vec::new();
    for object in doc.objects.values() {
        let Some(dict) = dict_of(object) else {
            continue;
        };
        let subtype = get_name(doc, dict, b"Subtype").unwrap_or_default();
        let filters = crate::objects::names_in(doc, dict.get(b"Filter").ok());
        let issue = if matches!(get(doc, dict, b"OP"), Some(Object::Boolean(true)))
            || matches!(get(doc, dict, b"op"), Some(Object::Boolean(true)))
        {
            Some(("portability.overprint", "Some colours are set to overprint, which screens usually don’t show but printers do."))
        } else if dict.has(b"BM")
            && get_name(doc, dict, b"BM")
                .is_some_and(|mode| mode != b"Normal" && mode != b"Compatible")
        {
            Some((
                "portability.blendModes",
                "Blend modes are used; older printers and some viewers flatten them differently.",
            ))
        } else if matches!(
            subtype.as_slice(),
            b"3D" | b"RichMedia" | b"Movie" | b"Sound" | b"Screen"
        ) {
            Some((
                "portability.media",
                "The document has video, sound or 3D content, which most viewers don’t play.",
            ))
        } else if subtype == b"Type3" {
            Some(("portability.type3", "Some text uses Type 3 fonts, which can look rough at some sizes and can’t always be searched."))
        } else if filters
            .iter()
            .any(|filter| filter == b"JBIG2Decode" || filter == b"JPXDecode")
        {
            Some((
                "portability.imageFormats",
                "Some pictures use JBIG2 or JPEG 2000, which older viewers can’t show.",
            ))
        } else if matches!(
            get(doc, dict, b"ColorSpace")
                .and_then(|value| value.as_array().ok())
                .and_then(|items| items.first())
                .and_then(|first| first.as_name().ok()),
            Some(b"Separation") | Some(b"DeviceN")
        ) {
            Some((
                "portability.spotColours",
                "Spot colours are used; screens show an approximation of them.",
            ))
        } else {
            None
        };
        if let Some((rule, message)) = issue {
            if !seen.contains(&rule) {
                seen.push(rule);
                findings.add(
                    rule,
                    "Viewers and printers",
                    message.to_owned(),
                    Fix::NotPossible,
                );
            }
        }
    }
}

fn summary(structure: &[Finding], portability: &[Finding]) -> String {
    match (structure.is_empty(), portability.is_empty()) {
        (true, true) => "The file is sound and uses nothing that viewers commonly show differently.".to_owned(),
        (true, false) => format!("The file is sound. {} thing(s) may look different in some viewers or printers.", portability.len()),
        (false, _) => format!(
            "The file has {} structural problem(s); saving it again in Letters rewrites its structure.",
            structure.len()
        ),
    }
}
