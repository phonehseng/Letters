//! the engine's formula-typing helpers (core/sheets `help::entry`), with
//! the cursor counted the way the web view counts it.
//!
//! javascript counts a string in utf-16 units, the engine in bytes (and
//! the argument hint in characters). they agree for plain formulas, but
//! "=A1&\"é\"" would put the engine's cursor inside a letter, which would
//! panic, so every offset is turned over here at the door.

use letters_sheets::help::entry::{self, ArgumentHint, REFERENCE_COLOURS};
use serde::Serialize;

/// the byte offset of a utf-16 offset (clamped to the text).
fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        if units >= utf16 {
            return byte;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// the utf-16 offset of a byte offset.
fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}

/// how many characters come before a utf-16 offset.
fn char_offset(text: &str, utf16: usize) -> usize {
    text[..byte_offset(text, utf16)].chars().count()
}

pub fn argument_hint(formula: &str, caret: usize) -> Option<ArgumentHint> {
    entry::argument_hint(formula, char_offset(formula, caret))
}

/// a reference in the formula, with its excel colour.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColouredReference {
    pub start: usize,
    pub end: usize,
    pub text: String,
    /// "#RRGGBB"
    pub colour: &'static str,
}

pub fn reference_spans(formula: &str) -> Vec<ColouredReference> {
    entry::reference_spans(formula)
        .into_iter()
        .map(|span| ColouredReference {
            start: utf16_offset(formula, span.start),
            end: utf16_offset(formula, span.end),
            text: span.text,
            colour: REFERENCE_COLOURS[span.colour % REFERENCE_COLOURS.len()],
        })
        .collect()
}

/// new text and cursor, both for the web view.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Typed {
    pub text: String,
    pub caret: usize,
}

pub fn insert_reference(formula: &str, caret: usize, reference: &str) -> Typed {
    let (text, at) = entry::insert_reference(formula, byte_offset(formula, caret), reference);
    let caret = utf16_offset(&text, at);
    Typed { text, caret }
}

pub fn expects_reference(formula: &str, caret: usize) -> bool {
    entry::expects_reference(formula, byte_offset(formula, caret))
}

pub fn cycle_reference(formula: &str, caret: usize) -> Typed {
    let (text, at) = entry::cycle_reference(formula, byte_offset(formula, caret));
    let caret = utf16_offset(&text, at);
    Typed { text, caret }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_after_an_accented_letter_count_like_javascript() {
        // "é" is one utf-16 unit and two bytes; "😀" is two units and four bytes
        let formula = "=\"é😀\"&A1";
        let spans = reference_spans(formula);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start, spans[0].end), (7, 9));
        let cycled = cycle_reference(formula, 9);
        assert_eq!(cycled.text, "=\"é😀\"&$A$1");
        assert_eq!(cycled.caret, 11);
    }

    #[test]
    fn a_cursor_inside_a_letter_never_breaks_the_engine() {
        let formula = "=\"😀\"&";
        // offset 3 is between the two halves of the emoji
        let typed = insert_reference(formula, 3, "B2");
        assert!(typed.text.contains("B2"));
        assert!(expects_reference(formula, 6));
    }

    #[test]
    fn the_argument_hint_finds_the_argument_after_accents() {
        let hint = argument_hint("=IF(\"é\"=A1,", 11).unwrap();
        assert_eq!(hint.function.name, "IF");
        assert_eq!(hint.argument, 1);
    }
}
