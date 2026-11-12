//! odt reading and writing, and conversions between odt and docx.

mod support;

use letters_office::model::*;
use letters_office::{open, read_docx, read_odt, write_docx, write_odt};
use support::fixtures::rich_docx;
use support::*;

const STYLES: &str = "<office:font-face-decls><style:font-face style:name=\"Liberation Serif\" svg:font-family=\"'Liberation Serif'\"/></office:font-face-decls>\
    <office:styles>\
      <style:default-style style:family=\"paragraph\"><style:paragraph-properties fo:margin-bottom=\"0.0835in\"/><style:text-properties fo:font-size=\"12pt\"/></style:default-style>\
      <style:style style:name=\"Standard\" style:family=\"paragraph\"><style:text-properties style:font-name=\"Liberation Serif\"/></style:style>\
      <style:style style:name=\"Heading\" style:family=\"paragraph\" style:parent-style-name=\"Standard\"><style:paragraph-properties fo:keep-with-next=\"always\"/><style:text-properties fo:font-weight=\"bold\"/></style:style>\
      <style:style style:name=\"Heading_20_1\" style:display-name=\"Heading 1\" style:family=\"paragraph\" style:parent-style-name=\"Heading\" style:default-outline-level=\"1\"><style:text-properties fo:font-size=\"18pt\"/></style:style>\
      <style:style style:name=\"Heading_20_2\" style:display-name=\"Heading 2\" style:family=\"paragraph\" style:parent-style-name=\"Heading\" style:default-outline-level=\"2\"/>\
      <style:style style:name=\"Emphasis\" style:family=\"text\"><style:text-properties fo:font-style=\"italic\"/></style:style>\
      <text:list-style style:name=\"Numbering_20_123\"><text:list-level-style-number text:level=\"1\" style:num-suffix=\".\" style:num-format=\"1\"/>\
        <text:list-level-style-number text:level=\"2\" style:num-suffix=\")\" style:num-format=\"a\"/></text:list-style>\
      <text:notes-configuration text:note-class=\"footnote\" style:num-format=\"1\" text:start-value=\"0\"/>\
    </office:styles>\
    <office:automatic-styles>\
      <style:page-layout style:name=\"pm1\"><style:page-layout-properties fo:page-width=\"8.5in\" fo:page-height=\"11in\" fo:margin-top=\"0.5in\" fo:margin-bottom=\"1in\" fo:margin-left=\"1in\" fo:margin-right=\"1in\"/>\
        <style:header-style><style:header-footer-properties fo:min-height=\"0.2in\" fo:margin-bottom=\"0.3in\"/></style:header-style></style:page-layout>\
      <style:style style:name=\"MP1\" style:family=\"paragraph\" style:parent-style-name=\"Standard\"><style:paragraph-properties fo:text-align=\"end\"/></style:style>\
    </office:automatic-styles>\
    <office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\">\
      <style:header><text:p text:style-name=\"MP1\">Running head <text:page-number text:select-page=\"current\">1</text:page-number></text:p></style:header>\
      <style:footer><text:p>Footer text</text:p></style:footer></style:master-page></office:master-styles>";

const AUTOMATIC: &str = "<style:style style:name=\"P1\" style:family=\"paragraph\" style:parent-style-name=\"Standard\">\
      <style:paragraph-properties fo:text-align=\"center\" fo:margin-left=\"0.5in\" fo:text-indent=\"-0.25in\" fo:line-height=\"200%\" fo:hyphenation-ladder-count=\"no-limit\"/></style:style>\
    <style:style style:name=\"T1\" style:family=\"text\"><style:text-properties fo:font-weight=\"bold\" fo:color=\"#ff0000\" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"/></style:style>\
    <style:style style:name=\"Table1\" style:family=\"table\"><style:table-properties style:width=\"6in\" table:align=\"margins\"/></style:style>\
    <style:style style:name=\"Table1.A\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"2in\"/></style:style>\
    <style:style style:name=\"Table1.A1\" style:family=\"table-cell\"><style:table-cell-properties fo:background-color=\"#d9e2f3\" fo:border=\"0.5pt solid #000000\"/></style:style>\
    <style:style style:name=\"fr1\" style:family=\"graphic\"><style:graphic-properties style:wrap=\"parallel\"/></style:style>";

