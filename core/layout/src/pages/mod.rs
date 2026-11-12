//! page building: the body's blocks flowed into the columns of pages,
//! with sections, headers and footers, footnotes, keeps and widow control.
//!
//! work happens in two steps. first every paragraph is broken into lines
//! at its column width (`prepare`); that depends only on the paragraph and
//! the width, so it can be cached and reused while editing. then the lines
//! are placed on pages (`Flow`), which is quick arithmetic.

pub mod emit;
pub mod geometry;

use std::collections::HashMap;

use letters_office::model::{Block, Document, Paragraph, SectionStart};

use crate::flavor::Flavor;
use crate::fonts::FontLibrary;
use crate::lists::Label;
use crate::paragraph::{layout_paragraph, ParagraphLayout};
use crate::result::{Line, Page, Rect, RectKind, Story};
use crate::style::ParaStyle;
use crate::text::{self, ClusterKind, FieldValues, ParagraphText, TextContext};
use crate::unsupported::{Unsupported, UnsupportedKind};
use emit::{emit_line, LinePlacement};
use geometry::{page_kind, part_id, PageKind, SectionGeometry};

/// lines may overrun the bottom of the page by this much (rounding)
pub(crate) const FIT_TOLERANCE: f64 = 0.01;

/// everything layout needs while it works through one document.
pub struct Context<'a> {
    pub document: &'a Document,
    pub flavor: Flavor,
    pub fonts: FontLibrary,
    /// footnote and endnote numbers as shown, by note id
    pub note_numbers: HashMap<String, String>,
    pub labels: HashMap<Vec<u32>, Label>,
    pub unsupported: Vec<Unsupported>,
    /// the style of the table whose cells are being prepared
    pub table_style: Option<crate::style::TableStyleProps>,
    /// the page count a page count field in body text shows (found by a
    /// first pass)
    pub page_count_hint: Option<u32>,
}

/// a paragraph broken into lines, ready to place.
#[derive(Clone, Debug)]
pub struct PreparedParagraph {
    pub story: Story,
    pub path: Vec<u32>,
    pub style: ParaStyle,
    pub text: ParagraphText,
    pub layout: ParagraphLayout,
    pub width: f64,
}

impl Context<'_> {
    /// shapes and breaks one paragraph at `width`.
    pub fn prepare_paragraph(
        &mut self,
        paragraph: &Paragraph,
        story: Story,
        path: Vec<u32>,
        width: f64,
        fields: FieldValues,
    ) -> PreparedParagraph {
        let style = crate::style::para_style_in(
            self.document,
            paragraph,
            self.table_style.as_ref(),
            self.flavor,
        );
        if style.right_to_left {
            self.unsupported.push(Unsupported::new(
                UnsupportedKind::RightToLeftText,
                &location(&story, &path),
            ));
        }
        let label = if story == Story::Body {
            self.labels.get(&path).cloned()
        } else {
            None
        };
        let label = label.or_else(|| self.label_outside_body(paragraph));
        let mut context = TextContext {
            document: self.document,
            flavor: self.flavor,
            fonts: &mut self.fonts,
            note_numbers: &self.note_numbers,
            fields,
            unsupported: &mut self.unsupported,
            location: location(&story, &path),
            table: self.table_style.as_ref(),
        };
        let text = text::build(&mut context, paragraph, &style, label.as_ref());
        let layout = layout_paragraph(&text, &style, width, self.flavor);
        PreparedParagraph {
            story,
            path,
            style,
            text,
            layout,
            width,
        }
    }

    /// list paragraphs in headers, notes and text boxes count on their own
    fn label_outside_body(&self, paragraph: &Paragraph) -> Option<Label> {
        let mut counter = letters_office::numbering::ListCounter::new(self.document);
        crate::lists::label_for(self.document, paragraph, &mut counter, self.flavor)
    }
}

