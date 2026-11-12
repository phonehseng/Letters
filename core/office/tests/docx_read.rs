//! reading hand-built docx packages into the model, one feature each.

mod support;

use letters_office::model::*;
use letters_office::read_docx;
use support::fixtures::*;
use support::*;

fn paragraphs(document: &Document) -> Vec<&Paragraph> {
    document
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .collect()
}

#[test]
fn plain_paragraphs_and_metadata() {
    let bytes = Docx::new(&(p("Hello") + &p("World"))).build();
    let document = read_docx(&bytes).unwrap();
    let texts: Vec<String> = paragraphs(&document)
        .iter()
        .map(|p| p.plain_text())
        .collect();
    assert_eq!(texts, ["Hello", "World"]);
    assert_eq!(document.sections.len(), 1);
    assert_eq!(document.carry.origin, Some(SourceFormat::Docx));
}

#[test]
fn run_formatting_becomes_marks() {
    let body = "<w:p><w:r><w:rPr><w:b/><w:i/><w:u w:val=\"double\"/><w:strike/>\
        <w:vertAlign w:val=\"superscript\"/><w:rFonts w:ascii=\"Georgia\" w:hAnsi=\"Georgia\" w:eastAsia=\"MS Mincho\"/>\
        <w:sz w:val=\"28\"/><w:color w:val=\"ff0000\"/><w:highlight w:val=\"yellow\"/><w:smallCaps/>\
        <w:lang w:val=\"fr-FR\"/><w:b w:val=\"0\"/></w:rPr><w:t>styled</w:t></w:r>\
        <w:r><w:rPr><w:w w:val=\"150\"/></w:rPr><w:t>wide</w:t></w:r></w:p>";
    let document = read_docx(&Docx::new(body).build()).unwrap();
    let content = &paragraphs(&document)[0].content;
    let Inline::Text { marks, .. } = &content[0] else {
        panic!()
    };
    assert!(marks.contains(&Mark::Italic));
    assert!(marks.contains(&Mark::Underline {
        style: "double".into()
    }));
    assert!(marks.contains(&Mark::Superscript));
    assert!(marks.contains(&Mark::SmallCaps));
    assert!(marks.contains(&Mark::FontSize { half_points: 28 }));
    assert!(marks.contains(&Mark::Color {
        hex: "FF0000".into()
    }));
    assert!(marks.contains(&Mark::Highlight {
        color: "yellow".into()
    }));
    assert!(marks.contains(&Mark::Language {
        tag: "fr-FR".into()
    }));
    // the later w:b w:val="0" wins over the earlier w:b
    assert!(marks.contains(&Mark::Off {
        property: Toggle::Bold
    }));
    let font = marks.iter().find_map(|m| match m {
        Mark::FontFamily { name, extra, .. } => Some((name.clone(), extra.clone())),
        _ => None,
    });
    let (name, extra) = font.unwrap();
    assert_eq!(name, "Georgia");
    assert_eq!(extra, vec![Attribute::new("w:eastAsia", "MS Mincho")]);
    // character width is not modelled: kept as xml
    let Inline::Text { marks, .. } = &content[1] else {
        panic!()
    };
    assert_eq!(
        marks,
        &vec![Mark::Preserved {
            xml: "<w:w w:val=\"150\"/>".into()
        }]
    );
}

#[test]
fn paragraph_formatting() {
    let body = "<w:p><w:pPr><w:keepNext/><w:pageBreakBefore/><w:tabs><w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"9360\"/></w:tabs>\
        <w:spacing w:before=\"120\" w:after=\"240\" w:line=\"480\" w:lineRule=\"auto\"/>\
        <w:ind w:left=\"720\" w:hanging=\"360\"/><w:jc w:val=\"both\"/><w:widowControl w:val=\"0\"/></w:pPr>\
        <w:r><w:t>x</w:t></w:r></w:p>";
    let document = read_docx(&Docx::new(body).build()).unwrap();
    let props = &paragraphs(&document)[0].props;
    assert_eq!(props.keep_with_next, Some(true));
    assert_eq!(props.page_break_before, Some(true));
    assert_eq!(props.alignment, Some(Alignment::Justify));
    assert_eq!(props.indent.left, Some(720));
    assert_eq!(props.indent.hanging, Some(360));
    assert_eq!(props.spacing.before, Some(120));
    assert_eq!(props.spacing.after, Some(240));
    assert_eq!(props.spacing.line, Some(LineSpacing::DOUBLE));
    assert_eq!(
        props.tabs,
        vec![TabStop {
            position: 9360,
            kind: TabKind::Right,
            leader: TabLeader::Dot
        }]
    );
    assert_eq!(
        props.preserved,
        vec!["<w:widowControl w:val=\"0\"/>".to_string()]
    );
}

