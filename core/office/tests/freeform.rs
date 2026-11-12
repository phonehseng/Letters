//! round trips of the freeform objects through both formats: shapes,
//! groups, connectors, floating tables, linked boxes, unequal columns,
//! continuous column sections and canvas pages. each is written, read
//! back and compared, in docx, in odt and across the two.

mod support;

use letters_office::freeform::{append_canvas_page, canvas_page_from_objects, CanvasObject};
use letters_office::model::*;
use letters_office::{read_docx, read_odt, write_docx, write_odt};
use support::png;

fn picture_item() -> MediaItem {
    MediaItem {
        id: "dot.png".into(),
        content_type: "image/png".into(),
        source_path: None,
        bytes: png(),
    }
}

fn gradient() -> Fill {
    Fill::Gradient(Gradient {
        kind: GradientKind::Linear,
        angle: 90.0,
        stops: vec![
            GradientStop {
                position: 0.0,
                color: "FFC800".into(),
                opacity: None,
            },
            GradientStop {
                position: 1.0,
                color: "C82828".into(),
                opacity: None,
            },
        ],
    })
}

fn arrow_line() -> Shape {
    let mut line = Shape::preset("line", 1440, 7200, 2880, 720, "000000");
    line.fill = Fill::None;
    line.flip_vertical = true;
    let mut stroke = Stroke::solid("C80000", 30);
    stroke.dash = "dash".into();
    stroke.tail = Some(LineEnd {
        kind: "triangle".into(),
        width: None,
        length: None,
    });
    stroke.head = Some(LineEnd {
        kind: "oval".into(),
        width: None,
        length: None,
    });
    line.stroke = Some(stroke);
    line.name = Some("Arrow".into());
    line
}

fn free_path() -> Shape {
    let mut shape = Shape::preset("rect", 5000, 7000, 1440, 1440, "5AC8A0");
    shape.geometry = Geometry::Path {
        paths: vec![ShapePath {
            width: 100.0,
            height: 100.0,
            fill: true,
            stroke: true,
            commands: vec![
                PathCommand::Move { x: 0.0, y: 20.0 },
                PathCommand::Line { x: 60.0, y: 0.0 },
                PathCommand::Cubic {
                    x1: 90.0,
                    y1: 20.0,
                    x2: 100.0,
                    y2: 80.0,
                    x: 60.0,
                    y: 100.0,
                },
                PathCommand::Close,
            ],
        }],
    };
    shape.name = Some("Path".into());
    shape
}

fn shapes() -> Vec<Shape> {
    let mut rounded = Shape::preset("roundRect", 1440, 1440, 2880, 1440, "3366CC");
    rounded.geometry = Geometry::Preset {
        name: "roundRect".into(),
        adjust: vec![Adjust {
            name: "adj".into(),
            value: 25000,
        }],
    };
    rounded.text = Some(ShapeText::new(vec![Block::text("Inside a shape")]));
    rounded.name = Some("Rounded".into());
    let mut turned = Shape::preset("triangle", 5040, 1440, 1440, 1440, "783CA0");
    turned.rotation = Some(30.0);
    turned.fill = Fill::Solid {
        color: "783CA0".into(),
        opacity: Some(0.5),
    };
    turned.name = Some("Turned".into());
    let mut fading = Shape::preset("ellipse", 7200, 1440, 1440, 1440, "FFFFFF");
    fading.fill = gradient();
    fading.name = Some("Fading".into());
    let mut pictured = Shape::preset("rect", 1440, 4320, 1440, 1080, "FFFFFF");
    pictured.fill = Fill::Picture {
        media_id: "dot.png".into(),
        tile: false,
    };
    pictured.name = Some("Pictured".into());
    let mut mirrored = Shape::preset("rightArrow", 4320, 4320, 2160, 720, "E6781E");
    mirrored.flip_horizontal = true;
    mirrored.name = Some("Mirrored".into());
    vec![
        rounded,
        turned,
        fading,
        pictured,
        mirrored,
        arrow_line(),
        free_path(),
    ]
}

