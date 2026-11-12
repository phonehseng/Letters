//! relationship files (`_rels/*.rels`): how a part names the parts and
//! links it uses, and the content type list of the package.

use crate::error::OfficeError;
use crate::model::{ContentTypeDefault, Relationship};
use crate::package::{resolve_target, Package};
use crate::xml::XmlWriter;

use super::names::relationship_kind;

/// where the relationships of `part` are stored: "word/document.xml"
/// keeps them in "word/_rels/document.xml.rels"; the package's own
/// relationships are in "_rels/.rels".
pub fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// reads the relationships of `part` ("" for the package itself).
/// a missing rels file simply means no relationships.
pub fn read(package: &Package, part: &str) -> Result<Vec<Relationship>, OfficeError> {
    let path = if part.is_empty() {
        "_rels/.rels".to_string()
    } else {
        rels_path(part)
    };
    let Some(document) = package.xml(&path)? else {
        return Ok(Vec::new());
    };
    Ok(document
        .root
        .children_named("pr:Relationship")
        .filter_map(|entry| {
            Some(Relationship {
                id: entry.attr("Id")?.to_string(),
                kind: entry.attr("Type")?.to_string(),
                target: entry.attr("Target")?.to_string(),
                external: entry
                    .attr("TargetMode")
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("external")),
            })
        })
        .collect())
}

/// the first relationship of a kind ("styles", "officeDocument"...).
pub fn find<'a>(relationships: &'a [Relationship], kind: &str) -> Option<&'a Relationship> {
    relationships
        .iter()
        .find(|rel| relationship_kind(&rel.kind) == kind)
}

/// the package path a relationship of `part` points at.
pub fn target_path(part: &str, relationship: &Relationship) -> String {
    resolve_target(part, &relationship.target)
}

pub fn write(relationships: &[Relationship]) -> String {
    let mut xml = XmlWriter::with_declaration();
    xml.open(
        "Relationships",
        &[(
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships",
        )],
    );
    for rel in relationships {
        let mut attributes = vec![
            ("Id", rel.id.as_str()),
            ("Type", rel.kind.as_str()),
            ("Target", rel.target.as_str()),
        ];
        if rel.external {
            attributes.push(("TargetMode", "External"));
        }
        xml.empty("Relationship", &attributes);
    }
    xml.close("Relationships");
    xml.finish()
}

/// `[Content_Types].xml`: content types by file extension, and by part
/// for parts that need their own.
#[derive(Default)]
pub struct ContentTypes {
    pub defaults: Vec<ContentTypeDefault>,
    pub overrides: Vec<(String, String)>,
}

impl ContentTypes {
    pub fn read(package: &Package) -> Result<ContentTypes, OfficeError> {
        let Some(document) = package.xml("[Content_Types].xml")? else {
            return Err(OfficeError::Damaged(
                "its list of content types is missing".into(),
            ));
        };
        let mut types = ContentTypes::default();
        for entry in document.root.elements() {
            match entry.name.as_str() {
                "ct:Default" => {
                    if let (Some(extension), Some(kind)) =
                        (entry.attr("Extension"), entry.attr("ContentType"))
                    {
                        types.defaults.push(ContentTypeDefault {
                            extension: extension.to_ascii_lowercase(),
                            content_type: kind.to_string(),
                        });
                    }
                }
                "ct:Override" => {
                    if let (Some(part), Some(kind)) =
                        (entry.attr("PartName"), entry.attr("ContentType"))
                    {
                        types
                            .overrides
                            .push((part.trim_start_matches('/').to_string(), kind.to_string()));
                    }
                }
                _ => {}
            }
        }
        Ok(types)
    }

    /// the content type of a part: its override, else its extension's default.
    pub fn of(&self, part: &str) -> Option<String> {
        if let Some((_, kind)) = self
            .overrides
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(part))
        {
            return Some(kind.clone());
        }
        let extension = part.rsplit('.').next()?.to_ascii_lowercase();
        self.defaults
            .iter()
            .find(|entry| entry.extension == extension)
            .map(|entry| entry.content_type.clone())
    }

    pub fn write(&self) -> String {
        let mut xml = XmlWriter::with_declaration();
        xml.open(
            "Types",
            &[(
                "xmlns",
                "http://schemas.openxmlformats.org/package/2006/content-types",
            )],
        );
        for entry in &self.defaults {
            xml.empty(
                "Default",
                &[
                    ("Extension", entry.extension.as_str()),
                    ("ContentType", entry.content_type.as_str()),
                ],
            );
        }
        for (part, kind) in &self.overrides {
            let name = format!("/{part}");
            xml.empty(
                "Override",
                &[("PartName", name.as_str()), ("ContentType", kind.as_str())],
            );
        }
        xml.close("Types");
        xml.finish()
    }

    pub fn add_default(&mut self, extension: &str, content_type: &str) {
        let extension = extension.to_ascii_lowercase();
        if !self
            .defaults
            .iter()
            .any(|entry| entry.extension == extension)
        {
            self.defaults.push(ContentTypeDefault {
                extension,
                content_type: content_type.to_string(),
            });
        }
    }

    pub fn add_override(&mut self, part: &str, content_type: &str) {
        self.overrides
            .retain(|(name, _)| !name.eq_ignore_ascii_case(part));
        self.overrides
            .push((part.to_string(), content_type.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rels_paths() {
        assert_eq!(
            rels_path("word/document.xml"),
            "word/_rels/document.xml.rels"
        );
        assert_eq!(rels_path("word/header1.xml"), "word/_rels/header1.xml.rels");
    }
}
