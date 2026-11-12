//! a slide as a list of drawing operations in points, y down from the
//! slide's top left: what the pdf writer draws, and what the editor's
//! canvas can draw too.

use crate::color::Rgba;
use crate::fonts::FontId;
use crate::geometry::{OutlinePath, Segment};

/// an affine transform: x' = a x + c y + e, y' = b x + d y + f.
pub type Matrix = [f64; 6];

pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `outer` applied after `inner`.
pub fn multiply(outer: &Matrix, inner: &Matrix) -> Matrix {
    [
        outer[0] * inner[0] + outer[2] * inner[1],
        outer[1] * inner[0] + outer[3] * inner[1],
        outer[0] * inner[2] + outer[2] * inner[3],
        outer[1] * inner[2] + outer[3] * inner[3],
        outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
        outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
    ]
}

pub fn translate(x: f64, y: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

pub fn scale(x: f64, y: f64) -> Matrix {
    [x, 0.0, 0.0, y, 0.0, 0.0]
}

/// clockwise by `degrees` (y points down, so this is visually clockwise).
pub fn rotate(degrees: f64) -> Matrix {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [cos, sin, -sin, cos, 0.0, 0.0]
}

pub fn apply(matrix: &Matrix, x: f64, y: f64) -> (f64, f64) {
    (
        matrix[0] * x + matrix[2] * y + matrix[4],
        matrix[1] * x + matrix[3] * y + matrix[5],
    )
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Solid(Rgba),
    /// stops at 0 to 1 along the line from `from` to `to` (local points)
    Linear {
        stops: Vec<(f64, Rgba)>,
        from: (f64, f64),
        to: (f64, f64),
    },
    /// stops from the centre (0) to `radius` (1)
    Radial {
        stops: Vec<(f64, Rgba)>,
        centre: (f64, f64),
        radius: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    pub color: Rgba,
    pub width: f64,
    /// dash and gap lengths in points; empty for a solid line
    pub dashes: Vec<f64>,
    /// 0 butt, 1 round, 2 square (pdf's numbers)
    pub cap: u8,
    /// 0 miter, 1 round, 2 bevel
    pub join: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Fill {
        matrix: Matrix,
        paths: Vec<OutlinePath>,
        paint: Paint,
    },
    Stroke {
        matrix: Matrix,
        paths: Vec<OutlinePath>,
        style: StrokeStyle,
    },
    /// a picture filling the unit square under `matrix`, clipped to
    /// `clip` (under `clip_matrix`) when given
    Image {
        matrix: Matrix,
        media_id: String,
        clip: Option<(Matrix, Vec<OutlinePath>)>,
        alpha: f64,
    },
    Text {
        matrix: Matrix,
        font: FontId,
        size: f64,
        color: Rgba,
        /// (glyph, x, y of the baseline, the character)
        glyphs: Vec<(u16, f64, f64, char)>,
        synthetic_bold: bool,
        synthetic_italic: bool,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub items: Vec<Item>,
}

/// a rectangle as an outline path, for backgrounds, cells and pictures.
pub fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Vec<OutlinePath> {
    vec![OutlinePath {
        segments: vec![
            Segment::Move(x, y),
            Segment::Line(x + width, y),
            Segment::Line(x + width, y + height),
            Segment::Line(x, y + height),
            Segment::Close,
        ],
        fill: true,
        stroke: true,
    }]
}

/// a line between two points as an open path.
pub fn segment_path(from: (f64, f64), to: (f64, f64)) -> Vec<OutlinePath> {
    vec![OutlinePath {
        segments: vec![Segment::Move(from.0, from.1), Segment::Line(to.0, to.1)],
        fill: false,
        stroke: true,
    }]
}
