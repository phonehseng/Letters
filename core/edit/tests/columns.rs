//! two-column pages: each column's lines make their own paragraphs, even
//! when the gutter is narrower than the gap that splits a line, and an
//! edit in one column leaves the other where it was.

mod support;

use letters_edit::{
    page_objects, reflow_paragraph, replace_text, PageObjects, ReflowOptions, TextBlock,
    TextEditOptions,
};
use support::{load, page_text, render, save, PageSetup, PdfBuilder};

const HELVETICA_RESOURCES: &str = "/Font << /F1 3 0 R >>";
const LEFT_X: f32 = 72.0;
const COLUMN_WIDTH: f32 = 200.0;
const TOP_BASELINE: f32 = 700.0;

const LEFT_WORDS: &str = "water gathers on cold glass in the early morning because the air \
    of a room is warm and damp while the glass is chilled by the night outside and the air \
    next to the pane cools below its dew point and cannot hold all of its moisture so tiny \
    drops form and grow until they run down the window in thin streaks that dry by noon";
const RIGHT_WORDS: &str = "warm lakes lose water in the afternoon when the sun heats the \
    shallow edges first and the deeper middle later so the whole surface steams gently and \
    the wind carries the moist air away so fresh dry air takes its place and the \
    lake keeps giving up water until the evening cools it and the wind drops again";

/// helvetica's widths in thousandths of the font size, for the letters
/// these pages use (from the standard font metrics).
fn helvetica_width(text: &str, size: f32) -> f32 {
    let units: u32 = text
        .chars()
        .map(|char| match char {
            ' ' | 'f' | 't' => 278,
            'i' | 'j' | 'l' => 222,
            'r' => 333,
            'm' => 833,
            'w' => 722,
            'c' | 'k' | 's' | 'v' | 'x' | 'y' | 'z' => 500,
            'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'n' | 'o' | 'p' | 'q' | 'u' => 556,
            other => panic!("no width for {other:?}"),
        })
        .sum();
    units as f32 * size / 1000.0
}

/// fills lines of `width` greedily and justifies all but the last with
/// word spacing, so every full line ends exactly at the column's edge.
fn justified_lines(words: &str, width: f32, size: f32) -> Vec<(String, f32)> {
    let mut lines: Vec<Vec<&str>> = vec![Vec::new()];
    for word in words.split_whitespace() {
        let current = lines.last_mut().unwrap();
        let mut candidate = current.clone();
        candidate.push(word);
        if current.is_empty() || helvetica_width(&candidate.join(" "), size) <= width {
            *current = candidate;
        } else {
            lines.push(vec![word]);
        }
    }
    let last = lines.len() - 1;
    lines
        .into_iter()
        .enumerate()
        .map(|(index, words)| {
            let text = words.join(" ");
            let spaces = (words.len() - 1).max(1) as f32;
            let spread = if index == last {
                0.0
            } else {
                (width - helvetica_width(&text, size)) / spaces
            };
            (text, spread)
        })
        .collect()
}

/// a heading across both columns, then two justified columns drawn the
/// way many writers draw them: across the page, a line of the left
/// column then the same line of the right. the left column is a line
/// longer at 10 pt, so its last line has nothing beside it.
fn two_columns(gutter: f32, size: f32) -> Vec<u8> {
    let leading = 1.4 * size;
    let right_x = LEFT_X + COLUMN_WIDTH + gutter;
    let mut content =
        String::from("BT /F1 18 Tf 72 740 Td (A Heading That Spans Both Columns Here) Tj ET\n");
    let left = justified_lines(LEFT_WORDS, COLUMN_WIDTH, size);
    let right = justified_lines(RIGHT_WORDS, COLUMN_WIDTH, size);
    for row in 0..left.len().max(right.len()) {
        let y = TOP_BASELINE - row as f32 * leading;
        let beside = [(left.get(row), LEFT_X), (right.get(row), right_x)];
        for (line, x) in beside {
            let Some((text, spread)) = line else {
                continue;
            };
            content.push_str(&format!(
                "BT /F1 {size} Tf {spread:.4} Tw {x} {y} Td ({text}) Tj ET\n"
            ));
        }
    }
    let mut pdf = PdfBuilder::new();
    pdf.page(&PageSetup::letter(), &[&content], HELVETICA_RESOURCES);
    pdf.finish()
}

/// the gutters the tests try: (gutter, font size). 24 pt at 10 pt and
/// 30 pt at 13 pt are narrower than the gap that splits a line on its own
/// (2.5 font sizes); 60 pt is wide.
const LAYOUTS: [(f32, f32); 3] = [(24.0, 10.0), (30.0, 13.0), (60.0, 10.0)];

fn column_text(words: &str, size: f32) -> String {
    justified_lines(words, COLUMN_WIDTH, size)
        .into_iter()
        .map(|(text, _)| text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn block_starting<'a>(objects: &'a PageObjects, start: &str) -> &'a TextBlock {
    objects
        .text_blocks
        .iter()
        .find(|block| block.text.starts_with(start))
        .unwrap_or_else(|| panic!("no block starts {start:?}"))
}

