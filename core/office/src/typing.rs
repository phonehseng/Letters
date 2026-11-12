//! what happens while someone types: smart quotes, autocorrect and
//! capitalising the start of a sentence.
//!
//! everything is one pure function, `on_type`. the editor gives it the
//! text before the cursor (in the current paragraph) and the character
//! just typed; it answers with what to put in their place, or `None` to
//! let the character through unchanged. each feature has its own switch
//! in `TypingSettings`, which the interface shows in one place.

use serde::{Deserialize, Serialize};

/// the switches for every typing helper. all are on by default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypingSettings {
    /// "straight" quotes become “curly” ones, in the language's style
    pub smart_quotes: bool,
    /// the first letter of a sentence becomes a capital
    pub capitalize_sentences: bool,
    /// a lone "i" becomes "I" (english)
    pub capitalize_i: bool,
    /// "THe" becomes "The"
    pub fix_two_initial_capitals: bool,
    /// "--" becomes an em dash and " - " an en dash
    pub replace_dashes: bool,
    /// (c), (r), (tm), ..., -> and <- become ©, ®, ™, …, →, ←
    pub replace_symbols: bool,
    /// common misspellings ("teh") are corrected
    pub correct_spelling: bool,
    /// bcp 47 language of the text, which decides the quote marks
    pub language: String,
    /// the person's own replacements, applied when a word is finished
    #[serde(default)]
    pub custom_rules: Vec<AutocorrectRule>,
}

impl Default for TypingSettings {
    fn default() -> Self {
        TypingSettings {
            smart_quotes: true,
            capitalize_sentences: true,
            capitalize_i: true,
            fix_two_initial_capitals: true,
            replace_dashes: true,
            replace_symbols: true,
            correct_spelling: true,
            language: "en-US".into(),
            custom_rules: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutocorrectRule {
    pub from: String,
    pub to: String,
}

/// one switch as the interface shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypingToggle {
    pub key: &'static str,
    pub label: &'static str,
    pub explanation: &'static str,
    pub on: bool,
}

impl TypingSettings {
    /// every switch with a label and a one-line explanation, in the order
    /// the settings panel and the status bar show them.
    pub fn toggles(&self) -> Vec<TypingToggle> {
        let toggle = |key, label, explanation, on| TypingToggle {
            key,
            label,
            explanation,
            on,
        };
        vec![
            toggle(
                "smartQuotes",
                "Smart Quotes",
                "Turns \"straight\" quotes into curly ones.",
                self.smart_quotes,
            ),
            toggle(
                "capitalizeSentences",
                "Capitalize Sentences",
                "Starts each sentence with a capital letter.",
                self.capitalize_sentences,
            ),
            toggle(
                "capitalizeI",
                "Capitalize “I”",
                "Turns a lone “i” into “I”.",
                self.capitalize_i,
            ),
            toggle(
                "fixTwoInitialCapitals",
                "Fix TWo Capitals",
                "Turns “THe” into “The”.",
                self.fix_two_initial_capitals,
            ),
            toggle(
                "replaceDashes",
                "Smart Dashes",
                "Turns -- into — and a spaced hyphen into –.",
                self.replace_dashes,
            ),
            toggle(
                "replaceSymbols",
                "Symbols",
                "Turns (c), (tm), ... and -> into ©, ™, … and →.",
                self.replace_symbols,
            ),
            toggle(
                "correctSpelling",
                "Fix Common Typos",
                "Corrects slips like “teh” and “recieve”.",
                self.correct_spelling,
            ),
        ]
    }

    /// switches one feature by its key; false for an unknown key.
    pub fn set(&mut self, key: &str, on: bool) -> bool {
        let slot = match key {
            "smartQuotes" => &mut self.smart_quotes,
            "capitalizeSentences" => &mut self.capitalize_sentences,
            "capitalizeI" => &mut self.capitalize_i,
            "fixTwoInitialCapitals" => &mut self.fix_two_initial_capitals,
            "replaceDashes" => &mut self.replace_dashes,
            "replaceSymbols" => &mut self.replace_symbols,
            "correctSpelling" => &mut self.correct_spelling,
            _ => return false,
        };
        *slot = on;
        true
    }
}

/// what to do with a keystroke: remove `delete_before` characters before
/// the cursor, then insert `insert` (which stands in for the typed
/// character too).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub delete_before: usize,
    pub insert: String,
    /// which feature made the change, so the interface can offer to undo
    /// it or switch the feature off
    pub feature: &'static str,
}

impl Replacement {
    fn new(delete_before: usize, insert: impl Into<String>, feature: &'static str) -> Replacement {
        Replacement {
            delete_before,
            insert: insert.into(),
            feature,
        }
    }

