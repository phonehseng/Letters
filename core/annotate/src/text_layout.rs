//! breaking a text box's text into lines that fit its width.
//!
//! the rule is the one people expect from a word processor: words move to
//! the next line when they don't fit, a new line in the text always
//! starts a new line in the box, and a single word too long for the box
//! is split between letters rather than running outside it.

use crate::error::AnnotateError;
use crate::fonts::StandardFont;
use crate::winansi;

/// one line of a text box, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    /// the line's characters in WinAnsiEncoding
    pub encoded: Vec<u8>,
    /// how wide the line is at the chosen size, in points
    pub width: f32,
}

/// how a text box measures and breaks its text.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextStyle {
    pub font: StandardFont,
    pub size: f32,
}

impl TextStyle {
    fn measure(&self, encoded: &[u8]) -> f32 {
        self.font.measure(encoded, self.size)
    }
}

/// checks that every character can be drawn, then wraps the text into
/// lines no wider than `max_width` points.
pub(crate) fn wrap(
    text: &str,
    style: TextStyle,
    max_width: f32,
) -> Result<Vec<Line>, AnnotateError> {
    let normalised = text.replace("\r\n", "\n").replace(['\r', '\u{2028}'], "\n");
    // a tab is drawn as a few spaces; the standard fonts have no tab stops
    let normalised = normalised.replace('\t', "    ");
    check_drawable(&normalised)?;

    let mut lines = Vec::new();
    for paragraph in normalised.split('\n') {
        let paragraph = winansi::encode(paragraph).expect("checked above");
        wrap_paragraph(&paragraph, style, max_width, &mut lines);
    }
    Ok(lines)
}

/// fails with every character the standard fonts can't draw, so the
/// person sees all of them at once instead of one at a time.
pub(crate) fn check_drawable(text: &str) -> Result<(), AnnotateError> {
    let without_breaks = text.replace(['\n', '\r', '\t'], " ");
    winansi::encode(&without_breaks)
        .map(|_| ())
        .map_err(|missing| AnnotateError::UnsupportedCharacters(missing.into_iter().collect()))
}

/// wraps one paragraph (text without line breaks) and adds its lines.
fn wrap_paragraph(paragraph: &[u8], style: TextStyle, max_width: f32, lines: &mut Vec<Line>) {
    let mut current: Vec<u8> = Vec::new();
    let first_line = lines.len();
    for word in paragraph
        .split(|&byte| byte == b' ')
        .filter(|word| !word.is_empty())
    {
        let mut candidate = current.clone();
        if !candidate.is_empty() {
            candidate.push(b' ');
        }
        candidate.extend_from_slice(word);
        if style.measure(&candidate) <= max_width {
            current = candidate;
            continue;
        }
        if !current.is_empty() {
            lines.push(finish_line(std::mem::take(&mut current), style));
        }
        // the word on its own: split it if even that is too wide
        current = split_long_word(word, style, max_width, lines);
    }
    if !current.is_empty() || lines.len() == first_line {
        // an empty paragraph still takes a line, like a blank line in an editor
        lines.push(finish_line(current, style));
    }
}

/// puts as many letters of a long word on each line as fit, and returns
/// whatever is left for the next line to continue from.
fn split_long_word(
    word: &[u8],
    style: TextStyle,
    max_width: f32,
    lines: &mut Vec<Line>,
) -> Vec<u8> {
    let mut piece = Vec::new();
    for &letter in word {
        piece.push(letter);
        // a line always keeps at least one letter, even in a very narrow box
        if piece.len() > 1 && style.measure(&piece) > max_width {
            piece.pop();
            lines.push(finish_line(std::mem::take(&mut piece), style));
            piece.push(letter);
        }
    }
    piece
}

fn finish_line(encoded: Vec<u8>, style: TextStyle) -> Line {
    Line {
        width: style.measure(&encoded),
        encoded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELVETICA_10: TextStyle = TextStyle {
        font: StandardFont::Helvetica,
        size: 10.0,
    };

    fn texts(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|line| String::from_utf8(line.encoded.clone()).unwrap())
            .collect()
    }

    #[test]
    fn words_move_down_when_the_line_is_full() {
        // "aaa" is 3 x 5.56 = 16.68 points, a space 2.78
        let lines = wrap("aaa aaa aaa", HELVETICA_10, 37.0).unwrap();
        assert_eq!(texts(&lines), ["aaa aaa", "aaa"]);
        assert!((lines[0].width - 36.14).abs() < 1e-3);
        assert!(lines.iter().all(|line| line.width <= 37.0));
    }

    #[test]
    fn exact_fit_stays_on_one_line() {
        let lines = wrap("aaa aaa", HELVETICA_10, 36.14).unwrap();
        assert_eq!(texts(&lines), ["aaa aaa"]);
    }

    #[test]
    fn new_lines_and_blank_lines_are_kept() {
        let lines = wrap("one\r\n\ntwo", HELVETICA_10, 200.0).unwrap();
        assert_eq!(texts(&lines), ["one", "", "two"]);
    }

    #[test]
    fn long_words_are_split_between_letters() {
        // each "m" is 8.33 points, so three fit in 25 points
        let lines = wrap("mmmmmmm", HELVETICA_10, 25.0).unwrap();
        assert_eq!(texts(&lines), ["mmm", "mmm", "m"]);
    }

    #[test]
    fn wider_fonts_wrap_sooner() {
        let text = "the quick brown fox jumps over the lazy dog";
        let narrow = wrap(text, HELVETICA_10, 60.0).unwrap();
        let courier = TextStyle {
            font: StandardFont::Courier,
            size: 10.0,
        };
        // courier fits 10 letters in 60 points, so 5 lines; helvetica needs 4
        let wide = wrap(text, courier, 60.0).unwrap();
        assert_eq!((narrow.len(), wide.len()), (4, 5));
        assert!(wide.iter().all(|line| line.width <= 60.0));
        assert!(narrow.iter().all(|line| line.width <= 60.0));
    }

    #[test]
    fn characters_the_fonts_lack_are_named() {
        let error = wrap("hello 世界", HELVETICA_10, 100.0).unwrap_err();
        assert_eq!(error, AnnotateError::UnsupportedCharacters("世界".into()));
    }

    #[test]
    fn empty_text_is_one_empty_line() {
        assert_eq!(texts(&wrap("", HELVETICA_10, 50.0).unwrap()), [""]);
    }
}
