//! the second, wider corpus: documents that look like what students
//! really write (essays in several fonts, syllabi, lab reports, papers
//! with many notes, columns, landscape pages, long tables), each made from
//! its own seed with letters-office's writer.
//!
//! every document belongs to the fit set (rules may be fitted on it) or
//! the held-out set (never looked at while fitting; it only says whether
//! the rules generalise). the split alternates inside each class, so both
//! sets cover every class.
//!
//! a few features the writer has no field for (fixed table layout, table
//! widths in percent, exact row heights, cell margins, page borders) are
//! written by patching the document part after writing (`Patch`).

use letters_office::model::*;

use super::corpus::{
    add_media, auto, base_document, family_mark, heading, heading_styles, image, paragraph, prose,
    sentence, simple_table, size_mark, styled_paragraph, CorpusDocument, Seeded,
};

/// a text change to word/document.xml after writing.
#[derive(Clone, Debug)]
pub enum Patch {
    /// put `xml` right before the first `</w:tblPr>` of the n-th table
    /// (0 based), or of every table when `table` is None
    TableProperty { table: Option<usize>, xml: String },
    /// replace text everywhere
    Replace { find: String, with: String },
    /// put `xml` right after every section's `<w:pgMar .../>`
    AfterPageMargins { xml: String },
}

/// one corpus entry with its set and its patches.
pub struct Entry {
    pub document: CorpusDocument,
    pub held_out: bool,
    pub patches: Vec<Patch>,
}

fn entry(name: &str, class: &'static str, document: Document) -> Entry {
    Entry {
        document: CorpusDocument {
            name: name.to_string(),
            class,
            document,
        },
        held_out: false,
        patches: Vec::new(),
    }
}

/// applies patches to the document part's xml.
pub fn apply_patches(xml: &str, patches: &[Patch]) -> String {
    let mut out = xml.to_string();
    for patch in patches {
        out = match patch {
            Patch::Replace { find, with } => out.replace(find.as_str(), with),
            Patch::TableProperty { table, xml } => {
                let mut result = String::with_capacity(out.len() + 64);
                let mut rest = out.as_str();
                let mut index = 0;
                while let Some(at) = rest.find("</w:tblPr>") {
                    result.push_str(&rest[..at]);
                    if table.is_none_or(|wanted| wanted == index) {
                        result.push_str(xml);
                    }
                    result.push_str("</w:tblPr>");
                    rest = &rest[at + "</w:tblPr>".len()..];
                    index += 1;
                }
                result.push_str(rest);
                result
            }
            Patch::AfterPageMargins { xml } => {
                let mut result = String::with_capacity(out.len() + 64);
                let mut rest = out.as_str();
                while let Some(at) = rest.find("<w:pgMar") {
                    let end = rest[at..].find("/>").map_or(rest.len(), |e| at + e + 2);
                    result.push_str(&rest[..end]);
                    result.push_str(xml);
                    rest = &rest[end..];
                }
                result.push_str(rest);
                result
            }
        };
    }
    out
}

