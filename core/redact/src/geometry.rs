//! points, rectangles and the affine matrices pdf uses to place things.
//!
//! pdf writes a matrix as six numbers `a b c d e f` and applies it to a
//! point as a row vector: `x' = a*x + c*y + e`, `y' = b*x + d*y + f`.
//! `cm` in a content stream puts a new matrix in front of the current one,
//! so "first this, then that" is `this.then(&that)`.

/// how close two numbers must be to count as touching rather than
/// overlapping. a glyph whose box only touches a redaction box along an
/// edge is left alone, so a box drawn exactly around a word does not eat
/// the letters next to it.
pub const OVERLAP_EPSILON: f64 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    pub fn translate(x: f64, y: f64) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    pub fn scale(x: f64, y: f64) -> Self {
        Self::new(x, 0.0, 0.0, y, 0.0, 0.0)
    }

    /// reads six numbers, as found in `cm`, `Tm`, /Matrix and /FontMatrix.
    pub fn from_numbers(numbers: &[f64]) -> Option<Self> {
        match numbers {
            [a, b, c, d, e, f] => Some(Self::new(*a, *b, *c, *d, *e, *f)),
            _ => None,
        }
    }

    /// applies `self` first and `next` after it.
    pub fn then(&self, next: &Matrix) -> Matrix {
        Matrix {
            a: self.a * next.a + self.b * next.c,
            b: self.a * next.b + self.b * next.d,
            c: self.c * next.a + self.d * next.c,
            d: self.c * next.b + self.d * next.d,
            e: self.e * next.a + self.f * next.c + next.e,
            f: self.e * next.b + self.f * next.d + next.f,
        }
    }

    pub fn apply(&self, point: Point) -> Point {
        Point {
            x: self.a * point.x + self.c * point.y + self.e,
            y: self.b * point.x + self.d * point.y + self.f,
        }
    }

    /// applies only the stretching and turning part, for directions and sizes.
    pub fn apply_vector(&self, x: f64, y: f64) -> Point {
        Point {
            x: self.a * x + self.c * y,
            y: self.b * x + self.d * y,
        }
    }

    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// the matrix that undoes this one, or none when it squashes
    /// everything onto a line or a point.
    pub fn inverse(&self) -> Option<Matrix> {
        let det = self.determinant();
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        Some(Matrix {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            e: (self.c * self.f - self.d * self.e) / det,
            f: (self.b * self.e - self.a * self.f) / det,
        })
    }

    /// true when the matrix keeps rectangles axis aligned (no turning
    /// other than by quarter turns, no slanting).
    pub fn keeps_axes(&self) -> bool {
        (self.b.abs() < 1e-9 && self.c.abs() < 1e-9) || (self.a.abs() < 1e-9 && self.d.abs() < 1e-9)
    }

    /// how much the matrix stretches lengths at most, used to turn a line
    /// width into page units.
    pub fn max_stretch(&self) -> f64 {
        let first = (self.a * self.a + self.b * self.b).sqrt();
        let second = (self.c * self.c + self.d * self.d).sqrt();
        first.max(second)
    }
}