    /// applies the replacement to the text before the cursor, for tests
    /// and for callers that keep plain strings.
    pub fn apply(&self, before: &str) -> String {
        let keep = before.chars().count().saturating_sub(self.delete_before);
        let mut text: String = before.chars().take(keep).collect();
        text.push_str(&self.insert);
        text
    }
}

/// decides what typing `typed` after `before` should produce.
pub fn on_type(before: &str, typed: char, settings: &TypingSettings) -> Option<Replacement> {
    if settings.smart_quotes && (typed == '"' || typed == '\'') {
        return Some(smart_quote(before, typed, &settings.language));
    }
    if settings.replace_symbols {
        if let Some(found) = symbol(before, typed) {
            return Some(found);
        }
    }
    if settings.replace_dashes {
        if let Some(found) = dash(before, typed) {
            return Some(found);
        }
    }
    if is_word_end(typed) {
        if let Some(found) = finished_word(before, typed, settings) {
            return Some(found);
        }
    }
    if settings.capitalize_sentences && typed.is_lowercase() && starts_sentence(before) {
        return Some(Replacement::new(
            0,
            typed.to_uppercase().collect::<String>(),
            "capitalizeSentences",
        ));
    }
    None
}

/// the quote marks a language uses: opening and closing double, then
/// opening and closing single.
fn quote_marks(language: &str) -> [&'static str; 4] {
    let primary = language
        .split(['-', '_'])
        .next()
        .unwrap_or("en")
        .to_ascii_lowercase();
    match primary.as_str() {
        "de" | "cs" | "sk" | "sl" | "lt" | "et" | "is" => {
            ["\u{201E}", "\u{201C}", "\u{201A}", "\u{2018}"]
        }
        "fr" => [
            "\u{AB}\u{A0}",
            "\u{A0}\u{BB}",
            "\u{2039}\u{A0}",
            "\u{A0}\u{203A}",
        ],
        "es" | "it" | "pt" | "ca" | "el" | "nb" | "no" => {
            ["\u{AB}", "\u{BB}", "\u{201C}", "\u{201D}"]
        }
        "ru" | "uk" | "be" => ["\u{AB}", "\u{BB}", "\u{201E}", "\u{201C}"],
        "pl" | "hu" | "ro" | "hr" | "bg" => ["\u{201E}", "\u{201D}", "\u{201A}", "\u{2019}"],
        "sv" | "fi" => ["\u{201D}", "\u{201D}", "\u{2019}", "\u{2019}"],
        "ja" | "zh" => ["\u{300C}", "\u{300D}", "\u{300E}", "\u{300F}"],
        _ => ["\u{201C}", "\u{201D}", "\u{2018}", "\u{2019}"],
    }
}

const APOSTROPHE: &str = "\u{2019}";
const PRIME: &str = "\u{2032}";
const DOUBLE_PRIME: &str = "\u{2033}";

fn smart_quote(before: &str, typed: char, language: &str) -> Replacement {
    let marks = quote_marks(language);
    let double = typed == '"';
    let previous = before.chars().last();
    // 5'10" is feet and inches: primes, not quotes
    if previous.is_some_and(|c| c.is_ascii_digit()) && is_measurement(before) {
        let mark = if double { DOUBLE_PRIME } else { PRIME };
        return Replacement::new(0, mark, "smartQuotes");
    }
    let (open, close) = if double {
        (marks[0], marks[1])
    } else {
        (marks[2], marks[3])
    };
    // french puts a space before the closing mark, so after a space with
    // a quote still open, the mark closes it
    let closes_after_space = previous == Some(' ')
        && close.starts_with('\u{A0}')
        && before.matches(open).count() > before.matches(close).count();
    let opening = !closes_after_space
        && match previous {
            None => true,
            Some(c) => {
                c.is_whitespace() || "([{<\u{2014}\u{2013}-/".contains(c) || is_opening_quote(c)
            }
        };
    if !double && !opening {
        // inside or after a word: don't, it's, the students'
        return Replacement::new(0, APOSTROPHE, "smartQuotes");
    }
    if opening {
        return Replacement::new(0, open, "smartQuotes");
    }
    // french closing marks bring their own no-break space
    if close.starts_with('\u{A0}') && previous == Some(' ') {
        return Replacement::new(1, close, "smartQuotes");
    }
    Replacement::new(0, close, "smartQuotes")
}

fn is_opening_quote(c: char) -> bool {
    matches!(
        c,
        '\u{201C}'
            | '\u{2018}'
            | '\u{201E}'
            | '\u{201A}'
            | '\u{AB}'
            | '\u{2039}'
            | '\u{300C}'
            | '\u{300E}'
    )
}

/// true when the word before the cursor is a measurement such as "5" or
/// "5′10" (only digits, points and primes).
fn is_measurement(before: &str) -> bool {
    let word: String = before
        .chars()
        .rev()
        .take_while(|c| !c.is_whitespace())
        .collect();
    // reversed, so the first number is last; a long number is a year
    // ("the 1990's"), not feet
    let first_number = word.rsplit(['\u{2032}', '\u{2033}']).next().unwrap_or("");
    !word.is_empty()
        && first_number.chars().count() <= 3
        && word
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '\u{2032}' || c == '\u{2033}')
}

