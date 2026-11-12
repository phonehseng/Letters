//! page size and margins for each run of pages, and the headers and
//! footers the analysis found repeated across pages.
//!
//! a pdf has no margins as such, only where the text happens to sit. the
//! margins here are read from the text's extent on those pages, kept
//! within sensible limits, so the flowing document lays out like the pdf.

use letters_convert::{Block, PageInfo, RepeatedKind, RepeatedText, StructuredDocument};
use letters_office::model::{
    Block as OfficeBlock, Columns, Field, HeaderFooter, HeaderFooterKind, Inline, Margins,
    Orientation, PageSetup, Paragraph, Section, Twips,
};

/// points to twips (a twentieth of a point).
pub fn twips(points: f32) -> Twips {
    (points * 20.0).round() as Twips
}

/// margins never come out smaller than a quarter inch or larger than two
/// inches, whatever the text extent suggests.
const SMALLEST_MARGIN_PT: f32 = 18.0;
const LARGEST_MARGIN_PT: f32 = 144.0;
/// the margin of a page with no text at all
const USUAL_MARGIN_PT: f32 = 72.0;

/// a run of consecutive pages that share their size and column count.
#[derive(Clone, Debug, PartialEq)]
pub struct PageRun {
    /// zero based, both ends included
    pub first: usize,
    pub last: usize,
    pub width: f32,
    pub height: f32,
    pub columns: usize,
}

/// groups the pages into runs; each run becomes one section.
pub fn page_runs(pages: &[PageInfo]) -> Vec<PageRun> {
    let mut runs: Vec<PageRun> = Vec::new();
    for page in pages {
        let same = runs.last().is_some_and(|run| {
            (run.width - page.width).abs() < 1.0
                && (run.height - page.height).abs() < 1.0
                && run.columns == page.columns.max(1)
        });
        match runs.last_mut() {
            Some(run) if same => run.last = page.page,
            _ => runs.push(PageRun {
                first: page.page,
                last: page.page,
                width: page.width,
                height: page.height,
                columns: page.columns.max(1),
            }),
        }
    }
    runs
}

/// the section for one run of pages. `header_footer` holds the ids of the
/// header and footer every section shares, if there are any.
pub fn section_for(run: &PageRun, blocks: &[Block], header_footer: &HeaderFooterIds) -> Section {
    let margins = margins_for(run, blocks);
    let mut section = Section {
        page: PageSetup {
            width: twips(run.width),
            height: twips(run.height),
            orientation: if run.width > run.height {
                Orientation::Landscape
            } else {
                Orientation::Portrait
            },
            margins,
        },
        ..Section::default()
    };
    if run.columns > 1 {
        section.columns = Columns {
            count: run.columns as u32,
            ..Columns::default()
        };
    }
    section.headers.default = header_footer.header.clone();
    section.footers.default = header_footer.footer.clone();
    section
}

fn margins_for(run: &PageRun, blocks: &[Block]) -> Margins {
    let on_pages: Vec<&Block> = blocks
        .iter()
        .filter(|block| (run.first..=run.last).contains(&block.page))
        .collect();
    let extent = |pick: fn(&Block) -> f32, fold: fn(f32, f32) -> f32, start: f32| {
        on_pages.iter().map(|block| pick(block)).fold(start, fold)
    };
    let left = extent(|b| b.rect.x, f32::min, f32::MAX);
    let top = extent(|b| b.rect.y, f32::min, f32::MAX);
    let right = extent(|b| b.rect.x + b.rect.width, f32::max, f32::MIN);
    let bottom = extent(|b| b.rect.y + b.rect.height, f32::max, f32::MIN);
    let side = |gap: f32| {
        if on_pages.is_empty() || !gap.is_finite() {
            twips(USUAL_MARGIN_PT)
        } else {
            twips(gap.clamp(SMALLEST_MARGIN_PT, LARGEST_MARGIN_PT))
        }
    };
    Margins {
        top: side(top),
        left: side(left),
        right: side(run.width - right),
        bottom: side(run.height - bottom),
        header: twips(36.0),
        footer: twips(36.0),
        gutter: 0,
    }
}

/// the ids of the header and footer parts, when the document has them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeaderFooterIds {
    pub header: Option<String>,
    pub footer: Option<String>,
}

