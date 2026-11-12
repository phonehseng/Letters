//! the standard field formats: numbers, percentages, dates, times and
//! special masks, as acrobat's built-in AFNumber_Format, AFPercent_Format,
//! AFDate_FormatEx, AFTime_Format and AFSpecial_Format describe them.
//!
//! two directions: `read_input` turns what a person typed into the raw
//! value stored in the field (or refuses it), and `display` turns a raw
//! value into what the field shows.

use serde::{Deserialize, Serialize};

use super::eval::{number_to_text, parse_number};
use crate::date::{format_date, parse_date};

use super::masks::{apply_mask, format_time, mask_example, parse_time};
use super::numbers::{example, group, read_number, round_to};
pub use super::masks::TIME_PATTERNS;

/// how digits are grouped and which mark starts the decimals
/// (AFNumber_Format's sepStyle, 0 to 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SeparatorStyle {
    /// 1,234.56
    #[default]
    CommaDot,
    /// 1234.56
    Dot,
    /// 1.234,56
    DotComma,
    /// 1234,56
    Comma,
    /// 1'234.56
    ApostropheDot,
}

impl SeparatorStyle {
    pub fn from_code(code: i64) -> Self {
        match code {
            1 => SeparatorStyle::Dot,
            2 => SeparatorStyle::DotComma,
            3 => SeparatorStyle::Comma,
            4 => SeparatorStyle::ApostropheDot,
            _ => SeparatorStyle::CommaDot,
        }
    }

    pub fn code(self) -> i64 {
        match self {
            SeparatorStyle::CommaDot => 0,
            SeparatorStyle::Dot => 1,
            SeparatorStyle::DotComma => 2,
            SeparatorStyle::Comma => 3,
            SeparatorStyle::ApostropheDot => 4,
        }
    }

    pub(super) fn grouping(self) -> Option<char> {
        match self {
            SeparatorStyle::CommaDot => Some(','),
            SeparatorStyle::DotComma => Some('.'),
            SeparatorStyle::ApostropheDot => Some('\''),
            SeparatorStyle::Dot | SeparatorStyle::Comma => None,
        }
    }

    pub(super) fn decimal_mark(self) -> char {
        match self {
            SeparatorStyle::DotComma | SeparatorStyle::Comma => ',',
            _ => '.',
        }
    }
}

/// how negative numbers are shown (AFNumber_Format's negStyle).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NegativeStyle {
    /// -1,234.56
    #[default]
    MinusSign,
    /// 1,234.56 in red
    Red,
    /// (1,234.56)
    Parentheses,
    /// (1,234.56) in red
    RedParentheses,
}

impl NegativeStyle {
    pub fn from_code(code: i64) -> Self {
        match code {
            1 => NegativeStyle::Red,
            2 => NegativeStyle::Parentheses,
            3 => NegativeStyle::RedParentheses,
            _ => NegativeStyle::MinusSign,
        }
    }

    pub fn code(self) -> i64 {
        match self {
            NegativeStyle::MinusSign => 0,
            NegativeStyle::Red => 1,
            NegativeStyle::Parentheses => 2,
            NegativeStyle::RedParentheses => 3,
        }
    }
}

/// the four masks AFSpecial_Format knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpecialFormat {
    /// 12345
    ZipCode,
    /// 12345-6789
    ZipPlus4,
    /// (123) 456-7890, or 456-7890
    Phone,
    /// 123-45-6789
    SocialSecurity,
}

impl SpecialFormat {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => SpecialFormat::ZipCode,
            1 => SpecialFormat::ZipPlus4,
            2 => SpecialFormat::Phone,
            3 => SpecialFormat::SocialSecurity,
            _ => return None,
        })
    }

    pub fn code(self) -> i64 {
        match self {
            SpecialFormat::ZipCode => 0,
            SpecialFormat::ZipPlus4 => 1,
            SpecialFormat::Phone => 2,
            SpecialFormat::SocialSecurity => 3,
        }
    }

    fn masks(self) -> &'static [&'static str] {
        match self {
            SpecialFormat::ZipCode => &["99999"],
            SpecialFormat::ZipPlus4 => &["99999-9999"],
            SpecialFormat::Phone => &["(999) 999-9999", "999-9999"],
            SpecialFormat::SocialSecurity => &["999-99-9999"],
        }
    }

    fn describe(self) -> &'static str {
        match self {
            SpecialFormat::ZipCode => "a 5 digit ZIP code",
            SpecialFormat::ZipPlus4 => "a ZIP+4 code like 12345-6789",
            SpecialFormat::Phone => "a phone number like (123) 456-7890",
            SpecialFormat::SocialSecurity => "a number like 123-45-6789",
        }
    }
}

