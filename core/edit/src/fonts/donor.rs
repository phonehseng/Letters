//! the outline of a glyph a subset font lacks, ready to be appended to it.
//!
//! the outlines come from letters-fonts' completion (see completion.rs);
//! here they are turned into TrueType contours (on-curve and off-curve
//! quadratic points) in the target font's own units.

use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// one point of a TrueType contour, in font units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutlinePoint {
    pub x: f32,
    pub y: f32,
    pub on_curve: bool,
}

/// a glyph ready to be added to a font.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Outline {
    pub contours: Vec<Vec<OutlinePoint>>,
    /// in the target font's units
    pub advance: f32,
}

/// what the target font is, so an outline can be drawn for it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Target {
    pub units_per_em: u16,
}

/// the outline of `glyph` in `face`, scaled by `scale` into the target's units.
pub(crate) fn outline_of(face: &Face, glyph: u16, scale: f32) -> Outline {
    let mut builder = Quadratic {
        scale,
        contours: Vec::new(),
        current: Vec::new(),
        pen: (0.0, 0.0),
    };
    face.outline_glyph(GlyphId(glyph), &mut builder);
    builder.close();
    let advance = f32::from(face.glyph_hor_advance(GlyphId(glyph)).unwrap_or(0)) * scale;
    Outline {
        contours: builder.contours,
        advance,
    }
}

/// turns any outline into TrueType contours: lines and quadratic curves
/// as they are, cubic curves (from CFF fonts) split into two quadratics.
struct Quadratic {
    scale: f32,
    contours: Vec<Vec<OutlinePoint>>,
    current: Vec<OutlinePoint>,
    pen: (f32, f32),
}

impl Quadratic {
    fn push(&mut self, x: f32, y: f32, on_curve: bool) {
        self.current.push(OutlinePoint {
            x: x * self.scale,
            y: y * self.scale,
            on_curve,
        });
    }
}

impl OutlineBuilder for Quadratic {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.push(x, y, true);
        self.pen = (x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push(x, y, true);
        self.pen = (x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.push(x1, y1, false);
        self.push(x, y, true);
        self.pen = (x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        // split the cubic at its middle; each half is close to the
        // quadratic whose control point is (3(p1 + p2) - (p0 + p3)) / 4
        let (x0, y0) = self.pen;
        let mid = |a: f32, b: f32| (a + b) / 2.0;
        let (ax, ay) = (mid(x0, x1), mid(y0, y1));
        let (bx, by) = (mid(x1, x2), mid(y1, y2));
        let (cx, cy) = (mid(x2, x), mid(y2, y));
        let (dx, dy) = (mid(ax, bx), mid(ay, by));
        let (ex, ey) = (mid(bx, cx), mid(by, cy));
        let (mx, my) = (mid(dx, ex), mid(dy, ey));
        let control = |p0: f32, p1: f32, p2: f32, p3: f32| (3.0 * (p1 + p2) - (p0 + p3)) / 4.0;
        self.push(control(x0, ax, dx, mx), control(y0, ay, dy, my), false);
        self.push(mx, my, true);
        self.push(control(mx, ex, cx, x), control(my, ey, cy, y), false);
        self.push(x, y, true);
        self.pen = (x, y);
    }

    fn close(&mut self) {
        // truetype contours close themselves; a repeated start is dropped
        if self.current.len() > 1 && self.current.first() == self.current.last() {
            self.current.pop();
        }
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
    }
}
