//! reads and updates the two xmp metadata entries this crate cares
//! about: the title (dc:title) and the pdf/ua identifier (pdfuaid:part).
//!
//! xmp is xml stored in a stream on the catalog. a full xml library would
//! be overkill for two entries, so this works on the text: it removes the
//! old entries and adds one new description holding the new ones, which
//! keeps everything else in the packet exactly as it was.

use lopdf::{Dictionary, Document, Object, Stream};

use crate::error::A11yError;
use crate::objects::{catalog_id, dict_mut, get, get_reference};

const DC: &str = "http://purl.org/dc/elements/1.1/";
const PDFUAID: &str = "http://www.aiim.org/pdfua/ns/id/";

/// the xmp packet as text, when the document has one.
pub fn read_packet(doc: &Document) -> Option<String> {
    let catalog = doc.catalog().ok()?;
    let stream = get(doc, catalog, b"Metadata")?.as_stream().ok()?;
    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// the title from dc:title, when it is there and not empty.
pub fn title(packet: &str) -> Option<String> {
    let block = element_text(packet, "dc:title")?;
    let item = element_text(block, "rdf:li").unwrap_or(block);
    let text = unescape(item.trim());
    (!text.is_empty()).then_some(text)
}

/// the pdf/ua part number, in element or attribute form.
pub fn pdfua_part(packet: &str) -> Option<String> {
    if let Some(text) = element_text(packet, "pdfuaid:part") {
        return Some(text.trim().to_owned());
    }
    let start = packet.find("pdfuaid:part=")? + "pdfuaid:part=".len();
    let quote = packet[start..].chars().next()?;
    let rest = &packet[start + quote.len_utf8()..];
    Some(rest[..rest.find(quote)?].to_owned())
}

/// the text between `<name …>` and `</name>`.
fn element_text<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = text.find(&format!("<{name}"))?;
    let after_tag = open + text[open..].find('>')? + 1;
    if text[..after_tag].ends_with("/>") {
        return None;
    }
    let close = after_tag + text[after_tag..].find(&format!("</{name}>"))?;
    Some(&text[after_tag..close])
}

/// takes out every `<name …>…</name>` element and every `name="…"` attribute.
fn remove_entry(packet: &str, name: &str) -> String {
    let mut text = packet.to_owned();
    let open = format!("<{name}");
    let close = format!("</{name}>");
    while let Some(start) = text.find(&open) {
        let Some(tag_end) = text[start..].find('>').map(|i| start + i + 1) else {
            break;
        };
        let end = if text[..tag_end].ends_with("/>") {
            tag_end
        } else {
            match text[tag_end..].find(&close) {
                Some(i) => tag_end + i + close.len(),
                None => break,
            }
        };
        text.replace_range(start..end, "");
    }
    let attribute = format!("{name}=");
    while let Some(start) = text.find(&attribute) {
        let value_start = start + attribute.len();
        let Some(quote) = text[value_start..].chars().next() else {
            break;
        };
        let Some(length) = text[value_start + 1..].find(quote) else {
            break;
        };
        text.replace_range(start..value_start + 1 + length + 1, "");
    }
    text
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// what the packet should say. `None` leaves an entry as it is.
pub struct XmpUpdate<'a> {
    pub title: Option<&'a str>,
    pub pdfua_part: Option<u32>,
}

/// the packet text with the new entries in place.
pub fn updated_packet(old: Option<&str>, update: &XmpUpdate) -> String {
    let title_text = update
        .title
        .map(str::to_owned)
        .or_else(|| old.and_then(title));
    let part = update
        .pdfua_part
        .map(|part| part.to_string())
        .or_else(|| old.and_then(pdfua_part));
    let mut description = String::from("<rdf:Description rdf:about=\"\"");
    description.push_str(&format!(" xmlns:dc=\"{DC}\" xmlns:pdfuaid=\"{PDFUAID}\">"));
    if let Some(title) = &title_text {
        description.push_str(&format!(
            "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>",
            escape(title)
        ));
    }
    if let Some(part) = &part {
        description.push_str(&format!("<pdfuaid:part>{}</pdfuaid:part>", escape(part)));
    }
    description.push_str("</rdf:Description>\n");

    let cleaned = old.map(|packet| remove_entry(&remove_entry(packet, "dc:title"), "pdfuaid:part"));
    match cleaned {
        Some(packet) if packet.contains("</rdf:RDF>") => {
            let at = packet.rfind("</rdf:RDF>").unwrap();
            let mut text = packet;
            text.insert_str(at, &description);
            text
        }
        _ => format!(
            "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
             <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
             <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
             {description}</rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>"
        ),
    }
}

/// writes the updated packet into the document, replacing the old stream.
pub fn update(doc: &mut Document, update: &XmpUpdate) -> Result<(), A11yError> {
    let packet = updated_packet(read_packet(doc).as_deref(), update);
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Metadata".to_vec()));
    dict.set("Subtype", Object::Name(b"XML".to_vec()));
    // metadata stays uncompressed so other tools can find it by scanning
    let stream = Stream::new(dict, packet.into_bytes()).with_compression(false);
    let catalog = catalog_id(doc)?;
    let existing = get_reference(dict_mut(doc, catalog)?, b"Metadata");
    match existing {
        Some(id) if doc.objects.contains_key(&id) => {
            doc.objects.insert(id, Object::Stream(stream));
        }
        _ => {
            let id = doc.add_object(stream);
            dict_mut(doc, catalog)?.set("Metadata", Object::Reference(id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKET: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:CreatorTool="Writer"/>
<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title><rdf:Alt><rdf:li xml:lang="x-default">Old &amp; tired</rdf:li></rdf:Alt></dc:title></rdf:Description>
</rdf:RDF></x:xmpmeta>"#;

    #[test]
    fn reads_the_title() {
        assert_eq!(title(PACKET).as_deref(), Some("Old & tired"));
    }

    #[test]
    fn replacing_the_title_keeps_everything_else() {
        let packet = updated_packet(
            Some(PACKET),
            &XmpUpdate {
                title: Some("New <title>"),
                pdfua_part: None,
            },
        );
        assert_eq!(title(&packet).as_deref(), Some("New <title>"));
        assert!(packet.contains("xmp:CreatorTool=\"Writer\""));
        assert_eq!(packet.matches("<dc:title>").count(), 1);
    }

    #[test]
    fn the_pdfua_part_is_added_and_the_title_kept() {
        let packet = updated_packet(
            Some(PACKET),
            &XmpUpdate {
                title: None,
                pdfua_part: Some(1),
            },
        );
        assert_eq!(pdfua_part(&packet).as_deref(), Some("1"));
        assert_eq!(title(&packet).as_deref(), Some("Old & tired"));
    }

    #[test]
    fn attribute_form_entries_are_read_and_replaced() {
        let old = r#"<rdf:RDF><rdf:Description pdfuaid:part="1"/></rdf:RDF>"#;
        assert_eq!(pdfua_part(old).as_deref(), Some("1"));
        let packet = updated_packet(
            Some(old),
            &XmpUpdate {
                title: Some("T"),
                pdfua_part: None,
            },
        );
        assert_eq!(packet.matches("pdfuaid:part").count(), 2, "{packet}");
    }

    #[test]
    fn a_new_packet_is_made_when_there_is_none() {
        let packet = updated_packet(
            None,
            &XmpUpdate {
                title: Some("Essay"),
                pdfua_part: Some(1),
            },
        );
        assert!(packet.starts_with("<?xpacket"));
        assert_eq!(title(&packet).as_deref(), Some("Essay"));
    }
}
