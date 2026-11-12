//! text functions: joining, cutting, searching, replacing, case,
//! spaces, TEXT with number formats, VALUE, EXACT, REPT, CHAR and CODE.
//! positions count characters from 1, as in excel.

use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::criteria::wildcard_match;
use super::super::eval::Evaluator;
use super::super::value::{to_number, to_text, CalcValue};
use super::args::*;

pub const NAMES: &[&str] = &[
    "CONCAT",
    "CONCATENATE",
    "TEXTJOIN",
    "LEFT",
    "RIGHT",
    "MID",
    "LEN",
    "FIND",
    "SEARCH",
    "SUBSTITUTE",
    "REPLACE",
    "UPPER",
    "LOWER",
    "PROPER",
    "TRIM",
    "TEXT",
    "VALUE",
    "EXACT",
    "REPT",
    "CHAR",
    "CODE",
    "CLEAN",
    "T",
    "UNICHAR",
    "UNICODE",
    "FIXED",
    "DOLLAR",
    "NUMBERVALUE",
];

/// a text function given its arguments as text and as values
type TextFunction<'f> = &'f dyn Fn(&[String], &[Value]) -> Result<Value, ErrorValue>;

/// the longest text a cell can hold in excel
const MAX_TEXT: usize = 32_767;

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    let system = ev.system();
    let texts = |f: TextFunction| {
        lift(ev, args, &move |values| {
            let strings: Result<Vec<String>, ErrorValue> = values.iter().map(to_text).collect();
            strings
                .and_then(|s| f(&s, values))
                .unwrap_or_else(Value::Error)
        })
    };
    Some(match name {
        "CONCAT" | "CONCATENATE" => done(concat(ev, args, name == "CONCATENATE")),
        "TEXTJOIN" => done(text_join(ev, args)),
        "LEFT" => texts(&|s, v| cut(&s[0], 0, count_arg(v.get(1), 1)?)),
        "RIGHT" => texts(&|s, v| {
            let take = count_arg(v.get(1), 1)?;
            let length = s[0].chars().count();
            cut(&s[0], length.saturating_sub(take), take)
        }),
        "MID" => texts(&|s, v| {
            let start = position_arg(v.get(1))?;
            cut(&s[0], start - 1, count_arg(v.get(2), 0)?)
        }),
        "LEN" => texts(&|s, _| Ok(Value::Number(s[0].chars().count() as f64))),
        "FIND" => texts(&|s, v| locate(&s[0], &s[1], v.get(2), false)),
        "SEARCH" => texts(&|s, v| locate(&s[0], &s[1], v.get(2), true)),
        "SUBSTITUTE" => texts(&|s, v| substitute(&s[0], &s[1], &s[2], v.get(3))),
        "REPLACE" => texts(&|s, v| {
            let start = position_arg(v.get(1))? - 1;
            let count = count_arg(v.get(2), 0)?;
            let chars: Vec<char> = s[0].chars().collect();
            let head: String = chars.iter().take(start).collect();
            let tail: String = chars.iter().skip(start + count).collect();
            Ok(Value::Text(format!("{head}{}{tail}", s[3])))
        }),
        "UPPER" => texts(&|s, _| Ok(Value::Text(s[0].to_uppercase()))),
        "LOWER" => texts(&|s, _| Ok(Value::Text(s[0].to_lowercase()))),
        "PROPER" => texts(&|s, _| Ok(Value::Text(proper(&s[0])))),
        "TRIM" => texts(&|s, _| {
            Ok(Value::Text(
                s[0].split(' ')
                    .filter(|w| !w.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            ))
        }),
        "CLEAN" => texts(&|s, _| {
            Ok(Value::Text(
                s[0].chars().filter(|c| *c as u32 >= 32).collect(),
            ))
        }),
        "EXACT" => texts(&|s, _| Ok(Value::Bool(s[0] == s[1]))),
        "REPT" => texts(&|s, v| repeat(&s[0], v.get(1))),
        "T" => lift(ev, args, &|v| match &v[0] {
            Value::Text(t) => Value::Text(t.clone()),
            Value::Error(e) => Value::Error(*e),
            _ => Value::Text(String::new()),
        }),
        "CHAR" | "UNICHAR" => {
            let unicode = name == "UNICHAR";
            lift(ev, args, &move |v| match to_number(&v[0], system) {
                Ok(n) => character(n, unicode).unwrap_or_else(Value::Error),
                Err(e) => Value::Error(e),
            })
        }
        "CODE" | "UNICODE" => texts(&|s, _| {
            let first = s[0].chars().next().ok_or(ErrorValue::Value)?;
            Ok(Value::Number(f64::from(u32::from(first))))
        }),
        "TEXT" => lift(ev, args, &|v| text_format(&v[0], &v[1], system)),
        "VALUE" | "NUMBERVALUE" => lift(ev, args, &|v| match &v[0] {
            Value::Number(n) => Value::Number(*n),
            Value::Error(e) => Value::Error(*e),
            // excel: VALUE(TRUE) is #VALUE!, a logical value is not text
            Value::Bool(_) => Value::Error(ErrorValue::Value),
            other => to_number(other, system)
                .map(Value::Number)
                .unwrap_or(Value::Error(ErrorValue::Value)),
        }),
        "FIXED" => lift(ev, args, &|v| fixed(v, system, false)),
        "DOLLAR" => lift(ev, args, &|v| fixed(v, system, true)),
        _ => return None,
    })
}

