//! "easy to edit" word and opendocument files: the structure the
//! convert engine reads from a pdf (headings, paragraphs, lists, tables,
//! pictures, footnotes, repeated headers and footers) rebuilt as a real
//! document in the office model, which `letters_office` then writes as
//! .docx or .odt.
//!
//! nothing here looks at the pdf itself except through `PictureSource`,
//! so the mapping is tested with structures made by hand.
//!
//! | file | job |
//! |---|---|
//! | `runs.rs` | text runs to inline text with marks |
//! | `pages.rs` | sections (page size, margins, columns), headers and footers |
//! | `tables.rs` | tables with spans, header rows, shading and borders |
//! | `mod.rs` | the document, lists, pictures, footnotes, the report |

mod pages;
mod runs;
mod tables;
// the hand-made structures are shared with the ai tools' tests
#[cfg(test)]
pub(crate) mod tests;

use std::collections::HashMap;

use letters_convert::{Block, BlockContent, Figure, ListItem, StructuredDocument, TextRun};
use letters_office::model::{
    Block as OfficeBlock, Document, FontRef, Heading, Inline, ListDefinition, ListRef, Note,
    NoteKind, Paragraph, Style, Twips,
};
use serde::Serialize;

pub use pages::twips;
pub use runs::{half_points, BodyText};

use runs::{inlines, RunContext};

/// gives the bytes of a picture in the pdf (png or jpeg), if it can be
/// copied out. the app reads them with letters-edit; tests hand them over.
pub trait PictureSource {
    fn picture(&self, page: usize, image_id: &str) -> Option<Vec<u8>>;
}

/// what was built, counted, for the result line of the export dialog.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyEditReport {
    pub pages: usize,
    pub headings: usize,
    pub paragraphs: usize,
    pub lists: usize,
    pub tables: usize,
    pub pictures: usize,
    /// pictures the pdf holds in a form that can't be copied out
    pub pictures_left_out: usize,
    pub footnotes: usize,
    pub header: bool,
    pub footer: bool,
    pub sections: usize,
}

const BULLETS: &str = "letters-bullets";
const NUMBERS: &str = "letters-numbers";

/// the document for a pdf's structure. `pictures` gives the bytes of the
/// pdf's pictures.
pub fn build_document(
    structure: &StructuredDocument,
    pictures: &dyn PictureSource,
) -> (Document, EasyEditReport) {
    let mut builder = Builder::new(structure);
    builder.add_blocks(&structure.blocks, pictures);
    builder.finish(structure)
}

struct Builder {
    document: Document,
    report: EasyEditReport,
    body: BodyText,
    /// the first page of every section after the first
    section_starts: Vec<usize>,
    /// the instance of the numbered list being written, until another
    /// kind of block ends it
    numbered_list: Option<String>,
}

impl Builder {
    fn new(structure: &StructuredDocument) -> Builder {
        let body = body_text(structure);
        let mut document = Document::new();
        document.blocks.clear();
        document.metadata.title = structure.title.clone();
        set_normal_style(&mut document, &body);
        document
            .numbering
            .definitions
            .push(ListDefinition::standard(BULLETS, false));
        document
            .numbering
            .definitions
            .push(ListDefinition::standard(NUMBERS, true));
        Builder {
            document,
            report: EasyEditReport {
                pages: structure.pages.len(),
                ..EasyEditReport::default()
            },
            body,
            section_starts: pages::page_runs(&structure.pages)
                .iter()
                .skip(1)
                .map(|run| run.first)
                .collect(),
            numbered_list: None,
        }
    }

    /// blocks in reading order, with a section break where the page size
    /// or the number of columns changes.
    fn add_blocks(&mut self, blocks: &[Block], pictures: &dyn PictureSource) {
        let mut previous_page: Option<usize> = None;
        for block in blocks {
            if let Some(previous) = previous_page {
                let crossed = self
                    .section_starts
                    .iter()
                    .any(|start| previous < *start && block.page >= *start);
                if crossed {
                    self.document.blocks.push(OfficeBlock::SectionBreak);
                    self.numbered_list = None;
                }
            }
            previous_page = Some(block.page);
            self.add_block(block, pictures);
        }
    }

