//! sections: page size, margins, columns and which headers and footers
//! apply. a new section can start on a new page or carry on the same one.

use serde::{Deserialize, Serialize};

use super::block::{Block, Preserved};
use super::Twips;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    #[serde(default)]
    pub page: PageSetup,
    #[serde(default)]
    pub columns: Columns,
    /// how this section begins relative to the one before it
    #[serde(default)]
    pub start: SectionStart,
    /// header ids by page kind. a missing one carries on the previous
    /// section's header, as in word
    #[serde(default)]
    pub headers: HeaderFooterRefs,
    #[serde(default)]
    pub footers: HeaderFooterRefs,
    /// the first page uses the `first` header and footer
    #[serde(default)]
    pub different_first_page: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_number_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Preserved::is_empty")]
    pub preserved: Preserved,
    /// columns of their own widths, left to right. empty: `columns.count`
    /// equal columns `columns.space` apart
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_widths: Vec<ColumnWidth>,
    /// a canvas page: everything on it is positioned (text boxes, shapes,
    /// pictures, floating tables) and no text flows. made by
    /// `freeform::canvas_page_from_objects` for content that came from a
    /// fixed layout (a pdf page, a slide, a flyer)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub canvas: bool,
}

impl Default for Section {
    fn default() -> Self {
        Section {
            page: PageSetup::default(),
            columns: Columns::default(),
            start: SectionStart::NextPage,
            headers: HeaderFooterRefs::default(),
            footers: HeaderFooterRefs::default(),
            different_first_page: false,
            page_number_start: None,
            preserved: Preserved::default(),
            column_widths: Vec::new(),
            canvas: false,
        }
    }
}

impl Section {
    /// each column's width and the gap after it, worked out for equal
    /// columns when the section does not list them.
    pub fn column_layout(&self) -> Vec<ColumnWidth> {
        if !self.column_widths.is_empty() {
            return self.column_widths.clone();
        }
        let count = self.columns.count.max(1) as i32;
        let gaps = self.columns.space * (count - 1);
        let width = ((self.page.text_width() - gaps) / count).max(1);
        (0..count)
            .map(|index| ColumnWidth {
                width,
                space: if index + 1 < count {
                    self.columns.space
                } else {
                    0
                },
            })
            .collect()
    }
}

/// one column of unequal columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnWidth {
    pub width: Twips,
    /// the gap after this column (before the next one)
    #[serde(default)]
    pub space: Twips,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageSetup {
    pub width: Twips,
    pub height: Twips,
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default)]
    pub margins: Margins,
}

impl Default for PageSetup {
    /// us letter, the size most student papers are set on
    fn default() -> Self {
        PageSetup {
            width: 12240,
            height: 15840,
            orientation: Orientation::Portrait,
            margins: Margins::default(),
        }
    }
}

impl PageSetup {
    /// the width text can use between the left and right margins.
    pub fn text_width(&self) -> Twips {
        (self.width - self.margins.left - self.margins.right - self.margins.gutter).max(0)
    }

    pub fn text_height(&self) -> Twips {
        (self.height - self.margins.top - self.margins.bottom).max(0)
    }

    pub const A4: PageSetup = PageSetup {
        width: 11906,
        height: 16838,
        orientation: Orientation::Portrait,
        margins: Margins::ONE_INCH,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Margins {
    pub top: Twips,
    pub right: Twips,
    pub bottom: Twips,
    pub left: Twips,
    /// distance from the page edge to the header's top
    #[serde(default)]
    pub header: Twips,
    #[serde(default)]
    pub footer: Twips,
    #[serde(default)]
    pub gutter: Twips,
}

impl Margins {
    pub const ONE_INCH: Margins = Margins {
        top: 1440,
        right: 1440,
        bottom: 1440,
        left: 1440,
        header: 720,
        footer: 720,
        gutter: 0,
    };
}

impl Default for Margins {
    fn default() -> Self {
        Margins::ONE_INCH
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Columns {
    pub count: u32,
    /// gap between columns
    #[serde(default)]
    pub space: Twips,
    /// a line between columns
    #[serde(default)]
    pub separator: bool,
}

impl Default for Columns {
    fn default() -> Self {
        Columns {
            count: 1,
            space: 720,
            separator: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SectionStart {
    #[default]
    NextPage,
    Continuous,
    EvenPage,
    OddPage,
    NextColumn,
}

/// which header (or footer) each kind of page uses, by `HeaderFooter` id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderFooterRefs {
    /// every page, or odd pages when even pages have their own
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub even: Option<String>,
}

impl HeaderFooterRefs {
    pub fn is_empty(&self) -> bool {
        self.default.is_none() && self.first.is_none() && self.even.is_none()
    }

    /// the ids in a fixed order: default, first, even.
    pub fn ids(&self) -> impl Iterator<Item = &String> {
        [&self.default, &self.first, &self.even]
            .into_iter()
            .flatten()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderFooter {
    pub id: String,
    pub kind: HeaderFooterKind,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeaderFooterKind {
    Header,
    Footer,
}
