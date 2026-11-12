//! the fill and stroke colours a content stream sets, as plain rgb.
//!
//! only gray, rgb and cmyk (and icc spaces with that many channels) are
//! understood. patterns, spot colours and the rest give "unknown", and
//! whatever is painted in them is left to the background picture.

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

use crate::source::resolve;

/// an opaque colour, 0 to 255 per channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);

    /// "1f6feb", the way css, ooxml and spreadsheets write colours.
    pub fn hex(&self) -> String {
        format!("{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    fn from_fractions(r: f32, g: f32, b: f32) -> Self {
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgb(byte(r), byte(g), byte(b))
    }

    /// the simple cmyk to rgb mix, without a colour profile.
    fn from_cmyk(c: f32, m: f32, y: f32, k: f32) -> Self {
        Self::from_fractions(
            (1.0 - c) * (1.0 - k),
            (1.0 - m) * (1.0 - k),
            (1.0 - y) * (1.0 - k),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColorSpaceKind {
    Gray,
    Rgb,
    Cmyk,
    Other,
}

/// the current colour for filling or stroking.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PaintColor {
    pub space: ColorSpaceKind,
    /// none when the colour can't be shown as plain rgb
    pub rgb: Option<Rgb>,
}

impl PaintColor {
    pub fn black() -> Self {
        Self {
            space: ColorSpaceKind::Gray,
            rgb: Some(Rgb::BLACK),
        }
    }
}

/// applies a colour operator to the fill or stroke colour. operators
/// that aren't about colour are ignored.
pub(crate) fn apply(
    operator: &str,
    operands: &[Object],
    numbers: &[f32],
    fill: &mut PaintColor,
    stroke: &mut PaintColor,
    space_named: impl Fn(&[u8]) -> ColorSpaceKind,
) {
    let target = if operator
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
    {
        stroke
    } else {
        fill
    };
    match operator {
        "g" | "G" => set(target, ColorSpaceKind::Gray, numbers),
        "rg" | "RG" => set(target, ColorSpaceKind::Rgb, numbers),
        "k" | "K" => set(target, ColorSpaceKind::Cmyk, numbers),
        "cs" | "CS" => {
            let kind = match operands.first() {
                Some(Object::Name(name)) => space_named(name),
                _ => ColorSpaceKind::Other,
            };
            // a new colour space starts at its darkest colour
            let start: &[f32] = match kind {
                ColorSpaceKind::Cmyk => &[0.0, 0.0, 0.0, 1.0],
                _ => &[0.0, 0.0, 0.0],
            };
            set(target, kind, start);
        }
        "sc" | "SC" | "scn" | "SCN" => {
            // a pattern name among the operands means a pattern fill
            let has_name = operands.iter().any(|o| matches!(o, Object::Name(_)));
            let kind = if has_name {
                ColorSpaceKind::Other
            } else {
                target.space
            };
            set(target, kind, numbers);
        }
        _ => {}
    }
}

fn set(target: &mut PaintColor, space: ColorSpaceKind, numbers: &[f32]) {
    target.space = space;
    target.rgb = match (space, numbers) {
        (ColorSpaceKind::Gray, [gray, ..]) => Some(Rgb::from_fractions(*gray, *gray, *gray)),
        (ColorSpaceKind::Rgb, [r, g, b, ..]) => Some(Rgb::from_fractions(*r, *g, *b)),
        (ColorSpaceKind::Cmyk, [c, m, y, k, ..]) => Some(Rgb::from_cmyk(*c, *m, *y, *k)),
        _ => None,
    };
}

/// what kind of space a colour space resource is.
pub(crate) fn kind_of_space(doc: &Document, space: &Object) -> ColorSpaceKind {
    match space {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" => ColorSpaceKind::Gray,
            b"DeviceRGB" => ColorSpaceKind::Rgb,
            b"DeviceCMYK" => ColorSpaceKind::Cmyk,
            _ => ColorSpaceKind::Other,
        },
        Object::Array(parts) => {
            let family = parts.first().and_then(|p| p.as_name().ok());
            match family {
                Some(b"CalGray") => ColorSpaceKind::Gray,
                Some(b"CalRGB") => ColorSpaceKind::Rgb,
                Some(b"ICCBased") => {
                    let channels = parts
                        .get(1)
                        .and_then(|p| resolve(doc, p).as_stream().ok())
                        .and_then(|s| s.dict.get(b"N").ok())
                        .and_then(|n| n.as_i64().ok());
                    match channels {
                        Some(1) => ColorSpaceKind::Gray,
                        Some(3) => ColorSpaceKind::Rgb,
                        Some(4) => ColorSpaceKind::Cmyk,
                        _ => ColorSpaceKind::Other,
                    }
                }
                _ => ColorSpaceKind::Other,
            }
        }
        _ => ColorSpaceKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_colours_become_rgb() {
        let mut fill = PaintColor::black();
        let mut stroke = PaintColor::black();
        apply("rg", &[], &[1.0, 0.5, 0.0], &mut fill, &mut stroke, |_| {
            ColorSpaceKind::Other
        });
        assert_eq!(fill.rgb, Some(Rgb(255, 128, 0)));
        apply(
            "K",
            &[],
            &[0.0, 0.0, 0.0, 1.0],
            &mut fill,
            &mut stroke,
            |_| ColorSpaceKind::Other,
        );
        assert_eq!(stroke.rgb, Some(Rgb::BLACK));
        assert_eq!(Rgb(255, 128, 0).hex(), "FF8000");
    }

    #[test]
    fn patterns_are_unknown() {
        let mut fill = PaintColor::black();
        let mut stroke = PaintColor::black();
        let operands = [Object::Name(b"P1".to_vec())];
        apply("scn", &operands, &[], &mut fill, &mut stroke, |_| {
            ColorSpaceKind::Other
        });
        assert_eq!(fill.rgb, None);
    }
}
