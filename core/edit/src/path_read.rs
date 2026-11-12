//! what the app is told about vector drawings: each painted path with its
//! shape, colours and line style, and suggested groups of paths.
//!
//! a path is classified on screen, so a rectangle drawn in a turned page
//! still counts as a rectangle when its sides look upright.

use crate::geometry::{Point, Rect};
use crate::interpret::{segment_points, FillColor, PathRecord, Segment};
use crate::layout::{source_key, PageModel};
use crate::model::{
    GroupReason, ImageObject, PaintInfo, PathGroup, PathObject, PathShape, StrokeInfo,
};

/// how close (points) two paths may be and still count as touching, for
/// suggesting that many small paths are one logo.
const CLUSTER_GAP: f32 = 2.0;
/// a suggested group covering more of the page than this is a page frame
/// or background, not an object someone wants to select
const MAX_GROUP_SHARE: f32 = 0.5;
/// paths count as one sharp corner when they are this close on screen
const CORNER_TOLERANCE: f32 = 0.05;

impl PageModel {
    pub fn path_id(&self, path: usize) -> String {
        let record = &self.run.paths[path];
        format!(
            "path:{:08x}:{}:{}",
            self.fingerprint,
            source_key(&self.run, record.source),
            record.paint_op
        )
    }

    pub fn find_path(&self, id: &str) -> Result<usize, crate::error::EditError> {
        (0..self.run.paths.len())
            .find(|path| self.path_id(*path) == id)
            .ok_or(crate::error::EditError::UnknownObject)
    }

    /// a path's points on screen (control points included).
    pub fn visible_points(&self, path: usize) -> Vec<Point> {
        let record = &self.run.paths[path];
        segment_points(&record.segments)
            .into_iter()
            .map(|point| self.run.space.visible_point(record.ctm.apply(point)))
            .collect()
    }

    /// the box a path covers on screen, with half its line width around
    /// it when stroked. a shading covers its clip.
    pub fn path_rect(&self, path: usize) -> Rect {
        let record = &self.run.paths[path];
        if record.paint.shading || record.segments.is_empty() {
            let bounds = record.clip_bounds.unwrap_or([0.0, 0.0, 0.0, 0.0]);
            let corners = [
                Point::new(bounds[0], bounds[1]),
                Point::new(bounds[2], bounds[3]),
            ];
            return self
                .run
                .space
                .visible_rect_of(&corners)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        }
        let rect = Rect::around(self.visible_points(path)).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        if !record.paint.stroke {
            return rect;
        }
        let half = self.visible_line_width(path) / 2.0;
        Rect::new(
            rect.x - half,
            rect.y - half,
            rect.width + 2.0 * half,
            rect.height + 2.0 * half,
        )
    }

    /// the line width as seen, in points.
    pub fn visible_line_width(&self, path: usize) -> f32 {
        let record = &self.run.paths[path];
        let scale = (record.ctm.a * record.ctm.d - record.ctm.b * record.ctm.c)
            .abs()
            .sqrt();
        record.line.width * scale
    }

    pub fn path_shape(&self, path: usize) -> (PathShape, Vec<Point>) {
        let record = &self.run.paths[path];
        if record.paint.shading {
            return (PathShape::Shading, Vec::new());
        }
        let to_screen = |point: Point| self.run.space.visible_point(record.ctm.apply(point));
        match record.segments.as_slice() {
            [Segment::Rect(at, w, h)] => {
                let corners = [
                    Point::new(at.x, at.y + h),
                    Point::new(at.x + w, at.y + h),
                    Point::new(at.x, at.y),
                    Point::new(at.x + w, at.y),
                ];
                rectangle_on_screen(corners.map(to_screen))
            }
            [Segment::Move(a), Segment::Line(b)] => {
                (PathShape::Line, vec![to_screen(*a), to_screen(*b)])
            }
            [Segment::Move(start), rest @ ..] if is_closed_polygon(rest, 4) => {
                let mut corners = vec![to_screen(*start)];
                corners.extend(rest.iter().take(3).filter_map(|segment| match segment {
                    Segment::Line(at) => Some(to_screen(*at)),
                    _ => None,
                }));
                let quad = sorted_corners(&corners);
                rectangle_on_screen(quad)
            }
            [Segment::Move(_), rest @ ..] if is_ellipse(rest) => (PathShape::Ellipse, Vec::new()),
            _ => (PathShape::Free, Vec::new()),
        }
    }