/// how a field shows its value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FieldFormat {
    Number {
        decimals: u32,
        #[serde(default)]
        separator: SeparatorStyle,
        #[serde(default)]
        negative: NegativeStyle,
        /// such as "$" or " €"; empty for none
        #[serde(default)]
        currency: String,
        /// true puts the currency before the number
        #[serde(default)]
        currency_before: bool,
    },
    /// the stored value is a fraction: 0.15 shows as 15.00%
    Percent {
        decimals: u32,
        #[serde(default)]
        separator: SeparatorStyle,
    },
    /// a date pattern such as "dd/mm/yyyy"
    Date {
        pattern: String,
    },
    /// "HH:MM", "h:MM tt", "HH:MM:ss" or "h:MM:ss tt"
    Time {
        pattern: String,
    },
    Special {
        kind: SpecialFormat,
    },
    /// an arbitrary mask: 9 a digit, A a letter, O a letter or digit,
    /// X any character, anything else itself (AFSpecial_KeystrokeEx)
    Mask {
        mask: String,
    },
}

/// what a field shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub text: String,
    /// negative numbers in a red style
    pub red: bool,
}

/// turns typed input into the stored value, or a sentence saying why not.
pub fn read_input(format: &FieldFormat, field: &str, typed: &str) -> Result<String, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Ok(String::new());
    }
    match format {
        FieldFormat::Number {
            separator,
            currency,
            ..
        } => read_number(typed, *separator, currency)
            .map(number_to_text)
            .ok_or_else(|| format!("“{field}” takes a number, such as {}.", example(*separator))),
        FieldFormat::Percent { separator, .. } => {
            let (text, fraction) = match typed.strip_suffix('%') {
                Some(rest) => (rest.trim(), true),
                None => (typed, false),
            };
            let number = read_number(text, *separator, "")
                .ok_or_else(|| format!("“{field}” takes a percentage, such as 15%."))?;
            Ok(number_to_text(if fraction {
                number / 100.0
            } else {
                number
            }))
        }
        FieldFormat::Date { pattern } => parse_date(typed, pattern)
            .map(|date| format_date(date, pattern))
            .map_err(|error| error.to_string()),
        FieldFormat::Time { pattern } => parse_time(typed)
            .map(|time| format_time(time, pattern))
            .ok_or_else(|| {
                format!(
                    "“{typed}” isn’t a time Letters understands. Type it like 14:30 or 2:30 pm."
                )
            }),
        FieldFormat::Special { kind } => {
            let digits: String = typed.chars().filter(char::is_ascii_digit).collect();
            let fits = kind.masks().iter().any(|mask| {
                mask.chars().filter(|c| *c == '9').count() == digits.len()
                    && apply_mask(mask, typed).is_some()
            });
            if fits {
                Ok(digits)
            } else {
                Err(format!("“{field}” takes {}.", kind.describe()))
            }
        }
        FieldFormat::Mask { mask } => apply_mask(mask, typed)
            .ok_or_else(|| format!("“{field}” must be typed like {}.", mask_example(mask))),
    }
}

