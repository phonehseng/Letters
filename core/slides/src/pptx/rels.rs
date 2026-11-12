//! package relationships (`_rels/*.rels`) and content types.

use crate::package::{relative_target, resolve_target};
use crate::xml;

/// relationship types, by their last path segment, which is how the
/// reader tells them apart (strict and transitional uris end alike).
pub const SLIDE: &str = "slide";
pub const SLIDE_LAYOUT: &str = "slideLayout";
pub const SLIDE_MASTER: &str = "slideMaster";
pub const THEME: &str = "theme";
pub const NOTES_SLIDE: &str = "notesSlide";
pub const NOTES_MASTER: &str = "notesMaster";
pub const IMAGE: &str = "image";
pub const COMMENTS: &str = "comments";
pub const OFFICE_DOCUMENT: &str = "officeDocument";

const TRANSITIONAL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";

/// one relationship of a part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rel {
    pub id: String,
    /// the full type uri as written
    pub kind: String,
    /// the target as written (relative to the source part's folder)
    pub target: String,
    pub external: bool,
}

impl Rel {
    /// the type's last segment ("slide", "image"...).
    pub fn short_kind(&self) -> &str {
        self.kind.rsplit('/').next().unwrap_or("")
    }

    /// the full part name this relationship points at.
    pub fn resolved(&self, source_part: &str) -> String {
        resolve_target(source_part, &self.target)
    }
}

/// the relationships of one part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rels {
    pub items: Vec<Rel>,
}

impl Rels {
    pub fn parse(text: &str) -> Rels {
        let Ok(document) = xml::parse(text) else {
            return Rels::default();
        };
        let items = document
            .root
            .elements()
            .filter(|element| xml::local_name(&element.name) == "Relationship")
            .filter_map(|element| {
                Some(Rel {
                    id: element.attr("Id")?.to_string(),
                    kind: element.attr("Type").unwrap_or("").to_string(),
                    target: element.attr("Target").unwrap_or("").to_string(),
                    external: element.attr("TargetMode") == Some("External"),
                })
            })
            .collect();
        Rels { items }
    }

    pub fn get(&self, id: &str) -> Option<&Rel> {
        self.items.iter().find(|rel| rel.id == id)
    }

    pub fn first_of(&self, short_kind: &str) -> Option<&Rel> {
        self.items.iter().find(|rel| rel.short_kind() == short_kind)
    }

    pub fn all_of<'a>(&'a self, short_kind: &'a str) -> impl Iterator<Item = &'a Rel> {
        self.items
            .iter()
            .filter(move |rel| rel.short_kind() == short_kind)
    }

    /// the id of a relationship from `source_part` to `target_part` of
    /// this kind, adding one when there is none.
    pub fn ensure(&mut self, source_part: &str, target_part: &str, short_kind: &str) -> String {
        if let Some(rel) = self.items.iter().find(|rel| {
            !rel.external
                && rel.short_kind() == short_kind
                && rel.resolved(source_part).eq_ignore_ascii_case(target_part)
        }) {
            return rel.id.clone();
        }
        let id = self.free_id();
        self.items.push(Rel {
            id: id.clone(),
            kind: kind_uri(short_kind),
            target: relative_target(source_part, target_part),
            external: false,
        });
        id
    }

    /// makes the one relationship of a kind a part may have (a slide's
    /// layout, a layout's master) point at `target_part`, keeping its id.
    pub fn set_single(&mut self, source_part: &str, target_part: &str, short_kind: &str) -> String {
        let target = relative_target(source_part, target_part);
        let existing = self
            .items
            .iter()
            .position(|rel| rel.short_kind() == short_kind);
        let Some(index) = existing else {
            return self.ensure(source_part, target_part, short_kind);
        };
        if !self.items[index]
            .resolved(source_part)
            .eq_ignore_ascii_case(target_part)
        {
            self.items[index].target = target;
        }
        let id = self.items[index].id.clone();
        let mut seen = false;
        self.items.retain(|rel| {
            if rel.short_kind() != short_kind {
                return true;
            }
            let keep = !seen;
            seen = true;
            keep
        });
        id
    }

    /// removes every relationship of a kind.
    pub fn remove_kind(&mut self, short_kind: &str) {
        self.items.retain(|rel| rel.short_kind() != short_kind);
    }

    /// an id no relationship uses yet ("rId7").
    pub fn free_id(&self) -> String {
        let mut number = self.items.len() + 1;
        loop {
            let candidate = format!("rId{number}");
            if self.get(&candidate).is_none() {
                return candidate;
            }
            number += 1;
        }
    }

