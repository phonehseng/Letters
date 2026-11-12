//! drawings: shapes (rectangles, ovals, arrows, callouts, lines, free
//! paths), groups of objects and connector lines.
//!
//! a shape is positioned exactly like a text box (see `TextBox`): an
//! offset from the page, the margins, the column or its paragraph, a size,
//! a way for text to wrap around it and a place in the stacking order. on
//! top of that it has an outline (`Geometry`), a `Fill`, an optional
//! `Stroke` and optional text inside.
//!
//! lengths follow the rest of the model: twips, with the exact english
//! metric units (emu, 914400 to the inch) kept beside them while they still
//! agree, so a file's exact numbers survive a round trip.
//!
//! two things make "nothing moves when you click" work:
//! - `OriginalObject` keeps the xml an object was read from together with a
//!   fingerprint of the model as read. while the model still matches, the
//!   writer puts the original xml back byte for byte
//! - `ObjectAnchor` remembers the paragraph a drawing was anchored in, so
//!   saving does not add or lose paragraphs around it

use serde::{Deserialize, Serialize};

use super::block::{Block, Paragraph, VerticalAlign};
use super::carry::{Attribute, SourceFormat};
use super::inline::{exact_emu, FloatPosition, Image, Mark, Wrap};
use super::objects::{BoxMargins, TextBox};
use super::Twips;

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

/// a shape: an outline filled and stroked, optionally holding text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    /// a name for the shape, shown in the selection list
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// alternative text for screen readers
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub horizontal: FloatPosition,
    pub vertical: FloatPosition,
    pub width: Twips,
    pub height: Twips,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_emu: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_emu: Option<i64>,
    pub wrap: Wrap,
    /// stacking order among floating objects; higher is in front
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_order: Option<i64>,
    /// clockwise, in degrees, around the shape's centre
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    /// mirrored left to right, before rotating
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_horizontal: bool,
    /// mirrored top to bottom, before rotating
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_vertical: bool,
    pub geometry: Geometry,
    #[serde(default)]
    pub fill: Fill,
    /// the outline's line; `None` draws no line
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    /// text inside the shape
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<ShapeText>,
    /// set when the shape is a connector line between two other objects
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector: Option<Connector>,
    /// what the model does not understand (shadows, glow, 3d...), kept so
    /// that changing the shape loses none of it in the same format
    #[serde(default, skip_serializing_if = "DrawingPreserved::is_empty")]
    pub preserved: DrawingPreserved,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<ObjectAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<OriginalObject>,
}

impl Shape {
    /// a shape of one of the preset outlines, at an offset from the page
    /// corner, filled in `fill` (six hex digits) with a thin dark line.
    pub fn preset(
        name: &str,
        x: Twips,
        y: Twips,
        width: Twips,
        height: Twips,
        fill: &str,
    ) -> Shape {
        Shape {
            name: None,
            description: None,
            title: None,
            horizontal: FloatPosition::new("page", x),
            vertical: FloatPosition::new("page", y),
            width,
            height,
            width_emu: None,
            height_emu: None,
            wrap: Wrap::InFrontOfText,
            z_order: None,
            rotation: None,
            flip_horizontal: false,
            flip_vertical: false,
            geometry: Geometry::Preset {
                name: name.to_string(),
                adjust: Vec::new(),
            },
            fill: Fill::solid(fill),
            stroke: Some(Stroke::solid("1F3864", 15)),
            text: None,
            connector: None,
            preserved: DrawingPreserved::default(),
            anchor: None,
            original: None,
        }
    }

    pub fn size_emu(&self) -> (i64, i64) {
        (
            exact_emu(self.width_emu, self.width),
            exact_emu(self.height_emu, self.height),
        )
    }

    /// the preset name, or `None` for a free path.
    pub fn preset_name(&self) -> Option<&str> {
        match &self.geometry {
            Geometry::Preset { name, .. } => Some(name),
            Geometry::Path { .. } => None,
        }
    }

