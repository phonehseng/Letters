//! text inside shapes: a body (insets, anchoring, autofit, columns) with
//! paragraphs of runs, mirroring drawingml's `a:txBody`.
//!
//! every property is optional: `None` means "inherit" (from the shape's
//! list style, the layout and master placeholders, the master's text
//! styles, the deck's default style). `crate::inherit` resolves the chain
//! so the interface can show effective values.

use serde::{Deserialize, Serialize};

use super::color::ColorRef;
use super::fill::Fill;
use super::source::{RawChild, Source};
use super::Emu;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBody {
    #[serde(default)]
    pub body: BodyProps,
    /// the shape's own per-level overrides (`a:lstStyle`)
    #[serde(default, skip_serializing_if = "TextStyle::is_empty")]
    pub list_style: TextStyle,
    #[serde(default)]
    pub paragraphs: Vec<TextParagraph>,
}

impl TextBody {
    /// a body holding one paragraph per line of `text`.
    pub fn from_text(text: &str) -> TextBody {
        TextBody {
            paragraphs: text.split('\n').map(TextParagraph::with_text).collect(),
            ..Default::default()
        }
    }

    /// the text with paragraphs on their own lines, line breaks as `\v`
    /// turned into new lines too.
    pub fn plain_text(&self) -> String {
        self.paragraphs
            .iter()
            .map(TextParagraph::plain_text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.paragraphs.iter().all(|p| p.plain_text().is_empty())
    }
}

/// `a:bodyPr`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyProps {
    /// insets between the shape's edge and the text (`lIns` etc.);
    /// powerpoint's default is 91440 at the sides and 45720 top and bottom
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inset_left: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inset_top: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inset_right: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inset_bottom: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    /// the text block is centred horizontally as a whole
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_center: Option<bool>,
    /// false for `wrap="none"`: lines only break at paragraph ends
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autofit: Option<Autofit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_spacing: Option<Emu>,
    /// `vert`: "horz", "vert", "vert270", "wordArtVert", "eaVert"...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<String>,
    /// extra rotation of the text, 60000ths of a degree
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
    /// attributes (as `name="value"`) and children (`a:prstTxWarp`,
    /// `a:scene3d`...) not modelled
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    Top,
    Middle,
    Bottom,
    Justified,
    Distributed,
}

/// how text that does not fit is handled.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Autofit {
    /// `a:noAutofit`: text runs over the edge
    None,
    /// `a:normAutofit`: text shrinks. the scale and the line spacing
    /// reduction are what the application worked out when it saved; they
    /// are used as stored so the text looks as it did there
    Shrink {
        /// thousandths of a percent; 100000 is full size
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font_scale: Option<i32>,
        /// thousandths of a percentage point taken off the line spacing
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_reduction: Option<i32>,
    },
    /// `a:spAutoFit`: the shape grows to fit its text
    ResizeShape,
}

/// per-level paragraph and run defaults (`a:lstStyle`, `p:titleStyle`...).
/// index 0 is level 1; `default` is `a:defPPr`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<LevelStyle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<Option<LevelStyle>>,
    /// children not modelled (`a:extLst`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
}

impl TextStyle {
    pub fn is_empty(&self) -> bool {
        self.default.is_none() && self.levels.iter().all(Option::is_none) && self.extra.is_empty()
    }

    /// the style for a level (0 based), if this style sets one.
    pub fn level(&self, level: u8) -> Option<&LevelStyle> {
        self.levels.get(usize::from(level)).and_then(Option::as_ref)
    }
}