fn group() -> Group {
    let mut inner_box = TextBox::new(0, 0, 0, 0, vec![Block::text("Text inside a group")]);
    inner_box.horizontal = FloatPosition {
        relative_to: "group".into(),
        offset: 0,
        align: None,
        offset_emu: Some(0),
    };
    inner_box.vertical = FloatPosition {
        relative_to: "group".into(),
        offset: 2000,
        align: None,
        offset_emu: Some(1_270_000),
    };
    inner_box.width = 4400;
    inner_box.height = 1000;
    inner_box.width_emu = Some(2_794_000);
    inner_box.height_emu = Some(635_000);
    let member = |name: &str, preset: &str, x: i64, fill: &str| {
        let mut shape = Shape::preset(preset, 0, 0, 0, 0, fill);
        shape.name = Some(name.into());
        shape.horizontal = FloatPosition {
            relative_to: "group".into(),
            offset: (x / 635) as i32,
            align: None,
            offset_emu: Some(x),
        };
        shape.vertical = FloatPosition {
            relative_to: "group".into(),
            offset: 0,
            align: None,
            offset_emu: Some(0),
        };
        shape.width = 1600;
        shape.height = 1200;
        shape.width_emu = Some(1_016_000);
        shape.height_emu = Some(762_000);
        GroupChild::Shape(shape)
    };
    Group {
        name: Some("Outer Group".into()),
        description: Some("two shapes and a box".into()),
        title: None,
        horizontal: FloatPosition::new("page", 2000),
        vertical: FloatPosition::new("page", 9000),
        width: 4400,
        height: 3000,
        width_emu: Some(2_794_000),
        height_emu: Some(1_905_000),
        wrap: Wrap::Square,
        z_order: Some(20),
        rotation: None,
        flip_horizontal: false,
        flip_vertical: false,
        child_offset_x: 0,
        child_offset_y: 0,
        child_width: 2_794_000,
        child_height: 1_905_000,
        children: vec![
            member("Left", "rect", 0, "3C78C8"),
            member("Right", "ellipse", 1_778_000, "DC5050"),
            GroupChild::TextBox(inner_box),
        ],
        preserved: DrawingPreserved::default(),
        anchor: None,
        original: None,
    }
}

fn floating_table() -> Table {
    let mut merged = TableCell::with_text("Merged down");
    merged.row_span = 2;
    let mut upward = TableCell::with_text("Upward");
    upward.text_direction = Some(TextDirection::Upward);
    upward.margins = Some(CellMargins {
        top: Some(160),
        ..Default::default()
    });
    let mut shaded = TableCell::with_text("Middle");
    shaded.shading = Some("DCEBFF".into());
    shaded.vertical_align = Some(VerticalAlign::Center);
    let row = |cells: Vec<TableCell>| TableRow {
        height: Some(600),
        height_rule: Some(HeightRule::Exact),
        cells,
        ..Default::default()
    };
    Table {
        column_widths: vec![1800, 2400, 1600],
        width: Some(5800),
        fixed_layout: Some(true),
        cell_margins: Some(CellMargins {
            top: Some(60),
            left: Some(120),
            bottom: None,
            right: Some(120),
        }),
        rows: vec![
            row(vec![
                TableCell::with_text("Exact rows"),
                TableCell::with_text("Top"),
                merged,
            ]),
            row(vec![upward, shaded]),
        ],
        position: Some(TablePosition {
            horizontal: FloatPosition::new("page", 5200),
            vertical: FloatPosition::new("page", 6000),
            distance: Distances {
                left: 180,
                top: 0,
                right: 180,
                bottom: 0,
            },
            overlap: false,
            anchor: None,
        }),
        ..Default::default()
    }
}

fn linked_boxes() -> Vec<TextBox> {
    (0..3)
        .map(|index| {
            let story = if index == 0 {
                (1..=6)
                    .map(|n| Block::text(&format!("Paragraph {n} of one story.")))
                    .collect()
            } else {
                Vec::new()
            };
            let mut text_box = TextBox::new(1440 + 3300 * index, 2880, 3000, 3200, story);
            text_box.name = Some(format!("Story Box {}", index + 1));
            text_box.chain = Some(TextChain {
                id: "story".into(),
                sequence: index as u32,
            });
            text_box
        })
        .collect()
}

