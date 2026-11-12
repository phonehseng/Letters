//! word's layout grid, for side-by-side diagnostics only.
//!
//! word lays text out on a grid of 1/600 inch (0.12 pt): every glyph's pen
//! and every baseline lands on a whole grid step (recipe 6.1 to 6.5, in
//! the fidelity work). this engine draws exactly instead, which is up to
//! about 0.06 pt more precise than word's own pdf. snapping a finished
//! layout to the grid shows how much of a remaining difference is only
//! that rounding; it is an option for comparing, never the default, and
//! it does not change any line or page break.

use crate::result::LayoutResult;

/// one grid step: 1/600 inch in points
pub const WORD_GRID: f64 = 72.0 / 600.0;

fn snap(value: f64) -> f64 {
    (value / WORD_GRID).round() * WORD_GRID
}

/// moves every glyph, baseline, rule and picture of a finished layout to
/// the nearest step of word's grid, measured from the page's top-left.
pub fn snap_to_word_grid(result: &mut LayoutResult) {
    for page in &mut result.pages {
        for line in &mut page.lines {
            line.x = snap(line.x);
            line.top = snap(line.top);
            line.baseline = snap(line.baseline);
            for caret in &mut line.carets {
                *caret = snap(*caret);
            }
            for run in &mut line.runs {
                run.y = snap(run.y);
                for x in &mut run.x {
                    *x = snap(*x);
                }
            }
        }
        for rect in &mut page.rects {
            rect.x = snap(rect.x);
            rect.y = snap(rect.y);
        }
        for image in &mut page.images {
            image.x = snap(image.x);
            image.y = snap(image.y);
        }
        for frame in &mut page.frames {
            frame.x = snap(frame.x);
            frame.y = snap(frame.y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_land_on_six_hundredths_of_an_inch() {
        assert!((snap(72.05) - 72.0).abs() < 1e-9);
        assert!((snap(72.07) - 72.12).abs() < 1e-9);
        // an inch is exactly 600 steps
        assert!((snap(72.0) - 72.0).abs() < 1e-9);
    }
}
