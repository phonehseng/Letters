//! themes: the colour scheme, the font scheme and the style lists shapes
//! refer to, and the colour map that ties a master's text and background
//! colours to scheme slots.

use serde::{Deserialize, Serialize};

use super::color::ColorRef;
use super::fill::{Fill, Line};
use super::source::Source;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub colors: ColorScheme,
    #[serde(default)]
    pub fonts: FontScheme,
    #[serde(default)]
    pub formats: FormatScheme,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// the twelve colour slots of a theme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorScheme {
    #[serde(default)]
    pub name: String,
    pub dk1: ColorRef,
    pub lt1: ColorRef,
    pub dk2: ColorRef,
    pub lt2: ColorRef,
    pub accent1: ColorRef,
    pub accent2: ColorRef,
    pub accent3: ColorRef,
    pub accent4: ColorRef,
    pub accent5: ColorRef,
    pub accent6: ColorRef,
    pub hlink: ColorRef,
    pub fol_hlink: ColorRef,
}

impl ColorScheme {
    /// the colour in a slot by its drawingml name.
    pub fn slot(&self, name: &str) -> Option<&ColorRef> {
        Some(match name {
            "dk1" => &self.dk1,
            "lt1" => &self.lt1,
            "dk2" => &self.dk2,
            "lt2" => &self.lt2,
            "accent1" => &self.accent1,
            "accent2" => &self.accent2,
            "accent3" => &self.accent3,
            "accent4" => &self.accent4,
            "accent5" => &self.accent5,
            "accent6" => &self.accent6,
            "hlink" => &self.hlink,
            "folHlink" => &self.fol_hlink,
            _ => return None,
        })
    }

    pub fn slot_mut(&mut self, name: &str) -> Option<&mut ColorRef> {
        Some(match name {
            "dk1" => &mut self.dk1,
            "lt1" => &mut self.lt1,
            "dk2" => &mut self.dk2,
            "lt2" => &mut self.lt2,
            "accent1" => &mut self.accent1,
            "accent2" => &mut self.accent2,
            "accent3" => &mut self.accent3,
            "accent4" => &mut self.accent4,
            "accent5" => &mut self.accent5,
            "accent6" => &mut self.accent6,
            "hlink" => &mut self.hlink,
            "folHlink" => &mut self.fol_hlink,
            _ => return None,
        })
    }

    pub const SLOTS: [&'static str; 12] = [
        "dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5",
        "accent6", "hlink", "folHlink",
    ];
}

impl Default for ColorScheme {
    /// the office 2013-2022 scheme
    fn default() -> Self {
        ColorScheme {
            name: "Office".into(),
            dk1: ColorRef::rgb("000000"),
            lt1: ColorRef::rgb("FFFFFF"),
            dk2: ColorRef::rgb("44546A"),
            lt2: ColorRef::rgb("E7E6E6"),
            accent1: ColorRef::rgb("4472C4"),
            accent2: ColorRef::rgb("ED7D31"),
            accent3: ColorRef::rgb("A5A5A5"),
            accent4: ColorRef::rgb("FFC000"),
            accent5: ColorRef::rgb("5B9BD5"),
            accent6: ColorRef::rgb("70AD47"),
            hlink: ColorRef::rgb("0563C1"),
            fol_hlink: ColorRef::rgb("954F72"),
        }
    }
}

/// the heading (major) and body (minor) fonts per script.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontScheme {
    #[serde(default)]
    pub name: String,
    pub major_latin: String,
    #[serde(default)]
    pub major_east_asian: String,
    #[serde(default)]
    pub major_complex: String,
    pub minor_latin: String,
    #[serde(default)]
    pub minor_east_asian: String,
    #[serde(default)]
    pub minor_complex: String,
    /// per-script fonts (`a:font script="Jpan"`) as (script, typeface)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub major_scripts: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minor_scripts: Vec<(String, String)>,
}

impl Default for FontScheme {
    fn default() -> Self {
        FontScheme {
            name: "Office".into(),
            major_latin: "Calibri Light".into(),
            major_east_asian: String::new(),
            major_complex: String::new(),
            minor_latin: "Calibri".into(),
            minor_east_asian: String::new(),
            minor_complex: String::new(),
            major_scripts: Vec::new(),
            minor_scripts: Vec::new(),
        }
    }
}

/// the style lists shape style references point into (1-based indices).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatScheme {
    #[serde(default)]
    pub fills: Vec<Fill>,
    #[serde(default)]
    pub lines: Vec<Line>,
    /// effect styles are kept as xml (shadows, glow), not drawn yet
    #[serde(default)]
    pub effects: Vec<String>,
    /// background fills, referred to as 1001, 1002, 1003
    #[serde(default)]
    pub background_fills: Vec<Fill>,
}

/// which scheme slot each logical colour uses (`p:clrMap`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorMap {
    /// (logical name, scheme slot): ("bg1", "lt1"), ("tx1", "dk1")...
    pub entries: Vec<(String, String)>,
}

impl ColorMap {
    pub fn map(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

impl Default for ColorMap {
    /// the usual map: light background, dark text
    fn default() -> Self {
        let pairs = [
            ("bg1", "lt1"),
            ("tx1", "dk1"),
            ("bg2", "lt2"),
            ("tx2", "dk2"),
            ("accent1", "accent1"),
            ("accent2", "accent2"),
            ("accent3", "accent3"),
            ("accent4", "accent4"),
            ("accent5", "accent5"),
            ("accent6", "accent6"),
            ("hlink", "hlink"),
            ("folHlink", "folHlink"),
        ];
        ColorMap {
            entries: pairs
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        }
    }
}