    pub fn to_xml(&self) -> String {
        let mut out = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        for rel in &self.items {
            out.push_str(&format!(
                "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/>",
                xml::escape_attribute(&rel.id),
                xml::escape_attribute(&rel.kind),
                xml::escape_attribute(&rel.target),
                if rel.external {
                    " TargetMode=\"External\""
                } else {
                    ""
                }
            ));
        }
        out.push_str("</Relationships>");
        out
    }
}

/// the full uri for a relationship type's short name.
pub fn kind_uri(short_kind: &str) -> String {
    match short_kind {
        // the only common type microsoft added under its own uri
        "media" => "http://schemas.microsoft.com/office/2007/relationships/media".to_string(),
        _ => format!("{TRANSITIONAL}{short_kind}"),
    }
}

/// where the relationships of a part live: "ppt/slides/slide1.xml" has
/// them in "ppt/slides/_rels/slide1.xml.rels".
pub fn rels_part_of(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// content types of the package (`[Content_Types].xml`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentTypes {
    /// (extension, content type)
    pub defaults: Vec<(String, String)>,
    /// (part name without the leading slash, content type)
    pub overrides: Vec<(String, String)>,
}

impl ContentTypes {
    pub fn parse(text: &str) -> ContentTypes {
        let mut types = ContentTypes::default();
        let Ok(document) = xml::parse(text) else {
            return types;
        };
        for element in document.root.elements() {
            match xml::local_name(&element.name) {
                "Default" => types.defaults.push((
                    element.attr("Extension").unwrap_or("").to_ascii_lowercase(),
                    element.attr("ContentType").unwrap_or("").to_string(),
                )),
                "Override" => types.overrides.push((
                    element
                        .attr("PartName")
                        .unwrap_or("")
                        .trim_start_matches('/')
                        .to_string(),
                    element.attr("ContentType").unwrap_or("").to_string(),
                )),
                _ => {}
            }
        }
        types
    }

    /// the content type of a part.
    pub fn of(&self, part: &str) -> Option<&str> {
        if let Some((_, kind)) = self
            .overrides
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(part))
        {
            return Some(kind);
        }
        let extension = part.rsplit('.').next()?.to_ascii_lowercase();
        self.defaults
            .iter()
            .find(|(known, _)| *known == extension)
            .map(|(_, kind)| kind.as_str())
    }

    pub fn ensure_default(&mut self, extension: &str, content_type: &str) {
        let extension = extension.to_ascii_lowercase();
        if !self.defaults.iter().any(|(known, _)| *known == extension) {
            self.defaults.push((extension, content_type.to_string()));
        }
    }

    pub fn set_override(&mut self, part: &str, content_type: &str) {
        match self
            .overrides
            .iter_mut()
            .find(|(name, _)| name.eq_ignore_ascii_case(part))
        {
            Some(entry) => entry.1 = content_type.to_string(),
            None => self
                .overrides
                .push((part.to_string(), content_type.to_string())),
        }
    }

    pub fn to_xml(&self) -> String {
        let mut out = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
        );
        for (extension, kind) in &self.defaults {
            out.push_str(&format!(
                "<Default Extension=\"{}\" ContentType=\"{}\"/>",
                xml::escape_attribute(extension),
                xml::escape_attribute(kind)
            ));
        }
        for (part, kind) in &self.overrides {
            out.push_str(&format!(
                "<Override PartName=\"/{}\" ContentType=\"{}\"/>",
                xml::escape_attribute(part),
                xml::escape_attribute(kind)
            ));
        }
        out.push_str("</Types>");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relationships_resolve_and_grow() {
        let mut rels = Rels::parse(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout2.xml"/></Relationships>"#,
        );
        let layout = rels.first_of(SLIDE_LAYOUT).unwrap();
        assert_eq!(
            layout.resolved("ppt/slides/slide1.xml"),
            "ppt/slideLayouts/slideLayout2.xml"
        );
        let id = rels.ensure("ppt/slides/slide1.xml", "ppt/media/image1.png", IMAGE);
        assert_eq!(id, "rId2");
        assert_eq!(rels.items[1].target, "../media/image1.png");
        assert_eq!(
            rels.ensure("ppt/slides/slide1.xml", "ppt/media/image1.png", IMAGE),
            "rId2"
        );
        assert_eq!(
            rels_part_of("ppt/slides/slide1.xml"),
            "ppt/slides/_rels/slide1.xml.rels"
        );
    }
}
