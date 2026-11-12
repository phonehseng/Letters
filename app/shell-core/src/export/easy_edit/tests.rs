//! the mapping, on structures made by hand the way the analysis makes
//! them, written as .docx and .odt and read back with the office engine.

use letters_convert::{
    Block, BlockContent, CellAlign, CellBorders, Figure, GenericFamily, ListItem, PageInfo, Rect,
    RepeatedKind, RepeatedText, Rgb, StructuredDocument, Table, TableCell, TextRun,
};
use letters_office::model::{Block as OfficeBlock, Inline, Mark};

use super::*;

pub fn run(text: &str) -> TextRun {
    TextRun {
        text: text.into(),
        font_family: "Times New Roman".into(),
        generic_family: GenericFamily::Serif,
        bold: false,
        italic: false,
        size: 12.0,
        color: Rgb::BLACK,
        link: None,
        superscript: false,
        font_ref: String::new(),
    }
}

fn raised(text: &str) -> TextRun {
    TextRun {
        superscript: true,
        size: 7.0,
        ..run(text)
    }
}

fn block(page: usize, y: f32, content: BlockContent) -> Block {
    Block {
        page,
        rect: Rect::new(72.0, y, 468.0, 20.0),
        confidence: 0.9,
        content,
    }
}

fn item(level: u8, ordered: bool, text: &str) -> ListItem {
    ListItem {
        level,
        ordered,
        marker: if ordered { "1.".into() } else { "•".into() },
        runs: vec![run(text)],
        rect: Rect::new(90.0, 0.0, 400.0, 14.0),
    }
}

fn cell(row: usize, column: usize, text: &str, column_span: usize) -> TableCell {
    TableCell {
        row,
        column,
        row_span: 1,
        column_span,
        runs: vec![run(text)],
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        fill: (row == 0).then_some(Rgb(220, 220, 220)),
        borders: CellBorders {
            top: true,
            bottom: true,
            left: true,
            right: true,
        },
        align: if column == 1 {
            CellAlign::Right
        } else {
            CellAlign::Left
        },
    }
}

/// a two page handbook: a letter page, then an a4 page, with every kind
/// of block the analysis makes.
pub fn handbook() -> StructuredDocument {
    let mut blocks = first_page_blocks();
    blocks.extend(second_page_blocks());
    StructuredDocument {
        title: Some("Study Handbook".into()),
        pages: vec![page_info(0, 612.0, 792.0), page_info(1, 595.0, 842.0)],
        blocks,
        repeated: repeated_header_and_footer(),
        body_size: 12.0,
    }
}

fn page_info(page: usize, width: f32, height: f32) -> PageInfo {
    PageInfo {
        page,
        width,
        height,
        columns: 1,
    }
}

/// a run in a heading's size, bold, as the analysis reports headings.
fn heading_run(text: &str, size: f32) -> TextRun {
    let mut heading = run(text);
    heading.size = size;
    heading.bold = true;
    heading
}

/// the letter page: two headings, a paragraph with a note marker, a
/// nested bulleted list, a numbered list and the footnote.
fn first_page_blocks() -> Vec<Block> {
    let title = BlockContent::Heading {
        level: 1,
        runs: vec![heading_run("Study Handbook", 20.0)],
    };
    let paragraph = BlockContent::Paragraph {
        runs: vec![run("Good notes save time."), raised("1")],
    };
    let section = BlockContent::Heading {
        level: 2,
        runs: vec![heading_run("Reading Well", 15.0)],
    };
    let bullets = BlockContent::List {
        items: vec![
            item(0, false, "Skim first"),
            item(1, false, "Look at headings"),
            item(0, false, "Then read"),
        ],
    };
    let numbers = BlockContent::List {
        items: vec![item(0, true, "Preview"), item(0, true, "Question")],
    };
    let footnote = BlockContent::Footnote {
        marker: "1".into(),
        runs: vec![run("Smith, 2020.")],
    };
    vec![
        block(0, 72.0, title),
        block(0, 110.0, paragraph),
        block(0, 140.0, section),
        block(0, 170.0, bullets),
        block(0, 230.0, numbers),
        block(0, 700.0, footnote),
    ]
}

/// the a4 page: a ruled table with a merged last row, and a picture.
fn second_page_blocks() -> Vec<Block> {
    let figure = BlockContent::Figure(Figure {
        image_id: "image:5-0".into(),
        pixel_width: 2,
        pixel_height: 2,
        caption: Some(vec![run("Figure 1: A dot")]),
        caption_confidence: 0.9,
    });
    vec![
        block(1, 80.0, BlockContent::Table(costs_table())),
        block(1, 200.0, figure),
    ]
}