/// the second corpus, fit and held-out entries interleaved per class.
pub fn corpus2() -> Vec<Entry> {
    let mut all = vec![
        essay("essay_calibri_108", "Calibri", 22, 160, auto(259), 1300, 0),
        essay(
            "essay_times_double",
            "Times New Roman",
            24,
            0,
            auto(480),
            1500,
            720,
        ),
        essay("essay_arial_15", "Arial", 22, 120, auto(360), 1200, 0),
        essay(
            "essay_georgia_single",
            "Georgia",
            24,
            240,
            auto(240),
            1300,
            0,
        ),
        essay("essay_cambria_115", "Cambria", 22, 200, auto(276), 1300, 0),
        essay("essay_aptos_116", "Aptos", 24, 160, auto(279), 1300, 0),
        essay("essay_verdana_10", "Verdana", 20, 200, auto(240), 1200, 0),
        essay("essay_segoe_108", "Segoe UI", 22, 160, auto(259), 1200, 360),
        mixed_formatting("mixed_sizes_styles", 301, false),
        mixed_formatting("mixed_fonts_justified", 302, true),
        caps_and_scripts("mixed_caps_scripts", 303),
        caps_and_scripts("mixed_caps_scripts_b", 304),
        justify_probe("justify_probe_times_12", "Times New Roman", 24, 401),
        justify_probe("justify_probe_calibri_11", "Calibri", 22, 402),
        justify_probe("justify_probe_arial_10", "Arial", 20, 403),
        justify_probe("justify_probe_georgia_11", "Georgia", 22, 404),
        justify_probe("justify_probe_cambria_12", "Cambria", 24, 405),
        justify_probe("justify_probe_aptos_11", "Aptos", 22, 406),
        syllabus("syllabus_calibri", "Calibri", 22, 501, true),
        syllabus("syllabus_times", "Times New Roman", 24, 502, true),
        syllabus("syllabus_auto_widths", "Calibri", 22, 503, false),
        syllabus("syllabus_arial_auto", "Arial", 20, 504, false),
        lab_report("lab_report_inline", 601, false),
        lab_report("lab_report_floating", 602, true),
        lab_report("lab_report_inline_b", 603, false),
        lab_report("lab_report_floating_b", 604, true),
        nested_lists("lists_nested_bullets", 701, false),
        nested_lists("lists_nested_numbers", 702, true),
        nested_lists("lists_nested_mixed", 703, true),
        nested_lists("lists_nested_bullets_b", 704, false),
        footnote_paper(
            "footnotes_heavy_times",
            "Times New Roman",
            24,
            auto(480),
            801,
        ),
        footnote_paper("footnotes_heavy_calibri", "Calibri", 22, auto(259), 802),
        footnote_paper("footnotes_heavy_garamond", "Cambria", 22, auto(360), 803),
        footnote_paper("footnotes_heavy_arial", "Arial", 20, auto(276), 804),
        columns("columns_two_whole", 2, false, 901),
        columns("columns_three_whole", 3, false, 902),
        columns("columns_two_notes", 2, true, 903),
        columns("columns_three_notes", 3, true, 904),
        balanced_region("columns_balanced_mid", 905),
        balanced_region("columns_balanced_three", 906),
        landscape("landscape_section", false, 1001),
        landscape("landscape_whole_columns", true, 1002),
        headers_first_page("headers_first_diff", 1101, false),
        headers_first_page("headers_first_tall_footer", 1102, true),
        page_borders("page_borders_box", 1103),
        page_borders("page_borders_shadow", 1104),
        long_table("tables_long_merged", 1201, false),
        long_table("tables_long_cant_split", 1202, true),
        fixed_layout_table("tables_fixed_layout", 1203),
        percent_table("tables_pct_width", 1204),
        auto_table("tables_auto_short", 1205, 3),
        auto_table("tables_auto_long_text", 1206, 25),
        row_heights("tables_row_heights", 1207),
        cell_margins("tables_cell_margins", 1208),
        keeps("keeps_chains", 1301),
        keeps("keeps_headings_bottom", 1302),
        floats("floats_square_middle", 1401, false),
        floats("floats_text_boxes", 1402, true),
    ];
    // alternate fit and held-out within each class, in corpus order
    let mut seen: Vec<(&'static str, usize)> = Vec::new();
    for item in &mut all {
        let class = item.document.class;
        let count = match seen.iter_mut().find(|(c, _)| *c == class) {
            Some((_, count)) => {
                *count += 1;
                *count
            }
            None => {
                seen.push((class, 0));
                0
            }
        };
        item.held_out = count % 2 == 1;
    }
    all
}

/// an essay: a title, a heading or two and many paragraphs in one font.
fn essay(
    name: &str,
    family: &str,
    half_points: u32,
    after: Twips,
    line: LineSpacing,
    words: usize,
    first_line: Twips,
) -> Entry {
    let mut document = base_document(family, half_points, after, line);
    heading_styles(&mut document, family);
    let mut rng = Seeded(1000 + name.len() as u64 * 31 + half_points as u64);
    let mut title = paragraph(&sentence(&mut rng, 6));
    title.props.alignment = Some(Alignment::Center);
    document.blocks.push(Block::Paragraph(title));
    let mut written = 0;
    let mut section = 1;
    while written < words {
        if written > 0 && rng.below(5) == 0 {
            document
                .blocks
                .push(heading(2, &format!("{section}. {}", sentence(&mut rng, 4))));
            section += 1;
        }
        let length = 60 + rng.below(110) as usize;
        let mut p = paragraph(&prose(&mut rng, length));
        if first_line > 0 {
            p.props.indent.first_line = Some(first_line);
        }
        document.blocks.push(Block::Paragraph(p));
        written += length;
    }
    if family == "Aptos" {
        document.sections[0].page = PageSetup::A4;
    }
    entry(name, "essay", document)
}

/// paragraphs whose runs change size, weight, slant, underline and font.
fn mixed_formatting(name: &str, seed: u64, justified: bool) -> Entry {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(seed);
    for _ in 0..8 {
        let mut content = Vec::new();
        for piece in 0..10 {
            let words = 2 + rng.below(9) as usize;
            let text = format!("{} ", sentence(&mut rng, words));
            let marks = match (piece + rng.below(3) as usize) % 7 {
                0 => vec![],
                1 => vec![Mark::Bold],
                2 => vec![Mark::Italic, size_mark(24)],
                3 => vec![family_mark("Georgia"), size_mark(20)],
                4 => vec![
                    Mark::Underline {
                        style: "single".into(),
                    },
                    size_mark(26),
                ],
                5 => vec![family_mark("Times New Roman"), Mark::Bold, size_mark(28)],
                _ => vec![family_mark("Arial"), size_mark(18)],
            };
            content.push(Inline::styled(&text, marks));
        }
        let mut p = Paragraph {
            content,
            ..Default::default()
        };
        if justified {
            p.props.alignment = Some(Alignment::Justify);
        }
        document.blocks.push(Block::Paragraph(p));
    }
    entry(name, "paragraphs", document)
}

/// small caps, all caps, colours, highlights, superscripts and subscripts.
fn caps_and_scripts(name: &str, seed: u64) -> Entry {
    let mut document = base_document("Times New Roman", 24, 120, auto(276));
    let mut rng = Seeded(seed);
    for _ in 0..8 {
        let mut content = Vec::new();
        for piece in 0..12 {
            let words = 1 + rng.below(7) as usize;
            let text = format!("{} ", sentence(&mut rng, words));
            let marks = match (piece + rng.below(4) as usize) % 8 {
                0 => vec![Mark::SmallCaps],
                1 => vec![Mark::AllCaps],
                2 => vec![Mark::Color {
                    hex: "C00000".into(),
                }],
                3 => vec![Mark::Highlight {
                    color: "yellow".into(),
                }],
                4 => vec![Mark::Superscript],
                5 => vec![Mark::Subscript],
                6 => vec![Mark::Strike],
                _ => vec![],
            };
            content.push(Inline::styled(&text, marks));
        }
        document.blocks.push(Block::Paragraph(Paragraph {
            content,
            ..Default::default()
        }));
    }
    entry(name, "paragraphs", document)
}

/// words with no inner break opportunities, for the justification probes
const PLAIN_WORDS: &[&str] = &[
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
    "measured",
    "carefully",
    "although",
    "important",
    "describes",
    "chapter",
    "analysis",
    "particular",
    "significant",
    "environment",
    "information",
    "laboratory",
    "temperature",
    "observation",
    "conclusion",
    "literature",
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
    "several",
    "different",
    "always",
    "never",
    "often",
    "through",
    "during",
    "within",
    "without",
];

/// paragraphs of two lines whose first line, with the first word of the
/// second, is too wide by a known share of its spaces (0 to 45 %): what an
/// application does with each tells its justification squeeze rule.
fn justify_probe(name: &str, family: &str, half_points: u32, seed: u64) -> Entry {
    let mut document = base_document(family, half_points, 120, LineSpacing::SINGLE);
    let size = f64::from(half_points) / 2.0;
    let mut library = letters_layout::fonts::FontLibrary::system();
    let font = library.resolve(family, false, false).expect("a font");
    let face = library.font(font).face.clone();
    let width = |text: &str| -> f64 {
        text.chars()
            .map(|ch| f64::from(face.glyph(ch).map_or(0, |g| g.1)) * face.scale(size))
            .sum()
    };
    let space = width(" ");
    let measure = 468.0;
    let mut rng = Seeded(seed);
    let mut bins = [false; 46];
    let mut tries = 0;
    while bins.iter().any(|b| !b) && tries < 200_000 {
        tries += 1;
        let mut words: Vec<&str> = Vec::new();
        let mut line = 0.0;
        // fill until the next word no longer fits naturally
        loop {
            let word = PLAIN_WORDS[rng.below(PLAIN_WORDS.len() as u64) as usize];
            let with = if words.is_empty() {
                width(word)
            } else {
                line + space + width(word)
            };
            if with > measure {
                // the share of the line's spaces it is too wide by
                let spaces = words.len() as f64 * space;
                let over = (with - measure) / spaces;
                let bin = (over * 100.0).round() as usize;
                if bin < bins.len() && !bins[bin] {
                    bins[bin] = true;
                    words.push(word);
                    let tail = sentence(&mut rng, 3).to_lowercase();
                    let text = format!("{} {tail}", words.join(" "));
                    let mut p = paragraph(&text);
                    p.props.alignment = Some(Alignment::Justify);
                    document.blocks.push(Block::Paragraph(p));
                }
                break;
            }
            line = with;
            words.push(word);
        }
    }
    entry(name, "justified", document)
}

/// a course syllabus: details as a two-column table, a weekly schedule
/// with a header row, policies as bullets.
fn syllabus(name: &str, family: &str, half_points: u32, seed: u64, sized: bool) -> Entry {
    let mut document = base_document(family, half_points, 120, auto(259));
    heading_styles(&mut document, family);
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("1", false));
    document.numbering.add_instance("1");
    let mut rng = Seeded(seed);
    let mut title = styled_paragraph(
        &format!(
            "Course {} 101: {}",
            100 + rng.below(800),
            sentence(&mut rng, 4)
        ),
        vec![Mark::Bold, size_mark(32)],
    );
    title.props.alignment = Some(Alignment::Center);
    document.blocks.push(Block::Paragraph(title));
    let mut details = simple_table(&[2600, 6760], 5, &mut rng, 8);
    for (row, label) in
        details
            .rows
            .iter_mut()
            .zip(["Instructor", "Office hours", "Email", "Room", "Textbook"])
    {
        row.cells[0] = TableCell {
            width: Some(2600),
            ..TableCell::with_text(label)
        };
    }
    if !sized {
        unsize(&mut details);
    }
    document.blocks.push(Block::Table(details));
    document.blocks.push(heading(2, "Schedule"));
    let mut schedule = simple_table(&[1200, 1800, 4160, 2200], 14, &mut rng, 9);
    schedule.rows[0].header = true;
    for (cell, text) in schedule.rows[0]
        .cells
        .iter_mut()
        .zip(["Week", "Date", "Topic", "Due"])
    {
        cell.blocks = vec![Block::Paragraph(styled_paragraph(text, vec![Mark::Bold]))];
    }
    for (week, row) in schedule.rows.iter_mut().enumerate().skip(1) {
        row.cells[0].blocks = vec![Block::text(&week.to_string())];
    }
    if !sized {
        unsize(&mut schedule);
    }
    document.blocks.push(Block::Table(schedule));
    document.blocks.push(heading(2, "Policies"));
    for _ in 0..8 {
        let words = 12 + rng.below(30) as usize;
        let mut p = paragraph(&prose(&mut rng, words));
        p.props.list = Some(ListRef {
            list_id: "1".into(),
            level: 0,
        });
        document.blocks.push(Block::Paragraph(p));
    }
    entry(name, "syllabus", document)
}

