//! font sizes as word stores, lays out and draws them (recipe 4 and 6.7).
//!
//! a docx stores half points (`w:sz`). word lays out at that stored size,
//! but draws glyphs at a whole number of 1/600 inch pixels per em: 10.5 pt
//! is drawn at 10.56, 11 at 11.04, 14 at 14.04 (measured, 16.0.20527).

use crate::units::{round_half_even, PT_PER_PIXEL};

/// font units per layout pixel in a grid font: at `h` half points an em is
/// `25h/6` pixels, so 50 units per half point make one pixel 12 units.
pub const GRID_UNITS_PER_PIXEL: i64 = 12;

/// the closest size a docx can store.
pub fn nearest_half_point(size_pt: f64) -> f64 {
    round_half_even(size_pt * 2.0) / 2.0
}

/// the size word draws glyphs at for a stored size.
pub fn drawn_size(stored_pt: f64) -> f64 {
    round_half_even(stored_pt / PT_PER_PIXEL) * PT_PER_PIXEL
}

/// units per em of a grid font for a stored size: `50 * 2H`, so a pixel is
/// exactly 12 units. truetype allows 16..=16384, so sizes up to 163.5 pt.
pub fn grid_units_per_em(stored_pt: f64) -> Option<i64> {
    let half_points = (stored_pt * 2.0).round() as i64;
    let units = 50 * half_points;
    (16..=16384).contains(&units).then_some(units)
}

/// the em the outlines are scaled to before it is relabelled as the grid
/// em, so glyphs drawn at the drawn size look like the pdf size:
/// `round(U * S / D)`.
pub fn outline_units_per_em(grid_units_per_em: i64, pdf_size_pt: f64, stored_pt: f64) -> i64 {
    round_half_even(grid_units_per_em as f64 * pdf_size_pt / drawn_size(stored_pt)) as i64
}

/// an advance snapped to a whole pixel, in grid font units.
///
/// the prototype rounds with python's `round`, half to even; an advance of
/// exactly half a pixel (6 units) does occur, so that tie rule is kept.
pub fn snap_advance(advance_units: i64) -> i64 {
    round_half_even(advance_units as f64 / GRID_UNITS_PER_PIXEL as f64) as i64
        * GRID_UNITS_PER_PIXEL
}

/// the exact line height for a line whose largest glyph is `max_size_pt`,
/// in twips: whole points, at least 1.25 of the size (recipe 5).
pub fn line_twips_for(max_size_pt: f64) -> i64 {
    20 * ((max_size_pt * 1.25).ceil() as i64).max(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_storable_size_up_to_163_pt_has_whole_unit_pixels() {
        for half_points in 2..328 {
            assert_eq!(
                grid_units_per_em(half_points as f64 / 2.0),
                Some(50 * half_points)
            );
        }
        assert_eq!(grid_units_per_em(164.0), None);
    }

    #[test]
    fn drawn_size_is_whole_pixels_per_em() {
        assert!((drawn_size(10.5) - 10.56).abs() < 1e-9);
        assert!((drawn_size(14.0) - 14.04).abs() < 1e-9);
        assert!((drawn_size(11.0) - 11.04).abs() < 1e-9);
        assert!((drawn_size(21.5) - 21.48).abs() < 1e-9);
        assert!((drawn_size(12.0) - 12.0).abs() < 1e-9);
        assert!((drawn_size(9.0) - 9.0).abs() < 1e-9);
    }

    #[test]
    fn nearest_half_point() {
        assert_eq!(super::nearest_half_point(10.3), 10.5);
        assert_eq!(super::nearest_half_point(11.27), 11.5);
        assert_eq!(super::nearest_half_point(7.9), 8.0);
    }

    #[test]
    fn outlines_are_scaled_against_the_drawn_size() {
        // 10.3 pt stored at 10.5, drawn at 10.56: U = 1050, round(1050 * 10.3 / 10.56) = 1024
        assert_eq!(outline_units_per_em(1050, 10.3, 10.5), 1024);
        assert_eq!(outline_units_per_em(1200, 12.0, 12.0), 1200);
    }

    #[test]
    fn advances_snap_to_whole_pixels_half_to_even() {
        assert_eq!(snap_advance(0), 0);
        assert_eq!(snap_advance(17), 12);
        assert_eq!(snap_advance(19), 24);
        assert_eq!(snap_advance(6), 0); // 0.5 px goes to the even pixel
        assert_eq!(snap_advance(18), 24); // 1.5 px goes to 2
    }

    #[test]
    fn exact_line_heights() {
        assert_eq!(line_twips_for(12.0), 300);
        assert_eq!(line_twips_for(9.0), 240);
        assert_eq!(line_twips_for(1.0), 40);
        assert_eq!(line_twips_for(10.5), 280);
    }
}