/// an axis aligned rectangle, stored by its corners with x0 <= x1 and y0 <= y1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    /// builds a rectangle from any two opposite corners.
    pub fn from_corners(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    /// the smallest rectangle around some points.
    pub fn around(points: &[Point]) -> Option<Self> {
        let first = points.first()?;
        let mut rect = Rect::from_corners(first.x, first.y, first.x, first.y);
        for point in &points[1..] {
            rect = rect.union(&Rect::from_corners(point.x, point.y, point.x, point.y));
        }
        Some(rect)
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    pub fn union(&self, other: &Rect) -> Rect {
        Rect {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    pub fn grow(&self, by: f64) -> Rect {
        Rect {
            x0: self.x0 - by,
            y0: self.y0 - by,
            x1: self.x1 + by,
            y1: self.y1 + by,
        }
    }

    /// true when the two share some area, not just an edge.
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x0 < other.x1 - OVERLAP_EPSILON
            && other.x0 < self.x1 - OVERLAP_EPSILON
            && self.y0 < other.y1 - OVERLAP_EPSILON
            && other.y0 < self.y1 - OVERLAP_EPSILON
    }

    /// true when `other` lies entirely inside this rectangle.
    pub fn contains(&self, other: &Rect) -> bool {
        other.x0 >= self.x0 - OVERLAP_EPSILON
            && other.x1 <= self.x1 + OVERLAP_EPSILON
            && other.y0 >= self.y0 - OVERLAP_EPSILON
            && other.y1 <= self.y1 + OVERLAP_EPSILON
    }

    pub fn corners(&self) -> [Point; 4] {
        [
            Point::new(self.x0, self.y0),
            Point::new(self.x1, self.y0),
            Point::new(self.x1, self.y1),
            Point::new(self.x0, self.y1),
        ]
    }

    pub fn transform(&self, matrix: &Matrix) -> Quad {
        Quad(self.corners().map(|corner| matrix.apply(corner)))
    }

    /// what is left of this rectangle after cutting `hole` out of it, as
    /// up to four rectangles (above, below, left and right of the hole).
    pub fn subtract(&self, hole: &Rect) -> Vec<Rect> {
        if !self.overlaps(hole) {
            return vec![*self];
        }
        let mut pieces = Vec::new();
        let middle_y0 = self.y0.max(hole.y0);
        let middle_y1 = self.y1.min(hole.y1);
        let candidates = [
            Rect::from_corners(self.x0, self.y0, self.x1, middle_y0),
            Rect::from_corners(self.x0, middle_y1, self.x1, self.y1),
            Rect::from_corners(self.x0, middle_y0, self.x1.min(hole.x0), middle_y1),
            Rect::from_corners(self.x0.max(hole.x1), middle_y0, self.x1, middle_y1),
        ];
        for piece in candidates {
            let inside_self = piece.x0 >= self.x0 && piece.x1 <= self.x1;
            if inside_self && piece.width() > OVERLAP_EPSILON && piece.height() > OVERLAP_EPSILON {
                pieces.push(piece);
            }
        }
        pieces
    }
}

/// four corners in order around the edge: the shape a rectangle becomes
/// after a matrix that may turn or slant it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad(pub [Point; 4]);

impl Quad {
    pub fn bounds(&self) -> Rect {
        Rect::around(&self.0).expect("a quad has four corners")
    }

    /// true when the quad and the rectangle share some area.
    ///
    /// uses the separating axis test: two convex shapes do not overlap
    /// exactly when some line direction keeps them apart. for a rectangle
    /// and a parallelogram the directions worth trying are the two page
    /// axes and the two edge directions of the parallelogram. a quad that
    /// collapsed to a line or a point is treated by its bounds.
    pub fn overlaps(&self, rect: &Rect) -> bool {
        let bounds = self.bounds();
        if !bounds.grow(OVERLAP_EPSILON * 2.0).overlaps(rect) && !rect.contains(&bounds) {
            return false;
        }
        if bounds.width() < OVERLAP_EPSILON || bounds.height() < OVERLAP_EPSILON {
            return thin_bounds_overlap(&bounds, rect);
        }
        let rect_corners = rect.corners();
        for axis in self.edge_normals() {
            let (quad_min, quad_max) = project(&self.0, axis);
            let (rect_min, rect_max) = project(&rect_corners, axis);
            if quad_max <= rect_min + OVERLAP_EPSILON || rect_max <= quad_min + OVERLAP_EPSILON {
                return false;
            }
        }
        // the page axes are covered by the bounds check above
        bounds.overlaps(rect)
    }

    /// true when every corner lies inside the rectangle.
    pub fn inside(&self, rect: &Rect) -> bool {
        rect.contains(&self.bounds())
    }

    fn edge_normals(&self) -> Vec<Point> {
        let mut normals = Vec::with_capacity(2);
        for (from, to) in [(0, 1), (1, 2)] {
            let dx = self.0[to].x - self.0[from].x;
            let dy = self.0[to].y - self.0[from].y;
            let length = (dx * dx + dy * dy).sqrt();
            if length > 1e-12 {
                normals.push(Point::new(-dy / length, dx / length));
            }
        }
        normals
    }
}

/// a line or a point overlaps a rectangle when it pokes inside it.
fn thin_bounds_overlap(bounds: &Rect, rect: &Rect) -> bool {
    let across_x = ranges_meet(
        (bounds.x0, bounds.x1),
        (rect.x0, rect.x1),
        bounds.width() < OVERLAP_EPSILON,
    );
    let across_y = ranges_meet(
        (bounds.y0, bounds.y1),
        (rect.y0, rect.y1),
        bounds.height() < OVERLAP_EPSILON,
    );
    across_x && across_y
}

/// along one axis: a thin range must sit inside the other, a wide one
/// must share more than an end point with it.
fn ranges_meet(range: (f64, f64), other: (f64, f64), thin: bool) -> bool {
    if thin {
        range.0 > other.0 + OVERLAP_EPSILON && range.1 < other.1 - OVERLAP_EPSILON
    } else {
        range.0 < other.1 - OVERLAP_EPSILON && other.0 < range.1 - OVERLAP_EPSILON
    }
}

fn project(points: &[Point], axis: Point) -> (f64, f64) {
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for point in points {
        let along = point.x * axis.x + point.y * axis.y;
        low = low.min(along);
        high = high.max(along);
    }
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn then_applies_the_first_matrix_first() {
        let move_right = Matrix::translate(10.0, 0.0);
        let double = Matrix::scale(2.0, 2.0);
        let point = move_right.then(&double).apply(Point::new(1.0, 1.0));
        assert!(close(point, Point::new(22.0, 2.0)));
    }

    #[test]
    fn inverse_undoes_the_matrix() {
        let matrix = Matrix::new(0.0, 2.0, -3.0, 0.0, 5.0, 7.0);
        let inverse = matrix.inverse().unwrap();
        let point = Point::new(3.0, -4.0);
        assert!(close(inverse.apply(matrix.apply(point)), point));
        assert!(Matrix::new(1.0, 2.0, 2.0, 4.0, 0.0, 0.0)
            .inverse()
            .is_none());
    }

    #[test]
    fn rectangles_touching_at_an_edge_do_not_overlap() {
        let left = Rect::from_corners(0.0, 0.0, 10.0, 10.0);
        let right = Rect::from_corners(10.0, 0.0, 20.0, 10.0);
        assert!(!left.overlaps(&right));
        assert!(left.overlaps(&Rect::from_corners(9.0, 9.0, 11.0, 11.0)));
    }

    #[test]
    fn a_turned_quad_misses_a_rectangle_its_bounds_would_hit() {
        // a diamond around (10, 10); its bounding box covers the corner at (1, 1)
        let diamond = Quad([
            Point::new(10.0, 0.0),
            Point::new(20.0, 10.0),
            Point::new(10.0, 20.0),
            Point::new(0.0, 10.0),
        ]);
        assert!(!diamond.overlaps(&Rect::from_corners(0.0, 0.0, 2.0, 2.0)));
        assert!(diamond.overlaps(&Rect::from_corners(8.0, 8.0, 12.0, 12.0)));
    }

    #[test]
    fn subtracting_a_hole_leaves_the_frame_around_it() {
        let page = Rect::from_corners(0.0, 0.0, 100.0, 100.0);
        let hole = Rect::from_corners(40.0, 40.0, 60.0, 60.0);
        let pieces = page.subtract(&hole);
        assert_eq!(pieces.len(), 4);
        let area: f64 = pieces
            .iter()
            .map(|piece| piece.width() * piece.height())
            .sum();
        assert!((area - (10000.0 - 400.0)).abs() < 1e-6);
        assert!(pieces.iter().all(|piece| !piece.overlaps(&hole)));
    }

    #[test]
    fn a_line_inside_a_rectangle_overlaps_it() {
        let line = Rect::from_corners(5.0, 5.0, 15.0, 5.0).transform(&Matrix::IDENTITY);
        assert!(line.overlaps(&Rect::from_corners(0.0, 0.0, 10.0, 10.0)));
        assert!(!line.overlaps(&Rect::from_corners(20.0, 0.0, 30.0, 10.0)));
    }
}