fn costs_table() -> Table {
    Table {
        rows: 3,
        columns: 2,
        column_widths: vec![200.0, 100.0],
        row_heights: vec![18.0, 18.0, 18.0],
        cells: vec![
            cell(0, 0, "Item", 1),
            cell(0, 1, "Cost", 1),
            cell(1, 0, "Books", 1),
            cell(1, 1, "$40.00", 1),
            cell(2, 0, "Total for the term", 2),
        ],
        header_rows: 1,
        header_confidence: 0.8,
        ruled: true,
        caption: Some(vec![run("Table 1: Costs")]),
    }
}

/// the title repeated at the top, and "Page 1 of 2" at the bottom.
fn repeated_header_and_footer() -> Vec<RepeatedText> {
    vec![
        RepeatedText {
            kind: RepeatedKind::Header,
            text: "Study Handbook".into(),
            page_number: false,
            pages: vec![0, 1],
            rect: Rect::new(72.0, 30.0, 100.0, 10.0),
            confidence: 0.9,
        },
        RepeatedText {
            kind: RepeatedKind::Footer,
            text: "Page 1 of 2".into(),
            page_number: true,
            pages: vec![0, 1],
            rect: Rect::new(280.0, 760.0, 50.0, 10.0),
            confidence: 0.9,
        },
    ]
}
/// a 2 by 2 png made with the png crate, standing in for the pdf's picture.
pub struct OnePicture;

impl PictureSource for OnePicture {
    fn picture(&self, _page: usize, image_id: &str) -> Option<Vec<u8>> {
        (image_id == "image:5-0").then(tiny_png)
    }
}

pub fn tiny_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255])
            .unwrap();
    }
    bytes
}

struct NoPictures;

impl PictureSource for NoPictures {
    fn picture(&self, _page: usize, _image_id: &str) -> Option<Vec<u8>> {
        None
    }
}

fn texts(document: &Document) -> Vec<String> {
    document
        .blocks
        .iter()
        .filter_map(|block| block.paragraph().map(|p| p.plain_text()))
        .collect()
}

#[test]
fn every_kind_of_block_is_rebuilt() {
    let (document, report) = build_document(&handbook(), &OnePicture);
    assert_eq!(report.headings, 2);
    assert_eq!(report.paragraphs, 1);
    assert_eq!(report.lists, 2);
    assert_eq!(report.tables, 1);
    assert_eq!(report.pictures, 1);
    assert_eq!(report.footnotes, 1);
    assert!(report.header && report.footer);
    assert_eq!(report.sections, 2);
    let headings: Vec<u8> = document
        .blocks
        .iter()
        .filter_map(|block| match block {
            OfficeBlock::Heading(heading) => Some(heading.level),
            _ => None,
        })
        .collect();
    assert_eq!(headings, vec![1, 2]);
    assert!(texts(&document).contains(&"Skim first".to_string()));
}

#[test]
fn headings_use_the_documents_heading_styles() {
    let (document, _) = build_document(&handbook(), &OnePicture);
    let OfficeBlock::Heading(heading) = &document.blocks[0] else {
        panic!("the first block is the title heading");
    };
    let style = document
        .style(heading.paragraph.style_id.as_deref().unwrap())
        .unwrap();
    assert_eq!(style.name, "heading 1");
}

#[test]
fn lists_nest_and_numbered_lists_restart() {
    let (document, _) = build_document(&handbook(), &OnePicture);
    let lists: Vec<(String, u8, String)> = document
        .blocks
        .iter()
        .filter_map(|block| {
            let paragraph = block.paragraph()?;
            let list = paragraph.props.list.as_ref()?;
            Some((list.list_id.clone(), list.level, paragraph.plain_text()))
        })
        .collect();
    assert_eq!(lists.len(), 5);
    assert_eq!(lists[1].1, 1, "the nested item is one level in");
    assert_eq!(lists[0].0, lists[2].0, "bullets share one list");
    assert_ne!(lists[0].0, lists[3].0, "the numbered list is another list");
    assert_eq!(lists[3].0, lists[4].0, "one numbered list keeps counting");
    let numbered = document.numbering.instance(&lists[3].0).unwrap();
    assert_eq!(numbered.definition_id, NUMBERS);
}

