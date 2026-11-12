//! vector paths: lines, curves and rectangles, and what happens to them
//! when they meet a redaction area.
//!
//! what is safe here needs care. a clipping path only hides: the path's
//! coordinates stay in the file, and anyone can read them back. that does
//! not matter for a table border, but a path can also be handwriting, a
//! signature, or text turned into outlines, and then its coordinates are
//! the secret. so by default a painted path that crosses an area is
//! removed whole, even the parts outside the area.
//!
//! two exceptions keep pages looking right without keeping anything:
//! - a filled rectangle (a background, a table cell, a highlight) is cut
//!   exactly: the parts outside the area are drawn as new rectangles and
//!   the part inside is gone. a rectangle's coordinates reveal nothing.
//! - with `clip_paths` set, crossing paths are kept and clipped so nothing
//!   is painted inside the area. this looks better but leaves the path's
//!   coordinates in the file, which is why it is not the default.
//!
//! a path used only as a clip (painted with `n`) is kept as it is:
//! removing it would change what is drawn outside the area, and nothing is
//! painted through it inside the area once the painting there is removed.

use lopdf::Object;

use crate::content::Op;
use crate::geometry::{Matrix, Point, Rect};
use crate::redact::walker::Walker;

pub fn is_path_operator(name: &str) -> bool {
    matches!(name, "m" | "l" | "c" | "v" | "y" | "h" | "re" | "W" | "W*")
}

pub fn is_paint_operator(name: &str) -> bool {
    matches!(
        name,
        "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n"
    )
}

fn strokes(paint: &str) -> bool {
    matches!(paint, "S" | "s" | "B" | "B*" | "b" | "b*")
}

fn fills_only(paint: &str) -> bool {
    matches!(paint, "f" | "F" | "f*")
}

/// the path being built, waiting for the operator that paints it.
#[derive(Default)]
pub struct PathBuilder {
    /// construction operators, in order (without W or W*)
    ops: Vec<Op>,
    /// W or W*, if the path also becomes the clip
    clip: Option<Op>,
    /// every point and control point, in page space
    points: Vec<Point>,
    /// the single rectangle the path is made of, in user space, if it is one
    rects: Vec<Rect>,
    curves_or_lines: bool,
    /// the matrix in force while the path was built
    ctm: Option<Matrix>,
}

impl PathBuilder {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty() && self.clip.is_none()
    }

    pub fn push(&mut self, op: Op, ctm: &Matrix) {
        self.ctm.get_or_insert(*ctm);
        match op.name() {
            "W" | "W*" => {
                self.clip = Some(op);
                return;
            }
            "re" => self.add_rect(&op, ctm),
            "h" => {}
            _ => {
                self.curves_or_lines = true;
                let values: Vec<f64> = op.operands().iter().filter_map(number_of).collect();
                for pair in values.chunks(2) {
                    if let [x, y] = pair {
                        self.points.push(ctm.apply(Point::new(*x, *y)));
                    }
                }
            }
        }
        self.ops.push(op);
    }

    fn add_rect(&mut self, op: &Op, ctm: &Matrix) {
        let values: Vec<f64> = op.operands().iter().filter_map(number_of).collect();
        if let [x, y, width, height] = values[..] {
            let rect = Rect::from_corners(x, y, x + width, y + height);
            self.rects.push(rect);
            self.points.extend(rect.transform(ctm).0);
        }
    }

    fn take(&mut self) -> PathBuilder {
        std::mem::take(self)
    }

    /// the path's operators, with the clip operator if there is one.
    fn into_ops(self, keep_clip: bool) -> Vec<Op> {
        let mut ops = self.ops;
        if keep_clip {
            ops.extend(self.clip);
        }
        ops
    }
}

