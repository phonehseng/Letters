//! the conditions COUNTIF, SUMIF and their relatives take: ">=10",
//! "<>apple", "a*", "=", 42. behaviour checked against excel: text
//! matching ignores case, "*" and "?" are wildcards ("~" escapes them),
//! a number condition also matches text that reads as that number, ""
//! matches empty cells and "<>" matches every cell that is not empty.

use std::cmp::Ordering;

use crate::model::{DateSystem, Value};

use super::text_numbers::parse_number;
use super::value::{compare_numbers, compare_text};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Clone, Debug, PartialEq)]
enum Operand {
    Number(f64),
    Text(String),
    Bool(bool),
    Error(crate::model::ErrorValue),
    /// "=" or "" with nothing after it
    Empty,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Criterion {
    comparison: Comparison,
    operand: Operand,
    /// the bare criterion was "" (matches blanks and empty text)
    bare_empty: bool,
}

impl Criterion {
    pub fn new(value: &Value, system: DateSystem) -> Criterion {
        match value {
            Value::Number(n) => Criterion::equal(Operand::Number(*n)),
            Value::Bool(b) => Criterion::equal(Operand::Bool(*b)),
            Value::Error(e) => Criterion::equal(Operand::Error(*e)),
            Value::Blank => Criterion {
                bare_empty: true,
                ..Criterion::equal(Operand::Empty)
            },
            Value::Text(text) => Criterion::parse(text, system),
        }
    }

    fn equal(operand: Operand) -> Criterion {
        Criterion {
            comparison: Comparison::Equal,
            operand,
            bare_empty: false,
        }
    }

    fn parse(text: &str, system: DateSystem) -> Criterion {
        let (comparison, rest) = split_operator(text);
        let operand = if rest.is_empty() {
            Operand::Empty
        } else if let Some(n) = parse_number(rest, system) {
            Operand::Number(n)
        } else if rest.eq_ignore_ascii_case("TRUE") || rest.eq_ignore_ascii_case("FALSE") {
            Operand::Bool(rest.eq_ignore_ascii_case("TRUE"))
        } else if let Some(error) = crate::model::ErrorValue::parse(rest) {
            Operand::Error(error)
        } else {
            Operand::Text(rest.to_string())
        };
        Criterion {
            comparison,
            operand,
            bare_empty: text.is_empty(),
        }
    }

    pub fn matches(&self, value: &Value, system: DateSystem) -> bool {
        match (&self.operand, self.comparison) {
            (Operand::Empty, Comparison::Equal) => {
                is_empty(value) && (self.bare_empty || value.is_blank())
            }
            (Operand::Empty, Comparison::NotEqual) => !value.is_blank(),
            (Operand::Empty, _) => false,
            (Operand::Number(target), comparison) => match number_of(value, system, comparison) {
                Some(n) => holds(comparison, compare_numbers(n, *target)),
                None => comparison == Comparison::NotEqual,
            },
            (Operand::Text(pattern), Comparison::Equal) => {
                text_of(value).is_some_and(|t| wildcard_match(pattern, &t))
            }
            (Operand::Text(pattern), Comparison::NotEqual) => {
                !text_of(value).is_some_and(|t| wildcard_match(pattern, &t))
            }
            (Operand::Text(target), comparison) => match value {
                Value::Text(text) => holds(comparison, compare_text(text, target)),
                _ => false,
            },
            (Operand::Bool(target), comparison) => match value {
                Value::Bool(b) => holds(comparison, b.cmp(target)),
                _ => comparison == Comparison::NotEqual,
            },
            (Operand::Error(target), comparison) => {
                let same = matches!(value, Value::Error(e) if e == target);
                if comparison == Comparison::NotEqual {
                    !same
                } else {
                    same && comparison == Comparison::Equal
                }
            }
        }
    }
}

fn split_operator(text: &str) -> (Comparison, &str) {
    for (prefix, comparison) in [
        ("<>", Comparison::NotEqual),
        ("<=", Comparison::LessEqual),
        (">=", Comparison::GreaterEqual),
        ("<", Comparison::Less),
        (">", Comparison::Greater),
        ("=", Comparison::Equal),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return (comparison, rest);
        }
    }
    (Comparison::Equal, text)
}

