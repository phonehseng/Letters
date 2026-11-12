//! the outline of a shape as drawingml stores it: a preset with handle
//! values, or custom paths whose coordinates may name guides.

use serde::{Deserialize, Serialize};

use super::source::RawChild;

/// the outline of a shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Geometry {
    /// `a:prstGeom`: one of the standard outlines with its handle values
    Preset {
        name: String,
        /// (guide name, value) from `a:avLst`, e.g. ("adj", 16667)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        adjust: Vec<(String, i64)>,
    },
    /// `a:custGeom`: guides (name, formula) and paths
    Custom {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        adjust: Vec<(String, String)>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        guides: Vec<(String, String)>,
        paths: Vec<ShapePath>,
        /// `a:cxnLst`, `a:rect`, `a:ahLst` kept as written
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        extra: Vec<RawChild>,
    },
}

impl Geometry {
    pub fn preset(name: &str) -> Geometry {
        Geometry::Preset {
            name: name.to_string(),
            adjust: Vec::new(),
        }
    }

    pub fn preset_name(&self) -> Option<&str> {
        match self {
            Geometry::Preset { name, .. } => Some(name),
            Geometry::Custom { .. } => None,
        }
    }
}

/// one path of a custom outline, in its own coordinate space
/// (`w` by `h`, stretched to the shape).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapePath {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    /// "norm" (default), "none", "lighten", "darken"...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(default = "super::yes")]
    pub stroke: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extrusion_ok: Option<bool>,
    pub commands: Vec<PathCommand>,
}

/// a coordinate that is a number or the name of a guide.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathPoint {
    pub x: String,
    pub y: String,
}

impl PathPoint {
    pub fn new(x: impl ToString, y: impl ToString) -> PathPoint {
        PathPoint {
            x: x.to_string(),
            y: y.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum PathCommand {
    Move {
        to: PathPoint,
    },
    Line {
        to: PathPoint,
    },
    /// an elliptical arc from the current point: radii, start angle and
    /// sweep (60000ths of a degree), each a number or a guide name
    Arc {
        width_radius: String,
        height_radius: String,
        start: String,
        swing: String,
    },
    Quadratic {
        control: PathPoint,
        to: PathPoint,
    },
    Cubic {
        control1: PathPoint,
        control2: PathPoint,
        to: PathPoint,
    },
    Close,
}
