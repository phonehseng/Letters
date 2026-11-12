//! the text the tools read, as another crate extracted it.
//!
//! this crate never opens a pdf. the caller hands over pages of
//! paragraphs, each with the rectangle it covers, so every answer can
//! point back to a place on a page.

use serde::{Deserialize, Serialize};

use crate::AiError;

/// a rectangle on a page in pdf points, measured from the top left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// one paragraph (or heading, or list item) and where it sits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    pub text: String,
    pub rect: Rect,
}

/// one page, numbered from 1 as people count pages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub number: u32,
    pub paragraphs: Vec<Paragraph>,
}

/// the whole document's text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub pages: Vec<Page>,
}

/// a run of pages, both ends included, numbered from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRange {
    pub first: u32,
    pub last: u32,
}

impl PageRange {
    pub fn new(first: u32, last: u32) -> Self {
        PageRange { first, last }
    }

    pub fn single(page: u32) -> Self {
        PageRange::new(page, page)
    }

    pub fn contains(&self, page: u32) -> bool {
        (self.first..=self.last).contains(&page)
    }
}

impl Document {
    /// the range from the first page to the last.
    pub fn all_pages(&self) -> Option<PageRange> {
        let first = self.pages.iter().map(|page| page.number).min()?;
        let last = self.pages.iter().map(|page| page.number).max()?;
        Some(PageRange::new(first, last))
    }

    /// checks a range the person chose, or uses the whole document.
    pub(crate) fn range_or_all(&self, range: Option<PageRange>) -> Result<PageRange, AiError> {
        let all = self.all_pages().ok_or(AiError::NothingToRead)?;
        let range = range.unwrap_or(all);
        if range.first == 0 || range.first > range.last || range.first > all.last {
            return Err(AiError::InvalidPages);
        }
        Ok(PageRange::new(range.first, range.last.min(all.last)))
    }

    /// the pages inside a range, in page order.
    pub(crate) fn pages_in(&self, range: PageRange) -> impl Iterator<Item = &Page> {
        self.pages
            .iter()
            .filter(move |page| range.contains(page.number))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(pages: u32) -> Document {
        Document {
            pages: (1..=pages)
                .map(|number| Page {
                    number,
                    paragraphs: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn missing_range_means_the_whole_document() {
        assert_eq!(
            document(4).range_or_all(None).unwrap(),
            PageRange::new(1, 4)
        );
    }

    #[test]
    fn a_range_past_the_end_is_trimmed() {
        let range = document(4)
            .range_or_all(Some(PageRange::new(3, 9)))
            .unwrap();
        assert_eq!(range, PageRange::new(3, 4));
    }

    #[test]
    fn impossible_ranges_are_refused() {
        let doc = document(4);
        for range in [
            PageRange::new(0, 2),
            PageRange::new(3, 2),
            PageRange::new(5, 6),
        ] {
            assert_eq!(doc.range_or_all(Some(range)), Err(AiError::InvalidPages));
        }
        assert_eq!(
            Document::default().range_or_all(None),
            Err(AiError::NothingToRead)
        );
    }
}
