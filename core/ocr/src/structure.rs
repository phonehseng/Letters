//! recognised words gathered into paragraphs, in the order a person
//! would read them, for turning a scan into editable text.
//!
//! tesseract already groups words into lines and paragraphs. what this
//! adds is the reading order of the paragraphs on a page with two
//! columns: the whole left column first, then the right one, with
//! headings and other text that spans both columns kept in their place.

use serde::Serialize;

use crate::geometry::Rect;
use crate::result::{OcrPage, OcrWord};

/// a page's text, paragraph by paragraph.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredText {
    /// 1 or 2
    pub columns: usize,
    /// in reading order
    pub paragraphs: Vec<TextParagraph>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextParagraph {
    /// the lines joined with spaces, words broken by a hyphen at a line's
    /// end joined up again
    pub text: String,
    /// each line's words, as they were on the page
    pub lines: Vec<String>,
    pub rect: Rect,
    /// the typical type size in points, for headings versus body text
    pub font_size: f32,
    /// 0 for text across the whole page, 1 for the left column, 2 for the right
    pub column: usize,
}

/// a paragraph only counts towards finding columns when it is narrower
/// than this share of the page; wider ones are headings or full-width text.
const NARROW_SHARE: f32 = 0.6;

/// the gap between columns is looked for in this middle part of the page.
const GUTTER_SEARCH: (f32, f32) = (0.25, 0.75);

pub fn structured_text(page: &OcrPage) -> StructuredText {
    let mut paragraphs = gather_paragraphs(&page.words);
    let gutter = find_gutter(&paragraphs, page.width);
    match gutter {
        Some(gutter) => {
            for paragraph in &mut paragraphs {
                paragraph.column = column_of(&paragraph.rect, gutter);
            }
            StructuredText {
                columns: 2,
                paragraphs: two_column_order(paragraphs),
            }
        }
        None => {
            paragraphs.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
            StructuredText {
                columns: 1,
                paragraphs,
            }
        }
    }
}

/// words to paragraphs, keeping tesseract's grouping.
fn gather_paragraphs(words: &[OcrWord]) -> Vec<TextParagraph> {
    let mut paragraphs: Vec<(usize, Vec<&OcrWord>)> = Vec::new();
    for word in words {
        match paragraphs.last_mut() {
            Some((number, members)) if *number == word.paragraph => members.push(word),
            _ => paragraphs.push((word.paragraph, vec![word])),
        }
    }
    paragraphs
        .into_iter()
        .map(|(_, members)| paragraph_from(&members))
        .collect()
}

fn paragraph_from(words: &[&OcrWord]) -> TextParagraph {
    let mut lines: Vec<String> = Vec::new();
    let mut previous_line = None;
    for word in words {
        if previous_line == Some(word.line) {
            let line = lines.last_mut().expect("a line was started");
            line.push(' ');
            line.push_str(&word.text);
        } else {
            lines.push(word.text.clone());
        }
        previous_line = Some(word.line);
    }
    let rect = words
        .iter()
        .map(|word| word.rect)
        .reduce(Rect::union)
        .unwrap_or_default();
    let mut sizes: Vec<f32> = words.iter().map(|word| word.font_size).collect();
    sizes.sort_by(f32::total_cmp);
    TextParagraph {
        text: join_lines(&lines),
        lines,
        rect,
        font_size: sizes[sizes.len() / 2],
        column: 0,
    }
}

/// joins lines with spaces, mending words split by a hyphen at a line's
/// end: "exam-" and "ple" become "example". before a capital the hyphen is
/// part of the word ("Jean-" and "Paul" become "Jean-Paul").
fn join_lines(lines: &[String]) -> String {
    let mut text = String::new();
    for line in lines {
        let split_word = text.ends_with('-')
            && text[..text.len() - 1].ends_with(|letter: char| letter.is_alphabetic());
        if split_word {
            if line.starts_with(|letter: char| letter.is_lowercase()) {
                text.pop();
            }
        } else if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(line);
    }
    text
}

/// the x position of an empty band between two columns, or none for a page
/// with one column.
fn find_gutter(paragraphs: &[TextParagraph], page_width: f32) -> Option<f32> {
    let narrow: Vec<&Rect> = paragraphs
        .iter()
        .map(|paragraph| &paragraph.rect)
        .filter(|rect| rect.width < page_width * NARROW_SHARE)
        .collect();
    let (from, to) = (page_width * GUTTER_SEARCH.0, page_width * GUTTER_SEARCH.1);
    let crosses = |x: f32| narrow.iter().any(|rect| rect.x < x && rect.right() > x);
    // the widest run of positions, a point apart, that no narrow paragraph crosses
    let mut best: Option<(f32, f32)> = None;
    let mut run_start: Option<f32> = None;
    let mut x = from;
    while x <= to {
        if crosses(x) {
            run_start = None;
        } else {
            let start = *run_start.get_or_insert(x);
            if best.is_none_or(|(a, b)| x - start > b - a) {
                best = Some((start, x));
            }
        }
        x += 1.0;
    }
    let (start, end) = best?;
    let gutter = (start + end) / 2.0;
    // two columns means narrow text on both sides, side by side somewhere
    let left: Vec<&&Rect> = narrow
        .iter()
        .filter(|rect| rect.right() <= gutter)
        .collect();
    let right: Vec<&&Rect> = narrow.iter().filter(|rect| rect.x >= gutter).collect();
    let side_by_side = left
        .iter()
        .any(|a| right.iter().any(|b| a.y < b.bottom() && b.y < a.bottom()));
    side_by_side.then_some(gutter)
}