/// a document holding one of everything, anchored in a paragraph of text.
fn freeform_document() -> Document {
    let mut document = Document::new();
    document.media.push(picture_item());
    let mut blocks = Vec::new();
    for shape in shapes() {
        blocks.push(Block::Shape(shape));
    }
    blocks.push(Block::Group(group()));
    for text_box in linked_boxes() {
        blocks.push(Block::TextBox(text_box));
    }
    blocks.push(Block::text("Everything above is anchored here."));
    blocks.push(Block::Table(floating_table()));
    blocks.push(Block::text("Text after the table."));
    document.blocks = blocks;
    document
}

fn drawings(document: &Document) -> Vec<&Block> {
    document
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Shape(_) | Block::Group(_) | Block::TextBox(_)))
        .collect()
}

fn shape_named<'a>(document: &'a Document, name: &str) -> &'a Shape {
    document
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Shape(s) if s.name.as_deref() == Some(name) => Some(s),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no shape {name} in {:#?}", drawings(document)))
}

/// what must survive any round trip of the freeform document.
fn check_freeform(back: &Document, format: &str) {
    let rounded = shape_named(back, "Rounded");
    assert_eq!(rounded.preset_name(), Some("roundRect"), "{format}");
    assert_eq!(
        rounded.text.as_ref().unwrap().blocks[0]
            .paragraph()
            .unwrap()
            .plain_text(),
        "Inside a shape"
    );
    assert!((rounded.horizontal.offset - 1440).abs() <= 1, "{format}");
    let turned = shape_named(back, "Turned");
    assert!(
        (turned.rotation.unwrap() - 30.0).abs() < 0.01,
        "{format}: {:?}",
        turned.rotation
    );
    assert!(
        (turned.horizontal.offset - 5040).abs() <= 1 && (turned.vertical.offset - 1440).abs() <= 1,
        "{format}: {:?}",
        (&turned.horizontal, &turned.vertical)
    );
    assert!(
        matches!(turned.fill, Fill::Solid { opacity: Some(o), .. } if (o - 0.5).abs() < 0.01),
        "{format}"
    );
    assert_eq!(shape_named(back, "Fading").fill, gradient(), "{format}");
    assert!(
        matches!(&shape_named(back, "Pictured").fill, Fill::Picture { .. }),
        "{format}"
    );
    assert!(shape_named(back, "Mirrored").flip_horizontal, "{format}");
    let arrow = shape_named(back, "Arrow");
    assert!(arrow.is_line() && arrow.flip_vertical, "{format}");
    let stroke = arrow.stroke.as_ref().unwrap();
    assert_eq!(
        (stroke.dash.as_str(), stroke.width),
        ("dash", 30),
        "{format}"
    );
    assert_eq!(stroke.tail.as_ref().unwrap().kind, "triangle", "{format}");
    assert_eq!(stroke.head.as_ref().unwrap().kind, "oval", "{format}");
    let Geometry::Path { paths } = &shape_named(back, "Path").geometry else {
        panic!("{format}: not a path")
    };
    assert_eq!(paths[0].commands.len(), 4, "{format}");
    let group = back
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Group(g) => Some(g),
            _ => None,
        })
        .unwrap();
    assert_eq!(group.children.len(), 3, "{format}");
    assert!(
        (group.horizontal.offset - 2000).abs() <= 1 && (group.width - 4400).abs() <= 1,
        "{format}"
    );
    let chained: Vec<&TextBox> = back
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::TextBox(t) if t.chain.is_some() => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(chained.len(), 3, "{format}");
    assert_eq!(
        chained[0].blocks.len(),
        6,
        "{format}: the story stays in the first box"
    );
    assert!(chained[1..].iter().all(|t| t.blocks.is_empty()), "{format}");
    let table = back
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let position = table
        .position
        .as_ref()
        .unwrap_or_else(|| panic!("{format}: the table no longer floats"));
    assert!((position.horizontal.offset - 5200).abs() <= 1, "{format}");
    assert_eq!(
        table.rows[0].height_rule,
        Some(HeightRule::Exact),
        "{format}"
    );
    assert_eq!(
        table.rows[1].cells[0].text_direction,
        Some(TextDirection::Upward),
        "{format}"
    );
    assert_eq!(table.rows[0].cells[2].row_span, 2, "{format}");
    // every drawing is anchored before the paragraph of text, as written
    let anchor_text = back
        .blocks
        .iter()
        .position(|b| {
            b.paragraph()
                .is_some_and(|p| p.plain_text().starts_with("Everything above"))
        })
        .unwrap();
    assert!(
        back.blocks[..anchor_text].iter().all(|b| b.is_positioned()),
        "{format}"
    );
}