fn content() -> String {
    String::from(
        "<text:tracked-changes>\
           <text:changed-region text:id=\"ct1\"><text:insertion><office:change-info><dc:creator>Ben</dc:creator><dc:date>2026-01-02T03:05:00</dc:date></office:change-info></text:insertion></text:changed-region>\
           <text:changed-region text:id=\"ct2\"><text:deletion><office:change-info><dc:creator>Ada</dc:creator><dc:date>2026-01-02T03:04:05</dc:date></office:change-info><text:p>old</text:p></text:deletion></text:changed-region>\
         </text:tracked-changes>\
         <text:sequence-decls><text:sequence-decl text:display-outline-level=\"0\" text:name=\"Figure\"/></text:sequence-decls>\
         <text:h text:style-name=\"Heading_20_1\" text:outline-level=\"1\">Introduction</text:h>\
         <text:p text:style-name=\"P1\">Hello <text:span text:style-name=\"T1\">bold red</text:span>  and <text:span text:style-name=\"Emphasis\">soft</text:span><text:s text:c=\"2\"/>text<text:tab/>after<text:line-break/>next.</text:p>\
         <text:p>The <text:change text:change-id=\"ct2\"/><text:change-start text:change-id=\"ct1\"/>new<text:change-end text:change-id=\"ct1\"/> word<text:note text:id=\"ftn1\" text:note-class=\"footnote\"><text:note-citation>1</text:note-citation><text:note-body><text:p>A note.</text:p></text:note-body></text:note>.</text:p>\
         <text:p>See <text:a xlink:type=\"simple\" xlink:href=\"https://example.org/\">the site</text:a> and <text:bookmark-start text:name=\"mark\"/>here<text:bookmark-end text:name=\"mark\"/>.\
           <office:annotation office:name=\"__Annotation__1\"><dc:creator>Prof</dc:creator><dc:date>2026-03-01T10:00:00</dc:date><text:p>Cite this.</text:p></office:annotation>noted<office:annotation-end office:name=\"__Annotation__1\"/></text:p>\
         <text:h text:style-name=\"Heading_20_2\" text:outline-level=\"2\">Lists</text:h>\
         <text:list xml:id=\"list1\" text:style-name=\"Numbering_20_123\"><text:list-item><text:p>one</text:p>\
           <text:list><text:list-item><text:p>one a</text:p></text:list-item></text:list></text:list-item>\
           <text:list-item><text:p>two</text:p></text:list-item></text:list>\
         <text:p>between</text:p>\
         <text:list text:continue-numbering=\"true\" text:style-name=\"Numbering_20_123\"><text:list-item><text:p>three</text:p></text:list-item></text:list>\
         <text:list text:style-name=\"Numbering_20_123\"><text:list-item><text:p>restart</text:p></text:list-item></text:list>\
         <table:table table:name=\"Table1\" table:style-name=\"Table1\"><table:table-column table:style-name=\"Table1.A\" table:number-columns-repeated=\"3\"/>\
           <table:table-header-rows><table:table-row><table:table-cell table:style-name=\"Table1.A1\" table:number-columns-spanned=\"2\" office:value-type=\"string\"><text:p>wide</text:p></table:table-cell><table:covered-table-cell/>\
             <table:table-cell table:number-rows-spanned=\"2\" office:value-type=\"string\"><text:p>tall</text:p></table:table-cell></table:table-row></table:table-header-rows>\
           <table:table-row><table:table-cell office:value-type=\"string\"><text:p>c</text:p></table:table-cell><table:table-cell office:value-type=\"string\"><text:p>d</text:p></table:table-cell><table:covered-table-cell/></table:table-row></table:table>\
         <text:p>Picture <draw:frame draw:style-name=\"fr1\" draw:name=\"Image1\" text:anchor-type=\"as-char\" svg:width=\"1in\" svg:height=\"0.5in\" draw:z-index=\"0\">\
           <draw:image xlink:href=\"Pictures/dot.png\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/><svg:desc>A dot</svg:desc></draw:frame>\
           <draw:frame draw:name=\"Box\" text:anchor-type=\"paragraph\" svg:width=\"2in\"><draw:text-box><text:p>In a box</text:p></draw:text-box></draw:frame></text:p>\
         <text:table-of-content text:name=\"Contents\"><text:index-body><text:p>Introduction 1</text:p></text:index-body></text:table-of-content>"
    )
}

