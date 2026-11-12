//! footnotes, endnotes and comments: each is a small story of its own.

use crate::error::OfficeError;
use crate::model::{Comment, Note, NoteKind};

use super::{DocxReader, Story};
use crate::docx::{names, rels};

impl DocxReader<'_> {
    pub(super) fn read_notes(&mut self) -> Result<(), OfficeError> {
        for (kind_name, element_name, kind, prefix) in [
            (
                names::KIND_FOOTNOTES,
                "w:footnote",
                NoteKind::Footnote,
                "fn",
            ),
            (names::KIND_ENDNOTES, "w:endnote", NoteKind::Endnote, "en"),
        ] {
            let Some(part) = self.main_target(kind_name) else {
                continue;
            };
            let Some(xml) = self.part_xml(&part)? else {
                continue;
            };
            self.consume(&part);
            self.record_part_name(kind_name, &part);
            let mut story = Story::new(&part, &xml, rels::read(self.package, &part)?);
            for element in xml.root.children_named(element_name) {
                let mut ignored_sections = Vec::new();
                let blocks = self.read_blocks(element, &mut story, &mut ignored_sections);
                self.doc.notes.push(Note {
                    id: format!("{prefix}{}", element.attr("w:id").unwrap_or_default()),
                    kind,
                    special: element.attr("w:type").map(str::to_string),
                    blocks,
                });
            }
            self.finish_story(story, true);
        }
        Ok(())
    }

    pub(super) fn read_comments(&mut self) -> Result<(), OfficeError> {
        let Some(part) = self.main_target(names::KIND_COMMENTS) else {
            return Ok(());
        };
        let Some(xml) = self.part_xml(&part)? else {
            return Ok(());
        };
        self.consume(&part);
        self.record_part_name(names::KIND_COMMENTS, &part);
        let mut story = Story::new(&part, &xml, rels::read(self.package, &part)?);
        for element in xml.root.children_named("w:comment") {
            let mut ignored_sections = Vec::new();
            let blocks = self.read_blocks(element, &mut story, &mut ignored_sections);
            self.doc.comments.push(Comment {
                id: element.attr("w:id").unwrap_or_default().to_string(),
                author: element.attr("w:author").unwrap_or_default().to_string(),
                initials: element.attr("w:initials").map(str::to_string),
                date: element.attr("w:date").map(str::to_string),
                blocks,
            });
        }
        self.finish_story(story, true);
        Ok(())
    }
}
