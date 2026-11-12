//! the measured rules of word's layout for real paragraphs, on top of
//! round one's (recipe 6). all measured on word 16.0.20527 with
//! `tests/hybrid_probe.rs`: 6 documents of 13 paragraphs with 3 lines each,
//! random line heights (228 to 300 twips), spacing before and after (0 to
//! 120 twips) and indents (0 to 1081 twips), stacked in one frame or
//! flowing in the body.

/// pixels word adds for an indent of `twips` inside a frame: 5/12 pixel a
/// twip, rounded half down (234 of 234 line starts: 378 twips is 157.5 px
/// and lands on 157, 359 twips is 149.6 px and lands on 150). the left
/// indent and the first-line indent are added in twips first (378 = 359 +
/// 19 lands on 157, not 150 + 8).
pub fn indent_pixels(twips: i64) -> i64 {
    (5 * twips + 5).div_euclid(12)
}

/// the smallest indent in twips that word turns into `pixels` pixels.
pub fn twips_for_indent(pixels: i64) -> i64 {
    // every pixel value is reached: 5/12 is below one
    let mut twips = (12 * pixels - 5).div_euclid(5) - 1;
    while indent_pixels(twips) < pixels {
        twips += 1;
    }
    // no indent is written as 0, not as the -1 twips word also reads as 0 px
    if pixels == 0 {
        return 0;
    }
    twips
}

/// between two paragraphs word keeps the larger of the first one's space
/// after and the second one's space before, not their sum (measured: a
/// space before of 31 twips after a space after of 60 moved the paragraph
/// 13 px less than the sum; the larger of the two explained all 144
/// paragraph gaps, 72 stacked and 72 in the body, up to the pixel-edge
/// effect below). the converter
/// therefore writes every gap as space before and no space after.
pub fn paragraph_gap(after_previous: i64, before: i64) -> i64 {
    after_previous.max(before)
}

/// a line top exactly on a pixel edge (`5t + 1` a multiple of 12) is
/// placed unpredictably inside a stack of paragraphs: 25 of 32 edge lines
/// differed from the frame rule (recipe 6.1) by a pixel, in both
/// directions; off the edges every following line was exact (135 of 135).
/// placement avoids these line tops.
pub fn on_pixel_edge(top_twips: i64) -> bool {
    (5 * top_twips + 1).rem_euclid(12) == 0
}

/// word squeezes the spaces of a justified line by up to about a quarter
/// of their width rather than move its last word down (letters-layout,
/// `flavor.rs`, measured on its oracle corpus).
pub const JUSTIFY_SQUEEZE: f64 = 0.25;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indents_round_half_down() {
        // the probe's line starts (tests/hybrid_probe.rs)
        let measured = [
            (0, 0),
            (7, 3),
            (19, 8),
            (133, 55),
            (152, 63),
            (226, 94),
            (359, 150),
            (360, 150),
            (378, 157),
            (493, 205),
            (588, 245),
            (721, 300),
            (740, 308),
            (1081, 450),
        ];
        for (twips, pixels) in measured {
            assert_eq!(indent_pixels(twips), pixels, "{twips} twips");
        }
    }

    #[test]
    fn every_pixel_has_an_indent() {
        for pixels in -50..400 {
            let twips = twips_for_indent(pixels);
            assert_eq!(indent_pixels(twips), pixels);
            if pixels != 0 {
                assert!(indent_pixels(twips - 1) < pixels);
            }
        }
    }

    #[test]
    fn gaps_take_the_larger_spacing() {
        assert_eq!(paragraph_gap(60, 31), 60);
        assert_eq!(paragraph_gap(0, 120), 120);
    }

    #[test]
    fn pixel_edges_repeat_every_twelve_twips() {
        // 5 * 7 + 1 = 36
        assert!(on_pixel_edge(7));
        assert!(on_pixel_edge(19));
        assert!(!on_pixel_edge(8));
    }
}
