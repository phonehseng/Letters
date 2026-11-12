//! removing or neutralising what pdf/a forbids, and listing every removal
//! so the person knows what changed: scripts and forbidden actions, event
//! actions, xfa, postscript, image flags, lzw compression, forbidden
//! annotations, attachments a level doesn't allow, and layer settings.

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::objects::{dict_of, get, get_dict, get_name, names_in, page_ids, resolve};
use crate::pdfa::level::PdfaLevel;
use crate::pdfa::{annotation_flags, forbidden_action, type_allowed};

/// what was taken out or changed, one sentence each.
#[derive(Default)]
pub(super) struct Removals {
    pub list: Vec<String>,
}

impl Removals {
    pub(super) fn note(&mut self, sentence: &str) {
        if !self.list.iter().any(|existing| existing == sentence) {
            self.list.push(sentence.to_owned());
        }
    }
}

pub(super) fn remove_forbidden(doc: &mut Document, level: PdfaLevel, removals: &mut Removals) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        clean_object(doc, id, level, removals);
    }
    clean_catalog(doc, level, removals);
    clean_annotations(doc, level, removals);
}

fn clean_object(doc: &mut Document, id: ObjectId, level: PdfaLevel, removals: &mut Removals) {
    let Some(dict) = doc.get_object(id).ok().and_then(dict_of).cloned() else {
        return;
    };
    let mut changed = dict.clone();
    if changed.remove(b"AA").is_some() {
        removals.note("Actions that ran on events (opening, closing, typing) were removed.");
    }
    for key in [b"A".as_slice(), b"OpenAction"] {
        let forbidden = get(doc, &dict, key)
            .and_then(|value| value.as_dict().ok())
            .and_then(|action| forbidden_action(doc, action, level));
        if forbidden.is_some() {
            changed.remove(key);
            removals.note(
                "Actions PDF/A forbids (scripts, launching programs, sounds, movies) were removed.",
            );
        }
    }
    clean_xobject(&mut changed, removals);
    let lzw = clean_lzw(doc, id, &mut changed, removals);
    if changed != dict || lzw {
        replace_dict(doc, id, changed);
    }
}

fn clean_xobject(dict: &mut Dictionary, removals: &mut Removals) {
    if matches!(dict.get(b"Interpolate"), Ok(Object::Boolean(true))) {
        dict.set("Interpolate", Object::Boolean(false));
        removals.note("Pictures no longer ask readers to smooth them.");
    }
    for key in [b"Alternates".as_slice(), b"OPI", b"Ref"] {
        if dict.remove(key).is_some() {
            removals.note("Alternative pictures and links to other files’ pages were removed.");
        }
    }
    let postscript = dict
        .get(b"Subtype2")
        .ok()
        .and_then(|value| value.as_name().ok())
        == Some(b"PS");
    if postscript {
        dict.remove(b"Subtype2");
        dict.remove(b"PS");
        removals.note("PostScript inside drawings was removed.");
    }
}

/// lzw streams are decoded and stored again with flate, which looks the
/// same. returns whether this stream is one that can be re-encoded.
fn clean_lzw(doc: &Document, id: ObjectId, dict: &mut Dictionary, removals: &mut Removals) -> bool {
    let filters = names_in(doc, dict.get(b"Filter").ok());
    if !filters.iter().any(|filter| filter == b"LZWDecode") {
        return false;
    }
    let Ok(Object::Stream(stream)) = doc.get_object(id) else {
        return false;
    };
    let decodable = stream.decompressed_content().is_ok();
    if decodable {
        removals.note("LZW compression was replaced with Flate compression.");
    }
    decodable
}

/// writes a changed dictionary back, re-encoding an lzw stream's data.
fn replace_dict(doc: &mut Document, id: ObjectId, dict: Dictionary) {
    let Ok(object) = doc.get_object_mut(id) else {
        return;
    };
    match object {
        Object::Stream(stream) => {
            let lzw = names_in_raw(&stream.dict)
                .iter()
                .any(|filter| filter == b"LZWDecode");
            let plain = if lzw {
                stream.decompressed_content().ok()
            } else {
                None
            };
            stream.dict = dict;
            if let Some(plain) = plain {
                stream.dict.remove(b"Filter");
                stream.dict.remove(b"DecodeParms");
                stream.set_plain_content(plain);
                let _ = stream.compress();
            }
        }
        Object::Dictionary(existing) => *existing = dict,
        _ => {}
    }
}

