//! opendocument lengths, percentages and colours, to and from the
//! model's units. lengths are written in centimetres with six decimals:
//! that keeps every emu exact (one emu is 1/360000 cm), where the usual
//! three decimals would move objects by up to 180 emu on every save.

use crate::model::{Emu, EMU_PER_CM, EMU_PER_INCH, EMU_PER_POINT};

/// "2.328cm", "1in", "12pt", "5mm", "0.5pc" as emu; `None` for anything else.
pub fn length(text: &str) -> Option<Emu> {
    let text = text.trim();
    let split = text
        .find(|c: char| c.is_ascii_alphabetic() || c == '%')
        .unwrap_or(text.len());
    let value: f64 = text[..split].parse().ok()?;
    let per_unit = match &text[split..] {
        "cm" => EMU_PER_CM as f64,
        "mm" => EMU_PER_CM as f64 / 10.0,
        "in" | "inch" => EMU_PER_INCH as f64,
        "pt" => EMU_PER_POINT as f64,
        "pc" => EMU_PER_POINT as f64 * 12.0,
        "px" => EMU_PER_INCH as f64 / 96.0,
        "" => 1.0,
        _ => return None,
    };
    Some((value * per_unit).round() as Emu)
}

pub fn write_length(emu: Emu) -> String {
    let centimetres = emu as f64 / EMU_PER_CM as f64;
    let text = format!("{centimetres:.6}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    format!(
        "{}cm",
        if trimmed.is_empty() || trimmed == "-" {
            "0"
        } else {
            trimmed
        }
    )
}

/// "90%" as thousandths of a percent (90000).
pub fn percent(text: &str) -> Option<i32> {
    let value: f64 = text.trim().strip_suffix('%')?.parse().ok()?;
    Some((value * 1000.0).round() as i32)
}

/// a font size ("24pt") in hundredths of a point.
pub fn font_size(text: &str) -> Option<i32> {
    length(text).map(|emu| (emu as f64 / EMU_PER_POINT as f64 * 100.0).round() as i32)
}

pub fn write_points(hundredths: i32) -> String {
    let points = f64::from(hundredths) / 100.0;
    let text = format!("{points:.2}");
    format!("{}pt", text.trim_end_matches('0').trim_end_matches('.'))
}

/// "#4472c4" as six upper-case hex digits.
pub fn color(text: &str) -> Option<String> {
    let hex = text.trim().strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| hex.to_ascii_uppercase())
}

/// "rotate (0.523) translate (2cm 3cm)": the rotation in radians
/// (counter-clockwise, as odf measures it) and the translation in emu.
pub fn transform(text: &str) -> (f64, Option<(Emu, Emu)>) {
    let mut rotation = 0.0;
    let mut translation = None;
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let arguments: Vec<&str> = rest[open + 1..open + close]
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|a| !a.is_empty())
            .collect();
        match name {
            "rotate" => {
                rotation = arguments
                    .first()
                    .and_then(|a| a.parse().ok())
                    .unwrap_or(0.0)
            }
            "translate" => {
                let x = arguments.first().and_then(|a| length(a)).unwrap_or(0);
                let y = arguments.get(1).and_then(|a| length(a)).unwrap_or(0);
                translation = Some((x, y));
            }
            _ => {}
        }
        rest = &rest[open + close + 1..];
    }
    (rotation, translation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_convert_exactly() {
        assert_eq!(length("2.54cm"), Some(914_400));
        assert_eq!(length("1in"), Some(914_400));
        assert_eq!(length("12pt"), Some(152_400));
        assert_eq!(length("-5mm"), Some(-180_000));
        assert_eq!(length("12em"), None);
        for emu in [0, 1, 12_700, 914_400, 6_858_000, 1_234_567, -91_440] {
            assert_eq!(length(&write_length(emu)), Some(emu), "{emu}");
        }
    }

    #[test]
    fn percentages_sizes_and_colours() {
        assert_eq!(percent("90%"), Some(90_000));
        assert_eq!(font_size("24pt"), Some(2_400));
        assert_eq!(write_points(1_850), "18.5pt");
        assert_eq!(color("#4472c4").as_deref(), Some("4472C4"));
    }

    #[test]
    fn transforms_give_rotation_and_offset() {
        let (rotation, offset) = transform("rotate (0.5) translate (2cm 3cm)");
        assert_eq!(rotation, 0.5);
        assert_eq!(offset, Some((720_000, 1_080_000)));
    }
}
