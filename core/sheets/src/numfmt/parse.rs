//! reading an excel number format code ("#,##0.00;[Red](#,##0.00)",
//! "d mmm yyyy", "0%", "# ?/?", "[h]:mm:ss", "@") into sections of tokens.
//! the grammar is ecma-376 part 1, 18.8.31 (numFmts).

/// one piece of a section
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Literal(String),
    /// '0' shows a digit or 0, '#' a digit or nothing, '?' a digit or a space
    Digit(char),
    DecimalPoint,
    /// a comma between digit placeholders: thousands separators
    Thousands,
    /// a comma after the last digit placeholder: divide by 1000
    Scale,
    Percent,
    /// "E+" (always show the sign) or "E-"
    Exponent {
        plus: bool,
    },
    /// the "/" of a fraction
    Slash,
    /// '@': the text itself
    Text,
    /// '*x': repeat x to fill the cell (shown once here)
    Fill(char),
    /// '_x': a space as wide as x
    Space,
    Date(DatePart),
    General,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatePart {
    Year2,
    Year4,
    Month,
    Month2,
    MonthShort,
    MonthLong,
    MonthLetter,
    Day,
    Day2,
    DayShort,
    DayLong,
    Hour,
    Hour2,
    Minute,
    Minute2,
    Second,
    Second2,
    /// tenths, hundredths or thousandths of a second
    SubSecond(u8),
    AmPm,
    /// "A/P"
    AP,
    ElapsedHours(u8),
    ElapsedMinutes(u8),
    ElapsedSeconds(u8),
}

/// a "[>100]" style condition
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Condition {
    pub operator: &'static str,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Section {
    pub tokens: Vec<Token>,
    pub color: Option<String>,
    pub condition: Option<Condition>,
}

impl Section {
    pub fn is_date(&self) -> bool {
        self.tokens.iter().any(|t| matches!(t, Token::Date(_)))
    }

    pub fn has(&self, wanted: &Token) -> bool {
        self.tokens.contains(wanted)
    }
}

/// the colours excel names in format codes
const COLORS: [&str; 8] = [
    "black", "blue", "cyan", "green", "magenta", "red", "white", "yellow",
];

pub fn parse(code: &str) -> Vec<Section> {
    split_sections(code)
        .iter()
        .map(|text| parse_section(text))
        .collect()
}

