//! how word turns frame twips into layout pixels, and layout pixels into the
//! numbers its pdf writer prints (recipe section 6, all measured on word
//! 16.0.20527).
//!
//! - layout positions are whole 1/600 inch pixels ("px", 0.12 pt)
//! - a text object's `Tm` is printed in 1/3000 inch units ("u", 0.024 pt):
//!   `u = 5 * p + k(p)`, where `k(p)` counts the writer's step pixels at or
//!   below `p`. the steps are measured per page size and axis (`StepTable`)
//! - the value is printed with 5 significant digits and at most 3 decimals
//! - the media box is the page size rounded to whole pixels
//!
//! printed values are kept as whole thousandths of a point (`i64`), which
//! makes the printing rule exact integer arithmetic: `u * 0.024 pt` is
//! `24 * u` thousandths.

use serde::{Deserialize, Serialize};

use crate::units::{round_half_even, PT_PER_PIXEL};

/// writer units per layout pixel (1/3000 in against 1/600 in).
pub const UNITS_PER_PIXEL: i64 = 5;
/// thousandths of a point per writer unit (0.024 pt).
pub const MILLI_PER_UNIT: i64 = 24;
const SIGNIFICANT_DIGITS: i64 = 5;
const MAX_DECIMALS: i64 = 3;
/// 60 twips are exactly 25 pixels, so every twip rule repeats every 60 twips.
pub const TWIPS_PERIOD: i64 = 60;
/// twips mod 60 that sit on a pixel edge and go to the lower pixel (recipe 6.1).
const EDGE_DOWN_RESIDUES: [i64; 2] = [19, 55];

/// decimals word prints for a value of this many thousandths of a point.
fn decimals_for_milli(milli: i64) -> i64 {
    let whole = milli.abs() / 1000;
    if whole == 0 {
        return MAX_DECIMALS;
    }
    let digits_before_point = whole.to_string().len() as i64;
    (SIGNIFICANT_DIGITS - digits_before_point).clamp(0, MAX_DECIMALS)
}

/// the printed value of an exact value, both in thousandths of a point.
///
/// rounding is half away from zero, but no ties can occur: `24 * u` is even,
/// so it never ends in 5 at the second decimal, and at the first decimal or
/// the whole point `24 u ≡ 50 (mod 100)` and `24 u ≡ 500 (mod 1000)` have no
/// solution (recipe 6.3).
pub fn printed_milli(exact_milli: i64) -> i64 {
    let step = 10_i64.pow((MAX_DECIMALS - decimals_for_milli(exact_milli)) as u32);
    let magnitude = (exact_milli.abs() + step / 2) / step * step;
    magnitude * exact_milli.signum()
}

/// a printed value as text, the way word writes it: 72.024, 108.86, 752.4.
pub fn printed_text(milli: i64) -> String {
    let decimals = decimals_for_milli(milli) as usize;
    let text = format!("{:.*}", decimals, milli as f64 / 1000.0);
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

/// a number read from a pdf as whole thousandths of a point.
pub fn to_milli(points: f64) -> i64 {
    (points * 1000.0).round() as i64
}

/// every whole writer unit that word could have printed as this value.
pub fn units_candidates(printed_points: f64) -> Vec<i64> {
    let printed = to_milli(printed_points);
    let half = 10_i64.pow((MAX_DECIMALS - decimals_for_milli(printed)) as u32) / 2;
    let low = (printed - half).div_euclid(MILLI_PER_UNIT)
        + i64::from((printed - half).rem_euclid(MILLI_PER_UNIT) != 0);
    let high = (printed + half).div_euclid(MILLI_PER_UNIT);
    (low..=high).collect()
}

/// the layout pixels at which the writer adds one more unit (k steps up).
/// `steps[j]` is the first pixel with `k = j + 1`; measured per page size
/// and axis (recipe 6.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepTable {
    pub steps: Vec<i64>,
}

impl StepTable {
    pub fn new(steps: Vec<i64>) -> Self {
        Self { steps }
    }

    /// how many step pixels lie at or below `pixel`.
    pub fn k(&self, pixel: i64) -> i64 {
        self.steps.partition_point(|&step| step <= pixel) as i64
    }

    /// the writer units word prints for a layout pixel.
    pub fn units(&self, pixel: i64) -> i64 {
        UNITS_PER_PIXEL * pixel + self.k(pixel)
    }