#[test]
fn every_freeform_object_survives_docx() {
    let back = read_docx(&write_docx(&freeform_document()).unwrap()).unwrap();
    check_freeform(&back, "docx");
    assert!(!back.sections[0].canvas, "text flows on this page");
}

#[test]
fn every_freeform_object_survives_odt() {
    let back = read_odt(&write_odt(&freeform_document()).unwrap()).unwrap();
    check_freeform(&back, "odt");
}

#[test]
fn every_freeform_object_survives_docx_to_odt_to_docx() {
    let first = read_docx(&write_docx(&freeform_document()).unwrap()).unwrap();
    let as_odt = read_odt(&write_odt(&first).unwrap()).unwrap();
    check_freeform(&as_odt, "docx then odt");
    let back = read_docx(&write_docx(&as_odt).unwrap()).unwrap();
    check_freeform(&back, "docx, odt, docx");
}

#[test]
fn a_second_save_of_a_read_document_is_byte_identical() {
    for (name, first) in [
        ("docx", write_docx(&freeform_document()).unwrap()),
        ("odt", write_odt(&freeform_document()).unwrap()),
    ] {
        let read = |bytes: &[u8]| {
            if name == "docx" {
                read_docx(bytes).unwrap()
            } else {
                read_odt(bytes).unwrap()
            }
        };
        let write = |document: &Document| {
            if name == "docx" {
                write_docx(document).unwrap()
            } else {
                write_odt(document).unwrap()
            }
        };
        let second = write(&read(&first));
        let third = write(&read(&second));
        let main = if name == "docx" {
            "word/document.xml"
        } else {
            "content.xml"
        };
        assert_eq!(
            support::part_text(&second, main),
            support::part_text(&third, main),
            "{name}"
        );
    }
}

#[test]
fn unequal_columns_and_column_sections_survive_both_formats() {
    let mut document = Document::new();
    document.blocks = vec![
        Block::text("One column first."),
        Block::SectionBreak,
        Block::text("Three columns."),
        Block::Paragraph(Paragraph {
            content: vec![
                Inline::Break {
                    kind: BreakKind::Column,
                    marks: vec![],
                },
                Inline::text("Next column."),
            ],
            ..Default::default()
        }),
        Block::SectionBreak,
        Block::text("One column again."),
    ];
    let columns = Section {
        start: SectionStart::Continuous,
        column_widths: vec![
            ColumnWidth {
                width: 2000,
                space: 360,
            },
            ColumnWidth {
                width: 3600,
                space: 240,
            },
            ColumnWidth {
                width: 3160,
                space: 0,
            },
        ],
        columns: Columns {
            count: 3,
            space: 720,
            separator: true,
        },
        ..Section::default()
    };
    let after = Section {
        start: SectionStart::Continuous,
        ..Section::default()
    };
    document.sections = vec![Section::default(), columns.clone(), after];
    for (format, back) in [
        ("docx", read_docx(&write_docx(&document).unwrap()).unwrap()),
        ("odt", read_odt(&write_odt(&document).unwrap()).unwrap()),
    ] {
        assert_eq!(back.sections.len(), 3, "{format}: {:#?}", back.sections);
        assert_eq!(back.sections[1].start, SectionStart::Continuous, "{format}");
        assert!(back.sections[1].columns.separator, "{format}");
        assert_eq!(
            back.sections[1].column_widths, columns.column_widths,
            "{format}"
        );
        assert_eq!(back.sections[2].start, SectionStart::Continuous, "{format}");
        assert_eq!(back.sections[2].column_layout().len(), 1, "{format}");
    }
}

