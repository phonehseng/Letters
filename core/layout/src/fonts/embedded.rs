//! fonts carried inside a docx (word's "embed fonts in the file").
//!
//! the font table (word/fontTable.xml) lists each family with its
//! embedded faces (w:embedRegular, w:embedBold...), each a part holding an
//! obfuscated font (ecma-376 part 1, 17.8.1): the first 32 bytes are xored
//! with a key made from the w:fontKey guid. the office model carries these
//! parts unread; this module turns them back into fonts the layout can use.

use std::sync::Arc;

use letters_office::model::Document;

use crate::style::preserved;

/// one embedded face: family, style and the plain font bytes.
#[derive(Clone, Debug)]
pub struct EmbeddedFace {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub bytes: Arc<Vec<u8>>,
}

/// every face embedded in a document's font table.
pub fn embedded_faces(document: &Document) -> Vec<EmbeddedFace> {
    let carry = &document.carry;
    let Some(table) = carry
        .parts
        .iter()
        .find(|part| part.name.ends_with("fontTable.xml"))
    else {
        return Vec::new();
    };
    let Ok(xml) = std::str::from_utf8(&table.bytes) else {
        return Vec::new();
    };
    let folder = table.name.rsplit_once('/').map_or("", |(folder, _)| folder);
    let relationships = font_table_relationships(document, &table.name, folder);
    let mut faces = Vec::new();
    let mut family = String::new();
    for tag in preserved::tags(xml) {
        if tag.name == "w:font" {
            family = tag.attr("w:name").unwrap_or_default().to_string();
            continue;
        }
        let (bold, italic) = match tag.name.as_str() {
            "w:embedRegular" => (false, false),
            "w:embedBold" => (true, false),
            "w:embedItalic" => (false, true),
            "w:embedBoldItalic" => (true, true),
            _ => continue,
        };
        let (Some(id), Some(key)) = (tag.attr("r:id"), tag.attr("w:fontKey")) else {
            continue;
        };
        let Some((_, target)) = relationships.iter().find(|(rid, _)| rid == id) else {
            continue;
        };
        let part_name = resolve(folder, target);
        let Some(part) = carry.parts.iter().find(|part| part.name == part_name) else {
            continue;
        };
        if let Some(bytes) = deobfuscate(&part.bytes, key) {
            faces.push(EmbeddedFace {
                family: family.clone(),
                bold,
                italic,
                bytes: Arc::new(bytes),
            });
        }
    }
    faces
}

/// the font table's relationships (id, target): the model's own list, or
/// the relationships part carried unread beside it.
fn font_table_relationships(
    document: &Document,
    table: &str,
    folder: &str,
) -> Vec<(String, String)> {
    let carry = &document.carry;
    let known: Vec<(String, String)> = carry
        .relationships(table)
        .iter()
        .map(|r| (r.id.clone(), r.target.clone()))
        .collect();
    if !known.is_empty() {
        return known;
    }
    let file = table.rsplit('/').next().unwrap_or(table);
    let rels_name = if folder.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{folder}/_rels/{file}.rels")
    };
    let Some(part) = carry.parts.iter().find(|part| part.name == rels_name) else {
        return Vec::new();
    };
    let xml = String::from_utf8_lossy(&part.bytes);
    preserved::tags(&xml)
        .into_iter()
        .filter(|tag| tag.name.ends_with("Relationship"))
        .filter_map(|tag| Some((tag.attr("Id")?.to_string(), tag.attr("Target")?.to_string())))
        .collect()
}

/// a relationship target relative to the part's folder.
fn resolve(folder: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_string();
    }
    let mut parts: Vec<&str> = if folder.is_empty() {
        Vec::new()
    } else {
        folder.split('/').collect()
    };
    for piece in target.split('/') {
        match piece {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// undoes the obfuscation: the guid's 16 bytes, read from its last hex
/// pair to its first, xored over the first 32 bytes of the file.
pub fn deobfuscate(data: &[u8], guid: &str) -> Option<Vec<u8>> {
    let hex: String = guid.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 || data.len() < 32 {
        return None;
    }
    let key: Vec<u8> = (0..16)
        .rev()
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    let mut out = data.to_vec();
    for (index, byte) in out.iter_mut().take(32).enumerate() {
        *byte ^= key[index % 16];
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obfuscation_round_trips() {
        let guid = "{29FE789F-5522-4188-961C-9BDDB91AB2CA}";
        let font: Vec<u8> = (0..64u8).collect();
        let hidden = deobfuscate(&font, guid).unwrap();
        assert_ne!(hidden, font);
        // xor twice gives the original back; bytes past 32 are untouched
        assert_eq!(deobfuscate(&hidden, guid).unwrap(), font);
        assert_eq!(hidden[40], 40);
        // the first byte is xored with the guid's last byte, 0xca
        assert_eq!(hidden[0], 0xca);
    }

    #[test]
    fn targets_resolve_from_the_part_folder() {
        assert_eq!(
            resolve("word", "fonts/font1.odttf"),
            "word/fonts/font1.odttf"
        );
        assert_eq!(resolve("word", "../media/x.ttf"), "media/x.ttf");
        assert_eq!(resolve("word", "/word/fonts/a.odttf"), "word/fonts/a.odttf");
    }
}