fn sample() -> Vec<u8> {
    let settings = b"<?xml version=\"1.0\"?><office:document-settings xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\"/>".to_vec();
    odt_with(
        &content(),
        STYLES,
        AUTOMATIC,
        &[
            ("Pictures/dot.png", png()),
            ("settings.xml", settings),
            ("Thumbnails/thumbnail.png", png()),
        ],
    )
}

#[test]
fn reads_text_headings_and_formatting() {
    let document = read_odt(&sample()).unwrap();
    assert_eq!(document.carry.origin, Some(SourceFormat::Odt));
    assert_eq!(document.metadata.title.as_deref(), Some("Test"));
    assert_eq!(document.metadata.author.as_deref(), Some("Ada"));
    let Block::Heading(heading) = &document.blocks[0] else {
        panic!("{:?}", document.blocks[0])
    };
    assert_eq!(heading.level, 1);
    assert_eq!(heading.paragraph.style_id.as_deref(), Some("Heading_20_1"));
    let paragraph = document.blocks[1].paragraph().unwrap();
    assert_eq!(paragraph.style_id.as_deref(), Some("Standard"));
    assert_eq!(paragraph.props.alignment, Some(Alignment::Center));
    assert_eq!(paragraph.props.indent.left, Some(720));
    assert_eq!(paragraph.props.indent.hanging, Some(360));
    assert_eq!(paragraph.props.spacing.line, Some(LineSpacing::DOUBLE));
    assert_eq!(
        paragraph.props.preserved,
        vec!["fo:hyphenation-ladder-count=\"no-limit\"".to_string()]
    );
    // white space collapses, text:s keeps its spaces
    assert_eq!(
        paragraph.plain_text(),
        "Hello bold red and soft  text\tafter\nnext."
    );
    let Inline::Text { marks, .. } = &paragraph.content[1] else {
        panic!()
    };
    assert!(marks.contains(&Mark::Bold));
    assert!(marks.contains(&Mark::Color {
        hex: "FF0000".into()
    }));
    assert!(marks.contains(&Mark::Underline {
        style: "single".into()
    }));
    let Inline::Text { marks, .. } = &paragraph.content[3] else {
        panic!()
    };
    assert_eq!(
        marks,
        &vec![Mark::CharacterStyle {
            style_id: "Emphasis".into()
        }]
    );
    let style = document.style("Heading_20_1").unwrap();
    assert_eq!(style.name, "Heading 1");
    assert_eq!(style.based_on.as_deref(), Some("Heading"));
    let resolved = letters_office::styles::resolved_run_props(&document, Some("Heading_20_1"), &[]);
    assert_eq!(resolved.bold, Some(true));
    assert_eq!(resolved.size, Some(36));
    assert_eq!(resolved.font.unwrap().name, "Liberation Serif");
}

#[test]
fn reads_changes_notes_links_comments_and_bookmarks() {
    let document = read_odt(&sample()).unwrap();
    let paragraph = document.blocks[2].paragraph().unwrap();
    assert_eq!(paragraph.plain_text(), "The new word.");
    let deleted = &paragraph.content[1];
    assert!(matches!(deleted, Inline::Text { text, .. } if text == "old"));
    assert!(matches!(&deleted.marks()[0], Mark::Deletion(change) if change.author == "Ada"));
    assert!(
        matches!(&paragraph.content[2].marks()[0], Mark::Insertion(change) if change.author == "Ben")
    );
    let note = document.note("ftn1").unwrap();
    assert_eq!(note.blocks[0].paragraph().unwrap().plain_text(), "A note.");
    let paragraph = document.blocks[3].paragraph().unwrap();
    assert!(paragraph.content[1]
        .marks()
        .iter()
        .any(|m| matches!(m, Mark::Link(t) if t.href.as_deref() == Some("https://example.org/"))));
    assert!(paragraph.content.contains(&Inline::BookmarkStart {
        id: "mark".into(),
        name: "mark".into()
    }));
    let comment = document.comment("__Annotation__1").unwrap();
    assert_eq!(comment.author, "Prof");
    assert_eq!(
        comment.blocks[0].paragraph().unwrap().plain_text(),
        "Cite this."
    );
    let noted = paragraph
        .content
        .iter()
        .find(|i| matches!(i, Inline::Text { text, .. } if text == "noted"))
        .unwrap();
    assert_eq!(
        noted.marks(),
        &[Mark::Comment {
            id: "__Annotation__1".into()
        }]
    );
    assert!(paragraph
        .content
        .iter()
        .any(|i| matches!(i, Inline::CommentReference { .. })));
}