/// a readable place for reports: "body 3", "header h1 0", "body 4/1/0/2".
pub fn location(story: &Story, path: &[u32]) -> String {
    let path: Vec<String> = path.iter().map(|p| p.to_string()).collect();
    let path = path.join("/");
    match story {
        Story::Body => format!("body {path}"),
        Story::Header { id } => format!("header {id} {path}"),
        Story::Footer { id } => format!("footer {id} {path}"),
        Story::Footnote { id } => format!("footnote {id} {path}"),
        Story::Endnote { id } => format!("endnote {id} {path}"),
        Story::TextBox { block } => format!("text box {block} {path}"),
        Story::Positioned { page } => format!("positioned page {page}"),
    }
}

/// footnote and endnote numbers in the order the body refers to them
/// (word numbers 1, 2, 3 for footnotes and i, ii, iii for endnotes).
pub fn note_numbers(document: &Document) -> HashMap<String, String> {
    let mut numbers = HashMap::new();
    let mut footnotes = 0u32;
    let mut endnotes = 0u32;
    let mut visit = |inline: &letters_office::model::Inline| {
        if let letters_office::model::Inline::NoteReference { note_id, .. } = inline {
            if numbers.contains_key(note_id) {
                return;
            }
            let endnote = document
                .note(note_id)
                .is_some_and(|note| note.kind == letters_office::model::NoteKind::Endnote);
            let shown = if endnote {
                endnotes += 1;
                letters_office::numbering::format_number(
                    endnotes,
                    &letters_office::model::NumberFormat::LowerRoman,
                )
            } else {
                footnotes += 1;
                footnotes.to_string()
            };
            numbers.insert(note_id.clone(), shown);
        }
    };
    for_each_inline(&document.blocks, &mut visit, 0);
    numbers
}

fn for_each_inline(
    blocks: &[Block],
    visit: &mut dyn FnMut(&letters_office::model::Inline),
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    for block in blocks {
        match block {
            Block::Paragraph(_) | Block::Heading(_) => {
                for inline in &block.paragraph().expect("paragraph").content {
                    visit(inline);
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        for_each_inline(&cell.blocks, visit, depth + 1);
                    }
                }
            }
            Block::TextBox(text_box) => for_each_inline(&text_box.blocks, visit, depth + 1),
            _ => {}
        }
    }
}

/// what the paragraph before left behind for the spacing rules.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviousParagraph {
    pub style_id: Option<String>,
    pub space_after: f64,
    pub contextual: bool,
}

/// what the next content to place is: a body block and, inside it, the
/// line of a paragraph or the row of a table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub block: usize,
    pub line: usize,
    /// false while placing something a page start cannot resume from
    /// (a table row split over pages, a paragraph laid out around a
    /// float, endnotes)
    pub resumable: bool,
}

/// how a page began: enough to start placing again from here, or to see
/// that a new layout has caught up with an old one (incremental relayout).
#[derive(Clone, Debug, PartialEq)]
pub struct PageStart {
    pub cursor: Cursor,
    pub section: usize,
    pub number: u32,
    pub kind: PageKind,
    pub first_of_section: bool,
    pub after_hard_break: bool,
    /// the page has one column, no notes carried from the page before and
    /// the cursor is resumable
    pub resumable: bool,
}

impl PageStart {
    /// the same start apart from where it is in the document and its
    /// page number (an edit earlier on shifts both).
    pub fn same_state(&self, other: &PageStart) -> bool {
        self.cursor.line == other.cursor.line
            && self.section == other.section
            && self.kind == other.kind
            && self.first_of_section == other.first_of_section
            && self.after_hard_break == other.after_hard_break
            && self.resumable
            && other.resumable
    }
}

/// what an incremental relayout looks for while it places: an old page
/// start that the new layout may meet again after the edited blocks.
#[derive(Clone, Debug, Default)]
pub struct ResyncTarget {
    /// the old run's page starts
    pub starts: Vec<PageStart>,
    /// new block indices from here on are unchanged since the old run
    pub unchanged_from: usize,
    /// old block index = new block index - shift
    pub shift: isize,
    /// even and odd headers: page numbers may only shift by even amounts
    pub parity_matters: bool,
}

