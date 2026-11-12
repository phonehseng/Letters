//! footnotes: the area at the foot of a page, above the bottom margin,
//! holding the notes referred to on that page.
//!
//! the rules, as word applies them: a note goes on the page of its
//! reference; the area grows upwards and the body gets shorter; a
//! separator line (two inches long) sits above the first note; a note that
//! does not fit is split, its first lines on the page of the reference and
//! the rest at the top of the next page's notes (under a full-width
//! continuation separator); a line whose note has no room for even one
//! line moves to the next page with its note.

use std::collections::HashMap;

use letters_office::model::{Block, NoteKind};

use crate::pages::emit::{emit_line, LinePlacement};
use crate::pages::{EmittedBlocks, Flow, PageState, PreparedParagraph, FIT_TOLERANCE};
use crate::result::{Rect, RectKind, Story};
use crate::text::FieldValues;

/// some lines of one note placed on one page.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteChunk {
    pub id: String,
    pub first: usize,
    pub count: usize,
    /// the note started on an earlier page (or column)
    pub continued: bool,
    /// the column whose foot it sits at
    pub column: usize,
}

/// one line of a note with the space above it.
#[derive(Clone, Debug)]
struct NoteLine {
    paragraph: usize,
    line: usize,
    height: f64,
}

/// a note broken into lines at the column width.
#[derive(Clone, Debug)]
pub struct PreparedNote {
    paragraphs: Vec<PreparedParagraph>,
    lines: Vec<NoteLine>,
}

#[derive(Default)]
pub struct NoteCache {
    notes: HashMap<String, PreparedNote>,
    separator: Option<f64>,
}

/// word's separator rule is 2 inches long and half a point thick
const SEPARATOR_LENGTH: f64 = 144.0;
const SEPARATOR_THICKNESS: f64 = 0.5;

impl Flow<'_, '_> {
    fn prepared_note(&mut self, id: &str) -> PreparedNote {
        if let Some(note) = self.notes.notes.get(id) {
            return note.clone();
        }
        let width = self.note_width();
        let document = self.context.document;
        let mut paragraphs = Vec::new();
        let mut lines = Vec::new();
        if let Some(note) = document.note(id) {
            let story = match note.kind {
                NoteKind::Footnote => Story::Footnote { id: id.to_string() },
                NoteKind::Endnote => Story::Endnote { id: id.to_string() },
            };
            let mut previous_after = 0.0;
            for (index, block) in note.blocks.iter().enumerate() {
                let Some(paragraph) = block.paragraph() else {
                    continue;
                };
                let paragraph = if paragraphs.is_empty() {
                    with_note_marker(document, paragraph, id)
                } else {
                    paragraph.clone()
                };
                let prepared = self.context.prepare_paragraph(
                    &paragraph,
                    story.clone(),
                    vec![index as u32],
                    width,
                    FieldValues::default(),
                );
                let before = if paragraphs.is_empty() {
                    0.0
                } else {
                    previous_after + prepared.style.space_before
                };
                for (line_index, line) in prepared.layout.lines.iter().enumerate() {
                    lines.push(NoteLine {
                        paragraph: paragraphs.len(),
                        line: line_index,
                        height: line.vertical.height + if line_index == 0 { before } else { 0.0 },
                    });
                }
                previous_after = prepared.style.space_after;
                paragraphs.push(prepared);
            }
        }
        let note = PreparedNote { paragraphs, lines };
        self.notes.notes.insert(id.to_string(), note.clone());
        note
    }

    /// the least of a note that must share its reference's page: all of a
    /// note of up to three lines, else its first two (widow and orphan).
    pub fn least_note_height(&mut self, id: &str) -> f64 {
        let note = self.prepared_note(id);
        let lines = if note.lines.len() <= 3 {
            note.lines.len()
        } else {
            2
        };
        note.lines[..lines].iter().map(|line| line.height).sum()
    }

    pub fn note_separator_height(&mut self) -> f64 {
        self.separator_height()
    }

