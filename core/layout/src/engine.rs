//! the entry point: a document in, pages out.

use std::sync::Arc;

use letters_office::model::{Block, Document, SectionStart};

use crate::flavor::Flavor;
use crate::fonts::{FontLibrary, FontSource, SystemFonts};
use crate::pages::geometry::SectionGeometry;
use crate::pages::{
    note_numbers, starts_new_page, Context, Cursor, Flow, PageStart, PageState, PreparedParagraph,
    ResyncTarget,
};
use crate::result::{FontInfo, LayoutResult, Page, Story};
use crate::tables::{prepare_table, PreparedTable};
use crate::text::FieldValues;
use crate::unsupported::{deduplicate, Unsupported, UnsupportedKind};

/// how to lay a document out.
#[derive(Clone)]
pub struct LayoutOptions {
    pub flavor: Flavor,
    /// where fonts come from; the computer's own by default
    pub fonts: Arc<dyn FontSource>,
    /// draw on word's 1/600 inch grid instead of exactly: for comparing
    /// side by side with word's pdf only, never the default (see grid.rs)
    pub word_grid_drawing: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            flavor: Flavor::Word,
            fonts: Arc::new(SystemFonts::default()),
            word_grid_drawing: false,
        }
    }
}

impl LayoutOptions {
    /// the computer's fonts as the flavor's application sees them.
    pub fn flavor(flavor: Flavor) -> LayoutOptions {
        LayoutOptions {
            flavor,
            fonts: Arc::new(SystemFonts {
                office_cloud_fonts: flavor.rules().office_cloud_fonts,
            }),
            word_grid_drawing: false,
        }
    }
}

/// a body block made ready for placing.
// paragraphs are by far the most common block, so they stay unboxed
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum Prepared {
    Paragraph(PreparedParagraph),
    Table(Box<PreparedTable>),
    PageBreak,
    SectionBreak,
    TextBox,
    Skip,
}

/// lays the whole document out.
pub fn layout(document: &Document, options: &LayoutOptions) -> LayoutResult {
    let fonts = FontLibrary::new(font_source(document, options));
    let mut context = new_context(document, options, fonts);
    let prepared = prepare_body(&mut context);
    let mut pages = place_body(&mut context, &prepared);
    if crate::fields::has_body_page_fields(document) {
        // page count fields in the text show the count a first pass found
        context.page_count_hint = Some(pages.len() as u32);
        pages = place_body(&mut context, &prepared);
    }
    // without an engine to remember blocks, a block's id is its number
    let ids: Vec<u64> = (1..=document.blocks.len() as u64).collect();
    crate::incremental::assign_line_ids(&mut pages, &ids);
    let mut result = finish_context(&mut context, pages);
    if options.word_grid_drawing {
        crate::grid::snap_to_word_grid(&mut result);
    }
    result
}

/// the document's embedded fonts first, then the caller's source.
pub fn font_source(document: &Document, options: &LayoutOptions) -> Arc<dyn FontSource> {
    let faces: Vec<crate::fonts::ProvidedFace> = crate::fonts::embedded::embedded_faces(document)
        .into_iter()
        .map(|face| crate::fonts::ProvidedFace {
            family: face.family,
            bold: face.bold,
            italic: face.italic,
            bytes: face.bytes,
            index: 0,
            origin: crate::fonts::FontOrigin::Embedded,
        })
        .collect();
    if faces.is_empty() {
        return options.fonts.clone();
    }
    Arc::new(crate::fonts::ProvidedFonts {
        faces,
        then: options.fonts.clone(),
    })
}

pub(crate) fn new_context<'d>(
    document: &'d Document,
    options: &LayoutOptions,
    fonts: FontLibrary,
) -> Context<'d> {
    Context {
        document,
        flavor: options.flavor,
        fonts,
        note_numbers: note_numbers(document),
        labels: crate::lists::body_labels(document, options.flavor),
        unsupported: Vec::new(),
        table_style: None,
        page_count_hint: None,
    }
}

/// the column width of the section a block is in.
pub(crate) fn column_width_of(document: &Document, block: usize) -> f64 {
    SectionGeometry::of(&document.section_of(block)).column_width()
}

