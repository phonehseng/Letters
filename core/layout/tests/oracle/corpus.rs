//! the graded corpus: small word-processor documents, each aimed at one
//! layout feature, written with letters-office's own docx writer.
//!
//! every document names its fonts, sizes and spacing explicitly, so the
//! oracles cannot fill gaps from their own templates differently. the text
//! comes from a seeded generator, so a document is the same on every run.

use letters_office::model::*;

/// one corpus document and the class it is scored in.
pub struct CorpusDocument {
    pub name: String,
    pub class: &'static str,
    pub document: Document,
}

/// a tiny deterministic random source (xorshift), so the corpus text and
/// the property tests never change between runs.
pub struct Seeded(pub u64);

impl Seeded {
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n.max(1)
    }
}

const WORDS: &[&str] = &[
    "the",
    "of",
    "and",
    "a",
    "to",
    "in",
    "is",
    "that",
    "for",
    "it",
    "as",
    "was",
    "with",
    "be",
    "by",
    "on",
    "not",
    "this",
    "are",
    "or",
    "which",
    "from",
    "an",
    "at",
    "but",
    "their",
    "study",
    "students",
    "learning",
    "evidence",
    "research",
    "between",
    "however",
    "results",
    "experiment",
    "measured",
    "carefully",
    "although",
    "important",
    "describes",
    "chapter",
    "analysis",
    "university",
    "particular",
    "understanding",
    "significant",
    "relationship",
    "environment",
    "development",
    "information",
    "approximately",
    "communication",
    "interpretation",
    "responsibility",
    "characteristics",
    "nevertheless",
    "laboratory",
    "temperature",
    "observation",
    "hypothesis",
    "conclusion",
    "literature",
    "historical",
    "economic",
    "policy",
    "theory",
    "method",
    "sample",
    "data",
    "value",
    "model",
    "time",
    "people",
    "water",
    "small",
    "large",
    "early",
    "first",
    "second",
    "third",
    "several",
    "different",
    "always",
    "never",
    "often",
    "rarely",
    "through",
    "during",
    "within",
    "without",
    "against",
    "towards",
    "argument",
    "essay",
    "paragraph",
    "author's",
    "readers",
    "well-known",
    "twenty-one",
    "1984",
    "3.5",
    "e.g.",
    "(see",
    "below)",
    "\"quoted\"",
    "x",
];

/// a sentence of `words` words, capitalised and ended with a full stop.
pub fn sentence(rng: &mut Seeded, words: usize) -> String {
    let mut out = String::new();
    for i in 0..words {
        let word = WORDS[rng.below(WORDS.len() as u64) as usize];
        if i == 0 {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
        } else {
            out.push(' ');
            out.push_str(word);
            if rng.below(11) == 0 && i + 1 < words {
                out.push(',');
            }
        }
    }
    out.push('.');
    out
}

/// a paragraph of about `words` words in sentences of 6 to 22 words.
pub fn prose(rng: &mut Seeded, words: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    while count < words {
        let length = 6 + rng.below(17) as usize;
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&sentence(rng, length));
        count += length;
    }
    out
}

fn font(name: &str) -> FontRef {
    FontRef {
        name: name.into(),
        theme: None,
        extra: Vec::new(),
    }
}

/// a document whose defaults and normal style say everything: font,
/// size, spacing and line spacing.
pub fn base_document(family: &str, half_points: u32, after: Twips, line: LineSpacing) -> Document {
    let mut document = Document::new();
    document.styles.defaults.run.font = Some(font(family));
    document.styles.defaults.run.size = Some(half_points);
    document.styles.defaults.paragraph.spacing = Spacing {
        before: Some(0),
        after: Some(after),
        line: Some(line),
    };
    document.blocks.clear();
    document.sections = vec![Section::default()];
    document
}

pub fn paragraph(text: &str) -> Paragraph {
    Paragraph::with_text(text)
}

pub fn styled_paragraph(text: &str, marks: Vec<Mark>) -> Paragraph {
    Paragraph {
        content: vec![Inline::styled(text, marks)],
        ..Default::default()
    }
}

