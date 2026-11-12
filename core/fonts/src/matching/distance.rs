//! how far apart two outlines are, in em units.
//!
//! each outline is resampled into points spaced evenly along its
//! contours (so where a contour starts and how many points the font used
//! don't matter), and every point is measured to the nearest place on
//! the other outline's contours. the mean of those distances, both ways,
//! is the "chamfer" distance; the largest is the hausdorff distance. two
//! copies of one design come out at (almost) zero.

use std::collections::HashMap;

use super::shape::Shape;

/// the spacing of sample points along a contour, in em. a lowercase "o"
/// gets about 300 points.
pub const SAMPLE_SPACING: f64 = 0.008;

/// a resampled outline: points along each contour with the outward
/// normal at each, and a grid for finding the nearest piece quickly.
#[derive(Clone, Debug, Default)]
pub struct Samples {
    /// per contour, its points in order (the contour is closed)
    pub contours: Vec<Vec<(f64, f64)>>,
    /// per contour, the unit normal at each point, pointing away from the ink
    pub normals: Vec<Vec<(f64, f64)>>,
    grid: SegmentGrid,
}

impl Samples {
    /// samples a shape. the shape should have TrueType direction (see
    /// `Shape::with_truetype_direction`) for the normals to point outward.
    pub fn of(shape: &Shape) -> Samples {
        Samples::of_spacing(shape, SAMPLE_SPACING)
    }

    pub fn of_spacing(shape: &Shape, spacing: f64) -> Samples {
        let contours = shape
            .polygons(spacing / 2.0)
            .iter()
            .map(|polygon| resample(polygon, spacing))
            .filter(|points| points.len() >= 3)
            .collect();
        Samples::from_contours(contours)
    }

    /// samples from contours of points that are already evenly spaced.
    pub fn from_contours(contours: Vec<Vec<(f64, f64)>>) -> Samples {
        let normals = contours.iter().map(|points| normals_of(points)).collect();
        let grid = SegmentGrid::build(&contours);
        Samples {
            contours,
            normals,
            grid,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.contours.is_empty()
    }

    pub fn point_count(&self) -> usize {
        self.contours.iter().map(Vec::len).sum()
    }

    /// every point, whatever its contour.
    pub fn points(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.contours.iter().flatten().copied()
    }

    /// the samples with every point moved by `mapping`; the normals are
    /// worked out again from the moved points.
    pub fn mapped(&self, mapping: impl Fn(f64, f64) -> (f64, f64)) -> Samples {
        Samples::from_contours(
            self.contours
                .iter()
                .map(|points| points.iter().map(|p| mapping(p.0, p.1)).collect())
                .collect(),
        )
    }

    /// every point moved `amount` em along its outward normal: a positive
    /// amount makes the strokes heavier, a negative one lighter.
    pub fn offset(&self, amount: f64) -> Samples {
        self.offset_xy(amount, amount)
    }

    /// the same with separate sideways (`across`) and up-and-down (`up`)
    /// amounts, as `offset::offset_shape_xy` moves outlines.
    pub fn offset_xy(&self, across: f64, up: f64) -> Samples {
        if across == 0.0 && up == 0.0 {
            return self.clone();
        }
        Samples::from_contours(
            self.contours
                .iter()
                .zip(&self.normals)
                .map(|(points, normals)| {
                    points
                        .iter()
                        .zip(normals)
                        .map(|(p, n)| (p.0 + n.0 * across, p.1 + n.1 * up))
                        .collect()
                })
                .collect(),
        )
    }

    /// the box around the points.
    pub fn bounds(&self) -> Option<super::shape::Bounds> {
        super::shape::bounds_of(self.points())
    }

    /// the distance from `point` to the nearest piece of these contours.
    pub fn distance_to(&self, point: (f64, f64)) -> f64 {
        self.grid.nearest(point)
    }
}

/// how far apart two outlines are.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShapeDistance {
    /// the average distance from a point of one outline to the other, em
    pub mean: f64,
    /// the largest such distance (hausdorff), em
    pub max: f64,
}

/// the distance given when one outline is empty and the other isn't: as
/// far apart as two glyphs can sensibly be.
pub const FAR: f64 = 0.25;