    /// the layout pixel whose printed unit value is `units`, if any.
    ///
    /// the fifth step wraps: `5p + 5 = 5(p + 1)`, so the pixel can be up to
    /// two below `units / 5` (recipe 6.3).
    pub fn pixel_for_units(&self, units: i64) -> Option<i64> {
        let pixel = units.div_euclid(UNITS_PER_PIXEL);
        [pixel, pixel - 1, pixel - 2]
            .into_iter()
            .find(|&candidate| candidate >= 0 && self.units(candidate) == units)
    }
}

/// the layout pixel word gives a frame edge at this many twips from the
/// page edge (recipe 6.1, 85,571 frames, no exception).
///
/// `floor((5t + 1) / 12)`; on a pixel edge (`5t + 1` a multiple of 12)
/// word takes the lower pixel for t mod 60 in {19, 55}.
pub fn twips_pixel(twips: i64) -> i64 {
    let numerator = 5 * twips + 1;
    let mut pixel = numerator.div_euclid(12);
    if numerator.rem_euclid(12) == 0 && EDGE_DOWN_RESIDUES.contains(&twips.rem_euclid(TWIPS_PERIOD))
    {
        pixel -= 1;
    }
    pixel
}

/// the smallest twip value whose frame edge lands on `pixel`.
pub fn twips_for_pixel(pixel: i64) -> i64 {
    let mut twips = ((12 * pixel - 1).div_euclid(5) - 1).max(0);
    while twips_pixel(twips) < pixel {
        twips += 1;
    }
    // every pixel has a twip value (the rule never skips one)
    debug_assert_eq!(twips_pixel(twips), pixel);
    twips
}

/// pixels from a frame's top pixel to its baseline in an exact-height line
/// (recipe 6.6, 960 of 960 frames).
///
/// the baseline sits 0.8 of the line below the top. for line twips mod 60
/// of 40, 0.8 of a line is a third of a pixel short of whole and word
/// anchors the baseline to the bottom edge instead. 0.8 of `L * 5/12`
/// pixels is `L / 3`, and 0.2 of it is `L / 12`, so the rounding is done
/// in whole numbers.
pub fn baseline_offset_pixels(line_twips: i64, y_twips: i64) -> i64 {
    if line_twips.rem_euclid(TWIPS_PERIOD) == 40 {
        let from_bottom = (line_twips + 6).div_euclid(12); // floor(L/12 + 1/2)
        let bottom = twips_pixel(y_twips + line_twips) - from_bottom;
        return bottom - twips_pixel(y_twips);
    }
    (2 * line_twips + 3).div_euclid(6) // floor(L/3 + 1/2)
}

/// the page size word writes into the media box, in whole pixels (recipe 6.2).
pub fn page_pixels(size_pt: f64) -> i64 {
    round_half_even(size_pt / PT_PER_PIXEL) as i64
}

/// a linear stand-in for page sizes nobody measured: `k = floor(p * s + 17/32)`.
///
/// on letter and a4 portrait it fits within one pixel per step; the wide
/// page did not fit it. it is only used when word isn't there to measure,
/// and the report then says positions on that page size are estimated.
pub fn estimated_step_table(page_pixels_along: i64) -> StepTable {
    let slope = 0.9458 * UNITS_PER_PIXEL as f64 / page_pixels_along as f64;
    let mut steps = Vec::new();
    for j in 1.. {
        let pixel = ((j as f64 - 17.0 / 32.0) / slope).ceil() as i64;
        if pixel > page_pixels_along {
            break;
        }
        steps.push(pixel);
    }
    StepTable::new(steps)
}

/// the x word prints for a text object at layout pixel `pixel`, in points.
pub fn exported_x(pixel: i64, table: &StepTable) -> f64 {
    exported_x_milli(pixel, table) as f64 / 1000.0
}

pub fn exported_x_milli(pixel: i64, table: &StepTable) -> i64 {
    printed_milli(table.units(pixel) * MILLI_PER_UNIT)
}

/// the pdf y word prints for a baseline `pixel_from_top` pixels below the
/// page top, in points.
pub fn exported_y(pixel_from_top: i64, table: &StepTable, page_height_pixels: i64) -> f64 {
    let units = UNITS_PER_PIXEL * page_height_pixels - table.units(pixel_from_top);
    printed_milli(units * MILLI_PER_UNIT) as f64 / 1000.0
}

