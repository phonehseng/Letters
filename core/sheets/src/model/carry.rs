//! what a workbook carries from the file it was read from, so saving it
//! back loses nothing the model does not understand.
//!
//! - whole package parts the model does not read (drawings, charts,
//!   pictures, pivot tables, slicers, macros, the theme, custom xml) are
//!   kept byte for byte with their content types and relationships
//! - elements of a rewritten part the model does not read are kept as
//!   their original xml, by element name, so the writer puts them back
//!   in schema order
//! - elements the model does read keep their original xml too, with a
//!   fingerprint of what the writer made of them when the file was read;
//!   if the writer would make the same xml again (nothing changed), the
//!   original is written instead, byte for byte

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceFormat {
    Xlsx,
    Ods,
}

/// one package part kept as it was.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarriedPart {
    /// path inside the zip, no leading slash
    pub name: String,
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
}

/// one child element of a rewritten part.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    /// the element's local name ("sheetViews", "extLst")
    pub name: String,
    /// its original xml
    pub xml: String,
    /// for elements the model reads: a fingerprint of what the writer
    /// produced from the model right after reading. none for elements
    /// the model does not read, which are always written as they were
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<u64>,
}

/// a package relationship.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub id: String,
    /// the full type uri
    pub kind: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// an element the model reads only partly: (element name, its other
/// attributes, its other children as xml)
pub type ElementExtras = (String, Vec<(String, String)>, String);

/// what a sheet carries from an xlsx or ods file.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetCarry {
    pub source: Option<SourceFormat>,
    /// the worksheet part it came from ("xl/worksheets/sheet1.xml")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// the root element's attributes (namespace declarations, mc:Ignorable)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_attributes: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<Section>,
    /// the sheet's relationships that are written back as they were
    /// (drawings, printer settings, pivot tables...)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,
    /// the notes part and its legacy drawing, with a fingerprint of the
    /// notes they hold, reused while the notes are unchanged
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<CarriedComments>,
    /// excel's sheetId, kept so links into the file stay valid
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet_id: Option<u32>,
    /// table parts by table id: (part name, original xml, fingerprint)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<(u32, String, String, u64)>,
    /// attributes and unknown children of elements the model reads only
    /// partly (sheetPr, sheetView, sheetFormatPr, pageSetup): (element,
    /// attributes, children xml)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub element_extras: Vec<ElementExtras>,
    /// ods: the table element's attributes and unknown children
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ods_attributes: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarriedComments {
    pub part: String,
    pub xml: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vml_part: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vml_xml: Option<String>,
    pub fingerprint: u64,
}

/// what the workbook as a whole carries.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Carry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceFormat>,
    /// parts copied byte for byte
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<CarriedPart>,
    /// content types of the carried parts: (part or extension, type, is_default)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_types: Vec<(String, String, bool)>,
    /// the main part's content type (it says whether macros are inside)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workbook_root_attributes: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workbook_sections: Vec<Section>,
    /// workbook relationships written back as they were
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workbook_relationships: Vec<Relationship>,
    /// package-level relationships (core and app properties, thumbnail)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package_relationships: Vec<Relationship>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles_root_attributes: Vec<(String, String)>,
    /// styles.xml children the model does not rebuild (named styles,
    /// table styles, colours, extensions)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles_sections: Vec<Section>,
    /// the core properties part as read and a fingerprint of the
    /// metadata it held
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_properties: Option<(String, u64)>,
    /// ods: the parts that hold settings, meta and styles, by name
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ods_sections: Vec<Section>,
}

/// bytes as base64 text in json.
mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        decode(&text).ok_or_else(|| serde::de::Error::custom("not base64"))
    }

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() / 4 * 3);
        let mut buffer = 0u32;
        let mut bits = 0;
        for byte in text
            .bytes()
            .filter(|b| *b != b'=' && !b.is_ascii_whitespace())
        {
            let value = ALPHABET.iter().position(|a| *a == byte)? as u32;
            buffer = (buffer << 6) | value;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buffer >> bits) as u8);
            }
        }
        Some(out)
    }

    #[cfg(test)]
    #[test]
    fn round_trips() {
        for sample in [&b""[..], b"a", b"ab", b"abc", b"\x00\xff\x10hello"] {
            assert_eq!(decode(&encode(sample)).unwrap(), sample);
        }
        assert_eq!(encode(b"Man"), "TWFu");
    }
}

/// a stable 64 bit fingerprint (fnv-1a) of some text.
pub fn fingerprint(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}
