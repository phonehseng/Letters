//! fills, lines and backgrounds.

use serde::{Deserialize, Serialize};

use super::color::ColorRef;
use super::source::RawChild;
use super::Emu;

/// what the inside of a shape (or a line, or text) is painted with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Fill {
    /// `a:noFill`: see-through
    None,
    Solid {
        color: ColorRef,
    },
    Gradient(GradientFill),
    /// `a:blipFill`: a picture stretched or tiled over the shape
    Picture {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_id: Option<String>,
        /// the original element, kept for tiling and effects
        #[serde(default, skip_serializing_if = "Option::is_none")]
        xml: Option<String>,
    },
    /// `a:pattFill`: a two-colour hatch ("pct50", "dkHorz"...)
    Pattern {
        preset: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        foreground: Option<ColorRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        background: Option<ColorRef>,
    },
    /// `a:grpFill`: the fill of the group the shape is in
    Group,
}

impl Fill {
    pub fn solid(color: ColorRef) -> Fill {
        Fill::Solid { color }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientFill {
    pub stops: Vec<GradientStop>,
    /// linear: the angle colours change along, 60000ths of a degree
    /// clockwise from left to right. `None` with no `path` is 0
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<i32>,
    /// linear: the angle turns with the shape's aspect ratio
    #[serde(default)]
    pub scaled: bool,
    /// "circle", "rect" or "shape" for gradients that spread from a point
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// the point a path gradient spreads from, as the inset rectangle
    /// `a:fillToRect` (l, t, r, b in thousandths of a percent)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<[i32; 4]>,
    #[serde(default)]
    pub rotate_with_shape: bool,
    /// attributes and children not modelled (`flip`, `tileRect`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStop {
    /// 0 to 100000
    pub position: i32,
    pub color: ColorRef,
}

/// the line along a shape's outline (`a:ln`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    /// preset dash ("solid", "dash", "sysDot"...)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<String>,
    /// "rnd", "sq" or "flat"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<String>,
    /// "sng", "dbl"...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compound: Option<String>,
    /// "round", "bevel" or "miter"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<LineEnd>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<LineEnd>,
    /// attributes and children not modelled (custom dashes, alignment)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
}

/// an arrowhead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEnd {
    /// "none", "triangle", "stealth", "diamond", "oval", "arrow"
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<String>,
}

/// a slide's, layout's or master's background.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Background {
    /// `p:bgPr`'s fill
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    /// `p:bgRef`: a background style of the theme with a colour
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<StyleRef>,
    /// the original `p:bg`, written back while it is set. editing
    /// operations that change the background clear it, so the writer
    /// knows to build a new one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xml: Option<String>,
}

/// a reference into the theme's style lists with a colour to use for
/// `phClr` (`a:lnRef`, `a:fillRef`, `a:effectRef`, `a:fontRef`, `p:bgRef`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleRef {
    /// 1-based index, or "major"/"minor" for a font reference (as 0/1)
    pub index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorRef>,
}