    /// the separator's height: the document's own separator paragraph,
    /// or one single-spaced line of the default font.
    fn separator_height(&mut self) -> f64 {
        if let Some(height) = self.notes.separator {
            return height;
        }
        let document = self.context.document;
        let paragraph = document
            .notes
            .iter()
            .find(|note| note.special.as_deref() == Some("separator"))
            .and_then(|note| note.blocks.iter().find_map(Block::paragraph).cloned())
            .unwrap_or_else(|| {
                let mut paragraph = letters_office::model::Paragraph::default();
                paragraph.props.spacing.before = Some(0);
                paragraph.props.spacing.after = Some(0);
                paragraph.props.spacing.line = Some(letters_office::model::LineSpacing::SINGLE);
                paragraph
            });
        let width = self.note_width();
        let prepared = self.context.prepare_paragraph(
            &paragraph,
            Story::Footnote {
                id: "separator".into(),
            },
            vec![],
            width,
            FieldValues::default(),
        );
        let height = prepared.layout.height() + prepared.style.space_after;
        self.notes.separator = Some(height);
        height
    }

    /// notes are as wide as the column they are in
    fn note_width(&self) -> f64 {
        self.pages.last().map_or(468.0, |page| {
            page.columns[page.active_column.min(page.columns.len() - 1)].1
        })
    }

    fn active_column(&self) -> usize {
        self.pages.last().map_or(0, |page| page.active_column)
    }

    /// makes room for the notes a line refers to before the line is
    /// placed. false when not even one line of the first note fits.
    pub fn make_room_for_notes(&mut self, references: &[String], line_height: f64) -> bool {
        let fresh: Vec<String> = references
            .iter()
            .filter(|id| !self.page_ref().notes.iter().any(|chunk| &chunk.id == *id))
            .filter(|id| {
                self.context
                    .document
                    .note(id)
                    .is_some_and(|n| n.kind == NoteKind::Footnote)
            })
            .cloned()
            .collect();
        if fresh.is_empty() {
            return true;
        }
        let separator = if self.page_ref().column_has_notes() {
            0.0
        } else {
            self.separator_height()
        };
        let column = self.active_column();
        let mut room = self.bottom() - separator - (self.y + line_height);
        let mut chunks = Vec::new();
        let mut used = separator;
        let mut carried = Vec::new();
        for id in &fresh {
            let note = self.prepared_note(id);
            let mut fitted = 0;
            for line in &note.lines {
                if line.height > room + FIT_TOLERANCE || !carried.is_empty() {
                    break;
                }
                fitted += 1;
                room -= line.height;
            }
            // a note splits by widow and orphan control like any paragraph:
            // word moved a reference line to the next page rather than
            // leave one line of its note behind (footnotes in the corpus)
            if fitted < note.lines.len() {
                let kept = crate::pages::widow_orphan(fitted, 0, note.lines.len());
                room += note.lines[kept..fitted]
                    .iter()
                    .map(|l| l.height)
                    .sum::<f64>();
                fitted = kept;
            }
            used += note.lines[..fitted].iter().map(|l| l.height).sum::<f64>();
            if fitted > 0 {
                chunks.push(NoteChunk {
                    id: id.clone(),
                    first: 0,
                    count: fitted,
                    continued: false,
                    column,
                });
            }
            if fitted < note.lines.len() {
                carried.push(NoteChunk {
                    id: id.clone(),
                    first: fitted,
                    count: note.lines.len() - fitted,
                    continued: fitted > 0,
                    column,
                });
            }
        }
        let first_has_room = chunks.first().is_some_and(|chunk| chunk.id == fresh[0]);
        if !first_has_room && !self.column_empty {
            return false;
        }
        let page = self.page();
        page.notes.extend(chunks);
        page.note_heights[column.min(15)] += used;
        self.carried_notes.extend(carried);
        true
    }

