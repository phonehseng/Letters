//! what a document carries from the file it was read from so that saving
//! it back loses nothing the model does not understand.
//!
//! three kinds of things are kept:
//! - whole package parts the model does not read (charts, custom xml,
//!   embedded fonts, the theme, macros...), byte for byte
//! - relationships that preserved xml points at, under their original ids
//! - the namespace declarations on the root element of each part that is
//!   rewritten, so preserved xml using those prefixes stays valid
//!
//! smaller unknown pieces (a run property, a shape) live next to where
//! they occurred, in `Preserved` fields and `Preserved` blocks and inlines.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceFormat {
    Docx,
    Odt,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Carry {
    /// which format the preserved xml is in. a writer for another format
    /// ignores it (only its text survives a conversion)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<SourceFormat>,
    /// package parts written back unchanged
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<CarriedPart>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub part_roots: Vec<PartRoot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub part_relationships: Vec<PartRelationships>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docx: Option<DocxCarry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub odt: Option<OdtCarry>,
}

impl Carry {
    pub fn part(&self, name: &str) -> Option<&CarriedPart> {
        self.parts
            .iter()
            .find(|part| part.name.eq_ignore_ascii_case(name))
    }

    /// the root attributes (mostly namespace declarations) a part had.
    pub fn root_attributes(&self, part: &str) -> Option<&[Attribute]> {
        self.part_roots
            .iter()
            .find(|root| root.part == part)
            .map(|root| root.attributes.as_slice())
    }

    pub fn relationships(&self, part: &str) -> &[Relationship] {
        self.part_relationships
            .iter()
            .find(|entry| entry.part == part)
            .map(|entry| entry.relationships.as_slice())
            .unwrap_or(&[])
    }
}

/// a package part kept byte for byte.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarriedPart {
    pub name: String,
    /// its content type in the package's list (docx) or manifest (odt)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(with = "crate::base64::serde_bytes")]
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartRoot {
    pub part: String,
    pub attributes: Vec<Attribute>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartRelationships {
    pub part: String,
    pub relationships: Vec<Relationship>,
}

/// one entry of a docx `.rels` file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub id: String,
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub external: bool,
}

/// an xml attribute, name as written in the file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

impl Attribute {
    pub fn new(name: &str, value: &str) -> Attribute {
        Attribute {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// docx specifics: where the rewritten parts lived and how they were
/// listed, so they go back to the same places.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocxCarry {
    /// "word/document.xml" in nearly every file
    pub main_part: String,
    /// differs for macro-enabled documents and templates
    pub main_content_type: String,
    /// extension to content type pairs from `[Content_Types].xml`
    #[serde(default)]
    pub defaults: Vec<ContentTypeDefault>,
    /// the package's own relationships (`_rels/.rels`)
    #[serde(default)]
    pub package_relationships: Vec<Relationship>,
    /// children of `w:document` other than the body (a page background)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_extras: Vec<String>,
    /// main-document relationships of kinds the model rewrites
    /// (styles, numbering, notes...) mapped to their part names
    #[serde(default)]
    pub part_names: Vec<Attribute>,
    /// core properties the model does not read (revision, category...)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub core_extras: Vec<String>,
}

impl DocxCarry {
    /// the part name used for a rewritten part kind ("styles"...).
    pub fn part_name(&self, kind: &str) -> Option<&str> {
        self.part_names
            .iter()
            .find(|entry| entry.name == kind)
            .map(|entry| entry.value.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentTypeDefault {
    pub extension: String,
    pub content_type: String,
}

/// odt specifics. automatic styles are kept as they were, under their
/// original names, because preserved xml may refer to them; the writer
/// gives its own automatic styles names that cannot clash.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OdtCarry {
    pub mimetype: String,
    #[serde(default)]
    pub content_automatic_styles: Vec<String>,
    #[serde(default)]
    pub styles_automatic_styles: Vec<String>,
    #[serde(default)]
    pub font_faces: Vec<String>,
    /// declarations at the start of the text body (variable and sequence
    /// declarations) kept as original xml
    #[serde(default)]
    pub body_prelude: Vec<String>,
    /// manifest entries for carried parts: path and media type
    #[serde(default)]
    pub manifest: Vec<Attribute>,
    /// meta.xml elements the model does not read (statistics, generator)
    #[serde(default)]
    pub meta_extras: Vec<String>,
}