fn number_of(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

impl Walker<'_, '_> {
    /// a path that is never painted is written out unchanged.
    pub fn flush_path(&mut self) {
        if !self.path.is_empty() {
            let path = self.path.take();
            for op in path.into_ops(true) {
                self.emit(op);
            }
        }
    }

    pub fn paint(&mut self, paint: Op) {
        let path = self.path.take();
        let name = paint.name().to_owned();
        let Some(bounds) = self.path_bounds(&path, &name) else {
            self.emit_path(path, paint);
            return;
        };
        let hits = self.areas_hitting(&bounds);
        if hits.is_empty() || name == "n" {
            self.emit_path(path, paint);
            return;
        }
        let area = self.first_area_hit(&bounds.transform(&Matrix::IDENTITY));
        self.touch();
        self.tally(area, false);
        let inside_one_area = hits.iter().any(|hit| hit.contains(&bounds));
        let ctm = path.ctm.unwrap_or(self.state.ctm);
        if !inside_one_area
            && fills_only(&name)
            && path.rects.len() == 1
            && !path.curves_or_lines
            && ctm.keeps_axes()
        {
            self.cut_rectangle(path, paint, &hits, &ctm);
        } else if !inside_one_area && self.settings().clip_paths {
            self.clip_path(path, paint, &hits, &ctm);
        } else {
            self.ctx.findings.counts.paths_removed += 1;
            self.keep_only_clip(path);
        }
    }

    fn emit_path(&mut self, path: PathBuilder, paint: Op) {
        for op in path.into_ops(true) {
            self.emit(op);
        }
        self.emit(paint);
    }

    /// a removed path that was also the clip still sets the clip, with
    /// `n` so it paints nothing.
    fn keep_only_clip(&mut self, path: PathBuilder) {
        if path.clip.is_some() {
            for op in path.into_ops(true) {
                self.emit(op);
            }
            self.emit(Op::new("n", vec![]));
        }
    }

    /// the page space box a path covers, with room for the stroke.
    fn path_bounds(&self, path: &PathBuilder, paint: &str) -> Option<Rect> {
        let bounds = Rect::around(&path.points)?;
        if !strokes(paint) {
            return Some(bounds);
        }
        // half the line width on each side, doubled to cover mitred
        // corners; a zero width line is drawn one device pixel wide
        let ctm = path.ctm.unwrap_or(self.state.ctm);
        let reach = (self.state.line_width.abs() * ctm.max_stretch()).max(1.0);
        Some(bounds.grow(reach))
    }

    /// draws what is left of a filled rectangle around the areas.
    fn cut_rectangle(&mut self, path: PathBuilder, paint: Op, hits: &[Rect], ctm: &Matrix) {
        let Some(to_user) = ctm.inverse() else {
            self.keep_only_clip(path);
            return;
        };
        let mut pieces = vec![path.rects[0].transform(ctm).bounds()];
        for hole in hits {
            pieces = pieces
                .iter()
                .flat_map(|piece| piece.subtract(hole))
                .collect();
        }
        self.ctx.findings.counts.paths_cut += 1;
        if !pieces.is_empty() {
            for piece in &pieces {
                let user = piece.transform(&to_user).bounds();
                self.emit(rect_op(&user));
            }
            self.emit(paint);
        }
        self.keep_only_clip(path);
    }

    /// keeps a crossing path but clips every area out of it.
    fn clip_path(&mut self, path: PathBuilder, paint: Op, hits: &[Rect], ctm: &Matrix) {
        let Some(clip) = complement_clip(hits, ctm) else {
            self.ctx.findings.counts.paths_removed += 1;
            self.keep_only_clip(path);
            return;
        };
        self.ctx.findings.counts.paths_clipped += 1;
        let has_clip = path.clip.is_some();
        let construction = path.ops.clone();
        self.emit(Op::new("q", vec![]));
        for op in clip {
            self.emit(op);
        }
        for op in construction.iter().cloned() {
            self.emit(op);
        }
        self.emit(paint);
        self.emit(Op::new("Q", vec![]));
        if has_clip {
            self.keep_only_clip(path);
        }
    }
}

/// operations that set a clip covering everything except the areas, in
/// the user space of `ctm`. each area is cut out with its own even-odd
/// clip (a huge rectangle with the area inside it), and clips add up, so
/// overlapping areas stay cut out too.
pub fn complement_clip(areas: &[Rect], ctm: &Matrix) -> Option<Vec<Op>> {
    let to_user = ctm.inverse()?;
    let everything = Rect::from_corners(-1.0e5, -1.0e5, 1.0e5, 1.0e5);
    let mut ops = Vec::new();
    for area in areas {
        ops.extend(polygon_ops(&everything.transform(&to_user).0));
        ops.extend(polygon_ops(&area.transform(&to_user).0));
        ops.push(Op::new("W*", vec![]));
        ops.push(Op::new("n", vec![]));
    }
    Some(ops)
}

fn polygon_ops(corners: &[Point; 4]) -> Vec<Op> {
    let mut ops = vec![Op::new("m", point_operands(corners[0]))];
    for corner in &corners[1..] {
        ops.push(Op::new("l", point_operands(*corner)));
    }
    ops.push(Op::new("h", vec![]));
    ops
}

fn point_operands(point: Point) -> Vec<Object> {
    vec![real(point.x), real(point.y)]
}

pub fn real(value: f64) -> Object {
    if (value - value.round()).abs() < 1e-6 && value.abs() < 1e12 {
        Object::Integer(value.round() as i64)
    } else {
        Object::Real(value as f32)
    }
}

pub fn rect_op(rect: &Rect) -> Op {
    Op::new(
        "re",
        vec![
            real(rect.x0),
            real(rect.y0),
            real(rect.width()),
            real(rect.height()),
        ],
    )
}