fn size_mark(points_x2: u32) -> Mark {
    Mark::FontSize {
        half_points: points_x2,
    }
}

fn family_mark(name: &str) -> Mark {
    Mark::FontFamily {
        name: name.into(),
        theme: None,
        extra: Vec::new(),
    }
}

fn auto(value: i32) -> LineSpacing {
    LineSpacing {
        rule: LineRule::Auto,
        value,
    }
}

fn exact(value: i32) -> LineSpacing {
    LineSpacing {
        rule: LineRule::Exact,
        value,
    }
}

fn at_least(value: i32) -> LineSpacing {
    LineSpacing {
        rule: LineRule::AtLeast,
        value,
    }
}

/// the whole corpus, in the order the report lists it.
pub fn corpus() -> Vec<CorpusDocument> {
    let mut all = Vec::new();
    for family in [
        "Calibri",
        "Cambria",
        "Times New Roman",
        "Arial",
        "Aptos",
        "Georgia",
        "Courier New",
    ] {
        all.push(font_probe(family));
    }
    all.push(line_spacing_modes("Calibri", 22));
    all.push(line_spacing_modes("Times New Roman", 24));
    all.push(alignments());
    all.push(justified());
    all.push(indents_and_tabs());
    all.push(lists());
    all.push(headings());
    all.push(spacing_rules());
    all.push(mixed_runs());
    all.push(widows_and_keeps());
    all.push(mla_essay());
    all.push(tables_fixed());
    all.push(tables_auto());
    all.push(tables_long());
    all.push(images_inline());
    all.push(images_floating());
    all.push(footnotes());
    all.push(headers_footers());
    all.push(two_columns());
    all.push(section_change());
    all
}

/// one font at four sizes, each in its own paragraph of several lines,
/// single spaced with no paragraph spacing: line pitch and the first
/// baseline give the font's line height as each oracle computes it.
fn font_probe(family: &str) -> CorpusDocument {
    let mut document = base_document(family, 22, 0, LineSpacing::SINGLE);
    let mut rng = Seeded(7 + family.len() as u64);
    for size in [18u32, 22, 24, 32] {
        let mut p = styled_paragraph(&prose(&mut rng, 60), vec![size_mark(size)]);
        p.props.spacing.after = Some(240);
        document.blocks.push(Block::Paragraph(p));
    }
    CorpusDocument {
        name: format!("font_{}", family.to_lowercase().replace(' ', "_")),
        class: "fonts",
        document,
    }
}

fn line_spacing_modes(family: &str, half_points: u32) -> CorpusDocument {
    let mut document = base_document(family, half_points, 0, LineSpacing::SINGLE);
    let mut rng = Seeded(11 + half_points as u64);
    let modes = [
        auto(240),
        auto(259),
        auto(276),
        auto(278),
        auto(360),
        auto(480),
        exact(300),
        exact(200),
        at_least(300),
        at_least(100),
    ];
    for mode in modes {
        let mut p = paragraph(&prose(&mut rng, 40));
        p.props.spacing.line = Some(mode);
        p.props.spacing.after = Some(200);
        document.blocks.push(Block::Paragraph(p));
    }
    CorpusDocument {
        name: format!("spacing_modes_{}", family.to_lowercase().replace(' ', "_")),
        class: "line spacing",
        document,
    }
}

fn alignments() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(21);
    for alignment in [
        Alignment::Left,
        Alignment::Center,
        Alignment::Right,
        Alignment::Justify,
    ] {
        for _ in 0..2 {
            let mut p = paragraph(&prose(&mut rng, 70));
            p.props.alignment = Some(alignment);
            document.blocks.push(Block::Paragraph(p));
        }
    }
    CorpusDocument {
        name: "alignments".into(),
        class: "paragraphs",
        document,
    }
}

fn justified() -> CorpusDocument {
    let mut document = base_document("Times New Roman", 24, 0, LineSpacing::SINGLE);
    let mut rng = Seeded(23);
    for _ in 0..8 {
        let mut p = paragraph(&prose(&mut rng, 160));
        p.props.alignment = Some(Alignment::Justify);
        p.props.indent.first_line = Some(720);
        p.props.spacing.after = Some(120);
        document.blocks.push(Block::Paragraph(p));
    }
    CorpusDocument {
        name: "justified".into(),
        class: "paragraphs",
        document,
    }
}

