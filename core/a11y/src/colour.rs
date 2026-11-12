//! colour contrast maths from wcag 2.
//!
//! contrast is worked out from relative luminance, how bright a colour
//! looks to the eye: 0 for black, 1 for white. the ratio of the brighter
//! colour to the darker one (each plus 0.05, for glare) runs from 1:1 (the
//! same colour) to 21:1 (black on white).

use serde::{Deserialize, Serialize};

/// a colour as 8-bit red, green and blue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// from components between 0 and 1, as pdf stores them.
    pub fn from_unit(r: f32, g: f32, b: f32) -> Self {
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self::new(byte(r), byte(g), byte(b))
    }

    /// as #rrggbb, the way people write colours.
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn unit(&self) -> [f32; 3] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        ]
    }

    /// relative luminance as wcag 2 defines it.
    pub fn luminance(&self) -> f64 {
        let channel = |value: u8| {
            let value = value as f64 / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    fn mix(&self, other: Rgb, amount: f64) -> Rgb {
        let blend = |from: u8, to: u8| {
            (from as f64 + (to as f64 - from as f64) * amount)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Rgb::new(
            blend(self.r, other.r),
            blend(self.g, other.g),
            blend(self.b, other.b),
        )
    }
}

/// the wcag 2 contrast ratio between two colours, from 1 to 21.
pub fn contrast_ratio(first: Rgb, second: Rgb) -> f64 {
    let (a, b) = (first.luminance(), second.luminance());
    let (lighter, darker) = if a > b { (a, b) } else { (b, a) };
    (lighter + 0.05) / (darker + 0.05)
}

/// the ratio text needs: 4.5:1 normally, 3:1 for large text (wcag counts
/// 18 points and up, or 14 points and up when bold, as large).
pub fn required_ratio(font_size: f32, bold: bool) -> f64 {
    if is_large_text(font_size, bold) {
        3.0
    } else {
        4.5
    }
}

pub fn is_large_text(font_size: f32, bold: bool) -> bool {
    font_size >= 18.0 || (bold && font_size >= 14.0)
}

/// the colour closest to `text` that reaches `required` against
/// `background`, found by moving the text colour towards black or towards
/// white, whichever needs the smaller change. keeping the same hue means
/// the document still looks the way its author meant, only clearer.
pub fn nearest_passing_colour(text: Rgb, background: Rgb, required: f64) -> Rgb {
    if contrast_ratio(text, background) >= required {
        return text;
    }
    let candidates = [Rgb::BLACK, Rgb::WHITE]
        .into_iter()
        .filter_map(|target| smallest_passing_mix(text, target, background, required));
    candidates
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, colour)| colour)
        // neither direction can reach it (only possible on mid-grey
        // backgrounds for very high targets): pick the stronger extreme
        .unwrap_or_else(|| {
            if contrast_ratio(Rgb::BLACK, background) >= contrast_ratio(Rgb::WHITE, background) {
                Rgb::BLACK
            } else {
                Rgb::WHITE
            }
        })
}

/// how far towards `target` the colour must move to pass, by halving the
/// interval: contrast changes steadily along the way, so this finds the
/// first passing point.
fn smallest_passing_mix(
    text: Rgb,
    target: Rgb,
    background: Rgb,
    required: f64,
) -> Option<(f64, Rgb)> {
    if contrast_ratio(target, background) < required {
        return None;
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if contrast_ratio(text.mix(target, middle), background) >= required {
            high = middle;
        } else {
            low = middle;
        }
    }
    // rounding to whole bytes can land just short, so step on until it passes
    let mut amount = high;
    loop {
        let colour = text.mix(target, amount);
        if contrast_ratio(colour, background) >= required || amount >= 1.0 {
            return Some((amount, colour));
        }
        amount = (amount + 0.002).min(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(level: u8) -> Rgb {
        Rgb::new(level, level, level)
    }

    #[test]
    fn black_on_white_is_twenty_one_to_one() {
        assert!((contrast_ratio(Rgb::BLACK, Rgb::WHITE) - 21.0).abs() < 0.001);
    }

    #[test]
    fn the_same_colour_is_one_to_one() {
        assert!((contrast_ratio(grey(120), grey(120)) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn grey_777_on_white_just_fails() {
        let ratio = contrast_ratio(grey(0x77), Rgb::WHITE);
        assert!((ratio - 4.48).abs() < 0.01, "{ratio}");
        assert!(ratio < 4.5);
    }

    #[test]
    fn grey_767676_on_white_just_passes() {
        let ratio = contrast_ratio(grey(0x76), Rgb::WHITE);
        assert!(ratio >= 4.5, "{ratio}");
        assert!((ratio - 4.54).abs() < 0.01);
    }

    #[test]
    fn large_text_needs_less() {
        assert_eq!(required_ratio(12.0, false), 4.5);
        assert_eq!(required_ratio(18.0, false), 3.0);
        assert_eq!(required_ratio(14.0, true), 3.0);
        assert_eq!(required_ratio(14.0, false), 4.5);
    }

    #[test]
    fn the_nearest_passing_colour_passes_and_stays_close() {
        let suggestion = nearest_passing_colour(grey(0x99), Rgb::WHITE, 4.5);
        assert!(contrast_ratio(suggestion, Rgb::WHITE) >= 4.5);
        // only a little darker than needed: #767676 is the lightest passing grey
        assert!(
            suggestion.r >= 0x70 && suggestion.r <= 0x76,
            "{suggestion:?}"
        );
    }

    #[test]
    fn light_text_on_a_dark_background_gets_lighter() {
        let suggestion = nearest_passing_colour(grey(0x60), grey(0x20), 4.5);
        assert!(suggestion.r > 0x60);
        assert!(contrast_ratio(suggestion, grey(0x20)) >= 4.5);
    }

    #[test]
    fn colours_print_as_hex() {
        assert_eq!(Rgb::new(255, 0, 16).hex(), "#ff0010");
    }
}
