//! the one document model every format is read into and written from.
//!
//! the shape follows the way a web rich-text editor (prosemirror style)
//! thinks about a document: a list of blocks, each block holding inline
//! content, and inline content carrying a list of marks (bold, a link,
//! a tracked insertion...). everything is serde with camelCase names, so
//! the interface can receive it as json and send it back.
//!
//! lengths are in twips, a twentieth of a point (1440 to the inch), which
//! is what word uses and is fine enough that nothing is lost converting
//! to and from odt. font sizes are in half points.

mod block;
mod carry;
mod inline;
mod media;
mod numbering;
mod section;
mod style;

use serde::{Deserialize, Serialize};

pub use block::{
    Alignment, Block, Border, CellBorders, Heading, Indent, LineRule, LineSpacing, ListRef,
    Paragraph, ParagraphProps, Preserved, Spacing, TabKind, TabLeader, TabStop, Table, TableCell,
    TableRow, VerticalAlign,
};
pub use carry::{
    Attribute, CarriedPart, Carry, ContentTypeDefault, DocxCarry, OdtCarry, PartRelationships,
    PartRoot, Relationship, SourceFormat,
};
pub use inline::{
    merge_adjacent_text, normalize_marks, BreakKind, Change, Field, FieldKind, FloatPosition,
    Image, ImageGeometry, Inline, LinkTarget, Mark, OriginalDrawing, Placement, Toggle, Wrap,
};
pub use media::MediaItem;
pub use numbering::{
    LevelOverride, ListDefinition, ListInstance, ListLevel, NumberFormat, Numbering,
};
pub use section::{
    Columns, HeaderFooter, HeaderFooterKind, HeaderFooterRefs, Margins, Orientation, PageSetup,
    Section, SectionStart,
};
pub use style::{
    FontRef, RunProps, Script, Style, StyleDefaults, StyleKind, StyleSheet, ThemeFonts,
};

/// a length in twentieths of a point. 1440 twips make an inch, 20 a point.
pub type Twips = i32;

/// twips in one inch, the unit most presets are described in.
pub const TWIPS_PER_INCH: Twips = 1440;

/// a whole document: its text, its formatting rules and everything
/// needed to write it back to the file it came from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub settings: DocumentSettings,
    #[serde(default)]
    pub styles: StyleSheet,
    #[serde(default)]
    pub numbering: Numbering,
    /// page setup, one entry per section. there is always one more section
    /// than there are `Block::SectionBreak` blocks: section `i` ends at the
    /// `i`-th break and the last one runs to the end of the document
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub blocks: Vec<Block>,
    /// header and footer contents, referred to from sections by id
    #[serde(default)]
    pub headers_footers: Vec<HeaderFooter>,
    /// footnote and endnote contents, referred to by `Inline::NoteReference`
    #[serde(default)]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub comments: Vec<Comment>,
    /// every picture's bytes, stored once however often it is shown
    #[serde(default)]
    pub media: Vec<MediaItem>,
    /// what the model does not understand, kept so saving loses nothing
    #[serde(default)]
    pub carry: Carry,
}

impl Document {
    /// an empty letter-sized document with one empty paragraph and the
    /// usual "Normal" style, ready to type into.
    pub fn new() -> Self {
        let mut document = Document {
            sections: vec![Section::default()],
            blocks: vec![Block::Paragraph(Paragraph::default())],
            ..Default::default()
        };
        document
            .styles
            .styles
            .push(Style::paragraph("Normal", "Normal"));
        document.styles.styles[0].is_default = true;
        document
    }

    /// looks a style up by its id.
    pub fn style(&self, id: &str) -> Option<&Style> {
        self.styles.styles.iter().find(|style| style.id == id)
    }

    /// looks a picture up by its media id.
    pub fn media_item(&self, id: &str) -> Option<&MediaItem> {
        self.media.iter().find(|item| item.id == id)
    }

    pub fn note(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|note| note.id == id)
    }

    pub fn comment(&self, id: &str) -> Option<&Comment> {
        self.comments.iter().find(|comment| comment.id == id)
    }

    pub fn header_footer(&self, id: &str) -> Option<&HeaderFooter> {
        self.headers_footers.iter().find(|part| part.id == id)
    }

    /// the section a block belongs to, counting section breaks before it.
    pub fn section_index_of(&self, block_index: usize) -> usize {
        let breaks = self.blocks[..block_index.min(self.blocks.len())]
            .iter()
            .filter(|block| matches!(block, Block::SectionBreak))
            .count();
        breaks.min(self.sections.len().saturating_sub(1))
    }

    /// the section settings for a block, or letter-size defaults when the
    /// document has no sections at all.
    pub fn section_of(&self, block_index: usize) -> Section {
        self.sections
            .get(self.section_index_of(block_index))
            .cloned()
            .unwrap_or_default()
    }

    /// makes sure there is exactly one section per section break plus one,
    /// so writers never have to guess. extra sections are dropped and
    /// missing ones copy the last known section.
    pub fn normalize_sections(&mut self) {
        let wanted = 1 + self
            .blocks
            .iter()
            .filter(|block| matches!(block, Block::SectionBreak))
            .count();
        if self.sections.is_empty() {
            self.sections.push(Section::default());
        }
        while self.sections.len() < wanted {
            let last = self.sections.last().cloned().unwrap_or_default();
            self.sections.push(last);
        }
        self.sections.truncate(wanted);
    }
}

/// what the file says about itself: title, author and dates.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified_by: Option<String>,
    /// iso 8601, as the file stores it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// document-wide switches.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSettings {
    /// even pages use the `even` header and footer of each section
    #[serde(default)]
    pub even_and_odd_headers: bool,
    /// distance between the automatic tab stops
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tab_stop: Option<Twips>,
    /// new edits are recorded as tracked changes
    #[serde(default)]
    pub track_changes: bool,
}

/// a footnote or an endnote.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub kind: NoteKind,
    /// word's separator lines are notes too ("separator",
    /// "continuationSeparator"); they are kept but never shown as notes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub special: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteKind {
    Footnote,
    Endnote,
}

/// a comment someone left on a range of text. the range itself is the
/// `Mark::Comment` on the inline content it covers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    #[serde(default)]
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    pub blocks: Vec<Block>,
}
