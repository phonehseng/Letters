//! block level content: paragraphs, headings, tables and breaks.

use serde::{Deserialize, Serialize};

use super::carry::Attribute;
use super::inline::Inline;
use super::Twips;

/// one block of the document. the json tag is `type`, as a rich-text
/// editor expects (`{"type": "paragraph", ...}`).
// paragraphs are by far the most common block, so keeping them unboxed
// beats saving space on the rare small variants
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Block {
    Paragraph(Paragraph),
    Heading(Heading),
    Table(Table),
    /// a paragraph that holds nothing but a page break
    PageBreak,
    /// the end of a section; see `Document::sections`
    SectionBreak,
    /// a block the model does not understand (a content control, a table
    /// of contents in a content control, a text box frame...), kept as the
    /// original xml so it is written back exactly
    Preserved {
        xml: String,
    },
    /// a text box floating on the page. in the flow it sits where its
    /// anchor is: word and odt anchor it to the paragraph that follows
    TextBox(super::objects::TextBox),
    /// a shape (rectangle, oval, arrow, callout, line, free path) floating
    /// on the page, anchored like a text box
    Shape(super::drawing::Shape),
    /// objects grouped so they move together, anchored like a text box
    Group(super::drawing::Group),
}

impl Block {
    /// the paragraph inside a paragraph or a heading.
    pub fn paragraph(&self) -> Option<&Paragraph> {
        match self {
            Block::Paragraph(paragraph) => Some(paragraph),
            Block::Heading(heading) => Some(&heading.paragraph),
            _ => None,
        }
    }

    pub fn paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        match self {
            Block::Paragraph(paragraph) => Some(paragraph),
            Block::Heading(heading) => Some(&mut heading.paragraph),
            _ => None,
        }
    }

    /// a plain paragraph of unformatted text, handy for building documents.
    pub fn text(text: &str) -> Block {
        Block::Paragraph(Paragraph::with_text(text))
    }

    /// true for the objects that float on the page instead of flowing:
    /// text boxes, shapes, groups and positioned tables.
    pub fn is_positioned(&self) -> bool {
        match self {
            Block::TextBox(_) | Block::Shape(_) | Block::Group(_) => true,
            Block::Table(table) => table.position.is_some(),
            _ => false,
        }
    }

    /// the drawing anchor of a text box, shape or group.
    pub fn object_anchor(&self) -> Option<&super::drawing::ObjectAnchor> {
        match self {
            Block::TextBox(object) => object.anchor.as_ref(),
            Block::Shape(object) => object.anchor.as_ref(),
            Block::Group(object) => object.anchor.as_ref(),
            Block::Table(table) => table.position.as_ref()?.anchor.as_ref(),
            _ => None,
        }
    }

    pub fn object_anchor_mut(&mut self) -> Option<&mut Option<super::drawing::ObjectAnchor>> {
        match self {
            Block::TextBox(object) => Some(&mut object.anchor),
            Block::Shape(object) => Some(&mut object.anchor),
            Block::Group(object) => Some(&mut object.anchor),
            Block::Table(table) => table.position.as_mut().map(|position| &mut position.anchor),
            _ => None,
        }
    }
}

/// a heading is a paragraph with an outline level, 1 (top) to 6.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heading {
    pub level: u8,
    #[serde(flatten)]
    pub paragraph: Paragraph,
}

/// a paragraph: its style, its own formatting and its inline content.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paragraph {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_id: Option<String>,
    /// formatting set on this paragraph itself, on top of its style
    #[serde(default, skip_serializing_if = "ParagraphProps::is_empty")]
    pub props: ParagraphProps,
    #[serde(default)]
    pub content: Vec<Inline>,
    /// attributes of the paragraph element the model does not read
    /// (word's paragraph ids, which comments refer to, live here)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<Attribute>,
}

impl Paragraph {
    pub fn with_text(text: &str) -> Paragraph {
        let content = if text.is_empty() {
            Vec::new()
        } else {
            vec![Inline::text(text)]
        };
        Paragraph {
            content,
            ..Default::default()
        }
    }

    /// the visible text of the paragraph: tabs become `\t`, line breaks
    /// `\n`, and text deleted by a tracked change is left out.
    pub fn plain_text(&self) -> String {
        let mut text = String::new();
        for inline in &self.content {
            inline.push_plain_text(&mut text);
        }
        text
    }
}

/// paragraph formatting. every field is optional: `None` means "whatever
/// the style says", so a paragraph only stores what it changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphProps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    #[serde(default, skip_serializing_if = "Indent::is_empty")]
    pub indent: Indent,
    #[serde(default, skip_serializing_if = "Spacing::is_empty")]
    pub spacing: Spacing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_with_next: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_lines_together: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_break_before: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<ListRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<TabStop>,
    /// 0 for heading 1 ... 8 for heading 9, as word stores it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_level: Option<u8>,
    /// paragraph properties the model does not understand (borders,
    /// shading, widow control...), as original xml
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved: Vec<String>,
}

