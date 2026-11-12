//! the package being written: its parts in order, its content types, and
//! helpers for naming new parts and reading carried relationships.

use crate::package::{write_zip, Part};

use super::super::rels::{rels_part_of, ContentTypes, Rels};
use crate::error::SlidesError;
use crate::model::{Presentation, SourceFormat};

pub struct OutPackage {
    /// (part name, bytes) in the order the original package listed them
    pub parts: Vec<(String, Vec<u8>)>,
    pub content_types: ContentTypes,
    /// the content types as read, so an unchanged list keeps its bytes
    original_types: ContentTypes,
}

pub const PRESENTATION_PART: &str = "ppt/presentation.xml";
pub const CONTENT_TYPES_PART: &str = "[Content_Types].xml";

impl OutPackage {
    /// the parts a pptx deck carried, or nothing for a deck from odp.
    pub fn from_carry(presentation: &Presentation) -> OutPackage {
        let carried = presentation.carry.format == Some(SourceFormat::Pptx);
        let parts: Vec<(String, Vec<u8>)> = if carried {
            presentation
                .carry
                .parts
                .iter()
                .map(|part| (part.name.clone(), part.bytes.clone()))
                .collect()
        } else {
            Vec::new()
        };
        let content_types = parts
            .iter()
            .find(|(name, _)| name == CONTENT_TYPES_PART)
            .map(|(_, bytes)| ContentTypes::parse(&String::from_utf8_lossy(bytes)))
            .unwrap_or_default();
        OutPackage {
            parts,
            original_types: content_types.clone(),
            content_types,
        }
    }

    pub fn has_presentation(&self) -> bool {
        self.get(PRESENTATION_PART).is_some()
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.parts
            .iter()
            .find(|(part, _)| part.eq_ignore_ascii_case(name))
            .map(|(_, bytes)| bytes.as_slice())
    }

    pub fn text(&self, name: &str) -> Option<String> {
        self.get(name).map(|bytes| {
            String::from_utf8_lossy(bytes)
                .trim_start_matches('\u{feff}')
                .to_string()
        })
    }

    pub fn set(&mut self, name: &str, bytes: Vec<u8>) {
        match self
            .parts
            .iter_mut()
            .find(|(part, _)| part.eq_ignore_ascii_case(name))
        {
            Some(entry) => entry.1 = bytes,
            None => self.parts.push((name.to_string(), bytes)),
        }
    }

    pub fn set_text(&mut self, name: &str, text: &str) {
        self.set(name, text.as_bytes().to_vec());
    }

    pub fn remove(&mut self, name: &str) {
        self.parts
            .retain(|(part, _)| !part.eq_ignore_ascii_case(name));
    }

    /// the relationships the package has for a part (empty when none).
    pub fn rels_of(&self, part: &str) -> Rels {
        self.text(&rels_part_of(part))
            .map(|text| Rels::parse(&text))
            .unwrap_or_default()
    }

    /// stores a part's relationships, leaving the original bytes alone
    /// when they say the same thing.
    pub fn set_rels(&mut self, part: &str, rels: &Rels) {
        let name = rels_part_of(part);
        if self.get(&name).is_some() && self.rels_of(part) == *rels {
            return;
        }
        self.set_text(&name, &rels.to_xml());
    }

    /// `folder` + `file`, or with a number added before the extension
    /// when that name is taken.
    pub fn free_name(&self, folder: &str, file: &str) -> String {
        let candidate = format!("{folder}{file}");
        if self.get(&candidate).is_none() {
            return candidate;
        }
        let (stem, extension) = file.rsplit_once('.').unwrap_or((file, "bin"));
        (2..)
            .map(|number| format!("{folder}{stem}{number}.{extension}"))
            .find(|name| self.get(name).is_none())
            .unwrap_or(candidate)
    }

    /// "ppt/slides/slide" + the lowest unused number + ".xml".
    pub fn numbered_name(&self, prefix: &str, taken: &[String]) -> String {
        (1..)
            .map(|number| format!("{prefix}{number}.xml"))
            .find(|name| {
                self.get(name).is_none() && !taken.iter().any(|t| t.eq_ignore_ascii_case(name))
            })
            .unwrap_or_else(|| format!("{prefix}.xml"))
    }

    /// the package as a zip, content types first as office expects.
    pub fn zip(mut self) -> Result<Vec<u8>, SlidesError> {
        let unchanged =
            self.content_types == self.original_types && self.get(CONTENT_TYPES_PART).is_some();
        if !unchanged {
            let types = self.content_types.to_xml();
            self.set_text(CONTENT_TYPES_PART, &types);
        }
        let mut ordered: Vec<Part> = Vec::with_capacity(self.parts.len());
        for (name, bytes) in self.parts {
            let part = Part { name, bytes };
            if part.name == CONTENT_TYPES_PART {
                ordered.insert(0, part);
            } else {
                ordered.push(part);
            }
        }
        write_zip(&ordered, &[])
    }
}