/// takes every width out of a table, so the applications fit it.
fn unsize(table: &mut Table) {
    table.column_widths.clear();
    for row in &mut table.rows {
        for cell in &mut row.cells {
            cell.width = None;
        }
    }
}

/// a lab report: sections, a data table, figures with captions.
fn lab_report(name: &str, seed: u64, floating: bool) -> Entry {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    heading_styles(&mut document, "Calibri Light");
    add_media(&mut document, "fig");
    let mut rng = Seeded(seed);
    let mut caption = Style::paragraph("Caption", "caption");
    caption.based_on = Some("Normal".into());
    caption.run.italic = Some(true);
    caption.run.size = Some(18);
    caption.paragraph.spacing.after = Some(200);
    document.styles.styles.push(caption);
    for (index, part) in ["Introduction", "Method", "Results", "Discussion"]
        .iter()
        .enumerate()
    {
        document.blocks.push(heading(1, part));
        for _ in 0..2 {
            let words = 60 + rng.below(90) as usize;
            document
                .blocks
                .push(Block::Paragraph(paragraph(&prose(&mut rng, words))));
        }
        if index == 2 {
            let mut data = simple_table(&[2340, 2340, 2340, 2340], 7, &mut rng, 2);
            data.alignment = Some(Alignment::Center);
            document.blocks.push(Block::Table(data));
        }
        let (width, height) = (3600 + 360 * index as i32, 2700);
        let placement = if floating {
            Placement::Floating {
                horizontal: FloatPosition {
                    align: Some(if index % 2 == 0 { "right" } else { "left" }.into()),
                    ..FloatPosition::new("margin", 0)
                },
                vertical: FloatPosition::new("paragraph", 120),
                wrap: Wrap::Square,
            }
        } else {
            Placement::Inline
        };
        let mut figure = Paragraph {
            content: vec![image("fig", width, height, placement)],
            ..Default::default()
        };
        if floating {
            figure
                .content
                .push(Inline::text(&prose(&mut rng, 70 + 20 * index)));
        } else {
            figure.props.alignment = Some(Alignment::Center);
        }
        document.blocks.push(Block::Paragraph(figure));
        let mut caption = paragraph(&format!("Figure {}. {}", index + 1, sentence(&mut rng, 10)));
        caption.style_id = Some("Caption".into());
        document.blocks.push(Block::Paragraph(caption));
    }
    entry(name, "lab reports", document)
}