    /// a plain rectangle holding text, which is all a text box is: word
    /// and libreoffice both write text boxes that way, and the model's
    /// `TextBox` is what the editor and the layout engine know best. a
    /// shape with anything a text box cannot show stays a shape.
    pub fn as_text_box(&self) -> Option<TextBox> {
        let plain_outline = matches!(&self.geometry, Geometry::Preset { name, adjust } if name == "rect" && adjust.is_empty());
        let plain_fill = matches!(&self.fill, Fill::None | Fill::Solid { opacity: None, .. });
        let plain_line = self.stroke.as_ref().is_none_or(|s| {
            s.dash == "solid" && s.head.is_none() && s.tail.is_none() && s.opacity.is_none()
        });
        let text = self.text.as_ref()?;
        let plain = plain_outline
            && plain_fill
            && plain_line
            && !self.flip_horizontal
            && !self.flip_vertical
            && self.connector.is_none()
            && text.color.is_none();
        if !plain {
            return None;
        }
        let mut text_box = TextBox::new(0, 0, self.width, self.height, text.blocks.clone());
        text_box.name = self.name.clone();
        text_box.horizontal = self.horizontal.clone();
        text_box.vertical = self.vertical.clone();
        text_box.width_emu = self.width_emu;
        text_box.height_emu = self.height_emu;
        text_box.auto_height = text.auto_fit;
        text_box.wrap = self.wrap;
        text_box.z_order = self.z_order;
        text_box.rotation = self.rotation;
        text_box.margins = text.margins;
        text_box.vertical_align = text.vertical_align;
        text_box.border = self.stroke.as_ref().map(|stroke| super::block::Border {
            style: "single".into(),
            size: ((stroke.width_emu() * 8) / 12700).max(1) as u32,
            color: Some(stroke.color.clone()),
        });
        text_box.fill = match &self.fill {
            Fill::Solid { color, .. } => Some(color.clone()),
            _ => None,
        };
        text_box.preserved = self.preserved.clone();
        text_box.anchor = self.anchor.clone();
        Some(text_box)
    }

    /// a straight line (or a straight connector), drawn corner to corner
    /// of its box; the flips say which corners.
    pub fn is_line(&self) -> bool {
        matches!(self.preset_name(), Some("line" | "straightConnector1"))
    }
}

/// the outline of a shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Geometry {
    /// one of the standard outlines, named as drawingml names them
    /// ("rect", "roundRect", "ellipse", "triangle", "rightArrow",
    /// "wedgeRectCallout", "line"...); `shapes::odf_type` gives the
    /// opendocument name. `adjust` holds the handles' values ("adj" for
    /// the corner of a rounded rectangle) in drawingml's units
    Preset {
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        adjust: Vec<Adjust>,
    },
    /// a free outline: one or more paths in the shape's own coordinate
    /// space, stretched to the shape's size
    Path { paths: Vec<ShapePath> },
}

impl Geometry {
    pub fn preset(name: &str) -> Geometry {
        Geometry::Preset {
            name: name.to_string(),
            adjust: Vec::new(),
        }
    }
}

/// one adjustment handle of a preset outline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Adjust {
    pub name: String,
    pub value: i64,
}

/// one path of a free outline. its coordinates run from 0 to `width` and
/// 0 to `height`, whatever size the shape is drawn at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapePath {
    pub width: f64,
    pub height: f64,
    /// false for a path that is only drawn as a line
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub fill: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub stroke: bool,
    pub commands: Vec<PathCommand>,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// one step of a path, in the path's own coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum PathCommand {
    Move {
        x: f64,
        y: f64,
    },
    Line {
        x: f64,
        y: f64,
    },
    /// a cubic bézier curve through two control points
    Cubic {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    /// a quadratic bézier curve through one control point
    Quadratic {
        x1: f64,
        y1: f64,
        x: f64,
        y: f64,
    },
    Close,
}

/// what the inside of a shape is painted with.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Fill {
    /// see-through
    #[default]
    None,
    Solid {
        /// six hex digits
        color: String,
        /// 1 is opaque, 0 invisible; `None` is opaque
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
    },
    Gradient(Gradient),
    /// a picture stretched (or tiled) over the shape
    Picture {
        #[serde(rename = "mediaId")]
        media_id: String,
        #[serde(default, skip_serializing_if = "is_false")]
        tile: bool,
    },
}

