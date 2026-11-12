//! colours as drawingml stores them: a base colour (an rgb value, a theme
//! slot, a system colour or a named preset) followed by transforms
//! (lighter, darker, more transparent...). they stay unresolved in the
//! model so that changing the theme changes every colour that follows it;
//! `crate::color::resolve` works out the final rgb.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorRef {
    pub kind: ColorKind,
    /// applied in order, as drawingml does
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<ColorMod>,
}

impl ColorRef {
    /// a plain colour from six hex digits.
    pub fn rgb(hex: &str) -> ColorRef {
        ColorRef {
            kind: ColorKind::Rgb {
                hex: hex.to_ascii_uppercase(),
            },
            mods: Vec::new(),
        }
    }

    /// a theme colour ("accent1", "tx1", "bg2"...).
    pub fn scheme(slot: &str) -> ColorRef {
        ColorRef {
            kind: ColorKind::Scheme {
                slot: slot.to_string(),
            },
            mods: Vec::new(),
        }
    }

    pub fn with(mut self, name: &str, value: i32) -> ColorRef {
        self.mods.push(ColorMod {
            name: name.to_string(),
            value,
        });
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ColorKind {
    /// `a:srgbClr`: six hex digits
    Rgb { hex: String },
    /// `a:schemeClr`: a theme slot, or "phClr" (the colour a style
    /// reference hands in)
    Scheme { slot: String },
    /// `a:sysClr`: a windows system colour with the value it had when saved
    System {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last: Option<String>,
    },
    /// `a:prstClr`: a named colour ("black", "white", "red"...)
    Preset { name: String },
    /// `a:hslClr`: hue in 60000ths of a degree, saturation and luminance
    /// in thousandths of a percent
    Hsl { hue: i32, sat: i32, lum: i32 },
    /// `a:scrgbClr`: linear rgb in thousandths of a percent
    ScRgb { r: i32, g: i32, b: i32 },
}

/// one transform: `lumMod`, `lumOff`, `tint`, `shade`, `alpha`, `satMod`...
/// with its value as stored (thousandths of a percent, or 60000ths of a
/// degree for `hueOff`/`hueMod`'s angles).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorMod {
    pub name: String,
    #[serde(default)]
    pub value: i32,
}