/// CHAR(65) is "A"; codes 1-255 follow windows-1252 like excel on windows
fn character(n: f64, unicode: bool) -> Result<Value, ErrorValue> {
    let code = n.trunc() as u32;
    if code < 1 || (!unicode && code > 255) {
        return Err(ErrorValue::Value);
    }
    let c = if unicode {
        char::from_u32(code)
    } else {
        windows_1252(code as u8)
    };
    c.map(|c| Value::Text(c.to_string()))
        .ok_or(ErrorValue::Value)
}

/// the windows-1252 characters for 128-159, the only ones that differ from latin-1
pub fn windows_1252(byte: u8) -> Option<char> {
    const HIGH: [u32; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013,
        0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    match byte {
        128..=159 => char::from_u32(HIGH[usize::from(byte - 128)]),
        other => Some(char::from(other)),
    }
}

/// a character count argument: missing means `default`, negative is #VALUE!
fn count_arg(value: Option<&Value>, default: usize) -> Result<usize, ErrorValue> {
    match value {
        None => Ok(default),
        Some(v) => {
            let n = to_number(v, crate::model::DateSystem::Excel1900)?;
            if n < 0.0 {
                Err(ErrorValue::Value)
            } else {
                Ok(n.trunc() as usize)
            }
        }
    }
}

/// a starting position: 1 or more
fn position_arg(value: Option<&Value>) -> Result<usize, ErrorValue> {
    let n = count_arg(value, 1)?;
    if n < 1 {
        return Err(ErrorValue::Value);
    }
    Ok(n)
}

fn cut(text: &str, skip: usize, take: usize) -> Result<Value, ErrorValue> {
    Ok(Value::Text(text.chars().skip(skip).take(take).collect()))
}

/// FIND (case and wildcards matter not: exact) and SEARCH (ignores case,
/// understands * ? ~); the position of `needle` in `haystack` from `start`
fn locate(
    needle: &str,
    haystack: &str,
    start: Option<&Value>,
    search: bool,
) -> Result<Value, ErrorValue> {
    let start = position_arg(start)?;
    let hay: Vec<char> = haystack.chars().collect();
    if start > hay.len() + 1 {
        return Err(ErrorValue::Value);
    }
    if needle.is_empty() {
        return Ok(Value::Number(start as f64));
    }
    for begin in (start - 1)..hay.len() {
        let rest: String = hay[begin..].iter().collect();
        let found = if search {
            (begin..=hay.len())
                .any(|end| wildcard_match(needle, &hay[begin..end].iter().collect::<String>()))
        } else {
            rest.starts_with(needle)
        };
        if found {
            return Ok(Value::Number(begin as f64 + 1.0));
        }
    }
    Err(ErrorValue::Value)
}

/// SUBSTITUTE(text, old, new, [instance])
fn substitute(
    text: &str,
    old: &str,
    new: &str,
    instance: Option<&Value>,
) -> Result<Value, ErrorValue> {
    if old.is_empty() {
        return Ok(Value::Text(text.to_string()));
    }
    let Some(instance) = instance else {
        return Ok(Value::Text(text.replace(old, new)));
    };
    let which = position_arg(Some(instance))?;
    let mut out = String::new();
    let mut rest = text;
    let mut seen = 0;
    while let Some(index) = rest.find(old) {
        seen += 1;
        out.push_str(&rest[..index]);
        out.push_str(if seen == which { new } else { old });
        rest = &rest[index + old.len()..];
    }
    out.push_str(rest);
    Ok(Value::Text(out))
}

/// capitals after anything that is not a letter, as excel's PROPER does
fn proper(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_letter = false;
    for c in text.chars() {
        if after_letter {
            out.extend(c.to_lowercase());
        } else {
            out.extend(c.to_uppercase());
        }
        after_letter = c.is_alphabetic();
    }
    out
}

fn repeat(text: &str, times: Option<&Value>) -> Result<Value, ErrorValue> {
    let times = count_arg(times, 0)?;
    if text.chars().count() * times > MAX_TEXT {
        return Err(ErrorValue::Value);
    }
    Ok(Value::Text(text.repeat(times)))
}

/// CONCAT joins every cell of its ranges; CONCATENATE takes single values
fn concat(ev: &Evaluator, args: &[Expr], single_values_only: bool) -> Outcome<Value> {
    let mut out = String::new();
    for arg in args {
        let pieces = if single_values_only {
            vec![ev.scalar(arg)]
        } else {
            grid(ev, arg)?.items
        };
        for piece in pieces {
            out.push_str(&to_text(&piece)?);
        }
    }
    if out.chars().count() > MAX_TEXT {
        return Err(ErrorValue::Value);
    }
    Ok(Value::Text(out))
}

/// TEXTJOIN(delimiter, ignore_empty, text1, ...)
fn text_join(ev: &Evaluator, args: &[Expr]) -> Outcome<Value> {
    if args.len() < 3 {
        return Err(ErrorValue::Value);
    }
    let delimiter = text(ev, &args[0])?;
    let ignore_empty = boolean(ev, &args[1])?;
    let mut pieces = Vec::new();
    for arg in &args[2..] {
        for value in grid(ev, arg)?.items {
            let piece = to_text(&value)?;
            if !(ignore_empty && piece.is_empty()) {
                pieces.push(piece);
            }
        }
    }
    Ok(Value::Text(pieces.join(&delimiter)))
}

/// TEXT(value, format): the value shown with an excel format code
fn text_format(value: &Value, format: &Value, system: crate::model::DateSystem) -> Value {
    let code = match to_text(format) {
        Ok(code) => code,
        Err(e) => return Value::Error(e),
    };
    let locale = crate::numfmt::Locale::default();
    // a date format cannot show a negative number; excel's TEXT says #VALUE!
    let is_date = crate::numfmt::parse::parse(&code)
        .first()
        .is_some_and(|s| s.is_date());
    if is_date && matches!(value, Value::Number(n) if *n < 0.0) {
        return Value::Error(ErrorValue::Value);
    }
    let shown = match value {
        Value::Error(e) => return Value::Error(*e),
        Value::Text(text) => match to_number(&Value::Text(text.clone()), system) {
            Ok(n) => crate::numfmt::format_number(n, &code, &locale, system),
            Err(_) => crate::numfmt::format_value(value, &code, &locale, system),
        },
        Value::Blank => crate::numfmt::format_number(0.0, &code, &locale, system),
        other => crate::numfmt::format_value(other, &code, &locale, system),
    };
    Value::Text(shown.text)
}

/// FIXED(number, [decimals], [no_commas]) and DOLLAR(number, [decimals])
fn fixed(values: &[Value], system: crate::model::DateSystem, currency: bool) -> Value {
    let number = match to_number(&values[0], system) {
        Ok(n) => n,
        Err(e) => return Value::Error(e),
    };
    let decimals = values
        .get(1)
        .map(|v| to_number(v, system))
        .unwrap_or(Ok(2.0));
    let Ok(decimals) = decimals else {
        return Value::Error(ErrorValue::Value);
    };
    let no_commas = !currency
        && values.get(2).is_some_and(|v| {
            matches!(v, Value::Bool(true)) || matches!(v, Value::Number(n) if *n != 0.0)
        });
    let decimals = decimals.clamp(-127.0, 127.0) as i32;
    let rounded =
        super::math::round_digits(number, f64::from(decimals), super::math::Rounding::Nearest);
    let places = "0".repeat(decimals.max(0) as usize);
    let whole = if no_commas { "0" } else { "#,##0" };
    let pattern = if places.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{places}")
    };
    let code = if currency {
        format!("\"$\"{pattern};(\"$\"{pattern})")
    } else {
        pattern
    };
    let locale = crate::numfmt::Locale::default();
    Value::Text(crate::numfmt::format_number(rounded, &code, &locale, system).text)
}
