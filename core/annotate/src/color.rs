//! colours as the app sends them (0-255 per channel) and as pdf stores them.

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

use crate::pdf_values::{format_number, numbers, real};

/// an opaque colour, 0 to 255 per channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color::new(0, 0, 0);
    pub const WHITE: Color = Color::new(255, 255, 255);
    /// the yellow most highlighters use
    pub const HIGHLIGHT_YELLOW: Color = Color::new(255, 212, 0);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    fn channels(self) -> [f32; 3] {
        [self.r, self.g, self.b].map(|channel| f32::from(channel) / 255.0)
    }

    /// the colour as a pdf array of three numbers between 0 and 1.
    pub(crate) fn to_object(self) -> Object {
        Object::Array(self.channels().map(real).to_vec())
    }

    /// reads a pdf colour array. pdf allows 0 (transparent), 1 (grey),
    /// 3 (rgb) or 4 (cmyk) numbers; transparent and malformed give none.
    pub(crate) fn from_object(doc: &Document, object: &Object) -> Option<Color> {
        let values = numbers(doc, object)?;
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        match values.as_slice() {
            [grey] => Some(Color::new(byte(*grey), byte(*grey), byte(*grey))),
            [r, g, b] => Some(Color::new(byte(*r), byte(*g), byte(*b))),
            // the simple conversion pdf itself describes for cmyk to rgb
            [c, m, y, k] => Some(Color::new(
                byte(1.0 - (c + k).min(1.0)),
                byte(1.0 - (m + k).min(1.0)),
                byte(1.0 - (y + k).min(1.0)),
            )),
            _ => None,
        }
    }

    /// the content stream operator that sets this as the fill colour.
    pub(crate) fn fill_operator(self) -> String {
        format!("{} rg", self.operands())
    }

    /// the content stream operator that sets this as the stroke colour.
    pub(crate) fn stroke_operator(self) -> String {
        format!("{} RG", self.operands())
    }

    fn operands(self) -> String {
        self.channels().map(format_number).join(" ")
    }
}

/// keeps an opacity between fully clear and fully solid. anything that
/// isn't a number becomes solid, the safe choice for a mark.
pub(crate) fn clamp_opacity(opacity: f32) -> f32 {
    if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_values::number_array;

    #[test]
    fn colours_round_trip_through_pdf() {
        let doc = Document::new();
        let colour = Color::new(255, 128, 0);
        assert_eq!(Color::from_object(&doc, &colour.to_object()), Some(colour));
    }

    #[test]
    fn grey_and_cmyk_are_understood() {
        let doc = Document::new();
        let grey = Color::from_object(&doc, &number_array(&[0.5]));
        assert_eq!(grey, Some(Color::new(128, 128, 128)));
        let cyan = Color::from_object(&doc, &number_array(&[1.0, 0.0, 0.0, 0.0]));
        assert_eq!(cyan, Some(Color::new(0, 255, 255)));
        assert_eq!(Color::from_object(&doc, &number_array(&[])), None);
    }

    #[test]
    fn operators_use_fractions() {
        assert_eq!(Color::new(255, 0, 51).fill_operator(), "1 0 0.2 rg");
        assert_eq!(Color::BLACK.stroke_operator(), "0 0 0 RG");
    }
}
