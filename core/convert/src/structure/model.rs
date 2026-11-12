//! the structure of a document as the "easy to edit" writers see it.
//!
//! everything serialises with camelCase names, and every block says
//! which page it came from, where it was, and how sure the analysis is
//! (`confidence`, 0 to 1), so a ui can show doubt and a writer can fall
//! back to something simpler when a guess is weak.

use serde::{Deserialize, Serialize};

use crate::content::Rgb;
use crate::geometry::Rect;

/// a whole document: pages, then its blocks in reading order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredDocument {
    /// from the pdf's information dictionary, or the first heading
    pub title: Option<String>,
    pub pages: Vec<PageInfo>,
    /// every block of every page, in reading order, page by page
    pub blocks: Vec<Block>,
    /// headers, footers and page numbers repeated across pages; they are
    /// not in `blocks`, so writers can make them real headers and footers
    pub repeated: Vec<RepeatedText>,
    /// the most common text size, in points: what "body text" means here
    pub body_size: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    /// zero based
    pub page: usize,
    pub width: f32,
    pub height: f32,
    /// how many text columns the body of the page was read as
    pub columns: usize,
}

/// one block: where it came from, how sure the analysis is, and what it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    /// zero based page the block is on
    pub page: usize,
    /// its box on the page, in points from the top left
    pub rect: Rect,
    /// 0 to 1: how sure the analysis is that this is the right kind
    pub confidence: f32,
    #[serde(flatten)]
    pub content: BlockContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum BlockContent {
    /// level 1 is the largest heading
    Heading {
        level: u8,
        runs: Vec<TextRun>,
    },
    Paragraph {
        runs: Vec<TextRun>,
    },
    /// a run of list items; nesting is given by each item's level
    List {
        items: Vec<ListItem>,
    },
    Table(Table),
    Figure(Figure),
    /// a note at the foot of the page, with its marker ("1", "*")
    Footnote {
        marker: String,
        runs: Vec<TextRun>,
    },
}

/// a stretch of text in one style.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRun {
    pub text: String,
    /// the family people know, e.g. "Times New Roman" for TimesNewRomanPS-BoldMT
    pub font_family: String,
    pub generic_family: GenericFamily,
    pub bold: bool,
    pub italic: bool,
    /// points
    pub size: f32,
    pub color: Rgb,
    /// a web address or "#page=N" (one based) when the text is a link
    pub link: Option<String>,
    /// raised and smaller than the text around it, like a note number
    pub superscript: bool,
    /// letters-edit's id of the pdf font ("font:12-0"), for finding the
    /// font's own file
    #[serde(default)]
    pub font_ref: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospace,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    /// 0 for the outermost list, 1 for a list inside an item, and so on
    pub level: u8,
    /// numbered or lettered (an "ol") rather than bulleted (a "ul")
    pub ordered: bool,
    /// the marker as printed: "•", "3.", "b)", "iv."
    pub marker: String,
    /// the item's text without its marker
    pub runs: Vec<TextRun>,
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub rows: usize,
    pub columns: usize,
    /// points, left to right
    pub column_widths: Vec<f32>,
    /// points, top to bottom
    pub row_heights: Vec<f32>,
    /// one entry per cell; a merged cell appears once, at its top left,
    /// with its spans
    pub cells: Vec<TableCell>,
    /// how many rows at the top are headings (0 or 1 here)
    pub header_rows: usize,
    pub header_confidence: f32,
    /// found from drawn lines (true) or from text lined up in columns
    pub ruled: bool,
    /// "Table 2: ..." found just above or below the table
    pub caption: Option<Vec<TextRun>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
    /// the cell's text; lines that wrapped inside the cell are joined
    pub runs: Vec<TextRun>,
    pub rect: Rect,
    /// background shading, if the cell has some
    pub fill: Option<Rgb>,
    pub borders: CellBorders,
    pub align: CellAlign,
}

/// which edges of a cell have a drawn line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellBorders {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CellAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// a picture, with the caption found next to it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Figure {
    /// letters-edit's id for the picture, for `letters_edit::extract_image`
    /// (valid while the document is unchanged)
    pub image_id: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub caption: Option<Vec<TextRun>>,
    /// how sure the analysis is that `caption` belongs to this picture
    pub caption_confidence: f32,
}

/// text that repeats at the top or bottom of pages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepeatedText {
    pub kind: RepeatedKind,
    /// the text on the first page it was found on
    pub text: String,
    /// the text is the page number (alone or as "Page 3 of 9")
    pub page_number: bool,
    /// zero based pages it appears on
    pub pages: Vec<usize>,
    /// where it is on the first of those pages
    pub rect: Rect,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RepeatedKind {
    Header,
    Footer,
}

/// the text of some runs joined together.
pub fn runs_text(runs: &[TextRun]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

impl Block {
    /// the block's plain text: a heading's or paragraph's words, list
    /// items one per line, table rows one per line with cells separated
    /// by tabs.
    pub fn text(&self) -> String {
        match &self.content {
            BlockContent::Heading { runs, .. }
            | BlockContent::Paragraph { runs }
            | BlockContent::Footnote { runs, .. } => runs_text(runs),
            BlockContent::List { items } => items
                .iter()
                .map(|item| format!("{} {}", item.marker, runs_text(&item.runs)))
                .collect::<Vec<_>>()
                .join("\n"),
            BlockContent::Table(table) => table.text(),
            BlockContent::Figure(figure) => figure
                .caption
                .as_ref()
                .map(|caption| runs_text(caption))
                .unwrap_or_default(),
        }
    }
}

impl Table {
    /// the cell that starts at this row and column, if one does.
    pub fn cell(&self, row: usize, column: usize) -> Option<&TableCell> {
        self.cells
            .iter()
            .find(|cell| cell.row == row && cell.column == column)
    }

    /// rows one per line, cells separated by tabs.
    pub fn text(&self) -> String {
        (0..self.rows)
            .map(|row| {
                (0..self.columns)
                    .map(|column| {
                        self.cell(row, column)
                            .map(|cell| runs_text(&cell.runs))
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl TableCell {
    pub fn text(&self) -> String {
        runs_text(&self.runs)
    }
}