/// one page while it is being filled.
#[derive(Clone)]
pub struct PageState {
    pub page: Page,
    pub section: usize,
    pub geometry: SectionGeometry,
    pub kind: PageKind,
    pub first_of_section: bool,
    pub body_top: f64,
    pub body_bottom: f64,
    /// footnotes placed on this page, each under the column that refers
    /// to it (word puts notes at the foot of their column)
    pub notes: Vec<crate::footnotes::NoteChunk>,
    /// the height of the note area under each column
    pub note_heights: Vec<f64>,
    /// the column being filled (mirrors `Flow::column`)
    pub active_column: usize,
    /// floating objects that text wraps around
    pub floats: Vec<crate::floats::FloatBox>,
    /// columns of the region being filled: page x and width
    pub columns: Vec<(f64, f64)>,
    pub region_top: f64,
    /// its notes and column rules are drawn: a page an incremental
    /// relayout resumes after must not get them twice
    pub finished: bool,
}

impl PageState {
    /// the lowest y body text may reach in the column being filled, above
    /// its footnotes.
    pub fn bottom(&self) -> f64 {
        self.body_bottom - self.note_height()
    }

    /// the note area's height. its columns share one height, the tallest:
    /// in word's two-column pages both columns' notes start level
    /// (real_research_paper in the corpus). word also shortens a column
    /// already filled when a later column's notes grow the area; this
    /// model does not go back (a known gap).
    pub fn note_height(&self) -> f64 {
        self.note_heights.iter().copied().fold(0.0, f64::max)
    }

    /// whether the column being filled has notes yet.
    pub fn column_has_notes(&self) -> bool {
        self.notes
            .iter()
            .any(|chunk| chunk.column == self.active_column)
    }
}

/// where the next line goes.
pub struct Flow<'c, 'd> {
    pub context: &'c mut Context<'d>,
    pub pages: Vec<PageState>,
    pub column: usize,
    pub y: f64,
    /// nothing has been placed in this column yet
    pub column_empty: bool,
    pub previous: Option<PreviousParagraph>,
    pub section: usize,
    /// header and footer heights by (section, page kind)
    part_heights: HashMap<(usize, PageKind, bool), f64>,
    /// footnotes waiting for room on the next page
    pub carried_notes: Vec<crate::footnotes::NoteChunk>,
    pub notes: crate::footnotes::NoteCache,
    /// set when the last placed content was a hard page break
    pub after_hard_break: bool,
    /// a continuous section began: the space after the paragraph before it,
    /// still to be combined with the first paragraph's space before
    pub region_gap: Option<f64>,
    /// the next content to place, and how every page began
    pub cursor: Cursor,
    pub starts: Vec<PageStart>,
    /// set by an incremental relayout: old page starts to meet again
    pub resync: Option<ResyncTarget>,
    /// the old page index the new layout met again: placing stops, and
    /// the old pages from there on are reused
    pub stopped: Option<usize>,
    /// a page or column break character is opening the next page
    opening_hard_break: bool,
}

impl<'c, 'd> Flow<'c, 'd> {
    pub fn new(context: &'c mut Context<'d>) -> Flow<'c, 'd> {
        Flow {
            context,
            pages: Vec::new(),
            column: 0,
            y: 0.0,
            column_empty: true,
            previous: None,
            section: 0,
            part_heights: HashMap::new(),
            carried_notes: Vec::new(),
            notes: crate::footnotes::NoteCache::default(),
            after_hard_break: false,
            region_gap: None,
            cursor: Cursor {
                resumable: true,
                ..Cursor::default()
            },
            starts: Vec::new(),
            resync: None,
            stopped: None,
            opening_hard_break: false,
        }
    }

    pub fn page(&mut self) -> &mut PageState {
        self.pages
            .last_mut()
            .expect("a page is always open while placing")
    }

    pub fn page_ref(&self) -> &PageState {
        self.pages
            .last()
            .expect("a page is always open while placing")
    }

    /// the current column's page x and width.
    pub fn column_box(&self) -> (f64, f64) {
        let page = self.page_ref();
        page.columns[self.column.min(page.columns.len() - 1)]
    }

    pub fn bottom(&self) -> f64 {
        self.page_ref().bottom()
    }