/// lists three levels deep, with long items that wrap.
fn nested_lists(name: &str, seed: u64, numbered: bool) -> Entry {
    let mut document = base_document("Calibri", 22, 120, auto(259));
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("1", numbered));
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("2", !numbered));
    document.numbering.add_instance("1");
    document.numbering.add_instance("2");
    let mut rng = Seeded(seed);
    for round in 0..4 {
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
        let list = if round % 2 == 0 { "1" } else { "2" };
        for _ in 0..10 {
            let level = rng.below(3) as u8;
            let words = 4 + rng.below(45) as usize;
            let mut p = paragraph(&prose(&mut rng, words));
            p.props.list = Some(ListRef {
                list_id: list.into(),
                level,
            });
            document.blocks.push(Block::Paragraph(p));
        }
    }
    entry(name, "lists", document)
}

/// a paper with notes on almost every sentence, some notes long.
fn footnote_paper(
    name: &str,
    family: &str,
    half_points: u32,
    line: LineSpacing,
    seed: u64,
) -> Entry {
    let mut document = base_document(family, half_points, 0, line);
    let mut rng = Seeded(seed);
    let mut note_id = 1;
    for i in 0..16 {
        let mut content = Vec::new();
        for _ in 0..1 + rng.below(3) {
            content.push(Inline::text(&format!("{} ", prose(&mut rng, 25))));
            let id = note_id.to_string();
            note_id += 1;
            content.push(Inline::NoteReference {
                note_id: id.clone(),
                marks: vec![Mark::Superscript],
            });
            let words = 8 + rng.below(if i % 5 == 2 { 180 } else { 40 }) as usize;
            let mut note = Paragraph::with_text(&format!(" {}", prose(&mut rng, words)));
            note.props.spacing.line = Some(LineSpacing::SINGLE);
            for inline in &mut note.content {
                if let Some(marks) = inline.marks_mut() {
                    marks.push(size_mark(half_points.saturating_sub(4)));
                }
            }
            document.notes.push(Note {
                id,
                kind: NoteKind::Footnote,
                special: None,
                blocks: vec![Block::Paragraph(note)],
            });
        }
        content.push(Inline::text(&format!(" {}", prose(&mut rng, 20))));
        let mut p = Paragraph {
            content,
            ..Default::default()
        };
        p.props.indent.first_line = Some(720);
        document.blocks.push(Block::Paragraph(p));
    }
    entry(name, "footnotes", document)
}