/// the layout pixel behind a printed `Tm` x, if one fits.
pub fn decode_x(printed_points: f64, table: &StepTable) -> Option<i64> {
    units_candidates(printed_points)
        .into_iter()
        .find_map(|units| table.pixel_for_units(units))
}

/// the layout pixel (from the top) behind a printed `Tm` y, if one fits.
pub fn decode_y(printed_points: f64, table: &StepTable, page_height_pixels: i64) -> Option<i64> {
    units_candidates(printed_points)
        .into_iter()
        .find_map(|units| table.pixel_for_units(UNITS_PER_PIXEL * page_height_pixels - units))
}

/// the layout pixel whose exported x is closest to `target_pt` (recipe 7.2).
///
/// the exported positions are 0.12 pt apart with a 0.144 pt gap at each
/// step, so a plain `round(x / 0.12)` can miss by a pixel near a step.
/// ties go to the lower pixel.
pub fn nearest_exported_pixel(target_pt: f64, table: &StepTable) -> i64 {
    let guess = round_half_even(target_pt / PT_PER_PIXEL) as i64;
    let mut best = (guess - 2).max(0);
    let mut best_error = f64::INFINITY;
    for pixel in (guess - 2).max(0)..=guess + 2 {
        let error = (exported_x(pixel, table) - target_pt).abs();
        if error < best_error {
            best = pixel;
            best_error = error;
        }
    }
    best
}

/// distance from `target_pt` to the nearest exported x: the floor of any
/// plan on this lattice.
pub fn lattice_error(target_pt: f64, table: &StepTable) -> f64 {
    (exported_x(nearest_exported_pixel(target_pt, table), table) - target_pt).abs()
}