    /// opens a new page for the current section.
    pub fn new_page(&mut self, first_of_section: bool) {
        self.finish_page();
        let document = self.context.document;
        let section_index = self.section.min(document.sections.len().saturating_sub(1));
        let section = document
            .sections
            .get(section_index)
            .cloned()
            .unwrap_or_default();
        let geometry = SectionGeometry::of(&section);
        let number = match (first_of_section, section.page_number_start) {
            (true, Some(start)) => start,
            _ => self.pages.last().map_or(1, |page| page.page.number + 1),
        };
        let kind = page_kind(document, &section, first_of_section, number);
        let header = self.part_height(section_index, kind, false, &geometry);
        let footer = self.part_height(section_index, kind, true, &geometry);
        // a header taller than the top margin pushes the body down, and a
        // tall footer pushes its bottom up (word and libreoffice alike)
        let body_top = geometry.top.max(geometry.header_distance + header);
        let body_bottom = (geometry.height - geometry.bottom)
            .min(geometry.height - geometry.footer_distance - footer);
        let index = self.pages.len() as u32;
        let columns = geometry.columns();
        self.pages.push(PageState {
            page: Page {
                index,
                number,
                section: section_index as u32,
                width: geometry.width,
                height: geometry.height,
                ..Page::default()
            },
            section: section_index,
            geometry,
            kind,
            first_of_section,
            body_top,
            body_bottom,
            notes: Vec::new(),
            note_heights: vec![0.0; 16],
            active_column: 0,
            floats: Vec::new(),
            columns,
            region_top: body_top,
            finished: false,
        });
        self.column = 0;
        self.y = body_top;
        self.column_empty = true;
        self.previous = None;
        let carried = !self.carried_notes.is_empty();
        self.place_carried_notes();
        self.record_start(section_index, number, kind, first_of_section, carried);
        self.settle_start();
    }

    /// notes how the page just opened began, and whether an incremental
    /// relayout has caught up with its old layout here.
    fn record_start(
        &mut self,
        section: usize,
        number: u32,
        kind: PageKind,
        first_of_section: bool,
        carried_notes: bool,
    ) {
        let single_column = self.page_ref().columns.len() == 1;
        let start = PageStart {
            cursor: self.cursor,
            section,
            number,
            kind,
            first_of_section,
            after_hard_break: self.opening_hard_break,
            resumable: self.cursor.resumable
                && single_column
                && !carried_notes
                && self.region_gap.is_none(),
        };
        self.starts.push(start);
    }

    /// checks whether an incremental relayout has met its old layout
    /// again at the page just opened.
    fn settle_start(&mut self) {
        let Some(start) = self.starts.last().cloned() else {
            return;
        };
        let Some(target) = &self.resync else {
            return;
        };
        if self.stopped.is_some() || start.cursor.block < target.unchanged_from {
            return;
        }
        let old_block = start.cursor.block as isize - target.shift;
        let found = target.starts.iter().position(|old| {
            old.cursor.block as isize == old_block
                && start.same_state(old)
                && (!target.parity_matters || (start.number % 2 == old.number % 2))
        });
        if let Some(index) = found {
            self.stopped = Some(index);
        }
    }

    /// the height of a section's header (or footer) for a kind of page,
    /// laid out with the fields' stored values.
    fn part_height(
        &mut self,
        section: usize,
        kind: PageKind,
        footer: bool,
        geometry: &SectionGeometry,
    ) -> f64 {
        if let Some(height) = self.part_heights.get(&(section, kind, footer)) {
            return *height;
        }
        let document = self.context.document;
        let height = match part_id(&document.sections, section, kind, footer)
            .and_then(|id| document.header_footer(&id).map(|part| (id, part)))
        {
            Some((id, part)) => {
                let story = if footer {
                    Story::Footer { id }
                } else {
                    Story::Header { id }
                };
                let (_, height) = crate::stack::stack_blocks(
                    self.context,
                    &part.blocks,
                    &story,
                    &[],
                    geometry.left,
                    geometry.text_width(),
                    0.0,
                    FieldValues::default(),
                );
                height
            }
            None => 0.0,
        };
        self.part_heights.insert((section, kind, footer), height);
        height
    }