/// the symmetric chamfer (mean) and hausdorff (max) distance.
pub fn shape_distance(a: &Samples, b: &Samples) -> ShapeDistance {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return ShapeDistance::default(),
        (true, false) | (false, true) => {
            return ShapeDistance {
                mean: FAR,
                max: FAR,
            }
        }
        _ => {}
    }
    let (mean_ab, max_ab) = directed(a, b);
    let (mean_ba, max_ba) = directed(b, a);
    ShapeDistance {
        mean: (mean_ab + mean_ba) / 2.0,
        max: max_ab.max(max_ba),
    }
}

/// the same distance after sliding `b` sideways so both boxes share a
/// centre: for comparing shapes whose spacing is judged separately.
pub fn centred_distance(a: &Samples, b: &Samples) -> ShapeDistance {
    let (Some(box_a), Some(box_b)) = (a.bounds(), b.bounds()) else {
        return shape_distance(a, b);
    };
    let shift = box_a.centre_x() - box_b.centre_x();
    shape_distance(a, &b.mapped(|x, y| (x + shift, y)))
}

fn directed(from: &Samples, to: &Samples) -> (f64, f64) {
    let mut total = 0.0;
    let mut largest: f64 = 0.0;
    let mut count = 0usize;
    for point in from.points() {
        let d = to.distance_to(point);
        total += d;
        largest = largest.max(d);
        count += 1;
    }
    (total / count.max(1) as f64, largest)
}

/// points every `spacing` em along a closed polygon, starting half a
/// step in.
fn resample(polygon: &[(f64, f64)], spacing: f64) -> Vec<(f64, f64)> {
    let mut points = Vec::new();
    let mut carried = spacing / 2.0;
    for (index, a) in polygon.iter().enumerate() {
        let b = polygon[(index + 1) % polygon.len()];
        let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let mut along = carried;
        while along < length {
            let t = along / length;
            points.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            along += spacing;
        }
        carried = along - length;
        if points.len() > 20_000 {
            break;
        }
    }
    points
}

/// the outward normal at each point of a closed run of points: the left
/// side of the direction of travel, which is away from the ink when the
/// contour has TrueType direction (ink on the right).
fn normals_of(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let count = points.len();
    (0..count)
        .map(|index| {
            let before = points[(index + count - 1) % count];
            let after = points[(index + 1) % count];
            let (dx, dy) = (after.0 - before.0, after.1 - before.1);
            let length = (dx * dx + dy * dy).sqrt();
            if length < 1e-12 {
                (0.0, 0.0)
            } else {
                (-dy / length, dx / length)
            }
        })
        .collect()
}

/// the pieces between neighbouring sample points, filed by grid cell.
#[derive(Clone, Debug, Default)]
struct SegmentGrid {
    cell: f64,
    segments: Vec<((f64, f64), (f64, f64))>,
    cells: HashMap<(i64, i64), Vec<usize>>,
    /// the reach of the furthest-out cell, so a search knows when to stop
    max_ring: i64,
}

impl SegmentGrid {
    const CELL: f64 = 0.03;

    fn build(contours: &[Vec<(f64, f64)>]) -> SegmentGrid {
        let mut grid = SegmentGrid {
            cell: Self::CELL,
            ..SegmentGrid::default()
        };
        for points in contours {
            for (index, a) in points.iter().enumerate() {
                let b = points[(index + 1) % points.len()];
                let id = grid.segments.len();
                grid.segments.push((*a, b));
                let middle = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                grid.cells.entry(grid.key(middle)).or_default().push(id);
            }
        }
        grid.max_ring = grid
            .cells
            .keys()
            .map(|(x, y)| x.abs().max(y.abs()))
            .max()
            .unwrap_or(0)
            + 2;
        grid
    }

    fn key(&self, point: (f64, f64)) -> (i64, i64) {
        (
            (point.0 / self.cell).floor().clamp(-1e6, 1e6) as i64,
            (point.1 / self.cell).floor().clamp(-1e6, 1e6) as i64,
        )
    }

