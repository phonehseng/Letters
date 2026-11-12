//! inline content (text, tabs, pictures, note references...) and the
//! marks that format it.

use serde::{Deserialize, Serialize};

use super::carry::Attribute;
use super::Twips;

/// one piece of inline content. every kind that can be formatted carries
/// its marks, the way a rich-text editor stores them.
// pictures are the large variant; boxing them would make every match and
// constructor noisier for little gain
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Inline {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        marks: Vec<Mark>,
    },
    Tab {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        marks: Vec<Mark>,
    },
    Break {
        kind: BreakKind,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        marks: Vec<Mark>,
    },
    Image(Image),
    /// the little number in the text that points at a footnote or endnote
    NoteReference {
        #[serde(rename = "noteId")]
        note_id: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        marks: Vec<Mark>,
    },
    Field(Field),
    BookmarkStart {
        id: String,
        name: String,
    },
    BookmarkEnd {
        id: String,
    },
    /// where a comment's marker sits (after the commented range)
    CommentReference {
        #[serde(rename = "commentId")]
        comment_id: String,
    },
    /// run content the model does not understand (a symbol, a shape, an
    /// equation object...), kept as original xml. word only: it is the
    /// inside of a `w:r`, and the marks are that run's formatting
    PreservedRun {
        xml: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        marks: Vec<Mark>,
    },
    /// any other inline xml the model does not understand (a content
    /// control, an equation, a smart tag...), kept as original xml
    Preserved {
        xml: String,
    },
}

impl Inline {
    pub fn text(text: &str) -> Inline {
        Inline::Text {
            text: text.to_string(),
            marks: Vec::new(),
        }
    }

    pub fn styled(text: &str, marks: Vec<Mark>) -> Inline {
        Inline::Text {
            text: text.to_string(),
            marks,
        }
    }

    pub fn marks(&self) -> &[Mark] {
        match self {
            Inline::Text { marks, .. }
            | Inline::Tab { marks }
            | Inline::Break { marks, .. }
            | Inline::NoteReference { marks, .. }
            | Inline::PreservedRun { marks, .. } => marks,
            Inline::Image(image) => &image.marks,
            Inline::Field(field) => &field.marks,
            _ => &[],
        }
    }

    pub fn marks_mut(&mut self) -> Option<&mut Vec<Mark>> {
        match self {
            Inline::Text { marks, .. }
            | Inline::Tab { marks }
            | Inline::Break { marks, .. }
            | Inline::NoteReference { marks, .. }
            | Inline::PreservedRun { marks, .. } => Some(marks),
            Inline::Image(image) => Some(&mut image.marks),
            Inline::Field(field) => Some(&mut field.marks),
            _ => None,
        }
    }

    /// true when a tracked change has deleted this content.
    pub fn is_deleted(&self) -> bool {
        self.marks()
            .iter()
            .any(|mark| matches!(mark, Mark::Deletion(_)))
    }

    /// adds the visible text of this inline to `out`.
    pub fn push_plain_text(&self, out: &mut String) {
        if self.is_deleted() {
            return;
        }
        match self {
            Inline::Text { text, .. } => out.push_str(text),
            Inline::Tab { .. } => out.push('\t'),
            Inline::Break { .. } => out.push('\n'),
            Inline::Field(field) => out.push_str(&field.result),
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BreakKind {
    Line,
    Page,
    Column,
}

/// formatting and annotations on inline content. a mark's presence means
/// "on"; `Off` records that a style's formatting is switched off here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Mark {
    Bold,
    Italic,
    /// "single", "double", "wave", "dotted"...
    Underline {
        style: String,
    },
    Strike,
    DoubleStrike,
    Superscript,
    Subscript,
    SmallCaps,
    AllCaps,
    Hidden,
    FontFamily {
        name: String,
        /// a theme font slot ("minorHAnsi") when the font comes from the
        /// theme; `name` is then the theme's font, already resolved
        #[serde(default, skip_serializing_if = "Option::is_none")]
        theme: Option<String>,
        /// fonts for other scripts, as the file's attributes
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        extra: Vec<Attribute>,
    },
    FontSize {
        #[serde(rename = "halfPoints")]
        half_points: u32,
    },
    /// six hex digits, no `#`
    Color {
        hex: String,
    },
    /// a highlighter colour name ("yellow") or six hex digits
    Highlight {
        color: String,
    },
    /// a bcp 47 tag such as "en-US"
    Language {
        tag: String,
    },
    CharacterStyle {
        #[serde(rename = "styleId")]
        style_id: String,
    },
    Link(LinkTarget),
    /// a tracked insertion, shown underlined until accepted
    Insertion(Change),
    /// a tracked deletion, shown struck through until accepted. the text
    /// stays in the model so the change can still be rejected
    Deletion(Change),
    /// the text a comment is about
    Comment {
        id: String,
    },
    /// a style turns this on but this text turns it off again
    Off {
        property: Toggle,
    },
    /// run formatting the model does not understand, kept as original xml
    Preserved {
        xml: String,
    },
}

impl Mark {
    /// a fixed order for marks, so the same formatting always gives the
    /// same list whatever order a file happened to store it in. that
    /// keeps model comparisons and run merging simple.
    pub fn rank(&self) -> u8 {
        match self {
            Mark::CharacterStyle { .. } => 0,
            Mark::FontFamily { .. } => 1,
            Mark::Bold => 2,
            Mark::Italic => 3,
            Mark::AllCaps => 4,
            Mark::SmallCaps => 5,
            Mark::Strike => 6,
            Mark::DoubleStrike => 7,
            Mark::Hidden => 8,
            Mark::Color { .. } => 9,
            Mark::FontSize { .. } => 10,
            Mark::Highlight { .. } => 11,
            Mark::Underline { .. } => 12,
            Mark::Superscript | Mark::Subscript => 13,
            Mark::Language { .. } => 14,
            Mark::Off { .. } => 15,
            Mark::Preserved { .. } => 16,
            Mark::Link(_) => 17,
            Mark::Insertion(_) | Mark::Deletion(_) => 18,
            Mark::Comment { .. } => 19,
        }
    }
}

/// puts marks in their fixed order (see `Mark::rank`).
pub fn normalize_marks(marks: &mut [Mark]) {
    marks.sort_by_key(Mark::rank);
}

/// joins neighbouring text with the same marks and drops empty text.
/// files often split text into many runs for no visible reason (spell
/// checking, revision ids), and readers call this so the model is the
/// same however the file happened to split it.
pub fn merge_adjacent_text(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in inlines {
        if let Inline::Text { text, marks } = &inline {
            if text.is_empty() {
                continue;
            }
            if let Some(Inline::Text {
                text: previous,
                marks: previous_marks,
            }) = out.last_mut()
            {
                if previous_marks == marks {
                    previous.push_str(text);
                    continue;
                }
            }
        }
        out.push(inline);
    }
    out
}

/// a formatting switch a style can turn on and a run can turn off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Toggle {
    Bold,
    Italic,
    Underline,
    Strike,
    DoubleStrike,
    SmallCaps,
    AllCaps,
    Hidden,
    /// back to normal text from super or subscript
    Baseline,
}

/// where a link goes: an address outside the document or a bookmark in it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    /// a bookmark name inside the document
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

/// who made a tracked change and when.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// a field: text the word processor works out itself (page numbers,
/// dates, a table of contents).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub kind: FieldKind,
    /// the field code as the file had it ("PAGE", "DATE \@ "d MMMM yyyy"").
    /// written back unchanged so formatting switches are not lost
    #[serde(default)]
    pub instruction: String,
    /// the last shown value
    #[serde(default)]
    pub result: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
}

