//! rotated lines: drawingml text boxes (recipe 8).
//!
//! frames cannot rotate, so a rotated line goes into an anchored text box,
//! turned about its centre so its first pen origin lands on the source
//! origin. word turns shapes clockwise and pdf angles run counter-clockwise,
//! so the box's rotation is `-angle`. geometry alone missed by 19.8 pt in
//! round 3, so the converter then moves each box by what measuring exports
//! show (see `convert`).

use super::glyphs::CharRun;
use crate::docx::xml::{rotated_text_drawing, Anchor};
use crate::units::TWIPS_PER_PT;

/// the box is the line plus this many points wide, so square wrapping never breaks it.
pub const EXTRA_WIDTH_PT: f64 = 20.0;
/// measuring exports: the first move plus up to three refinements.
pub const MEASURING_PASSES: usize = 4;
/// half a pixel: below that, planning against the start is enough.
pub const START_TOLERANCE_PT: f64 = 0.06;

/// one rotated line and where its box stands.
#[derive(Clone, Debug)]
pub struct RotatedLine {
    pub page: usize,
    pub runs: Vec<CharRun>,
    /// the source's first pen origin, in points from the page's top left
    pub origin_top_down: (f64, f64),
    /// counter-clockwise degrees, as in the pdf
    pub angle: f64,
    pub width_pt: f64,
    pub line_twips: i64,
    /// the baseline below the box top: 0.804 of the line height
    pub baseline_pt: f64,
    /// the line's text, for finding it again in word's pdf
    pub text: String,
    /// the moves measured so far
    pub correction: (f64, f64),
    /// the index of its plan in the conversion's line plans
    pub plan_index: Option<usize>,
}

impl RotatedLine {
    /// the box, anchored so the first pen (with the measured correction) is on the origin.
    pub fn drawing(&self, shape_id: u32) -> String {
        let height = self.line_twips as f64 / TWIPS_PER_PT;
        let clockwise = -self.angle;
        let radians = clockwise.to_radians();
        // the start point relative to the box centre, before rotation
        let (sx, sy) = (-self.width_pt / 2.0, self.baseline_pt - height / 2.0);
        let rx = sx * radians.cos() - sy * radians.sin();
        let ry = sx * radians.sin() + sy * radians.cos();
        let origin = (
            self.origin_top_down.0 + self.correction.0,
            self.origin_top_down.1 + self.correction.1,
        );
        let (centre_x, centre_y) = (origin.0 - rx, origin.1 - ry);
        let anchor = Anchor {
            id: shape_id,
            left_pt: centre_x - self.width_pt / 2.0,
            top_pt: centre_y - height / 2.0,
            width_pt: self.width_pt,
            height_pt: height,
            behind: false,
        };
        rotated_text_drawing(&anchor, clockwise, &self.runs, self.line_twips)
    }
}

/// the offset along the baseline between where a line should start and
/// where word started it, from a measured start (both top-down points).
pub fn along_offset(angle_deg: f64, target: (f64, f64), produced: (f64, f64)) -> f64 {
    let (dx, dy) = (target.0 - produced.0, target.1 - produced.1);
    // page y grows upwards and these are top-down, so the pdf-space offset is (-dx, +dy)
    let angle = angle_deg.to_radians();
    -dx * angle.cos() + dy * angle.sin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_turned_a_quarter_puts_its_start_on_the_origin() {
        let line = RotatedLine {
            page: 0,
            runs: Vec::new(),
            origin_top_down: (100.0, 300.0),
            angle: 90.0,
            width_pt: 120.0,
            line_twips: 300,
            baseline_pt: 0.804 * 15.0,
            text: String::new(),
            correction: (0.0, 0.0),
            plan_index: None,
        };
        let xml = line.drawing(5);
        assert!(xml.contains(r#"rot="16200000""#));
        assert!(xml.contains(r#"wrap="square""#));
    }

    #[test]
    fn offsets_along_a_vertical_line() {
        // a line running up the page: word started it 1 pt too low (top-down y larger)
        let offset = along_offset(90.0, (100.0, 300.0), (100.0, 301.0));
        assert!((offset + 1.0).abs() < 1e-9);
    }
}
