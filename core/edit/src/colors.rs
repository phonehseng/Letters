//! colours as the app sends them (0-255 per channel) and as content
//! streams set them.

use serde::{Deserialize, Serialize};

use crate::pdf_values::format_number;

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

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub(crate) fn from_fractions(r: f32, g: f32, b: f32) -> Self {
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self::new(byte(r), byte(g), byte(b))
    }

    /// the operator that makes this the fill colour, e.g. "1 0 0 rg".
    pub(crate) fn fill_operator(self) -> String {
        let channel = |value: u8| format_number(f32::from(value) / 255.0);
        format!(
            "{} {} {} rg",
            channel(self.r),
            channel(self.g),
            channel(self.b)
        )
    }
}