impl Field {
    pub fn page_number() -> Field {
        Field {
            kind: FieldKind::PageNumber,
            instruction: "PAGE".into(),
            result: "1".into(),
            marks: Vec::new(),
        }
    }

    pub fn page_count() -> Field {
        Field {
            kind: FieldKind::PageCount,
            instruction: "NUMPAGES".into(),
            result: "1".into(),
            marks: Vec::new(),
        }
    }

    pub fn table_of_contents() -> Field {
        Field {
            kind: FieldKind::TableOfContents,
            instruction: "TOC \\o \"1-3\" \\h \\z \\u".into(),
            result: "Update this table of contents to fill it in.".into(),
            marks: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    PageNumber,
    PageCount,
    Date,
    TableOfContents,
}

impl FieldKind {
    /// works out what a word field code means, or `None` for codes the
    /// model does not understand (those are kept as preserved xml).
    pub fn from_instruction(instruction: &str) -> Option<FieldKind> {
        let keyword = instruction
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        match keyword.as_str() {
            "PAGE" => Some(FieldKind::PageNumber),
            "NUMPAGES" => Some(FieldKind::PageCount),
            "DATE" | "CREATEDATE" | "SAVEDATE" | "PRINTDATE" | "TIME" => Some(FieldKind::Date),
            "TOC" => Some(FieldKind::TableOfContents),
            _ => None,
        }
    }
}

/// a picture placed in the text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    /// the `MediaItem` holding the picture's bytes
    pub media_id: String,
    pub width: Twips,
    pub height: Twips,
    /// alternative text for screen readers
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub placement: Placement,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
    /// the drawing xml as the file had it. while the picture's geometry is
    /// unchanged it is written back as is, so crops, borders and effects
    /// the model does not understand survive
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<OriginalDrawing>,
}

impl Image {
    pub fn geometry(&self) -> ImageGeometry {
        ImageGeometry {
            media_id: self.media_id.clone(),
            width: self.width,
            height: self.height,
            description: self.description.clone(),
            placement: self.placement.clone(),
        }
    }
}

/// the parts of a picture the model can change. if these still match what
/// was read, the original drawing xml is safe to write back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageGeometry {
    pub media_id: String,
    pub width: Twips,
    pub height: Twips,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub placement: Placement,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginalDrawing {
    pub xml: String,
    pub as_read: ImageGeometry,
}

/// in the line of text like a big letter, or floating on the page with
/// text flowing around it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Placement {
    #[default]
    Inline,
    Floating {
        horizontal: FloatPosition,
        vertical: FloatPosition,
        wrap: Wrap,
    },
}

/// where a floating picture sits along one direction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FloatPosition {
    /// "page", "margin", "column", "paragraph", "character", "line"
    pub relative_to: String,
    /// distance from that edge
    #[serde(default)]
    pub offset: Twips,
    /// "left", "center", "right", "top", "bottom" instead of an offset
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Wrap {
    /// text wraps around the picture's box
    Square,
    /// text wraps close to the picture's outline
    Tight,
    Through,
    /// text above and below only
    TopAndBottom,
    BehindText,
    InFrontOfText,
}
