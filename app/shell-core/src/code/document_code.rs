//! finds code inside a word document or a pdf, for "Check Code in This
//! Document": runs of monospaced text, code paragraph styles, and fenced
//! ``` blocks typed into ordinary paragraphs.
//!
//! in a pdf only the drawn characters are known, so a block is a run of
//! lines in a monospaced font, and each line's indent is rebuilt from how
//! far right it starts. hyphens at line ends are kept as they are: in code
//! a trailing "-" is part of the code, not a split word.

use letters_edit::{FontFamily, PageObjects};
use letters_office::model::{Block, Document};
use serde::Serialize;

use super::export::simple::{is_code_paragraph, is_monospaced_font, plain_text};

/// a monospaced character is about 0.6 of the font size wide (courier,
/// consolas and menlo all are); used when a line has no character boxes
const MONOSPACE_ADVANCE_PER_SIZE: f32 = 0.6;

/// one stretch of code found in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundCode {
    pub text: String,
    /// "page 3" or "paragraph 12", for the problem list
    pub location: String,
    /// 1-based, for going there
    pub page: Option<u32>,
    pub paragraph: Option<u32>,
    /// why it was taken for code: "monospaced font", "code style", "fenced block"
    pub found_by: String,
}

/// every code paragraph run and fenced block of a word document.
pub fn code_in_office_document(document: &Document) -> Vec<FoundCode> {
    let mut found = Vec::new();
    let mut open: Option<(FoundCode, bool)> = None;
    for (index, block) in document.blocks.iter().enumerate() {
        let Some(paragraph) = block.paragraph() else {
            flush(&mut open, &mut found);
            continue;
        };
        let text = plain_text(&paragraph.content);
        let number = index as u32 + 1;
        if let Some((code, true)) = open.as_mut() {
            // inside a fenced block every paragraph is code until the closing fence
            if text.trim_start().starts_with("```") {
                let (code, _) = open.take().unwrap();
                found.push(code);
            } else {
                if !code.text.is_empty() {
                    code.text.push('\n');
                }
                code.text.push_str(&text);
            }
            continue;
        }
        if text.trim_start().starts_with("```") {
            flush(&mut open, &mut found);
            open = Some((new_found(String::new(), number, "fenced block"), true));
            continue;
        }
        if is_code_paragraph(document, paragraph) && matches!(block, Block::Paragraph(_)) {
            match open.as_mut() {
                Some((code, false)) => {
                    code.text.push('\n');
                    code.text.push_str(&text);
                }
                _ => {
                    open = Some((
                        new_found(text, number, found_by_style(document, paragraph)),
                        false,
                    ))
                }
            }
        } else {
            flush(&mut open, &mut found);
        }
    }
    flush(&mut open, &mut found);
    found
}

fn found_by_style(
    document: &Document,
    paragraph: &letters_office::model::Paragraph,
) -> &'static str {
    let has_code_style = paragraph
        .style_id
        .as_deref()
        .and_then(|id| document.style(id))
        .is_some_and(|style| !style.name.is_empty() && is_code_style_name(&style.name));
    if has_code_style {
        "code style"
    } else {
        "monospaced font"
    }
}

fn is_code_style_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("code") || name.contains("preformatted")
}

fn new_found(text: String, paragraph: u32, found_by: &str) -> FoundCode {
    FoundCode {
        text,
        location: format!("paragraph {paragraph}"),
        page: None,
        paragraph: Some(paragraph),
        found_by: found_by.to_owned(),
    }
}

fn flush(open: &mut Option<(FoundCode, bool)>, found: &mut Vec<FoundCode>) {
    if let Some((code, _)) = open.take() {
        if !code.text.trim().is_empty() {
            found.push(code);
        }
    }
}

/// one drawn line of a pdf page, reduced to what code detection needs.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfLine {
    pub text: String,
    /// points from the page's left edge to the line's first character
    pub x: f32,
    /// the width of one character, for turning the indent into spaces
    pub character_width: f32,
    pub monospaced: bool,
}

/// the lines of one page, in the order the page draws them.
pub fn pdf_lines(objects: &PageObjects) -> Vec<PdfLine> {
    let mut lines = Vec::new();
    for block in &objects.text_blocks {
        for line in &block.lines {
            let visible_runs: Vec<_> = line.runs.iter().filter(|run| !run.invisible).collect();
            if visible_runs.is_empty() || line.text.trim().is_empty() {
                continue;
            }
            let monospaced = visible_runs.iter().all(|run| {
                run.font.family == FontFamily::Monospace || is_monospaced_font(&run.font.name)
            });
            lines.push(PdfLine {
                text: line.text.clone(),
                x: line.rect.x,
                character_width: character_width(&visible_runs),
                monospaced,
            });
        }
    }
    lines
}