fn indents_and_tabs() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(31);
    let mut first = paragraph(&prose(&mut rng, 60));
    first.props.indent.first_line = Some(720);
    let mut hanging = paragraph(&prose(&mut rng, 60));
    hanging.props.indent.left = Some(720);
    hanging.props.indent.hanging = Some(720);
    let mut both = paragraph(&prose(&mut rng, 60));
    both.props.indent.left = Some(1440);
    both.props.indent.right = Some(1440);
    document
        .blocks
        .extend([first, hanging, both].map(Block::Paragraph));
    // default tab stops every half inch, after words of different lengths
    for words in ["a", "word", "longerword", "much longer words here"] {
        let p = Paragraph {
            content: vec![
                Inline::text(words),
                Inline::Tab { marks: vec![] },
                Inline::text("next"),
                Inline::Tab { marks: vec![] },
                Inline::text("third"),
            ],
            ..Default::default()
        };
        document.blocks.push(Block::Paragraph(p));
    }
    // custom stops of every kind, with leaders
    let stops = vec![
        TabStop {
            position: 2160,
            kind: TabKind::Center,
            leader: TabLeader::None,
        },
        TabStop {
            position: 5040,
            kind: TabKind::Decimal,
            leader: TabLeader::Dot,
        },
        TabStop {
            position: 9360,
            kind: TabKind::Right,
            leader: TabLeader::Dot,
        },
    ];
    for (a, b, c) in [
        ("Item", "12.5", "page 3"),
        ("Longer item name", "1234.75", "page 12"),
        ("x", "7", "page 100"),
    ] {
        let p = Paragraph {
            props: ParagraphProps {
                tabs: stops.clone(),
                ..Default::default()
            },
            content: vec![
                Inline::Tab { marks: vec![] },
                Inline::text(a),
                Inline::Tab { marks: vec![] },
                Inline::text(b),
                Inline::Tab { marks: vec![] },
                Inline::text(c),
            ],
            ..Default::default()
        };
        document.blocks.push(Block::Paragraph(p));
    }
    CorpusDocument {
        name: "indents_tabs".into(),
        class: "indents and tabs",
        document,
    }
}

fn lists() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("1", false));
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("2", true));
    document.numbering.add_instance("1");
    document.numbering.add_instance("2");
    let mut rng = Seeded(41);
    for (list, levels) in [
        ("1", [0u8, 0, 1, 1, 0, 2, 0]),
        ("2", [0u8, 0, 1, 1, 2, 0, 0]),
    ] {
        for level in levels {
            let words = 5 + rng.below(40) as usize;
            let mut p = paragraph(&prose(&mut rng, words));
            p.props.list = Some(ListRef {
                list_id: list.into(),
                level,
            });
            document.blocks.push(Block::Paragraph(p));
        }
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, 30))));
    }
    // a long numbered list so labels reach two digits
    for _ in 0..12 {
        let words = 3 + rng.below(12) as usize;
        let mut p = paragraph(&prose(&mut rng, words));
        p.props.list = Some(ListRef {
            list_id: "2".into(),
            level: 0,
        });
        document.blocks.push(Block::Paragraph(p));
    }
    CorpusDocument {
        name: "lists".into(),
        class: "lists",
        document,
    }
}

fn heading_styles(document: &mut Document, family: &str) {
    for (level, size, before, after) in [(1u8, 32u32, 480, 120), (2, 26, 240, 80)] {
        let mut style = Style::paragraph(&format!("Heading{level}"), &format!("heading {level}"));
        style.based_on = Some("Normal".into());
        style.next = Some("Normal".into());
        style.paragraph.keep_with_next = Some(true);
        style.paragraph.keep_lines_together = Some(true);
        style.paragraph.outline_level = Some(level - 1);
        style.paragraph.spacing.before = Some(before);
        style.paragraph.spacing.after = Some(after);
        style.run.font = Some(font(family));
        style.run.size = Some(size);
        style.run.bold = Some(true);
        document.styles.styles.push(style);
    }
}

