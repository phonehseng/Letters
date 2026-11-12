//! lengths: opendocument writes "2.258cm", "0.889in", "12pt"; excel
//! measures columns in characters of the default font and rows in points.
//! a column of w characters is w * 7 + 5 pixels wide at 96 pixels per
//! inch (excel's documented rule for its default font, calibri 11).

/// points in an opendocument length, none when it can't be read
pub fn points(length: &str) -> Option<f64> {
    let length = length.trim();
    let split = length
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(length.len());
    let (number, unit) = length.split_at(split);
    let value: f64 = number.trim().parse().ok()?;
    let factor = match unit {
        "pt" => 1.0,
        "in" => 72.0,
        "cm" => 72.0 / 2.54,
        "mm" => 72.0 / 25.4,
        "pc" => 12.0,
        "px" => 0.75,
        _ => return None,
    };
    Some(value * factor)
}

const PIXELS_PER_INCH: f64 = 96.0;

/// excel characters for a width in points
pub fn characters(width_points: f64) -> f64 {
    let pixels = width_points / 72.0 * PIXELS_PER_INCH;
    (((pixels - 5.0) / 7.0) * 100.0).round().max(0.0) / 100.0
}

/// an opendocument length (in inches) for a width in excel characters
pub fn column_width(characters: f64) -> String {
    let pixels = (characters * 7.0 + 5.0).trunc();
    format!("{:.4}in", pixels / PIXELS_PER_INCH)
}

/// an opendocument length for a row height in points
pub fn row_height(points: f64) -> String {
    format!("{:.4}in", points / 72.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_convert_both_ways() {
        assert_eq!(points("72pt"), Some(72.0));
        assert!((points("2.54cm").unwrap() - 72.0).abs() < 1e-9);
        assert_eq!(points("bad"), None);
        let width = column_width(8.43);
        let back = characters(points(&width).unwrap());
        assert!((back - 8.43).abs() < 0.1, "{back}");
    }
}