impl ParagraphProps {
    pub fn is_empty(&self) -> bool {
        *self == ParagraphProps::default()
    }

    /// lays `over` on top of `self`: anything `over` sets wins.
    /// this is how a style's formatting is combined with the style it is
    /// based on, and a paragraph's own formatting with its style.
    pub fn merge(&mut self, over: &ParagraphProps) {
        merge_option(&mut self.alignment, &over.alignment);
        self.indent.merge(&over.indent);
        self.spacing.merge(&over.spacing);
        merge_option(&mut self.keep_with_next, &over.keep_with_next);
        merge_option(&mut self.keep_lines_together, &over.keep_lines_together);
        merge_option(&mut self.page_break_before, &over.page_break_before);
        merge_option(&mut self.list, &over.list);
        merge_option(&mut self.outline_level, &over.outline_level);
        if !over.tabs.is_empty() {
            merge_tabs(&mut self.tabs, &over.tabs);
        }
        for xml in &over.preserved {
            if !self.preserved.contains(xml) {
                self.preserved.push(xml.clone());
            }
        }
    }
}

/// tab stops add up through styles; a stop of kind `Clear` removes the
/// inherited stop at the same position.
fn merge_tabs(base: &mut Vec<TabStop>, over: &[TabStop]) {
    for stop in over {
        base.retain(|existing| existing.position != stop.position);
        if stop.kind != TabKind::Clear {
            base.push(stop.clone());
        }
    }
    base.sort_by_key(|stop| stop.position);
}

pub(crate) fn merge_option<T: Clone>(base: &mut Option<T>, over: &Option<T>) {
    if over.is_some() {
        base.clone_from(over);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    Left,
    Center,
    Right,
    Justify,
}

/// paragraph indents. a first-line indent pushes the first line in; a
/// hanging indent pulls it out (the rest of the lines stay at `left`).
/// only one of the two is normally set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Indent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_line: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hanging: Option<Twips>,
}

impl Indent {
    pub fn is_empty(&self) -> bool {
        *self == Indent::default()
    }

    pub fn merge(&mut self, over: &Indent) {
        merge_option(&mut self.left, &over.left);
        merge_option(&mut self.right, &over.right);
        // first line and hanging are two views of one value, so setting
        // either replaces both
        if over.first_line.is_some() || over.hanging.is_some() {
            self.first_line = over.first_line;
            self.hanging = over.hanging;
        }
    }

