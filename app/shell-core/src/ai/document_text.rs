//! the text the ai tools read, from the pdf's structure.
//!
//! the ai engine never opens a pdf: it is given pages of paragraphs, each
//! with the rectangle it covers, so every answer can point back to a
//! place on a page. the convert engine's structure analysis already finds
//! paragraphs, headings, list items and tables in reading order, with
//! their boxes, so this adapter only reshapes it.

use letters_ai::{Document, Page, Paragraph, Rect as AiRect};
use letters_convert::{Block, BlockContent, Rect, StructuredDocument};

/// the document for the ai engine: every page, numbered from 1, even the
/// ones without text (so page ranges mean what the person typed).
pub fn ai_document(structure: &StructuredDocument) -> Document {
    let mut pages: Vec<Page> = structure
        .pages
        .iter()
        .map(|info| Page {
            number: info.page as u32 + 1,
            paragraphs: Vec::new(),
        })
        .collect();
    for block in &structure.blocks {
        let Some(page) = pages
            .iter_mut()
            .find(|page| page.number == block.page as u32 + 1)
        else {
            continue;
        };
        page.paragraphs.extend(paragraphs_of(block));
    }
    Document { pages }
}

/// one paragraph for most blocks; one per item for a list, so a citation
/// can outline the very item it quotes.
fn paragraphs_of(block: &Block) -> Vec<Paragraph> {
    match &block.content {
        BlockContent::List { items } => items
            .iter()
            .map(|item| paragraph(letters_convert::runs_text(&item.runs), item.rect))
            .filter(has_words)
            .collect(),
        BlockContent::Table(table) => table_rows(block, table),
        _ => [paragraph(block.text(), block.rect)]
            .into_iter()
            .filter(has_words)
            .collect(),
    }
}

/// a table row by row, cells separated by " | ", each row with its own box
/// from its cells.
fn table_rows(block: &Block, table: &letters_convert::Table) -> Vec<Paragraph> {
    (0..table.rows)
        .filter_map(|row| {
            let cells: Vec<&letters_convert::TableCell> =
                table.cells.iter().filter(|cell| cell.row == row).collect();
            let text = cells
                .iter()
                .map(|cell| cell.text())
                .collect::<Vec<_>>()
                .join(" | ");
            let rect = cells
                .iter()
                .map(|cell| cell.rect)
                .reduce(union)
                .unwrap_or(block.rect);
            Some(paragraph(text, rect)).filter(has_words)
        })
        .collect()
}

fn paragraph(text: String, rect: Rect) -> Paragraph {
    Paragraph {
        text: text.split_whitespace().collect::<Vec<_>>().join(" "),
        rect: AiRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        },
    }
}

fn has_words(paragraph: &Paragraph) -> bool {
    !paragraph.text.trim().is_empty()
}

fn union(a: Rect, b: Rect) -> Rect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rect::new(left, top, right - left, bottom - top)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::easy_edit::tests::handbook;

    #[test]
    fn every_page_is_there_and_numbered_from_one() {
        let document = ai_document(&handbook());
        let numbers: Vec<u32> = document.pages.iter().map(|page| page.number).collect();
        assert_eq!(numbers, vec![1, 2]);
    }

    #[test]
    fn list_items_and_table_rows_are_paragraphs_of_their_own() {
        let document = ai_document(&handbook());
        let first: Vec<&str> = document.pages[0]
            .paragraphs
            .iter()
            .map(|p| p.text.as_str())
            .collect();
        assert!(first.contains(&"Skim first"));
        assert!(first.contains(&"Look at headings"));
        let second: Vec<&str> = document.pages[1]
            .paragraphs
            .iter()
            .map(|p| p.text.as_str())
            .collect();
        assert!(second.contains(&"Item | Cost"), "{second:?}");
        assert!(second.contains(&"Books | $40.00"));
    }

    #[test]
    fn rectangles_are_kept_for_jumping_to_the_place() {
        let document = ai_document(&handbook());
        let title = &document.pages[0].paragraphs[0];
        assert_eq!(title.text, "Study Handbook");
        assert_eq!(title.rect.x, 72.0);
        assert_eq!(title.rect.y, 72.0);
    }

    #[test]
    fn the_offline_finder_works_on_it() {
        let document = ai_document(&handbook());
        let found = letters_ai::find_where_offline(&document, "headings", None, 3).unwrap();
        assert_eq!(found[0].page, 1);
        assert!(found[0].text.contains("headings"));
    }
}
