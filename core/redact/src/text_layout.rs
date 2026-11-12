//! putting glyphs back together into text.
//!
//! a pdf stores glyphs, not words: spaces are often just gaps, and lines
//! are just glyphs placed lower down. to search a page or to show what a
//! redaction will remove, the glyphs (in the order the page draws them)
//! are joined into a string, with a space where there is a visible gap
//! and a line break where the baseline jumps.

use crate::geometry::{Point, Quad};

/// a glyph's text and where it is.
pub struct PlacedGlyph<'a> {
    pub text: &'a str,
    pub quad: &'a Quad,
}

/// the page text and, for every character, the glyph it came from (none
/// for spaces and line breaks that were added).
#[derive(Debug, Default)]
pub struct LaidOutText {
    pub text: String,
    /// glyph index of each char of `text`, by char position
    pub glyph_of_char: Vec<Option<usize>>,
    /// the line number of each glyph
    pub line_of_glyph: Vec<usize>,
}

/// a gap wider than this share of the glyph height reads as a space.
const SPACE_GAP: f64 = 0.15;
/// a baseline that moves more than this share of the height is a new line.
const LINE_JUMP: f64 = 0.5;

pub fn lay_out(glyphs: &[PlacedGlyph]) -> LaidOutText {
    let mut result = LaidOutText::default();
    let mut line = 0;
    for (index, glyph) in glyphs.iter().enumerate() {
        if index > 0 {
            match separator(&glyphs[index - 1], glyph) {
                Separator::Line => {
                    line += 1;
                    push(&mut result, "\n", None);
                }
                Separator::Space => push(&mut result, " ", None),
                Separator::None => {}
            }
        }
        result.line_of_glyph.push(line);
        push(&mut result, glyph.text, Some(index));
    }
    result
}

fn push(result: &mut LaidOutText, text: &str, glyph: Option<usize>) {
    for letter in text.chars() {
        result.text.push(letter);
        result.glyph_of_char.push(glyph);
    }
}

enum Separator {
    None,
    Space,
    Line,
}

fn separator(before: &PlacedGlyph, after: &PlacedGlyph) -> Separator {
    let [origin, end, _, top] = before.quad.0;
    let direction =
        unit(Point::new(end.x - origin.x, end.y - origin.y)).unwrap_or(Point::new(1.0, 0.0));
    let height = distance(origin, top).max(1e-3);
    let next = after.quad.0[0];
    let across = direction.x * (next.y - origin.y) - direction.y * (next.x - origin.x);
    if across.abs() > LINE_JUMP * height {
        return Separator::Line;
    }
    let gap = direction.x * (next.x - end.x) + direction.y * (next.y - end.y);
    let has_space =
        before.text.ends_with(char::is_whitespace) || after.text.starts_with(char::is_whitespace);
    if !has_space && (gap > SPACE_GAP * height || gap < -height * 2.0) {
        return Separator::Space;
    }
    Separator::None
}

fn unit(vector: Point) -> Option<Point> {
    let length = (vector.x * vector.x + vector.y * vector.y).sqrt();
    (length > 1e-9).then(|| Point::new(vector.x / length, vector.y / length))
}

fn distance(a: Point, b: Point) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    fn glyph_at(x: f64, y: f64, width: f64) -> Quad {
        Rect::from_corners(x, y - 2.0, x + width, y + 8.0)
            .transform(&crate::geometry::Matrix::IDENTITY)
    }

    #[test]
    fn gaps_become_spaces_and_lower_lines_become_breaks() {
        let quads = [
            glyph_at(0.0, 100.0, 5.0),
            glyph_at(5.0, 100.0, 5.0),
            glyph_at(14.0, 100.0, 5.0),
            glyph_at(0.0, 85.0, 5.0),
        ];
        let texts = ["a", "b", "c", "d"];
        let glyphs: Vec<PlacedGlyph> = texts
            .iter()
            .zip(&quads)
            .map(|(text, quad)| PlacedGlyph { text, quad })
            .collect();
        let laid_out = lay_out(&glyphs);
        assert_eq!(laid_out.text, "ab c\nd");
        assert_eq!(laid_out.glyph_of_char[3], Some(2));
        assert_eq!(laid_out.line_of_glyph, vec![0, 0, 0, 1]);
    }
}