#[test]
fn headings_come_from_styles_including_inherited_ones() {
    let body = heading("Intro", "Heading1")
        + &heading("Part", "Heading2")
        + &heading("Odd", "Fancy")
        + &p("Text");
    let document = read_docx(&Docx::new(&body).styles(HEADING_STYLES).build()).unwrap();
    let levels: Vec<Option<u8>> = document
        .blocks
        .iter()
        .map(|block| match block {
            Block::Heading(heading) => Some(heading.level),
            _ => None,
        })
        .collect();
    assert_eq!(levels, [Some(1), Some(2), Some(2), None]);
    assert!(document.styles.preserved[0].starts_with("<w:latentStyles"));
}

#[test]
fn style_inheritance_resolves_through_the_chain() {
    let body = heading("Odd", "Fancy");
    let document = read_docx(&Docx::new(&body).styles(HEADING_STYLES).build()).unwrap();
    let paragraph = document.blocks[0].paragraph().unwrap();
    let props = letters_office::styles::resolved_paragraph_props(&document, paragraph);
    // keepNext from Heading1, outline level from Heading2, spacing from defaults
    assert_eq!(props.keep_with_next, Some(true));
    assert_eq!(props.outline_level, Some(1));
    assert_eq!(props.spacing.after, Some(160));
    let run = letters_office::styles::resolved_run_props(&document, Some("Fancy"), &[]);
    assert_eq!(run.bold, Some(true));
    assert_eq!(run.italic, Some(true));
    assert_eq!(run.size, Some(26));
    // a direct mark beats the styles
    let run = letters_office::styles::resolved_run_props(
        &document,
        Some("Fancy"),
        &[
            Mark::Off {
                property: Toggle::Bold,
            },
            Mark::FontSize { half_points: 40 },
        ],
    );
    assert_eq!(run.bold, Some(false));
    assert_eq!(run.size, Some(40));
}

#[test]
fn theme_fonts_are_resolved() {
    let theme = "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Office\">\
        <a:themeElements><a:fontScheme name=\"Office\"><a:majorFont><a:latin typeface=\"Calibri Light\"/></a:majorFont>\
        <a:minorFont><a:latin typeface=\"Calibri\"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>";
    let bytes = Docx::new(&p("x"))
        .related(
            "rIdTheme",
            "theme",
            "word/theme/theme1.xml",
            "application/vnd.openxmlformats-officedocument.theme+xml",
            theme,
        )
        .styles(HEADING_STYLES)
        .build();
    let document = read_docx(&bytes).unwrap();
    assert_eq!(
        document.styles.theme_fonts.minor.as_deref(),
        Some("Calibri")
    );
    let normal = document.style("Normal").unwrap();
    let font = normal.run.font.as_ref().unwrap();
    assert_eq!(font.name, "Calibri");
    assert_eq!(font.theme.as_deref(), Some("minorHAnsi"));
}

#[test]
fn numbering_definitions_and_list_membership() {
    let body =
        list_item("one", "1", 0) + &list_item("one a", "1", 1) + &list_item("bullet", "2", 0);
    let document = read_docx(&Docx::new(&body).numbering(NUMBERING).build()).unwrap();
    assert_eq!(document.numbering.definitions.len(), 2);
    assert_eq!(document.numbering.instances.len(), 3);
    let level = document.numbering.level("1", 1).unwrap();
    assert_eq!(level.format, NumberFormat::LowerLetter);
    assert_eq!(level.text, "%2)");
    assert_eq!(level.paragraph.indent.left, Some(1440));
    assert_eq!(
        document.numbering.level("2", 0).unwrap().format,
        NumberFormat::Bullet
    );
    let list = paragraphs(&document)[1].props.list.clone().unwrap();
    assert_eq!(
        list,
        ListRef {
            list_id: "1".into(),
            level: 1
        }
    );
    assert_eq!(document.numbering.instances[2].overrides[0].start, Some(1));
}

