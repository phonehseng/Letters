//! splitting one document into several.

use lopdf::{Document, ObjectId};
use serde::{Deserialize, Serialize};

use crate::combine::extract_positions;
use crate::links::{read_text_string, NamedDestinations};
use crate::outline;
use crate::tree::{check_page, page_ids};
use crate::PagesError;

/// the longest a name suffix taken from a bookmark title may be.
const MAX_SUFFIX_CHARS: usize = 60;

/// a run of pages from `start` to `end`, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageSpan {
    pub start: u32,
    pub end: u32,
}

/// how to split, for `split`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SplitParams {
    /// one file per range. ranges may overlap.
    ByRanges { ranges: Vec<PageSpan> },
    /// a new file every so many pages; the last one may be shorter
    EveryNPages { pages_per_part: u32 },
    /// one file per top level bookmark (a chapter), running up to the
    /// page before the next chapter. pages before the first chapter make
    /// a file of their own.
    ByBookmarks,
    /// files no bigger than this many bytes, filled greedily in page order
    ByMaxSize { max_bytes: u64 },
}

/// one of the files a split makes.
pub struct SplitPart {
    pub document: Document,
    /// a short text for the file name, such as "pages 1-4" or a chapter
    /// title. the ui adds it to the original name
    pub name_suffix: String,
    /// the first page this part holds, as an index in the original
    pub first_page: u32,
    pub page_count: u32,
    /// set when a single page is already bigger than the size limit. it
    /// gets a file of its own, and the ui should say why it is too big
    pub over_size_limit: bool,
}

/// splits a document into new ones. the original is not changed.
pub fn split(doc: &Document, params: &SplitParams) -> Result<Vec<SplitPart>, PagesError> {
    let pages = page_ids(doc)?;
    if pages.is_empty() {
        return Err(PagesError::NoPagesChosen);
    }
    match params {
        SplitParams::ByRanges { ranges } => by_ranges(doc, &pages, ranges),
        SplitParams::EveryNPages { pages_per_part } => every_n_pages(doc, &pages, *pages_per_part),
        SplitParams::ByBookmarks => by_bookmarks(doc, &pages),
        SplitParams::ByMaxSize { max_bytes } => by_max_size(doc, &pages, *max_bytes),
    }
}

/// a part holding the pages from `start` to `end`, both included.
fn part(
    doc: &Document,
    pages: &[ObjectId],
    start: usize,
    end: usize,
    name_suffix: String,
) -> SplitPart {
    let positions: Vec<usize> = (start..=end).collect();
    SplitPart {
        document: extract_positions(doc, pages, &positions),
        name_suffix,
        first_page: start as u32,
        page_count: (end - start + 1) as u32,
        over_size_limit: false,
    }
}

/// "page 3" or "pages 3-7", counting from 1 the way people do.
fn pages_suffix(start: usize, end: usize) -> String {
    if start == end {
        format!("page {}", start + 1)
    } else {
        format!("pages {}-{}", start + 1, end + 1)
    }
}

fn by_ranges(
    doc: &Document,
    pages: &[ObjectId],
    ranges: &[PageSpan],
) -> Result<Vec<SplitPart>, PagesError> {
    if ranges.is_empty() {
        return Err(PagesError::NoPagesChosen);
    }
    // every range is checked before any part is made
    for range in ranges {
        check_page(range.start, pages.len())?;
        check_page(range.end, pages.len())?;
        if range.start > range.end {
            return Err(PagesError::BackwardsRange(range.start + 1, range.end + 1));
        }
    }
    Ok(ranges
        .iter()
        .map(|range| {
            let (start, end) = (range.start as usize, range.end as usize);
            part(doc, pages, start, end, pages_suffix(start, end))
        })
        .collect())
}

fn every_n_pages(
    doc: &Document,
    pages: &[ObjectId],
    pages_per_part: u32,
) -> Result<Vec<SplitPart>, PagesError> {
    if pages_per_part == 0 {
        return Err(PagesError::PartSizeZero);
    }
    let size = pages_per_part as usize;
    Ok((0..pages.len())
        .step_by(size)
        .map(|start| {
            let end = (start + size).min(pages.len()) - 1;
            part(doc, pages, start, end, pages_suffix(start, end))
        })
        .collect())
}

fn by_bookmarks(doc: &Document, pages: &[ObjectId]) -> Result<Vec<SplitPart>, PagesError> {
    let chapters = chapter_starts(doc, pages);
    if chapters.is_empty() {
        return Err(PagesError::NoBookmarks);
    }
    let mut parts = Vec::new();
    if chapters[0].0 > 0 {
        let end = chapters[0].0 - 1;
        parts.push(part(doc, pages, 0, end, "start".to_owned()));
    }
    for (index, (start, title)) in chapters.iter().enumerate() {
        let end = chapters
            .get(index + 1)
            .map_or(pages.len() - 1, |(next, _)| next - 1);
        let suffix = file_name_safe(title).unwrap_or_else(|| format!("chapter {}", index + 1));
        parts.push(part(doc, pages, *start, end, suffix));
    }
    Ok(parts)
}