/// what a stored value looks like in the field.
pub fn display(format: &FieldFormat, raw: &str) -> Shown {
    let plain = || Shown {
        text: raw.to_owned(),
        red: false,
    };
    if raw.trim().is_empty() {
        return plain();
    }
    match format {
        FieldFormat::Number {
            decimals,
            separator,
            negative,
            currency,
            currency_before,
        } => {
            let Some(value) = read_number(raw.trim(), SeparatorStyle::Dot, currency)
                .or_else(|| read_number(raw.trim(), *separator, currency))
            else {
                return plain();
            };
            let rounded = round_to(value, *decimals);
            let digits = group(rounded.abs(), *decimals, *separator);
            let body = if currency.is_empty() {
                digits
            } else if *currency_before {
                format!("{currency}{digits}")
            } else {
                format!("{digits}{currency}")
            };
            let negative_value = rounded < 0.0;
            let text = match (negative_value, negative) {
                (false, _) => body,
                (true, NegativeStyle::MinusSign) => format!("-{body}"),
                (true, NegativeStyle::Red) => body,
                (true, NegativeStyle::Parentheses | NegativeStyle::RedParentheses) => {
                    format!("({body})")
                }
            };
            Shown {
                text,
                red: negative_value
                    && matches!(negative, NegativeStyle::Red | NegativeStyle::RedParentheses),
            }
        }
        FieldFormat::Percent {
            decimals,
            separator,
        } => match parse_number(raw.trim()) {
            Some(value) => {
                let percent = round_to(value * 100.0, *decimals);
                let sign = if percent < 0.0 { "-" } else { "" };
                Shown {
                    text: format!("{sign}{}%", group(percent.abs(), *decimals, *separator)),
                    red: false,
                }
            }
            None => plain(),
        },
        FieldFormat::Special { kind } => {
            let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
            let mask = kind
                .masks()
                .iter()
                .find(|mask| mask.chars().filter(|c| *c == '9').count() == digits.len());
            match mask.and_then(|mask| apply_mask(mask, &digits)) {
                Some(text) => Shown { text, red: false },
                None => plain(),
            }
        }
        // dates, times and masks are stored the way they are shown
        FieldFormat::Date { .. } | FieldFormat::Time { .. } | FieldFormat::Mask { .. } => plain(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(separator: SeparatorStyle, negative: NegativeStyle) -> FieldFormat {
        FieldFormat::Number {
            decimals: 2,
            separator,
            negative,
            currency: "$".into(),
            currency_before: true,
        }
    }

    #[test]
    fn numbers_show_in_each_style() {
        let cases = [
            (SeparatorStyle::CommaDot, "1234567.891", "$1,234,567.89"),
            (SeparatorStyle::Dot, "1234567.891", "$1234567.89"),
            (SeparatorStyle::DotComma, "1234567.891", "$1.234.567,89"),
            (SeparatorStyle::Comma, "1234567.891", "$1234567,89"),
            (
                SeparatorStyle::ApostropheDot,
                "1234567.891",
                "$1'234'567.89",
            ),
            (SeparatorStyle::CommaDot, "1.005", "$1.01"),
            (SeparatorStyle::CommaDot, "999.999", "$1,000.00"),
        ];
        for (style, raw, shown) in cases {
            assert_eq!(
                display(&money(style, NegativeStyle::MinusSign), raw).text,
                shown
            );
        }
        let negative = |style| display(&money(SeparatorStyle::CommaDot, style), "-1234.5");
        assert_eq!(negative(NegativeStyle::MinusSign).text, "-$1,234.50");
        assert_eq!(negative(NegativeStyle::Parentheses).text, "($1,234.50)");
        assert!(negative(NegativeStyle::Red).red);
        assert_eq!(negative(NegativeStyle::Red).text, "$1,234.50");
        assert!(negative(NegativeStyle::RedParentheses).red);
        let euro = FieldFormat::Number {
            decimals: 0,
            separator: SeparatorStyle::DotComma,
            negative: NegativeStyle::MinusSign,
            currency: " €".into(),
            currency_before: false,
        };
        assert_eq!(display(&euro, "1500").text, "1.500 €");
    }

    #[test]
    fn typed_numbers_are_read_in_the_fields_own_style() {
        let comma = money(SeparatorStyle::DotComma, NegativeStyle::MinusSign);
        assert_eq!(read_input(&comma, "x", "1.234,5").unwrap(), "1234.5");
        assert_eq!(read_input(&comma, "x", "12,5").unwrap(), "12.5");
        assert_eq!(read_input(&comma, "x", "$ 7").unwrap(), "7");
        let dot = money(SeparatorStyle::CommaDot, NegativeStyle::MinusSign);
        assert_eq!(read_input(&dot, "x", "1,234.5").unwrap(), "1234.5");
        assert_eq!(read_input(&dot, "x", "(12)").unwrap(), "-12");
        assert_eq!(read_input(&dot, "x", "-$3.25").unwrap(), "-3.25");
        let refused = read_input(&dot, "Price", "twelve").unwrap_err();
        assert_eq!(refused, "“Price” takes a number, such as 1,234.50.");
    }

    #[test]
    fn percentages_store_fractions() {
        let percent = FieldFormat::Percent {
            decimals: 1,
            separator: SeparatorStyle::CommaDot,
        };
        assert_eq!(read_input(&percent, "x", "15%").unwrap(), "0.15");
        assert_eq!(read_input(&percent, "x", "0.15").unwrap(), "0.15");
        assert_eq!(display(&percent, "0.155").text, "15.5%");
        assert_eq!(display(&percent, "12.5").text, "1,250.0%");
    }

    #[test]
    fn times_read_and_show() {
        let time = parse_time("2:05 pm").unwrap();
        assert_eq!(format_time(time, "HH:MM"), "14:05");
        assert_eq!(format_time(time, "h:MM tt"), "2:05 pm");
        assert_eq!(
            format_time(parse_time("00:30:09").unwrap(), "h:MM:ss tt"),
            "12:30:09 am"
        );
        assert!(parse_time("25:00").is_none());
        assert!(parse_time("13 pm").is_none());
        assert!(parse_time("noon").is_none());
    }

    #[test]
    fn special_formats_and_masks() {
        let phone = FieldFormat::Special {
            kind: SpecialFormat::Phone,
        };
        assert_eq!(
            read_input(&phone, "x", "(021) 555-1234").unwrap(),
            "0215551234"
        );
        assert_eq!(display(&phone, "0215551234").text, "(021) 555-1234");
        assert_eq!(display(&phone, "5551234").text, "555-1234");
        assert!(read_input(&phone, "x", "12345").is_err());
        let ssn = FieldFormat::Special {
            kind: SpecialFormat::SocialSecurity,
        };
        assert_eq!(display(&ssn, "123456789").text, "123-45-6789");
        let zip = FieldFormat::Special {
            kind: SpecialFormat::ZipCode,
        };
        assert!(read_input(&zip, "x", "1234a").is_err());
        let mask = FieldFormat::Mask {
            mask: "AA-9999".into(),
        };
        assert_eq!(read_input(&mask, "x", "AB-1234").unwrap(), "AB-1234");
        assert_eq!(read_input(&mask, "x", "ab1234").unwrap(), "ab-1234");
        assert_eq!(
            read_input(&mask, "Code", "1234").unwrap_err(),
            "“Code” must be typed like aa-1111."
        );
    }
}