#[test]
fn reads_lists_tables_images_headers_and_keeps_the_unknown() {
    let document = read_odt(&sample()).unwrap();
    let labels: Vec<String> = letters_office::numbering::list_labels(&document)
        .into_iter()
        .map(|label| label.unwrap_or_else(|| "-".into()))
        .collect();
    let texts: Vec<String> = document
        .blocks
        .iter()
        .map(|b| b.paragraph().map(|p| p.plain_text()).unwrap_or_default())
        .collect();
    let find = |text: &str| texts.iter().position(|t| t == text).unwrap();
    assert_eq!(labels[find("one")], "1.");
    assert_eq!(labels[find("one a")], "a)");
    assert_eq!(labels[find("two")], "2.");
    assert_eq!(labels[find("three")], "3.");
    assert_eq!(labels[find("restart")], "1.");
    let table = document
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(table.width, Some(8640));
    assert_eq!(table.column_widths, [2880, 2880, 2880]);
    assert!(table.rows[0].header);
    assert_eq!(table.rows[0].cells[0].col_span, 2);
    assert_eq!(table.rows[0].cells[0].shading.as_deref(), Some("D9E2F3"));
    assert_eq!(
        table.rows[0].cells[0]
            .borders
            .as_ref()
            .unwrap()
            .top
            .as_ref()
            .unwrap()
            .size,
        4
    );
    assert_eq!(table.rows[0].cells[1].row_span, 2);
    assert_eq!(table.rows[1].cells.len(), 2);
    let picture = &document.blocks[find("Picture ")]
        .paragraph()
        .unwrap()
        .content;
    let Inline::Image(image) = &picture[1] else {
        panic!("{picture:?}")
    };
    assert_eq!((image.width, image.height), (1440, 720));
    assert_eq!(image.description.as_deref(), Some("A dot"));
    assert_eq!(document.media[0].bytes, png());
    // the text box in the same paragraph is a model object now, placed
    // just before the paragraph that anchors it and remembering where in
    // the paragraph it sat
    let picture_block = find("Picture ");
    let Block::TextBox(text_box) = &document.blocks[picture_block - 1] else {
        panic!("{:?}", document.blocks[picture_block - 1])
    };
    assert_eq!(
        text_box.blocks[0].paragraph().unwrap().plain_text(),
        "In a box"
    );
    let anchor = text_box.anchor.as_ref().unwrap();
    assert_eq!(
        (anchor.with, anchor.inline_index),
        (AnchorWith::Next, Some(2))
    );
    assert_eq!(picture.len(), 2);
    assert!(
        matches!(document.blocks.last().unwrap(), Block::Preserved { xml } if xml.starts_with("<text:table-of-content"))
    );
    // the header sits inside odt's top margin
    let section = &document.sections[0];
    assert_eq!(section.page.margins.header, 720);
    assert_eq!(section.page.margins.top, 720 + 288 + 432);
    let header = document
        .header_footer(section.headers.default.as_deref().unwrap())
        .unwrap();
    let text = header.blocks[0].paragraph().unwrap();
    assert_eq!(text.plain_text(), "Running head 1");
    assert_eq!(text.props.alignment, Some(Alignment::Right));
    assert!(matches!(&text.content[1], Inline::Field(f) if f.kind == FieldKind::PageNumber));
    assert!(document
        .carry
        .parts
        .iter()
        .any(|p| p.name == "settings.xml"));
}