/// prepares every body block (the part that can be cached).
pub(crate) fn prepare_body(context: &mut Context) -> Vec<Prepared> {
    let document = context.document;
    (0..document.blocks.len())
        .map(|index| prepare_block(context, index))
        .collect()
}

pub(crate) fn prepare_block(context: &mut Context, index: usize) -> Prepared {
    let document = context.document;
    let width = column_width_of(document, index);
    let path = vec![index as u32];
    match &document.blocks[index] {
        Block::Paragraph(_) | Block::Heading(_) => {
            let paragraph = document.blocks[index].paragraph().expect("paragraph block");
            Prepared::Paragraph(context.prepare_paragraph(
                paragraph,
                Story::Body,
                path,
                width,
                FieldValues::default(),
            ))
        }
        Block::Table(table) => Prepared::Table(Box::new(prepare_table(
            context,
            table,
            &Story::Body,
            &path,
            width,
            FieldValues::default(),
            0,
        ))),
        Block::PageBreak => Prepared::PageBreak,
        Block::SectionBreak => Prepared::SectionBreak,
        Block::TextBox(_) => Prepared::TextBox,
        Block::Preserved { .. } => {
            context.unsupported.push(Unsupported::new(
                UnsupportedKind::PreservedBlock,
                &format!("body {index}"),
            ));
            Prepared::Skip
        }
        // shapes and groups (newer in the office model) are not drawn yet;
        // they float, so leaving them out moves no text, and the report
        // says they were left out
        #[allow(unreachable_patterns)]
        _ => {
            context.unsupported.push(Unsupported::new(
                UnsupportedKind::Drawing,
                &format!("body {index}"),
            ));
            Prepared::Skip
        }
    }
}

/// places prepared blocks on pages.
pub(crate) fn place_body(context: &mut Context, prepared: &[Prepared]) -> Vec<Page> {
    let placed = place_body_with(context, prepared, None, None);
    draw_parts(context, placed.body)
}

/// where an incremental relayout starts placing: the unchanged pages
/// before a page start of the old run, and that start.
pub(crate) struct Resume {
    pub pages: Vec<PageState>,
    pub starts: Vec<PageStart>,
    pub start: PageStart,
}

/// body pages before headers and footers, how each began, and the old
/// page the placing caught up with (incremental relayout).
pub(crate) struct Placed {
    pub body: Vec<PageState>,
    pub starts: Vec<PageStart>,
    pub stopped: Option<usize>,
}

/// places prepared blocks, from the start or from `resume`, stopping
/// early when `resync` finds the old layout again.
pub(crate) fn place_body_with(
    context: &mut Context,
    prepared: &[Prepared],
    resume: Option<Resume>,
    resync: Option<ResyncTarget>,
) -> Placed {
    let document = context.document;
    let mut flow = Flow::new(context);
    flow.resync = resync;
    let first = match resume {
        None => {
            flow.section = 0;
            flow.new_page(true);
            Cursor::default()
        }
        Some(resume) => {
            flow.pages = resume.pages;
            flow.starts = resume.starts;
            flow.section = resume.start.section;
            flow.cursor = resume.start.cursor;
            flow.new_page(resume.start.first_of_section);
            flow.after_hard_break = resume.start.after_hard_break;
            if let Some(last) = flow.starts.last_mut() {
                last.after_hard_break = resume.start.after_hard_break;
            }
            resume.start.cursor
        }
    };
    for (index, block) in prepared.iter().enumerate().skip(first.block) {
        let line = if index == first.block { first.line } else { 0 };
        flow.cursor = Cursor {
            block: index,
            line,
            resumable: true,
        };
        match block {
            Prepared::Paragraph(paragraph) => {
                let need = need_after(prepared, index, &flow);
                match with_page_fields(&mut flow, index, paragraph) {
                    Some(fresh) => flow.place_paragraph_from(&fresh, need, line),
                    None => flow.place_paragraph_from(paragraph, need, line),
                }
            }
            Prepared::Table(table) => flow.place_table_from(table, line),
            Prepared::PageBreak => {
                flow.cursor.block = index + 1;
                flow.hard_break(crate::text::ClusterKind::PageBreak);
            }
            Prepared::SectionBreak => {
                flow.cursor.block = index + 1;
                change_section(&mut flow);
            }
            Prepared::TextBox => {
                if let Block::TextBox(text_box) = &document.blocks[index] {
                    flow.place_text_box(text_box, index as u32);
                }
            }
            Prepared::Skip => {}
        }
        if flow.stopped.is_some() {
            break;
        }
    }
    if let Some(stopped) = flow.stopped {
        // the page just opened is the old one met again: it is reused
        flow.pages.pop();
        flow.starts.pop();
        return Placed {
            body: std::mem::take(&mut flow.pages),
            starts: std::mem::take(&mut flow.starts),
            stopped: Some(stopped),
        };
    }
    flow.cursor = Cursor {
        block: prepared.len(),
        line: 0,
        resumable: false,
    };
    place_endnotes(&mut flow);
    let body = flow.finish_body();
    Placed {
        body,
        starts: std::mem::take(&mut flow.starts),
        stopped: None,
    }
}

