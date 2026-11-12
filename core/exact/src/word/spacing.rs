//! how word turns each glyph's `w:spacing` into whole layout pixels, and the
//! planner that picks spacings so every glyph lands on its target pixel
//! (recipe 6.5 and 7.4, probes e15 and e16 on word 16.0.20527).
//!
//! all arithmetic is done in twelfths of a pixel: a twip is 5/12 pixel, so
//! every spacing and every carried remainder is a whole number of twelfths
//! and no rounding error can creep in.

use std::fmt;

/// twelfths of a pixel in one twip.
const TWELFTHS_PER_TWIP: i64 = 5;
const TWELFTHS_PER_PIXEL: i64 = 12;

/// the first glyph of a line is the line start, so its target must be 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstTargetNotZero;

impl fmt::Display for FirstTargetNotZero {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the first glyph is the line start, its target must be 0")
    }
}

/// whole pixels word adds for one glyph's `w:spacing` (e15).
///
/// each glyph's spacing is converted on its own, `twips * 5/12` rounded half
/// away from zero; the fraction is dropped, never carried to the next glyph.
pub fn spacing_pixels(twips: i64) -> i64 {
    // floor(|t| * 5/12 + 1/2) = floor((10 |t| + 12) / 24)
    let magnitude = (10 * twips.abs() + TWELFTHS_PER_PIXEL) / (2 * TWELFTHS_PER_PIXEL);
    magnitude * twips.signum()
}

/// a `w:spacing` value word turns into exactly `pixels` pixels.
///
/// every twip value whose 5/12 pixel lies strictly within half a pixel of
/// the wanted pixels works (two or three of them). the one chosen keeps the
/// carried remainder (exact spacing minus whole pixels so far, in twelfths)
/// closest to zero, then the smaller |twips| (recipe 7.4).
pub fn twips_for_spacing_pixels(pixels: i64, carried_twelfths: i64) -> i64 {
    let wanted = TWELFTHS_PER_PIXEL * pixels;
    let lowest = (wanted - 6).div_euclid(TWELFTHS_PER_TWIP) - 1;
    let highest = (wanted + 6).div_euclid(TWELFTHS_PER_TWIP) + 1;
    (lowest..=highest)
        .filter(|twips| (TWELFTHS_PER_TWIP * twips - wanted).abs() < TWELFTHS_PER_PIXEL / 2)
        .min_by_key(|twips| {
            let remainder = carried_twelfths + TWELFTHS_PER_TWIP * twips - wanted;
            (remainder.abs(), twips.abs())
        })
        .expect("every whole pixel step has twip values")
}

/// `w:spacing` twips after each glyph so word puts glyph i at
/// `target_pixels[i]` (with `target_pixels[0] = 0`).
///
/// word turns each glyph's spacing into whole pixels on its own (e15), but
/// it also keeps the exact sum and moves a glyph by a pixel once the two
/// drift about a pixel apart (e16: 47 of 1,980 steps with random
/// spacings). choosing among the twip values that give the wanted pixels
/// the one that keeps the exact sum within 5/24 pixel of the whole pixels
/// stops the drift from building up (0 of 1,980). the last glyph's spacing
/// is 0.
pub fn spacing_for_pixel_steps(
    advance_pixels: &[i64],
    target_pixels: &[i64],
) -> Result<Vec<i64>, FirstTargetNotZero> {
    if target_pixels.first() != Some(&0) {
        return Err(FirstTargetNotZero);
    }
    let mut spacing = Vec::with_capacity(target_pixels.len());
    let mut carried = 0;
    for i in 1..target_pixels.len() {
        let step = target_pixels[i] - target_pixels[i - 1] - advance_pixels[i - 1];
        let twips = twips_for_spacing_pixels(step, carried);
        carried += TWELFTHS_PER_TWIP * twips - TWELFTHS_PER_PIXEL * step;
        spacing.push(twips);
    }
    spacing.push(0);
    Ok(spacing)
}

/// pen offsets word produces for these advances and spacings (the e15 rule).
pub fn pen_pixels(advance_pixels: &[i64], spacing_twips: &[i64]) -> Vec<i64> {
    let mut pens = vec![0];
    for (advance, twips) in advance_pixels
        .iter()
        .zip(spacing_twips)
        .take(advance_pixels.len().saturating_sub(1))
    {
        let last = *pens.last().expect("pens start with the line start");
        pens.push(last + advance + spacing_pixels(*twips));
    }
    pens
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a small seeded generator, so random tests repeat exactly.
    struct Seeded(u64);
    impl Seeded {
        fn next_in(&mut self, low: i64, high: i64) -> i64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            low + ((self.0 >> 33) % (high - low + 1) as u64) as i64
        }
    }

    #[test]
    fn each_spacing_rounds_half_away_from_zero() {
        // e15: 6 twips = 2.5 px -> 3, -6 -> -3, 1 -> 0, 2 -> 1
        let pixels: Vec<i64> = [6, -6, 1, 2, -1, 0, 12].map(spacing_pixels).to_vec();
        assert_eq!(pixels, [3, -3, 0, 1, 0, 0, 5]);
    }

    #[test]
    fn pairs_add_their_own_roundings() {
        // e15 pairs: (13, -11) gives 5 - 5 = 0, although their sum alone would round to 1
        assert_eq!(pen_pixels(&[53, 53, 53], &[13, -11, 0]), [0, 58, 106]);
    }

    #[test]
    fn planned_steps_are_reproduced_and_the_remainder_stays_small() {
        let mut random = Seeded(4);
        let advances: Vec<i64> = (0..200).map(|_| random.next_in(15, 80)).collect();
        let mut targets = vec![0];
        for advance in &advances[..advances.len() - 1] {
            let last = *targets.last().unwrap();
            targets.push(last + advance + random.next_in(-15, 15));
        }
        let spacing = spacing_for_pixel_steps(&advances, &targets).unwrap();
        assert_eq!(pen_pixels(&advances, &spacing), targets);
        let mut carried = 0;
        for (i, twips) in spacing[..spacing.len() - 1].iter().enumerate() {
            let step = targets[i + 1] - targets[i] - advances[i];
            carried += 5 * twips - 12 * step;
            // 5/24 pixel is 2.5 twelfths
            assert!(
                2 * carried.abs() <= 5,
                "remainder {carried}/12 px at glyph {i}"
            );
        }
    }

    #[test]
    fn every_pixel_step_has_twips() {
        for pixels in -40..=40 {
            assert_eq!(spacing_pixels(twips_for_spacing_pixels(pixels, 0)), pixels);
        }
    }

    #[test]
    fn the_first_target_must_be_the_line_start() {
        assert_eq!(
            spacing_for_pixel_steps(&[10], &[1]),
            Err(FirstTargetNotZero)
        );
    }

    #[test]
    fn the_last_glyph_gets_no_spacing() {
        assert_eq!(
            spacing_for_pixel_steps(&[10, 10], &[0, 12]).unwrap(),
            [5, 0]
        );
    }
}