#[test]
fn odt_round_trip_keeps_the_model_and_the_unknown_bytes() {
    let original = sample();
    let first = read_odt(&original).unwrap();
    let saved = write_odt(&first).unwrap();
    let second = read_odt(&saved).unwrap();
    assert_eq!(first.blocks, second.blocks);
    assert_eq!(first.styles, second.styles);
    assert_eq!(first.notes, second.notes);
    assert_eq!(first.comments, second.comments);
    assert_eq!(first.headers_footers, second.headers_footers);
    assert_eq!(first.sections, second.sections);
    assert_eq!(first.media, second.media);
    assert_eq!(first.metadata, second.metadata);
    // a second save changes nothing at all
    let third = read_odt(&write_odt(&second).unwrap()).unwrap();
    assert_eq!(second, third);
    // unknown parts and pieces come back exactly
    let before = parts_of(&original);
    let after = parts_of(&saved);
    for name in [
        "settings.xml",
        "Thumbnails/thumbnail.png",
        "Pictures/dot.png",
    ] {
        let a = before.iter().find(|(n, _)| n == name).map(|(_, b)| b);
        let b = after.iter().find(|(n, _)| n == name).map(|(_, b)| b);
        assert!(b.is_some() && a == b, "{name}");
    }
    let content = part_text(&saved, "content.xml");
    assert!(content.contains("<draw:text-box><text:p>In a box</text:p></draw:text-box>"));
    assert!(content.contains("<text:table-of-content text:name=\"Contents\">"));
    assert!(content
        .contains("<text:sequence-decl text:display-outline-level=\"0\" text:name=\"Figure\"/>"));
    assert!(content.contains("fo:hyphenation-ladder-count=\"no-limit\""));
    // the unchanged picture keeps its original frame xml
    assert!(content.contains("draw:style-name=\"fr1\" draw:name=\"Image1\""));
    let styles = part_text(&saved, "styles.xml");
    assert!(styles.contains("<text:notes-configuration"));
    // the package is a proper odt: mimetype first and stored
    let package = letters_office::package::Package::read(&saved).unwrap();
    assert_eq!(package.parts[0].name, "mimetype");
    // odf: the first entry is "mimetype", stored, with no extra field
    assert_eq!(&saved[0..4], b"PK\x03\x04");
    assert_eq!(
        u16::from_le_bytes([saved[8], saved[9]]),
        0,
        "mimetype must be stored"
    );
    assert_eq!(
        u16::from_le_bytes([saved[28], saved[29]]),
        0,
        "mimetype must have no extra field"
    );
    assert_eq!(&saved[30..38], b"mimetype");
    assert!(saved[38..].starts_with(b"application/vnd.oasis.opendocument.text"));
}

/// what must survive a conversion: each block's kind, heading level and
/// visible text.
fn shape(document: &Document) -> Vec<String> {
    fn block_shape(block: &Block, out: &mut Vec<String>) {
        match block {
            Block::Heading(h) => out.push(format!("h{} {}", h.level, visible_text(&h.paragraph))),
            Block::Paragraph(p) => out.push(format!("p {}", visible_text(p))),
            Block::Table(t) => {
                out.push(format!(
                    "table {}x{}",
                    t.rows.len(),
                    letters_office::tables::grid_width(t)
                ));
                for row in &t.rows {
                    for cell in &row.cells {
                        for block in &cell.blocks {
                            block_shape(block, out);
                        }
                    }
                }
            }
            Block::PageBreak => out.push("page break".into()),
            Block::TextBox(text_box) => {
                out.push("text box".into());
                for block in &text_box.blocks {
                    block_shape(block, out);
                }
            }
            Block::Shape(_) | Block::Group(_) => {
                out.push("drawing".into());
                for inner in block.drawing_blocks() {
                    block_shape(inner, out);
                }
            }
            Block::SectionBreak => out.push("section break".into()),
            // a conversion keeps the text of what it cannot convert
            Block::Preserved { xml } => out.push(format!("p {}", xml_text(xml))),
        }
    }
    let mut out = Vec::new();
    for block in &document.blocks {
        block_shape(block, &mut out);
    }
    out
}