#[test]
fn list_numbers_continue_and_restart() {
    let body = list_item("a", "1", 0)
        + &list_item("b", "1", 0)
        + &list_item("b.i", "1", 1)
        + &list_item("b.ii", "1", 1)
        + &list_item("c", "1", 0)
        + &list_item("c.i", "1", 1)
        + &p("between")
        + &list_item("d", "1", 0)
        + &list_item("restart", "3", 0)
        + &list_item("next", "3", 0);
    let document = read_docx(&Docx::new(&body).numbering(NUMBERING).build()).unwrap();
    let labels: Vec<Option<String>> = letters_office::numbering::list_labels(&document);
    let labels: Vec<&str> = labels.iter().map(|l| l.as_deref().unwrap_or("-")).collect();
    assert_eq!(
        labels,
        ["1.", "2.", "a)", "b)", "3.", "a)", "-", "4.", "1.", "2."]
    );
}

#[test]
fn tables_with_column_and_row_spans() {
    let body = "<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblLook w:val=\"04A0\"/></w:tblPr>\
        <w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid>\
        <w:tr><w:trPr><w:tblHeader/></w:trPr>\
          <w:tc><w:tcPr><w:tcW w:w=\"6000\" w:type=\"dxa\"/><w:gridSpan w:val=\"2\"/><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"D9E2F3\"/></w:tcPr><w:p><w:r><w:t>wide</w:t></w:r></w:p></w:tc>\
          <w:tc><w:tcPr><w:vMerge w:val=\"restart\"/><w:vAlign w:val=\"center\"/></w:tcPr><w:p><w:r><w:t>tall</w:t></w:r></w:p></w:tc></w:tr>\
        <w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>d</w:t></w:r></w:p></w:tc>\
          <w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc></w:tr>\
        <w:tr><w:tc><w:tcPr><w:tcBorders><w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/></w:tcBorders></w:tcPr>\
          <w:tbl><w:tblGrid><w:gridCol w:w=\"1000\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>nested</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/></w:tc>\
          <w:tc><w:p/></w:tc><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>";
    let document = read_docx(&Docx::new(body).build()).unwrap();
    let Block::Table(table) = &document.blocks[0] else {
        panic!("no table")
    };
    assert_eq!(table.style_id.as_deref(), Some("TableGrid"));
    assert_eq!(table.column_widths, [3000, 3000, 3000]);
    assert_eq!(table.rows.len(), 3);
    assert!(table.rows[0].header);
    let wide = &table.rows[0].cells[0];
    assert_eq!(wide.col_span, 2);
    assert_eq!(wide.shading.as_deref(), Some("D9E2F3"));
    let tall = &table.rows[0].cells[1];
    assert_eq!(tall.row_span, 2);
    assert_eq!(tall.vertical_align, Some(VerticalAlign::Center));
    // the covered cell below "tall" is not in the model
    assert_eq!(table.rows[1].cells.len(), 2);
    let border = table.rows[2].cells[0]
        .borders
        .as_ref()
        .unwrap()
        .top
        .clone()
        .unwrap();
    assert_eq!(
        border,
        Border {
            style: "single".into(),
            size: 4,
            color: Some("000000".into())
        }
    );
    assert!(matches!(table.rows[2].cells[0].blocks[0], Block::Table(_)));
    // tblW auto and tblLook are kept as xml
    assert_eq!(table.preserved.children.len(), 2);
}