    /// the distance to the nearest segment, searching rings of cells
    /// outward until no closer segment can be found.
    fn nearest(&self, point: (f64, f64)) -> f64 {
        if self.segments.is_empty() || !(point.0.is_finite() && point.1.is_finite()) {
            return FAR;
        }
        let centre = self.key(point);
        let mut best = f64::MAX;
        let mut ring = 0i64;
        let ring_limit = self.max_ring + centre.0.abs().max(centre.1.abs());
        loop {
            for (dx, dy) in ring_cells(ring) {
                if let Some(ids) = self.cells.get(&(centre.0 + dx, centre.1 + dy)) {
                    for id in ids {
                        let (a, b) = self.segments[*id];
                        best = best.min(point_to_segment(point, a, b));
                    }
                }
            }
            // a segment's midpoint is in its cell, but its ends can reach
            // half a sample spacing beyond; one extra ring covers that
            let reach = (ring as f64 - 1.0) * self.cell;
            if best <= reach || ring > ring_limit {
                break;
            }
            ring += 1;
        }
        best.min(FAR * 4.0)
    }
}

fn ring_cells(ring: i64) -> Vec<(i64, i64)> {
    if ring == 0 {
        return vec![(0, 0)];
    }
    let mut cells = Vec::with_capacity((8 * ring) as usize);
    for d in -ring..=ring {
        cells.push((d, -ring));
        cells.push((d, ring));
    }
    for d in -ring + 1..ring {
        cells.push((-ring, d));
        cells.push((ring, d));
    }
    cells
}

fn point_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length < 1e-18 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length).clamp(0.0, 1.0)
    };
    let (x, y) = (a.0 + dx * t, a.1 + dy * t);
    ((p.0 - x).powi(2) + (p.1 - y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::shape::tests::{ring, square};
    use crate::matching::shape::Shape;

    #[test]
    fn a_shape_is_no_distance_from_itself() {
        let shape = ring();
        let samples = Samples::of(&shape);
        let distance = shape_distance(&samples, &samples);
        assert!(distance.mean < 1e-9 && distance.max < 1e-9);
    }

    #[test]
    fn where_a_contour_starts_does_not_matter() {
        let shape = ring();
        let mut rotated = shape.clone();
        rotated.contours[0].nodes.rotate_left(1);
        rotated.contours.reverse();
        let distance = shape_distance(&Samples::of(&shape), &Samples::of(&rotated));
        assert!(distance.mean < 1e-6, "{distance:?}");
    }

    #[test]
    fn a_shift_shows_as_its_size() {
        let a = Shape {
            contours: vec![square(0.0, 0.0, 0.5, 0.5)],
        };
        let b = a.translated(0.01, 0.0);
        let distance = shape_distance(&Samples::of(&a), &Samples::of(&b));
        // the side edges are 0.01 apart, the top and bottom overlap
        assert!(
            distance.max > 0.009 && distance.max < 0.0102,
            "{distance:?}"
        );
        assert!(
            distance.mean > 0.002 && distance.mean < 0.006,
            "{distance:?}"
        );
        let centred = centred_distance(&Samples::of(&a), &Samples::of(&b));
        assert!(centred.mean < 1e-6);
    }

    #[test]
    fn normals_point_away_from_the_ink() {
        let shape = ring();
        let samples = Samples::of(&shape);
        // offsetting outward grows the outer square and shrinks the hole
        let heavier = samples.offset(0.02);
        let bounds = heavier.bounds().unwrap();
        assert!((bounds.x_min - 0.08).abs() < 0.002, "{bounds:?}");
        // the middle of the hole's left side moved right (points right at
        // a corner move diagonally, which is why only the middle is checked)
        let hole_left = heavier.contours[1]
            .iter()
            .filter(|p| (p.1 - 0.3).abs() < 0.05)
            .map(|p| p.0)
            .fold(f64::MAX, f64::min);
        assert!((hole_left - 0.22).abs() < 0.002, "{hole_left}");
    }

    #[test]
    fn empty_outlines_are_far_from_drawn_ones() {
        let empty = Samples::of(&Shape::default());
        let drawn = Samples::of(&ring());
        assert_eq!(shape_distance(&empty, &empty).mean, 0.0);
        assert_eq!(shape_distance(&empty, &drawn).mean, FAR);
    }
}