fn symbol(before: &str, typed: char) -> Option<Replacement> {
    let lower = before.to_lowercase();
    let found = match typed {
        ')' if lower.ends_with("(c") => Some((2, "\u{A9}")),
        ')' if lower.ends_with("(r") => Some((2, "\u{AE}")),
        ')' if lower.ends_with("(tm") => Some((3, "\u{2122}")),
        '.' if before.ends_with("..") => Some((2, "\u{2026}")),
        '>' if before.ends_with('-') => Some((1, "\u{2192}")),
        '>' if before.ends_with('=') => Some((1, "\u{21D2}")),
        '-' if before.ends_with('<') => Some((1, "\u{2190}")),
        _ => None,
    };
    found.map(|(delete, insert)| Replacement::new(delete, insert, "replaceSymbols"))
}

fn dash(before: &str, typed: char) -> Option<Replacement> {
    // "word--" becomes "word—" as soon as the second hyphen is typed
    if typed == '-' && before.ends_with('-') && !before.ends_with("--") {
        let ahead = before.chars().rev().nth(1);
        if ahead.is_some_and(|c| c.is_alphanumeric() || c == ' ') {
            return Some(Replacement::new(1, "\u{2014}", "replaceDashes"));
        }
    }
    // "word - " becomes "word – " when the space after the hyphen is typed
    if typed == ' ' && before.ends_with(" -") {
        let ahead = before.chars().rev().nth(2);
        if ahead.is_some_and(char::is_alphanumeric) {
            return Some(Replacement::new(1, "\u{2013} ", "replaceDashes"));
        }
    }
    None
}

fn is_word_end(c: char) -> bool {
    c.is_whitespace() || ".,;:!?)]}\"'\u{201D}\u{2019}".contains(c)
}

/// the letters (and apostrophes) just before the cursor.
fn last_word(before: &str) -> &str {
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '\'' || *c == '\u{2019}')
        .last()
        .map(|(index, _)| index)
        .unwrap_or(before.len());
    &before[start..]
}

/// rules that look at a word once it is finished.
fn finished_word(before: &str, typed: char, settings: &TypingSettings) -> Option<Replacement> {
    let word = last_word(before);
    if word.is_empty() {
        return None;
    }
    let length = word.chars().count();
    let finish = |replacement: String, feature| {
        let mut insert = replacement;
        insert.push(typed);
        Some(Replacement::new(length, insert, feature))
    };
    for rule in &settings.custom_rules {
        if rule.from == word {
            return finish(rule.to.clone(), "customRules");
        }
    }
    let english = settings.language.to_ascii_lowercase().starts_with("en");
    if settings.capitalize_i && english {
        if let Some(fixed) = capital_i(word) {
            return finish(fixed, "capitalizeI");
        }
    }
    if settings.fix_two_initial_capitals {
        if let Some(fixed) = two_initial_capitals(word) {
            return finish(fixed, "fixTwoInitialCapitals");
        }
    }
    if settings.correct_spelling && english {
        if let Some(fixed) = common_typo(word) {
            return finish(fixed, "correctSpelling");
        }
    }
    None
}

