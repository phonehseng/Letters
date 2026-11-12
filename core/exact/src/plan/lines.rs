//! glyphs to lines: the unit that becomes one positioned frame (recipe 5).
//!
//! a line is a run of glyphs in content order that share a baseline and an
//! angle and move forward without a big jump. a jump wider than 1.5 em
//! (columns, table cells) starts a new line, and so does any baseline
//! change, which makes superscripts and subscripts lines of their own,
//! placed exactly.

use crate::pdf::Glyph;

pub const BASELINE_TOLERANCE_PT: f64 = 0.01;
/// a gap wider than this many ems is a new line
pub const LINE_GAP_EM: f64 = 1.5;
/// a gap this wide between words gets a space char, for search and copy
pub const VIRTUAL_SPACE_EM: f64 = 0.18;
pub const ANGLE_TOLERANCE_DEG: f64 = 0.05;

#[derive(Clone, Debug)]
pub struct Line {
    pub page: usize,
    pub glyphs: Vec<Glyph>,
    pub angle: f64,
    /// a space char goes in before these glyph indexes
    pub virtual_spaces: Vec<usize>,
}

impl Line {
    pub fn x(&self) -> f64 {
        self.glyphs[0].x
    }

    pub fn y(&self) -> f64 {
        self.glyphs[0].y
    }

    pub fn max_size(&self) -> f64 {
        self.glyphs.iter().map(|g| g.size).fold(f64::MIN, f64::max)
    }

    /// render modes 3 and 7 everywhere: the line is invisible text.
    pub fn hidden(&self) -> bool {
        self.glyphs.iter().all(Glyph::invisible)
    }

    pub fn is_rotated(&self) -> bool {
        self.angle.abs() > ANGLE_TOLERANCE_DEG
    }

    pub fn width(&self) -> f64 {
        let last = self.glyphs.last().expect("lines are never empty");
        (last.x - self.x()).hypot(last.y - self.y()) + last.advance
    }

    /// distance of a point along the baseline from the line's start.
    pub fn along(&self, x: f64, y: f64) -> f64 {
        let angle = self.angle.to_radians();
        (x - self.x()) * angle.cos() + (y - self.y()) * angle.sin()
    }
}

fn continues(line: &Line, glyph: &Glyph) -> bool {
    let last = line.glyphs.last().expect("lines are never empty");
    if (glyph.angle - line.angle).abs() > ANGLE_TOLERANCE_DEG
        || glyph.render_mode != last.render_mode
    {
        return false;
    }
    let angle = line.angle.to_radians();
    // distance along and across the baseline from the line's start
    let (dx, dy) = (glyph.x - line.x(), glyph.y - line.y());
    let across = -dx * angle.sin() + dy * angle.cos();
    if across.abs() > BASELINE_TOLERANCE_PT {
        return false;
    }
    let along_last = (last.x - line.x()) * angle.cos() + (last.y - line.y()) * angle.sin();
    let along = dx * angle.cos() + dy * angle.sin();
    let gap = along - (along_last + last.advance);
    -0.5 * last.size < gap && gap < LINE_GAP_EM * glyph.size.max(last.size)
}

/// groups glyphs (of any pages) into lines, page by page in content order.
pub fn group_lines(glyphs: &[Glyph]) -> Vec<Line> {
    let mut ordered: Vec<&Glyph> = glyphs.iter().collect();
    // sequence numbers restart on every page, so pages are kept apart first
    ordered.sort_by_key(|g| (g.page, g.sequence));
    let mut lines: Vec<Line> = Vec::new();
    for glyph in ordered {
        if let Some(line) = lines.last_mut() {
            if line.page == glyph.page && continues(line, glyph) {
                line.glyphs.push(glyph.clone());
                continue;
            }
        }
        lines.push(Line {
            page: glyph.page,
            glyphs: vec![glyph.clone()],
            angle: glyph.angle,
            virtual_spaces: Vec::new(),
        });
    }
    for line in &mut lines {
        line.virtual_spaces = virtual_spaces(line);
        drop_trailing_spaces(line);
    }
    lines.retain(|line| !line.glyphs.is_empty());
    lines
}

/// indexes where words touch nothing but a gap, so the docx text still has the space.
fn virtual_spaces(line: &Line) -> Vec<usize> {
    let mut positions = Vec::new();
    for index in 1..line.glyphs.len() {
        let (previous, current) = (&line.glyphs[index - 1], &line.glyphs[index]);
        if previous.text == " " || current.text == " " {
            continue;
        }
        let gap = (current.x - previous.x).hypot(current.y - previous.y) - previous.advance;
        if gap > VIRTUAL_SPACE_EM * current.size {
            positions.push(index);
        }
    }
    positions
}

fn drop_trailing_spaces(line: &mut Line) {
    while line.glyphs.len() > 1 && line.glyphs.last().is_some_and(|g| g.text == " ") {
        line.glyphs.pop();
    }
}

/// counts of right-to-left and complex-script glyphs, which are not shaped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptCounts {
    pub right_to_left: usize,
    pub complex_script: usize,
}

