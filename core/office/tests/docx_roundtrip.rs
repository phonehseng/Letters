//! docx -> model -> docx: the model survives a save, and everything the
//! model does not understand is written back byte for byte.

mod support;

use letters_office::model::*;
use letters_office::{read_docx, write_docx};
use support::fixtures::*;
use support::*;

/// the parts of two models that must match after a save. the carried
/// package bookkeeping is compared separately, because a first save adds
/// the parts every docx should have (core properties, settings).
fn assert_same_content(a: &Document, b: &Document) {
    assert_eq!(a.blocks, b.blocks, "blocks differ");
    assert_eq!(a.styles, b.styles, "styles differ");
    assert_eq!(a.numbering, b.numbering, "numbering differs");
    assert_eq!(a.sections, b.sections, "sections differ");
    assert_eq!(a.headers_footers, b.headers_footers, "headers differ");
    assert_eq!(a.notes, b.notes, "notes differ");
    assert_eq!(a.comments, b.comments, "comments differ");
    assert_eq!(a.media, b.media, "media differ");
    assert_eq!(a.metadata, b.metadata, "metadata differ");
    assert_eq!(a.settings, b.settings, "settings differ");
}

#[test]
fn a_rich_document_survives_write_and_read() {
    let first = read_docx(&rich_docx()).unwrap();
    let saved = write_docx(&first).unwrap();
    let second = read_docx(&saved).unwrap();
    assert_same_content(&first, &second);
    // and from then on the whole model, carry included, is a fixed point
    let third = read_docx(&write_docx(&second).unwrap()).unwrap();
    assert_eq!(second, third);
}

#[test]
fn the_rich_document_is_read_as_expected() {
    let document = read_docx(&rich_docx()).unwrap();
    assert!(matches!(
        document.blocks[0],
        Block::Heading(Heading { level: 1, .. })
    ));
    assert_eq!(document.sections.len(), 2);
    assert_eq!(document.media.len(), 1);
    assert_eq!(document.comments.len(), 1);
    assert_eq!(document.notes.len(), 3);
    assert_eq!(document.headers_footers.len(), 2);
    // the chart, the shape, the content control and the equation are kept
    let json = serde_json::to_string(&document.blocks).unwrap();
    assert!(json.contains("c:chart"));
    assert!(json.contains("mc:AlternateContent"));
    assert!(json.contains("w:sdt"));
    assert!(json.contains("m:oMathPara"));
}

/// every piece of preserved xml in the model, wherever it sits.
fn preserved_pieces(document: &Document) -> Vec<String> {
    let mut pieces = Vec::new();
    collect_blocks(&document.blocks, &mut pieces);
    for part in &document.headers_footers {
        collect_blocks(&part.blocks, &mut pieces);
    }
    pieces
}

