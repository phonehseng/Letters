//! the objects on a slide: shapes and text boxes, pictures, groups,
//! connectors, tables and graphic frames (charts, smartart, ole objects).
//!
//! the shapes mirror drawingml closely (`p:sp`, `p:pic`, `p:grpSp`,
//! `p:cxnSp`, `p:graphicFrame`) because pptx is the richer of the two
//! formats; odp is mapped onto the same types.

use serde::{Deserialize, Serialize};

use super::fill::{Fill, Line, StyleRef};
use super::frames::GraphicFrame;
use super::geometry::Geometry;
use super::source::{RawChild, Source};
use super::text::TextBody;
use super::Emu;

/// one object on a slide, layout or master. the json tag is `type`.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Object {
    /// an autoshape, a text box or a placeholder (`p:sp`)
    Shape(Shape),
    Picture(Picture),
    Group(Group),
    Connector(Connector),
    /// a table, chart, smartart diagram, ole object... (`p:graphicFrame`)
    Frame(GraphicFrame),
    /// something the model does not read at all (`p:contentPart`, an
    /// unknown `mc:AlternateContent`), kept exactly
    Preserved {
        xml: String,
    },
}

impl Object {
    /// the object's id on its slide (`p:cNvPr id`), 0 for preserved xml.
    pub fn id(&self) -> u32 {
        match self {
            Object::Shape(shape) => shape.id,
            Object::Picture(picture) => picture.id,
            Object::Group(group) => group.id,
            Object::Connector(connector) => connector.id,
            Object::Frame(frame) => frame.id,
            Object::Preserved { .. } => 0,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Object::Shape(shape) => &shape.name,
            Object::Picture(picture) => &picture.name,
            Object::Group(group) => &group.name,
            Object::Connector(connector) => &connector.name,
            Object::Frame(frame) => &frame.name,
            Object::Preserved { .. } => "",
        }
    }

    pub fn transform(&self) -> Option<&Transform> {
        match self {
            Object::Shape(shape) => shape.transform.as_ref(),
            Object::Picture(picture) => picture.transform.as_ref(),
            Object::Group(group) => Some(&group.transform),
            Object::Connector(connector) => connector.transform.as_ref(),
            Object::Frame(frame) => frame.transform.as_ref(),
            Object::Preserved { .. } => None,
        }
    }

    pub fn transform_mut(&mut self) -> Option<&mut Option<Transform>> {
        match self {
            Object::Shape(shape) => Some(&mut shape.transform),
            Object::Picture(picture) => Some(&mut picture.transform),
            Object::Connector(connector) => Some(&mut connector.transform),
            Object::Frame(frame) => Some(&mut frame.transform),
            Object::Group(_) | Object::Preserved { .. } => None,
        }
    }