impl Fill {
    pub fn solid(color: &str) -> Fill {
        Fill::Solid {
            color: color.to_string(),
            opacity: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gradient {
    pub kind: GradientKind,
    /// for a linear gradient: the direction colours change in, clockwise
    /// in degrees from left-to-right (90 is top to bottom)
    #[serde(default)]
    pub angle: f64,
    pub stops: Vec<GradientStop>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientKind {
    Linear,
    /// colours change outwards from the centre
    Radial,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStop {
    /// 0 at the start of the gradient, 1 at its end
    pub position: f64,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
}

/// the line drawn along a shape's outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stroke {
    /// six hex digits
    pub color: String,
    /// thickness in twips (20 to the point)
    pub width: Twips,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_emu: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    /// "solid", "dot", "dash", "dashDot", "longDash", "longDashDot",
    /// "longDashDotDot", "sysDot", "sysDash", "sysDashDot", "sysDashDotDot"
    #[serde(default = "solid", skip_serializing_if = "is_solid")]
    pub dash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<LineCap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<LineJoin>,
    /// the arrowhead at the start of the line
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<LineEnd>,
    /// the arrowhead at the end of the line
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<LineEnd>,
}

fn solid() -> String {
    "solid".into()
}

fn is_solid(dash: &String) -> bool {
    dash == "solid"
}

impl Stroke {
    pub fn solid(color: &str, width: Twips) -> Stroke {
        Stroke {
            color: color.to_string(),
            width,
            width_emu: None,
            opacity: None,
            dash: solid(),
            cap: None,
            join: None,
            head: None,
            tail: None,
        }
    }

    pub fn width_emu(&self) -> i64 {
        exact_emu(self.width_emu, self.width)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineCap {
    Flat,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

/// an arrowhead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEnd {
    /// "triangle", "stealth", "diamond", "oval", "arrow"
    pub kind: String,
    /// "sm", "med" or "lg"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<String>,
}

/// text inside a shape: the same blocks a text box holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeText {
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub margins: BoxMargins,
    /// where the text sits inside the shape; `None` is the top
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<VerticalAlign>,
    /// the shape grows to fit its text
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_fit: bool,
    /// the text colour the shape's style gives (word shapes default to
    /// white text from the theme); `None` is the document's own default
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl ShapeText {
    pub fn new(blocks: Vec<Block>) -> ShapeText {
        ShapeText {
            blocks,
            margins: BoxMargins::default(),
            vertical_align: None,
            auto_fit: false,
            color: None,
        }
    }
}

/// which objects a connector line joins.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connector {
    /// "straight", "elbow" or "curved"
    #[serde(default = "straight")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<ConnectionEnd>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<ConnectionEnd>,
}

fn straight() -> String {
    "straight".into()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEnd {
    /// the name of the object the end is glued to
    pub object: String,
    /// which of that object's connection points (0 is the first)
    #[serde(default)]
    pub site: u32,
}

/// a group: objects that move, resize and rotate together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub horizontal: FloatPosition,
    pub vertical: FloatPosition,
    pub width: Twips,
    pub height: Twips,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_emu: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_emu: Option<i64>,
    pub wrap: Wrap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_order: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_horizontal: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_vertical: bool,
    /// the group transform: children are placed in their own coordinate
    /// space, `child_offset` to `child_offset + child_size` (emu in word
    /// files), which is stretched over the group's box
    #[serde(default)]
    pub child_offset_x: i64,
    #[serde(default)]
    pub child_offset_y: i64,
    pub child_width: i64,
    pub child_height: i64,
    /// back to front
    pub children: Vec<GroupChild>,
    #[serde(default, skip_serializing_if = "DrawingPreserved::is_empty")]
    pub preserved: DrawingPreserved,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<ObjectAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<OriginalObject>,
}

impl Group {
    pub fn size_emu(&self) -> (i64, i64) {
        (
            exact_emu(self.width_emu, self.width),
            exact_emu(self.height_emu, self.height),
        )
    }
}

/// one object inside a group. its `horizontal` and `vertical` offsets and
/// its size are in the group's child coordinates (relative to "group",
/// using the `offset_emu`/`width_emu` fields for the exact numbers).
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GroupChild {
    Shape(Shape),
    TextBox(TextBox),
    Picture(Image),
    Group(Group),
}

/// how a drawing sits in the paragraph that anchors it in the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnchorWith {
    /// in an otherwise empty paragraph of its own
    #[default]
    Own,
    /// in the same paragraph as the object just before it
    Previous,
    /// in the next paragraph of text (written at `inline_index`)
    Next,
    /// on the page itself, outside any paragraph (opendocument's page
    /// anchor, `page_number` says which page); word files give it an
    /// empty paragraph of its own
    Page,
}

/// where in the file's paragraphs a drawing was anchored, so writing it
/// back adds no paragraph and loses none. objects without one get an
/// empty paragraph of their own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectAnchor {
    #[serde(default)]
    pub with: AnchorWith,
    /// the formatting of the empty paragraph holding the object (`Own`);
    /// its content is always empty
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<Paragraph>,
    /// for `Next`: which inline of the paragraph the object came before
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_index: Option<usize>,
    /// the formatting of the run that held the object
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
    /// for `Page`: the page the object is on, counting from 1
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_number: Option<u32>,
}

impl ObjectAnchor {
    pub fn with(with: AnchorWith) -> ObjectAnchor {
        ObjectAnchor {
            with,
            ..Default::default()
        }
    }
}

/// the xml an object was read from, written back unchanged while the
/// object's model still matches `fingerprint`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginalObject {
    pub format: SourceFormat,
    pub xml: String,
    /// `fingerprint_of` the object as it was read
    pub fingerprint: String,
}

/// the parts of a drawing the model does not understand, in the format
/// they came from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingPreserved {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<SourceFormat>,
    /// shape property elements (effects, shadow, 3d, extensions) as xml
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    /// style attributes (opendocument graphic properties) not modelled
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<Attribute>,
    /// word's style reference (`wps:style`), which gives a shape its theme
    /// colours and its text colour
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

impl DrawingPreserved {
    pub fn is_empty(&self) -> bool {
        self.children.is_empty() && self.attributes.is_empty() && self.style.is_none()
    }

    /// the preserved xml, only when it is in `format`.
    pub fn in_format(&self, format: SourceFormat) -> Option<&DrawingPreserved> {
        (self.format == Some(format)).then_some(self)
    }
}

impl TextBox {
    /// the box drawn the way a shape is: a rectangle with the box's fill,
    /// line and text. groups hold their text boxes like this in both
    /// formats (libreoffice does not draw a text frame inside a group).
    pub fn as_shape(&self) -> Shape {
        let mut shape = Shape::preset("rect", 0, 0, self.width, self.height, "FFFFFF");
        shape.name = self.name.clone();
        shape.horizontal = self.horizontal.clone();
        shape.vertical = self.vertical.clone();
        shape.width_emu = self.width_emu;
        shape.height_emu = self.height_emu;
        shape.wrap = self.wrap;
        shape.z_order = self.z_order;
        shape.rotation = self.rotation;
        shape.fill = match &self.fill {
            Some(hex) => Fill::solid(hex),
            None => Fill::None,
        };
        shape.stroke = self
            .border
            .as_ref()
            .filter(|border| border.style != "none" && border.style != "nil")
            .map(|border| {
                // a border is measured in eighths of a point, 12700 emu each
                let width_emu = i64::from(border.size) * 12700 / 8;
                let color = border.color.as_deref().unwrap_or("000000");
                let mut stroke = Stroke::solid(color, crate::units::emu_to_twips(width_emu));
                stroke.width_emu = Some(width_emu);
                stroke
            });
        shape.text = Some(ShapeText {
            blocks: self.blocks.clone(),
            margins: self.margins,
            vertical_align: self.vertical_align,
            auto_fit: self.auto_height,
            color: None,
        });
        shape.preserved = self.preserved.clone();
        shape
    }
}

impl Block {
    /// the text inside a text box, a shape or every member of a group, in
    /// order, for exports and searches that only want the words.
    pub fn drawing_blocks(&self) -> Vec<&Block> {
        fn child_blocks<'a>(child: &'a GroupChild, out: &mut Vec<&'a Block>) {
            match child {
                GroupChild::Shape(shape) => {
                    if let Some(text) = &shape.text {
                        out.extend(text.blocks.iter());
                    }
                }
                GroupChild::TextBox(text_box) => out.extend(text_box.blocks.iter()),
                GroupChild::Group(group) => {
                    for member in &group.children {
                        child_blocks(member, out);
                    }
                }
                GroupChild::Picture(_) => {}
            }
        }
        let mut out = Vec::new();
        match self {
            Block::TextBox(text_box) => out.extend(text_box.blocks.iter()),
            Block::Shape(shape) => {
                if let Some(text) = &shape.text {
                    out.extend(text.blocks.iter());
                }
            }
            Block::Group(group) => {
                for member in &group.children {
                    child_blocks(member, &mut out);
                }
            }
            _ => {}
        }
        out
    }
}

/// a stable fingerprint of any model value: fnv-1a over its json. the
/// object's own `original` and `anchor` are cleared by the callers first,
/// so moving an object between paragraphs keeps its original xml usable.
pub fn fingerprint_of<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_vec(value).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in json {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// the parts every positioned drawing shares, for code that treats text
/// boxes, shapes and groups alike.
pub trait Positioned {
    fn horizontal(&self) -> &FloatPosition;
    fn vertical(&self) -> &FloatPosition;
    fn size(&self) -> (Twips, Twips);
    fn z_order(&self) -> Option<i64>;
    fn set_position(&mut self, horizontal: FloatPosition, vertical: FloatPosition);
    fn set_z_order(&mut self, z: i64);
    fn anchor(&self) -> Option<&ObjectAnchor>;
    fn anchor_mut(&mut self) -> &mut Option<ObjectAnchor>;
    /// the fingerprint the object has now (`original` and `anchor` left out)
    fn current_fingerprint(&self) -> String;
    fn original(&self) -> Option<&OriginalObject>;
    fn set_original(&mut self, original: Option<OriginalObject>);
}

macro_rules! positioned {
    ($type:ty) => {
        impl Positioned for $type {
            fn horizontal(&self) -> &FloatPosition {
                &self.horizontal
            }
            fn vertical(&self) -> &FloatPosition {
                &self.vertical
            }
            fn size(&self) -> (Twips, Twips) {
                (self.width, self.height)
            }
            fn z_order(&self) -> Option<i64> {
                self.z_order
            }
            fn set_position(&mut self, horizontal: FloatPosition, vertical: FloatPosition) {
                self.horizontal = horizontal;
                self.vertical = vertical;
            }
            fn set_z_order(&mut self, z: i64) {
                self.z_order = Some(z);
            }
            fn anchor(&self) -> Option<&ObjectAnchor> {
                self.anchor.as_ref()
            }
            fn anchor_mut(&mut self) -> &mut Option<ObjectAnchor> {
                &mut self.anchor
            }
            fn current_fingerprint(&self) -> String {
                let mut plain = self.clone();
                plain.original = None;
                plain.anchor = None;
                fingerprint_of(&plain)
            }
            fn original(&self) -> Option<&OriginalObject> {
                self.original.as_ref()
            }
            fn set_original(&mut self, original: Option<OriginalObject>) {
                self.original = original;
            }
        }
    };
}

positioned!(Shape);
positioned!(Group);
positioned!(TextBox);

/// the original xml of an object, when it is in `format` and the object
/// has not changed since it was read.
pub fn reusable_original<T: Positioned>(object: &T, format: SourceFormat) -> Option<&str> {
    let original = object.original()?;
    (original.format == format && original.fingerprint == object.current_fingerprint())
        .then_some(original.xml.as_str())
}

/// records `xml` as the original of an object just read.
pub fn remember_original<T: Positioned>(object: &mut T, format: SourceFormat, xml: String) {
    object.set_original(None);
    let fingerprint = object.current_fingerprint();
    object.set_original(Some(OriginalObject {
        format,
        xml,
        fingerprint,
    }));
}