/// the exported y nearest a pdf y (for the lattice floor of a source).
pub fn nearest_exported_y(pdf_y: f64, table: &StepTable, page_height_pixels: i64) -> f64 {
    let from_top = page_height_pixels as f64 * PT_PER_PIXEL - pdf_y;
    let guess = round_half_even(from_top / PT_PER_PIXEL) as i64;
    let mut best = f64::NAN;
    let mut best_error = f64::INFINITY;
    for pixel in guess - 2..=guess + 2 {
        let y = exported_y(pixel, table, page_height_pixels);
        let error = (y - pdf_y).abs();
        if error < best_error {
            best = y;
            best_error = error;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    // the measured letter tables (recipe 6.3)
    fn letter_x() -> StepTable {
        StepTable::new(vec![506, 1586, 2665, 3743, 4823])
    }
    fn letter_y() -> StepTable {
        StepTable::new(vec![654, 2049, 3444, 4839, 6234])
    }

    #[test]
    fn five_significant_digits_at_most_three_decimals() {
        assert_eq!(printed_text(72_024), "72.024");
        assert_eq!(printed_text(printed_milli(108_864)), "108.86");
        assert_eq!(printed_text(752_400), "752.4");
        assert_eq!(printed_text(6_360), "6.36");
        assert_eq!(printed_text(0), "0");
        assert_eq!(printed_text(1_008_000), "1008");
    }

    #[test]
    fn units_candidates_cover_print_rounding() {
        assert_eq!(units_candidates(72.024), vec![3001]);
        assert_eq!(units_candidates(108.86), vec![4536]);
    }

    #[test]
    fn floor_rule_for_small_twips() {
        // measured: 0,1,2 -> 0; 3,4 -> 1; 5,6 -> 2; 7,8,9 -> 3; 10,11 -> 4; 12 -> 5
        let pixels: Vec<i64> = (0..13).map(twips_pixel).collect();
        assert_eq!(pixels, [0, 0, 0, 1, 1, 2, 2, 3, 3, 3, 4, 4, 5]);
    }

    #[test]
    fn pixel_edges_follow_the_60_twip_pattern() {
        // 5t + 1 divisible by 12: up for t mod 60 in (7, 31, 43), down for (19, 55)
        assert_eq!(twips_pixel(19), 7);
        assert_eq!(twips_pixel(31), 13);
        assert_eq!(twips_pixel(55), 22);
        assert_eq!(twips_pixel(1219), 507); // e10: printed 60.864, pixel 507
    }

    #[test]
    fn every_pixel_has_a_twip_value() {
        for pixel in 0..3000 {
            assert_eq!(twips_pixel(twips_for_pixel(pixel)), pixel);
        }
    }

    #[test]
    fn twips_rule_repeats_every_60_twips() {
        for twips in 0..600 {
            assert_eq!(twips_pixel(twips + 60), twips_pixel(twips) + 25);
        }
    }

    #[test]
    fn measured_letter_values() {
        // values word printed for frames in the e10 x probe on a letter page
        let table = letter_x();
        assert_eq!(exported_x(600, &table), 72.024);
        assert_eq!(exported_x(500, &table), 60.0);
        assert_eq!(exported_x(507, &table), 60.864);
        assert_eq!(printed_text(exported_x_milli(907, &table)), "108.86");
        assert_eq!(printed_text(exported_x_milli(4830, &table)), "579.72");
    }

    #[test]
    fn fifth_step_wraps_to_the_next_pixel_value() {
        let table = letter_x();
        let units = table.units(4823);
        assert_eq!(units, 5 * 4824);
        assert_eq!(table.pixel_for_units(units), Some(4823));
    }

    #[test]
    fn decode_inverts_export() {
        let (x, y) = (letter_x(), letter_y());
        for pixel in (0..5000).step_by(7) {
            assert_eq!(decode_x(exported_x(pixel, &x), &x), Some(pixel));
        }
        for pixel in (81..6500).step_by(11) {
            assert_eq!(decode_y(exported_y(pixel, &y, 6600), &y, 6600), Some(pixel));
        }
    }

    #[test]
    fn nearest_exported_pixel_is_never_beaten() {
        let table = letter_x();
        for target in [72.03, 99.99, 300.017, 450.06, 61.0] {
            let best = nearest_exported_pixel(target, &table);
            for other in best - 3..best + 4 {
                assert!(
                    (exported_x(best, &table) - target).abs()
                        <= (exported_x(other, &table) - target).abs() + 1e-12
                );
            }
            assert!(lattice_error(target, &table) <= 0.072 + 0.005);
        }
    }

    #[test]
    fn page_pixels_round_the_media_box() {
        assert_eq!(page_pixels(595.3), 4961);
        assert_eq!(page_pixels(841.9), 7016);
        assert_eq!(page_pixels(612.0), 5100);
    }

    #[test]
    fn measured_baseline_offsets() {
        // constant offsets measured for these exact line heights (round 4 tables)
        for (line_twips, offset) in [
            (140, 47),
            (240, 80),
            (260, 87),
            (300, 100),
            (380, 127),
            (900, 300),
        ] {
            for y in 1200..1260 {
                assert_eq!(baseline_offset_pixels(line_twips, y), offset);
            }
        }
    }

    #[test]
    fn two_valued_heights_follow_the_bottom_edge() {
        let values: std::collections::BTreeSet<i64> = (1200..1260)
            .map(|y| baseline_offset_pixels(220, y))
            .collect();
        assert_eq!(values.into_iter().collect::<Vec<_>>(), [73, 74]);
        // the measured table for 220 twips starts 0,1,1,1,1,0 above its minimum
        let start: Vec<i64> = (0..6)
            .map(|r| baseline_offset_pixels(220, 1200 + r) - 73)
            .collect();
        assert_eq!(start, [0, 1, 1, 1, 1, 0]);
    }

    #[test]
    fn word_positions_have_zero_floor() {
        let (x, y) = (letter_x(), letter_y());
        for pixel in [100, 2000, 5000] {
            let printed_y = exported_y(pixel, &y, 6600);
            assert!((nearest_exported_y(printed_y, &y, 6600) - printed_y).abs() < 1e-9);
            assert!(lattice_error(exported_x(pixel / 2, &x), &x) < 1e-9);
        }
    }

    #[test]
    fn estimated_table_is_close_to_letter_width() {
        // close, but up to 3 pixels late: good enough to convert without
        // word, never good enough to skip measuring when word is there
        let estimate = estimated_step_table(5100);
        assert_eq!(estimate.steps.len(), 5);
        for (guess, measured) in estimate.steps.iter().zip(letter_x().steps) {
            assert!((guess - measured).abs() <= 3, "{guess} vs {measured}");
        }
    }
}
