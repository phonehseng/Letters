//! where a line's frame goes: the twips whose printed line start and
//! baseline are nearest the source's (recipe 7.2 and 7.3).

use crate::calibration::model::{is_landscape, Axis, WordModel};
use crate::units::{round_half_even, TWIPS_PER_PT};
use crate::word::writer_model::{
    exported_x, exported_y, nearest_exported_pixel, page_pixels, twips_for_pixel, twips_pixel,
};

/// the baseline's first guess: 0.804 of an exact line below its top (e2, e8).
pub const BASELINE_SHARE: f64 = 0.804;
/// how far around the guess frame y is searched, in twips.
const Y_SEARCH_TWIPS: i64 = 12;

/// one line's frame position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x_twips: i64,
    pub y_twips: i64,
    /// layout pixel of the line start, from the left page edge
    pub x_pixel: i64,
    /// layout pixel of the baseline, from the top page edge
    pub baseline_pixel: i64,
    /// where word will print the line start
    pub x_pt: f64,
    /// where word will print the baseline, from the top edge
    pub baseline_from_top_pt: f64,
}

/// the baseline pixel of a frame at `y_twips` (recipe 6.6).
pub fn baseline_pixel(model: &WordModel, line_twips: i64, y_twips: i64) -> i64 {
    twips_pixel(y_twips) + model.baseline_offset(line_twips, y_twips)
}

/// frame twips whose printed line start and baseline are nearest the wanted ones.
///
/// x: the pixel whose printed x is nearest the source x, then the smallest
/// twip value on that pixel. y: over the 25 twip values around the guess,
/// the one whose printed baseline is nearest; ties go to the twip value
/// nearest the guess, then the lower one.
pub fn place(
    model: &mut WordModel,
    x_pt: f64,
    baseline_from_top_pt: f64,
    line_twips: i64,
    page_width_pt: f64,
    page_height_pt: f64,
) -> Placement {
    let (width_px, height_px) = (page_pixels(page_width_pt), page_pixels(page_height_pt));
    let landscape = is_landscape(page_width_pt, page_height_pt);
    let x_table = model.table(Axis::X, width_px, landscape);
    let y_table = model.table(Axis::Y, height_px, landscape);
    let x_pixel = nearest_exported_pixel(x_pt, &x_table);
    let x_twips = twips_for_pixel(x_pixel);
    let pdf_y = page_height_pt - baseline_from_top_pt;
    let line_pt = line_twips as f64 / TWIPS_PER_PT;
    let guess =
        round_half_even((baseline_from_top_pt - BASELINE_SHARE * line_pt) * TWIPS_PER_PT) as i64;
    let printed_error = |y_twips: i64| {
        (exported_y(
            baseline_pixel(model, line_twips, y_twips),
            &y_table,
            height_px,
        ) - pdf_y)
            .abs()
    };
    let mut y_twips = guess - Y_SEARCH_TWIPS;
    let mut best = (printed_error(y_twips), 0i64);
    best.1 = (y_twips - guess).abs();
    for candidate in guess - Y_SEARCH_TWIPS + 1..=guess + Y_SEARCH_TWIPS {
        let key = (printed_error(candidate), (candidate - guess).abs());
        if key.0 < best.0 || (key.0 == best.0 && key.1 < best.1) {
            best = key;
            y_twips = candidate;
        }
    }
    let pixel = baseline_pixel(model, line_twips, y_twips);
    Placement {
        x_twips,
        y_twips,
        x_pixel,
        baseline_pixel: pixel,
        x_pt: exported_x(x_pixel, &x_table),
        baseline_from_top_pt: page_height_pt - exported_y(pixel, &y_table, height_px),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_prints_the_nearest_lattice_point() {
        // the prototype's case (test_prototype.py)
        let mut model = WordModel::built_in();
        let placement = place(&mut model, 72.03, 100.0, 240, 612.0, 792.0);
        // 72.024 is the exported x of pixel 600; nothing on the lattice is closer to 72.03
        assert!((placement.x_pt - 72.024).abs() < 1e-9);
        assert_eq!(placement.x_pixel, 600);
        assert_eq!(twips_pixel(placement.x_twips), 600);
        assert!((placement.baseline_from_top_pt - 100.0).abs() <= 0.072);
    }

    #[test]
    fn baseline_pixel_follows_the_offset_table() {
        let model = WordModel::built_in();
        assert_eq!(baseline_pixel(&model, 240, 1440), 600 + 80);
    }
}
