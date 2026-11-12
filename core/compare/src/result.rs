//! what a comparison finds, in the shape the ui receives it as json.
//!
//! the result is designed for the two ways the ui shows a comparison:
//! - side by side: each change has rectangles on the old page and on the
//!   new page and one id, so the ui can draw a connector between them, and
//!   each page pair has a scroll map so both sides scroll together
//! - overlay: each page pair knows which pages it compares, and
//!   `overlay_image` / `aligned_images` draw them on top of each other
//!
//! page numbers are 0-based, like everywhere else in the api; the `detail`
//! sentences count from 1, the way people do.

use serde::{Deserialize, Serialize};

use crate::geometry::{PageSize, Rect};
use crate::source::Colour;

/// everything one comparison found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonResult {
    pub left_page_count: u32,
    pub right_page_count: u32,
    /// the pages of both documents matched up, in reading order of the new one
    pub page_pairs: Vec<PagePair>,
    pub summary: Summary,
    /// things the person should know about the comparison as a whole
    pub notes: Vec<String>,
}

/// an old page and the new page it became, or a page only one side has.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagePair {
    /// the old document's page, none when the page was added
    pub left_page: Option<u32>,
    /// the new document's page, none when the page was removed
    pub right_page: Option<u32>,
    pub left_size: Option<PageSize>,
    pub right_size: Option<PageSize>,
    /// how alike the two pages' text is, from 0 (nothing shared) to 1
    pub similarity: f32,
    pub changes: Vec<Change>,
    /// for side by side scrolling: matching heights on the two pages, in
    /// points from the top, sorted by `left_y`. use `map_scroll` to look one up
    pub scroll_map: Vec<ScrollPoint>,
    /// how far the new page's content sits from the old page's, in points on
    /// the visible pages. the overlay and the picture comparison line the
    /// pages up by this much
    pub offset: Offset,
    /// things worth knowing about this pair, such as "compared as a picture"
    pub notes: Vec<String>,
}

impl PagePair {
    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offset {
    pub dx: f32,
    pub dy: f32,
}

/// one matching height on the old and new page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollPoint {
    pub left_y: f32,
    pub right_y: f32,
}

/// one difference between the two documents.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// unique in the result, so the ui can link the two sides of a change
    pub id: u32,
    pub kind: ChangeKind,
    pub category: Category,
    /// the old page the left rectangles are on. usually the pair's left
    /// page; a moved passage may come from another one
    pub left_page: Option<u32>,
    /// the new page the right rectangles are on
    pub right_page: Option<u32>,
    pub left_rects: Vec<Rect>,
    pub right_rects: Vec<Rect>,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
    /// one plain sentence, such as "Font size changed from 11 to 12"
    pub detail: String,
    /// for formatting changes: the style before and after
    pub style: Option<StyleChange>,
}

/// what sort of difference a change is. see README.md for each in plain words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    /// text that only the new version has
    #[default]
    Inserted,
    /// text that only the old version has
    Deleted,
    /// text that was rewritten
    Replaced,
    /// the same passage, found in a different place
    Moved,
    /// the same text in a different font, size, weight, slant or colour
    Restyled,
    /// the same text in a different position
    Shifted,
    /// the same paragraph broken into lines differently
    Reflowed,
    /// a page of a different size or orientation
    Resized,
    /// a picture or drawing that looks different
    Redrawn,
    /// a page only the new version has
    PageAdded,
    /// a page only the old version has
    PageRemoved,
    /// a page that is in a different position
    PageMoved,
}

/// the group a change belongs to, for the summary and the filters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    #[default]
    Text,
    Formatting,
    Layout,
    Image,
    Page,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Category::Text,
        Category::Formatting,
        Category::Layout,
        Category::Image,
        Category::Page,
    ];

    /// the name people see, as in the summary table.
    pub fn label(&self) -> &'static str {
        match self {
            Category::Text => "Text",
            Category::Formatting => "Formatting",
            Category::Layout => "Layout",
            Category::Image => "Pictures and drawings",
            Category::Page => "Pages",
        }
    }
}

/// how text looked: the parts a formatting change can be about.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    /// the font family, without bold or italic ("Helvetica", "TimesNewRoman")
    pub font: String,
    /// in points, to one decimal place
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub colour: Colour,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleChange {
    pub old: TextStyle,
    pub new: TextStyle,
}

/// how many changes of each kind were found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub text: u32,
    pub formatting: u32,
    pub layout: u32,
    pub image: u32,
    pub page: u32,
    pub total: u32,
    /// differences only in spacing or hyphenation, which are not reported
    /// unless asked for (see `CompareOptions::ignore_spacing`)
    pub ignored: u32,
    pub page_pairs: u32,
    pub page_pairs_with_changes: u32,
}

impl Summary {
    pub fn count(&self, category: Category) -> u32 {
        match category {
            Category::Text => self.text,
            Category::Formatting => self.formatting,
            Category::Layout => self.layout,
            Category::Image => self.image,
            Category::Page => self.page,
        }
    }

    fn add(&mut self, category: Category) {
        match category {
            Category::Text => self.text += 1,
            Category::Formatting => self.formatting += 1,
            Category::Layout => self.layout += 1,
            Category::Image => self.image += 1,
            Category::Page => self.page += 1,
        }
        self.total += 1;
    }

    /// counts the changes of a finished list of page pairs.
    pub(crate) fn of(pairs: &[PagePair], ignored: u32) -> Summary {
        let mut summary = Summary {
            ignored,
            page_pairs: pairs.len() as u32,
            ..Summary::default()
        };
        for pair in pairs {
            if pair.has_changes() {
                summary.page_pairs_with_changes += 1;
            }
            for change in &pair.changes {
                summary.add(change.category);
            }
        }
        summary
    }
}

/// the height on the new page that matches `left_y` on the old page,
/// between the nearest known points of the scroll map.
pub fn map_scroll(map: &[ScrollPoint], left_y: f32) -> f32 {
    let Some(first) = map.first() else {
        return left_y;
    };
    if left_y <= first.left_y {
        return first.right_y + (left_y - first.left_y);
    }
    for pair in map.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if left_y <= b.left_y {
            let span = b.left_y - a.left_y;
            if span <= f32::EPSILON {
                return b.right_y;
            }
            let along = (left_y - a.left_y) / span;
            return a.right_y + along * (b.right_y - a.right_y);
        }
    }
    let last = map[map.len() - 1];
    last.right_y + (left_y - last.left_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_maps_interpolate_between_points() {
        let map = [
            ScrollPoint {
                left_y: 0.0,
                right_y: 0.0,
            },
            ScrollPoint {
                left_y: 100.0,
                right_y: 120.0,
            },
            ScrollPoint {
                left_y: 792.0,
                right_y: 792.0,
            },
        ];
        assert_eq!(map_scroll(&map, 50.0), 60.0);
        assert_eq!(map_scroll(&map, 100.0), 120.0);
        assert!((map_scroll(&map, 446.0) - 456.0).abs() < 0.01);
        assert_eq!(map_scroll(&[], 33.0), 33.0);
    }

    #[test]
    fn names_are_camel_case_in_json() {
        let change = Change {
            kind: ChangeKind::PageAdded,
            category: Category::Image,
            ..Change::default()
        };
        let json = serde_json::to_string(&change).unwrap();
        assert!(json.contains("\"kind\":\"pageAdded\""), "{json}");
        assert!(json.contains("\"category\":\"image\""), "{json}");
        assert!(json.contains("\"leftRects\""), "{json}");
        assert!(json.contains("\"oldText\""), "{json}");
    }
}
