//! line breaking on made-up text: every letter 6 pt wide and every space
//! 3 pt, so the expected breaks can be worked out by hand.

use super::*;
use crate::fonts::LineMetrics;
use crate::style::{para_style, Tab};
use crate::text::breaks::break_opportunities;
use crate::text::{Cluster, ParagraphText};
use letters_office::model::{Document, Paragraph};

const LETTER: f64 = 6.0;
const SPACE: f64 = 3.0;

fn fake_text(source: &str) -> ParagraphText {
    let breaks = break_opportunities(source);
    let clusters = source
        .chars()
        .zip(breaks)
        .enumerate()
        .map(|(i, (ch, break_before))| Cluster {
            kind: match ch {
                ' ' => ClusterKind::Space,
                '\t' => ClusterKind::Tab,
                '\n' => ClusterKind::LineBreak,
                _ => ClusterKind::Glyphs,
            },
            offset: i as u32,
            units: 1,
            style: 0,
            font: 0,
            glyph_start: 0,
            glyph_count: 0,
            width: match ch {
                ' ' => SPACE,
                '\t' | '\n' => 0.0,
                _ => LETTER,
            },
            break_before,
            generated: false,
            ch,
            char_start: i as u32,
            char_count: 1,
        })
        .collect();
    let metrics = LineMetrics {
        ascent: 8.0,
        descent: 2.0,
    };
    ParagraphText {
        clusters,
        styles: vec![crate::style::run_style_from_props(
            &Default::default(),
            Flavor::Word,
        )],
        style_metrics: vec![metrics],
        mark_metrics: metrics,
        length: source.chars().count() as u32,
        ..Default::default()
    }
}

fn style() -> ParaStyle {
    para_style(&Document::new(), &Paragraph::with_text(""), Flavor::Word)
}

fn line_texts(source: &str, layout: &ParagraphLayout) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    layout
        .lines
        .iter()
        .map(|line| chars[line.start..line.end].iter().collect())
        .collect()
}

#[test]
fn greedy_first_fit() {
    // "aaa bbb ccc": each word 18, space 3. 40 pt holds "aaa bbb" (39)
    let source = "aaa bbb ccc";
    let layout = layout_paragraph(&fake_text(source), &style(), 40.0, Flavor::Word);
    assert_eq!(line_texts(source, &layout), vec!["aaa bbb ", "ccc"]);
}

#[test]
fn trailing_spaces_hang_and_do_not_count() {
    // "aaa bbb   " is 39 pt of content plus hanging spaces: still one line
    let source = "aaa bbb    ccc";
    let layout = layout_paragraph(&fake_text(source), &style(), 39.0, Flavor::Word);
    assert_eq!(line_texts(source, &layout), vec!["aaa bbb    ", "ccc"]);
    assert_eq!(layout.lines[0].width, 39.0);
}

#[test]
fn a_word_exactly_filling_the_line_fits() {
    let source = "aaaaa bb";
    let layout = layout_paragraph(&fake_text(source), &style(), 30.0, Flavor::Word);
    assert_eq!(line_texts(source, &layout), vec!["aaaaa ", "bb"]);
}

#[test]
fn over_long_words_are_cut_between_characters() {
    let source = "aaaaaaaaaa";
    let layout = layout_paragraph(&fake_text(source), &style(), 25.0, Flavor::Word);
    assert_eq!(line_texts(source, &layout), vec!["aaaa", "aaaa", "aa"]);
}

#[test]
fn hyphenated_words_break_after_the_hyphen() {
    let source = "aa well-known";
    let layout = layout_paragraph(&fake_text(source), &style(), 50.0, Flavor::Word);
    assert_eq!(line_texts(source, &layout), vec!["aa well-", "known"]);
}

#[test]
fn line_breaks_end_lines() {
    let source = "aa\nbb cc";
    let layout = layout_paragraph(&fake_text(source), &style(), 500.0, Flavor::Word);
    assert_eq!(line_texts(source, &layout), vec!["aa\n", "bb cc"]);
}

