//! where the outline of a glyph a subset font lacks comes from.
//!
//! a `GlyphSource` hands out outlines in the target font's own units, as
//! TrueType contours (on-curve and off-curve quadratic points). two
//! sources exist: the same font installed on this computer, and a font
//! file the app passes in (a look-alike it chose). when core/fonts offers
//! glyph completion (`letters_fonts::complete`, not there yet), it becomes
//! a third source behind this same trait.

use ttf_parser::{GlyphId, OutlineBuilder};

use crate::fonts::system;
use crate::fonts::truetype::{Embedding, FontProgram};

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

/// what the target font is, so a source can draw for it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Target {
    pub units_per_em: u16,
    /// the capital height in thousandths of the em, when the file says
    pub cap_height: Option<f32>,
}

pub(crate) trait GlyphSource {
    /// where the glyphs come from, for the person: "Arial on this computer"
    fn describe(&self) -> String;
    /// the outline of `character` for `target`, or none
    fn outline(&self, character: char, target: Target) -> Option<Outline>;
}

/// a TrueType font file used as a source of glyphs.
pub(crate) struct FontFileSource {
    data: Vec<u8>,
    index: u32,
    label: String,
    /// scale the glyphs so capitals are as tall as the target's: right for
    /// a look-alike of another design, off for the same font
    match_cap_height: bool,
}

impl FontFileSource {
    /// the same font installed on this computer, if its licence allows it
    /// to be copied into a file.
    pub fn installed(pdf_name: &str) -> Option<FontFileSource> {
        let found = system::find(pdf_name)?;
        let data = std::fs::read(&found.path).ok()?;
        let source = FontFileSource {
            data,
            index: found.index,
            label: format!("{} on this computer", found.family),
            match_cap_height: false,
        };
        source.embeddable().then_some(source)
    }

    /// a font file the app chose as a look-alike.
    pub fn look_alike(data: Vec<u8>, label: String) -> Option<FontFileSource> {
        let source = FontFileSource {
            data,
            index: 0,
            label,
            match_cap_height: true,
        };
        source.embeddable().then_some(source)
    }

    fn embeddable(&self) -> bool {
        FontProgram::parse(&self.data, self.index)
            .is_some_and(|program| program.embedding() != Embedding::Forbidden)
    }
}

impl GlyphSource for FontFileSource {
    fn describe(&self) -> String {
        self.label.clone()
    }

    fn outline(&self, character: char, target: Target) -> Option<Outline> {
        let program = FontProgram::parse(&self.data, self.index)?;
        let glyph = program.glyph_for_char(character)?;
        let mut scale =
            f32::from(target.units_per_em) / f32::from(program.face.units_per_em().max(1));
        if self.match_cap_height {
            let donor_cap = program
                .face
                .capital_height()
                .map(|height| program.scale(height));
            if let (Some(wanted), Some(donor)) =
                (target.cap_height, donor_cap.filter(|cap| *cap > 0.0))
            {
                scale *= wanted / donor;
            }
        }
        let mut builder = Quadratic {
            scale,
            contours: Vec::new(),
            current: Vec::new(),
            pen: (0.0, 0.0),
        };
        program.face.outline_glyph(GlyphId(glyph), &mut builder);
        builder.close();
        let advance =
            f32::from(program.face.glyph_hor_advance(GlyphId(glyph)).unwrap_or(0)) * scale;
        Some(Outline {
            contours: builder.contours,
            advance,
        })
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
