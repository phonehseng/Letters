//! masks (AFSpecial_KeystrokeEx: 9 a digit, A a letter, O either, X any)
//! and times of day (AFTime_Format), read from typing and written out.

/// the patterns AFTime_Format(0..3) stands for.
pub const TIME_PATTERNS: [&str; 4] = ["HH:MM", "h:MM tt", "HH:MM:ss", "h:MM:ss tt"];

/// fits typed text to a mask: either it already matches with the mask's
/// own characters, or it matches once they are put in.
pub(super) fn apply_mask(mask: &str, typed: &str) -> Option<String> {
    let slot = |placeholder: char, c: char| match placeholder {
        '9' => c.is_ascii_digit(),
        'A' => c.is_alphabetic(),
        'O' => c.is_alphanumeric(),
        'X' => true,
        _ => false,
    };
    let is_slot = |c: char| matches!(c, '9' | 'A' | 'O' | 'X');
    let mask_chars: Vec<char> = mask.chars().collect();
    let typed_chars: Vec<char> = typed.chars().collect();
    // exactly as the mask, literal characters included
    if mask_chars.len() == typed_chars.len()
        && mask_chars
            .iter()
            .zip(&typed_chars)
            .all(|(m, c)| if is_slot(*m) { slot(*m, *c) } else { m == c })
    {
        return Some(typed.to_owned());
    }
    // only the characters for the slots, with the literals put in
    // (spaces and punctuation typed by the person are dropped here)
    let wanted: Vec<char> = typed_chars
        .iter()
        .copied()
        .filter(|c| c.is_alphanumeric())
        .collect();
    let slots = mask_chars.iter().filter(|m| is_slot(**m)).count();
    if wanted.len() != slots {
        return None;
    }
    let mut out = String::new();
    let mut next = wanted.iter();
    for m in &mask_chars {
        if is_slot(*m) {
            let c = *next.next()?;
            if !slot(*m, c) {
                return None;
            }
            out.push(c);
        } else {
            out.push(*m);
        }
    }
    Some(out)
}

pub(super) fn mask_example(mask: &str) -> String {
    mask.chars()
        .map(|c| match c {
            '9' => '1',
            'A' => 'a',
            'O' => 'b',
            'X' => 'x',
            other => other,
        })
        .collect()
}

/// a time of day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// reads 14:30, 14:30:05, 2:30 pm, 2 pm or 2:30:05 PM.
pub(super) fn parse_time(text: &str) -> Option<Time> {
    let lower = text.trim().to_ascii_lowercase();
    let (body, half) = if let Some(rest) = lower.strip_suffix("pm").or(lower.strip_suffix("p")) {
        (rest.trim().to_owned(), Some(true))
    } else if let Some(rest) = lower.strip_suffix("am").or(lower.strip_suffix("a")) {
        (rest.trim().to_owned(), Some(false))
    } else {
        (lower, None)
    };
    let parts: Vec<&str> = body.split(':').collect();
    if parts.is_empty()
        || parts.len() > 3
        || parts.iter().any(|part| {
            part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) || part.len() > 2
        })
    {
        return None;
    }
    let numbers: Vec<u32> = parts
        .iter()
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    let mut hour = numbers[0];
    let minute = numbers.get(1).copied().unwrap_or(0);
    let second = numbers.get(2).copied().unwrap_or(0);
    if parts.len() == 1 && half.is_none() {
        return None;
    }
    match half {
        Some(pm) => {
            if !(1..=12).contains(&hour) {
                return None;
            }
            hour %= 12;
            if pm {
                hour += 12;
            }
        }
        None if hour > 23 => return None,
        None => {}
    }
    (minute < 60 && second < 60).then_some(Time {
        hour,
        minute,
        second,
    })
}

/// writes a time in an acrobat pattern: HH or H (24 hour), hh or h
/// (12 hour), MM minutes, ss seconds, tt am/pm.
pub(super) fn format_time(time: Time, pattern: &str) -> String {
    let twelve = if time.hour.is_multiple_of(12) {
        12
    } else {
        time.hour % 12
    };
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        let run = chars[index..].iter().take_while(|next| **next == c).count();
        let (text, used) = match (c, run) {
            ('H', 2..) => (format!("{:02}", time.hour), 2),
            ('H', _) => (time.hour.to_string(), 1),
            ('h', 2..) => (format!("{twelve:02}"), 2),
            ('h', _) => (twelve.to_string(), 1),
            ('M', _) => (format!("{:02}", time.minute), run.min(2)),
            ('s', _) => (format!("{:02}", time.second), run.min(2)),
            ('t', 2..) => ((if time.hour < 12 { "am" } else { "pm" }).to_owned(), 2),
            ('t', _) => ((if time.hour < 12 { "a" } else { "p" }).to_owned(), 1),
            (other, _) => (other.to_string(), 1),
        };
        out.push_str(&text);
        index += used;
    }
    out
}
