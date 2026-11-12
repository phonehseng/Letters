//! the audit trail as pages a person can read, added at the end of the
//! document when the person chooses. the pages are a copy for reading; the
//! embedded file stays the record that is checked.
//!
//! adding pages changes page content, which a certification forbids, so
//! add them before certifying or signing.

use lopdf::{dictionary, Document, Object, Stream};

use super::entry::{AuditEntry, AuditEvent};
use super::store::read_entries;
use super::AUDIT_TRAIL_NOTE;
use crate::appearance::standard_font_dict;
use crate::error::{FormError, FormResult};
use crate::fonts::StandardFont;
use crate::objects::{check_document, page_ids};
use crate::signing::write_update;
use crate::text::{literal_string, win_ansi_byte};

/// a us letter page, 612 by 792 points.
const PAGE_SIZE: (f32, f32) = (612.0, 792.0);
const MARGIN: f32 = 54.0;
const FONT_SIZE: f32 = 9.0;
const LEADING: f32 = 12.0;
/// characters per line at 9 points of helvetica across the text width
/// (about 4.6 points per average character).
const LINE_CHARACTERS: usize = 100;

/// adds pages listing every audit entry at the end of the document, as an
/// incremental update.
pub fn add_audit_page(doc_bytes: &[u8]) -> FormResult<Vec<u8>> {
    let before = Document::load_mem(doc_bytes)?;
    check_document(&before)?;
    let entries = read_entries(&before)?;
    if entries.is_empty() {
        return Err(FormError::AuditFailed(
            "this document has no audit trail yet".into(),
        ));
    }
    let mut after = before.clone();
    let lines = text_lines(&entries);
    let per_page = ((PAGE_SIZE.1 - 2.0 * MARGIN) / LEADING) as usize;
    for chunk in lines.chunks(per_page) {
        add_text_page(&mut after, chunk)?;
    }
    Ok(write_update(doc_bytes, &before, &after, &[])?.bytes)
}

fn text_lines(entries: &[AuditEntry]) -> Vec<String> {
    let mut lines = vec!["Audit trail".to_owned(), String::new()];
    lines.extend(wrap(AUDIT_TRAIL_NOTE));
    lines.push(String::new());
    for entry in entries {
        lines.extend(wrap(&format!(
            "{}. {} - {}",
            entry.sequence,
            entry.time,
            headline(entry)
        )));
        for detail in details(&entry.event) {
            lines.extend(wrap(&format!("    {detail}")));
        }
        lines.push(format!(
            "    entry hash {}",
            &entry.hash[..16.min(entry.hash.len())]
        ));
    }
    lines
}

fn headline(entry: &AuditEntry) -> String {
    let actor = &entry.actor;
    match &entry.event {
        AuditEvent::Created => format!("created by {actor}"),
        AuditEvent::Prepared { .. } => format!("prepared by {actor}"),
        AuditEvent::Signed { signer, field, .. } => format!("signed by {signer} in \"{field}\""),
        AuditEvent::Saved => format!("saved by {actor}"),
        AuditEvent::Timestamped { authority } => format!("time stamped by {authority}"),
        AuditEvent::Note { text } => format!("{actor}: {text}"),
    }
}

fn details(event: &AuditEvent) -> Vec<String> {
    match event {
        AuditEvent::Signed {
            certificate_subject,
            certificate_fingerprint,
            signing_time,
            time_source,
            reason,
            location,
            ..
        } => {
            let mut out = vec![
                format!("certificate {certificate_subject}"),
                format!("certificate sha-256 {certificate_fingerprint}"),
                format!("signed {signing_time}, time from {time_source}"),
            ];
            if let Some(reason) = reason {
                out.push(format!("reason: {reason}"));
            }
            if let Some(location) = location {
                out.push(format!("location: {location}"));
            }
            out
        }
        AuditEvent::Prepared { note: Some(note) } => vec![note.clone()],
        _ => Vec::new(),
    }
}

/// breaks text at spaces into lines of at most `LINE_CHARACTERS`.
fn wrap(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > LINE_CHARACTERS {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    lines
}

fn add_text_page(doc: &mut Document, lines: &[String]) -> FormResult<()> {
    let pages_id = doc
        .catalog()?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| FormError::BadPdf("the file has no page tree".into()))?;
    let font = doc.add_object(standard_font_dict(StandardFont::Helvetica));
    let mut content = format!(
        "BT /F1 {FONT_SIZE} Tf {LEADING} TL {MARGIN} {} Td\n",
        PAGE_SIZE.1 - MARGIN
    );
    for line in lines {
        // letters outside winansi show as "?" in this reading copy only
        let bytes: Vec<u8> = line
            .chars()
            .map(|c| win_ansi_byte(c).unwrap_or(b'?'))
            .collect();
        content.push_str(&format!("{} Tj T*\n", literal_string(&bytes)));
    }
    content.push_str("ET\n");
    let contents = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => vec![0.into(), 0.into(), PAGE_SIZE.0.into(), PAGE_SIZE.1.into()],
        "Contents" => Object::Reference(contents),
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } },
    });
    let count = page_ids(doc).len() as i64 + 1;
    let pages = doc.get_dictionary_mut(pages_id)?;
    let mut kids = pages
        .get(b"Kids")
        .and_then(Object::as_array)
        .cloned()
        .unwrap_or_default();
    kids.push(Object::Reference(page));
    pages.set("Kids", Object::Array(kids));
    pages.set("Count", Object::Integer(count));
    Ok(())
}