/// right-to-left and complex scripts by unicode block (hebrew, arabic,
/// syriac, thaana and nko read right to left; indic, thai, khmer and
/// myanmar need shaping).
pub fn script_counts(glyphs: &[Glyph]) -> ScriptCounts {
    let mut counts = ScriptCounts::default();
    for glyph in glyphs {
        for char in glyph.text.chars() {
            let code = char as u32;
            let right_to_left = (0x0590..=0x08FF).contains(&code)
                || (0xFB1D..=0xFDFF).contains(&code)
                || (0xFE70..=0xFEFF).contains(&code);
            let complex = (0x0590..=0x074F).contains(&code)
                || (0x0780..=0x07BF).contains(&code)
                || (0x0900..=0x0DFF).contains(&code)
                || (0x0E00..=0x0E7F).contains(&code)
                || (0x1000..=0x109F).contains(&code)
                || (0x1780..=0x17FF).contains(&code)
                || (0xFB50..=0xFDFF).contains(&code)
                || (0xFE70..=0xFEFF).contains(&code);
            counts.right_to_left += usize::from(right_to_left);
            counts.complex_script += usize::from(complex);
        }
    }
    counts
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Arc;

    use super::*;
    use crate::pdf::fonts::PdfFont;

    pub(crate) fn test_font() -> Arc<PdfFont> {
        Arc::new(PdfFont {
            key: "t 1".into(),
            base_name: "Test".into(),
            subtype: "TrueType".into(),
            code_bytes: 1,
            widths: HashMap::new(),
            default_width: 500.0,
            to_unicode: HashMap::new(),
            glyph_names: HashMap::new(),
            program: None,
            program_kind: None,
            cid_to_gid: None,
            symbolic: false,
            ascent: 750.0,
            descent: -250.0,
            serif: false,
            problems: Vec::new(),
            type3_procs: BTreeMap::new(),
            font_matrix: [0.001, 0.0, 0.0, 0.001, 0.0, 0.0],
        })
    }

    /// a 10 pt glyph with a 5 pt advance, as in the prototype's tests.
    pub(crate) fn glyph(
        text: &str,
        x: f64,
        y: f64,
        size: f64,
        sequence: usize,
        page: usize,
    ) -> Glyph {
        Glyph {
            page,
            font: test_font(),
            code: text.chars().next().map(|c| c as u32).unwrap_or(0),
            text: text.into(),
            x,
            y,
            size,
            horizontal_ratio: 1.0,
            angle: 0.0,
            advance: 5.0,
            color: [0.0; 3],
            render_mode: 0,
            sequence,
        }
    }

    #[test]
    fn same_baseline_forward_is_one_line() {
        let glyphs: Vec<Glyph> = "word"
            .chars()
            .enumerate()
            .map(|(i, c)| glyph(&c.to_string(), 72.0 + 5.0 * i as f64, 700.0, 10.0, i, 0))
            .collect();
        assert_eq!(group_lines(&glyphs).len(), 1);
    }

    #[test]
    fn column_gap_and_baseline_change_split() {
        let glyphs = vec![
            glyph("a", 72.0, 700.0, 10.0, 0, 0),
            glyph("b", 77.0, 700.0, 10.0, 1, 0),
            glyph("c", 300.0, 700.0, 10.0, 2, 0),
            glyph("2", 305.0, 703.0, 6.0, 3, 0),
        ];
        let sizes: Vec<usize> = group_lines(&glyphs)
            .iter()
            .map(|l| l.glyphs.len())
            .collect();
        assert_eq!(sizes, [2, 1, 1]);
    }

    #[test]
    fn pages_do_not_interleave() {
        let glyphs = vec![
            glyph("a", 72.0, 700.0, 10.0, 0, 0),
            glyph("b", 72.0, 700.0, 10.0, 0, 1),
            glyph("c", 77.0, 700.0, 10.0, 1, 0),
        ];
        let shape: Vec<(usize, usize)> = group_lines(&glyphs)
            .iter()
            .map(|l| (l.page, l.glyphs.len()))
            .collect();
        assert_eq!(shape, [(0, 2), (1, 1)]);
    }

    #[test]
    fn virtual_space_for_a_wide_gap() {
        // a 2.5 pt gap after a 5 pt advance
        let glyphs = vec![
            glyph("a", 72.0, 700.0, 10.0, 0, 0),
            glyph("b", 79.5, 700.0, 10.0, 1, 0),
        ];
        assert_eq!(group_lines(&glyphs)[0].virtual_spaces, [1]);
    }

    #[test]
    fn trailing_spaces_are_dropped() {
        let glyphs = vec![
            glyph("a", 72.0, 700.0, 10.0, 0, 0),
            glyph(" ", 77.0, 700.0, 10.0, 1, 0),
        ];
        assert_eq!(group_lines(&glyphs)[0].glyphs.len(), 1);
    }

    #[test]
    fn right_to_left_and_complex_scripts_are_counted() {
        let glyphs = vec![
            glyph("\u{05D0}", 72.0, 700.0, 10.0, 0, 0),
            glyph("\u{0915}", 80.0, 700.0, 10.0, 1, 0),
            glyph("a", 90.0, 700.0, 10.0, 2, 0),
        ];
        assert_eq!(
            script_counts(&glyphs),
            ScriptCounts {
                right_to_left: 1,
                complex_script: 2
            }
        );
    }
}