fn xml_text(xml: &str) -> String {
    letters_office::xml::parse(xml)
        .map(|d| d.root.deep_text())
        .unwrap_or_default()
}

/// the paragraph's text including the text inside preserved xml.
fn visible_text(paragraph: &Paragraph) -> String {
    let mut text = String::new();
    for inline in &paragraph.content {
        match inline {
            Inline::Preserved { xml } | Inline::PreservedRun { xml, .. }
                if !inline.is_deleted() =>
            {
                text.push_str(&xml_text(xml))
            }
            _ => inline.push_plain_text(&mut text),
        }
    }
    text
}

fn bold_texts(document: &Document) -> Vec<String> {
    let mut texts = Vec::new();
    for block in &document.blocks {
        if let Some(paragraph) = block.paragraph() {
            for inline in &paragraph.content {
                if let Inline::Text { text, marks } = inline {
                    if marks.contains(&Mark::Bold) {
                        texts.push(text.clone());
                    }
                }
            }
        }
    }
    texts
}

#[test]
fn docx_to_odt_and_back_keeps_text_structure_and_formatting() {
    let from_word = read_docx(&rich_docx()).unwrap();
    let as_odt = write_odt(&from_word).unwrap();
    let converted = open(&as_odt).unwrap();
    // blocks word kept as preserved xml become plain paragraphs of their text
    assert_eq!(shape(&converted), shape(&from_word));
    assert_eq!(bold_texts(&converted), bold_texts(&from_word));
    let labels = |d: &Document| letters_office::numbering::list_labels(d);
    assert_eq!(labels(&converted), labels(&from_word));
    assert_eq!(converted.media[0].bytes, from_word.media[0].bytes);
    assert_eq!(
        converted.comments[0].blocks[0]
            .paragraph()
            .unwrap()
            .plain_text(),
        "Cite this."
    );
    let note_text = |d: &Document| {
        d.notes
            .iter()
            .filter(|n| n.special.is_none())
            .map(|n| n.blocks[0].paragraph().unwrap().plain_text())
            .collect::<Vec<_>>()
    };
    assert_eq!(note_text(&converted), note_text(&from_word));
    let header = |d: &Document| {
        d.header_footer(d.sections[0].headers.default.as_deref().unwrap())
            .unwrap()
            .blocks[0]
            .paragraph()
            .unwrap()
            .plain_text()
    };
    assert_eq!(header(&converted), header(&from_word));
    // tracked changes come across with their authors
    let json = serde_json::to_string(&converted.blocks).unwrap();
    assert!(json.contains("\"deletion\"") && json.contains("\"author\":\"Ada\""));
    assert!(json.contains("\"insertion\"") && json.contains("\"author\":\"Ben\""));
    assert_eq!(converted.sections.len(), from_word.sections.len());

    // and on to word again
    let back = read_docx(&write_docx(&converted).unwrap()).unwrap();
    assert_eq!(shape(&back), shape(&converted));
    assert_eq!(bold_texts(&back), bold_texts(&converted));
}

#[test]
fn odt_to_docx_keeps_text_and_structure() {
    let first = read_odt(&sample()).unwrap();
    let docx = write_docx(&first).unwrap();
    let back = read_docx(&docx).unwrap();
    assert_eq!(shape(&back), shape(&first));
    let labels = |d: &Document| letters_office::numbering::list_labels(d);
    assert_eq!(labels(&back), labels(&first));
}

#[test]
fn a_document_built_from_nothing_writes_valid_odt() {
    let mut document = Document::new();
    letters_office::outline::apply_heading(&mut document, 0, 1);
    document.blocks[0].paragraph_mut().unwrap().content = vec![Inline::text("Title")];
    document
        .blocks
        .push(Block::text("  leading spaces and  doubles"));
    let saved = write_odt(&document).unwrap();
    let again = read_odt(&saved).unwrap();
    assert!(matches!(&again.blocks[0], Block::Heading(h) if h.level == 1));
    assert_eq!(
        again.blocks[1].paragraph().unwrap().plain_text(),
        "  leading spaces and  doubles"
    );
    let styles = part_text(&saved, "styles.xml");
    assert!(styles.contains("style:display-name=\"Heading 1\""));
}
