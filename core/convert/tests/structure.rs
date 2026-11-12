//! structure analysis of a hand-built three page handbook: headings,
//! lists, a ruled table with its caption, a table of lined-up text, a
//! picture with its caption, a link, a footnote, and a header and page
//! numbers repeated on every page.

mod support;

use letters_convert::*;
use support::*;

fn analyse(name: &str) -> StructuredDocument {
    let dir = temp_dir(name);
    let path = dir.join("handbook.pdf");
    write_pdf(&path, &handbook_pages());
    let source = PdfSource::open(&path).unwrap();
    analyze_structure(
        &source,
        &StructureOptions::default(),
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap()
}

fn kind(block: &Block) -> &'static str {
    match block.content {
        BlockContent::Heading { .. } => "heading",
        BlockContent::Paragraph { .. } => "paragraph",
        BlockContent::List { .. } => "list",
        BlockContent::Table(_) => "table",
        BlockContent::Figure(_) => "figure",
        BlockContent::Footnote { .. } => "footnote",
    }
}

#[test]
fn blocks_come_in_reading_order_with_their_kinds() {
    let document = analyse("kinds");
    let summary: Vec<(usize, &str)> = document.blocks.iter().map(|b| (b.page, kind(b))).collect();
    assert_eq!(
        summary,
        [
            (0, "heading"),
            (0, "paragraph"),
            (0, "heading"),
            (0, "list"),
            (0, "paragraph"),
            (0, "list"),
            (0, "table"),
            (0, "paragraph"),
            (0, "footnote"),
            (1, "table"),
            (1, "figure"),
            (2, "paragraph"),
        ]
    );
    assert_eq!(document.body_size, 11.0);
    assert_eq!(document.pages.len(), 3);
    assert_eq!(document.pages[0].columns, 1);
    for block in &document.blocks {
        assert!((0.0..=1.0).contains(&block.confidence), "{block:?}");
        assert!(block.rect.width > 0.0 && block.rect.height > 0.0);
    }
}

#[test]
fn headings_get_levels_by_size() {
    let document = analyse("headings");
    let headings: Vec<(u8, String)> = document
        .blocks
        .iter()
        .filter_map(|b| match &b.content {
            BlockContent::Heading { level, .. } => Some((*level, b.text())),
            _ => None,
        })
        .collect();
    assert_eq!(
        headings,
        [
            (1, "Chemistry Handbook".to_string()),
            (2, "Safety rules".to_string())
        ]
    );
    assert_eq!(document.title.as_deref(), Some("Chemistry Handbook"));
    let BlockContent::Heading { runs, .. } = &document.blocks[0].content else {
        panic!()
    };
    assert!(runs[0].bold);
    assert_eq!(runs[0].size, 20.0);
    assert_eq!(runs[0].font_family, "Helvetica");
}

#[test]
fn lists_keep_markers_nesting_and_numbering() {
    let document = analyse("lists");
    let lists: Vec<&Vec<ListItem>> = document
        .blocks
        .iter()
        .filter_map(|b| match &b.content {
            BlockContent::List { items } => Some(items),
            _ => None,
        })
        .collect();
    let bullets: Vec<(u8, bool, &str, String)> = lists[0]
        .iter()
        .map(|i| (i.level, i.ordered, i.marker.as_str(), runs_text(&i.runs)))
        .collect();
    assert_eq!(
        bullets,
        [
            (0, false, "•", "Wear goggles at all times".to_string()),
            (0, false, "•", "Tie back long hair".to_string()),
            (1, false, "–", "Use a band or a clip".to_string()),
            (0, false, "•", "Know where the exits are".to_string()),
        ]
    );
    let numbered: Vec<(bool, &str)> = lists[1]
        .iter()
        .map(|i| (i.ordered, i.marker.as_str()))
        .collect();
    assert_eq!(numbered, [(true, "1."), (true, "2.")]);
}