    /// moves to the next column, or the next page after the last column.
    pub fn next_column(&mut self) {
        let columns = self.page_ref().columns.len();
        if self.column + 1 < columns {
            self.column += 1;
            self.page().active_column = self.column;
            self.y = self.page_ref().region_top;
            self.column_empty = true;
            self.previous = None;
            self.place_carried_notes();
        } else {
            self.new_page(false);
        }
    }

    /// space between the previous paragraph and `style`, by the rules:
    /// the larger of after and before (see `spacing_between`), contextual spacing between
    /// paragraphs of the same style removed, nothing at the column top.
    pub fn space_before(&self, style: &ParaStyle) -> f64 {
        let rules = self.context.flavor.rules();
        if self.column_empty {
            // the first paragraph of the document keeps its space before
            // in word (headings in the corpus: a 24 pt heading space on
            // page 1 stays, the same heading space on later pages goes)
            if self.pages.len() == 1
                && self.column == 0
                && !self.after_hard_break
                && self.page_ref().page.lines.is_empty()
            {
                return style.space_before;
            }
            let drop = if self.after_hard_break {
                rules.drop_space_before_after_hard_break
            } else {
                rules.drop_space_before_at_page_top
            };
            // the very first paragraph of a document keeps its space only
            // when the flavor keeps space at the top of a page
            return if drop { 0.0 } else { style.space_before };
        }
        let Some(previous) = &self.previous else {
            return style.space_before;
        };
        let same_style = previous.style_id == style.style_id;
        let after = if previous.contextual && same_style && rules.contextual_spacing {
            0.0
        } else {
            previous.space_after
        };
        let before = if style.contextual_spacing && same_style && rules.contextual_spacing {
            0.0
        } else {
            style.space_before
        };
        spacing_between(after, before)
    }

    /// places a paragraph's lines, splitting it across columns and pages
    /// by widow/orphan control and keep-lines-together. `need_after` is the
    /// room the following content needs on the same page (keep with next).
    pub fn place_paragraph(&mut self, paragraph: &PreparedParagraph, need_after: f64) {
        self.place_paragraph_from(paragraph, need_after, 0);
    }

    /// places a paragraph's lines from `first_line` on (an incremental
    /// relayout resumes at the line a page started with).
    pub fn place_paragraph_from(
        &mut self,
        paragraph: &PreparedParagraph,
        need_after: f64,
        first_line: usize,
    ) {
        self.cursor.line = first_line;
        if first_line == 0 {
            if paragraph.style.page_break_before && !self.page_is_empty() {
                self.new_page(false);
                if self.stopped.is_some() {
                    return;
                }
            }
            if let Some(after) = self.region_gap.take() {
                let gap = spacing_between(after, paragraph.style.space_before);
                self.y += gap;
                self.page().region_top = self.y;
            }
        }
        let first_before = self.space_before(&paragraph.style);
        if first_line == 0 {
            self.place_anchored_objects(paragraph, self.y + first_before);
        }
        let wrapped = self.with_wrapping(paragraph, first_before);
        // a paragraph laid out around a float keeps its own line breaks
        // across the page: a page starting inside it cannot be resumed
        // so does one whose floating objects were placed on the page where
        // it began, before its lines moved on
        let resumable = self.cursor.resumable;
        let anchors = paragraph
            .text
            .clusters
            .iter()
            .any(|c| matches!(c.kind, ClusterKind::Anchor(_)));
        if matches!(wrapped, std::borrow::Cow::Owned(_)) || anchors {
            self.cursor.resumable = false;
        }
        let paragraph = &*wrapped;
        let count = paragraph.layout.lines.len();
        let mut index = first_line.min(count);
        while index < count {
            self.cursor.line = index;
            let before = if index == 0 {
                self.space_before(&paragraph.style)
            } else {
                0.0
            };
            let fitting = self.lines_that_fit(paragraph, index, before);
            let wanted = self.lines_to_place(paragraph, index, fitting, before, need_after);
            if wanted == 0 {
                self.next_column();
                if self.stopped.is_some() {
                    return;
                }
                continue;
            }
            if index == 0 {
                self.y += before;
            }
            let mut placed = 0;
            while placed < wanted {
                if !self.place_line(paragraph, index + placed) {
                    break;
                }
                placed += 1;
            }
            index += placed;
            if index < count {
                self.cursor.line = index;
                self.next_column();
                if self.stopped.is_some() {
                    return;
                }
            }
        }
        self.cursor.resumable = resumable;
        self.cursor.line = 0;
        self.y = self.y.max(self.page_ref().region_top);
        self.previous = Some(PreviousParagraph {
            style_id: paragraph.style.style_id.clone(),
            space_after: paragraph.style.space_after,
            contextual: paragraph.style.contextual_spacing,
        });
        self.after_hard_break = false;
        if let Some(kind) = paragraph
            .layout
            .lines
            .last()
            .and_then(|line| line.forced_break)
        {
            // the page after the break starts with the next block
            self.cursor.block += 1;
            self.hard_break(kind);
        }
    }