/// one level of a text style: paragraph properties with the run defaults
/// inside them, which is how drawingml nests them (`a:lvl1pPr/a:defRPr`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelStyle {
    #[serde(flatten)]
    pub paragraph: ParagraphProps,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextParagraph {
    #[serde(default)]
    pub props: ParagraphProps,
    #[serde(default)]
    pub runs: Vec<TextRun>,
    /// formatting of the paragraph mark (`a:endParaRPr`), which sets the
    /// height of an empty paragraph
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_props: Option<RunProps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

impl TextParagraph {
    pub fn with_text(text: &str) -> TextParagraph {
        let runs = if text.is_empty() {
            Vec::new()
        } else {
            vec![TextRun::text(text)]
        };
        TextParagraph {
            runs,
            ..Default::default()
        }
    }

    pub fn plain_text(&self) -> String {
        let mut text = String::new();
        for run in &self.runs {
            match run {
                TextRun::Text { text: part, .. }
                | TextRun::Field { text: part, .. }
                | TextRun::Preserved { text: part, .. } => text.push_str(part),
                TextRun::Break { .. } => text.push('\u{b}'),
            }
        }
        text
    }

    pub fn level(&self) -> u8 {
        self.props.level.unwrap_or(0)
    }
}

/// `a:pPr` (and the level styles' `a:lvlNpPr`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphProps {
    /// outline level, 0 to 8
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    /// where the text starts (`marL`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_left: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_right: Option<Emu>,
    /// first line relative to `margin_left`; negative hangs, and the
    /// bullet sits at `margin_left + indent`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<Spacing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_before: Option<Spacing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_after: Option<Spacing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet: Option<Bullet>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet_font: Option<BulletFont>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet_color: Option<BulletColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet_size: Option<BulletSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tab: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tabs: Option<Vec<TabStop>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtl: Option<bool>,
    /// run defaults (`a:defRPr`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunProps>,
    /// attributes (`eaLnBrk="1"`...) and children (`a:extLst`) not modelled
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
    Distributed,
    JustifyLow,
    Thai,
}