/// a whole document in columns, optionally with footnotes.
fn columns(name: &str, count: u32, notes: bool, seed: u64) -> Entry {
    let mut document = base_document("Times New Roman", 20, 120, LineSpacing::SINGLE);
    let mut rng = Seeded(seed);
    let mut note_id = 1;
    for _ in 0..22 {
        let words = 40 + rng.below(90) as usize;
        let mut content = vec![Inline::text(&prose(&mut rng, words))];
        if notes && rng.below(2) == 0 {
            let id = note_id.to_string();
            note_id += 1;
            content.push(Inline::NoteReference {
                note_id: id.clone(),
                marks: vec![Mark::Superscript],
            });
            let note_words = 8 + rng.below(50) as usize;
            let mut note = Paragraph::with_text(&format!(" {}", prose(&mut rng, note_words)));
            for inline in &mut note.content {
                if let Some(marks) = inline.marks_mut() {
                    marks.push(size_mark(16));
                }
            }
            document.notes.push(Note {
                id,
                kind: NoteKind::Footnote,
                special: None,
                blocks: vec![Block::Paragraph(note)],
            });
        }
        let mut p = Paragraph {
            content,
            ..Default::default()
        };
        p.props.alignment = Some(Alignment::Justify);
        p.props.indent.first_line = Some(288);
        document.blocks.push(Block::Paragraph(p));
    }
    document.sections[0].columns = Columns {
        count,
        space: 432,
        separator: count == 3,
    };
    entry(name, "columns and sections", document)
}