    /// a page or column break character at the end of the paragraph.
    pub fn hard_break(&mut self, kind: ClusterKind) {
        self.opening_hard_break = true;
        match kind {
            ClusterKind::ColumnBreak => self.next_column(),
            _ => self.new_page(false),
        }
        self.opening_hard_break = false;
        self.after_hard_break = true;
    }

    fn page_is_empty(&self) -> bool {
        self.column == 0 && self.column_empty
    }

    /// how many lines from `index` fit in the rest of the column, with
    /// room for the least of each footnote they refer to.
    fn lines_that_fit(
        &mut self,
        paragraph: &PreparedParagraph,
        index: usize,
        before: f64,
    ) -> usize {
        let mut y = self.y + before;
        let mut bottom = self.bottom();
        let mut has_notes = self.page_ref().column_has_notes();
        let mut count = 0;
        for line in &paragraph.layout.lines[index..] {
            let notes = self.least_note_room(paragraph, line, &mut has_notes);
            if y + self.room_needed(line, bottom - notes) > bottom - notes + FIT_TOLERANCE {
                break;
            }
            bottom -= notes;
            y += line.vertical.height;
            count += 1;
        }
        count
    }

    /// how much of a line must fit above `bottom`. the extra space of a
    /// multiple line spacing may hang into the bottom margin, but not into
    /// the footnotes: word moved a double-spaced line whose text fitted
    /// above the notes but whose spacing did not (footnotes corpus), and
    /// kept a 1.08 line whose spacing reached into the margin (headings).
    fn room_needed(&self, line: &crate::paragraph::LineLayout, bottom: f64) -> f64 {
        if bottom < self.page_ref().body_bottom - 0.01 {
            line.vertical.height
        } else {
            line.vertical.fit_height
        }
    }

    /// the room the footnotes a line refers to need at least on its page:
    /// all of a short note, the first two lines of a longer one (it may
    /// split after them), and the separator before the page's first note.
    fn least_note_room(
        &mut self,
        paragraph: &PreparedParagraph,
        line: &crate::paragraph::LineLayout,
        has_notes: &mut bool,
    ) -> f64 {
        let ids: Vec<String> = paragraph
            .text
            .note_references
            .iter()
            .filter(|(_, cluster)| (line.start..line.end).contains(cluster))
            .map(|(id, _)| id.clone())
            .filter(|id| !self.page_ref().notes.iter().any(|chunk| &chunk.id == id))
            .filter(|id| {
                self.context
                    .document
                    .note(id)
                    .is_some_and(|n| n.kind == letters_office::model::NoteKind::Footnote)
            })
            .collect();
        let mut room = 0.0;
        for id in ids {
            if !*has_notes {
                room += self.note_separator_height();
                *has_notes = true;
            }
            room += self.least_note_height(&id);
        }
        room
    }