fn holds(comparison: Comparison, order: Ordering) -> bool {
    match comparison {
        Comparison::Equal => order == Ordering::Equal,
        Comparison::NotEqual => order != Ordering::Equal,
        Comparison::Less => order == Ordering::Less,
        Comparison::LessEqual => order != Ordering::Greater,
        Comparison::Greater => order == Ordering::Greater,
        Comparison::GreaterEqual => order != Ordering::Less,
    }
}

fn is_empty(value: &Value) -> bool {
    matches!(value, Value::Blank) || matches!(value, Value::Text(t) if t.is_empty())
}

/// a cell's number for a number condition: numbers always, and for "="
/// text that reads as the number ("1" matches 1)
fn number_of(value: &Value, system: DateSystem, comparison: Comparison) -> Option<f64> {
    match value {
        Value::Number(n) => Some(*n),
        Value::Text(text) if comparison == Comparison::Equal => parse_number(text, system),
        _ => None,
    }
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::Text(text) => Some(text.clone()),
        _ => None,
    }
}

/// "*" any run of characters, "?" one character, "~" makes the next one
/// plain; case is ignored
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    wildcard_match_with_case(pattern, text, true)
}

/// the same, with case mattering unless `ignore_case`
pub fn wildcard_match_with_case(pattern: &str, text: &str, ignore_case: bool) -> bool {
    // most lookups have no wildcard at all: compare without copying, as
    // a scan of 200,000 keys would otherwise copy each one twice
    if !pattern.contains(['*', '?', '~']) {
        return match ignore_case {
            false => pattern == text,
            true if pattern.is_ascii() && text.is_ascii() => pattern.eq_ignore_ascii_case(text),
            true => pattern.to_lowercase() == text.to_lowercase(),
        };
    }
    let fold = |s: &str| {
        if ignore_case {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let pattern: Vec<char> = fold(pattern).chars().collect();
    let text: Vec<char> = fold(text).chars().collect();
    matches_from(&pattern, &text)
}

fn matches_from(pattern: &[char], text: &[char]) -> bool {
    // iterative matching with one backtrack point for the last "*"
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('~') if p + 1 < pattern.len() && pattern[p + 1] == text[t] => {
                p += 2;
                t += 1;
            }
            Some('?') => {
                p += 1;
                t += 1;
            }
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(c) if *c == text[t] && *c != '~' => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, star_t + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(criterion: Value, value: Value) -> bool {
        Criterion::new(&criterion, DateSystem::Excel1900).matches(&value, DateSystem::Excel1900)
    }

    #[test]
    fn conditions_match_like_excel() {
        assert!(check(Value::text(">=10"), Value::Number(10.0)));
        assert!(!check(Value::text(">=10"), Value::text("20")));
        assert!(check(Value::Number(1.0), Value::text("1")));
        assert!(check(Value::text("app*"), Value::text("Apple")));
        assert!(check(Value::text("?b"), Value::text("ab")));
        assert!(!check(Value::text("?b"), Value::text("abb")));
        assert!(check(Value::text("~*"), Value::text("*")));
        assert!(!check(Value::text("~*"), Value::text("a")));
        assert!(check(Value::text("<>apple"), Value::Blank));
        assert!(check(Value::text(""), Value::Blank));
        assert!(check(Value::text(""), Value::text("")));
        assert!(check(Value::text("="), Value::Blank));
        assert!(!check(Value::text("="), Value::text("")));
        assert!(check(Value::text("<>"), Value::text("x")));
        assert!(!check(Value::text("<>"), Value::Blank));
        assert!(check(Value::text("<b"), Value::text("a")));
        assert!(!check(Value::text("<b"), Value::Number(1.0)));
        assert!(check(Value::Bool(true), Value::Bool(true)));
    }
}
