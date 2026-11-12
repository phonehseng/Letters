//! reads the layout of one page: lines of text, paragraphs, pictures,
//! ruled tables, and running headers and footers, in reading order.

use std::collections::HashSet;

use crate::content::{Mark, PageContent, TextRun};
use crate::geometry::Rect;
use crate::layout::reading_order;
use crate::rewrite::ArtifactKind;

use super::grid::{find_grids, Grid};

/// the top and bottom margin, as a share of the page height, where
/// running headers, footers and page numbers live.
const MARGIN_BAND: f32 = 0.08;

/// pictures smaller than this share of the page are taken as decoration
/// (bullets, rules, little icons).
const TINY_PICTURE: f32 = 0.002;

/// one line of text: runs side by side on the same baseline.
#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub runs: Vec<usize>,
    pub rect: Rect,
    pub size: f32,
    pub bold: bool,
    pub text: String,
}

/// one thing on the page, in reading order.
#[derive(Clone, Debug)]
pub(crate) enum Item {
    /// lines that belong together
    Paragraph(Vec<Line>),
    Figure {
        image: usize,
        rect: Rect,
    },
    /// cells[row][column] holds the runs in that cell
    Table {
        grid: Grid,
        cells: Vec<Vec<Vec<usize>>>,
    },
}

impl Item {
    pub fn rect(&self) -> Rect {
        match self {
            Item::Paragraph(lines) => {
                Rect::union_all(lines.iter().map(|l| &l.rect)).unwrap_or_default()
            }
            Item::Figure { rect, .. } => *rect,
            Item::Table { grid, .. } => grid.rect(),
        }
    }
}

/// everything worked out about one page.
#[derive(Clone, Debug, Default)]
pub(crate) struct PageLayout {
    pub items: Vec<Item>,
    /// text runs that are running headers, footers or page numbers
    pub margin_runs: Vec<(usize, ArtifactKind)>,
    /// pictures taken as decoration: tiny ones, and scans behind text
    pub decorative_images: Vec<usize>,
    /// pictures behind text (the image of a scanned page)
    pub background_images: Vec<usize>,
    /// runs with only spaces, which go with the text around them
    pub blank_runs: Vec<usize>,
}