#[test]
fn footnotes_and_endnotes() {
    let notes = "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>\
        <w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>\
        <w:footnote w:id=\"1\"><w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/></w:pPr><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r>\
        <w:r><w:t xml:space=\"preserve\"> See the source.</w:t></w:r></w:p></w:footnote>";
    let body = "<w:p><w:r><w:t>Claim</w:t></w:r><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"1\"/></w:r></w:p>";
    let document = read_docx(&Docx::new(body).footnotes(notes).build()).unwrap();
    assert_eq!(document.notes.len(), 3);
    let note = document.note("fn1").unwrap();
    assert_eq!(note.kind, NoteKind::Footnote);
    assert_eq!(note.special, None);
    assert_eq!(
        note.blocks[0].paragraph().unwrap().plain_text(),
        " See the source."
    );
    assert_eq!(
        document.note("fn-1").unwrap().special.as_deref(),
        Some("separator")
    );
    let content = &paragraphs(&document)[0].content;
    assert!(matches!(&content[1], Inline::NoteReference { note_id, .. } if note_id == "fn1"));
}

#[test]
fn headers_and_footers_per_section_with_first_page() {
    let body = format!(
        "{}<w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdH1\"/>\
         <w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr></w:pPr>\
         <w:r><w:t>end of one</w:t></w:r></w:p>{}\
         <w:sectPr><w:headerReference w:type=\"first\" r:id=\"rIdH2\"/><w:footerReference w:type=\"default\" r:id=\"rIdF1\"/>\
         <w:type w:val=\"continuous\"/><w:pgSz w:w=\"15840\" w:h=\"12240\" w:orient=\"landscape\"/>\
         <w:pgMar w:top=\"720\" w:right=\"720\" w:bottom=\"720\" w:left=\"720\" w:header=\"360\" w:footer=\"360\" w:gutter=\"0\"/>\
         <w:cols w:num=\"2\" w:space=\"360\"/><w:titlePg/><w:docGrid w:linePitch=\"360\"/></w:sectPr>",
        p("one"),
        p("two")
    );
    let page = "<w:p><w:r><w:t xml:space=\"preserve\">Page </w:t></w:r><w:fldSimple w:instr=\" PAGE \"><w:r><w:t>1</w:t></w:r></w:fldSimple></w:p>";
    let bytes = Docx::new(&body)
        .header("rIdH1", "word/header1.xml", &p("Running head"))
        .header("rIdH2", "word/header2.xml", &p("First page head"))
        .footer("rIdF1", "word/footer1.xml", page)
        .build();
    let document = read_docx(&bytes).unwrap();
    assert_eq!(document.sections.len(), 2);
    assert!(matches!(document.blocks[2], Block::SectionBreak));
    let first = &document.sections[0];
    assert_eq!(first.headers.default.as_deref(), Some("header1"));
    let second = &document.sections[1];
    assert_eq!(second.start, SectionStart::Continuous);
    assert_eq!(second.page.orientation, Orientation::Landscape);
    assert_eq!(second.page.margins.left, 720);
    assert_eq!(second.columns.count, 2);
    assert!(second.different_first_page);
    assert_eq!(second.headers.first.as_deref(), Some("header2"));
    assert_eq!(
        second.preserved.children,
        vec!["<w:docGrid w:linePitch=\"360\"/>".to_string()]
    );
    let footer = document.header_footer("footer1").unwrap();
    assert_eq!(footer.kind, HeaderFooterKind::Footer);
    let content = &footer.blocks[0].paragraph().unwrap().content;
    let Inline::Field(field) = &content[1] else {
        panic!("no field")
    };
    assert_eq!(field.kind, FieldKind::PageNumber);
    assert_eq!(field.result, "1");
}

