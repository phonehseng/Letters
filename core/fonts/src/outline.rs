//! glyph outlines as paths: laying out a line of sample text in a font
//! and collecting the outlines of its glyphs, so a preview can be drawn
//! as svg or rasterised (raster.rs) without installing the font anywhere.

use ttf_parser::{Face, GlyphId, OutlineBuilder, Tag};

/// one drawing step, in pixels from the top left, y down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// a line of text laid out at a size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaidOut {
    pub segments: Vec<Segment>,
    /// the width the text takes, in pixels
    pub width: f32,
    /// the height of a line (ascender to descender), in pixels
    pub height: f32,
    /// where the baseline sits, in pixels from the top
    pub baseline: f32,
    /// characters the font has no glyph for (drawn as its empty box)
    pub missing: Vec<char>,
}

/// collects the outline of one glyph, scaled and placed.
struct Collector<'a> {
    segments: &'a mut Vec<Segment>,
    scale: f32,
    x: f32,
    baseline: f32,
}

impl Collector<'_> {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (self.x + x * self.scale, self.baseline - y * self.scale)
    }
}

impl OutlineBuilder for Collector<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.segments.push(Segment::Move(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.segments.push(Segment::Line(x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x, y) = self.point(x, y);
        self.segments.push(Segment::Quad(x1, y1, x, y));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x2, y2) = self.point(x2, y2);
        let (x, y) = self.point(x, y);
        self.segments.push(Segment::Cubic(x1, y1, x2, y2, x, y));
    }
    fn close(&mut self) {
        self.segments.push(Segment::Close);
    }
}

/// previews are one line; very long samples are cut here so a pasted
/// essay can't make a huge picture.
pub const MAX_SAMPLE_CHARS: usize = 200;

/// sets a variable font to the given axis values ("wght" 700...).
pub fn apply_variations(face: &mut Face, variations: &[(String, f32)]) {
    for (tag, value) in variations {
        if let Ok(bytes) = <[u8; 4]>::try_from(tag.as_bytes()) {
            face.set_variation(Tag::from_bytes(&bytes), *value);
        }
    }
}

/// lays out `text` in one line at `size` pixels per em: the font's own
/// advances and its old-style kerning, no shaping. that is enough for a
/// preview of the alphabet or a name; complex scripts look rougher.
pub fn lay_out(face: &Face, text: &str, size: f32) -> LaidOut {
    let units = f32::from(face.units_per_em().max(16));
    let scale = size / units;
    let ascender = f32::from(face.ascender()) * scale;
    let descender = f32::from(face.descender()) * scale;
    let mut laid = LaidOut {
        height: (ascender - descender).max(size * 0.5),
        baseline: ascender.max(0.0),
        ..LaidOut::default()
    };
    let mut x = 0.0f32;
    let mut previous: Option<GlyphId> = None;
    for character in text
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_SAMPLE_CHARS)
    {
        let glyph = match face.glyph_index(character) {
            Some(glyph) => glyph,
            None => {
                if !character.is_whitespace() && !laid.missing.contains(&character) {
                    laid.missing.push(character);
                }
                GlyphId(0)
            }
        };
        if let Some(before) = previous {
            x += kerning(face, before, glyph) * scale;
        }
        let mut collector = Collector {
            segments: &mut laid.segments,
            scale,
            x,
            baseline: laid.baseline,
        };
        face.outline_glyph(glyph, &mut collector);
        x += f32::from(face.glyph_hor_advance(glyph).unwrap_or(0)) * scale;
        previous = Some(glyph);
    }
    laid.width = x.max(0.0);
    laid
}

/// one glyph's outline at `scale` pixels per font unit, its origin at
/// (`x`, `baseline`).
pub fn glyph_segments(
    face: &Face,
    glyph: GlyphId,
    scale: f32,
    x: f32,
    baseline: f32,
) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut collector = Collector {
        segments: &mut segments,
        scale,
        x,
        baseline,
    };
    face.outline_glyph(glyph, &mut collector);
    segments
}

fn kerning(face: &Face, left: GlyphId, right: GlyphId) -> f32 {
    let Some(kern) = face.tables().kern else {
        return 0.0;
    };
    kern.subtables
        .into_iter()
        .filter(|table| table.horizontal && !table.variable)
        .find_map(|table| table.glyphs_kerning(left, right))
        .map(f32::from)
        .unwrap_or(0.0)
}

