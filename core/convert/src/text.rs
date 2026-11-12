//! pdf to plain text, in two ways:
//! - reading order: the words as a person reads them, columns one after
//!   the other, paragraphs joined into single lines and separated by a
//!   blank line, words split at line ends rejoined (optional)
//! - layout: every line where it is on the page, in monospace columns,
//!   so tables and indents keep their shape

use std::io::Write;
use std::path::Path;

use letters_ocr::{CancelFlag, Progress};
use serde::{Deserialize, Serialize};

use crate::error::write_failed;
use crate::source::{PageSelection, PdfSource};
use crate::structure::fragments::Fragment;
use crate::structure::model::{runs_text, BlockContent, StructuredDocument};
use crate::structure::{analyze_structure, page_number_map, read_page, StructureOptions};
use crate::ConvertError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextMode {
    #[default]
    ReadingOrder,
    Layout,
}

/// what goes between pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageSeparator {
    /// a blank line, like between paragraphs
    #[default]
    None,
    /// the form feed character, which printers and some editors treat as
    /// a new page
    FormFeed,
    /// a line saying "--- Page 2 ---"
    Line,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextExportOptions {
    pub pages: PageSelection,
    pub mode: TextMode,
    /// rejoin words split with a hyphen at the end of a line
    pub dehyphenate: bool,
    pub page_separator: PageSeparator,
    /// keep headers, footers and page numbers (reading order only; the
    /// layout mode always keeps everything)
    pub include_headers_footers: bool,
}

impl Default for TextExportOptions {
    fn default() -> Self {
        Self {
            pages: PageSelection::All,
            mode: TextMode::ReadingOrder,
            dehyphenate: true,
            page_separator: PageSeparator::None,
            include_headers_footers: false,
        }
    }
}

