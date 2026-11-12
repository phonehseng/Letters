//! named styles and the run formatting they (and runs) carry.

use serde::{Deserialize, Serialize};

use super::block::{merge_option, ParagraphProps, Preserved};
use super::carry::Attribute;
use super::inline::{normalize_marks, Mark, Toggle};

/// every style in the document plus the defaults they all start from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleSheet {
    #[serde(default)]
    pub defaults: StyleDefaults,
    #[serde(default)]
    pub styles: Vec<Style>,
    #[serde(default)]
    pub theme_fonts: ThemeFonts,
    /// style-sheet xml the model does not understand (word's latent style
    /// list, odt's outline and notes settings...), kept as original xml
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved: Vec<String>,
}

/// formatting every paragraph starts from before any style applies.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleDefaults {
    #[serde(default, skip_serializing_if = "ParagraphProps::is_empty")]
    pub paragraph: ParagraphProps,
    #[serde(default, skip_serializing_if = "RunProps::is_empty")]
    pub run: RunProps,
}

/// the theme's two fonts: one for headings ("major"), one for body text
/// ("minor"). word styles often name a theme slot instead of a font.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFonts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor: Option<String>,
}

impl ThemeFonts {
    /// the font a theme slot such as "majorHAnsi" or "minorBidi" stands for.
    pub fn resolve(&self, slot: &str) -> Option<String> {
        if slot.starts_with("major") {
            self.major.clone()
        } else if slot.starts_with("minor") {
            self.minor.clone()
        } else {
            None
        }
    }
}

/// a named style ("Heading 1", "Quote"...). a style can be based on
/// another; anything it does not set comes from that one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    /// what paragraphs refer to; the file's own identifier
    pub id: String,
    /// the name people see
    pub name: String,
    pub kind: StyleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    /// the style the next paragraph gets after pressing return
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// the default style of its kind (word's "Normal")
    #[serde(default)]
    pub is_default: bool,
    #[serde(default, skip_serializing_if = "ParagraphProps::is_empty")]
    pub paragraph: ParagraphProps,
    #[serde(default, skip_serializing_if = "RunProps::is_empty")]
    pub run: RunProps,
    #[serde(default, skip_serializing_if = "Preserved::is_empty")]
    pub preserved: Preserved,
}