#[test]
fn a_canvas_page_reads_back_as_a_canvas_page() {
    let mut document = Document::new();
    document.media.push(picture_item());
    let mut picture = Image {
        media_id: "dot.png".into(),
        width: 3000,
        height: 2000,
        placement: Placement::Floating {
            horizontal: FloatPosition::new("page", 1000),
            vertical: FloatPosition::new("page", 4000),
            wrap: Wrap::InFrontOfText,
        },
        ..Default::default()
    };
    picture.name = Some("Photo".into());
    let mut banner = Shape::preset("rect", 0, 0, 12240, 3200, "1E3C6E");
    banner.stroke = None;
    banner.wrap = Wrap::BehindText;
    let mut title = TextBox::new(
        1080,
        800,
        10080,
        1600,
        vec![Block::text("Spring Science Fair")],
    );
    title.border = None;
    title.fill = None;
    let page = canvas_page_from_objects(
        PageSetup::default(),
        vec![
            CanvasObject::Shape(banner),
            CanvasObject::TextBox(title),
            CanvasObject::Picture(picture),
            CanvasObject::Table(floating_table()),
        ],
    );
    append_canvas_page(&mut document, page);
    for (format, back) in [
        ("docx", read_docx(&write_docx(&document).unwrap()).unwrap()),
        ("odt", read_odt(&write_odt(&document).unwrap()).unwrap()),
    ] {
        assert!(back.sections[0].canvas, "{format}: {:#?}", back.blocks);
        let flowing: Vec<&Block> = back.blocks.iter().filter(|b| !b.is_positioned()).collect();
        assert_eq!(
            flowing.len(),
            1,
            "{format}: one anchor paragraph only, {flowing:#?}"
        );
        let pictures = flowing[0].paragraph().unwrap().content.len();
        assert_eq!(
            pictures, 1,
            "{format}: the picture stays in the anchor paragraph"
        );
    }
}

/// every drawing's original xml, by name, from a document just read.
fn originals(document: &Document) -> Vec<(String, String)> {
    document
        .blocks
        .iter()
        .filter_map(|block| {
            let (name, original) = match block {
                Block::Shape(s) => (s.name.clone(), s.original.as_ref()),
                Block::Group(g) => (g.name.clone(), g.original.as_ref()),
                Block::TextBox(t) => (t.name.clone(), t.original.as_ref()),
                _ => return None,
            };
            Some((name.unwrap_or_default(), original?.xml.clone()))
        })
        .collect()
}

#[test]
fn editing_one_object_leaves_every_other_object_byte_identical() {
    for format in ["docx", "odt"] {
        let (bytes, main) = match format {
            "docx" => (
                write_docx(&freeform_document()).unwrap(),
                "word/document.xml",
            ),
            _ => (write_odt(&freeform_document()).unwrap(), "content.xml"),
        };
        let read = |bytes: &[u8]| {
            if format == "docx" {
                read_docx(bytes).unwrap()
            } else {
                read_odt(bytes).unwrap()
            }
        };
        let write = |document: &Document| {
            if format == "docx" {
                write_docx(document).unwrap()
            } else {
                write_odt(document).unwrap()
            }
        };
        let mut document = read(&bytes);
        let kept = originals(&document);
        assert!(kept.len() >= 10, "{format}: {} originals", kept.len());
        // move one shape by one twip, as a nudge with the arrow key would
        for block in &mut document.blocks {
            if let Block::Shape(shape) = block {
                if shape.name.as_deref() == Some("Turned") {
                    shape.horizontal.offset += 1;
                    shape.horizontal.offset_emu = None;
                }
            }
        }
        let edited = support::part_text(&write(&document), main);
        for (name, xml) in &kept {
            let present = edited.contains(xml.as_str());
            assert_eq!(present, name != "Turned", "{format}: {name}");
        }
    }
}
#[test]
fn unknown_table_and_row_properties_stay_where_word_puts_them() {
    let body =
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblLook w:val=\"0000\"/></w:tblPr>\
        <w:tblGrid><w:gridCol w:w=\"1080\"/></w:tblGrid>\
        <w:tr><w:tblPrEx><w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/></w:tblCellMar></w:tblPrEx>\
        <w:trPr><w:trHeight w:hRule=\"exact\" w:val=\"600\"/></w:trPr>\
        <w:tc><w:p><w:r><w:t>in</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>";
    let docx = support::Docx::new(body).build();
    let saved = write_docx(&read_docx(&docx).unwrap()).unwrap();
    let xml = support::part_text(&saved, "word/document.xml");
    let properties = &xml[xml.find("<w:tblPr>").unwrap()..xml.find("</w:tblPr>").unwrap()];
    assert!(
        properties.contains("<w:tblLook") && properties.contains("<w:tblW"),
        "{xml}"
    );
    let row = &xml[xml.find("<w:tr>").unwrap()..];
    assert!(row.starts_with("<w:tr><w:tblPrEx>"), "{row}");
    assert!(
        row.contains("<w:trPr><w:trHeight w:hRule=\"exact\" w:val=\"600\"/></w:trPr>"),
        "{row}"
    );
}
/// the prefixes a part uses that its root does not declare.
fn undeclared_prefixes(xml: &str) -> Vec<String> {
    let root_end = xml.find("?>").map(|at| at + 2).unwrap_or(0);
    let root = &xml[root_end
        ..xml[root_end..]
            .find('>')
            .map(|at| root_end + at)
            .unwrap_or(xml.len())];
    let mut missing = std::collections::BTreeSet::new();
    for piece in xml.split('<').skip(1) {
        let name = piece
            .trim_start_matches('/')
            .split([' ', '>', '/'])
            .next()
            .unwrap_or("");
        let attributes = piece
            .split('>')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .skip(1);
        let names = std::iter::once(name).chain(attributes.filter_map(|a| a.split('=').next()));
        for qualified in names {
            if let Some((prefix, _)) = qualified.split_once(':') {
                let declared = prefix == "xml"
                    || prefix == "xmlns"
                    || root.contains(&format!("xmlns:{prefix}="))
                    || piece.contains(&format!("xmlns:{prefix}="));
                if !declared
                    && !prefix.is_empty()
                    && prefix.chars().all(|c| c.is_ascii_alphanumeric())
                {
                    missing.insert(prefix.to_string());
                }
            }
        }
    }
    missing.into_iter().collect()
}