    pub fn path_object(&self, path: usize) -> PathObject {
        let record = &self.run.paths[path];
        let (shape, points) = self.path_shape(path);
        let rect = self.path_rect(path);
        let clipped = record.clip_bounds.is_some_and(|bounds| {
            let corners = [
                Point::new(bounds[0], bounds[1]),
                Point::new(bounds[2], bounds[3]),
            ];
            self.run
                .space
                .visible_rect_of(&corners)
                .is_some_and(|clip| !contains(&clip, &rect))
        });
        let scale = self.visible_line_width(path) / record.line.width.max(1e-6);
        PathObject {
            id: self.path_id(path),
            rect,
            shape,
            fill: (record.paint.fill || record.paint.shading).then(|| paint_info(&record.fill)),
            stroke: record.paint.stroke.then(|| StrokeInfo {
                paint: paint_info(&record.stroke),
                width: self.visible_line_width(path),
                dash: record
                    .line
                    .dash
                    .iter()
                    .map(|length| length * scale)
                    .collect(),
            }),
            fill_opacity: record.fill_alpha,
            stroke_opacity: record.stroke_alpha,
            clipped,
            point_editable: matches!(shape, PathShape::Rectangle | PathShape::Line)
                && !record.paint.clip,
            points,
        }
    }

    /// paths that are likely one object to the eye.
    pub fn path_groups(&self, paths: &[PathObject]) -> Vec<PathGroup> {
        let page_area = (self.run.space.width * self.run.space.height).max(1.0);
        let mut groups: Vec<(Vec<usize>, GroupReason)> = Vec::new();
        groups.extend(self.groups_by(
            |record| (record.block != 0).then_some((record.source, record.block)),
            GroupReason::SameBlock,
        ));
        groups.extend(self.groups_by(
            |record| record.marked.map(|marked| (record.source, marked)),
            GroupReason::MarkedContent,
        ));
        groups.extend(
            self.clusters(paths)
                .into_iter()
                .map(|members| (members, GroupReason::Cluster)),
        );
        let mut seen: Vec<Vec<usize>> = Vec::new();
        let mut result = Vec::new();
        for (members, reason) in groups {
            if members.len() < 2 || seen.contains(&members) {
                continue;
            }
            let rect = members
                .iter()
                .skip(1)
                .fold(paths[members[0]].rect, |all, index| {
                    all.union(&paths[*index].rect)
                });
            if rect.width * rect.height > MAX_GROUP_SHARE * page_area {
                continue;
            }
            result.push(PathGroup {
                id: format!("group:{}", paths[members[0]].id),
                path_ids: members
                    .iter()
                    .map(|index| paths[*index].id.clone())
                    .collect(),
                rect,
                reason,
            });
            seen.push(members);
        }
        result
    }

    /// paths with the same key, in drawing order.
    fn groups_by<K: PartialEq>(
        &self,
        key: impl Fn(&PathRecord) -> Option<K>,
        reason: GroupReason,
    ) -> Vec<(Vec<usize>, GroupReason)> {
        let mut groups: Vec<(K, Vec<usize>)> = Vec::new();
        for (index, record) in self.run.paths.iter().enumerate() {
            let Some(value) = key(record) else { continue };
            match groups.iter_mut().find(|(existing, _)| *existing == value) {
                Some((_, members)) => members.push(index),
                None => groups.push((value, vec![index])),
            }
        }
        groups
            .into_iter()
            .map(|(_, members)| (members, reason))
            .collect()
    }

    /// runs of consecutive paths that touch: logos are usually drawn as
    /// many paths, one after another, overlapping or nearly so.
    fn clusters(&self, paths: &[PathObject]) -> Vec<Vec<usize>> {
        let mut clusters: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();
        let mut bounds: Option<Rect> = None;
        for (index, path) in paths.iter().enumerate() {
            let touches = bounds.is_some_and(|area| near(&area, &path.rect, CLUSTER_GAP));
            if touches {
                current.push(index);
                bounds = bounds.map(|area| area.union(&path.rect));
            } else {
                if current.len() >= 3 {
                    clusters.push(std::mem::take(&mut current));
                }
                current = vec![index];
                bounds = Some(path.rect);
            }
        }
        if current.len() >= 3 {
            clusters.push(current);
        }
        clusters
    }