#[test]
fn images_inline_and_floating_share_one_media_entry() {
    let floating = "<w:r><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"114300\" distR=\"114300\" simplePos=\"0\" relativeHeight=\"3\" behindDoc=\"1\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\">\
        <wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"page\"><wp:posOffset>914400</wp:posOffset></wp:positionH>\
        <wp:positionV relativeFrom=\"paragraph\"><wp:align>top</wp:align></wp:positionV><wp:extent cx=\"457200\" cy=\"457200\"/>\
        <wp:wrapNone/><wp:docPr id=\"2\" name=\"Picture 2\"/><wp:cNvGraphicFramePr/>\
        <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"x.png\"/><pic:cNvPicPr/></pic:nvPicPr>\
        <pic:blipFill><a:blip r:embed=\"rIdImg\"/></pic:blipFill><pic:spPr/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>";
    let body = format!(
        "<w:p>{}{floating}</w:p>",
        inline_image("rIdImg", 914400, 457200)
    );
    let bytes = Docx::new(&body)
        .image("rIdImg", "word/media/image1.png", &png())
        .build();
    let document = read_docx(&bytes).unwrap();
    assert_eq!(document.media.len(), 1);
    assert_eq!(document.media[0].bytes, png());
    assert_eq!(document.media[0].content_type, "image/png");
    let content = &paragraphs(&document)[0].content;
    let Inline::Image(inline) = &content[0] else {
        panic!()
    };
    assert_eq!((inline.width, inline.height), (1440, 720));
    assert_eq!(inline.description.as_deref(), Some("A red square"));
    assert_eq!(inline.placement, Placement::Inline);
    let Inline::Image(floating) = &content[1] else {
        panic!()
    };
    assert_eq!(floating.media_id, inline.media_id);
    let Placement::Floating {
        horizontal,
        vertical,
        wrap,
    } = &floating.placement
    else {
        panic!()
    };
    assert_eq!(horizontal.relative_to, "page");
    assert_eq!(horizontal.offset, 1440);
    assert_eq!(vertical.align.as_deref(), Some("top"));
    assert_eq!(*wrap, Wrap::BehindText);
}

#[test]
fn tracked_changes_are_marks_with_author_and_date() {
    let body = "<w:p><w:r><w:t xml:space=\"preserve\">The </w:t></w:r>\
        <w:del w:id=\"7\" w:author=\"Ada\" w:date=\"2026-01-02T03:04:05Z\"><w:r><w:delText>old</w:delText></w:r></w:del>\
        <w:ins w:id=\"8\" w:author=\"Ben\" w:date=\"2026-01-02T03:05:00Z\"><w:r><w:t>new</w:t></w:r></w:ins>\
        <w:r><w:t xml:space=\"preserve\"> word</w:t></w:r></w:p>";
    let document = read_docx(&Docx::new(body).build()).unwrap();
    let paragraph = paragraphs(&document)[0];
    assert_eq!(paragraph.plain_text(), "The new word");
    let Inline::Text { text, marks } = &paragraph.content[1] else {
        panic!()
    };
    assert_eq!(text, "old");
    assert_eq!(
        marks,
        &vec![Mark::Deletion(Change {
            id: "7".into(),
            author: "Ada".into(),
            date: Some("2026-01-02T03:04:05Z".into())
        })]
    );
    let Inline::Text { marks, .. } = &paragraph.content[2] else {
        panic!()
    };
    assert!(matches!(&marks[0], Mark::Insertion(change) if change.author == "Ben"));
}

#[test]
fn comments_with_ranges() {
    let comments =
        "<w:comment w:id=\"0\" w:author=\"Prof\" w:date=\"2026-03-01T10:00:00Z\" w:initials=\"P\">\
        <w:p><w:r><w:annotationRef/></w:r><w:r><w:t>Cite this.</w:t></w:r></w:p></w:comment>";
    let body = "<w:p><w:r><w:t xml:space=\"preserve\">Before </w:t></w:r><w:commentRangeStart w:id=\"0\"/>\
        <w:r><w:t>noted</w:t></w:r><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r>\
        <w:r><w:t xml:space=\"preserve\"> after</w:t></w:r></w:p>";
    let document = read_docx(&Docx::new(body).comments(comments).build()).unwrap();
    let comment = document.comment("0").unwrap();
    assert_eq!(comment.author, "Prof");
    assert_eq!(comment.initials.as_deref(), Some("P"));
    assert_eq!(
        comment.blocks[0].paragraph().unwrap().plain_text(),
        "Cite this."
    );
    let content = &paragraphs(&document)[0].content;
    assert!(content[0].marks().is_empty());
    assert_eq!(content[1].marks(), &[Mark::Comment { id: "0".into() }]);
    assert!(matches!(&content[2], Inline::CommentReference { comment_id } if comment_id == "0"));
    assert!(content[3].marks().is_empty());
}