#[test]
fn ruled_tables_have_cells_headers_and_captions() {
    let document = analyse("ruled");
    let BlockContent::Table(table) = &document.blocks[6].content else {
        panic!("{:?}", document.blocks[6]);
    };
    assert!(table.ruled);
    assert_eq!((table.rows, table.columns), (3, 3));
    assert_eq!(table.column_widths, vec![120.0, 90.0, 90.0]);
    assert_eq!(table.row_heights, vec![20.0, 20.0, 20.0]);
    assert_eq!(
        table.text(),
        "Substance\tMelting point\tState\nIce\t0 °C\tsolid\nIron\t1538 °C\tsolid"
    );
    assert_eq!(table.header_rows, 1);
    assert!(table.header_confidence >= 0.8);
    assert!(table.cells.iter().all(|c| c.borders.top && c.borders.left));
    assert_eq!(
        runs_text(table.caption.as_ref().unwrap()),
        "Table 1: Melting points"
    );
    assert!(document.blocks[6].confidence >= 0.9);
}

#[test]
fn lined_up_text_becomes_a_table() {
    let document = analyse("whitespace");
    let block = document
        .blocks
        .iter()
        .find(|b| b.page == 1 && kind(b) == "table")
        .unwrap();
    let BlockContent::Table(table) = &block.content else {
        panic!()
    };
    assert!(!table.ruled);
    assert_eq!((table.rows, table.columns), (4, 3));
    assert_eq!(table.cell(3, 2).unwrap().text(), "$9.99");
    assert_eq!(table.cell(0, 0).unwrap().text(), "Item");
    assert_eq!(table.header_rows, 1);
    assert!(
        block.confidence < 0.9,
        "white space tables are less certain"
    );
}

#[test]
fn figures_links_and_footnotes_are_found() {
    let document = analyse("figures");
    let figure = document
        .blocks
        .iter()
        .find_map(|b| match &b.content {
            BlockContent::Figure(f) => Some((b, f)),
            _ => None,
        })
        .unwrap();
    assert_eq!(figure.0.rect, Rect::new(72.0, 200.0, 144.0, 96.0));
    assert_eq!((figure.1.pixel_width, figure.1.pixel_height), (4, 4));
    assert_eq!(
        runs_text(figure.1.caption.as_ref().unwrap()),
        "Figure 1: A flame test."
    );
    assert!(figure.1.caption_confidence >= 0.8);

    let link = document
        .blocks
        .iter()
        .find(|b| b.text().starts_with("Read the full guide"))
        .unwrap();
    let BlockContent::Paragraph { runs } = &link.content else {
        panic!()
    };
    assert!(runs
        .iter()
        .any(|r| r.link.as_deref() == Some("https://example.org/guide")));

    let note = document
        .blocks
        .iter()
        .find(|b| kind(b) == "footnote")
        .unwrap();
    let BlockContent::Footnote { marker, runs } = &note.content else {
        panic!()
    };
    assert_eq!(marker, "1");
    assert_eq!(runs_text(runs), "Measured at normal air pressure.");
    assert!(
        note.confidence >= 0.8,
        "the rule above makes it more certain"
    );
}

#[test]
fn headers_and_page_numbers_repeat() {
    let document = analyse("repeated");
    assert_eq!(document.repeated.len(), 2);
    let header = document
        .repeated
        .iter()
        .find(|r| r.kind == RepeatedKind::Header)
        .unwrap();
    assert_eq!(header.text, "Course Handbook");
    assert_eq!(header.pages, vec![0, 1, 2]);
    assert!(!header.page_number);
    let footer = document
        .repeated
        .iter()
        .find(|r| r.kind == RepeatedKind::Footer)
        .unwrap();
    assert!(footer.page_number);
    assert_eq!(footer.pages, vec![0, 1, 2]);
    // neither shows up in the body
    assert!(!document
        .blocks
        .iter()
        .any(|b| b.text().contains("Course Handbook")));
}

#[test]
fn the_model_serialises_for_the_writers() {
    let document = analyse("serde");
    let json = serde_json::to_value(&document).unwrap();
    assert_eq!(json["blocks"][0]["type"], "heading");
    assert_eq!(json["blocks"][0]["level"], 1);
    assert_eq!(json["blocks"][0]["runs"][0]["fontFamily"], "Helvetica");
    assert_eq!(json["blocks"][0]["page"], 0);
    assert!(json["blocks"][0]["rect"]["width"].as_f64().unwrap() > 0.0);
    assert_eq!(json["repeated"][0]["kind"], "header");
    let back: StructuredDocument = serde_json::from_value(json).unwrap();
    assert_eq!(back, document);
}