    /// where the first line starts relative to the other lines:
    /// positive for a first-line indent, negative for a hanging one.
    pub fn first_line_offset(&self) -> Twips {
        match (self.first_line, self.hanging) {
            (_, Some(hanging)) if hanging != 0 => -hanging,
            (Some(first), _) => first,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spacing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<LineSpacing>,
}

impl Spacing {
    pub fn is_empty(&self) -> bool {
        *self == Spacing::default()
    }

    pub fn merge(&mut self, over: &Spacing) {
        merge_option(&mut self.before, &over.before);
        merge_option(&mut self.after, &over.after);
        merge_option(&mut self.line, &over.line);
    }
}

/// line spacing. with `Auto` the value is in 240ths of a line (240 is
/// single, 480 double); with `Exact` and `AtLeast` it is a height in twips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineSpacing {
    pub rule: LineRule,
    pub value: i32,
}

impl LineSpacing {
    pub const SINGLE: LineSpacing = LineSpacing {
        rule: LineRule::Auto,
        value: 240,
    };
    pub const DOUBLE: LineSpacing = LineSpacing {
        rule: LineRule::Auto,
        value: 480,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineRule {
    Auto,
    Exact,
    AtLeast,
}

/// a paragraph's place in a list: which list and how deep (0 is the top).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListRef {
    /// a `ListInstance` id. "0" means "explicitly not in a list", which
    /// is how a paragraph switches off a list its style would add
    pub list_id: String,
    pub level: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabStop {
    /// measured from the left margin, as word and the ruler show it
    /// (odt measures from the left indent; its reader and writer convert)
    pub position: Twips,
    pub kind: TabKind,
    #[serde(default, skip_serializing_if = "is_no_leader")]
    pub leader: TabLeader,
}

fn is_no_leader(leader: &TabLeader) -> bool {
    *leader == TabLeader::None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabKind {
    Left,
    Center,
    Right,
    Decimal,
    Bar,
    /// removes a tab stop the style would otherwise give
    Clear,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabLeader {
    #[default]
    None,
    Dot,
    Hyphen,
    Underscore,
    MiddleDot,
    Heavy,
}

/// a table. covered cells of a merge are left out, the way html and
/// rich-text editors do it: a cell spanning two columns is one cell with
/// `col_span: 2`, a cell spanning two rows is one cell with `row_span: 2`
/// and the row below simply has one cell fewer at that place.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_id: Option<String>,
    /// total width when it is a fixed length
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    /// the width of each grid column, left to right
    #[serde(default)]
    pub column_widths: Vec<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borders: Option<CellBorders>,
    pub rows: Vec<TableRow>,
    #[serde(default, skip_serializing_if = "Preserved::is_empty")]
    pub preserved: Preserved,
    /// a floating table: positioned on the page like a text box, with
    /// text flowing around it. `None` for a table in the flow
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<TablePosition>,
    /// `Some(true)`: columns keep exactly their widths (no autofit)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_layout: Option<bool>,
    /// the default space between each cell's edges and its text
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_margins: Option<CellMargins>,
    /// distance of the table's left edge from the margin
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent: Option<Twips>,
}

/// where a floating table sits and how close text may come.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TablePosition {
    /// relative to "page", "margin" or "column" (word's "text")
    pub horizontal: super::inline::FloatPosition,
    /// relative to "page", "margin" or "paragraph"
    pub vertical: super::inline::FloatPosition,
    /// the gap kept between the table and the text around it
    #[serde(default)]
    pub distance: Distances,
    /// other floating tables may overlap this one
    #[serde(default = "allowed", skip_serializing_if = "is_allowed")]
    pub overlap: bool,
    /// opendocument floats a table in a frame anchored in a paragraph;
    /// which paragraph is kept here (word floats tables by themselves)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<super::drawing::ObjectAnchor>,
}

fn allowed() -> bool {
    true
}

fn is_allowed(value: &bool) -> bool {
    *value
}

/// four distances, one per side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Distances {
    #[serde(default)]
    pub left: Twips,
    #[serde(default)]
    pub top: Twips,
    #[serde(default)]
    pub right: Twips,
    #[serde(default)]
    pub bottom: Twips,
}

/// the space inside a cell around its text, per side; `None` keeps the
/// table's (or the application's) default for that side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellMargins {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Twips>,
}

impl CellMargins {
    pub fn is_empty(&self) -> bool {
        *self == CellMargins::default()
    }
}

/// how a row's height is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeightRule {
    /// at least `height`, taller when the text needs it (the default)
    AtLeast,
    /// exactly `height`; text that does not fit is cut off
    Exact,
}

/// which way text runs in a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextDirection {
    /// left to right, lines top to bottom (normal text)
    Horizontal,
    /// turned to read upwards, bottom to top
    Upward,
    /// turned to read downwards, top to bottom
    Downward,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<Twips>,
    /// how `height` is used; `None` is "at least"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_rule: Option<HeightRule>,
    /// repeated at the top of each page the table runs onto
    #[serde(default)]
    pub header: bool,
    #[serde(default)]
    pub cant_split: bool,
    pub cells: Vec<TableCell>,
    #[serde(default, skip_serializing_if = "Preserved::is_empty")]
    pub preserved: Preserved,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    #[serde(default = "one")]
    pub col_span: u32,
    #[serde(default = "one")]
    pub row_span: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Twips>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borders: Option<CellBorders>,
    /// background colour as six hex digits
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shading: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<VerticalAlign>,
    pub blocks: Vec<Block>,
    #[serde(default, skip_serializing_if = "Preserved::is_empty")]
    pub preserved: Preserved,
    /// this cell's own space around its text, over the table's
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margins: Option<CellMargins>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_direction: Option<TextDirection>,
}

fn one() -> u32 {
    1
}

impl Default for TableCell {
    fn default() -> Self {
        TableCell {
            col_span: 1,
            row_span: 1,
            width: None,
            borders: None,
            shading: None,
            vertical_align: None,
            blocks: Vec::new(),
            preserved: Preserved::default(),
            margins: None,
            text_direction: None,
        }
    }
}

impl TableCell {
    pub fn with_text(text: &str) -> TableCell {
        TableCell {
            blocks: vec![Block::text(text)],
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellBorders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Border>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Border>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Border>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Border>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside_horizontal: Option<Border>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside_vertical: Option<Border>,
}

/// one border line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Border {
    /// "single", "double", "dashed", "dotted", "none"...
    pub style: String,
    /// thickness in eighths of a point, as word stores it
    #[serde(default)]
    pub size: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// the parts of an element the model does not understand: attributes it
/// does not read and child elements it does not read, each child kept as
/// its original xml text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preserved {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<Attribute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
}

impl Preserved {
    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty() && self.children.is_empty()
    }
}
