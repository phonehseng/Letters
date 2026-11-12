//! converting lengths between the units files use and twips.

use crate::model::Twips;

/// english metric units per twip. drawings in docx measure in emu:
/// 914400 to the inch, so 635 to the twip.
pub const EMU_PER_TWIP: i64 = 635;

pub fn emu_to_twips(emu: i64) -> Twips {
    (emu as f64 / EMU_PER_TWIP as f64).round() as Twips
}

pub fn twips_to_emu(twips: Twips) -> i64 {
    i64::from(twips) * EMU_PER_TWIP
}

/// a length with a unit ("2.54cm", "0.5in", "12pt", "-3mm") in twips.
/// a bare number is taken as twips, as word writes them.
pub fn parse_length(text: &str) -> Option<Twips> {
    let text = text.trim();
    let split = text
        .find(|c: char| c.is_ascii_alphabetic() || c == '%')
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let value: f64 = number.trim().parse().ok()?;
    let per_unit = match unit.trim() {
        "" => 1.0,
        "in" => 1440.0,
        "cm" => 1440.0 / 2.54,
        "mm" => 1440.0 / 25.4,
        "pt" => 20.0,
        "pc" | "pi" => 240.0,
        "px" => 15.0,
        _ => return None,
    };
    let twips = value * per_unit;
    if twips.is_finite() && twips.abs() < 1.0e9 {
        Some(twips.round() as Twips)
    } else {
        None
    }
}

/// twips as inches for odt, with enough digits to come back exactly.
pub fn twips_to_inches(twips: Twips) -> String {
    let inches = f64::from(twips) / 1440.0;
    let mut text = format!("{inches:.4}");
    while text.contains('.') && (text.ends_with('0') || text.ends_with('.')) {
        text.pop();
    }
    if text == "-0" {
        text = "0".into();
    }
    format!("{text}in")
}

/// a length with a unit in emu, for drawings: twips would round word's
/// positions by up to half a twip, which shows up after a round trip.
pub fn parse_length_emu(text: &str) -> Option<i64> {
    let text = text.trim();
    let split = text
        .find(|c: char| c.is_ascii_alphabetic() || c == '%')
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let value: f64 = number.trim().parse().ok()?;
    let per_unit = match unit.trim() {
        "" => EMU_PER_TWIP as f64,
        "in" => 914_400.0,
        "cm" => 360_000.0,
        "mm" => 36_000.0,
        "pt" => 12_700.0,
        "pc" | "pi" => 152_400.0,
        "px" => 9_525.0,
        _ => return None,
    };
    let emu = value * per_unit;
    (emu.is_finite() && emu.abs() < 1.0e15).then(|| emu.round() as i64)
}

/// emu as inches for odt drawings. six decimals keep every emu (an emu
/// is 0.0000011 inch), so the exact value comes back.
pub fn emu_to_inches(emu: i64) -> String {
    let inches = emu as f64 / 914_400.0;
    let mut text = format!("{inches:.7}");
    while text.contains('.') && (text.ends_with('0') || text.ends_with('.')) {
        text.pop();
    }
    if text == "-0" {
        text = "0".into();
    }
    format!("{text}in")
}

/// twips as points for odt font sizes and border widths.
pub fn points(value: f64) -> String {
    let mut text = format!("{value:.2}");
    while text.contains('.') && (text.ends_with('0') || text.ends_with('.')) {
        text.pop();
    }
    format!("{text}pt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_in_every_unit() {
        assert_eq!(parse_length("1in"), Some(1440));
        assert_eq!(parse_length("2.54cm"), Some(1440));
        assert_eq!(parse_length("25.4mm"), Some(1440));
        assert_eq!(parse_length("12pt"), Some(240));
        assert_eq!(parse_length("-0.5in"), Some(-720));
        assert_eq!(parse_length("720"), Some(720));
        assert_eq!(parse_length("abc"), None);
        assert_eq!(parse_length("1furlong"), None);
    }

    #[test]
    fn inches_come_back_exactly() {
        for twips in [-1440, 0, 1, 7, 360, 720, 1439, 12240, 15840] {
            assert_eq!(parse_length(&twips_to_inches(twips)), Some(twips));
        }
        assert_eq!(twips_to_inches(720), "0.5in");
    }

    #[test]
    fn emu_come_back_exactly_through_inches() {
        for emu in [0, 1, 635, 1_905_000, 2_895_600, 914_401, -12_700] {
            assert_eq!(parse_length_emu(&emu_to_inches(emu)), Some(emu));
        }
        assert_eq!(parse_length_emu("1cm"), Some(360_000));
    }

    #[test]
    fn emu() {
        assert_eq!(emu_to_twips(914400), 1440);
        assert_eq!(twips_to_emu(1440), 914400);
    }
}