#[test]
fn links_bookmarks_and_fields() {
    let body = "<w:p><w:bookmarkStart w:id=\"0\" w:name=\"_Toc1\"/><w:hyperlink r:id=\"rIdLink\" w:history=\"1\">\
        <w:r><w:rPr><w:rStyle w:val=\"Hyperlink\"/></w:rPr><w:t>site</w:t></w:r></w:hyperlink><w:bookmarkEnd w:id=\"0\"/>\
        <w:hyperlink w:anchor=\"_Toc1\"><w:r><w:t>back</w:t></w:r></w:hyperlink>\
        <w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> NUMPAGES </w:instrText></w:r>\
        <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>9</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>\
        <w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> AUTHOR </w:instrText></w:r>\
        <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>Ada</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>";
    let document = read_docx(
        &Docx::new(body)
            .link("rIdLink", "https://example.org/")
            .build(),
    )
    .unwrap();
    let content = &paragraphs(&document)[0].content;
    assert_eq!(
        content[0],
        Inline::BookmarkStart {
            id: "0".into(),
            name: "_Toc1".into()
        }
    );
    let link = content[1].marks().iter().find_map(|m| match m {
        Mark::Link(target) => Some(target.clone()),
        _ => None,
    });
    assert_eq!(link.unwrap().href.as_deref(), Some("https://example.org/"));
    let anchor = content[3].marks().iter().find_map(|m| match m {
        Mark::Link(target) => target.anchor.clone(),
        _ => None,
    });
    assert_eq!(anchor.as_deref(), Some("_Toc1"));
    let Inline::Field(field) = &content[4] else {
        panic!("{:?}", content[4])
    };
    assert_eq!(field.kind, FieldKind::PageCount);
    assert_eq!(field.instruction, "NUMPAGES");
    assert_eq!(field.result, "9");
    // a field the model does not know keeps its runs, with the result as text
    assert!(matches!(content[5], Inline::PreservedRun { .. }));
    assert!(content
        .iter()
        .any(|i| matches!(i, Inline::Text { text, .. } if text == "Ada")));
}

#[test]
fn page_breaks_and_unknown_body_content() {
    let body = format!(
        "{}<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p><w:sdt><w:sdtPr><w:alias w:val=\"Box\"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>\
         <w:p><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara></w:p>",
        p("a"),
        p("inside")
    );
    let document = read_docx(&Docx::new(&body).build()).unwrap();
    assert!(matches!(document.blocks[1], Block::PageBreak));
    let Block::Preserved { xml } = &document.blocks[2] else {
        panic!()
    };
    assert!(xml.starts_with("<w:sdt>") && xml.ends_with("</w:sdt>"));
    let content = &document.blocks[3].paragraph().unwrap().content;
    assert!(matches!(&content[0], Inline::Preserved { xml } if xml.starts_with("<m:oMathPara>")));
}

#[test]
fn core_properties() {
    let core = "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
        xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
        <dc:title>Essay</dc:title><dc:creator>Ada Lovelace</dc:creator><cp:revision>4</cp:revision>\
        <dcterms:created xsi:type=\"dcterms:W3CDTF\">2026-01-01T00:00:00Z</dcterms:created></cp:coreProperties>";
    let mut builder = Docx::new(&p("x"));
    builder = builder.loose_part(
        "docProps/core.xml",
        "application/vnd.openxmlformats-package.core-properties+xml",
        core.as_bytes(),
    );
    let bytes = builder.build();
    // the test package's root rels do not list core.xml; the usual path is tried
    let document = read_docx(&bytes).unwrap();
    assert_eq!(document.metadata.title.as_deref(), Some("Essay"));
    assert_eq!(document.metadata.author.as_deref(), Some("Ada Lovelace"));
    assert_eq!(
        document.metadata.created.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );
    assert_eq!(
        document.carry.docx.as_ref().unwrap().core_extras,
        vec!["<cp:revision>4</cp:revision>".to_string()]
    );
}

#[test]
fn the_model_round_trips_through_json() {
    let body = heading("Intro", "Heading1") + &list_item("one", "1", 0);
    let document = read_docx(
        &Docx::new(&body)
            .styles(HEADING_STYLES)
            .numbering(NUMBERING)
            .image("rIdImg", "word/media/image1.png", &png())
            .build(),
    )
    .unwrap();
    let json = serde_json::to_string(&document).unwrap();
    assert!(json.contains("\"type\":\"heading\""));
    assert!(json.contains("\"styleId\":\"Heading1\""));
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back, document);
}