/// the text of a margin line made comparable between pages: page numbers
/// differ from page to page, so digits are all the same here.
pub(crate) fn margin_key(text: &str) -> String {
    let mut key = String::new();
    for c in text.to_lowercase().chars() {
        // a whole number becomes one '#', however many digits it has
        if c.is_ascii_digit() {
            if !key.ends_with('#') {
                key.push('#');
            }
        } else {
            key.push(c);
        }
    }
    key.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "3", "Page 3", "page 3 of 10", "- 3 -", "iv".
pub(crate) fn is_page_number(text: &str) -> bool {
    let lower = text
        .trim()
        .trim_matches(|c: char| c == '-' || c == '–' || c.is_whitespace())
        .to_lowercase();
    let rest = lower.strip_prefix("page").map(str::trim).unwrap_or(&lower);
    let parts: Vec<&str> = rest
        .split(|c: char| c == '/' || c.is_whitespace())
        .filter(|p| !p.is_empty() && *p != "of")
        .collect();
    let numeric = |p: &&str| p.chars().all(|c| c.is_ascii_digit()) && p.len() <= 5;
    let roman = |p: &&str| p.len() <= 5 && p.chars().all(|c| matches!(c, 'i' | 'v' | 'x'));
    !parts.is_empty()
        && parts.len() <= 2
        && (parts.iter().all(numeric) || (parts.len() == 1 && parts.iter().all(roman)))
}

/// which band a rectangle is in: the header band, the footer band or neither.
pub(crate) fn margin_band(rect: &Rect, page_height: f32) -> Option<ArtifactKind> {
    if rect.bottom() <= page_height * MARGIN_BAND {
        Some(ArtifactKind::Header)
    } else if rect.y >= page_height * (1.0 - MARGIN_BAND) {
        Some(ArtifactKind::Footer)
    } else {
        None
    }
}

/// groups runs into lines: same baseline, close together.
pub(crate) fn group_lines(content: &PageContent, runs: &[usize]) -> Vec<Line> {
    let texts = &content.texts;
    let mut sorted: Vec<usize> = runs.to_vec();
    sorted.sort_by(|&a, &b| texts[a].rect.bottom().total_cmp(&texts[b].rect.bottom()));
    // rows of runs whose baselines are close
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for run in sorted {
        let tolerance = (texts[run].font_size * 0.3).max(1.0);
        match rows.last_mut() {
            Some(row)
                if (texts[row[0]].rect.bottom() - texts[run].rect.bottom()).abs() <= tolerance =>
            {
                row.push(run)
            }
            _ => rows.push(vec![run]),
        }
    }
    let mut lines = Vec::new();
    for mut row in rows {
        row.sort_by(|&a, &b| texts[a].rect.x.total_cmp(&texts[b].rect.x));
        let mut current: Vec<usize> = Vec::new();
        for run in row {
            let starts_new = current.last().is_some_and(|&last: &usize| {
                let gap = texts[run].rect.x - texts[last].rect.right();
                // a wide gap is a column gutter or a tab, not a word space
                gap > texts[last].font_size.max(texts[run].font_size) * 2.0
            });
            if starts_new {
                lines.push(make_line(texts, std::mem::take(&mut current)));
            }
            current.push(run);
        }
        if !current.is_empty() {
            lines.push(make_line(texts, current));
        }
    }
    lines
}

fn make_line(texts: &[TextRun], runs: Vec<usize>) -> Line {
    let rect = Rect::union_all(runs.iter().map(|&r| &texts[r].rect)).unwrap_or_default();
    let size = runs.iter().map(|&r| texts[r].font_size).fold(0.0, f32::max);
    let characters = |r: usize| texts[r].text.trim().chars().count();
    let bold_characters: usize = runs
        .iter()
        .filter(|&&r| texts[r].bold)
        .map(|&r| characters(r))
        .sum();
    let all_characters: usize = runs.iter().map(|&r| characters(r)).sum();
    let mut text = String::new();
    for &run in &runs {
        let piece = texts[run].text.as_str();
        if !text.is_empty() && !text.ends_with(' ') && !piece.starts_with(' ') {
            text.push(' ');
        }
        text.push_str(piece);
    }
    Line {
        runs,
        rect,
        size,
        bold: all_characters > 0 && bold_characters * 2 > all_characters,
        text: text.split_whitespace().collect::<Vec<_>>().join(" "),
    }
}

/// works out the layout of one page. only content for which `eligible`
/// says yes is placed; the rest is left alone. `repeated` holds the
/// margin texts (see `margin_key`) found on more than one page.
pub(crate) fn analyse_page(
    content: &PageContent,
    page_size: (f32, f32),
    eligible: &dyn Fn(Mark) -> bool,
    repeated: &HashSet<String>,
) -> PageLayout {
    let (page_width, page_height) = page_size;
    let page_area = (page_width * page_height).max(1.0);
    let mut layout = PageLayout::default();
    let texts = &content.texts;
    let usable: Vec<usize> = (0..texts.len())
        .filter(|&r| eligible(texts[r].mark))
        .collect();
    let (blank, words): (Vec<usize>, Vec<usize>) = usable
        .into_iter()
        .partition(|&r| texts[r].text.trim().is_empty());
    layout.blank_runs = blank;

    // tables first: their cells' text must not be joined into lines
    let grids = find_grids(&content.paths);
    let mut in_table: HashSet<usize> = HashSet::new();
    let mut tables = Vec::new();
    for grid in grids {
        let mut cells = vec![vec![Vec::new(); grid.column_count()]; grid.row_count()];
        let mut any = false;
        for &run in &words {
            if let Some((row, column)) = grid.cell_at(texts[run].rect.center()) {
                cells[row][column].push(run);
                in_table.insert(run);
                any = true;
            }
        }
        if any {
            tables.push(Item::Table { grid, cells });
        }
    }
    let free: Vec<usize> = words
        .into_iter()
        .filter(|r| !in_table.contains(r))
        .collect();
    let mut lines = Vec::new();
    for line in group_lines(content, &free) {
        match margin_band(&line.rect, page_height) {
            Some(kind)
                if is_page_number(&line.text) || repeated.contains(&margin_key(&line.text)) =>
            {
                layout
                    .margin_runs
                    .extend(line.runs.iter().map(|&r| (r, kind)));
            }
            _ => lines.push(line),
        }
    }

    let mut figures = Vec::new();
    for (index, image) in content.images.iter().enumerate() {
        if !eligible(image.mark) {
            continue;
        }
        let share = image.rect.area() / page_area;
        let behind_text = share >= 0.5
            && lines
                .iter()
                .any(|line| line.rect.intersection(&image.rect).is_some());
        if share < TINY_PICTURE {
            layout.decorative_images.push(index);
        } else if behind_text {
            layout.background_images.push(index);
        } else {
            figures.push(Item::Figure {
                image: index,
                rect: image.rect,
            });
        }
    }

    layout.items = order_and_group(lines, figures, tables);
    layout
}

/// puts lines, figures and tables in reading order and joins lines that
/// belong to one paragraph.
fn order_and_group(lines: Vec<Line>, figures: Vec<Item>, tables: Vec<Item>) -> Vec<Item> {
    enum Piece {
        Line(Line),
        Other(Item),
    }
    let mut pieces: Vec<Piece> = lines.into_iter().map(Piece::Line).collect();
    pieces.extend(figures.into_iter().map(Piece::Other));
    pieces.extend(tables.into_iter().map(Piece::Other));
    let boxes: Vec<Rect> = pieces
        .iter()
        .map(|piece| match piece {
            Piece::Line(line) => line.rect,
            Piece::Other(item) => item.rect(),
        })
        .collect();
    let order = reading_order(&boxes);
    let mut slots: Vec<Option<Piece>> = pieces.into_iter().map(Some).collect();
    let mut items: Vec<Item> = Vec::new();
    for index in order {
        match slots[index].take() {
            Some(Piece::Line(line)) => {
                if let Some(Item::Paragraph(paragraph)) = items.last_mut() {
                    if continues(paragraph.last().unwrap(), &line) {
                        paragraph.push(line);
                        continue;
                    }
                }
                items.push(Item::Paragraph(vec![line]));
            }
            Some(Piece::Other(item)) => items.push(item),
            None => {}
        }
    }
    items
}

/// whether `next` carries on the paragraph that `previous` is part of.
fn continues(previous: &Line, next: &Line) -> bool {
    let size = previous.size.max(next.size).max(1.0);
    let gap = next.rect.y - previous.rect.bottom();
    let same_style = (previous.size - next.size).abs() <= 0.15 * size && previous.bold == next.bold;
    let overlaps = next.rect.x < previous.rect.right() && previous.rect.x < next.rect.right();
    let starts_item = crate::rules::list_label(&next.text).is_some();
    same_style && overlaps && gap >= -0.5 * size && gap <= 0.7 * size && !starts_item
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_numbers_are_recognised() {
        for text in ["3", "Page 3", "page 3 of 10", "- 12 -", "iv", "3 / 10"] {
            assert!(is_page_number(text), "{text}");
        }
        for text in ["Chapter 3", "Introduction", "3 apples", "Page three"] {
            assert!(!is_page_number(text), "{text}");
        }
    }

    #[test]
    fn margin_texts_compare_without_their_numbers() {
        assert_eq!(
            margin_key("Essay on Rivers   — 3"),
            margin_key("essay on rivers — 14")
        );
    }
}