/// a body paragraph with page number or page count fields, made again
/// with the number of the page it starts on and the count of the last
/// pass (word updates them the same way when it prints).
fn with_page_fields(
    flow: &mut Flow,
    index: usize,
    paragraph: &PreparedParagraph,
) -> Option<PreparedParagraph> {
    let model = flow.context.document.blocks.get(index)?.paragraph()?;
    if !model.content.iter().any(crate::fields::is_page_field) {
        return None;
    }
    let fields = FieldValues {
        page: Some(flow.page_ref().page.number),
        pages: flow.context.page_count_hint,
    };
    Some(flow.context.prepare_paragraph(
        model,
        Story::Body,
        paragraph.path.clone(),
        paragraph.width,
        fields,
    ))
}

/// headers and footers on finished body pages.
pub(crate) fn draw_parts(context: &mut Context, body: Vec<PageState>) -> Vec<Page> {
    let mut flow = Flow::new(context);
    flow.draw_all_parts(body)
}

/// endnotes go after the body, each note's paragraphs in order.
fn place_endnotes(flow: &mut Flow) {
    let document = flow.context.document;
    let mut endnotes: Vec<(&String, &String)> = flow
        .context
        .note_numbers
        .iter()
        .filter(|(id, _)| {
            document
                .note(id)
                .is_some_and(|n| n.kind == letters_office::model::NoteKind::Endnote)
        })
        .collect();
    if endnotes.is_empty() {
        return;
    }
    endnotes.sort_by_key(|(_, shown)| roman_value(shown));
    let ids: Vec<String> = endnotes.into_iter().map(|(id, _)| id.clone()).collect();
    let (_, width) = flow.column_box();
    for id in ids {
        let Some(note) = document.note(&id) else {
            continue;
        };
        for (index, block) in note.blocks.iter().enumerate() {
            if let Some(paragraph) = block.paragraph() {
                let prepared = flow.context.prepare_paragraph(
                    paragraph,
                    Story::Endnote { id: id.clone() },
                    vec![index as u32],
                    width,
                    FieldValues::default(),
                );
                flow.place_paragraph(&prepared, 0.0);
            }
        }
    }
}

fn roman_value(text: &str) -> u32 {
    let value = |c: char| match c {
        'i' => 1,
        'v' => 5,
        'x' => 10,
        'l' => 50,
        'c' => 100,
        'd' => 500,
        'm' => 1000,
        _ => 0,
    };
    let digits: Vec<u32> = text.chars().map(value).collect();
    let mut total: i64 = 0;
    for (i, digit) in digits.iter().enumerate() {
        if digits.get(i + 1).is_some_and(|next| next > digit) {
            total -= i64::from(*digit);
        } else {
            total += i64::from(*digit);
        }
    }
    total.max(0) as u32
}

/// the room the content after a keep-with-next paragraph needs on the
/// same page: the next paragraph's first lines (two with widow control,
/// all with keep lines together), or the first row of a table, and what
/// that next paragraph itself keeps with.
fn need_after(prepared: &[Prepared], index: usize, flow: &Flow) -> f64 {
    let page_room = flow.page_ref().bottom() - flow.page_ref().body_top;
    need_after_depth(prepared, index, 0).min(page_room * 0.8)
}