fn heading(level: u8, text: &str) -> Block {
    Block::Heading(Heading {
        level,
        paragraph: Paragraph {
            style_id: Some(format!("Heading{level}")),
            ..Paragraph::with_text(text)
        },
    })
}

fn headings() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    heading_styles(&mut document, "Calibri Light");
    let mut rng = Seeded(51);
    for chapter in 1..=6 {
        document.blocks.push(heading(
            1,
            &format!("Chapter {chapter}: {}", sentence(&mut rng, 4)),
        ));
        for section in 1..=3 {
            document.blocks.push(heading(
                2,
                &format!("{chapter}.{section} {}", sentence(&mut rng, 5)),
            ));
            for _ in 0..2 {
                let words = 40 + rng.below(80) as usize;
                document
                    .blocks
                    .push(Block::Paragraph(paragraph(&prose(&mut rng, words))));
            }
        }
    }
    CorpusDocument {
        name: "headings".into(),
        class: "headings and keeps",
        document,
    }
}

fn spacing_rules() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 0, LineSpacing::SINGLE);
    let mut rng = Seeded(61);
    // before and after both set: word adds them (no collapsing)
    for (before, after) in [(0, 240), (240, 0), (360, 360), (120, 480), (0, 0)] {
        let mut p = paragraph(&prose(&mut rng, 30));
        p.props.spacing.before = Some(before);
        p.props.spacing.after = Some(after);
        document.blocks.push(Block::Paragraph(p));
    }
    // contextual spacing through a style that sets it, as list paragraph does
    let mut style = Style::paragraph("Contextual", "Contextual");
    style.based_on = Some("Normal".into());
    style
        .paragraph
        .preserved
        .push("<w:contextualSpacing/>".into());
    document.styles.styles.push(style);
    for _ in 0..4 {
        let mut p = paragraph(&prose(&mut rng, 20));
        p.style_id = Some("Contextual".into());
        p.props.spacing.before = Some(240);
        p.props.spacing.after = Some(240);
        document.blocks.push(Block::Paragraph(p));
    }
    // space before at the top of a new page
    document.blocks.push(Block::PageBreak);
    let mut p = paragraph(&prose(&mut rng, 30));
    p.props.spacing.before = Some(720);
    document.blocks.push(Block::Paragraph(p));
    let mut p = paragraph(&prose(&mut rng, 30));
    p.props.page_break_before = Some(true);
    p.props.spacing.before = Some(720);
    document.blocks.push(Block::Paragraph(p));
    CorpusDocument {
        name: "spacing_rules".into(),
        class: "spacing",
        document,
    }
}

fn mixed_runs() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(71);
    for _ in 0..6 {
        let mut content = Vec::new();
        for piece in 0..8 {
            let words = 3 + rng.below(8) as usize;
            let text = format!("{} ", sentence(&mut rng, words));
            let marks = match piece % 6 {
                0 => vec![],
                1 => vec![Mark::Bold],
                2 => vec![Mark::Italic],
                3 => vec![family_mark("Times New Roman"), size_mark(28)],
                4 => vec![size_mark(16)],
                _ => vec![family_mark("Arial"), Mark::Bold, Mark::Italic],
            };
            content.push(Inline::styled(&text, marks));
        }
        document.blocks.push(Block::Paragraph(Paragraph {
            content,
            ..Default::default()
        }));
    }
    CorpusDocument {
        name: "mixed_runs".into(),
        class: "paragraphs",
        document,
    }
}

fn widows_and_keeps() -> CorpusDocument {
    let mut document = base_document("Times New Roman", 24, 0, LineSpacing::SINGLE);
    let mut rng = Seeded(81);
    // paragraphs whose lengths walk across the page bottom, so widow and
    // orphan control decide where page breaks fall
    for i in 0..30 {
        let words = 30 + (i * 37 % 90);
        let mut p = paragraph(&prose(&mut rng, words));
        p.props.spacing.after = Some(120);
        if i % 7 == 3 {
            p.props.keep_lines_together = Some(true);
        }
        if i % 5 == 4 {
            p.props.keep_with_next = Some(true);
        }
        document.blocks.push(Block::Paragraph(p));
    }
    CorpusDocument {
        name: "widows_keeps".into(),
        class: "headings and keeps",
        document,
    }
}