    /// the lines to put in this column, after widow and orphan control,
    /// keep lines together and keep with next have had their say.
    fn lines_to_place(
        &self,
        paragraph: &PreparedParagraph,
        index: usize,
        fitting: usize,
        before: f64,
        need_after: f64,
    ) -> usize {
        let count = paragraph.layout.lines.len();
        let rest = count - index;
        let style = &paragraph.style;
        let mut wanted = fitting.min(rest);
        if wanted == rest && need_after > 0.0 && !self.column_empty {
            let height: f64 = paragraph.layout.lines[index..]
                .iter()
                .map(|l| l.vertical.height)
                .sum();
            if self.y + before + height + need_after > self.bottom() + FIT_TOLERANCE {
                // keep with next: the paragraph's end must go with what follows
                wanted = if style.keep_together || count <= 2 {
                    0
                } else {
                    rest - 1
                };
            }
        }
        if wanted < rest && style.keep_together && index == 0 && !self.column_empty {
            wanted = 0;
        }
        if style.widow_control && wanted < rest {
            wanted = widow_orphan(wanted, index, count);
        }
        if wanted == 0 && self.column_empty {
            // nothing fits even in an empty column: place what fits, at
            // least one line, so layout always moves on
            wanted = fitting.clamp(1, rest);
        }
        wanted
    }

    /// emits one line at the current position. false when its footnotes
    /// leave it no room (it then goes to the next column).
    fn place_line(&mut self, paragraph: &PreparedParagraph, index: usize) -> bool {
        let line = &paragraph.layout.lines[index];
        let references: Vec<String> = paragraph
            .text
            .note_references
            .iter()
            .filter(|(_, cluster)| (line.start..line.end).contains(cluster))
            .map(|(id, _)| id.clone())
            .collect();
        // the footnotes placed so far may have taken the room counted on
        if !self.column_empty
            && self.y + self.room_needed(line, self.bottom()) > self.bottom() + FIT_TOLERANCE
        {
            return false;
        }
        if !references.is_empty() && !self.make_room_for_notes(&references, line.vertical.height) {
            return false;
        }
        let (left, _) = self.column_box();
        let placement = LinePlacement {
            story: paragraph.story.clone(),
            block: &paragraph.path,
            left,
            top: self.y,
        };
        let emitted = emit_line(
            &paragraph.text,
            &paragraph.layout,
            line,
            &placement,
            &self.context.fonts,
        );
        let page = &mut self.page().page;
        page.lines.push(emitted.line);
        page.rects.extend(emitted.rects);
        page.images.extend(emitted.images);
        self.y += line.vertical.height;
        self.column_empty = false;
        true
    }

    /// puts the section's header and footer on every finished page, and
    /// its footnotes. called once at the end.
    pub fn finish(mut self) -> Vec<Page> {
        let states = self.finish_body();
        self.draw_all_parts(states)
    }

    /// the body pages, footnotes drawn, before headers and footers (which
    /// depend on the page count): what an incremental relayout keeps.
    pub fn finish_body(&mut self) -> Vec<PageState> {
        self.cursor.resumable = false;
        self.finish_page();
        while !self.carried_notes.is_empty() {
            // notes that never found room get pages of their own
            self.new_page(false);
            self.finish_page();
        }
        std::mem::take(&mut self.pages)
    }

    /// headers and footers drawn on finished body pages.
    pub fn draw_all_parts(&mut self, mut states: Vec<PageState>) -> Vec<Page> {
        let total = states.len() as u32;
        for state in &mut states {
            self.draw_parts(state, total);
        }
        states.into_iter().map(|state| state.page).collect()
    }

    /// draws footnotes and the column separator of the page being left.
    fn finish_page(&mut self) {
        let Some(mut state) = self.pages.pop() else {
            return;
        };
        if !state.finished {
            crate::footnotes::draw_notes(self, &mut state);
            if state.geometry.column_separator && state.columns.len() > 1 {
                draw_column_separators(&mut state);
            }
            state.finished = true;
        }
        self.pages.push(state);
    }