fn capital_i(word: &str) -> Option<String> {
    let normal = word.replace('\'', APOSTROPHE);
    match normal.as_str() {
        "i" => Some("I".into()),
        "i\u{2019}m" | "i\u{2019}ll" | "i\u{2019}d" | "i\u{2019}ve" => {
            Some(format!("I{}", &normal[1..]))
        }
        _ => None,
    }
}

fn two_initial_capitals(word: &str) -> Option<String> {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 3 || !chars.iter().all(|c| c.is_alphabetic()) {
        return None;
    }
    let rest_lower = chars[2..].iter().all(|c| c.is_lowercase());
    if chars[0].is_uppercase() && chars[1].is_uppercase() && rest_lower {
        let mut fixed = String::new();
        fixed.push(chars[0]);
        fixed.extend(chars[1].to_lowercase());
        fixed.extend(&chars[2..]);
        return Some(fixed);
    }
    None
}

/// slips that are never real words.
const TYPOS: &[(&str, &str)] = &[
    ("teh", "the"),
    ("adn", "and"),
    ("taht", "that"),
    ("hte", "the"),
    ("recieve", "receive"),
    ("recieved", "received"),
    ("beleive", "believe"),
    ("seperate", "separate"),
    ("definately", "definitely"),
    ("occured", "occurred"),
    ("occuring", "occurring"),
    ("untill", "until"),
    ("wich", "which"),
    ("thier", "their"),
    ("accomodate", "accommodate"),
    ("acheive", "achieve"),
    ("arguement", "argument"),
    ("enviroment", "environment"),
    ("goverment", "government"),
    ("independant", "independent"),
    ("neccessary", "necessary"),
    ("noticable", "noticeable"),
    ("persue", "pursue"),
    ("publically", "publicly"),
    ("recomend", "recommend"),
    ("refered", "referred"),
    ("succesful", "successful"),
    ("tommorow", "tomorrow"),
    ("truely", "truly"),
    ("wierd", "weird"),
    ("alot", "a lot"),
    ("dont", "don\u{2019}t"),
    ("doesnt", "doesn\u{2019}t"),
    ("didnt", "didn\u{2019}t"),
    ("isnt", "isn\u{2019}t"),
    ("wasnt", "wasn\u{2019}t"),
    ("couldnt", "couldn\u{2019}t"),
    ("shouldnt", "shouldn\u{2019}t"),
    ("wouldnt", "wouldn\u{2019}t"),
];

/// corrects a typo, keeping a leading capital ("Teh" gives "The").
fn common_typo(word: &str) -> Option<String> {
    let lower = word.to_lowercase();
    let (_, fixed) = TYPOS.iter().find(|(typo, _)| *typo == lower)?;
    let capital = word.chars().next().is_some_and(char::is_uppercase);
    if !capital {
        return Some(fixed.to_string());
    }
    let mut chars = fixed.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

/// abbreviations whose full stop does not end a sentence.
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "st", "jr", "sr", "vs", "etc", "e.g", "i.e", "cf", "al", "p",
    "pp", "no", "fig", "vol", "ch", "ed", "eds", "approx", "dept", "est", "inc", "ltd", "co",
    "a.m", "p.m",
];