impl Style {
    pub fn paragraph(id: &str, name: &str) -> Style {
        Style {
            id: id.into(),
            name: name.into(),
            kind: StyleKind::Paragraph,
            based_on: None,
            next: None,
            is_default: false,
            paragraph: ParagraphProps::default(),
            run: RunProps::default(),
            preserved: Preserved::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleKind {
    Paragraph,
    Character,
    Table,
    Numbering,
}

/// run (character) formatting in one struct, used by styles. inline
/// content carries the same information as a list of marks;
/// `to_marks` and `from_marks` convert between the two.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<FontRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_caps: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_caps: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub double_strike: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// in half points (24 is 12 pt)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
    /// "none" switches an inherited underline off
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<Script>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// run properties the model does not understand, as original xml
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved: Vec<String>,
}

/// a font, by name, and the theme slot it came from if any.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontRef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// fonts for other scripts (east asian, complex script) and hints,
    /// kept as the file's attributes
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<Attribute>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Script {
    Baseline,
    Superscript,
    Subscript,
}

impl RunProps {
    pub fn is_empty(&self) -> bool {
        *self == RunProps::default()
    }

    /// lays `over` on top of `self`; anything `over` sets wins.
    pub fn merge(&mut self, over: &RunProps) {
        merge_option(&mut self.style_id, &over.style_id);
        merge_option(&mut self.font, &over.font);
        merge_option(&mut self.bold, &over.bold);
        merge_option(&mut self.italic, &over.italic);
        merge_option(&mut self.all_caps, &over.all_caps);
        merge_option(&mut self.small_caps, &over.small_caps);
        merge_option(&mut self.strike, &over.strike);
        merge_option(&mut self.double_strike, &over.double_strike);
        merge_option(&mut self.hidden, &over.hidden);
        merge_option(&mut self.color, &over.color);
        merge_option(&mut self.size, &over.size);
        merge_option(&mut self.highlight, &over.highlight);
        merge_option(&mut self.underline, &over.underline);
        merge_option(&mut self.script, &over.script);
        merge_option(&mut self.language, &over.language);
        for xml in &over.preserved {
            if !self.preserved.contains(xml) {
                self.preserved.push(xml.clone());
            }
        }
    }

    /// the same formatting as a list of marks, in their fixed order.
    pub fn to_marks(&self) -> Vec<Mark> {
        let mut marks = Vec::new();
        if let Some(id) = &self.style_id {
            marks.push(Mark::CharacterStyle {
                style_id: id.clone(),
            });
        }
        if let Some(font) = &self.font {
            marks.push(Mark::FontFamily {
                name: font.name.clone(),
                theme: font.theme.clone(),
                extra: font.extra.clone(),
            });
        }
        push_toggle(&mut marks, self.bold, Mark::Bold, Toggle::Bold);
        push_toggle(&mut marks, self.italic, Mark::Italic, Toggle::Italic);
        push_toggle(&mut marks, self.all_caps, Mark::AllCaps, Toggle::AllCaps);
        push_toggle(
            &mut marks,
            self.small_caps,
            Mark::SmallCaps,
            Toggle::SmallCaps,
        );
        push_toggle(&mut marks, self.strike, Mark::Strike, Toggle::Strike);
        push_toggle(
            &mut marks,
            self.double_strike,
            Mark::DoubleStrike,
            Toggle::DoubleStrike,
        );
        push_toggle(&mut marks, self.hidden, Mark::Hidden, Toggle::Hidden);
        if let Some(hex) = &self.color {
            marks.push(Mark::Color { hex: hex.clone() });
        }
        if let Some(size) = self.size {
            marks.push(Mark::FontSize { half_points: size });
        }
        if let Some(color) = &self.highlight {
            marks.push(Mark::Highlight {
                color: color.clone(),
            });
        }
        match self.underline.as_deref() {
            Some("none") => marks.push(Mark::Off {
                property: Toggle::Underline,
            }),
            Some(style) => marks.push(Mark::Underline {
                style: style.to_string(),
            }),
            None => {}
        }
        match self.script {
            Some(Script::Superscript) => marks.push(Mark::Superscript),
            Some(Script::Subscript) => marks.push(Mark::Subscript),
            Some(Script::Baseline) => marks.push(Mark::Off {
                property: Toggle::Baseline,
            }),
            None => {}
        }
        if let Some(tag) = &self.language {
            marks.push(Mark::Language { tag: tag.clone() });
        }
        for xml in &self.preserved {
            marks.push(Mark::Preserved { xml: xml.clone() });
        }
        normalize_marks(&mut marks);
        marks
    }

    /// reads formatting marks back into one struct. links, tracked
    /// changes and comments are not formatting and are ignored here.
    pub fn from_marks(marks: &[Mark]) -> RunProps {
        let mut props = RunProps::default();
        for mark in marks {
            props.apply_mark(mark);
        }
        props
    }

    fn apply_mark(&mut self, mark: &Mark) {
        match mark {
            Mark::Bold => self.bold = Some(true),
            Mark::Italic => self.italic = Some(true),
            Mark::Underline { style } => self.underline = Some(style.clone()),
            Mark::Strike => self.strike = Some(true),
            Mark::DoubleStrike => self.double_strike = Some(true),
            Mark::Superscript => self.script = Some(Script::Superscript),
            Mark::Subscript => self.script = Some(Script::Subscript),
            Mark::SmallCaps => self.small_caps = Some(true),
            Mark::AllCaps => self.all_caps = Some(true),
            Mark::Hidden => self.hidden = Some(true),
            Mark::FontFamily { name, theme, extra } => {
                self.font = Some(FontRef {
                    name: name.clone(),
                    theme: theme.clone(),
                    extra: extra.clone(),
                })
            }
            Mark::FontSize { half_points } => self.size = Some(*half_points),
            Mark::Color { hex } => self.color = Some(hex.clone()),
            Mark::Highlight { color } => self.highlight = Some(color.clone()),
            Mark::Language { tag } => self.language = Some(tag.clone()),
            Mark::CharacterStyle { style_id } => self.style_id = Some(style_id.clone()),
            Mark::Off { property } => self.switch_off(*property),
            Mark::Preserved { xml } => self.preserved.push(xml.clone()),
            Mark::Link(_) | Mark::Insertion(_) | Mark::Deletion(_) | Mark::Comment { .. } => {}
        }
    }

    fn switch_off(&mut self, property: Toggle) {
        match property {
            Toggle::Bold => self.bold = Some(false),
            Toggle::Italic => self.italic = Some(false),
            Toggle::Underline => self.underline = Some("none".into()),
            Toggle::Strike => self.strike = Some(false),
            Toggle::DoubleStrike => self.double_strike = Some(false),
            Toggle::SmallCaps => self.small_caps = Some(false),
            Toggle::AllCaps => self.all_caps = Some(false),
            Toggle::Hidden => self.hidden = Some(false),
            Toggle::Baseline => self.script = Some(Script::Baseline),
        }
    }
}

fn push_toggle(marks: &mut Vec<Mark>, value: Option<bool>, on: Mark, off: Toggle) {
    match value {
        Some(true) => marks.push(on),
        Some(false) => marks.push(Mark::Off { property: off }),
        None => {}
    }
}
