//! bullet labels: a character, or the number an auto-numbered paragraph
//! shows ("3.", "c)", "iv.") counted the way powerpoint counts them.

use crate::model::{Bullet, ParagraphProps};

/// counts auto-numbered paragraphs per level. a paragraph at a shallower
/// level restarts the deeper ones, and a paragraph without numbering at
/// a level restarts that level, as powerpoint does.
#[derive(Default)]
pub struct NumberCounter {
    /// per level: the scheme and the next number
    levels: Vec<Option<(String, u32)>>,
}

impl NumberCounter {
    /// the label for a paragraph, advancing the count.
    pub fn label(&mut self, props: &ParagraphProps) -> Option<String> {
        let level = usize::from(props.level.unwrap_or(0));
        if self.levels.len() <= level {
            self.levels.resize(level + 1, None);
        }
        self.levels.truncate(level + 1);
        match &props.bullet {
            Some(Bullet::Char { char }) => {
                self.levels[level] = None;
                Some(char.clone())
            }
            Some(Bullet::AutoNumber { scheme, start_at }) => {
                let start = start_at.unwrap_or(1);
                let number = match &self.levels[level] {
                    Some((current, next)) if current == scheme => *next,
                    _ => start,
                };
                self.levels[level] = Some((scheme.clone(), number + 1));
                Some(number_label(scheme, number))
            }
            Some(Bullet::Picture { .. }) => {
                self.levels[level] = None;
                // picture bullets are drawn as a round bullet for now
                Some("•".to_string())
            }
            Some(Bullet::None) | None => {
                self.levels[level] = None;
                None
            }
        }
    }
}

/// the label an auto-number scheme gives a number ("alphaLcParenR", 3 is "c)").
pub fn number_label(scheme: &str, number: u32) -> String {
    let digits = if scheme.starts_with("alphaLc") {
        letters(number).to_lowercase()
    } else if scheme.starts_with("alphaUc") {
        letters(number)
    } else if scheme.starts_with("romanLc") {
        roman(number).to_lowercase()
    } else if scheme.starts_with("romanUc") {
        roman(number)
    } else {
        number.to_string()
    };
    if scheme.ends_with("ParenBoth") {
        format!("({digits})")
    } else if scheme.ends_with("ParenR") {
        format!("{digits})")
    } else if scheme.ends_with("Period") {
        format!("{digits}.")
    } else if scheme.ends_with("Minus") {
        format!("- {digits} -")
    } else {
        digits
    }
}

/// 1 is A, 26 is Z, 27 is AA: letters repeat, as office numbers them.
fn letters(number: u32) -> String {
    let number = number.max(1);
    let letter = char::from(b'A' + ((number - 1) % 26) as u8);
    let repeats = (number - 1) / 26 + 1;
    std::iter::repeat_n(letter, repeats as usize).collect()
}

fn roman(number: u32) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut rest = number.max(1);
    let mut out = String::new();
    for (value, numeral) in TABLE {
        while rest >= value {
            out.push_str(numeral);
            rest -= value;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(level: u8, scheme: &str) -> ParagraphProps {
        ParagraphProps {
            level: Some(level),
            bullet: Some(Bullet::AutoNumber {
                scheme: scheme.into(),
                start_at: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn schemes_give_office_labels() {
        assert_eq!(number_label("arabicPeriod", 3), "3.");
        assert_eq!(number_label("alphaLcParenR", 3), "c)");
        assert_eq!(number_label("romanUcPeriod", 4), "IV.");
        assert_eq!(number_label("alphaUcPeriod", 28), "BB.");
        assert_eq!(number_label("arabicParenBoth", 2), "(2)");
    }

    #[test]
    fn deeper_levels_restart_after_a_shallower_paragraph() {
        let mut counter = NumberCounter::default();
        let labels: Vec<_> = [
            numbered(0, "arabicPeriod"),
            numbered(1, "alphaLcPeriod"),
            numbered(1, "alphaLcPeriod"),
            numbered(0, "arabicPeriod"),
            numbered(1, "alphaLcPeriod"),
        ]
        .iter()
        .map(|p| counter.label(p).unwrap())
        .collect();
        assert_eq!(labels, ["1.", "a.", "b.", "2.", "a."]);
    }
}