fn need_after_depth(prepared: &[Prepared], index: usize, depth: usize) -> f64 {
    let Some(Prepared::Paragraph(paragraph)) = prepared.get(index) else {
        return 0.0;
    };
    if !paragraph.style.keep_with_next || depth > 8 {
        return 0.0;
    }
    match prepared.get(index + 1) {
        Some(Prepared::Paragraph(next)) => {
            let lines = &next.layout.lines;
            let wanted = if next.style.keep_together || next.style.keep_with_next {
                lines.len()
            } else if next.style.widow_control {
                lines.len().min(2)
            } else {
                1
            };
            // the last line needed may hang its line spacing into the
            // margin, as any line at the foot of a page may (headings corpus)
            let height: f64 = lines[..wanted]
                .iter()
                .map(|l| l.vertical.height)
                .sum::<f64>()
                - lines[..wanted]
                    .last()
                    .map_or(0.0, |l| l.vertical.height - l.vertical.fit_height);
            paragraph.style.space_after
                + next.style.space_before
                + height
                + need_after_depth(prepared, index + 1, depth + 1)
        }
        Some(Prepared::Table(table)) => {
            paragraph.style.space_after + table.rows.first().map_or(0.0, |row| row.height)
        }
        _ => 0.0,
    }
}

/// a section break: balance the columns of a continuous multi-column
/// section, then start the next section on a new page or below.
fn change_section(flow: &mut Flow) {
    let document = flow.context.document;
    let next = (flow.section + 1).min(document.sections.len().saturating_sub(1));
    if next == flow.section {
        return;
    }
    let start = document.sections[next].start;
    let new_geometry = SectionGeometry::of(&document.sections[next]);
    let same_size = {
        let current = &flow.page_ref().geometry;
        current.width == new_geometry.width && current.height == new_geometry.height
    };
    let continuous = !starts_new_page(start) && same_size;
    if continuous && flow.page_ref().columns.len() > 1 {
        crate::columns::balance_region(flow);
    }
    flow.section = next;
    if continuous {
        // the new region starts below the larger of the last paragraph's
        // space after and the first paragraph's space before, and every
        // column of it starts at that height (two_columns: 8 pt after a
        // paragraph with no space before; real_research_paper: 8 pt for a
        // heading with 8 before after a paragraph with 6 after)
        flow.region_gap = Some(flow.previous.as_ref().map_or(0.0, |p| p.space_after));
        let y = flow.region_bottom();
        flow.previous = None;
        flow.column_empty = true;
        let page = flow.page();
        page.columns = new_geometry.columns();
        page.region_top = y;
        page.geometry.column_separator = new_geometry.column_separator;
        page.active_column = 0;
        flow.column = 0;
        flow.y = y;
        return;
    }
    flow.new_page(true);
    let number = flow.page_ref().page.number;
    let wrong_parity = match start {
        SectionStart::OddPage => number.is_multiple_of(2),
        SectionStart::EvenPage => !number.is_multiple_of(2),
        _ => false,
    };
    if wrong_parity {
        // a blank page first, so the section starts on the side asked for
        flow.new_page(true);
    }
}

/// the result of a finished layout; the context's fonts stay with it.
pub(crate) fn finish_context(context: &mut Context, pages: Vec<Page>) -> LayoutResult {
    let mut pages = pages;
    if pages.is_empty() {
        pages.push(Page::default());
    }
    let mut unsupported = std::mem::take(&mut context.unsupported);
    deduplicate(&mut unsupported);
    let fonts = context.fonts.fonts();
    LayoutResult {
        flavor: context.flavor,
        pages,
        fonts: fonts
            .iter()
            .map(|font| FontInfo {
                family: font.face.family.clone(),
                postscript_name: font.face.postscript_name.clone(),
                bold: font.face.bold,
                italic: font.face.italic,
                synthetic_bold: font.synthetic_bold,
                synthetic_italic: font.synthetic_italic,
                origin: font.face.origin.clone(),
            })
            .collect(),
        faces: fonts.iter().map(|font| font.face.clone()).collect(),
        substitutions: dedup_substitutions(context.fonts.substitutions.clone()),
        unsupported,
    }
}

fn dedup_substitutions(
    mut list: Vec<crate::fonts::FontSubstitution>,
) -> Vec<crate::fonts::FontSubstitution> {
    let mut seen = Vec::new();
    list.retain(|item| {
        let key = (
            item.requested.clone(),
            item.bold,
            item.italic,
            item.used.clone(),
        );
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
    list
}