/// one column, then a balanced region in columns, then one column again.
fn balanced_region(name: &str, seed: u64) -> Entry {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(seed);
    let count = if name.ends_with("three") { 3 } else { 2 };
    for _ in 0..2 {
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, 60))));
    }
    document.blocks.push(Block::SectionBreak);
    for _ in 0..5 + rng.below(4) {
        let words = 30 + rng.below(60) as usize;
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, words))));
    }
    document.blocks.push(Block::SectionBreak);
    for _ in 0..3 {
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, 70))));
    }
    let columns = Section {
        start: SectionStart::Continuous,
        columns: Columns {
            count,
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
    entry(name, "columns and sections", document)
}

/// landscape pages: a landscape section with a wide table between
/// portrait ones, or a whole landscape document in two columns.
fn landscape(name: &str, whole: bool, seed: u64) -> Entry {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(seed);
    let mut wide = Section::default();
    wide.page.width = 15840;
    wide.page.height = 12240;
    wide.page.orientation = Orientation::Landscape;
    if whole {
        wide.columns = Columns {
            count: 2,
            space: 720,
            separator: false,
        };
        for _ in 0..14 {
            let words = 60 + rng.below(80) as usize;
            document
                .blocks
                .push(Block::Paragraph(paragraph(&prose(&mut rng, words))));
        }
        document.sections = vec![wide];
    } else {
        for _ in 0..4 {
            document
                .blocks
                .push(Block::Paragraph(paragraph(&prose(&mut rng, 80))));
        }
        document.blocks.push(Block::SectionBreak);
        document
            .blocks
            .push(Block::Table(simple_table(&[2600; 5], 8, &mut rng, 10)));
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, 90))));
        document.blocks.push(Block::SectionBreak);
        for _ in 0..3 {
            document
                .blocks
                .push(Block::Paragraph(paragraph(&prose(&mut rng, 80))));
        }
        wide.start = SectionStart::NextPage;
        let portrait = Section {
            start: SectionStart::NextPage,
            ..Section::default()
        };
        document.sections = vec![Section::default(), wide, portrait];
    }
    entry(name, "columns and sections", document)
}

/// different first page headers and footers, page numbers in fields.
fn headers_first_page(name: &str, seed: u64, tall_footer: bool) -> Entry {
    let mut document = base_document("Times New Roman", 24, 0, auto(480));
    let mut rng = Seeded(seed);
    let header = HeaderFooter {
        id: "h1".into(),
        kind: HeaderFooterKind::Header,
        blocks: vec![Block::Paragraph(Paragraph {
            props: ParagraphProps {
                alignment: Some(Alignment::Right),
                ..Default::default()
            },
            content: vec![Inline::text("Smith "), Inline::Field(Field::page_number())],
            ..Default::default()
        })],
    };
    let first = HeaderFooter {
        id: "h0".into(),
        kind: HeaderFooterKind::Header,
        blocks: vec![
            Block::Paragraph(paragraph("Jordan Smith")),
            Block::Paragraph(paragraph("Professor Alvarez")),
        ],
    };
    let mut footer_blocks = vec![Block::Paragraph(Paragraph {
        props: ParagraphProps {
            alignment: Some(Alignment::Center),
            ..Default::default()
        },
        content: vec![
            Inline::text("Page "),
            Inline::Field(Field::page_number()),
            Inline::text(" of "),
            Inline::Field(Field::page_count()),
        ],
        ..Default::default()
    })];
    if tall_footer {
        for _ in 0..4 {
            footer_blocks.push(Block::Paragraph(paragraph(&sentence(&mut rng, 9))));
        }
    }
    let footer = HeaderFooter {
        id: "f1".into(),
        kind: HeaderFooterKind::Footer,
        blocks: footer_blocks,
    };
    document.headers_footers = vec![header, first, footer];
    let section = &mut document.sections[0];
    section.headers.default = Some("h1".into());
    section.headers.first = Some("h0".into());
    section.footers.default = Some("f1".into());
    section.different_first_page = true;
    for _ in 0..12 {
        let words = 70 + rng.below(80) as usize;
        let mut p = paragraph(&prose(&mut rng, words));
        p.props.indent.first_line = Some(720);
        document.blocks.push(Block::Paragraph(p));
    }
    entry(name, "headers and footers", document)
}