/// the chosen pages as plain text.
pub fn pdf_to_text(
    source: &PdfSource,
    options: &TextExportOptions,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<String, ConvertError> {
    let pages = match options.mode {
        TextMode::ReadingOrder => {
            let structure = analyze_structure(
                source,
                &StructureOptions {
                    pages: options.pages.clone(),
                    dehyphenate: options.dehyphenate,
                    find_tables: true,
                    find_repeated: !options.include_headers_footers,
                },
                progress,
                cancel,
            )?;
            reading_order_pages(&structure)
        }
        TextMode::Layout => layout_pages(source, &options.pages, progress, cancel)?,
    };
    Ok(join_pages(&pages, options.page_separator))
}

/// writes the text to a file as utf-8.
pub fn write_text_file(
    source: &PdfSource,
    options: &TextExportOptions,
    path: &Path,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<(), ConvertError> {
    let text = pdf_to_text(source, options, progress, cancel)?;
    let mut file = std::fs::File::create(path).map_err(write_failed)?;
    file.write_all(text.as_bytes()).map_err(write_failed)
}

fn join_pages(pages: &[(usize, String)], separator: PageSeparator) -> String {
    let mut out = String::new();
    for (index, (page, text)) in pages.iter().enumerate() {
        if index > 0 {
            match separator {
                PageSeparator::None => out.push_str("\n\n"),
                PageSeparator::FormFeed => out.push_str("\n\u{c}\n"),
                PageSeparator::Line => out.push_str(&format!("\n\n--- Page {} ---\n\n", page + 1)),
            }
        }
        out.push_str(text.trim_end());
    }
    out.push('\n');
    out
}

/// each page's blocks as text, blocks separated by blank lines.
fn reading_order_pages(structure: &StructuredDocument) -> Vec<(usize, String)> {
    structure
        .pages
        .iter()
        .map(|info| {
            let blocks: Vec<String> = structure
                .blocks
                .iter()
                .filter(|block| block.page == info.page)
                .map(|block| block_text(&block.content))
                .filter(|text| !text.trim().is_empty())
                .collect();
            (info.page, blocks.join("\n\n"))
        })
        .collect()
}

fn block_text(content: &BlockContent) -> String {
    match content {
        BlockContent::Heading { runs, .. } | BlockContent::Paragraph { runs } => runs_text(runs),
        BlockContent::Footnote { marker, runs } => format!("{marker} {}", runs_text(runs)),
        BlockContent::List { items } => items
            .iter()
            .map(|item| {
                format!(
                    "{}{} {}",
                    "  ".repeat(usize::from(item.level)),
                    item.marker,
                    runs_text(&item.runs)
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        BlockContent::Table(table) => {
            let mut text = table.text();
            if let Some(caption) = &table.caption {
                text = format!("{}\n{text}", runs_text(caption));
            }
            text
        }
        BlockContent::Figure(figure) => figure
            .caption
            .as_ref()
            .map(|caption| runs_text(caption))
            .unwrap_or_default(),
    }
}

/// each page with its lines placed in monospace columns.
fn layout_pages(
    source: &PdfSource,
    pages: &PageSelection,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<Vec<(usize, String)>, ConvertError> {
    let pages = pages.resolve(source.page_count())?;
    let page_numbers = page_number_map(source);
    let mut out = Vec::new();
    for (done, page) in pages.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(ConvertError::Cancelled);
        }
        let read = read_page(source, *page, &page_numbers)?;
        out.push((*page, layout_text(&read.fragments)));
        progress(Progress {
            page: *page,
            done: done + 1,
            total: pages.len(),
        });
    }
    Ok(out)
}

/// the most blank lines kept between two lines of text
const MAX_BLANK_LINES: usize = 3;

/// places each fragment on a grid of characters: a column is the width
/// of a typical character, a row the usual distance between lines.
pub(crate) fn layout_text(fragments: &[Fragment]) -> String {
    let upright: Vec<&Fragment> = fragments.iter().filter(|f| !f.rotated).collect();
    if upright.is_empty() {
        return String::new();
    }
    let char_width = median(
        upright
            .iter()
            .flat_map(|f| f.glyphs.iter())
            .filter(|g| !g.ch.is_whitespace())
            .map(|g| g.rect.width)
            .filter(|w| *w > 0.1),
    )
    .unwrap_or(6.0)
    .max(1.0);
    let rows = rows_of(&upright);
    let row_height = median(
        rows.windows(2)
            .map(|pair| pair[1].0 - pair[0].0)
            .filter(|d| *d > 1.0),
    )
    .unwrap_or(14.0)
    .max(1.0);
    let left = upright
        .iter()
        .map(|f| f.rect.x)
        .fold(f32::INFINITY, f32::min);

    let mut lines: Vec<String> = Vec::new();
    let mut previous_baseline: Option<f32> = None;
    for (baseline, members) in rows {
        if let Some(before) = previous_baseline {
            let steps = ((baseline - before) / row_height).round() as usize;
            for _ in 1..steps.clamp(1, MAX_BLANK_LINES + 1) {
                lines.push(String::new());
            }
        }
        previous_baseline = Some(baseline);
        lines.push(row_text(&members, left, char_width));
    }
    lines.join("\n")
}

/// fragments grouped by baseline, top to bottom, each row left to right.
fn rows_of<'a>(fragments: &[&'a Fragment]) -> Vec<(f32, Vec<&'a Fragment>)> {
    let mut sorted: Vec<&Fragment> = fragments.to_vec();
    sorted.sort_by(|a, b| a.baseline.total_cmp(&b.baseline));
    let mut rows: Vec<(f32, Vec<&Fragment>)> = Vec::new();
    for fragment in sorted {
        match rows.last_mut() {
            Some((baseline, members))
                if (fragment.baseline - *baseline).abs() < fragment.size * 0.4 =>
            {
                members.push(fragment)
            }
            _ => rows.push((fragment.baseline, vec![fragment])),
        }
    }
    for (_, members) in &mut rows {
        members.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
    }
    rows
}

fn row_text(members: &[&Fragment], left: f32, char_width: f32) -> String {
    let mut line = String::new();
    for fragment in members {
        let column = ((fragment.rect.x - left) / char_width).round().max(0.0) as usize;
        let length = line.chars().count();
        // never run into the text before, always at least one space apart
        let start = if length == 0 {
            column
        } else {
            column.max(length + 1)
        };
        line.extend(std::iter::repeat_n(' ', start - length));
        line.push_str(&fragment.text());
    }
    line.trim_end().to_string()
}

fn median(values: impl Iterator<Item = f32>) -> Option<f32> {
    let mut values: Vec<f32> = values.collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    // the lower middle value: with only two distances, the line spacing
    // is the smaller one and the larger is a gap
    Some(values[(values.len() - 1) / 2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::fragments::tests::fragment;

    #[test]
    fn layout_keeps_columns_and_gaps() {
        let fragments = vec![
            fragment("Name", 72.0, 100.0, 10.0, false),
            fragment("Score", 172.0, 100.0, 10.0, false),
            fragment("Ana", 72.0, 112.0, 10.0, false),
            fragment("9", 172.0, 112.0, 10.0, false),
            fragment("Note", 72.0, 148.0, 10.0, false),
        ];
        // characters are 5 points wide, so 100 points is 20 columns
        assert_eq!(
            layout_text(&fragments),
            "Name                Score\nAna                 9\n\n\nNote"
        );
    }

    #[test]
    fn pages_are_separated_as_chosen() {
        let pages = vec![(0, "one".to_string()), (1, "two".to_string())];
        assert_eq!(join_pages(&pages, PageSeparator::None), "one\n\ntwo\n");
        assert_eq!(
            join_pages(&pages, PageSeparator::FormFeed),
            "one\n\u{c}\ntwo\n"
        );
        assert_eq!(
            join_pages(&pages, PageSeparator::Line),
            "one\n\n--- Page 2 ---\n\ntwo\n"
        );
    }
}