/// true when a letter typed after `before` would begin a sentence.
fn starts_sentence(before: &str) -> bool {
    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.len() == before.len() {
        // no space typed yet: this letter continues the current word
        return false;
    }
    // look past closing quotes and brackets: He said "Stop." Then...
    let core = trimmed.trim_end_matches(|c: char| "\"'\u{201D}\u{2019})]\u{BB}".contains(c));
    let Some(last) = core.chars().last() else {
        return false;
    };
    if last == '!' || last == '?' {
        return true;
    }
    if last != '.' || core.ends_with("..") {
        return false;
    }
    let word = core[..core.len() - 1]
        .rsplit(|c: char| c.is_whitespace() || c == '(' || c == '"')
        .next()
        .unwrap_or("")
        .to_lowercase();
    if word.is_empty() || ABBREVIATIONS.contains(&word.as_str()) {
        return false;
    }
    // "J." is an initial and "3." a number in a list, not a sentence end
    let single_letter = word.chars().count() == 1 && word.chars().all(char::is_alphabetic);
    let number = word.chars().all(|c| c.is_ascii_digit());
    !(single_letter || number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(before: &str, c: char) -> String {
        let settings = TypingSettings::default();
        match on_type(before, c, &settings) {
            Some(replacement) => replacement.apply(before),
            None => format!("{before}{c}"),
        }
    }

    #[test]
    fn quotes_in_english() {
        assert_eq!(typed("", '"'), "\u{201C}");
        assert_eq!(
            typed("He said \u{201C}hi", '"'),
            "He said \u{201C}hi\u{201D}"
        );
        assert_eq!(typed("(", '"'), "(\u{201C}");
        assert_eq!(typed("don", '\''), "don\u{2019}");
        assert_eq!(typed("the students", '\''), "the students\u{2019}");
        assert_eq!(typed("a ", '\''), "a \u{2018}");
        assert_eq!(typed("\u{201C}", '\''), "\u{201C}\u{2018}");
        assert_eq!(typed("5", '\''), "5\u{2032}");
        assert_eq!(typed("5\u{2032}10", '"'), "5\u{2032}10\u{2033}");
    }

    #[test]
    fn quotes_follow_the_language() {
        let mut settings = TypingSettings {
            language: "de-DE".into(),
            ..Default::default()
        };
        assert_eq!(on_type("", '"', &settings).unwrap().insert, "\u{201E}");
        assert_eq!(on_type("Hallo", '"', &settings).unwrap().insert, "\u{201C}");
        settings.language = "fr".into();
        assert_eq!(on_type("", '"', &settings).unwrap().insert, "\u{AB}\u{A0}");
        let close = on_type("\u{AB}\u{A0}oui ", '"', &settings).unwrap();
        assert_eq!(
            close.apply("\u{AB}\u{A0}oui "),
            "\u{AB}\u{A0}oui\u{A0}\u{BB}"
        );
    }

    #[test]
    fn sentences_start_with_capitals() {
        assert_eq!(typed("", 't'), "T");
        assert_eq!(typed("It ended. ", 't'), "It ended. T");
        assert_eq!(typed("Really? ", 'y'), "Really? Y");
        assert_eq!(
            typed("He said \u{201C}Stop.\u{201D} ", 't'),
            "He said \u{201C}Stop.\u{201D} T"
        );
        assert_eq!(typed("See e.g. ", 't'), "See e.g. t");
        assert_eq!(typed("Dr. ", 's'), "Dr. s");
        assert_eq!(typed("J. ", 's'), "J. s");
        assert_eq!(typed("version 3.", '5'), "version 3.5");
        assert_eq!(typed("word", 's'), "words");
    }

    #[test]
    fn autocorrect_rules() {
        assert_eq!(typed("so i", ' '), "so I ");
        assert_eq!(typed("so i'm", ' '), "so I\u{2019}m ");
        assert_eq!(typed("THe", ' '), "The ");
        assert_eq!(typed("NASA", ' '), "NASA ");
        assert_eq!(typed("teh", ' '), "the ");
        assert_eq!(typed("Teh", ','), "The,");
        assert_eq!(typed("word-", '-'), "word\u{2014}");
        assert_eq!(typed("this -", ' '), "this \u{2013} ");
        assert_eq!(typed("(c", ')'), "\u{A9}");
        assert_eq!(typed("Brand(TM", ')'), "Brand\u{2122}");
        assert_eq!(typed("wait..", '.'), "wait\u{2026}");
        assert_eq!(typed("a -", '>'), "a \u{2192}");
    }

    #[test]
    fn every_feature_can_be_switched_off() {
        let mut settings = TypingSettings::default();
        for toggle in settings.clone().toggles() {
            assert!(settings.set(toggle.key, false), "{}", toggle.key);
        }
        for (before, c) in [
            ("", '"'),
            ("", 't'),
            ("so i", ' '),
            ("teh", ' '),
            ("(c", ')'),
            ("a-", '-'),
            ("THe", ' '),
        ] {
            assert_eq!(on_type(before, c, &settings), None, "{before:?} {c:?}");
        }
        assert!(!settings.set("nonsense", true));
    }

    #[test]
    fn custom_rules_apply_to_finished_words() {
        let settings = TypingSettings {
            custom_rules: vec![AutocorrectRule {
                from: "lbl".into(),
                to: "label".into(),
            }],
            ..Default::default()
        };
        let replacement = on_type("a lbl", ' ', &settings).unwrap();
        assert_eq!(replacement.apply("a lbl"), "a label ");
        assert_eq!(replacement.feature, "customRules");
    }
}