    fn add_block(&mut self, block: &Block, pictures: &dyn PictureSource) {
        if !matches!(block.content, BlockContent::List { .. }) {
            self.numbered_list = None;
        }
        match &block.content {
            BlockContent::Heading { level, runs } => self.add_heading(*level, runs),
            BlockContent::Paragraph { runs } => self.add_paragraph(runs),
            BlockContent::List { items } => self.add_list(items),
            BlockContent::Table(table) => self.add_table(table),
            BlockContent::Figure(figure) => self.add_figure(block, figure, pictures),
            BlockContent::Footnote { marker, runs } => self.add_footnote(marker, runs),
        }
    }

    fn add_heading(&mut self, level: u8, runs: &[TextRun]) {
        let level = level.clamp(1, 6);
        let style = letters_office::outline::heading_style(&mut self.document, level);
        let paragraph = Paragraph {
            style_id: Some(style),
            content: inlines(runs, &self.body, RunContext::Heading),
            ..Paragraph::default()
        };
        self.document
            .blocks
            .push(OfficeBlock::Heading(Heading { level, paragraph }));
        self.report.headings += 1;
    }

    /// raised note markers stay as text here; the footnote that follows
    /// swaps them for real references.
    fn add_paragraph(&mut self, runs: &[TextRun]) {
        let paragraph = Paragraph {
            content: inlines(runs, &self.body, RunContext::Body),
            ..Paragraph::default()
        };
        self.document.blocks.push(OfficeBlock::Paragraph(paragraph));
        self.report.paragraphs += 1;
    }

    /// bulleted items share one list; each numbered list starts again at 1.
    fn add_list(&mut self, items: &[ListItem]) {
        for item in items {
            let list_id = if item.ordered {
                self.numbered_list_id()
            } else {
                self.bullet_list_id()
            };
            let paragraph = Paragraph {
                props: letters_office::model::ParagraphProps {
                    list: Some(ListRef {
                        list_id,
                        level: item.level.min(8),
                    }),
                    ..Default::default()
                },
                content: inlines(&item.runs, &self.body, RunContext::Body),
                ..Paragraph::default()
            };
            self.document.blocks.push(OfficeBlock::Paragraph(paragraph));
        }
        self.report.lists += 1;
    }

    fn bullet_list_id(&mut self) -> String {
        let numbering = &mut self.document.numbering;
        let existing = numbering
            .instances
            .iter()
            .find(|instance| instance.definition_id == BULLETS)
            .map(|instance| instance.id.clone());
        existing.unwrap_or_else(|| numbering.add_instance(BULLETS))
    }

    fn numbered_list_id(&mut self) -> String {
        if let Some(id) = &self.numbered_list {
            return id.clone();
        }
        let id = self.document.numbering.restart(NUMBERS);
        self.numbered_list = Some(id.clone());
        id
    }

    fn add_table(&mut self, found: &letters_convert::Table) {
        self.document
            .blocks
            .push(OfficeBlock::Table(tables::table(found, &self.body)));
        if let Some(caption) = &found.caption {
            self.add_caption(caption);
        }
        self.report.tables += 1;
    }

    fn add_figure(&mut self, block: &Block, figure: &Figure, pictures: &dyn PictureSource) {
        let picture = pictures
            .picture(block.page, &figure.image_id)
            .and_then(|bytes| {
                letters_office::clipboard::add_image(&mut self.document, &bytes, None).ok()
            });
        match picture {
            Some(pasted) => {
                let mut inline = pasted.inline;
                if let Inline::Image(image) = &mut inline {
                    // the size it had on the page, not its pixel count
                    image.width = twips(block.rect.width);
                    image.height = twips(block.rect.height);
                    image.description = figure
                        .caption
                        .as_ref()
                        .map(|caption| letters_convert::runs_text(caption));
                }
                let paragraph = Paragraph {
                    content: vec![inline],
                    ..Paragraph::default()
                };
                self.document.blocks.push(OfficeBlock::Paragraph(paragraph));
                self.report.pictures += 1;
            }
            None => self.report.pictures_left_out += 1,
        }
        if let Some(caption) = &figure.caption {
            self.add_caption(caption);
        }
    }

    fn add_caption(&mut self, runs: &[TextRun]) {
        let style = caption_style(&mut self.document);
        let paragraph = Paragraph {
            style_id: Some(style),
            content: inlines(runs, &self.body, RunContext::Body),
            ..Paragraph::default()
        };
        self.document.blocks.push(OfficeBlock::Paragraph(paragraph));
    }