fn collect_blocks(blocks: &[Block], pieces: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Preserved { xml } => pieces.push(xml.clone()),
            Block::Table(table) => {
                pieces.extend(table.preserved.children.iter().cloned());
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_blocks(&cell.blocks, pieces);
                    }
                }
            }
            _ => {
                if let Some(paragraph) = block.paragraph() {
                    pieces.extend(paragraph.props.preserved.iter().cloned());
                    for inline in &paragraph.content {
                        match inline {
                            Inline::Preserved { xml } | Inline::PreservedRun { xml, .. } => {
                                pieces.push(xml.clone())
                            }
                            _ => {}
                        }
                        for mark in inline.marks() {
                            if let Mark::Preserved { xml } = mark {
                                pieces.push(xml.clone());
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn editing_one_word_keeps_every_unknown_piece_and_part_byte_for_byte() {
    let original = rich_docx();
    let mut document = read_docx(&original).unwrap();
    let pieces = preserved_pieces(&document);
    assert!(
        pieces.len() >= 8,
        "expected many preserved pieces, got {}",
        pieces.len()
    );

    // change one word, as a person would
    let Block::Paragraph(paragraph) = &mut document.blocks[1] else {
        panic!()
    };
    let Inline::Text { text, .. } = &mut paragraph.content[1] else {
        panic!()
    };
    assert_eq!(text, "words");
    *text = "letters".into();
    let saved = write_docx(&document).unwrap();

    let body = part_text(&saved, "word/document.xml");
    assert!(body.contains("letters"));
    for piece in &pieces {
        assert!(body.contains(piece.as_str()), "lost: {piece}");
    }
    // the attributes word uses to tie comments to paragraphs survive too
    assert!(body.contains("w14:paraId=\"1A2B3C4D\""));
    assert!(body.contains("mc:Ignorable=\"w14\""));
    assert!(body.contains(UNKNOWN_RUN_PROPERTY));

    // parts the model does not read are identical
    let before = parts_of(&original);
    let after = parts_of(&saved);
    for name in [
        "word/charts/chart1.xml",
        "word/charts/_rels/chart1.xml.rels",
        "word/embeddings/Sheet1.xlsx",
        "customXml/item1.xml",
        "word/media/image1.png",
    ] {
        let a = before.iter().find(|(n, _)| n == name).map(|(_, b)| b);
        let b = after.iter().find(|(n, _)| n == name).map(|(_, b)| b);
        assert_eq!(a, b, "{name} changed");
        assert!(b.is_some(), "{name} missing");
    }
    // the chart's relationship keeps its id, so the preserved drawing still finds it
    let rels = part_text(&saved, "word/_rels/document.xml.rels");
    assert!(rels.contains("Id=\"rIdChart\""));
    assert!(rels.contains("charts/chart1.xml"));
    // the unchanged picture is written back with its original xml
    assert!(body.contains("<a:alphaModFix amt=\"50000\"/>"));
}

#[test]
fn a_changed_picture_gets_fresh_drawing_xml() {
    let mut document = read_docx(&rich_docx()).unwrap();
    let mut found = false;
    for block in &mut document.blocks {
        if let Some(paragraph) = block.paragraph_mut() {
            for inline in &mut paragraph.content {
                if let Inline::Image(image) = inline {
                    image.width *= 2;
                    found = true;
                }
            }
        }
    }
    assert!(found);
    let saved = write_docx(&document).unwrap();
    let again = read_docx(&saved).unwrap();
    let image = again
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .flat_map(|p| p.content.iter())
        .find_map(|inline| match inline {
            Inline::Image(image) => Some(image.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(image.width, 2880);
    assert_eq!(again.media[0].bytes, png());
}

#[test]
fn styles_numbering_and_settings_keep_unknown_pieces() {
    let original = rich_docx();
    let saved = write_docx(&read_docx(&original).unwrap()).unwrap();
    let styles = part_text(&saved, "word/styles.xml");
    assert!(styles.contains("<w:latentStyles w:defLockedState=\"0\" w:count=\"376\"/>"));
    assert!(styles.contains("<w:uiPriority w:val=\"9\"/>"));
    let numbering = part_text(&saved, "word/numbering.xml");
    assert!(numbering.contains("<w:multiLevelType w:val=\"hybridMultilevel\"/>"));
    assert!(numbering.contains("w:hint=\"default\""));
}

#[test]
fn settings_are_patched_not_replaced() {
    let settings = "<w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
        <w:zoom w:percent=\"120\"/><w:defaultTabStop w:val=\"720\"/><w:characterSpacingControl w:val=\"doNotCompress\"/>\
        <w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/></w:compat>\
        <w:rsids><w:rsidRoot w:val=\"00AB12CD\"/></w:rsids></w:settings>";
    let bytes = Docx::new(&p("x"))
        .related(
            "rIdSettings",
            "settings",
            "word/settings.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
            settings,
        )
        .build();
    let mut document = read_docx(&bytes).unwrap();
    assert_eq!(document.settings.default_tab_stop, Some(720));
    document.settings.track_changes = true;
    document.settings.even_and_odd_headers = true;
    let saved = write_docx(&document).unwrap();
    let text = part_text(&saved, "word/settings.xml");
    assert!(text.contains("<w:zoom w:percent=\"120\"/>"));
    assert!(text.contains("<w:rsidRoot w:val=\"00AB12CD\"/>"));
    // trackRevisions sits before defaultTabStop, evenAndOddHeaders after it
    let track = text.find("w:trackRevisions").unwrap();
    let tab = text.find("w:defaultTabStop").unwrap();
    let even = text.find("w:evenAndOddHeaders").unwrap();
    let compat = text.find("w:compat>").unwrap();
    assert!(track < tab && tab < even && even < compat, "{text}");
    let again = read_docx(&saved).unwrap();
    assert!(again.settings.track_changes && again.settings.even_and_odd_headers);
}

#[test]
fn a_document_built_from_nothing_writes_and_reads_back() {
    let mut document = Document::new();
    document.metadata.title = Some("Built".into());
    document.blocks = vec![
        Block::Heading(Heading {
            level: 1,
            paragraph: Paragraph::with_text("Title"),
        }),
        Block::text("Body text with \"quotes\" & <brackets>."),
        Block::PageBreak,
        Block::Paragraph(Paragraph {
            content: vec![
                Inline::styled("bold", vec![Mark::Bold]),
                Inline::Tab { marks: vec![] },
                Inline::Field(Field::page_number()),
            ],
            ..Default::default()
        }),
    ];
    let saved = write_docx(&document).unwrap();
    let again = read_docx(&saved).unwrap();
    assert_eq!(again.metadata.title.as_deref(), Some("Built"));
    assert_eq!(again.blocks.len(), 4);
    assert!(matches!(&again.blocks[0], Block::Heading(h) if h.level == 1));
    assert_eq!(
        again.blocks[1].paragraph().unwrap().plain_text(),
        "Body text with \"quotes\" & <brackets>."
    );
    assert!(matches!(again.blocks[2], Block::PageBreak));
    assert_eq!(again.blocks[3], document.blocks[3]);
}

#[test]
fn tracked_changes_and_comments_survive_a_save() {
    let first = read_docx(&rich_docx()).unwrap();
    let saved = write_docx(&first).unwrap();
    let body = part_text(&saved, "word/document.xml");
    assert!(body.contains("<w:del w:id=\"7\" w:author=\"Ada\" w:date=\"2026-01-02T03:04:05Z\">"));
    assert!(body.contains("<w:delText xml:space=\"preserve\">old</w:delText>"));
    assert!(body.contains("<w:ins w:id=\"8\" w:author=\"Ben\""));
    assert!(body.contains("<w:commentRangeStart w:id=\"0\"/>"));
    assert!(body.contains("<w:commentRangeEnd w:id=\"0\"/>"));
    let comments = part_text(&saved, "word/comments.xml");
    assert!(comments.contains("w:author=\"Prof\""));
    assert!(comments.contains("Cite this."));
}

#[test]
fn ids_from_other_formats_become_numbers() {
    let mut document = Document::new();
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("L1", true));
    document.numbering.instances.push(ListInstance {
        id: "list-a".into(),
        definition_id: "L1".into(),
        overrides: vec![],
        preserved: Preserved::default(),
    });
    let mut paragraph = Paragraph::with_text("item");
    paragraph.props.list = Some(ListRef {
        list_id: "list-a".into(),
        level: 0,
    });
    document.blocks = vec![
        Block::Paragraph(paragraph),
        Block::Paragraph(Paragraph {
            content: vec![
                Inline::text("claim"),
                Inline::NoteReference {
                    note_id: "ftn1".into(),
                    marks: vec![],
                },
            ],
            ..Default::default()
        }),
    ];
    document.notes.push(Note {
        id: "ftn1".into(),
        kind: NoteKind::Footnote,
        special: None,
        blocks: vec![Block::text("a note")],
    });
    let saved = write_docx(&document).unwrap();
    let numbering = part_text(&saved, "word/numbering.xml");
    assert!(numbering.contains("w:abstractNumId=\"0\""));
    assert!(numbering.contains("<w:num w:numId=\"1\">"));
    let footnotes = part_text(&saved, "word/footnotes.xml");
    assert!(footnotes.contains("w:type=\"separator\" w:id=\"-1\""));
    assert!(footnotes.contains("<w:footnote w:id=\"1\">"));
    let again = read_docx(&saved).unwrap();
    let labels = letters_office::numbering::list_labels(&again);
    assert_eq!(labels[0].as_deref(), Some("1."));
    assert_eq!(
        again.note("fn1").unwrap().blocks[0]
            .paragraph()
            .unwrap()
            .plain_text(),
        "a note"
    );
}
