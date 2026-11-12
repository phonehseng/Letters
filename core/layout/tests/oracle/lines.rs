//! groups the glyphs of a pdf page into lines, the unit every score uses.
//!
//! a line is a run of glyphs in content order that moves forward along one
//! baseline. a raised or lowered glyph (a footnote number, a superscript)
//! stays in its line when it is close to the baseline; the line's baseline
//! is the one most of its glyphs share.

use super::pdf_glyphs::{Glyph, PdfPage};

#[derive(Clone, Debug)]
pub struct TextLine {
    pub page: usize,
    pub baseline: f64,
    pub x: f64,
    pub size: f64,
    /// the line's text without spaces, normalised (see `normalise`)
    pub text: String,
    /// pen x of each char of `text`
    pub xs: Vec<f64>,
}

/// characters the two oracles write differently for the same thing: word
/// writes symbol-font bullets in the private use area, libreoffice may
/// keep a ligature as one character.
pub fn normalise(ch: char, out: &mut String) {
    match ch {
        '\u{f0b7}' | '\u{2022}' | '\u{00b7}' | '\u{f0a7}' | '\u{25aa}' | '\u{f0fc}' => {
            out.push('•')
        }
        '\u{f06f}' | '\u{25e6}' => out.push('o'),
        '\u{fb00}' => out.push_str("ff"),
        '\u{fb01}' => out.push_str("fi"),
        '\u{fb02}' => out.push_str("fl"),
        '\u{fb03}' => out.push_str("ffi"),
        '\u{fb04}' => out.push_str("ffl"),
        '\u{2019}' | '\u{2018}' => out.push('\''),
        '\u{201c}' | '\u{201d}' => out.push('"'),
        '\u{2013}' | '\u{2014}' | '\u{2011}' | '\u{00ad}' => out.push('-'),
        c if ('\u{f020}'..='\u{f0ff}').contains(&c) => {
            out.push(char::from_u32(c as u32 - 0xf000).unwrap_or(c))
        }
        c => out.push(c),
    }
}

/// splits a page's glyphs into lines.
pub fn lines_of_page(page_index: usize, page: &PdfPage) -> Vec<TextLine> {
    let mut groups: Vec<Vec<&Glyph>> = Vec::new();
    for glyph in &page.glyphs {
        let joins = groups.last().is_some_and(|line| continues(line, glyph));
        if joins {
            groups.last_mut().expect("checked").push(glyph);
        } else {
            groups.push(vec![glyph]);
        }
    }
    groups
        .into_iter()
        .map(|glyphs| finish_line(page_index, &glyphs))
        .collect()
}

/// a glyph continues a line when it sits to the right of the previous one
/// (not more than a few ems away) and near the line's baseline.
fn continues(line: &[&Glyph], glyph: &Glyph) -> bool {
    let last = line.last().expect("lines are never empty");
    let base = main_baseline(line);
    let size = last.size.max(glyph.size).max(1.0);
    // superscripts (note numbers) sit up to about half the size above
    let close_vertically = (glyph.y - base).abs() < 0.6 * size;
    let forward = glyph.x > last.x - 0.5 && glyph.x - last.x < 12.0 * size;
    close_vertically && forward
}

fn main_baseline(line: &[&Glyph]) -> f64 {
    // the baseline of the largest glyph group: superscripts are smaller
    let biggest = line.iter().map(|g| g.size).fold(0.0, f64::max);
    line.iter()
        .filter(|g| g.size >= biggest - 0.01)
        .map(|g| g.y)
        .next()
        .unwrap_or(line[0].y)
}

/// runs of three or more leader characters count as one leader: the
/// number of dots an application draws is not what layout is scored on.
pub fn collapse_leaders(line: &mut TextLine) {
    let chars: Vec<char> = line.text.chars().collect();
    let mut text = String::new();
    let mut xs = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        let mut end = index;
        while end < chars.len() && chars[end] == ch {
            end += 1;
        }
        if matches!(ch, '.' | '-' | '_' | '\u{b7}') && end - index >= 3 {
            text.push('\u{2026}');
            xs.push(line.xs[index]);
        } else {
            text.extend(&chars[index..end]);
            xs.extend_from_slice(&line.xs[index..end]);
        }
        index = end;
    }
    line.text = text;
    line.xs = xs;
}

fn finish_line(page: usize, glyphs: &[&Glyph]) -> TextLine {
    let baseline = main_baseline(glyphs);
    let mut text = String::new();
    let mut xs = Vec::new();
    for glyph in glyphs {
        let before = text.chars().count();
        normalise(glyph.ch, &mut text);
        for _ in before..text.chars().count() {
            xs.push(glyph.x);
        }
    }
    let mut line = TextLine {
        page,
        baseline,
        x: glyphs.iter().map(|g| g.x).fold(f64::INFINITY, f64::min),
        size: glyphs.iter().map(|g| g.size).fold(0.0, f64::max),
        text,
        xs,
    };
    collapse_leaders(&mut line);
    drop_line_end_hyphen(&mut line);
    line
}

/// pdfium reports a hyphen that ends a line in word's pdfs as generated
/// text (as if it were a soft hyphen) and the reader skips it, while ours
/// keeps it: "students twenty-" read as "studentstwenty" against
/// "studentstwenty-". lines are compared without it on every side.
fn drop_line_end_hyphen(line: &mut TextLine) {
    if line.text.ends_with('-') {
        line.text.pop();
        line.xs.pop();
    }
}

/// every line of a document, page by page.
pub fn lines_of_document(pages: &[PdfPage]) -> Vec<TextLine> {
    pages
        .iter()
        .enumerate()
        .flat_map(|(index, page)| lines_of_page(index, page))
        .collect()
}