fn mla_essay() -> CorpusDocument {
    let mut document = base_document("Times New Roman", 24, 0, LineSpacing::DOUBLE);
    let mut rng = Seeded(91);
    for _ in 0..34 {
        let words = 90 + rng.below(90) as usize;
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, words))));
    }
    let details = letters_office::presets::MlaDetails {
        student_name: "Jordan Smith".into(),
        instructor: "Professor Alvarez".into(),
        course: "English 101".into(),
        date: "11 November 2026".into(),
        title: "On Measuring Things Carefully".into(),
        insert_heading: true,
    };
    letters_office::presets::apply_mla(&mut document, &details);
    CorpusDocument {
        name: "mla_essay".into(),
        class: "essay",
        document,
    }
}

fn simple_table(columns: &[Twips], rows: usize, rng: &mut Seeded, words: usize) -> Table {
    let border = Border {
        style: "single".into(),
        size: 4,
        color: Some("000000".into()),
    };
    Table {
        column_widths: columns.to_vec(),
        borders: Some(CellBorders {
            top: Some(border.clone()),
            left: Some(border.clone()),
            bottom: Some(border.clone()),
            right: Some(border.clone()),
            inside_horizontal: Some(border.clone()),
            inside_vertical: Some(border),
        }),
        rows: (0..rows)
            .map(|_| TableRow {
                cells: columns
                    .iter()
                    .map(|width| {
                        let length = 1 + rng.below(words as u64) as usize;
                        TableCell {
                            width: Some(*width),
                            ..TableCell::with_text(&sentence(rng, length))
                        }
                    })
                    .collect(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn tables_fixed() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(101);
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    document.blocks.push(Block::Table(simple_table(
        &[2340, 2340, 2340, 2340],
        6,
        &mut rng,
        10,
    )));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    let mut merged = simple_table(&[3120, 3120, 3120], 5, &mut rng, 6);
    merged.rows[0].cells.truncate(2);
    merged.rows[0].cells[0].col_span = 2;
    merged.rows[1].cells[2].row_span = 2;
    merged.rows[2].cells.truncate(2);
    document.blocks.push(Block::Table(merged));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    CorpusDocument {
        name: "tables_fixed".into(),
        class: "tables",
        document,
    }
}

fn tables_auto() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 0, LineSpacing::SINGLE);
    let mut rng = Seeded(111);
    let mut table = simple_table(&[1000, 1000, 1000], 5, &mut rng, 4);
    // no widths at all: the oracles fit columns to their content
    for row in &mut table.rows {
        for cell in &mut row.cells {
            cell.width = None;
        }
    }
    table.rows[2].cells[1] = TableCell::with_text(&prose(&mut rng, 30));
    table.column_widths.clear();
    document.blocks.push(Block::Table(table));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 30))));
    let mut nested = simple_table(&[4680, 4680], 2, &mut rng, 5);
    nested.rows[1].cells[1]
        .blocks
        .push(Block::Table(simple_table(&[2000, 2000], 2, &mut rng, 3)));
    nested.rows[1].cells[1]
        .blocks
        .push(Block::Paragraph(paragraph("after the nested table")));
    document.blocks.push(Block::Table(nested));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 30))));
    CorpusDocument {
        name: "tables_auto".into(),
        class: "tables",
        document,
    }
}

fn tables_long() -> CorpusDocument {
    let mut document = base_document("Times New Roman", 24, 0, LineSpacing::SINGLE);
    let mut rng = Seeded(121);
    let mut table = simple_table(&[1800, 3780, 3780], 60, &mut rng, 14);
    table.rows[0].header = true;
    for (i, row) in table.rows.iter_mut().enumerate() {
        if i % 9 == 4 {
            row.cant_split = true;
            row.cells[1] = TableCell::with_text(&prose(&mut rng, 60));
        }
    }
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 50))));
    document.blocks.push(Block::Table(table));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 50))));
    CorpusDocument {
        name: "tables_long".into(),
        class: "tables",
        document,
    }
}