    /// at the top of a new page, the notes left over from the last one.
    pub fn place_carried_notes(&mut self) {
        if self.carried_notes.is_empty() {
            return;
        }
        let carried = std::mem::take(&mut self.carried_notes);
        let separator = self.separator_height();
        let column = self.active_column();
        // continued notes may take up to most of the page, as word allows
        let limit = (self.bottom() - self.y) * 0.9;
        let mut used = separator;
        let mut remaining = Vec::new();
        let mut placed = Vec::new();
        for chunk in carried {
            if !remaining.is_empty() {
                remaining.push(chunk);
                continue;
            }
            let note = self.prepared_note(&chunk.id);
            let mut count = 0;
            for line in &note.lines[chunk.first..chunk.first + chunk.count] {
                if used + line.height > limit {
                    break;
                }
                used += line.height;
                count += 1;
            }
            if count > 0 {
                placed.push(NoteChunk {
                    id: chunk.id.clone(),
                    first: chunk.first,
                    count,
                    continued: true,
                    column,
                });
            }
            if count < chunk.count {
                remaining.push(NoteChunk {
                    id: chunk.id,
                    first: chunk.first + count,
                    count: chunk.count - count,
                    continued: true,
                    column,
                });
            }
        }
        let page = self.page();
        page.notes.extend(placed);
        page.note_heights[column.min(15)] += used;
        self.carried_notes = remaining;
    }
}

/// the note's own number at the start of its first paragraph. files keep
/// it as a marker (w:footnoteRef) the office model leaves out and its
/// writer puts back, in the "FootnoteReference" character style when the
/// document has one, else as plain superscript; layout shows it the same.
pub fn with_note_marker(
    document: &letters_office::model::Document,
    paragraph: &letters_office::model::Paragraph,
    id: &str,
) -> letters_office::model::Paragraph {
    use letters_office::model::{Inline, Mark};
    let marks = match document
        .style("FootnoteReference")
        .or_else(|| document.style("EndnoteReference"))
    {
        Some(style) => vec![Mark::CharacterStyle {
            style_id: style.id.clone(),
        }],
        None => vec![Mark::Superscript],
    };
    let mut marked = paragraph.clone();
    let already = marked.content.first().is_some_and(
        |inline| matches!(inline, Inline::NoteReference { note_id, .. } if note_id == id),
    );
    if !already {
        marked.content.insert(
            0,
            Inline::NoteReference {
                note_id: id.to_string(),
                marks,
            },
        );
    }
    marked
}

/// draws a page's notes at the foot of each column: separator, then the
/// chunks of that column.
pub fn draw_notes(flow: &mut Flow, state: &mut PageState) {
    let columns: Vec<usize> = {
        let mut seen: Vec<usize> = state.notes.iter().map(|chunk| chunk.column).collect();
        seen.dedup();
        seen
    };
    for column in columns {
        draw_column_notes(flow, state, column);
    }
}

fn draw_column_notes(flow: &mut Flow, state: &mut PageState, column: usize) {
    let chunks: Vec<NoteChunk> = state
        .notes
        .iter()
        .filter(|chunk| chunk.column == column)
        .cloned()
        .collect();
    let Some(first) = chunks.first() else {
        return;
    };
    let separator = flow.notes.separator.unwrap_or(0.0);
    let (left, width) = state.columns[column.min(state.columns.len() - 1)];
    let mut y = state.body_bottom - state.note_height();
    let continued = first.continued;
    let length = if continued {
        width
    } else {
        SEPARATOR_LENGTH.min(width)
    };
    state.page.rects.push(Rect {
        kind: RectKind::NoteSeparator,
        x: left,
        y: y + separator / 2.0 - SEPARATOR_THICKNESS / 2.0,
        width: length,
        height: SEPARATOR_THICKNESS,
        color: [0, 0, 0],
        dotted: false,
    });
    y += separator;
    let mut out = EmittedBlocks::default();
    for chunk in chunks {
        let note = flow.prepared_note(&chunk.id);
        for line in &note.lines[chunk.first..chunk.first + chunk.count] {
            let paragraph = &note.paragraphs[line.paragraph];
            let layout_line = &paragraph.layout.lines[line.line];
            let space = line.height - layout_line.vertical.height;
            let placement = LinePlacement {
                story: paragraph.story.clone(),
                block: &paragraph.path,
                left,
                top: y + space,
            };
            let emitted = emit_line(
                &paragraph.text,
                &paragraph.layout,
                layout_line,
                &placement,
                &flow.context.fonts,
            );
            out.lines.push(emitted.line);
            out.rects.extend(emitted.rects);
            out.images.extend(emitted.images);
            y += line.height;
        }
    }
    out.apply_to(&mut state.page);
}
