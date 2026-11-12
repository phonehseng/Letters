//! document-wide settings: language, title, showing the title, and the
//! "this file is tagged" flag.

use lopdf::{Dictionary, Document, Object};

use crate::error::A11yError;
use crate::objects::{catalog_id, dict_mut, ensure_editable, get, text_object};
use crate::report::FixResult;
use crate::rules::is_valid_language_tag;
use crate::xmp::{self, XmpUpdate};

/// sets the document's language, like "en-US".
pub fn set_language(doc: &mut Document, language: &str) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let language = language.trim();
    if !is_valid_language_tag(language) {
        return Err(A11yError::InvalidLanguage(language.to_owned()));
    }
    let catalog = catalog_id(doc)?;
    dict_mut(doc, catalog)?.set("Lang", text_object(language));
    Ok(FixResult::one(format!(
        "Set the document’s language to {language}."
    )))
}

/// sets the title in the document information and the xmp metadata, and
/// makes readers show it instead of the file name.
pub fn set_title(doc: &mut Document, title: &str) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let title = super::answer(title, "title")?;
    catalog_id(doc)?;
    set_info_title(doc, &title)?;
    xmp::update(
        doc,
        &XmpUpdate {
            title: Some(&title),
            pdfua_part: None,
        },
    )?;
    let mut result = FixResult::one(format!("Set the title to “{title}”."));
    result.changes.extend(show_title(doc)?.changes);
    Ok(result)
}

fn set_info_title(doc: &mut Document, title: &str) -> Result<(), A11yError> {
    match doc.trailer.get(b"Info").ok() {
        Some(Object::Reference(id)) if doc.get_dictionary(*id).is_ok() => {
            let id = *id;
            dict_mut(doc, id)?.set("Title", text_object(title));
        }
        Some(Object::Dictionary(_)) => {
            if let Ok(Object::Dictionary(info)) = doc.trailer.get_mut(b"Info") {
                info.set("Title", text_object(title));
            }
        }
        _ => {
            let mut info = Dictionary::new();
            info.set("Title", text_object(title));
            let id = doc.add_object(info);
            doc.trailer.set("Info", Object::Reference(id));
        }
    }
    Ok(())
}

/// makes pdf readers show the title in the window instead of the file name.
pub fn show_title(doc: &mut Document) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let catalog = catalog_id(doc)?;
    let preferences = match doc
        .get_dictionary(catalog)
        .ok()
        .and_then(|c| c.get(b"ViewerPreferences").ok())
    {
        Some(Object::Reference(id)) => Some(*id),
        _ => None,
    };
    match preferences {
        Some(id) if doc.get_dictionary(id).is_ok() => {
            dict_mut(doc, id)?.set("DisplayDocTitle", Object::Boolean(true));
        }
        _ => {
            let catalog_dict = dict_mut(doc, catalog)?;
            let mut preferences = match catalog_dict.get(b"ViewerPreferences") {
                Ok(Object::Dictionary(existing)) => existing.clone(),
                _ => Dictionary::new(),
            };
            preferences.set("DisplayDocTitle", Object::Boolean(true));
            catalog_dict.set("ViewerPreferences", Object::Dictionary(preferences));
        }
    }
    Ok(FixResult::one(
        "Made PDF readers show the title instead of the file name.",
    ))
}

/// turns on the "this file is tagged" flag (MarkInfo /Marked).
pub fn mark_as_tagged(doc: &mut Document) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let catalog = catalog_id(doc)?;
    if get(doc, crate::objects::catalog(doc)?, b"StructTreeRoot").is_none() {
        return Err(A11yError::NotTagged);
    }
    let reference = match doc
        .get_dictionary(catalog)
        .ok()
        .and_then(|c| c.get(b"MarkInfo").ok())
    {
        Some(Object::Reference(id)) if doc.get_dictionary(*id).is_ok() => Some(*id),
        _ => None,
    };
    match reference {
        Some(id) => dict_mut(doc, id)?.set("Marked", Object::Boolean(true)),
        None => {
            let catalog_dict = dict_mut(doc, catalog)?;
            let mut info = match catalog_dict.get(b"MarkInfo") {
                Ok(Object::Dictionary(existing)) => existing.clone(),
                _ => Dictionary::new(),
            };
            info.set("Marked", Object::Boolean(true));
            catalog_dict.set("MarkInfo", Object::Dictionary(info));
        }
    }
    Ok(FixResult::one("Marked the document as tagged."))
}