/// a small solid png, so pictures need no files on disk.
pub fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        let mut data = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height {
            for x in 0..width {
                data.extend_from_slice(&[(x * 255 / width) as u8, (y * 255 / height) as u8, 160]);
            }
        }
        writer.write_image_data(&data).expect("png data");
    }
    out
}

fn add_media(document: &mut Document, id: &str) {
    document.media.push(MediaItem {
        id: id.into(),
        content_type: "image/png".into(),
        source_path: None,
        bytes: png_bytes(64, 48),
    });
}

fn image(id: &str, width: Twips, height: Twips, placement: Placement) -> Inline {
    Inline::Image(Image {
        media_id: id.into(),
        width,
        height,
        placement,
        ..Default::default()
    })
}

fn images_inline() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    add_media(&mut document, "pic1");
    let mut rng = Seeded(131);
    for height in [720, 1440, 2880] {
        let mut content = vec![Inline::text(&format!("{} ", prose(&mut rng, 25)))];
        content.push(image("pic1", height * 4 / 3, height, Placement::Inline));
        content.push(Inline::text(&format!(" {}", prose(&mut rng, 25))));
        document.blocks.push(Block::Paragraph(Paragraph {
            content,
            ..Default::default()
        }));
    }
    let centered = Paragraph {
        props: ParagraphProps {
            alignment: Some(Alignment::Center),
            ..Default::default()
        },
        content: vec![image("pic1", 4320, 3240, Placement::Inline)],
        ..Default::default()
    };
    document.blocks.push(Block::Paragraph(centered));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    CorpusDocument {
        name: "images_inline".into(),
        class: "images",
        document,
    }
}

fn images_floating() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    add_media(&mut document, "pic1");
    let mut rng = Seeded(141);
    let float =
        |horizontal: FloatPosition, vertical: FloatPosition, wrap: Wrap| Placement::Floating {
            horizontal,
            vertical,
            wrap,
        };
    let cases = [
        float(
            FloatPosition::new("margin", 0),
            FloatPosition::new("paragraph", 0),
            Wrap::Square,
        ),
        float(
            FloatPosition {
                align: Some("right".into()),
                ..FloatPosition::new("margin", 0)
            },
            FloatPosition::new("paragraph", 240),
            Wrap::Square,
        ),
        float(
            FloatPosition::new("margin", 2880),
            FloatPosition::new("paragraph", 0),
            Wrap::TopAndBottom,
        ),
        float(
            FloatPosition::new("page", 4320),
            FloatPosition::new("page", 8640),
            Wrap::BehindText,
        ),
    ];
    for placement in cases {
        let content = vec![
            image("pic1", 2160, 1620, placement),
            Inline::text(&prose(&mut rng, 110)),
        ];
        document.blocks.push(Block::Paragraph(Paragraph {
            content,
            ..Default::default()
        }));
    }
    CorpusDocument {
        name: "images_floating".into(),
        class: "images",
        document,
    }
}

fn footnotes() -> CorpusDocument {
    let mut document = base_document("Times New Roman", 24, 0, LineSpacing::DOUBLE);
    let mut rng = Seeded(151);
    let mut note_id = 1;
    for i in 0..14 {
        let mut content = vec![Inline::text(&prose(&mut rng, 50))];
        let notes_here = if i % 3 == 0 { 2 } else { 1 };
        for _ in 0..notes_here {
            let id = note_id.to_string();
            note_id += 1;
            content.push(Inline::NoteReference {
                note_id: id.clone(),
                marks: vec![Mark::Superscript],
            });
            content.push(Inline::text(&format!(" {}", prose(&mut rng, 20))));
            let words = 10 + rng.below(if i == 7 { 260 } else { 50 }) as usize;
            // the note's own number is added by the writer (w:footnoteRef)
            let mut note_paragraph = Paragraph::with_text(&format!(" {}", prose(&mut rng, words)));
            note_paragraph.props.spacing.line = Some(LineSpacing::SINGLE);
            for inline in &mut note_paragraph.content {
                if let Some(marks) = inline.marks_mut() {
                    marks.push(size_mark(20));
                }
            }
            document.notes.push(Note {
                id,
                kind: NoteKind::Footnote,
                special: None,
                blocks: vec![Block::Paragraph(note_paragraph)],
            });
        }
        let mut p = Paragraph {
            content,
            ..Default::default()
        };
        p.props.indent.first_line = Some(720);
        document.blocks.push(Block::Paragraph(p));
    }
    CorpusDocument {
        name: "footnotes".into(),
        class: "footnotes",
        document,
    }
}

