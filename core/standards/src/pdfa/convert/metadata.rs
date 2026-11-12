//! the document-level parts of a conversion: xmp metadata with the pdf/a
//! identification, an info dictionary that agrees with it, a file
//! identifier, and an srgb output intent.

use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use sha2::{Digest, Sha256};

use super::icc::srgb_profile;
use crate::error::StandardsResult;
use crate::objects::{get_array, get_name, resolve};
use crate::pdfa::info_dictionary;
use crate::pdfa::level::PdfaLevel;
use crate::pdfa::xmp::{pdf_to_xmp_date, write_xmp, xmp_to_pdf_date, DocumentInfo};

/// the producer written into converted files.
const PRODUCER: &str = "Letters";

/// writes xmp metadata naming the level, and makes the info dictionary
/// agree with it (19005-1 6.7.3). the modification date becomes now.
pub(super) fn write_metadata(
    doc: &mut Document,
    level: PdfaLevel,
    now: &str,
) -> StandardsResult<()> {
    let mut info = info_dictionary(doc);
    info.producer = Some(PRODUCER.to_owned());
    info.modification_date = Some(now.to_owned());
    if info.creation_date.is_none() {
        info.creation_date = Some(now.to_owned());
    }
    // dates must survive the trip through xmp unchanged, so they are
    // written as xmp would read them back
    for date in [&mut info.creation_date, &mut info.modification_date] {
        if let Some(value) = date.as_deref().and_then(pdf_to_xmp_date) {
            *date = Some(xmp_to_pdf_date(&value));
        }
    }
    let packet = write_xmp(level.part(), level.conformance(), &info);
    let mut stream = Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        packet.into_bytes(),
    );
    // part 1 asks for uncompressed metadata; later parts allow either, and
    // plain text keeps it readable by tools that scan for xmp
    stream.allows_compression = false;
    let metadata = doc.add_object(stream);
    let catalog = doc.trailer.get(b"Root")?.as_reference()?;
    doc.get_dictionary_mut(catalog)?
        .set("Metadata", Object::Reference(metadata));
    write_info(doc, &info)
}

fn write_info(doc: &mut Document, info: &DocumentInfo) -> StandardsResult<()> {
    let mut dict = lopdf::Dictionary::new();
    let text = |value: &str| text_string(value);
    let entries = [
        ("Title", &info.title),
        ("Author", &info.author),
        ("Subject", &info.subject),
        ("Keywords", &info.keywords),
        ("Creator", &info.creator),
        ("Producer", &info.producer),
        ("CreationDate", &info.creation_date),
        ("ModDate", &info.modification_date),
    ];
    for (key, value) in entries {
        if let Some(value) = value {
            dict.set(key, text(value));
        }
    }
    let id = doc.add_object(Object::Dictionary(dict));
    doc.trailer.set("Info", Object::Reference(id));
    Ok(())
}

/// a pdf text string: plain bytes for ascii, utf-16 otherwise.
pub(crate) fn text_string(text: &str) -> Object {
    if text.is_ascii() {
        return Object::String(text.as_bytes().to_vec(), StringFormat::Literal);
    }
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

/// a file identifier (19005-1 6.1.3): kept if there is one, else made from
/// the hash of the document's objects so the same file gets the same id.
/// returns whether one was added.
pub(super) fn ensure_file_id(doc: &mut Document) -> bool {
    if doc.trailer.has(b"ID") {
        return false;
    }
    let mut hasher = Sha256::new();
    for (id, object) in &doc.objects {
        hasher.update(format!("{id:?}{object:?}").as_bytes());
    }
    let digest = hasher.finalize()[..16].to_vec();
    let id = Object::String(digest, StringFormat::Hexadecimal);
    doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
    true
}

/// adds an srgb output intent unless the document has a pdf/a one.
/// returns whether one was added.
pub(super) fn ensure_output_intent(doc: &mut Document) -> StandardsResult<bool> {
    let catalog_id = doc.trailer.get(b"Root")?.as_reference()?;
    let catalog = doc.get_dictionary(catalog_id)?;
    let has_pdfa_intent = get_array(doc, catalog, b"OutputIntents")
        .into_iter()
        .flatten()
        .filter_map(|intent| resolve(doc, intent).as_dict().ok())
        .any(|intent| get_name(doc, intent, b"S").as_deref() == Some(b"GTS_PDFA1"));
    if has_pdfa_intent {
        return Ok(false);
    }
    let profile = doc.add_object(Stream::new(dictionary! { "N" => 3 }, srgb_profile()));
    let intent = doc.add_object(dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFA1",
        "OutputConditionIdentifier" => Object::string_literal("sRGB IEC61966-2.1"),
        "Info" => Object::string_literal("sRGB IEC61966-2.1"),
        "DestOutputProfile" => Object::Reference(profile),
    });
    let catalog = doc.get_dictionary_mut(catalog_id)?;
    let mut intents = match catalog.get(b"OutputIntents") {
        Ok(Object::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    intents.push(Object::Reference(intent));
    catalog.set("OutputIntents", Object::Array(intents));
    Ok(true)
}