/// splits at ";" outside quotes and brackets
fn split_sections(code: &str) -> Vec<String> {
    let mut sections = vec![String::new()];
    let mut chars = code.chars();
    let mut quoted = false;
    let mut bracket = false;
    while let Some(c) = chars.next() {
        let current = sections.last_mut().expect("there is always a section");
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            '\\' if !quoted => {
                current.push(c);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
                continue;
            }
            ';' if !quoted && !bracket => {
                sections.push(String::new());
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    sections
}

fn parse_section(text: &str) -> Section {
    let mut section = Section::default();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        i = read_token(&chars, i, &mut section);
    }
    mark_commas(&mut section.tokens);
    resolve_minutes(&mut section.tokens);
    section
}

/// reads the token at `i` into the section and returns where the next starts
fn read_token(chars: &[char], i: usize, section: &mut Section) -> usize {
    let c = chars[i];
    let rest: String = chars[i..].iter().collect();
    let lower = rest.to_ascii_lowercase();
    let push = |section: &mut Section, token: Token| section.tokens.push(token);
    match c {
        '"' => {
            let end = chars[i + 1..]
                .iter()
                .position(|c| *c == '"')
                .map(|p| i + 1 + p)
                .unwrap_or(chars.len());
            push(section, Token::Literal(chars[i + 1..end].iter().collect()));
            end + 1
        }
        '\\' => {
            if let Some(next) = chars.get(i + 1) {
                push(section, Token::Literal(next.to_string()));
            }
            i + 2
        }
        '[' => read_bracket(chars, i, section),
        '0' | '#' | '?' => {
            push(section, Token::Digit(c));
            i + 1
        }
        '.' => read_point(chars, i, section),
        ',' => {
            // decided by `mark_commas` once the section is read
            push(section, Token::Thousands);
            i + 1
        }
        '%' => {
            push(section, Token::Percent);
            i + 1
        }
        '@' => {
            push(section, Token::Text);
            i + 1
        }
        '*' => {
            push(
                section,
                Token::Fill(chars.get(i + 1).copied().unwrap_or(' ')),
            );
            i + 2
        }
        '_' => {
            push(section, Token::Space);
            i + 2
        }
        '/' => {
            push(section, Token::Slash);
            i + 1
        }
        'E' | 'e' if matches!(chars.get(i + 1), Some('+') | Some('-')) => {
            push(
                section,
                Token::Exponent {
                    plus: chars[i + 1] == '+',
                },
            );
            i + 2
        }
        _ if lower.starts_with("general") => {
            push(section, Token::General);
            i + 7
        }
        _ if lower.starts_with("am/pm") => {
            push(section, Token::Date(DatePart::AmPm));
            i + 5
        }
        _ if lower.starts_with("a/p") => {
            push(section, Token::Date(DatePart::AP));
            i + 3
        }
        'y' | 'Y' | 'm' | 'M' | 'd' | 'D' | 'h' | 'H' | 's' | 'S' => read_date(chars, i, section),
        _ => {
            push(section, Token::Literal(c.to_string()));
            i + 1
        }
    }
}

/// "[Red]", "[Color12]", "[>100]", "[$€-407]", "[h]"
fn read_bracket(chars: &[char], i: usize, section: &mut Section) -> usize {
    let end = chars[i..]
        .iter()
        .position(|c| *c == ']')
        .map(|p| i + p)
        .unwrap_or(chars.len());
    let inside: String = chars[i + 1..end.min(chars.len())].iter().collect();
    let lower = inside.to_ascii_lowercase();
    let next = end + 1;
    if COLORS.contains(&lower.as_str()) || lower.starts_with("color") {
        section.color = Some(inside);
    } else if let Some(condition) = parse_condition(&inside) {
        section.condition = Some(condition);
    } else if let Some(currency) = inside.strip_prefix('$') {
        let symbol = currency.split('-').next().unwrap_or("");
        if !symbol.is_empty() {
            section.tokens.push(Token::Literal(symbol.to_string()));
        }
    } else if !lower.is_empty()
        && lower
            .chars()
            .all(|c| c == lower.chars().next().unwrap_or(' '))
    {
        let width = lower.len().min(9) as u8;
        let part = match lower.chars().next() {
            Some('h') => Some(DatePart::ElapsedHours(width)),
            Some('m') => Some(DatePart::ElapsedMinutes(width)),
            Some('s') => Some(DatePart::ElapsedSeconds(width)),
            _ => None,
        };
        section.tokens.extend(part.map(Token::Date));
    }
    next
}

fn parse_condition(text: &str) -> Option<Condition> {
    for operator in ["<=", ">=", "<>", "<", ">", "="] {
        if let Some(value) = text.strip_prefix(operator) {
            return value
                .trim()
                .parse()
                .ok()
                .map(|value| Condition { operator, value });
        }
    }
    None
}

/// "." starts the decimals, or in a time ("ss.00") the fractions of a second
fn read_point(chars: &[char], i: usize, section: &mut Section) -> usize {
    let after_seconds = matches!(
        section.tokens.last(),
        Some(Token::Date(
            DatePart::Second | DatePart::Second2 | DatePart::ElapsedSeconds(_)
        ))
    );
    let zeros = chars[i + 1..].iter().take_while(|c| **c == '0').count();
    if after_seconds && zeros > 0 {
        section
            .tokens
            .push(Token::Date(DatePart::SubSecond(zeros.min(3) as u8)));
        return i + 1 + zeros;
    }
    section.tokens.push(Token::DecimalPoint);
    i + 1
}

fn read_date(chars: &[char], i: usize, section: &mut Section) -> usize {
    let letter = chars[i].to_ascii_lowercase();
    let run = chars[i..]
        .iter()
        .take_while(|c| c.to_ascii_lowercase() == letter)
        .count();
    let part = match (letter, run) {
        ('y', 1..=2) => DatePart::Year2,
        ('y', _) => DatePart::Year4,
        ('m', 1) => DatePart::Month,
        ('m', 2) => DatePart::Month2,
        ('m', 3) => DatePart::MonthShort,
        ('m', 5) => DatePart::MonthLetter,
        ('m', _) => DatePart::MonthLong,
        ('d', 1) => DatePart::Day,
        ('d', 2) => DatePart::Day2,
        ('d', 3) => DatePart::DayShort,
        ('d', _) => DatePart::DayLong,
        ('h', 1) => DatePart::Hour,
        ('h', _) => DatePart::Hour2,
        ('s', 1) => DatePart::Second,
        (_, _) => DatePart::Second2,
    };
    section.tokens.push(Token::Date(part));
    i + run
}

/// a comma is a thousands separator when a digit placeholder follows it
/// somewhere before the decimal point; otherwise it scales by 1000
fn mark_commas(tokens: &mut [Token]) {
    for index in 0..tokens.len() {
        if tokens[index] != Token::Thousands {
            continue;
        }
        let digit_follows = tokens[index + 1..]
            .iter()
            .take_while(|t| {
                !matches!(
                    t,
                    Token::DecimalPoint | Token::Exponent { .. } | Token::Slash
                )
            })
            .any(|t| matches!(t, Token::Digit(_)));
        let digit_before = tokens[..index].iter().any(|t| matches!(t, Token::Digit(_)));
        if !digit_follows && digit_before {
            tokens[index] = Token::Scale;
        } else if !digit_before {
            tokens[index] = Token::Literal(",".into());
        }
    }
}

/// "m" after an hour or before a second means minutes (excel's rule)
fn resolve_minutes(tokens: &mut [Token]) {
    let date_positions: Vec<usize> = (0..tokens.len())
        .filter(|i| matches!(tokens[*i], Token::Date(_)))
        .collect();
    for (k, index) in date_positions.iter().enumerate() {
        let is_month = matches!(
            tokens[*index],
            Token::Date(DatePart::Month | DatePart::Month2)
        );
        if !is_month {
            continue;
        }
        let previous = k.checked_sub(1).map(|p| &tokens[date_positions[p]]);
        let next = date_positions.get(k + 1).map(|n| &tokens[*n]);
        let after_hour = matches!(
            previous,
            Some(Token::Date(
                DatePart::Hour | DatePart::Hour2 | DatePart::ElapsedHours(_)
            ))
        );
        let before_second = matches!(
            next,
            Some(Token::Date(DatePart::Second | DatePart::Second2))
        );
        if after_hour || before_second {
            let two = matches!(tokens[*index], Token::Date(DatePart::Month2));
            tokens[*index] = Token::Date(if two {
                DatePart::Minute2
            } else {
                DatePart::Minute
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_colours_and_conditions() {
        let sections = parse("#,##0.00;[Red](#,##0.00);\"zero\";@");
        assert_eq!(sections.len(), 4);
        assert_eq!(sections[1].color.as_deref(), Some("Red"));
        assert!(sections[0].has(&Token::Thousands));
        assert_eq!(sections[2].tokens, vec![Token::Literal("zero".into())]);
        let conditional = parse("[>=100]0;0.00");
        assert_eq!(
            conditional[0].condition,
            Some(Condition {
                operator: ">=",
                value: 100.0
            })
        );
    }

    #[test]
    fn months_and_minutes() {
        let tokens = &parse("h:mm m/d")[0].tokens;
        assert!(tokens.contains(&Token::Date(DatePart::Minute2)));
        assert!(tokens.contains(&Token::Date(DatePart::Month)));
        let tokens = &parse("mm:ss.00")[0].tokens;
        assert_eq!(tokens[0], Token::Date(DatePart::Minute2));
        assert!(tokens.contains(&Token::Date(DatePart::SubSecond(2))));
        assert_eq!(
            parse("[h]:mm")[0].tokens[0],
            Token::Date(DatePart::ElapsedHours(1))
        );
    }

    #[test]
    fn commas_scale_after_digits() {
        assert!(parse("0,,")[0].has(&Token::Scale));
        assert!(parse("[$€-407]#,##0")[0]
            .tokens
            .contains(&Token::Literal("€".into())));
    }
}