#[test]
fn first_line_indent_and_hanging_indent() {
    let source = "aaa bbb ccc ddd";
    let mut indented = style();
    indented.first_line = 18.0;
    // the first line has 40 - 18 = 22: only "aaa"
    let layout = layout_paragraph(&fake_text(source), &indented, 40.0, Flavor::Word);
    assert_eq!(line_texts(source, &layout), vec!["aaa ", "bbb ccc ", "ddd"]);
    assert_eq!(layout.lines[0].x, 18.0);
    assert_eq!(layout.lines[1].x, 0.0);
    let mut hanging = style();
    hanging.left = 18.0;
    hanging.first_line = -18.0;
    let layout = layout_paragraph(&fake_text(source), &hanging, 58.0, Flavor::Word);
    assert_eq!(layout.lines[0].x, 0.0);
    assert_eq!(layout.lines[1].x, 18.0);
}

#[test]
fn alignment_moves_the_line() {
    let source = "aaa";
    let mut centered = style();
    centered.alignment = Alignment::Center;
    let layout = layout_paragraph(&fake_text(source), &centered, 100.0, Flavor::Word);
    assert_eq!(layout.lines[0].x, 41.0);
    let mut right = style();
    right.alignment = Alignment::Right;
    let layout = layout_paragraph(&fake_text(source), &right, 100.0, Flavor::Word);
    assert_eq!(layout.lines[0].x, 82.0);
}

#[test]
fn justified_lines_stretch_their_spaces_but_not_the_last_line() {
    let source = "aa bb cc dd";
    let mut justified = style();
    justified.alignment = Alignment::Justify;
    // 35 pt: "aa bb cc" is 42, too wide; "aa bb" is 27, slack 8 on one space
    let layout = layout_paragraph(&fake_text(source), &justified, 35.0, Flavor::Word);
    assert_eq!(line_texts(source, &layout), vec!["aa bb ", "cc dd"]);
    assert_eq!(layout.cluster_width[2], SPACE + 8.0);
    assert_eq!(layout.cluster_x[3], 12.0 + SPACE + 8.0);
    // the last line keeps its natural spacing
    assert_eq!(layout.cluster_width[8], SPACE);
}

#[test]
fn default_tabs_go_to_the_next_half_inch() {
    let source = "aa\tbb";
    let layout = layout_paragraph(&fake_text(source), &style(), 300.0, Flavor::Word);
    assert_eq!(layout.cluster_x[3], 36.0);
    assert_eq!(layout.cluster_width[2], 24.0);
}

#[test]
fn right_and_decimal_tabs_align_their_text() {
    let mut tabbed = style();
    tabbed.tabs = vec![
        Tab {
            position: 100.0,
            kind: TabKind::Right,
            leader: TabLeader::Dot,
        },
        Tab {
            position: 200.0,
            kind: TabKind::Decimal,
            leader: TabLeader::None,
        },
    ];
    let source = "a\tbbb\t12.5";
    let layout = layout_paragraph(&fake_text(source), &tabbed, 300.0, Flavor::Word);
    // "bbb" ends at the right stop
    assert_eq!(layout.cluster_x[4] + LETTER, 100.0);
    // the decimal point sits on the decimal stop
    assert_eq!(layout.cluster_x[8], 200.0);
    assert_eq!(
        layout.lines[0].leaders,
        vec![(6.0, 82.0, TabLeader::Dot, 1)]
    );
}

#[test]
fn centre_tabs_centre_their_text() {
    let mut tabbed = style();
    tabbed.tabs = vec![Tab {
        position: 100.0,
        kind: TabKind::Center,
        leader: TabLeader::None,
    }];
    let source = "\taaaa";
    let layout = layout_paragraph(&fake_text(source), &tabbed, 300.0, Flavor::Word);
    assert_eq!(layout.cluster_x[1], 88.0);
}

#[test]
fn empty_paragraphs_have_one_line_of_mark_height() {
    let text = fake_text("");
    let layout = layout_paragraph(&text, &style(), 100.0, Flavor::Word);
    assert_eq!(layout.lines.len(), 1);
    assert_eq!(layout.lines[0].vertical.height, 10.0);
}
