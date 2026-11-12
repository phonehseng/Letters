//! package relationships (`_rels/*.rels`) and the content type list
//! (`[Content_Types].xml`) that every xlsx file carries, read and written.

use crate::model::carry::Relationship;
use crate::xml::{Event, Reader, XmlWriter};

pub const RELATIONSHIPS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
pub const CONTENT_TYPES_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
pub const OFFICE_DOCUMENT_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const WORKSHEET_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
pub const STYLES_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
pub const SHARED_STRINGS_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
pub const THEME_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
pub const CALC_CHAIN_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain";
pub const COMMENTS_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
pub const VML_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
pub const TABLE_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
pub const HYPERLINK_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
pub const DRAWING_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
pub const CHART_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
pub const IMAGE_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
pub const CORE_PROPERTIES_REL: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";

pub const WORKBOOK_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
pub const WORKSHEET_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
pub const STYLES_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
pub const SHARED_STRINGS_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
pub const COMMENTS_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
pub const VML_TYPE: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";
pub const TABLE_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
pub const CORE_PROPERTIES_TYPE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
pub const RELATIONSHIPS_TYPE: &str = "application/vnd.openxmlformats-package.relationships+xml";

/// reads a `.rels` part. unreadable parts give no relationships rather
/// than an error: a broken link should not stop a workbook opening.
pub fn read_relationships(xml: &str) -> Vec<Relationship> {
    let mut reader = Reader::new(xml);
    let mut found = Vec::new();
    while let Ok(Some(event)) = reader.next() {
        let Event::Start(tag) = event else { continue };
        if tag.local() != "Relationship" {
            continue;
        }
        let (Some(id), Some(kind), Some(target)) =
            (tag.attr("Id"), tag.attr("Type"), tag.attr("Target"))
        else {
            continue;
        };
        found.push(Relationship {
            id: id.into_owned(),
            kind: kind.into_owned(),
            target: target.into_owned(),
            mode: tag.attr("TargetMode").map(|mode| mode.into_owned()),
        });
    }
    found
}

pub fn write_relationships(relationships: &[Relationship]) -> String {
    let mut writer = XmlWriter::with_declaration();
    writer.open("Relationships", &[("xmlns", RELATIONSHIPS_NS)]);
    for relationship in relationships {
        let mut attributes = vec![
            ("Id", relationship.id.as_str()),
            ("Type", relationship.kind.as_str()),
            ("Target", relationship.target.as_str()),
        ];
        if let Some(mode) = &relationship.mode {
            attributes.push(("TargetMode", mode.as_str()));
        }
        writer.empty("Relationship", &attributes);
    }
    writer.close("Relationships");
    writer.finish()
}

/// a relationship id not used yet, such as "rIdL3".
pub fn fresh_id(taken: &[Relationship], stem: &str) -> String {
    (1..)
        .map(|n| format!("rId{stem}{n}"))
        .find(|id| !taken.iter().any(|relationship| relationship.id == *id))
        .unwrap_or_else(|| format!("rId{stem}"))
}

/// one entry of the content type list.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentType {
    /// an extension ("png") for defaults, a part name for overrides
    pub key: String,
    pub content_type: String,
    pub is_default: bool,
}

pub fn read_content_types(xml: &str) -> Vec<ContentType> {
    let mut reader = Reader::new(xml);
    let mut found = Vec::new();
    while let Ok(Some(event)) = reader.next() {
        let Event::Start(tag) = event else { continue };
        let Some(content_type) = tag.attr("ContentType") else {
            continue;
        };
        let (key, is_default) = match tag.local() {
            "Default" => (tag.attr("Extension"), true),
            "Override" => (tag.attr("PartName"), false),
            _ => continue,
        };
        if let Some(key) = key {
            found.push(ContentType {
                key: key.trim_start_matches('/').to_string(),
                content_type: content_type.into_owned(),
                is_default,
            });
        }
    }
    found
}

pub fn write_content_types(entries: &[ContentType]) -> String {
    let mut writer = XmlWriter::with_declaration();
    writer.open("Types", &[("xmlns", CONTENT_TYPES_NS)]);
    for entry in entries.iter().filter(|entry| entry.is_default) {
        writer.empty(
            "Default",
            &[
                ("Extension", entry.key.as_str()),
                ("ContentType", entry.content_type.as_str()),
            ],
        );
    }
    for entry in entries.iter().filter(|entry| !entry.is_default) {
        let part = format!("/{}", entry.key);
        writer.empty(
            "Override",
            &[
                ("PartName", part.as_str()),
                ("ContentType", entry.content_type.as_str()),
            ],
        );
    }
    writer.close("Types");
    writer.finish()
}

/// the `.rels` part that belongs to a part: "xl/workbook.xml" ->
/// "xl/_rels/workbook.xml.rels".
pub fn rels_part_for(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relationships_round_trip() {
        let relationships = vec![Relationship {
            id: "rId1".into(),
            kind: HYPERLINK_REL.into(),
            target: "https://example.com/?a=1&b=2".into(),
            mode: Some("External".into()),
        }];
        assert_eq!(
            read_relationships(&write_relationships(&relationships)),
            relationships
        );
        assert_eq!(fresh_id(&relationships, "H"), "rIdH1");
        assert_eq!(
            rels_part_for("xl/worksheets/sheet1.xml"),
            "xl/worksheets/_rels/sheet1.xml.rels"
        );
    }
}
