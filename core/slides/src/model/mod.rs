//! the one presentation model both formats are read into and written from.
//!
//! it is made for a canvas editor: a deck is a list of slides, each slide
//! a list of positioned objects in stacking order (the first is drawn
//! first, at the back). everything is serde with camelCase names so the
//! interface can receive it as json and send it back.
//!
//! lengths are emu (english metric units, 914400 to the inch, 12700 to the
//! point), which is what pptx stores; odp lengths in centimetres or inches
//! convert exactly (one hundredth of a millimetre is 360 emu). angles are
//! sixty-thousandths of a degree, font sizes hundredths of a point, and
//! percentages thousandths of a percent (100000 is 100 %), all as pptx
//! stores them, so reading and writing pptx never rounds anything.
//!
//! what the model does not understand is kept: every object, paragraph,
//! slide and part remembers its original xml and a fingerprint of what was
//! read (see `Source`). writers put the original back while the
//! fingerprint still matches, which is how "nothing moves" is kept.

mod color;
mod fill;
mod frames;
mod geometry;
mod media;
mod objects;
mod source;
mod text;
mod theme;

use serde::{Deserialize, Serialize};

pub use color::{ColorKind, ColorMod, ColorRef};
pub use fill::{Background, Fill, GradientFill, GradientStop, Line, LineEnd, StyleRef};
pub use frames::{CellBorders, GraphicFrame, GraphicKind, Table, TableCell, TableRow};
pub use geometry::{Geometry, PathCommand, PathPoint, ShapePath};
pub use media::MediaItem;
pub use objects::{
    Connection, Connector, Group, Object, Picture, Placeholder, PlaceholderKind, Shape, ShapeStyle,
    Transform,
};
pub use source::{fingerprint_of, RawChild, Source, SourceFormat};
pub use text::{
    Anchor, Autofit, BodyProps, Bullet, BulletColor, BulletFont, BulletSize, FieldKind, LevelStyle,
    ParagraphProps, RunProps, Spacing, TabStop, TextAlign, TextBody, TextParagraph, TextRun,
    TextStyle, Typeface,
};
pub use theme::{ColorMap, ColorScheme, FontScheme, FormatScheme, Theme};

/// english metric units: 914400 per inch, 360000 per centimetre, 12700 per point.
pub type Emu = i64;

pub const EMU_PER_INCH: Emu = 914_400;
pub const EMU_PER_POINT: Emu = 12_700;
pub const EMU_PER_CM: Emu = 360_000;

/// a whole deck.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presentation {
    pub slide_size: SlideSize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_size: Option<SlideSize>,
    #[serde(default)]
    pub slides: Vec<Slide>,
    #[serde(default)]
    pub layouts: Vec<Layout>,
    #[serde(default)]
    pub masters: Vec<Master>,
    #[serde(default)]
    pub themes: Vec<Theme>,
    /// the text style of text boxes that are not placeholders
    /// (`p:defaultTextStyle` in pptx)
    #[serde(default)]
    pub default_text_style: TextStyle,
    /// named groups of slides, in order
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<SlideSection>,
    /// fonts carried inside the file
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded_fonts: Vec<EmbeddedFont>,
    /// every picture, video and sound, stored once
    #[serde(default)]
    pub media: Vec<MediaItem>,
    #[serde(default)]
    pub metadata: Metadata,
    /// number of the first slide (pptx `firstSlideNum`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_slide_number: Option<u32>,
    /// everything kept for the file the deck came from
    #[serde(default)]
    pub carry: Carry,
}

impl Presentation {
    pub fn layout(&self, id: &str) -> Option<&Layout> {
        self.layouts.iter().find(|layout| layout.id == id)
    }

    pub fn master(&self, id: &str) -> Option<&Master> {
        self.masters.iter().find(|master| master.id == id)
    }

    pub fn theme(&self, id: &str) -> Option<&Theme> {
        self.themes.iter().find(|theme| theme.id == id)
    }

    pub fn media_item(&self, id: &str) -> Option<&MediaItem> {
        self.media.iter().find(|item| item.id == id)
    }

    pub fn slide_index(&self, id: &str) -> Option<usize> {
        self.slides.iter().position(|slide| slide.id == id)
    }

    /// the layout and master a slide is built on.
    pub fn chain_of(&self, slide: &Slide) -> (Option<&Layout>, Option<&Master>) {
        let layout = slide.layout_id.as_deref().and_then(|id| self.layout(id));
        let master = layout
            .and_then(|layout| self.master(&layout.master_id))
            .or_else(|| self.masters.first());
        (layout, master)
    }

