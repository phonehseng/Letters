//! finding text to redact: a phrase, or a kind of text students often need
//! to hide (email addresses, phone numbers, long numbers such as student,
//! id or card numbers).
//!
//! the positions come from the same content walk that redaction uses, so a
//! box offered here covers exactly the glyphs redaction will remove.

use lopdf::Document;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::error::RedactError;
use crate::geometry::Rect;
use crate::page::{page_ids, PageFrame, PageRect};
use crate::redact::{collect_glyphs, ensure_unlocked, walker::GlyphRecord};
use crate::text_layout::{lay_out, LaidOutText, PlacedGlyph};

/// what to look for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum TextQuery {
    /// a word or phrase. white space in it matches any white space,
    /// including a line break
    Phrase {
        text: String,
        match_case: bool,
    },
    /// a regular expression, for people who know them
    Pattern {
        pattern: String,
        match_case: bool,
    },
    EmailAddresses,
    PhoneNumbers,
    /// runs of 8 or more digits, which may be split by single spaces or
    /// dashes: id numbers, student numbers, card numbers
    LongNumbers,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FindOptions {
    /// zero based pages to search; all pages when empty
    pub pages: Vec<u32>,
}

/// one place the query was found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundText {
    pub page: u32,
    pub text: String,
    /// one box per line the match covers, in points from the visible top
    /// left of the page
    pub rects: Vec<PageRect>,
}

/// the most matches returned, so a pattern like "e" cannot produce a
/// list nobody could use.
pub const MAX_FOUND: usize = 5_000;

pub fn find_text_areas(
    doc: &Document,
    query: &TextQuery,
    options: &FindOptions,
) -> Result<Vec<FoundText>, RedactError> {
    ensure_unlocked(doc)?;
    let matcher = Matcher::new(query)?;
    let pages = page_ids(doc);
    let wanted: Vec<u32> = if options.pages.is_empty() {
        (0..pages.len() as u32).collect()
    } else {
        options.pages.clone()
    };
    let mut found = Vec::new();
    for page in wanted {
        let Some(&page_id) = pages.get(page as usize) else {
            return Err(RedactError::UnknownPage(RedactError::page_number(page)));
        };
        let glyphs = collect_glyphs(doc, page_id).map_err(|reason| {
            RedactError::UnreadablePage(RedactError::page_number(page), reason)
        })?;
        let frame = PageFrame::of_page(doc, page_id);
        found.extend(find_on_page(page, &glyphs, &frame, &matcher));
        if found.len() >= MAX_FOUND {
            found.truncate(MAX_FOUND);
            break;
        }
    }
    Ok(found)
}

struct Matcher {
    regex: Regex,
    /// numbers must not be part of a longer run of digits
    digits_alone: bool,
    /// phone numbers must have a phone number's count of digits
    digit_count: Option<(usize, usize)>,
}

impl Matcher {
    fn new(query: &TextQuery) -> Result<Matcher, RedactError> {
        let (pattern, case_insensitive, digits_alone, digit_count) = match query {
            TextQuery::Phrase { text, match_case } => {
                let words: Vec<String> = text.split_whitespace().map(regex::escape).collect();
                if words.is_empty() {
                    return Err(RedactError::InvalidPattern(
                        "there is nothing to look for".into(),
                    ));
                }
                (words.join(r"\s+"), !match_case, false, None)
            }
            TextQuery::Pattern {
                pattern,
                match_case,
            } => (pattern.clone(), !match_case, false, None),
            TextQuery::EmailAddresses => (
                r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)*\.[A-Za-z]{2,}".to_owned(),
                false,
                false,
                None,
            ),
            TextQuery::PhoneNumbers => (
                r"(?:\+\d{1,3}[ .\-]?)?(?:\(\d{1,4}\)[ .\-]?)?\d{2,4}(?:[ .\-]?\d{2,4}){1,4}"
                    .to_owned(),
                false,
                true,
                Some((7, 15)),
            ),
            TextQuery::LongNumbers => (r"\d(?:[ \-]?\d){7,}".to_owned(), false, true, None),
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(case_insensitive)
            .size_limit(1 << 22)
            .build()
            .map_err(|error| RedactError::InvalidPattern(error.to_string()))?;
        Ok(Matcher {
            regex,
            digits_alone,
            digit_count,
        })
    }

    fn accepts(&self, text: &str, start: usize, end: usize) -> bool {
        let found = &text[start..end];
        if self.digits_alone {
            let before = text[..start].chars().next_back();
            let after = text[end..].chars().next();
            if before.is_some_and(|c| c.is_ascii_digit())
                || after.is_some_and(|c| c.is_ascii_digit())
            {
                return false;
            }
        }
        if let Some((low, high)) = self.digit_count {
            let digits = found.chars().filter(char::is_ascii_digit).count();
            if digits < low || digits > high {
                return false;
            }
        }
        !found.trim().is_empty()
    }
}

fn find_on_page(
    page: u32,
    glyphs: &[GlyphRecord],
    frame: &PageFrame,
    matcher: &Matcher,
) -> Vec<FoundText> {
    let placed: Vec<PlacedGlyph> = glyphs
        .iter()
        .map(|g| PlacedGlyph {
            text: &g.text,
            quad: &g.quad,
        })
        .collect();
    let laid_out = lay_out(&placed);
    let mut found = Vec::new();
    for hit in matcher.regex.find_iter(&laid_out.text) {
        if !matcher.accepts(&laid_out.text, hit.start(), hit.end()) {
            continue;
        }
        let rects = match_rects(&laid_out, glyphs, frame, hit.start(), hit.end());
        if !rects.is_empty() {
            found.push(FoundText {
                page,
                text: hit.as_str().to_owned(),
                rects,
            });
        }
    }
    found
}

/// the boxes of the glyphs a match covers, one per line, in visible
/// page coordinates.
fn match_rects(
    laid_out: &LaidOutText,
    glyphs: &[GlyphRecord],
    frame: &PageFrame,
    start: usize,
    end: usize,
) -> Vec<PageRect> {
    let first_char = laid_out.text[..start].chars().count();
    let char_count = laid_out.text[start..end].chars().count();
    let mut lines: Vec<(usize, Rect)> = Vec::new();
    let mut last_glyph = None;
    for glyph in laid_out.glyph_of_char[first_char..first_char + char_count]
        .iter()
        .flatten()
    {
        // a glyph that stands for several letters (a ligature) is counted once
        if last_glyph == Some(*glyph) {
            continue;
        }
        last_glyph = Some(*glyph);
        let line = laid_out.line_of_glyph[*glyph];
        let bounds = glyphs[*glyph].quad.bounds();
        match lines.iter_mut().find(|(number, _)| *number == line) {
            Some((_, rect)) => *rect = rect.union(&bounds),
            None => lines.push((line, bounds)),
        }
    }
    lines
        .iter()
        .map(|(_, rect)| frame.to_visible(rect))
        .collect()
}