#[test]
fn every_prefix_a_written_part_uses_is_declared() {
    let document = freeform_document();
    let docx = write_docx(&document).unwrap();
    let odt = write_odt(&document).unwrap();
    for (bytes, part) in [
        (&docx, "word/document.xml"),
        (&odt, "content.xml"),
        (&odt, "styles.xml"),
    ] {
        let xml = support::part_text(bytes, part);
        assert_eq!(undeclared_prefixes(&xml), Vec::<String>::new(), "{part}");
    }
    // converted documents carry no root of their own, so this matters most
    let converted = write_docx(&read_odt(&odt).unwrap()).unwrap();
    let xml = support::part_text(&converted, "word/document.xml");
    assert_eq!(undeclared_prefixes(&xml), Vec::<String>::new());
}
#[test]
fn drawings_waiting_for_their_paragraph_are_not_taken_by_a_shapes_text() {
    let next = || {
        Some(ObjectAnchor {
            with: AnchorWith::Next,
            inline_index: Some(0),
            ..Default::default()
        })
    };
    let mut plain = Shape::preset("rect", 1440, 1440, 1440, 720, "3366CC");
    plain.anchor = next();
    plain.name = Some("Plain".into());
    let mut with_text = Shape::preset("roundRect", 4320, 1440, 2880, 720, "FFFFCC");
    with_text.text = Some(ShapeText::new(vec![Block::text("Words")]));
    with_text.anchor = next();
    with_text.name = Some("With text".into());
    let mut document = Document::new();
    document.blocks = vec![
        Block::Shape(plain),
        Block::Shape(with_text),
        Block::text("The anchor."),
    ];
    for (format, back) in [
        ("odt", read_odt(&write_odt(&document).unwrap()).unwrap()),
        ("docx", read_docx(&write_docx(&document).unwrap()).unwrap()),
    ] {
        let shapes = back
            .blocks
            .iter()
            .filter(|b| matches!(b, Block::Shape(_)))
            .count();
        assert_eq!(shapes, 2, "{format}: {:#?}", back.blocks);
        let texts: Vec<String> = back
            .blocks
            .iter()
            .filter_map(Block::paragraph)
            .map(Paragraph::plain_text)
            .collect();
        assert_eq!(texts, vec!["The anchor.".to_string()], "{format}");
    }
}
