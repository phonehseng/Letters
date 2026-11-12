//! graphic frames: tables (with rows, cells, spans and borders), charts,
//! smartart diagrams and embedded objects.

use serde::{Deserialize, Serialize};

use super::fill::{Fill, Line};
use super::objects::{Placeholder, Transform};
use super::source::{RawChild, Source};
use super::text::TextBody;
use super::Emu;

/// a graphic frame: a table, a chart, a diagram or an embedded object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicFrame {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<Placeholder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    pub kind: GraphicKind,
    /// the picture powerpoint saved to stand in for a chart, diagram or
    /// ole object, drawn when the object itself is not
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_media_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GraphicKind {
    Table(Table),
    /// a chart part, carried with its relationships and drawn by the
    /// applications themselves (no editable chart model yet)
    Chart {
        part: String,
    },
    /// smartart: its data, layout, style and colour parts
    Diagram {
        parts: Vec<String>,
    },
    /// an embedded or linked ole object ("Excel.Sheet.12"...)
    Ole {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prog_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part: Option<String>,
    },
    /// anything else, named by its `a:graphicData uri`
    Other {
        uri: String,
    },
}

/// a table (`a:tbl`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    /// column widths
    pub columns: Vec<Emu>,
    pub rows: Vec<TableRow>,
    /// the table style's guid (`a:tableStyleId`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_id: Option<String>,
    /// `a:tblPr` switches: firstRow, bandRow, firstCol, lastRow, lastCol, bandCol
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// `a:tblPr` children and attributes not modelled (fills, effects)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRow {
    pub height: Emu,
    pub cells: Vec<TableCell>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// one cell. covered cells of a merge are kept (as pptx does) with
/// `h_merge`/`v_merge` set, so the grid is always complete.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    #[serde(default)]
    pub text: TextBody,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub grid_span: u32,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub row_span: u32,
    #[serde(default)]
    pub h_merge: bool,
    #[serde(default)]
    pub v_merge: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    #[serde(default)]
    pub borders: CellBorders,
    /// left, right, top, bottom insets (`marL`...)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margins: Option<[Option<Emu>; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<super::text::Anchor>,
    /// `a:tcPr` attributes and children not modelled
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

fn one() -> u32 {
    1
}

fn is_one(value: &u32) -> bool {
    *value == 1
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellBorders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagonal_down: Option<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagonal_up: Option<Line>,
}