    pub fn placeholder(&self) -> Option<&Placeholder> {
        match self {
            Object::Shape(shape) => shape.placeholder.as_ref(),
            Object::Picture(picture) => picture.placeholder.as_ref(),
            Object::Frame(frame) => frame.placeholder.as_ref(),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&TextBody> {
        match self {
            Object::Shape(shape) => shape.text.as_ref(),
            _ => None,
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut TextBody> {
        match self {
            Object::Shape(shape) => shape.text.as_mut(),
            _ => None,
        }
    }

    pub fn source(&self) -> Option<&Source> {
        match self {
            Object::Shape(shape) => shape.source.as_ref(),
            Object::Picture(picture) => picture.source.as_ref(),
            Object::Group(group) => group.source.as_ref(),
            Object::Connector(connector) => connector.source.as_ref(),
            Object::Frame(frame) => frame.source.as_ref(),
            Object::Preserved { .. } => None,
        }
    }

    pub fn set_source(&mut self, source: Option<Source>) {
        match self {
            Object::Shape(shape) => shape.source = source,
            Object::Picture(picture) => picture.source = source,
            Object::Group(group) => group.source = source,
            Object::Connector(connector) => connector.source = source,
            Object::Frame(frame) => frame.source = source,
            Object::Preserved { .. } => {}
        }
    }

    pub fn set_id(&mut self, id: u32) {
        match self {
            Object::Shape(shape) => shape.id = id,
            Object::Picture(picture) => picture.id = id,
            Object::Group(group) => group.id = id,
            Object::Connector(connector) => connector.id = id,
            Object::Frame(frame) => frame.id = id,
            Object::Preserved { .. } => {}
        }
    }
}

/// position, size, rotation and flips (`a:xfrm`). a group also maps its
/// children's coordinate space (`a:chOff`, `a:chExt`) onto its box.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub x: Emu,
    pub y: Emu,
    pub width: Emu,
    pub height: Emu,
    /// clockwise around the centre, 60000ths of a degree
    #[serde(default)]
    pub rotation: i32,
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_x: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_y: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_width: Option<Emu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_height: Option<Emu>,
}

impl Transform {
    pub fn new(x: Emu, y: Emu, width: Emu, height: Emu) -> Transform {
        Transform {
            x,
            y,
            width,
            height,
            ..Default::default()
        }
    }
}

/// what makes a shape a placeholder (`p:ph`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placeholder {
    pub kind: PlaceholderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// "horz" or "vert"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orient: Option<String>,
    /// "full", "half", "quarter"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default)]
    pub custom_prompt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaceholderKind {
    Title,
    CenteredTitle,
    Subtitle,
    Body,
    /// "obj": any content; the default when a placeholder names no type
    Object,
    Chart,
    Table,
    ClipArt,
    Diagram,
    Media,
    Picture,
    SlideImage,
    Date,
    Footer,
    Header,
    SlideNumber,
}

impl PlaceholderKind {
    pub const ALL: [(PlaceholderKind, &'static str); 16] = [
        (PlaceholderKind::Title, "title"),
        (PlaceholderKind::CenteredTitle, "ctrTitle"),
        (PlaceholderKind::Subtitle, "subTitle"),
        (PlaceholderKind::Body, "body"),
        (PlaceholderKind::Object, "obj"),
        (PlaceholderKind::Chart, "chart"),
        (PlaceholderKind::Table, "tbl"),
        (PlaceholderKind::ClipArt, "clipArt"),
        (PlaceholderKind::Diagram, "dgm"),
        (PlaceholderKind::Media, "media"),
        (PlaceholderKind::Picture, "pic"),
        (PlaceholderKind::SlideImage, "sldImg"),
        (PlaceholderKind::Date, "dt"),
        (PlaceholderKind::Footer, "ftr"),
        (PlaceholderKind::Header, "hdr"),
        (PlaceholderKind::SlideNumber, "sldNum"),
    ];

    pub fn from_pptx(name: &str) -> PlaceholderKind {
        PlaceholderKind::ALL
            .iter()
            .find(|(_, known)| *known == name)
            .map(|(kind, _)| *kind)
            .unwrap_or(PlaceholderKind::Object)
    }

    pub fn pptx_name(self) -> &'static str {
        PlaceholderKind::ALL
            .iter()
            .find(|(kind, _)| *kind == self)
            .map(|(_, name)| *name)
            .unwrap_or("obj")
    }

    /// titles take the master's title style, the date, footer and slide
    /// number its "other" style, everything else its body style.
    pub fn is_title(self) -> bool {
        matches!(
            self,
            PlaceholderKind::Title | PlaceholderKind::CenteredTitle
        )
    }

    pub fn is_furniture(self) -> bool {
        matches!(
            self,
            PlaceholderKind::Date
                | PlaceholderKind::Footer
                | PlaceholderKind::Header
                | PlaceholderKind::SlideNumber
        )
    }
}

/// an autoshape, text box or placeholder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// alternative text
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    /// `txBox="1"`: made with the text box tool
    #[serde(default)]
    pub text_box: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<Placeholder>,
    /// `None` inherits the placeholder's position from the layout
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<Line>,
    /// theme style references (`p:style`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ShapeStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextBody>,
    /// shape property children not modelled (effects, 3d, extensions)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// theme style references of a shape (`p:style`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeStyle {
    pub line: StyleRef,
    pub fill: StyleRef,
    pub effect: StyleRef,
    /// the font reference: index 1 is the major (heading) font, 2 the
    /// minor (body) font, 0 none; its colour is the text colour
    pub font: StyleRef,
}

/// a picture, or a video or sound with its poster frame.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Picture {
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
    /// the picture's bytes in the media table
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<String>,
    /// how much is cut off each side (`a:srcRect` l, t, r, b), thousandths
    /// of a percent of the picture; negative values add space
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<[i32; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<Line>,
    /// recolouring and other picture effects inside `a:blip`
    /// (`a:alphaModFix`, `a:duotone`, `a:grayscl`, `a:lum`...), kept
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blip_extra: Vec<RawChild>,
    /// set for a video or sound: "video" or "audio", and its media id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// a group of objects that move, resize and rotate together.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    /// the group's box and its children's coordinate space
    pub transform: Transform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    pub objects: Vec<Object>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// a line or connector, possibly glued to other objects.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connector {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    /// "straightConnector1", "bentConnector3", "curvedConnector3"...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ShapeStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Connection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<Connection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<RawChild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// where a connector end is glued: an object id and one of its
/// connection sites (0 is the first; a rectangle's are top, left,
/// bottom, right).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: u32,
    pub site: u32,
}
