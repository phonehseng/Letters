//! free-floating objects that are not pictures: text boxes, movable
//! blocks of text that sit on the page wherever they are dragged.

use serde::{Deserialize, Serialize};

use super::block::{Block, Border};
use super::inline::{exact_emu, FloatPosition, Wrap};
use super::Twips;

/// a text box: blocks of text in a frame of their own, positioned on the
/// page like a floating picture. its content is ordinary blocks, so lists,
/// tables and formatting all work inside it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBox {
    /// a name for the box, shown in the selection list
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub horizontal: FloatPosition,
    pub vertical: FloatPosition,
    pub width: Twips,
    pub height: Twips,
    /// the box grows to fit its text instead of keeping `height`
    #[serde(default)]
    pub auto_height: bool,
    pub wrap: Wrap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_order: Option<i64>,
    /// clockwise, in degrees
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    /// space between the frame and the text
    #[serde(default)]
    pub margins: BoxMargins,
    /// the frame's line; `None` draws no line
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<Border>,
    /// background colour as six hex digits; `None` is transparent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    pub blocks: Vec<Block>,
    /// exact sizes in emu when the file gave them (see `Image::width_emu`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_emu: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_emu: Option<i64>,
}

impl TextBox {
    /// a box of `width` by `height` at an offset from the page corner,
    /// with word's usual frame: a thin black line and a white fill.
    pub fn new(x: Twips, y: Twips, width: Twips, height: Twips, blocks: Vec<Block>) -> TextBox {
        TextBox {
            name: None,
            horizontal: FloatPosition::new("page", x),
            vertical: FloatPosition::new("page", y),
            width,
            height,
            auto_height: false,
            wrap: Wrap::Square,
            z_order: None,
            rotation: None,
            margins: BoxMargins::default(),
            border: Some(Border {
                style: "single".into(),
                size: 4,
                color: Some("000000".into()),
            }),
            fill: Some("FFFFFF".into()),
            blocks,
            width_emu: None,
            height_emu: None,
        }
    }

    pub fn size_emu(&self) -> (i64, i64) {
        (
            exact_emu(self.width_emu, self.width),
            exact_emu(self.height_emu, self.height),
        )
    }
}

/// inner margins of a text box. word's defaults are 0.1 inch at the sides
/// and 0.05 inch at the top and bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxMargins {
    pub left: Twips,
    pub top: Twips,
    pub right: Twips,
    pub bottom: Twips,
}

impl Default for BoxMargins {
    fn default() -> Self {
        BoxMargins {
            left: 144,
            top: 72,
            right: 144,
            bottom: 72,
        }
    }
}