/// a spacing either as a share of the line (thousandths of a percent of
/// single spacing) or in hundredths of a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Spacing {
    Percent(i32),
    Points(i32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Bullet {
    /// `a:buNone`
    None,
    /// `a:buChar`
    Char { char: String },
    /// `a:buAutoNum`: "arabicPeriod", "alphaLcParenR", "romanUcPeriod"...
    AutoNumber {
        scheme: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_at: Option<u32>,
    },
    /// `a:buBlip`: a picture bullet
    Picture {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_id: Option<String>,
        xml: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BulletFont {
    /// `a:buFontTx`: the text's own font
    FollowText,
    Font(Typeface),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BulletColor {
    FollowText,
    Color(ColorRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum BulletSize {
    FollowText,
    /// thousandths of a percent of the text size
    Percent(i32),
    /// hundredths of a point
    Points(i32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabStop {
    pub position: Emu,
    /// "l", "ctr", "r", "dec"
    #[serde(default = "left_tab")]
    pub align: String,
}

fn left_tab() -> String {
    "l".into()
}

/// a typeface reference: a family name or a theme font ("+mj-lt" is the
/// major latin font, "+mn-ea" the minor east asian one).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Typeface {
    pub typeface: String,
    /// `panose`, `pitchFamily`, `charset` as written
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
}

impl Typeface {
    pub fn named(name: &str) -> Typeface {
        Typeface {
            typeface: name.to_string(),
            attributes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TextRun {
    Text {
        text: String,
        #[serde(default)]
        props: RunProps,
    },
    /// `a:br`, a line break inside the paragraph
    Break {
        #[serde(default)]
        props: RunProps,
    },
    /// `a:fld`: slide number, date and the like, with the text it showed
    Field {
        kind: FieldKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        text: String,
        #[serde(default)]
        props: RunProps,
    },
    /// something inside a paragraph the model does not read (an equation
    /// in `mc:AlternateContent`...), kept in place with its text
    Preserved {
        xml: String,
        #[serde(default)]
        text: String,
        #[serde(default)]
        props: RunProps,
    },
}

impl TextRun {
    pub fn text(text: &str) -> TextRun {
        TextRun::Text {
            text: text.to_string(),
            props: RunProps::default(),
        }
    }

    pub fn props(&self) -> &RunProps {
        match self {
            TextRun::Text { props, .. }
            | TextRun::Break { props }
            | TextRun::Field { props, .. }
            | TextRun::Preserved { props, .. } => props,
        }
    }

    pub fn props_mut(&mut self) -> &mut RunProps {
        match self {
            TextRun::Text { props, .. }
            | TextRun::Break { props }
            | TextRun::Field { props, .. }
            | TextRun::Preserved { props, .. } => props,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum FieldKind {
    SlideNumber,
    /// "datetime", "datetime1"... as written
    DateTime(String),
    Other(String),
}

/// `a:rPr`, `a:defRPr`, `a:endParaRPr`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProps {
    /// hundredths of a point
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// "none", "sng", "dbl", "heavy", "dotted"...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<String>,
    /// "noStrike", "sngStrike", "dblStrike"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<String>,
    /// "none", "small", "all"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caps: Option<String>,
    /// superscript (positive) or subscript, thousandths of a percent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<i32>,
    /// extra space between characters, hundredths of a point
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<i32>,
    /// kerning from this size up, hundredths of a point (0 is off)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kern: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<ColorRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latin: Option<Typeface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub east_asian: Option<Typeface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complex: Option<Typeface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<Typeface>,
    /// attributes (`dirty`, `altLang`...) and children (outline, effects,
    /// hyperlinks) not modelled, kept as written
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
}

impl RunProps {
    /// `self` with every property `self` leaves open taken from `parent`.
    pub fn over(&self, parent: &RunProps) -> RunProps {
        RunProps {
            size: self.size.or(parent.size),
            bold: self.bold.or(parent.bold),
            italic: self.italic.or(parent.italic),
            underline: self.underline.clone().or_else(|| parent.underline.clone()),
            strike: self.strike.clone().or_else(|| parent.strike.clone()),
            caps: self.caps.clone().or_else(|| parent.caps.clone()),
            baseline: self.baseline.or(parent.baseline),
            spacing: self.spacing.or(parent.spacing),
            kern: self.kern.or(parent.kern),
            lang: self.lang.clone().or_else(|| parent.lang.clone()),
            fill: self.fill.clone().or_else(|| parent.fill.clone()),
            highlight: self.highlight.clone().or_else(|| parent.highlight.clone()),
            latin: self.latin.clone().or_else(|| parent.latin.clone()),
            east_asian: self
                .east_asian
                .clone()
                .or_else(|| parent.east_asian.clone()),
            complex: self.complex.clone().or_else(|| parent.complex.clone()),
            symbol: self.symbol.clone().or_else(|| parent.symbol.clone()),
            // unknown xml belongs to the element it was written on
            extra: self.extra.clone(),
        }
    }
}

impl ParagraphProps {
    /// `self` with every property `self` leaves open taken from `parent`.
    pub fn over(&self, parent: &ParagraphProps) -> ParagraphProps {
        let run = match (&self.run, &parent.run) {
            (Some(own), Some(theirs)) => Some(own.over(theirs)),
            (own, theirs) => own.clone().or_else(|| theirs.clone()),
        };
        ParagraphProps {
            level: self.level.or(parent.level),
            align: self.align.or(parent.align),
            margin_left: self.margin_left.or(parent.margin_left),
            margin_right: self.margin_right.or(parent.margin_right),
            indent: self.indent.or(parent.indent),
            line_spacing: self.line_spacing.or(parent.line_spacing),
            space_before: self.space_before.or(parent.space_before),
            space_after: self.space_after.or(parent.space_after),
            bullet: self.bullet.clone().or_else(|| parent.bullet.clone()),
            bullet_font: self
                .bullet_font
                .clone()
                .or_else(|| parent.bullet_font.clone()),
            bullet_color: self
                .bullet_color
                .clone()
                .or_else(|| parent.bullet_color.clone()),
            bullet_size: self.bullet_size.or(parent.bullet_size),
            default_tab: self.default_tab.or(parent.default_tab),
            tabs: self.tabs.clone().or_else(|| parent.tabs.clone()),
            rtl: self.rtl.or(parent.rtl),
            run,
            extra: self.extra.clone(),
        }
    }
}
