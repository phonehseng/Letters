//! what later changes a signature allows: certification (docmdp, iso
//! 32000 12.8.2.2) with its three permission levels, and field locks
//! (fieldmdp, 12.8.2.4 and the /Lock dictionary of 12.7.5.5), so a form
//! can say "after this person signs, these fields can't change".

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use super::document::signature_dictionaries;
use crate::error::{FormError, FormResult};
use crate::objects::{check_document, get, get_dict, get_name, name, text_string};
use crate::read::{kind_of, FieldKind};
use crate::tree::{collect_fields, find_field, flags};

/// the docmdp permission levels (/P, iso 32000 table 257).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CertificationLevel {
    /// 1: no changes at all
    NoChanges,
    /// 2: filling in the form and signing
    FormFillingAndSigning,
    /// 3: as 2, and adding, changing or removing comments
    FormFillingSigningAndComments,
}

impl CertificationLevel {
    pub fn code(self) -> i64 {
        match self {
            CertificationLevel::NoChanges => 1,
            CertificationLevel::FormFillingAndSigning => 2,
            CertificationLevel::FormFillingSigningAndComments => 3,
        }
    }

    pub fn from_code(code: i64) -> Self {
        match code {
            1 => CertificationLevel::NoChanges,
            3 => CertificationLevel::FormFillingSigningAndComments,
            // iso 32000 12.8.2.2: a missing or unknown /P means 2
            _ => CertificationLevel::FormFillingAndSigning,
        }
    }

    /// what the level allows, in a sentence for the interface.
    pub fn sentence(self) -> &'static str {
        match self {
            CertificationLevel::NoChanges => "The author certified this document and allowed no changes.",
            CertificationLevel::FormFillingAndSigning => {
                "The author certified this document and allowed filling in the form and signing."
            }
            CertificationLevel::FormFillingSigningAndComments => {
                "The author certified this document and allowed filling in the form, signing and comments."
            }
        }
    }
}

/// which fields a signature locks once it is made (/Lock, /FieldMDP).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FieldLock {
    /// every field in the form
    All,
    /// only these fields
    Include { fields: Vec<String> },
    /// every field except these
    Exclude { fields: Vec<String> },
}

impl FieldLock {
    /// the /Action and /Fields entries both /Lock and /FieldMDP use.
    fn write_into(&self, dict: &mut Dictionary) {
        let (action, fields) = match self {
            FieldLock::All => ("All", None),
            FieldLock::Include { fields } => ("Include", Some(fields)),
            FieldLock::Exclude { fields } => ("Exclude", Some(fields)),
        };
        dict.set("Action", name(action));
        if let Some(fields) = fields {
            let names = fields.iter().map(|field| text_string(field)).collect();
            dict.set("Fields", Object::Array(names));
        }
    }

    /// reads /Action and /Fields back.
    pub(crate) fn read(doc: &Document, dict: &Dictionary) -> Option<FieldLock> {
        let fields = || -> Vec<String> {
            crate::objects::get_array(doc, dict, b"Fields")
                .into_iter()
                .flatten()
                .filter_map(|value| match crate::objects::resolve(doc, value) {
                    Object::String(bytes, _) => Some(crate::objects::decode_text_string(bytes)),
                    _ => None,
                })
                .collect()
        };
        match get_name(doc, dict, b"Action")?.as_slice() {
            b"All" => Some(FieldLock::All),
            b"Include" => Some(FieldLock::Include { fields: fields() }),
            b"Exclude" => Some(FieldLock::Exclude { fields: fields() }),
            _ => None,
        }
    }

    /// whether the lock covers a field (a name also covers its kids).
    pub fn covers(&self, field: &str) -> bool {
        let named = |names: &[String]| {
            names
                .iter()
                .any(|name| field == name || field.starts_with(&format!("{name}.")))
        };
        match self {
            FieldLock::All => true,
            FieldLock::Include { fields } => named(fields),
            FieldLock::Exclude { fields } => !named(fields),
        }
    }
}

/// says which fields an empty signature field locks once it is signed,
/// for forms where several people sign in turn.
pub fn set_signature_lock(
    doc: &mut Document,
    signature_field: &str,
    lock: Option<&FieldLock>,
) -> FormResult<()> {
    check_document(doc)?;
    let field = find_field(doc, signature_field)?;
    if kind_of(doc, &field) != Some(FieldKind::Signature) {
        return Err(FormError::NotASignatureField(signature_field.to_owned()));
    }
    if field.inherited(doc, b"V").is_some() {
        return Err(FormError::AlreadySigned(signature_field.to_owned()));
    }
    if let Some(FieldLock::Include { fields } | FieldLock::Exclude { fields }) = lock {
        for name in fields {
            find_field(doc, name)?;
        }
    }
    let dict = doc.get_dictionary_mut(field.id)?;
    match lock {
        Some(lock) => {
            let mut lock_dict = Dictionary::new();
            lock_dict.set("Type", name("SigFieldLock"));
            lock.write_into(&mut lock_dict);
            dict.set("Lock", Object::Dictionary(lock_dict));
        }
        None => {
            dict.remove(b"Lock");
        }
    }
    Ok(())
}

