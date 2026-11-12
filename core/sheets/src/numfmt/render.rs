//! applying a parsed format code to a value: choosing the section,
//! numbers with digit placeholders, thousands and scaling, percentages,
//! scientific notation, fractions, dates and times, and text.

use crate::calc::functions::math::{round_digits, Rounding};
use crate::model::{DateSystem, Value};

use super::dates::render_date;
use super::parse::{parse, Section, Token};

/// the separators of the reader's locale
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Locale {
    pub decimal: char,
    pub thousands: char,
}

impl Default for Locale {
    fn default() -> Locale {
        Locale {
            decimal: '.',
            thousands: ',',
        }
    }
}

/// a value as shown, with the colour the format asks for
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Formatted {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

pub fn format_value(value: &Value, code: &str, locale: &Locale, system: DateSystem) -> Formatted {
    match value {
        Value::Number(n) => format_number(*n, code, locale, system),
        Value::Text(text) => format_text(text, code),
        Value::Bool(b) => plain(if *b { "TRUE" } else { "FALSE" }),
        Value::Error(e) => plain(e.as_str()),
        Value::Blank => Formatted::default(),
    }
}

fn plain(text: &str) -> Formatted {
    Formatted {
        text: text.to_string(),
        color: None,
    }
}

/// text uses the fourth section, or a lone "@" section; otherwise it is
/// shown as it is
fn format_text(text: &str, code: &str) -> Formatted {
    let sections = parse(code);
    let section = match sections.len() {
        4 => Some(&sections[3]),
        1 if sections[0].has(&Token::Text) => Some(&sections[0]),
        _ => None,
    };
    let Some(section) = section else {
        return plain(text);
    };
    Formatted {
        text: text_section(text, section),
        color: section.color.clone(),
    }
}

/// a section with "@": its literals around the text
fn text_section(text: &str, section: &Section) -> String {
    let mut out = String::new();
    for token in &section.tokens {
        match token {
            Token::Text => out.push_str(text),
            Token::Literal(literal) => out.push_str(literal),
            Token::Space => out.push(' '),
            _ => {}
        }
    }
    out
}

fn condition_holds(section: &Section, n: f64) -> Option<bool> {
    let condition = section.condition?;
    Some(match condition.operator {
        "<" => n < condition.value,
        "<=" => n <= condition.value,
        ">" => n > condition.value,
        ">=" => n >= condition.value,
        "<>" => n != condition.value,
        _ => n == condition.value,
    })
}

/// the section a number uses and whether its minus sign is shown
fn pick_section(sections: &[Section], n: f64) -> (usize, bool) {
    if sections.iter().any(|s| s.condition.is_some()) {
        for (index, section) in sections.iter().enumerate().take(3) {
            // a section picked by its own condition shows no minus of its
            // own: -1 as [<0]"neg" is "neg" in excel
            match condition_holds(section, n) {
                Some(true) => return (index, index == 0 && n >= 0.0),
                Some(false) => continue,
                None => return (index, index == 0 || n >= 0.0),
            }
        }
        return (sections.len() - 1, false);
    }
    match (sections.len(), n) {
        (1, _) => (0, true),
        (_, n) if n < 0.0 => (1, false),
        (len, n) if n == 0.0 && len >= 3 => (2, true),
        _ => (0, true),
    }
}

pub fn format_number(n: f64, code: &str, locale: &Locale, system: DateSystem) -> Formatted {
    let sections = parse(code);
    let (index, show_sign) = pick_section(&sections, n);
    let Some(section) = sections.get(index) else {
        return plain(&super::general::general(n, locale.decimal));
    };
    let color = section.color.clone();
    let text = if section.has(&Token::Text) {
        // "@" given a number shows it as General would
        text_section(&super::general::general(n, locale.decimal), section)
    } else if section.is_date() {
        render_date(n, section, system)
    } else if section.tokens.contains(&Token::General) || section.tokens.is_empty() && index == 0 {
        general_section(n, section, show_sign, locale)
    } else {
        let value = if show_sign { n } else { n.abs() };
        render_number(value, section, locale)
    };
    Formatted { text, color }
}

/// "General" with literals around it ("General\" kg\"")
fn general_section(n: f64, section: &Section, show_sign: bool, locale: &Locale) -> String {
    let value = if show_sign { n } else { n.abs() };
    let mut out = String::new();
    for token in &section.tokens {
        match token {
            Token::General => out.push_str(&super::general::general(value, locale.decimal)),
            Token::Literal(text) => out.push_str(text),
            Token::Space => out.push(' '),
            _ => {}
        }
    }
    if section.tokens.is_empty() {
        out = super::general::general(value, locale.decimal);
    }
    out
}

fn render_number(n: f64, section: &Section, locale: &Locale) -> String {
    let tokens = &section.tokens;
    let percent = tokens.iter().filter(|t| **t == Token::Percent).count() as i32;
    let scale = tokens.iter().filter(|t| **t == Token::Scale).count() as i32;
    let value = n * 100f64.powi(percent) / 1000f64.powi(scale);
    let has_digits = tokens.iter().any(|t| matches!(t, Token::Digit(_)));
    if !has_digits {
        return literals_only(n, tokens);
    }
    if tokens.iter().any(|t| matches!(t, Token::Exponent { .. })) {
        return super::scientific::render(value, tokens, locale);
    }
    if tokens.contains(&Token::Slash) {
        if let Some(text) = super::fraction::render(value, tokens) {
            return text;
        }
    }
    render_plain(value, tokens, locale)
}

/// a section without digit placeholders ("\"none\"") shows its text,
/// with a minus for negative numbers
fn literals_only(n: f64, tokens: &[Token]) -> String {
    let mut out = String::new();
    if n < 0.0 {
        out.push('-');
    }
    for token in tokens {
        match token {
            Token::Literal(text) => out.push_str(text),
            Token::Space => out.push(' '),
            Token::Percent => out.push('%'),
            _ => {}
        }
    }
    out
}

/// placeholder characters before and after the decimal point
fn placeholders(tokens: &[Token]) -> (Vec<char>, Vec<char>) {
    let point = tokens
        .iter()
        .position(|t| *t == Token::DecimalPoint)
        .unwrap_or(tokens.len());
    let pick = |slice: &[Token]| {
        slice
            .iter()
            .filter_map(|t| {
                if let Token::Digit(c) = t {
                    Some(*c)
                } else {
                    None
                }
            })
            .collect()
    };
    (
        pick(&tokens[..point]),
        pick(tokens.get(point + 1..).unwrap_or(&[])),
    )
}

fn render_plain(value: f64, tokens: &[Token], locale: &Locale) -> String {
    let (whole_places, decimal_places) = placeholders(tokens);
    let rounded = round_digits(value.abs(), decimal_places.len() as f64, Rounding::Nearest);
    let text = format!("{:.*}", decimal_places.len(), rounded);
    let (whole, decimals) = text.split_once('.').unwrap_or((&text, ""));
    let whole = whole.trim_start_matches('0');
    let mut whole_chunks = place_whole(whole, &whole_places);
    if tokens.contains(&Token::Thousands) {
        // grouping works on the digits as one run, written at the first place
        let joined: String = whole_chunks.concat();
        whole_chunks = vec![String::new(); whole_places.len()];
        if let Some(first) = whole_chunks.first_mut() {
            *first = group(&joined, locale.thousands);
        }
    }
    let decimal_chunks = place_decimals(decimals, &decimal_places);
    // excel drops the minus when the number shows as zero: -1 as 0.0,,"M" is 0.0M
    let shows_zero = rounded == 0.0;
    assemble(
        tokens,
        &whole_chunks,
        &decimal_chunks,
        value < 0.0 && !shows_zero,
        locale,
    )
}

/// what each whole-part placeholder shows: digits fill from the right,
/// extra digits go with the first place, an empty '0' shows 0 and an
/// empty '?' a space
fn place_whole(digits: &str, places: &[char]) -> Vec<String> {
    let digits: Vec<char> = digits.chars().collect();
    let extra = digits.len().saturating_sub(places.len());
    let offset = places.len() as isize - digits.len() as isize;
    let mut chunks = Vec::with_capacity(places.len());
    for (index, place) in places.iter().enumerate() {
        let mut chunk = String::new();
        if index == 0 {
            chunk.extend(&digits[..extra]);
        }
        let digit_index = index as isize - offset;
        if digit_index >= 0 {
            chunk.push(digits[digit_index as usize]);
        } else if *place == '0' {
            chunk.push('0');
        } else if *place == '?' {
            chunk.push(' ');
        }
        chunks.push(chunk);
    }
    chunks
}

/// inserts the thousands separator into the digits (spaces from '?' stay in front)
fn group(digits: &str, separator: char) -> String {
    let lead: String = digits.chars().take_while(|c| !c.is_ascii_digit()).collect();
    let body: Vec<char> = digits.chars().skip(lead.chars().count()).collect();
    let mut out = String::new();
    for (index, c) in body.iter().enumerate() {
        if index > 0 && (body.len() - index).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(*c);
    }
    lead + &out
}

/// what each decimal placeholder shows: trailing zeros vanish under '#'
/// and become spaces under '?'
fn place_decimals(digits: &str, places: &[char]) -> Vec<String> {
    let digits: Vec<char> = digits.chars().collect();
    let mut keep = digits.len();
    while keep > 0 && digits[keep - 1] == '0' && places[keep - 1] != '0' {
        keep -= 1;
    }
    let mut chunks = Vec::with_capacity(places.len());
    for (index, place) in places.iter().enumerate() {
        let chunk = if index < keep {
            digits[index].to_string()
        } else if *place == '?' {
            " ".to_string()
        } else {
            String::new()
        };
        chunks.push(chunk);
    }
    chunks
}

/// writes the section's tokens with the number's pieces in place
fn assemble(
    tokens: &[Token],
    whole: &[String],
    decimals: &[String],
    negative: bool,
    locale: &Locale,
) -> String {
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    let mut whole_index = 0;
    let mut decimal_index = 0;
    let mut after_point = false;
    for token in tokens {
        match token {
            Token::Digit(_) if !after_point => {
                out.push_str(whole.get(whole_index).map(String::as_str).unwrap_or(""));
                whole_index += 1;
            }
            Token::Digit(_) => {
                out.push_str(
                    decimals
                        .get(decimal_index)
                        .map(String::as_str)
                        .unwrap_or(""),
                );
                decimal_index += 1;
            }
            Token::DecimalPoint => {
                out.push(locale.decimal);
                after_point = true;
            }
            Token::Literal(text) => out.push_str(text),
            Token::Space => out.push(' '),
            Token::Percent => out.push('%'),
            // a fill repeats to the cell's width, which only the grid
            // knows; as text it shows nothing (libreoffice's csv does the same)
            _ => {}
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;

    fn show(n: f64, code: &str) -> String {
        format_number(n, code, &Locale::default(), DateSystem::Excel1900).text
    }

    #[test]
    fn numbers_with_placeholders() {
        assert_eq!(show(1234.567, "#,##0.00"), "1,234.57");
        assert_eq!(show(0.5, "#.00"), ".50");
        assert_eq!(show(0.0, "#,##0"), "0");
        assert_eq!(show(5.0, "000"), "005");
        assert_eq!(show(1.5, "0.0#"), "1.5");
        assert_eq!(show(1.5, "0.0?"), "1.5 ");
        assert_eq!(show(5551234.0, "000-0000"), "555-1234");
        assert_eq!(show(-1234.0, "#,##0;(#,##0)"), "(1,234)");
        assert_eq!(show(0.0, "0;-0;\"zero\""), "zero");
        assert_eq!(show(0.256, "0.0%"), "25.6%");
        assert_eq!(show(1_234_567.0, "#,##0,\"K\""), "1,235K");
        assert_eq!(show(1_234_567.0, "0.0,,\"M\""), "1.2M");
        assert_eq!(show(-5.0, "0"), "-5");
        assert_eq!(show(2.675, "0.00"), "2.68");
        assert_eq!(show(1234.5, "$#,##0.00"), "$1,234.50");
        assert_eq!(show(1.0 / 3.0, "General"), "0.333333333");
    }

    #[test]
    fn colours_conditions_and_text() {
        let red = format_number(-1.0, "0;[Red]-0", &Locale::default(), DateSystem::Excel1900);
        assert_eq!(
            (red.text.as_str(), red.color.as_deref()),
            ("-1", Some("Red"))
        );
        assert_eq!(show(150.0, "[>100]\"big\";\"small\""), "big");
        assert_eq!(show(50.0, "[>100]\"big\";\"small\""), "small");
        let text = format_value(
            &Value::text("x"),
            "0;0;0;\"<\"@\">\"",
            &Locale::default(),
            DateSystem::Excel1900,
        );
        assert_eq!(text.text, "<x>");
        let euro = Locale {
            decimal: ',',
            thousands: '.',
        };
        assert_eq!(
            format_number(1234.5, "#,##0.00", &euro, DateSystem::Excel1900).text,
            "1.234,50"
        );
    }
}