fn character_width(runs: &[&letters_edit::TextRun]) -> f32 {
    let boxes: Vec<f32> = runs
        .iter()
        .flat_map(|run| run.char_boxes.iter().map(|rect| rect.width))
        .filter(|width| *width > 0.0)
        .collect();
    if boxes.is_empty() {
        let size = runs.first().map(|run| run.size).unwrap_or(10.0);
        return size * MONOSPACE_ADVANCE_PER_SIZE;
    }
    boxes.iter().sum::<f32>() / boxes.len() as f32
}

/// consecutive monospaced lines of a page joined into blocks, each line
/// indented by how far right of the block's leftmost line it starts.
pub fn code_in_pdf_page(lines: &[PdfLine], page: u32) -> Vec<FoundCode> {
    let mut found = Vec::new();
    let mut run: Vec<&PdfLine> = Vec::new();
    for line in lines {
        if line.monospaced {
            run.push(line);
        } else {
            push_pdf_block(&mut run, page, &mut found);
        }
    }
    push_pdf_block(&mut run, page, &mut found);
    found
}

fn push_pdf_block(run: &mut Vec<&PdfLine>, page: u32, found: &mut Vec<FoundCode>) {
    if run.is_empty() {
        return;
    }
    let left = run.iter().map(|line| line.x).fold(f32::MAX, f32::min);
    let text_lines: Vec<String> = run
        .iter()
        .map(|line| {
            let width = line.character_width.max(0.1);
            let spaces = ((line.x - left) / width).round().max(0.0) as usize;
            format!("{}{}", " ".repeat(spaces), line.text.trim_end())
        })
        .collect();
    found.push(FoundCode {
        text: text_lines.join("\n"),
        location: format!("page {page}"),
        page: Some(page),
        paragraph: None,
        found_by: "monospaced font".into(),
    });
    run.clear();
}

/// every page's code blocks. a page the reader can't take apart is
/// skipped rather than failing the whole document.
pub fn code_in_pdf(document: &lopdf::Document) -> Vec<FoundCode> {
    let page_count = document.get_pages().len();
    let mut found = Vec::new();
    for page in 0..page_count {
        if let Ok(objects) = letters_edit::page_objects(document, page) {
            found.extend(code_in_pdf_page(&pdf_lines(&objects), page as u32 + 1));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Inline, Mark, Paragraph};

    fn paragraph(text: &str, monospaced: bool) -> Block {
        let marks = if monospaced {
            vec![Mark::FontFamily {
                name: "Courier New".into(),
                theme: None,
                extra: Vec::new(),
            }]
        } else {
            Vec::new()
        };
        Block::Paragraph(Paragraph {
            content: vec![Inline::styled(text, marks)],
            ..Paragraph::default()
        })
    }

    #[test]
    fn monospaced_paragraphs_in_a_row_are_one_block_with_its_paragraph_number() {
        let mut document = Document::new();
        document.blocks = vec![
            paragraph("Here is my program:", false),
            paragraph("int main(void) {", true),
            paragraph("  return 0", true),
            paragraph("}", true),
            paragraph("It doesn't compile.", false),
        ];
        let found = code_in_office_document(&document);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "int main(void) {\n  return 0\n}");
        assert_eq!(found[0].location, "paragraph 2");
        assert_eq!(found[0].found_by, "monospaced font");
    }

    #[test]
    fn fenced_blocks_in_plain_paragraphs_are_found() {
        let mut document = Document::new();
        document.blocks = vec![
            paragraph("```python", false),
            paragraph("print('hi')", false),
            paragraph("```", false),
        ];
        let found = code_in_office_document(&document);
        assert_eq!(found[0].text, "print('hi')");
        assert_eq!(found[0].found_by, "fenced block");
    }

    #[test]
    fn pdf_lines_keep_their_indent_and_hyphens() {
        let line = |text: &str, x: f32, monospaced: bool| PdfLine {
            text: text.into(),
            x,
            character_width: 6.0,
            monospaced,
        };
        let lines = vec![
            line("Listing 1", 72.0, false),
            line("for i in range(3):", 72.0, true),
            line("print(i -", 96.0, true),
            line("1)", 96.0, true),
            line("The loop prints three numbers.", 72.0, false),
        ];
        let found = code_in_pdf_page(&lines, 4);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "for i in range(3):\n    print(i -\n    1)");
        assert_eq!(found[0].location, "page 4");
    }
}