#[test]
fn the_footnote_replaces_the_raised_marker() {
    let (document, _) = build_document(&handbook(), &OnePicture);
    let OfficeBlock::Paragraph(paragraph) = &document.blocks[1] else {
        panic!("the body paragraph");
    };
    assert_eq!(paragraph.content.len(), 2);
    assert!(
        matches!(&paragraph.content[1], Inline::NoteReference { note_id, .. } if note_id == "1")
    );
    assert_eq!(document.notes.len(), 1);
    assert_eq!(
        document.notes[0].blocks[0]
            .paragraph()
            .unwrap()
            .plain_text(),
        "Smith, 2020."
    );
}

#[test]
fn tables_keep_spans_widths_header_and_shading() {
    let (document, _) = build_document(&handbook(), &OnePicture);
    let table = document
        .blocks
        .iter()
        .find_map(|block| match block {
            OfficeBlock::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    assert_eq!(table.column_widths, vec![4000, 2000]);
    assert!(table.rows[0].header && !table.rows[1].header);
    assert_eq!(table.rows[2].cells.len(), 1);
    assert_eq!(table.rows[2].cells[0].col_span, 2);
    assert_eq!(table.rows[0].cells[0].shading.as_deref(), Some("DCDCDC"));
    assert!(table.rows[0].cells[0]
        .borders
        .as_ref()
        .unwrap()
        .top
        .is_some());
}

#[test]
fn sections_follow_page_sizes_with_a_break_between() {
    let (document, _) = build_document(&handbook(), &OnePicture);
    assert_eq!(document.sections.len(), 2);
    assert_eq!(document.sections[0].page.width, 12240);
    assert_eq!(document.sections[1].page.width, 11900);
    let breaks = document
        .blocks
        .iter()
        .filter(|block| matches!(block, OfficeBlock::SectionBreak))
        .count();
    assert_eq!(breaks, 1);
    assert_eq!(
        document.sections[1].footers.default.as_deref(),
        Some("letters-footer")
    );
}

#[test]
fn a_picture_that_cant_be_copied_is_counted_not_lost_silently() {
    let (document, report) = build_document(&handbook(), &NoPictures);
    assert_eq!(report.pictures, 0);
    assert_eq!(report.pictures_left_out, 1);
    // its caption is still there
    assert!(texts(&document).contains(&"Figure 1: A dot".to_string()));
}

#[test]
fn the_body_font_lives_in_the_normal_style() {
    let (document, _) = build_document(&handbook(), &OnePicture);
    let normal = document
        .styles
        .styles
        .iter()
        .find(|style| style.is_default)
        .unwrap();
    assert_eq!(normal.run.font.as_ref().unwrap().name, "Times New Roman");
    assert_eq!(normal.run.size, Some(24));
    // so the plain body text carries no marks of its own
    let OfficeBlock::Paragraph(paragraph) = &document.blocks[1] else {
        panic!("the body paragraph");
    };
    assert_eq!(paragraph.content[0], Inline::text("Good notes save time."));
    assert!(!paragraph.content[0].marks().contains(&Mark::Bold));
}

#[test]
fn docx_and_odt_round_trip_through_the_office_engine() {
    let (document, _) = build_document(&handbook(), &OnePicture);
    let docx = letters_office::write_docx(&document).unwrap();
    let back = letters_office::read_docx(&docx).unwrap();
    assert_eq!(texts(&back), texts(&document));
    assert_eq!(
        back.notes
            .iter()
            .filter(|note| note.special.is_none())
            .count(),
        1
    );
    assert_eq!(back.media.len(), 1);
    let odt = letters_office::write_odt(&document).unwrap();
    let back = letters_office::read_odt(&odt).unwrap();
    assert!(texts(&back).contains(&"Study Handbook".to_string()));
    assert!(back
        .blocks
        .iter()
        .any(|block| matches!(block, OfficeBlock::Table(_))));
}

#[test]
fn an_empty_structure_still_makes_a_document() {
    let empty = StructuredDocument {
        title: None,
        pages: vec![PageInfo {
            page: 0,
            width: 612.0,
            height: 792.0,
            columns: 1,
        }],
        blocks: vec![],
        repeated: vec![],
        body_size: 0.0,
    };
    let (document, report) = build_document(&empty, &NoPictures);
    assert_eq!(document.blocks.len(), 1);
    assert_eq!(report.paragraphs, 0);
    assert!(letters_office::write_docx(&document).is_ok());
}