fn names_in_raw(dict: &Dictionary) -> Vec<Vec<u8>> {
    match dict.get(b"Filter") {
        Ok(Object::Name(name)) => vec![name.clone()],
        Ok(Object::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    }
}

fn clean_catalog(doc: &mut Document, level: PdfaLevel, removals: &mut Removals) {
    let Ok(catalog_id) = doc.trailer.get(b"Root").and_then(Object::as_reference) else {
        return;
    };
    let Ok(mut catalog) = doc.get_dictionary(catalog_id).cloned() else {
        return;
    };
    if let Some(mut names) = get_dict(doc, &catalog, b"Names").cloned() {
        if names.remove(b"JavaScript").is_some() {
            removals.note("Scripts that ran when the document opened were removed.");
        }
        let attachments_forbidden = level.part() < 3 && names.has(b"EmbeddedFiles");
        if attachments_forbidden {
            names.remove(b"EmbeddedFiles");
            catalog.remove(b"AF");
            removals.note(&format!(
                "Attachments were removed, because {} doesn’t allow them (PDF/A-3 does).",
                level.name()
            ));
        }
        catalog.set("Names", Object::Dictionary(names));
    }
    clean_form(doc, &catalog, removals);
    clean_layers(doc, &mut catalog, level, removals);
    if let Ok(target) = doc.get_dictionary_mut(catalog_id) {
        *target = catalog;
    }
}

fn clean_form(doc: &mut Document, catalog: &Dictionary, removals: &mut Removals) {
    let Ok(form_id) = catalog.get(b"AcroForm").and_then(Object::as_reference) else {
        return;
    };
    let Ok(form) = doc.get_dictionary_mut(form_id) else {
        return;
    };
    if form.remove(b"XFA").is_some() {
        removals.note("The form’s XFA copy was removed; its regular form fields stay.");
    }
    if form.remove(b"NeedAppearances").is_some() {
        removals.note("The form no longer asks readers to draw its fields; Letters drew them.");
    }
}

/// layer configurations need names and no automatic switching (19005-2 6.9).
fn clean_layers(
    doc: &Document,
    catalog: &mut Dictionary,
    level: PdfaLevel,
    removals: &mut Removals,
) {
    if level.is_part_one() {
        return;
    }
    let Some(mut properties) = get_dict(doc, catalog, b"OCProperties").cloned() else {
        return;
    };
    if let Some(mut default) = get_dict(doc, &properties, b"D").cloned() {
        if !default.has(b"Name") {
            default.set("Name", Object::string_literal("Default"));
        }
        default.remove(b"AS");
        properties.set("D", Object::Dictionary(default));
    }
    if properties.remove(b"Configs").is_some() {
        removals.note("Extra layer configurations were removed; the default one stays.");
    }
    catalog.set("OCProperties", Object::Dictionary(properties));
}

/// forbidden annotation types are removed; the rest print, show, are
/// opaque in part 1, and keep only their normal appearance.
fn clean_annotations(doc: &mut Document, level: PdfaLevel, removals: &mut Removals) {
    use annotation_flags::*;
    for page_id in page_ids(doc) {
        let annotations: Vec<Object> = doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|page| page.get(b"Annots").ok())
            .map(|value| resolve(doc, value).as_array().cloned().unwrap_or_default())
            .unwrap_or_default();
        let mut kept = Vec::new();
        for annotation in annotations {
            let Ok(id) = annotation.as_reference() else {
                kept.push(annotation);
                continue;
            };
            let Ok(mut dict) = doc.get_dictionary(id).cloned() else {
                continue;
            };
            let subtype = get_name(doc, &dict, b"Subtype").unwrap_or_default();
            if !type_allowed(&subtype, level) {
                removals.note(&format!(
                    "{} annotations were removed, because {} doesn’t allow them.",
                    String::from_utf8_lossy(&subtype),
                    level.name()
                ));
                continue;
            }
            if subtype != b"Popup" {
                let flags = dict
                    .get(b"F")
                    .ok()
                    .and_then(|value| value.as_i64().ok())
                    .unwrap_or(0);
                let fixed = (flags | PRINT) & !(INVISIBLE | HIDDEN | NO_VIEW | TOGGLE_NO_VIEW);
                if fixed != flags {
                    dict.set("F", Object::Integer(fixed));
                    removals.note("Hidden or non-printing annotations were set to show and print.");
                }
            }
            if level.is_part_one() && dict.remove(b"CA").is_some() {
                removals.note(
                    "See-through annotations were made opaque (PDF/A-1 forbids transparency).",
                );
            }
            if let Ok(Object::Dictionary(appearance)) = dict.get(b"AP").cloned() {
                if appearance.len() > 1 || !appearance.has(b"N") {
                    let mut only_normal = Dictionary::new();
                    if let Ok(normal) = appearance.get(b"N") {
                        only_normal.set("N", normal.clone());
                    }
                    dict.set("AP", Object::Dictionary(only_normal));
                    removals.note("Rollover and pressed looks of annotations were removed; the normal look stays.");
                }
            }
            if let Ok(target) = doc.get_dictionary_mut(id) {
                *target = dict;
            }
            kept.push(Object::Reference(id));
        }
        if let Ok(page) = doc.get_dictionary_mut(page_id) {
            if page.has(b"Annots") {
                page.set("Annots", Object::Array(kept));
            }
        }
    }
}