fn column_of(rect: &Rect, gutter: f32) -> usize {
    if rect.right() <= gutter {
        1
    } else if rect.x >= gutter {
        2
    } else {
        0
    }
}

/// reading order on a two column page: text across the whole page cuts it
/// into bands; within each band, the left column then the right.
fn two_column_order(mut paragraphs: Vec<TextParagraph>) -> Vec<TextParagraph> {
    paragraphs.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
    let mut ordered = Vec::with_capacity(paragraphs.len());
    let mut band: Vec<TextParagraph> = Vec::new();
    let flush = |band: &mut Vec<TextParagraph>, ordered: &mut Vec<TextParagraph>| {
        // sorting is stable, so each column keeps its top to bottom order
        band.sort_by_key(|paragraph| paragraph.column);
        ordered.append(band);
    };
    for paragraph in paragraphs {
        if paragraph.column == 0 {
            flush(&mut band, &mut ordered);
            ordered.push(paragraph);
        } else {
            band.push(paragraph);
        }
    }
    flush(&mut band, &mut ordered);
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use crate::result::Baseline;

    fn word(text: &str, rect: Rect, line: usize, paragraph: usize) -> OcrWord {
        OcrWord {
            text: text.into(),
            rect,
            baseline: Baseline {
                start: Point::new(rect.x, rect.bottom()),
                end: Point::new(rect.right(), rect.bottom()),
            },
            font_size: rect.height,
            confidence: 90.0,
            line,
            paragraph,
            block: paragraph,
            language: None,
        }
    }

    fn page(words: Vec<OcrWord>) -> OcrPage {
        OcrPage {
            width: 600.0,
            height: 800.0,
            words,
            language: "eng".into(),
            skew_angle: 0.0,
            orientation: 0,
            mean_confidence: 90.0,
            steps: Vec::new(),
        }
    }

    #[test]
    fn one_column_reads_top_to_bottom_and_mends_hyphens() {
        let words = vec![
            word("an", Rect::new(50.0, 100.0, 20.0, 10.0), 0, 0),
            word("exam-", Rect::new(80.0, 100.0, 40.0, 10.0), 0, 0),
            word("ple", Rect::new(50.0, 115.0, 30.0, 10.0), 1, 0),
            word("Next", Rect::new(50.0, 200.0, 40.0, 10.0), 2, 1),
        ];
        let text = structured_text(&page(words));
        assert_eq!(text.columns, 1);
        assert_eq!(text.paragraphs[0].text, "an example");
        assert_eq!(text.paragraphs[0].lines, ["an exam-", "ple"]);
        assert_eq!(text.paragraphs[1].text, "Next");
    }

    #[test]
    fn two_columns_read_left_then_right_below_a_heading() {
        let words = vec![
            // a heading across the page
            word("Title", Rect::new(50.0, 50.0, 500.0, 20.0), 0, 0),
            // tesseract may list the columns interleaved; order must not depend on it
            word("left-top", Rect::new(50.0, 100.0, 220.0, 10.0), 1, 1),
            word("right-top", Rect::new(330.0, 100.0, 220.0, 10.0), 2, 2),
            word("left-bottom", Rect::new(50.0, 300.0, 220.0, 10.0), 3, 3),
            word("right-bottom", Rect::new(330.0, 300.0, 220.0, 10.0), 4, 4),
            // a footer across the page
            word("Footer", Rect::new(50.0, 700.0, 500.0, 10.0), 5, 5),
        ];
        let text = structured_text(&page(words));
        assert_eq!(text.columns, 2);
        let order: Vec<&str> = text
            .paragraphs
            .iter()
            .map(|paragraph| paragraph.text.as_str())
            .collect();
        assert_eq!(
            order,
            [
                "Title",
                "left-top",
                "left-bottom",
                "right-top",
                "right-bottom",
                "Footer"
            ]
        );
        assert_eq!(text.paragraphs[1].column, 1);
        assert_eq!(text.paragraphs[3].column, 2);
    }

    #[test]
    fn a_narrow_paragraph_alone_is_not_a_column() {
        let words = vec![
            word("short", Rect::new(50.0, 100.0, 200.0, 10.0), 0, 0),
            word("also", Rect::new(50.0, 200.0, 200.0, 10.0), 1, 1),
        ];
        assert_eq!(structured_text(&page(words)).columns, 1);
    }

    #[test]
    fn a_capitalised_part_after_a_hyphen_keeps_it() {
        assert_eq!(join_lines(&["Jean-".into(), "Paul".into()]), "Jean-Paul");
    }
}