    /// the theme a master uses, or the first theme.
    pub fn theme_of(&self, master: Option<&Master>) -> Option<&Theme> {
        master
            .and_then(|master| master.theme_id.as_deref())
            .and_then(|id| self.theme(id))
            .or_else(|| self.themes.first())
    }
}

/// a slide (or notes page) size.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideSize {
    pub width: Emu,
    pub height: Emu,
    /// pptx's name for the size ("screen16x9", "A4"...), kept as written
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl Default for SlideSize {
    /// powerpoint's default: 13.333 by 7.5 inches (16:9)
    fn default() -> Self {
        SlideSize {
            width: 12_192_000,
            height: 6_858_000,
            kind: None,
        }
    }
}

/// one slide.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slide {
    /// stable id the interface refers to the slide by
    pub id: String,
    /// pptx's numeric slide id (`p:sldId id`), kept so links and sections
    /// still point at the right slide
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    /// in stacking order: the first is at the back
    #[serde(default)]
    pub objects: Vec<Object>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Notes>,
    /// kept exactly as read; the editor shows its kind
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animations: Vec<Animation>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<SlideComment>,
    /// the slide's own colour map override, if it has one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_map: Option<ColorMap>,
    /// draw the layout's and master's own shapes behind this slide
    #[serde(default = "yes")]
    pub show_master_shapes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

pub(crate) fn yes() -> bool {
    true
}

/// a slide layout: placeholders and decoration slides start from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub master_id: String,
    /// pptx's layout type ("title", "obj", "twoObj", "blank"...)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    #[serde(default)]
    pub objects: Vec<Object>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_map: Option<ColorMap>,
    #[serde(default = "yes")]
    pub show_master_shapes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// a slide master: the theme, the colour map, text styles per level and
/// the placeholders every layout inherits from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Master {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    #[serde(default)]
    pub objects: Vec<Object>,
    #[serde(default)]
    pub color_map: ColorMap,
    #[serde(default)]
    pub title_style: TextStyle,
    #[serde(default)]
    pub body_style: TextStyle,
    #[serde(default)]
    pub other_style: TextStyle,
    /// the layouts that belong to this master, in order
    #[serde(default)]
    pub layout_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// speaker notes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notes {
    /// the notes text (the notes page's body placeholder)
    pub text: TextBody,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

impl Notes {
    pub fn plain_text(&self) -> String {
        self.text.plain_text()
    }
}

/// a slide transition, kept exactly; `kind` names it for the interface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    /// "fade", "push", "wipe"... (the first child's local name), or the
    /// presentation:transition-style of odp
    #[serde(default)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    /// the original xml of the transition (pptx `p:transition`, including
    /// an `mc:AlternateContent` wrapper when it had one)
    pub xml: String,
    pub format: SourceFormat,
}

/// one animation effect, in play order. the timing tree itself is kept
/// exactly (`Carry`), this is a reading of it for the interface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Animation {
    /// the object's `id` on the slide
    pub target_id: u32,
    pub class: AnimationClass,
    /// pptx preset id ("10" is fade, "1" appear...) as written
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub trigger: AnimationTrigger,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationClass {
    Entrance,
    Emphasis,
    Exit,
    Path,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationTrigger {
    OnClick,
    WithPrevious,
    AfterPrevious,
}

/// a comment on a slide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideComment {
    #[serde(default)]
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    pub text: String,
    /// where the comment marker sits, emu from the slide corner
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<(Emu, Emu)>,
}

/// a named run of slides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideSection {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// the `Slide::id`s in this section
    pub slide_ids: Vec<String>,
}

/// a font carried inside the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedFont {
    pub typeface: String,
    /// media ids of the regular, bold, italic and bold italic faces
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regular: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold_italic: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// what a deck carries from the file it was read from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Carry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<SourceFormat>,
    /// every package part as read, byte for byte; writers reuse the ones
    /// whose model did not change and copy the ones the model never read
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<CarriedPart>,
    /// fingerprints of the deck-level settings (slide size, sections,
    /// default text style...) when read, so presentation.xml is copied
    /// while they are unchanged and patched piece by piece otherwise
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deck_pieces: Vec<(String, String)>,
    /// for each slide id: the part it was read from, so an unchanged slide
    /// keeps its name and relationships
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub part_names: Vec<(String, String)>,
}

impl Carry {
    pub fn part(&self, name: &str) -> Option<&CarriedPart> {
        self.parts
            .iter()
            .find(|part| part.name.eq_ignore_ascii_case(name))
    }

    pub fn part_name_of(&self, id: &str) -> Option<&str> {
        self.part_names
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, part)| part.as_str())
    }
}

/// one package part kept byte for byte.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarriedPart {
    pub name: String,
    #[serde(with = "crate::base64::serde_bytes")]
    pub bytes: Vec<u8>,
}