/// the parts for repeated text: the most certain header and footer, each
/// on every page. a page number becomes a field that counts by itself.
pub fn header_footer_parts(document: &StructuredDocument) -> (Vec<HeaderFooter>, HeaderFooterIds) {
    let mut parts = Vec::new();
    let mut ids = HeaderFooterIds::default();
    for kind in [RepeatedKind::Header, RepeatedKind::Footer] {
        let Some(repeated) = most_certain(&document.repeated, kind) else {
            continue;
        };
        let (id, office_kind) = match kind {
            RepeatedKind::Header => ("letters-header", HeaderFooterKind::Header),
            RepeatedKind::Footer => ("letters-footer", HeaderFooterKind::Footer),
        };
        let paragraph = Paragraph {
            content: repeated_content(repeated, document.pages.len()),
            ..Paragraph::default()
        };
        parts.push(HeaderFooter {
            id: id.into(),
            kind: office_kind,
            blocks: vec![OfficeBlock::Paragraph(paragraph)],
        });
        match kind {
            RepeatedKind::Header => ids.header = Some(id.into()),
            RepeatedKind::Footer => ids.footer = Some(id.into()),
        }
    }
    (parts, ids)
}

fn most_certain(repeated: &[RepeatedText], kind: RepeatedKind) -> Option<&RepeatedText> {
    repeated
        .iter()
        .filter(|item| item.kind == kind)
        .max_by(|a, b| a.confidence.total_cmp(&b.confidence))
}

/// the repeated text, with its page number (and page count, as in "Page
/// 3 of 9") made into fields.
pub fn repeated_content(repeated: &RepeatedText, page_count: usize) -> Vec<Inline> {
    if !repeated.page_number {
        return vec![Inline::text(&repeated.text)];
    }
    let first_page = repeated.pages.first().map_or(1, |page| page + 1);
    let mut content = Vec::new();
    let mut numbered = false;
    for piece in split_numbers(&repeated.text) {
        match piece.parse::<usize>() {
            Ok(number) if !numbered && number == first_page => {
                numbered = true;
                content.push(Inline::Field(Field::page_number()));
            }
            Ok(number) if numbered && number == page_count => {
                content.push(Inline::Field(Field::page_count()));
            }
            _ => content.push(Inline::text(&piece)),
        }
    }
    letters_office::model::merge_adjacent_text(content)
}

/// "Page 3 of 9" as ["Page ", "3", " of ", "9"].
fn split_numbers(text: &str) -> Vec<String> {
    let mut pieces: Vec<String> = Vec::new();
    for ch in text.chars() {
        let digit = ch.is_ascii_digit();
        let same_kind = pieces
            .last()
            .and_then(|piece| piece.chars().last())
            .is_some_and(|last| last.is_ascii_digit() == digit);
        match pieces.last_mut() {
            Some(piece) if same_kind => piece.push(ch),
            _ => pieces.push(ch.to_string()),
        }
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_convert::Rect;

    fn page(index: usize, width: f32) -> PageInfo {
        PageInfo {
            page: index,
            width,
            height: 792.0,
            columns: 1,
        }
    }

    #[test]
    fn pages_of_one_size_make_one_section() {
        let runs = page_runs(&[page(0, 612.0), page(1, 612.0), page(2, 842.0)]);
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].first, runs[0].last), (0, 1));
        assert_eq!((runs[1].first, runs[1].last), (2, 2));
    }

    #[test]
    fn page_numbers_become_fields() {
        let repeated = RepeatedText {
            kind: RepeatedKind::Footer,
            text: "Page 1 of 3".into(),
            page_number: true,
            pages: vec![0, 1, 2],
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            confidence: 0.9,
        };
        let content = repeated_content(&repeated, 3);
        assert_eq!(content.len(), 4);
        assert!(matches!(&content[1], Inline::Field(field) if field.instruction == "PAGE"));
        assert!(matches!(&content[3], Inline::Field(field) if field.instruction == "NUMPAGES"));
    }

    #[test]
    fn margins_follow_the_text_but_stay_sensible() {
        let run = PageRun {
            first: 0,
            last: 0,
            width: 612.0,
            height: 792.0,
            columns: 1,
        };
        let block = |x: f32, y: f32, w: f32, h: f32| Block {
            page: 0,
            rect: Rect::new(x, y, w, h),
            confidence: 1.0,
            content: letters_convert::BlockContent::Paragraph { runs: vec![] },
        };
        let margins = margins_for(
            &run,
            &[
                block(90.0, 72.0, 432.0, 20.0),
                block(90.0, 700.0, 432.0, 85.0),
            ],
        );
        assert_eq!(margins.left, twips(90.0));
        assert_eq!(margins.right, twips(90.0));
        assert_eq!(margins.top, twips(72.0));
        // the text reaches 7 points from the bottom edge: a quarter inch at least
        assert_eq!(margins.bottom, twips(SMALLEST_MARGIN_PT));
    }
}