/// the first page of each top level bookmark with its title, in page
/// order. when two chapters start on the same page the first one wins,
/// since the other would be empty.
fn chapter_starts(doc: &Document, pages: &[ObjectId]) -> Vec<(usize, String)> {
    let names = NamedDestinations::read(doc);
    let mut chapters: Vec<(usize, String)> = outline::read(doc, &names)
        .into_iter()
        .filter_map(|entry| {
            let page = entry.target?.page;
            let position = pages.iter().position(|&candidate| candidate == page)?;
            let title = entry
                .title
                .as_str()
                .map(read_text_string)
                .unwrap_or_default();
            Some((position, title))
        })
        .collect();
    chapters.sort_by_key(|(position, _)| *position);
    chapters.dedup_by_key(|(position, _)| *position);
    chapters
}

/// a bookmark title made safe to put in a file name, or none if nothing
/// usable is left.
fn file_name_safe(title: &str) -> Option<String> {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || r#"\/:*?"<>|"#.contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned: String = cleaned.chars().take(MAX_SUFFIX_CHARS).collect();
    let cleaned = cleaned.trim_end_matches(['.', ' ']).to_owned();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// the size a document takes once saved.
fn saved_size(doc: &mut Document) -> Result<u64, PagesError> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|error| PagesError::Internal(error.to_string()))?;
    Ok(bytes.len() as u64)
}

/// a part and its saved size.
fn measured(
    doc: &Document,
    pages: &[ObjectId],
    start: usize,
    end: usize,
) -> Result<(SplitPart, u64), PagesError> {
    let mut candidate = part(doc, pages, start, end, pages_suffix(start, end));
    let size = saved_size(&mut candidate.document)?;
    Ok((candidate, size))
}

/// fills each part with as many pages as fit under the limit.
///
/// trying one page more at a time would save the growing part once per
/// page, which is slow for long documents. instead the part doubles in
/// length until it is too big, and then a binary search finds the exact
/// longest one that fits. that relies on a longer run of pages never
/// being smaller once saved, which holds because each extra page only
/// adds objects.
fn by_max_size(
    doc: &Document,
    pages: &[ObjectId],
    max_bytes: u64,
) -> Result<Vec<SplitPart>, PagesError> {
    if max_bytes == 0 {
        return Err(PagesError::SizeLimitZero);
    }
    let mut parts = Vec::new();
    let mut start = 0;
    while start < pages.len() {
        let (first, size) = measured(doc, pages, start, start)?;
        if size > max_bytes {
            parts.push(SplitPart {
                over_size_limit: true,
                ..first
            });
            start += 1;
            continue;
        }
        let (best, end) = longest_fitting(doc, pages, start, max_bytes, first)?;
        parts.push(best);
        start = end + 1;
    }
    Ok(parts)
}

/// the longest part from `start` that fits, given that the single page
/// `one_page` does. returns it with the index of its last page.
fn longest_fitting(
    doc: &Document,
    pages: &[ObjectId],
    start: usize,
    max_bytes: u64,
    one_page: SplitPart,
) -> Result<(SplitPart, usize), PagesError> {
    let mut best = (one_page, start);
    // the first end known to be too big (or one past the last page)
    let mut too_big = pages.len();
    let mut step = 1;
    while start + step < pages.len() {
        let end = start + step;
        let (candidate, size) = measured(doc, pages, start, end)?;
        if size > max_bytes {
            too_big = end;
            break;
        }
        best = (candidate, end);
        step *= 2;
    }
    let (mut low, mut high) = (best.1 + 1, too_big);
    while low < high {
        let middle = (low + high) / 2;
        let (candidate, size) = measured(doc, pages, start, middle)?;
        if size <= max_bytes {
            best = (candidate, middle);
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_become_safe_file_name_parts() {
        assert_eq!(
            file_name_safe("Part 1: Intro / Overview"),
            Some("Part 1 Intro Overview".into())
        );
        assert_eq!(file_name_safe("  ...  "), None);
        assert_eq!(
            file_name_safe("Ends with a dot."),
            Some("Ends with a dot".into())
        );
    }

    #[test]
    fn page_suffixes_count_from_one() {
        assert_eq!(pages_suffix(0, 0), "page 1");
        assert_eq!(pages_suffix(2, 6), "pages 3-7");
    }

    #[test]
    fn split_settings_read_from_the_ui() {
        let json = r#"{"mode":"everyNPages","pagesPerPart":3}"#;
        let params: SplitParams = serde_json::from_str(json).unwrap();
        assert_eq!(params, SplitParams::EveryNPages { pages_per_part: 3 });
    }
}
