//! completing a column by example ("flash fill"): from two or three
//! typed examples next to a column of text, find the one simple rule
//! that turns each source into its example (a word of the text, its
//! first letters, a change of case, two words joined) and apply it to
//! the other rows. when more than one rule fits and they disagree on any
//! other row, nothing is offered: a wrong guess is worse than none.

/// the separators words are split at
const SEPARATORS: [char; 6] = [' ', ',', '@', '-', '.', '_'];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Same,
    Upper,
    Lower,
    Proper,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Rule {
    /// word `index` (negative counts from the end) after splitting at `separator`
    Word {
        separator: char,
        index: i32,
        case: Case,
    },
    /// the first `count` characters
    Start { count: usize, case: Case },
    /// two words joined by `joiner`
    Join {
        separator: char,
        first: i32,
        second: i32,
        joiner: String,
        case: Case,
    },
    /// the first letter of every word ("Jane Doe" -> "JD")
    Initials { separator: char, case: Case },
}

fn change_case(text: &str, case: Case) -> String {
    match case {
        Case::Same => text.to_string(),
        Case::Upper => text.to_uppercase(),
        Case::Lower => text.to_lowercase(),
        Case::Proper => {
            let mut chars = text.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase())
                .unwrap_or_default()
        }
    }
}

fn word(text: &str, separator: char, index: i32) -> Option<String> {
    let words: Vec<&str> = text.split(separator).filter(|w| !w.is_empty()).collect();
    let position = if index < 0 {
        words.len() as i32 + index
    } else {
        index
    };
    words
        .get(usize::try_from(position).ok()?)
        .map(|w| w.trim().to_string())
}

fn apply(rule: &Rule, text: &str) -> Option<String> {
    match rule {
        Rule::Word {
            separator,
            index,
            case,
        } => word(text, *separator, *index).map(|w| change_case(&w, *case)),
        Rule::Start { count, case } => (text.chars().count() >= *count)
            .then(|| change_case(&text.chars().take(*count).collect::<String>(), *case)),
        Rule::Join {
            separator,
            first,
            second,
            joiner,
            case,
        } => {
            let (a, b) = (
                word(text, *separator, *first)?,
                word(text, *separator, *second)?,
            );
            Some(format!(
                "{}{joiner}{}",
                change_case(&a, *case),
                change_case(&b, *case)
            ))
        }
        Rule::Initials { separator, case } => {
            let initials: String = text
                .split(*separator)
                .filter_map(|w| w.trim().chars().next())
                .collect();
            (initials.chars().count() > 1).then(|| change_case(&initials, *case))
        }
    }
}

fn candidates() -> Vec<Rule> {
    let cases = [Case::Same, Case::Upper, Case::Lower, Case::Proper];
    let mut rules = Vec::new();
    for case in cases {
        for separator in SEPARATORS {
            for index in [0, 1, 2, -1, -2] {
                rules.push(Rule::Word {
                    separator,
                    index,
                    case,
                });
            }
            for (first, second) in [(-1, 0), (0, -1), (1, 0)] {
                for joiner in [", ", " ", ".", "", "_", "-"] {
                    rules.push(Rule::Join {
                        separator,
                        first,
                        second,
                        joiner: joiner.to_string(),
                        case,
                    });
                }
            }
            rules.push(Rule::Initials { separator, case });
        }
        for count in 1..=5 {
            rules.push(Rule::Start { count, case });
        }
    }
    rules
}

fn priority(rule: &Rule) -> usize {
    match rule {
        Rule::Word { .. } => 0,
        Rule::Join { .. } => 1,
        Rule::Initials { .. } => 2,
        Rule::Start { .. } => 3,
    }
}

/// the values for `rest` given (source, typed) `examples`, or none when
/// no rule fits or the fitting rules disagree
pub fn complete_by_example(examples: &[(String, String)], rest: &[String]) -> Option<Vec<String>> {
    if examples.is_empty() || rest.is_empty() {
        return None;
    }
    let fits = |rule: &Rule| {
        examples
            .iter()
            .all(|(source, typed)| apply(rule, source).as_deref() == Some(typed.as_str()))
    };
    // words beat joins, joins beat initials, initials beat "first n
    // letters": "Jane" from "Jane Doe" is the first word, not four letters
    let fitting: Vec<Rule> = (0..4)
        .map(|level| {
            candidates()
                .into_iter()
                .filter(|rule| priority(rule) == level)
                .filter(|rule| fits(rule))
                .collect::<Vec<_>>()
        })
        .find(|group| !group.is_empty())?;
    let first = fitting.first()?;
    let answer: Option<Vec<String>> = rest.iter().map(|text| apply(first, text)).collect();
    let answer = answer?;
    let all_agree = fitting.iter().all(|rule| {
        rest.iter()
            .zip(&answer)
            .all(|(text, expected)| apply(rule, text).as_deref() == Some(expected.as_str()))
    });
    all_agree.then_some(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    fn texts(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn completes_names_and_emails_by_example() {
        let first_names = complete_by_example(
            &pairs(&[("Jane Doe", "Jane"), ("John Smith", "John")]),
            &texts(&["Ada Lovelace"]),
        );
        assert_eq!(first_names, Some(texts(&["Ada"])));
        let reversed = complete_by_example(
            &pairs(&[("Jane Doe", "Doe, Jane")]),
            &texts(&["Ada Lovelace"]),
        );
        assert_eq!(reversed, Some(texts(&["Lovelace, Ada"])));
        let users = complete_by_example(
            &pairs(&[("jane@school.edu", "jane"), ("ada@uni.org", "ada")]),
            &texts(&["max@x.com"]),
        );
        assert_eq!(users, Some(texts(&["max"])));
        let initials = complete_by_example(
            &pairs(&[("Jane Doe", "JD"), ("Ada Lovelace", "AL")]),
            &texts(&["Max Planck"]),
        );
        assert_eq!(initials, Some(texts(&["MP"])));
    }

    #[test]
    fn says_nothing_when_unsure_or_impossible() {
        // "a" from "ab cd" is its first letter, but kept as typed or made
        // lower case? the two rules disagree on "Xy z", so nothing is offered
        assert_eq!(
            complete_by_example(&pairs(&[("ab cd", "a")]), &texts(&["Xy z"])),
            None
        );
        assert_eq!(
            complete_by_example(&pairs(&[("Jane Doe", "xyz")]), &texts(&["Ada Lovelace"])),
            None
        );
    }
}