/// the lock a signature field asks for, if any.
pub fn signature_lock(doc: &Document, signature_field: &str) -> FormResult<Option<FieldLock>> {
    let field = find_field(doc, signature_field)?;
    let dict = doc.get_dictionary(field.id)?;
    Ok(get_dict(doc, dict, b"Lock").and_then(|lock| FieldLock::read(doc, lock)))
}

/// the lock written on a signature field (by its id), if any.
pub(crate) fn lock_of_field(doc: &Document, field: ObjectId) -> Option<FieldLock> {
    let dict = doc.get_dictionary(field).ok()?;
    get_dict(doc, dict, b"Lock").and_then(|lock| FieldLock::read(doc, lock))
}

/// the certification of a document: its level, read from the catalog's
/// /Perms /DocMDP signature.
pub(crate) fn certification_level(doc: &Document) -> Option<CertificationLevel> {
    let catalog = doc.catalog().ok()?;
    let perms = get_dict(doc, catalog, b"Perms")?;
    let signature = get_dict(doc, perms, b"DocMDP")?;
    reference_level(doc, signature)
}

/// the /P of a signature dictionary's docmdp reference.
pub(crate) fn reference_level(
    doc: &Document,
    signature: &Dictionary,
) -> Option<CertificationLevel> {
    for reference in crate::objects::get_array(doc, signature, b"Reference")
        .into_iter()
        .flatten()
    {
        let Ok(reference) = crate::objects::resolve(doc, reference).as_dict() else {
            continue;
        };
        if get_name(doc, reference, b"TransformMethod").as_deref() != Some(b"DocMDP") {
            continue;
        }
        let level = get_dict(doc, reference, b"TransformParams")
            .and_then(|params| get(doc, params, b"P"))
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(2);
        return Some(CertificationLevel::from_code(level));
    }
    None
}

/// refuses signatures the document's certification doesn't allow, and a
/// certification that wouldn't be the first signature (iso 32000 12.8.2.2:
/// only the first signature may be a certification).
pub(crate) fn check_signing_allowed(
    doc: &Document,
    certify: Option<CertificationLevel>,
) -> FormResult<()> {
    if certification_level(doc) == Some(CertificationLevel::NoChanges) {
        return Err(FormError::CertifiedNoChanges);
    }
    if certify.is_some() && !signature_dictionaries(doc).is_empty() {
        return Err(FormError::CertifyFirst);
    }
    Ok(())
}

/// the /Reference entries of a new signature: docmdp when certifying, and
/// fieldmdp when the signature locks fields.
pub(crate) fn transform_references(
    certify: Option<CertificationLevel>,
    lock: Option<&FieldLock>,
) -> Vec<Object> {
    let reference = |method: &str, params: Dictionary| {
        let mut dict = Dictionary::new();
        dict.set("Type", name("SigRef"));
        dict.set("TransformMethod", name(method));
        dict.set("TransformParams", Object::Dictionary(params));
        Object::Dictionary(dict)
    };
    let params = || {
        let mut dict = Dictionary::new();
        dict.set("Type", name("TransformParams"));
        // the version of the transform rules (iso 32000 tables 257 and 259)
        dict.set("V", name("1.2"));
        dict
    };
    let mut out = Vec::new();
    if let Some(level) = certify {
        let mut dict = params();
        dict.set("P", Object::Integer(level.code()));
        out.push(reference("DocMDP", dict));
    }
    if let Some(lock) = lock {
        let mut dict = params();
        lock.write_into(&mut dict);
        out.push(reference("FieldMDP", dict));
    }
    out
}

/// points the catalog's /Perms /DocMDP at the certifying signature.
pub(crate) fn mark_certified(doc: &mut Document, signature: ObjectId) -> FormResult<()> {
    let catalog_id = crate::objects::catalog_id(doc)?;
    let mut perms = Dictionary::new();
    perms.set("DocMDP", Object::Reference(signature));
    doc.get_dictionary_mut(catalog_id)?
        .set("Perms", Object::Dictionary(perms));
    Ok(())
}

/// makes the fields a lock covers read-only, in the same update as the
/// signature, the way acrobat does, so they can't be filled any more.
/// signature fields are left alone: later signers still sign theirs.
pub(crate) fn lock_fields(doc: &mut Document, lock: &FieldLock) -> FormResult<Vec<String>> {
    let mut locked = Vec::new();
    for field in collect_fields(doc) {
        if !lock.covers(&field.name) || kind_of(doc, &field) == Some(FieldKind::Signature) {
            continue;
        }
        let current = field.flags(doc);
        doc.get_dictionary_mut(field.id)?
            .set("Ff", Object::Integer((current | flags::READ_ONLY) as i64));
        locked.push(field.name.clone());
    }
    Ok(locked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_cover_fields_and_their_kids() {
        let include = FieldLock::Include {
            fields: vec!["address".into()],
        };
        assert!(include.covers("address.city"));
        assert!(!include.covers("addressee"));
        let exclude = FieldLock::Exclude {
            fields: vec!["notes".into()],
        };
        assert!(exclude.covers("name") && !exclude.covers("notes"));
    }

    #[test]
    fn unknown_levels_mean_form_filling() {
        assert_eq!(
            CertificationLevel::from_code(7),
            CertificationLevel::FormFillingAndSigning
        );
        assert_eq!(CertificationLevel::from_code(1).code(), 1);
    }
}