fn headers_footers() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(161);
    let header = HeaderFooter {
        id: "h1".into(),
        kind: HeaderFooterKind::Header,
        blocks: vec![Block::Paragraph(Paragraph {
            props: ParagraphProps {
                alignment: Some(Alignment::Right),
                ..Default::default()
            },
            content: vec![
                Inline::text("Running head, page "),
                Inline::Field(Field::page_number()),
                Inline::text(" of "),
                Inline::Field(Field::page_count()),
            ],
            ..Default::default()
        })],
    };
    let first = HeaderFooter {
        id: "h0".into(),
        kind: HeaderFooterKind::Header,
        blocks: vec![
            Block::Paragraph(paragraph("A first page header that is taller")),
            Block::Paragraph(paragraph("with a second line")),
            Block::Paragraph(paragraph("and a third line")),
            Block::Paragraph(paragraph("and a fourth line pushing the body down")),
        ],
    };
    let footer = HeaderFooter {
        id: "f1".into(),
        kind: HeaderFooterKind::Footer,
        blocks: vec![Block::Paragraph(Paragraph {
            props: ParagraphProps {
                alignment: Some(Alignment::Center),
                ..Default::default()
            },
            content: vec![Inline::Field(Field::page_number())],
            ..Default::default()
        })],
    };
    document.headers_footers = vec![header, first, footer];
    let section = &mut document.sections[0];
    section.headers.default = Some("h1".into());
    section.headers.first = Some("h0".into());
    section.footers.default = Some("f1".into());
    section.footers.first = Some("f1".into());
    section.different_first_page = true;
    for _ in 0..14 {
        let words = 60 + rng.below(80) as usize;
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, words))));
    }
    CorpusDocument {
        name: "headers_footers".into(),
        class: "headers and footers",
        document,
    }
}

fn two_columns() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(171);
    document.blocks.push(Block::Paragraph(paragraph(
        "A one column introduction before the columns start.",
    )));
    document.blocks.push(Block::SectionBreak);
    for _ in 0..9 {
        let words = 50 + rng.below(70) as usize;
        let mut p = paragraph(&prose(&mut rng, words));
        p.props.alignment = Some(Alignment::Justify);
        document.blocks.push(Block::Paragraph(p));
    }
    document.blocks.push(Block::SectionBreak);
    document.blocks.push(Block::Paragraph(paragraph(
        "One column again after a balanced two column section.",
    )));
    let columns = Section {
        start: SectionStart::Continuous,
        columns: Columns {
            count: 2,
            space: 720,
            separator: false,
        },
        ..Section::default()
    };
    let after = Section {
        start: SectionStart::Continuous,
        ..Section::default()
    };
    document.sections = vec![Section::default(), columns, after];
    CorpusDocument {
        name: "two_columns".into(),
        class: "columns and sections",
        document,
    }
}

fn section_change() -> CorpusDocument {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(181);
    for _ in 0..5 {
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, 90))));
    }
    document.blocks.push(Block::SectionBreak);
    for _ in 0..6 {
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, 90))));
    }
    let mut landscape = Section::default();
    landscape.page.width = 15840;
    landscape.page.height = 12240;
    landscape.page.orientation = Orientation::Landscape;
    landscape.page.margins.left = 1080;
    landscape.page.margins.right = 1080;
    let mut a4 = Section {
        page: PageSetup::A4,
        ..Section::default()
    };
    a4.start = SectionStart::NextPage;
    document.sections = vec![a4, landscape];
    CorpusDocument {
        name: "section_change".into(),
        class: "columns and sections",
        document,
    }
}