/// a border around every page (only drawn; text must not move).
fn page_borders(name: &str, seed: u64) -> Entry {
    let mut document = base_document("Georgia", 22, 160, auto(276));
    let mut rng = Seeded(seed);
    for _ in 0..10 {
        let words = 60 + rng.below(90) as usize;
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, words))));
    }
    let style = if name.ends_with("shadow") {
        "double"
    } else {
        "single"
    };
    let side = |edge: &str| {
        format!("<w:{edge} w:val=\"{style}\" w:sz=\"12\" w:space=\"24\" w:color=\"1F3864\"/>")
    };
    let mut item = entry(name, "headers and footers", document);
    item.patches.push(Patch::AfterPageMargins {
        xml: format!(
            "<w:pgBorders w:offsetFrom=\"page\">{}{}{}{}</w:pgBorders>",
            side("top"),
            side("left"),
            side("bottom"),
            side("right")
        ),
    });
    item
}

/// a long table over several pages: header rows repeat, cells merged
/// across columns and rows, some rows that may not split.
fn long_table(name: &str, seed: u64, cant_split: bool) -> Entry {
    let mut document = base_document("Times New Roman", 22, 0, LineSpacing::SINGLE);
    let mut rng = Seeded(seed);
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 60))));
    let mut table = simple_table(&[1440, 2880, 2520, 2520], 70, &mut rng, 16);
    table.rows[0].header = true;
    table.rows[1].header = true;
    // the first header row: one cell over the two last columns
    table.rows[0].cells.truncate(3);
    table.rows[0].cells[2].col_span = 2;
    table.rows[0].cells[2].width = Some(5040);
    for (index, row) in table.rows.iter_mut().enumerate().skip(2) {
        if index % 11 == 5 {
            row.cells[1].row_span = 2;
        }
        if cant_split && index % 6 == 3 {
            row.cant_split = true;
            row.cells[2] = TableCell {
                width: Some(2520),
                ..TableCell::with_text(&prose(&mut rng, 70))
            };
        }
    }
    // a row below a merge start gives up its covered cell
    let starts: Vec<usize> = (2..table.rows.len())
        .filter(|i| table.rows[*i].cells.len() > 1 && table.rows[*i].cells[1].row_span == 2)
        .collect();
    for start in starts {
        if start + 1 < table.rows.len() {
            table.rows[start + 1].cells.remove(1);
        }
    }
    document.blocks.push(Block::Table(table));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 60))));
    entry(name, "tables", document)
}

/// a fixed layout table whose cells' text is wider than the columns.
fn fixed_layout_table(name: &str, seed: u64) -> Entry {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(seed);
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    document.blocks.push(Block::Table(simple_table(
        &[1500, 3000, 2000, 2860],
        12,
        &mut rng,
        18,
    )));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    let mut item = entry(name, "tables", document);
    item.patches.push(Patch::TableProperty {
        table: None,
        xml: "<w:tblLayout w:type=\"fixed\"/>".into(),
    });
    item
}

/// a table as wide as the margins in percent, columns left to fit.
fn percent_table(name: &str, seed: u64) -> Entry {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(seed);
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    let mut table = simple_table(&[2000, 2000, 2000], 8, &mut rng, 8);
    unsize(&mut table);
    document.blocks.push(Block::Table(table));
    let mut half = simple_table(&[2000, 2000], 5, &mut rng, 5);
    unsize(&mut half);
    document.blocks.push(Block::Table(half));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    let mut item = entry(name, "tables", document);
    item.patches.push(Patch::Replace {
        find: "<w:tblW w:w=\"0\" w:type=\"auto\"/>".into(),
        with: "<w:tblW w:w=\"5000\" w:type=\"pct\"/>".into(),
    });
    item
}

/// tables with no widths at all, with short or long cell texts.
fn auto_table(name: &str, seed: u64, words: usize) -> Entry {
    let mut document = base_document("Calibri", 22, 0, LineSpacing::SINGLE);
    let mut rng = Seeded(seed);
    for columns in [2usize, 3, 5] {
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, 30))));
        let mut table = simple_table(&vec![1000; columns], 4, &mut rng, words);
        unsize(&mut table);
        document.blocks.push(Block::Table(table));
    }
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 30))));
    entry(name, "tables", document)
}