    fn draw_parts(&mut self, state: &mut PageState, total: u32) {
        let document = self.context.document;
        let fields = FieldValues {
            page: Some(state.page.number),
            pages: Some(total),
        };
        let geometry = state.geometry.clone();
        for footer in [false, true] {
            let Some(id) = part_id(&document.sections, state.section, state.kind, footer) else {
                continue;
            };
            let Some(part) = document.header_footer(&id) else {
                continue;
            };
            let story = if footer {
                Story::Footer { id }
            } else {
                Story::Header { id }
            };
            let height = self.part_height(state.section, state.kind, footer, &geometry);
            let top = if footer {
                geometry.height - geometry.footer_distance - height
            } else {
                geometry.header_distance
            };
            let (emitted, _) = crate::stack::stack_blocks(
                self.context,
                &part.blocks,
                &story,
                &[],
                geometry.left,
                geometry.text_width(),
                top,
                fields,
            );
            emitted.apply_to(&mut state.page);
        }
    }
}

/// the space between two paragraphs: the larger of the first one's space
/// after and the second one's space before. both oracles collapse them
/// this way for docx: 12 pt after followed by 12 pt before leaves 12 pt,
/// 18 after and 6 before leaves 18 (spacing_rules in the corpus, word and
/// libreoffice alike).
pub fn spacing_between(after: f64, before: f64) -> f64 {
    after.max(before)
}

/// widow and orphan control: never leave one line of a paragraph alone at
/// the bottom (orphan) or the top (widow) of a page. `wanted` lines from
/// `index` would go in this column; returns how many should.
pub fn widow_orphan(wanted: usize, index: usize, count: usize) -> usize {
    let rest = count - index;
    if wanted >= rest {
        return wanted;
    }
    let mut wanted = wanted;
    // a widow: the last line alone on the next page; send one more along
    if rest - wanted == 1 && wanted > 0 {
        wanted -= 1;
    }
    // an orphan: the first line alone on this page; send it along too
    if index == 0 && wanted == 1 && count > 1 {
        wanted = 0;
    }
    wanted
}

fn draw_column_separators(state: &mut PageState) {
    let top = state.region_top;
    let bottom = state
        .page
        .lines
        .iter()
        .map(|l| l.top + l.height)
        .fold(top, f64::max);
    for pair in state.columns.windows(2) {
        let x = (pair[0].0 + pair[0].1 + pair[1].0) / 2.0;
        state.page.rects.push(Rect {
            kind: RectKind::ColumnSeparator,
            x: x - 0.25,
            y: top,
            width: 0.5,
            height: bottom - top,
            color: [0, 0, 0],
            dotted: false,
        });
    }
}

/// lines and rules laid out for some story, before they go on a page.
#[derive(Default)]
pub struct EmittedBlocks {
    pub lines: Vec<Line>,
    pub rects: Vec<Rect>,
    pub images: Vec<crate::result::PlacedImage>,
}

impl EmittedBlocks {
    pub fn apply_to(self, page: &mut Page) {
        page.lines.extend(self.lines);
        page.rects.extend(self.rects);
        page.images.extend(self.images);
    }
}

/// how a section begins: on the same page, or on a new (odd, even) page.
pub fn starts_new_page(start: SectionStart) -> bool {
    !matches!(start, SectionStart::Continuous | SectionStart::NextColumn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_spacing_collapses_to_the_larger() {
        assert_eq!(spacing_between(12.0, 12.0), 12.0);
        assert_eq!(spacing_between(18.0, 6.0), 18.0);
        assert_eq!(spacing_between(0.0, 24.0), 24.0);
    }

    #[test]
    fn widows_and_orphans() {
        // 5 lines, 3 fit: fine
        assert_eq!(widow_orphan(3, 0, 5), 3);
        // 5 lines, 4 fit: the last would be a widow, so 3 stay
        assert_eq!(widow_orphan(4, 0, 5), 3);
        // 5 lines, 1 fits: an orphan, move the whole paragraph
        assert_eq!(widow_orphan(1, 0, 5), 0);
        // 3 lines, 2 fit: widow fix leaves 1, an orphan, so all move
        assert_eq!(widow_orphan(2, 0, 3), 0);
        // 2 lines, 1 fits: move both
        assert_eq!(widow_orphan(1, 0, 2), 0);
        // continuing a paragraph: lines 5..9, 3 fit leaves 1: keep 2
        assert_eq!(widow_orphan(3, 5, 9), 2);
    }
}