    /// the crop, resolution and opacity of an image.
    pub fn image_extras(&self, image: usize, object: &mut ImageObject) {
        let record = &self.run.images[image];
        let quad = self.image_quad(image);
        let across = quad[1].minus(quad[0]).length() / 72.0;
        let down = quad[0].minus(quad[2]).length() / 72.0;
        if across > 0.0 && down > 0.0 && object.pixel_width > 0 {
            let ppi_x = object.pixel_width as f32 / across;
            let ppi_y = object.pixel_height as f32 / down;
            object.pixels_per_inch = ppi_x.min(ppi_y);
        }
        object.opacity = record.fill_alpha;
        if let Some(bounds) = record.clip_bounds {
            let corners = [
                Point::new(bounds[0], bounds[1]),
                Point::new(bounds[2], bounds[3]),
            ];
            if let Some(clip) = self.run.space.visible_rect_of(&corners) {
                if !contains(&clip, &object.rect) {
                    object.crop_rect = intersect(&clip, &object.rect);
                }
            }
        }
    }
}

fn paint_info(color: &FillColor) -> PaintInfo {
    PaintInfo {
        color: color.color,
        color_space: if color.pattern_space {
            "Pattern".into()
        } else {
            color.space.clone()
        },
        components: color.components.clone(),
    }
}

/// a rectangle when the four corners (top left, top right, bottom left,
/// bottom right) make upright sides on screen.
fn rectangle_on_screen(quad: [Point; 4]) -> (PathShape, Vec<Point>) {
    let rect = Rect::around(quad).expect("four corners");
    let corners = rect.corners();
    let upright = quad.iter().all(|point| {
        corners
            .iter()
            .any(|corner| corner.minus(*point).length() < CORNER_TOLERANCE)
    });
    if upright {
        (PathShape::Rectangle, corners.to_vec())
    } else {
        (PathShape::Free, Vec::new())
    }
}

/// four points as top left, top right, bottom left, bottom right.
fn sorted_corners(points: &[Point]) -> [Point; 4] {
    let rect = Rect::around(points.iter().copied()).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
    let nearest = |target: Point| {
        points
            .iter()
            .copied()
            .min_by(|a, b| {
                a.minus(target)
                    .length()
                    .total_cmp(&b.minus(target).length())
            })
            .unwrap_or(target)
    };
    rect.corners().map(nearest)
}

/// three or four lines (the fourth back to the start), then maybe a close.
fn is_closed_polygon(rest: &[Segment], corners: usize) -> bool {
    let lines = rest
        .iter()
        .take_while(|segment| matches!(segment, Segment::Line(_)))
        .count();
    let after = &rest[lines..];
    (lines == corners - 1 || lines == corners)
        && after
            .iter()
            .all(|segment| matches!(segment, Segment::Close))
}

/// four curves (then maybe a close) whose ends sit on the middles of the
/// box's sides: how ellipses are drawn.
fn is_ellipse(rest: &[Segment]) -> bool {
    let curves: Vec<Point> = rest
        .iter()
        .filter_map(|segment| match segment {
            Segment::Curve(_, _, end) => Some(*end),
            _ => None,
        })
        .collect();
    let others_closed = rest
        .iter()
        .all(|segment| matches!(segment, Segment::Curve(..) | Segment::Close));
    if curves.len() != 4 || !others_closed {
        return false;
    }
    let Some(rect) = Rect::around(curves.iter().copied()) else {
        return false;
    };
    let middle = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let tolerance = 0.05 * rect.width.max(rect.height);
    curves
        .iter()
        .all(|end| (end.x - middle.x).abs() < tolerance || (end.y - middle.y).abs() < tolerance)
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    let slack = 0.5;
    inner.x >= outer.x - slack
        && inner.y >= outer.y - slack
        && inner.right() <= outer.right() + slack
        && inner.bottom() <= outer.bottom() + slack
}

pub(crate) fn intersect(a: &Rect, b: &Rect) -> Option<Rect> {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    (right > left && bottom > top).then(|| Rect::new(left, top, right - left, bottom - top))
}

pub(crate) fn near(a: &Rect, b: &Rect, gap: f32) -> bool {
    a.x - gap <= b.right()
        && b.x - gap <= a.right()
        && a.y - gap <= b.bottom()
        && b.y - gap <= a.bottom()
}