/// rows with stated heights, at least and exact.
fn row_heights(name: &str, seed: u64) -> Entry {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    let mut rng = Seeded(seed);
    let mut table = simple_table(&[3120, 3120, 3120], 10, &mut rng, 14);
    for (index, row) in table.rows.iter_mut().enumerate() {
        row.height = Some(match index % 3 {
            0 => 400,
            1 => 900,
            _ => 1300,
        });
    }
    document.blocks.push(Block::Table(table));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    let mut exact_rows = simple_table(&[4680, 4680], 6, &mut rng, 12);
    for row in &mut exact_rows.rows {
        row.height = Some(601);
    }
    document.blocks.push(Block::Table(exact_rows));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 40))));
    let mut item = entry(name, "tables", document);
    item.patches.push(Patch::Replace {
        find: "<w:trHeight w:val=\"601\"/>".into(),
        with: "<w:trHeight w:val=\"601\" w:hRule=\"exact\"/>".into(),
    });
    item
}

/// word's usual cell margins (0.08 inch left and right) set on the table.
fn cell_margins(name: &str, seed: u64) -> Entry {
    let mut document = base_document("Calibri", 22, 0, auto(259));
    let mut rng = Seeded(seed);
    for widths in [vec![4680, 4680], vec![2340, 2340, 2340, 2340]] {
        document
            .blocks
            .push(Block::Paragraph(paragraph(&prose(&mut rng, 30))));
        document
            .blocks
            .push(Block::Table(simple_table(&widths, 6, &mut rng, 12)));
    }
    let mut auto_table = simple_table(&[1000, 1000, 1000], 5, &mut rng, 9);
    unsize(&mut auto_table);
    document.blocks.push(Block::Table(auto_table));
    document
        .blocks
        .push(Block::Paragraph(paragraph(&prose(&mut rng, 30))));
    let mut item = entry(name, "tables", document);
    item.patches.push(Patch::TableProperty {
        table: None,
        xml: "<w:tblCellMar><w:top w:w=\"29\" w:type=\"dxa\"/><w:left w:w=\"115\" w:type=\"dxa\"/><w:bottom w:w=\"29\" w:type=\"dxa\"/><w:right w:w=\"115\" w:type=\"dxa\"/></w:tblCellMar>".into(),
    });
    item
}

/// keep-with-next chains and keep-lines-together paragraphs near the
/// foot of pages, with headings.
fn keeps(name: &str, seed: u64) -> Entry {
    let mut document = base_document("Cambria", 22, 120, auto(276));
    heading_styles(&mut document, "Cambria");
    let mut rng = Seeded(seed);
    for i in 0..36 {
        if i % 5 == 0 {
            let words = 3 + rng.below(4) as usize;
            document
                .blocks
                .push(heading(1 + (i % 2) as u8, &sentence(&mut rng, words)));
        }
        let words = 25 + rng.below(110) as usize;
        let mut p = paragraph(&prose(&mut rng, words));
        if i % 4 == 1 {
            p.props.keep_with_next = Some(true);
        }
        if i % 6 == 2 {
            p.props.keep_lines_together = Some(true);
        }
        document.blocks.push(Block::Paragraph(p));
    }
    entry(name, "headings and keeps", document)
}

/// pictures in the middle of a column with text on both sides, and text
/// boxes anchored to paragraphs.
fn floats(name: &str, seed: u64, boxes: bool) -> Entry {
    let mut document = base_document("Calibri", 22, 160, auto(259));
    add_media(&mut document, "pic");
    let mut rng = Seeded(seed);
    for index in 0..5 {
        let placement = Placement::Floating {
            horizontal: FloatPosition::new("margin", 1440 + 720 * (index % 3)),
            vertical: FloatPosition::new("paragraph", 240),
            wrap: if index == 3 {
                Wrap::TopAndBottom
            } else {
                Wrap::Square
            },
        };
        let mut content = Vec::new();
        if !boxes {
            content.push(image("pic", 2160, 1440, placement));
        }
        content.push(Inline::text(&prose(&mut rng, 120)));
        document.blocks.push(Block::Paragraph(Paragraph {
            content,
            ..Default::default()
        }));
        if boxes {
            let mut text_box = TextBox::new(
                0,
                0,
                2880,
                1440,
                vec![Block::Paragraph(paragraph(&sentence(&mut rng, 14)))],
            );
            text_box.horizontal = FloatPosition::new("margin", 720 * (index % 4));
            text_box.vertical = FloatPosition::new("paragraph", 120);
            document.blocks.push(Block::TextBox(text_box));
        }
    }
    entry(name, "images", document)
}