#[test]
fn each_column_makes_its_own_paragraph_however_narrow_the_gutter() {
    for (gutter, size) in LAYOUTS {
        let doc = load(&two_columns(gutter, size));
        assert_eq!(
            page_text(&doc, 0),
            [
                "A Heading That Spans Both Columns Here".to_string(),
                column_text(LEFT_WORDS, size),
                column_text(RIGHT_WORDS, size),
            ],
            "gutter {gutter} at {size} pt"
        );
        let objects = page_objects(&doc, 0).unwrap();
        let right = &objects.text_blocks[2];
        let right_lines = justified_lines(RIGHT_WORDS, COLUMN_WIDTH, size).len();
        assert_eq!(right.lines.len(), right_lines);
        let right_x = LEFT_X + COLUMN_WIDTH + gutter;
        for line in &right.lines {
            assert!((line.baseline[0].x - right_x).abs() < 0.01, "{gutter}");
        }
    }
}

#[test]
fn editing_a_left_line_leaves_the_right_column_where_it_was() {
    for (gutter, size) in LAYOUTS {
        let bytes = two_columns(gutter, size);
        let mut doc = load(&bytes);
        let before = page_objects(&doc, 0).unwrap();
        let left_line = &block_starting(&before, "water").lines[2];
        replace_text(
            &mut doc,
            0,
            &left_line.id,
            "an edited line",
            &TextEditOptions::default(),
        )
        .unwrap();
        let saved = save(&mut doc);
        let after = page_objects(&load(&saved), 0).unwrap();
        assert_eq!(after.text_blocks[1].lines[2].text, "an edited line");
        assert_same_right_column(&before, &after, gutter);
        let right_x = (LEFT_X + COLUMN_WIDTH + gutter) as u32;
        assert_eq!(
            render(&bytes, 0).differences_in(&render(&saved, 0), (right_x - 2, 0, 612, 792)),
            0,
            "gutter {gutter}: the right column's pixels are the same"
        );
    }
}

#[test]
fn reflowing_the_left_paragraph_keeps_it_inside_its_column() {
    for (gutter, size) in LAYOUTS {
        let mut doc = load(&two_columns(gutter, size));
        let before = page_objects(&doc, 0).unwrap();
        let left = block_starting(&before, "water");
        let text = left.text.replace('\n', " ").replace("early", "very early");
        let result =
            reflow_paragraph(&mut doc, 0, &left.id, &text, &ReflowOptions::default()).unwrap();
        assert!(result.applied, "{gutter}: {:?}", result.overflow);
        let edited = block_starting(&result.objects, "water");
        assert!(edited.text.replace('\n', " ").contains("very early"));
        assert!(
            edited.rect.right() < LEFT_X + COLUMN_WIDTH + gutter / 2.0,
            "gutter {gutter}: {:?}",
            edited.rect
        );
        let reloaded = page_objects(&load(&save(&mut doc)), 0).unwrap();
        assert_same_right_column(&before, &reloaded, gutter);
    }
}

fn assert_same_right_column(before: &PageObjects, after: &PageObjects, gutter: f32) {
    let (old, new) = (
        block_starting(before, "warm"),
        block_starting(after, "warm"),
    );
    assert_eq!(old.text, new.text, "gutter {gutter}");
    assert_eq!(old.rect, new.rect, "gutter {gutter}");
    for (old, new) in old.lines.iter().zip(&new.lines) {
        assert_eq!(old.baseline, new.baseline, "gutter {gutter}");
    }
}

#[test]
fn justified_text_with_wide_word_spaces_stays_one_paragraph() {
    // 5.22 points of word spacing on top of helvetica's 2.78 point space:
    // every word space is 0.8 of the font size wide
    let lines = [
        "the quick brown fox jumps",
        "over a lazy dog and then",
        "runs back home to sleep",
        "under the old oak tree",
        "until the sun goes down",
    ];
    let mut content = String::from("BT /F1 10 Tf 14 TL 5.22 Tw 72 700 Td\n");
    for line in lines {
        content.push_str(&format!("({line}) Tj T*\n"));
    }
    content.push_str("ET");
    let mut pdf = PdfBuilder::new();
    pdf.page(&PageSetup::letter(), &[&content], HELVETICA_RESOURCES);
    let doc = load(&pdf.finish());
    assert_eq!(page_text(&doc, 0), [lines.join("\n")]);
}

#[test]
fn a_tab_gap_on_one_line_stays_inside_the_line() {
    // 20 points between the label and its value: under the 25 that splits
    // a line, on a line with no column of lines below it
    let content = "BT /F1 10 Tf 72 700 Td (Name:) Tj 48 0 Td (Ada Lovelace) Tj ET\n\
                   BT /F1 10 Tf 72 686 Td (A sentence under the label.) Tj ET";
    let mut pdf = PdfBuilder::new();
    pdf.page(&PageSetup::letter(), &[content], HELVETICA_RESOURCES);
    let doc = load(&pdf.finish());
    assert_eq!(
        page_text(&doc, 0),
        ["Name: Ada Lovelace\nA sentence under the label."]
    );
}
