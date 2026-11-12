//! the shape of a page: size, margins, columns, and which header and
//! footer it shows.

use letters_office::model::{Document, HeaderFooterRefs, Section};

use crate::units::twips_to_points;

/// a section's page in points.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionGeometry {
    pub width: f64,
    pub height: f64,
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
    pub header_distance: f64,
    pub footer_distance: f64,
    pub columns: u32,
    pub column_gap: f64,
    pub column_separator: bool,
}

impl SectionGeometry {
    pub fn of(section: &Section) -> SectionGeometry {
        let page = &section.page;
        let margins = &page.margins;
        SectionGeometry {
            width: twips_to_points(page.width.max(144)),
            height: twips_to_points(page.height.max(144)),
            // a negative top margin means "exactly, even if the header is
            // taller" in word; the distance is what matters for layout
            top: twips_to_points(margins.top.abs()),
            bottom: twips_to_points(margins.bottom.abs()),
            // the gutter is added to the left margin (no mirrored margins)
            left: twips_to_points(margins.left + margins.gutter.max(0)),
            right: twips_to_points(margins.right),
            header_distance: twips_to_points(margins.header.max(0)),
            footer_distance: twips_to_points(margins.footer.max(0)),
            columns: section.columns.count.clamp(1, 16),
            column_gap: twips_to_points(section.columns.space.max(0)),
            column_separator: section.columns.separator,
        }
    }

    /// the width text can use between the margins.
    pub fn text_width(&self) -> f64 {
        (self.width - self.left - self.right).max(18.0)
    }

    /// each column's left edge (page x) and width.
    pub fn columns(&self) -> Vec<(f64, f64)> {
        let count = f64::from(self.columns);
        let width = ((self.text_width() - self.column_gap * (count - 1.0)) / count).max(18.0);
        (0..self.columns)
            .map(|i| (self.left + f64::from(i) * (width + self.column_gap), width))
            .collect()
    }

    pub fn column_width(&self) -> f64 {
        self.columns()[0].1
    }
}

/// which header or footer a page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageKind {
    First,
    Even,
    Default,
}

pub fn page_kind(
    document: &Document,
    section: &Section,
    first_of_section: bool,
    number: u32,
) -> PageKind {
    if first_of_section && section.different_first_page {
        PageKind::First
    } else if document.settings.even_and_odd_headers && number.is_multiple_of(2) {
        PageKind::Even
    } else {
        PageKind::Default
    }
}

/// the header (or footer) id for a page kind. a section that does not
/// name one carries the previous section's on, as word does.
pub fn part_id(
    sections: &[Section],
    section: usize,
    kind: PageKind,
    footer: bool,
) -> Option<String> {
    if sections.is_empty() {
        return None;
    }
    for index in (0..=section.min(sections.len() - 1)).rev() {
        let refs: &HeaderFooterRefs = if footer {
            &sections[index].footers
        } else {
            &sections[index].headers
        };
        let found = match kind {
            PageKind::First => refs.first.clone(),
            PageKind::Even => refs.even.clone(),
            PageKind::Default => refs.default.clone(),
        };
        if found.is_some() {
            return found;
        }
        // a first-page header asked for but never given is empty in word
        if kind == PageKind::First && index == section && !refs.is_empty() {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_page_with_one_inch_margins() {
        let geometry = SectionGeometry::of(&Section::default());
        assert_eq!(geometry.width, 612.0);
        assert_eq!(geometry.text_width(), 468.0);
        assert_eq!(geometry.columns(), vec![(72.0, 468.0)]);
    }

    #[test]
    fn two_columns_share_the_width_less_the_gap() {
        let mut section = Section::default();
        section.columns.count = 2;
        section.columns.space = 720;
        let geometry = SectionGeometry::of(&section);
        assert_eq!(geometry.columns(), vec![(72.0, 216.0), (324.0, 216.0)]);
    }

    #[test]
    fn headers_carry_on_from_earlier_sections() {
        let mut first = Section::default();
        first.headers.default = Some("h1".into());
        let second = Section::default();
        let sections = vec![first, second];
        assert_eq!(
            part_id(&sections, 1, PageKind::Default, false),
            Some("h1".into())
        );
        assert_eq!(part_id(&sections, 1, PageKind::Default, true), None);
    }
}