/// the segments as an svg path's "d" attribute, rounded to a tenth of a
/// pixel to keep it short.
pub fn svg_path(segments: &[Segment]) -> String {
    use std::fmt::Write;
    let round = |value: f32| (value * 10.0).round() / 10.0;
    let mut path = String::new();
    for segment in segments {
        let _ = match *segment {
            Segment::Move(x, y) => write!(path, "M{} {}", round(x), round(y)),
            Segment::Line(x, y) => write!(path, "L{} {}", round(x), round(y)),
            Segment::Quad(x1, y1, x, y) => {
                write!(
                    path,
                    "Q{} {} {} {}",
                    round(x1),
                    round(y1),
                    round(x),
                    round(y)
                )
            }
            Segment::Cubic(x1, y1, x2, y2, x, y) => write!(
                path,
                "C{} {} {} {} {} {}",
                round(x1),
                round(y1),
                round(x2),
                round(y2),
                round(x),
                round(y)
            ),
            Segment::Close => write!(path, "Z"),
        };
    }
    path
}

/// the segments turned into straight lines, curves cut into short
/// pieces. each contour is closed, as filling needs.
pub fn flatten(segments: &[Segment]) -> Vec<((f32, f32), (f32, f32))> {
    let mut lines = Vec::new();
    let mut start = (0.0, 0.0);
    let mut at = (0.0, 0.0);
    let push = |lines: &mut Vec<_>, from: (f32, f32), to: (f32, f32)| {
        if from != to {
            lines.push((from, to));
        }
    };
    for segment in segments {
        match *segment {
            Segment::Move(x, y) => {
                push(&mut lines, at, start);
                start = (x, y);
                at = start;
            }
            Segment::Line(x, y) => {
                push(&mut lines, at, (x, y));
                at = (x, y);
            }
            Segment::Quad(x1, y1, x, y) => {
                let steps = curve_steps(at, (x, y), (x1, y1), (x1, y1));
                let from = at;
                for step in 1..=steps {
                    let t = step as f32 / steps as f32;
                    let u = 1.0 - t;
                    let point = (
                        u * u * from.0 + 2.0 * u * t * x1 + t * t * x,
                        u * u * from.1 + 2.0 * u * t * y1 + t * t * y,
                    );
                    push(&mut lines, at, point);
                    at = point;
                }
            }
            Segment::Cubic(x1, y1, x2, y2, x, y) => {
                let steps = curve_steps(at, (x, y), (x1, y1), (x2, y2));
                let from = at;
                for step in 1..=steps {
                    let t = step as f32 / steps as f32;
                    let u = 1.0 - t;
                    let point = (
                        u * u * u * from.0
                            + 3.0 * u * u * t * x1
                            + 3.0 * u * t * t * x2
                            + t * t * t * x,
                        u * u * u * from.1
                            + 3.0 * u * u * t * y1
                            + 3.0 * u * t * t * y2
                            + t * t * t * y,
                    );
                    push(&mut lines, at, point);
                    at = point;
                }
            }
            Segment::Close => {
                push(&mut lines, at, start);
                at = start;
            }
        }
    }
    push(&mut lines, at, start);
    lines
}

/// enough pieces that each is about two pixels long, within limits.
fn curve_steps(from: (f32, f32), to: (f32, f32), c1: (f32, f32), c2: (f32, f32)) -> usize {
    let length = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let rough = length(from, c1) + length(c1, c2) + length(c2, to);
    if !rough.is_finite() {
        return 1;
    }
    ((rough / 2.0).ceil() as usize).clamp(1, 64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_compact_svg() {
        let path = svg_path(&[
            Segment::Move(0.0, 0.0),
            Segment::Line(10.04, 0.0),
            Segment::Quad(10.0, 5.0, 5.0, 5.0),
            Segment::Close,
        ]);
        assert_eq!(path, "M0 0L10 0Q10 5 5 5Z");
    }

    #[test]
    fn flattening_closes_contours() {
        let lines = flatten(&[
            Segment::Move(0.0, 0.0),
            Segment::Line(4.0, 0.0),
            Segment::Line(4.0, 4.0),
        ]);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines.last().unwrap().1, (0.0, 0.0));
    }
}