    /// a footnote becomes a real note. its reference goes where the
    /// marker was printed as a raised number in the text above; if no
    /// such number was found, at the end of the paragraph before it.
    fn add_footnote(&mut self, marker: &str, runs: &[TextRun]) {
        let id = (self.document.notes.len() + 1).to_string();
        let content = inlines(runs, &self.body, RunContext::Body);
        self.document.notes.push(Note {
            id: id.clone(),
            kind: NoteKind::Footnote,
            special: None,
            blocks: vec![OfficeBlock::Paragraph(Paragraph {
                content,
                ..Paragraph::default()
            })],
        });
        if !self.place_reference(marker, &id) {
            self.append_reference(&id);
        }
        self.report.footnotes += 1;
    }

    /// replaces the newest raised marker in the text with the reference.
    fn place_reference(&mut self, marker: &str, id: &str) -> bool {
        for block in self.document.blocks.iter_mut().rev() {
            let Some(paragraph) = block.paragraph_mut() else {
                continue;
            };
            let found = paragraph
                .content
                .iter()
                .rposition(|inline| is_marker(inline, marker));
            if let Some(index) = found {
                paragraph.content[index] = note_reference(id);
                return true;
            }
        }
        false
    }

    fn append_reference(&mut self, id: &str) {
        let last = self
            .document
            .blocks
            .iter_mut()
            .rev()
            .find_map(|block| block.paragraph_mut());
        match last {
            Some(paragraph) => paragraph.content.push(note_reference(id)),
            None => self.document.blocks.push(OfficeBlock::Paragraph(Paragraph {
                content: vec![note_reference(id)],
                ..Paragraph::default()
            })),
        }
    }

    fn finish(mut self, structure: &StructuredDocument) -> (Document, EasyEditReport) {
        let (parts, ids) = pages::header_footer_parts(structure);
        self.report.header = ids.header.is_some();
        self.report.footer = ids.footer.is_some();
        self.document.headers_footers = parts;
        let runs = pages::page_runs(&structure.pages);
        self.document.sections = runs
            .iter()
            .map(|run| pages::section_for(run, &structure.blocks, &ids))
            .collect();
        self.report.sections = self.document.sections.len();
        if self.document.blocks.is_empty() {
            self.document
                .blocks
                .push(OfficeBlock::Paragraph(Paragraph::default()));
        }
        self.document.normalize_sections();
        (self.document, self.report)
    }
}

/// the body font is the family most of the body-size text is set in.
fn body_text(structure: &StructuredDocument) -> BodyText {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for block in &structure.blocks {
        if let BlockContent::Paragraph { runs } = &block.content {
            for run in runs {
                if (run.size - structure.body_size).abs() < 0.6 {
                    *counts.entry(run.font_family.as_str()).or_default() += run.text.len();
                }
            }
        }
    }
    let family = counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(family, _)| family.to_string())
        .unwrap_or_else(|| "Times New Roman".to_string());
    BodyText {
        family,
        half_points: half_points(structure.body_size.max(4.0)),
    }
}

/// Normal carries the body font and size, and a little space after each
/// paragraph so paragraphs read apart the way they did on the page.
fn set_normal_style(document: &mut Document, body: &BodyText) {
    let Some(normal) = document
        .styles
        .styles
        .iter_mut()
        .find(|style| style.is_default)
    else {
        return;
    };
    normal.run.font = Some(FontRef {
        name: body.family.clone(),
        theme: None,
        extra: Vec::new(),
    });
    normal.run.size = Some(body.half_points);
    normal.paragraph.spacing.after = Some(half_points_to_twips(body.half_points) / 2);
}

fn half_points_to_twips(half_points: u32) -> Twips {
    (half_points * 10) as Twips
}

/// the "Caption" style word and libreoffice both know, added once.
fn caption_style(document: &mut Document) -> String {
    const ID: &str = "Caption";
    if document.style(ID).is_none() {
        let mut style = Style::paragraph(ID, "caption");
        style.based_on = document
            .styles
            .styles
            .iter()
            .find(|s| s.is_default)
            .map(|s| s.id.clone());
        style.run.italic = Some(true);
        document.styles.styles.push(style);
    }
    ID.to_string()
}

/// a raised number in the text that matches a footnote's marker.
fn is_marker(inline: &Inline, marker: &str) -> bool {
    match inline {
        Inline::Text { text, marks } => {
            text.trim() == marker.trim()
                && marks
                    .iter()
                    .any(|mark| matches!(mark, letters_office::model::Mark::Superscript))
        }
        _ => false,
    }
}

fn note_reference(id: &str) -> Inline {
    Inline::NoteReference {
        note_id: id.to_string(),
        marks: Vec::new(),
    }
}