/// part 1: a page group alone isn't transparency; it is dropped when the
/// page draws nothing transparent. returns the pages it was dropped from.
pub(super) fn drop_page_groups(
    doc: &mut Document,
    transparent_pages: &[u32],
    removals: &mut Removals,
) {
    for (index, page_id) in page_ids(doc).into_iter().enumerate() {
        if transparent_pages.contains(&(index as u32)) {
            continue;
        }
        if let Ok(page) = doc.get_dictionary_mut(page_id) {
            if page.remove(b"Group").is_some() {
                removals.note("Unused transparency settings were removed from pages.");
            }
        }
    }
}

/// attachments in pdf/a-3 need a relationship and a media type, and must
/// be associated with the document through an /AF list (19005-3 6.8).
pub(super) fn describe_attachments(doc: &mut Document, removals: &mut Removals) {
    associate_attachments(doc, removals);
    let ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, object)| {
            dict_of(object).is_some_and(|dict| {
                dict.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"Filespec")
            })
        })
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        let Ok(spec) = doc.get_dictionary(id).cloned() else {
            continue;
        };
        let mut changed = spec.clone();
        if !changed.has(b"AFRelationship") {
            changed.set("AFRelationship", Object::Name(b"Unspecified".to_vec()));
        }
        if !changed.has(b"UF") {
            if let Ok(name) = spec.get(b"F") {
                changed.set("UF", name.clone());
            }
        }
        let file = get_dict(doc, &spec, b"EF")
            .and_then(|ef| ef.get(b"F").ok())
            .and_then(|f| f.as_reference().ok());
        if let Some(file) = file {
            if let Ok(Object::Stream(stream)) = doc.get_object_mut(file) {
                if !stream.dict.has(b"Subtype") {
                    stream.dict.set(
                        "Subtype",
                        Object::Name(b"application/octet-stream".to_vec()),
                    );
                }
            }
        }
        if changed != spec {
            if let Ok(target) = doc.get_dictionary_mut(id) {
                *target = changed;
            }
            removals
                .note("Attachments were given the relationship and media type PDF/A-3 asks for.");
        }
    }
}

/// lists every embedded file's specification in the catalog's /AF when no
/// /AF in the document lists it yet.
fn associate_attachments(doc: &mut Document, removals: &mut Removals) {
    let Ok(catalog_id) = doc.trailer.get(b"Root").and_then(Object::as_reference) else {
        return;
    };
    let associated = crate::pdfa::associated_files(doc);
    let listed: Vec<ObjectId> = doc
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| get_dict(doc, catalog, b"Names"))
        .and_then(|names| get_dict(doc, names, b"EmbeddedFiles"))
        .and_then(|tree| tree.get(b"Names").ok())
        .and_then(|value| resolve(doc, value).as_array().ok())
        .map(|pairs| {
            pairs
                .iter()
                .skip(1)
                .step_by(2)
                .filter_map(|value| value.as_reference().ok())
                .collect()
        })
        .unwrap_or_default();
    let missing: Vec<ObjectId> = listed
        .into_iter()
        .filter(|id| !associated.contains(id))
        .collect();
    if missing.is_empty() {
        return;
    }
    let Ok(catalog) = doc.get_dictionary_mut(catalog_id) else {
        return;
    };
    let mut list = match catalog.get(b"AF") {
        Ok(Object::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    list.extend(missing.into_iter().map(Object::Reference));
    catalog.set("AF", Object::Array(list));
    removals.note("Attachments were associated with the document, as PDF/A-3 asks.");
}
