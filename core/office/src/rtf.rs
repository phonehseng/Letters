//! rich text format (rtf 1.9), written from the model and read back.
//!
//! rtf has no styles that apply by themselves (each paragraph repeats its
//! formatting), so the writer writes the resolved formatting of every
//! paragraph and run. lists are written with their labels as text, which
//! every reader shows, but the numbering is no longer live. comments and
//! tracked deletions are left out; text boxes become frames.

use std::collections::HashMap;

use crate::model::*;
use crate::numbering::ListCounter;
use crate::styles::{resolved_paragraph_props, resolved_run_props};
use crate::tables::{grid, Slot};

/// writes a document as rtf.
pub fn write_rtf(document: &Document) -> Vec<u8> {
    let mut writer = RtfWriter::new(document);
    writer.document();
    writer.out.into_bytes()
}

struct RtfWriter<'a> {
    document: &'a Document,
    fonts: Vec<String>,
    colours: Vec<String>,
    counter: ListCounter<'a>,
    out: String,
}

impl<'a> RtfWriter<'a> {
    fn new(document: &'a Document) -> RtfWriter<'a> {
        let mut fonts = vec!["Times New Roman".to_string()];
        fonts.extend(
            crate::compat::fonts_used(document)
                .into_iter()
                .filter(|f| f != "Times New Roman"),
        );
        RtfWriter {
            document,
            fonts,
            colours: Vec::new(),
            counter: ListCounter::new(document),
            out: String::new(),
        }
    }

    fn font(&mut self, name: &str) -> usize {
        if let Some(index) = self.fonts.iter().position(|f| f == name) {
            return index;
        }
        self.fonts.push(name.to_string());
        self.fonts.len() - 1
    }

    /// colour table index; 0 is "automatic".
    fn colour(&mut self, hex: &str) -> usize {
        let hex = hex.to_ascii_uppercase();
        if let Some(index) = self.colours.iter().position(|c| *c == hex) {
            return index + 1;
        }
        self.colours.push(hex);
        self.colours.len()
    }

    fn document(&mut self) {
        // the body is built first so the font and colour tables know
        // everything it uses
        let mut body = String::new();
        std::mem::swap(&mut self.out, &mut body);
        let document = self.document;
        let mut section = 0;
        self.section_start(section);
        for block in &document.blocks {
            if let Block::SectionBreak = block {
                section += 1;
                self.out.push_str("\\sect");
                self.section_start(section);
                continue;
            }
            self.block(block, false);
        }
        std::mem::swap(&mut self.out, &mut body);
        self.out
            .push_str("{\\rtf1\\ansi\\ansicpg1252\\deff0\\uc1{\\fonttbl");
        for (index, font) in self.fonts.iter().enumerate() {
            self.out
                .push_str(&format!("{{\\f{index}\\fnil {};}}", escape(font)));
        }
        self.out.push_str("}{\\colortbl;");
        for hex in &self.colours {
            let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
            self.out.push_str(&format!(
                "\\red{}\\green{}\\blue{};",
                channel(0),
                channel(2),
                channel(4)
            ));
        }
        self.out.push_str("}{\\*\\generator Letters;}{\\info");
        let metadata = &document.metadata;
        for (word, value) in [
            ("title", &metadata.title),
            ("author", &metadata.author),
            ("subject", &metadata.subject),
        ] {
            if let Some(value) = value {
                self.out
                    .push_str(&format!("{{\\{word} {}}}", escape(value)));
            }
        }
        self.out.push('}');
        // without this rtf readers treat footnotes as endnotes
        self.out.push_str("\\ftnbj\\aenddoc");
        if let Some(first) = document.sections.first() {
            self.out.push_str(&page_setup(first));
        }
        self.out.push_str(&body);
        self.out.push_str("}\n");
    }

    fn section_start(&mut self, index: usize) {
        let Some(section) = self.document.sections.get(index) else {
            return;
        };
        self.out.push_str("\\sectd");
        let page = &section.page;
        let m = &page.margins;
        self.out.push_str(&format!(
            "\\pgwsxn{}\\pghsxn{}\\marglsxn{}\\margrsxn{}\\margtsxn{}\\margbsxn{}\\headery{}\\footery{}{}",
            page.width,
            page.height,
            m.left,
            m.right,
            m.top,
            m.bottom,
            m.header,
            m.footer,
            if page.orientation == Orientation::Landscape { "\\lndscpsxn" } else { "" }
        ));
        if section.columns.count > 1 {
            self.out.push_str(&format!(
                "\\cols{}\\colsx{}",
                section.columns.count, section.columns.space
            ));
        }
        for (word, id) in [
            ("header", &section.headers.default),
            ("footer", &section.footers.default),
        ] {
            let Some(part) = id.as_ref().and_then(|id| self.document.header_footer(id)) else {
                continue;
            };
            self.out.push_str(&format!("{{\\{word} "));
            for block in &part.blocks {
                self.block(block, false);
            }
            self.out.push('}');
        }
    }

    fn block(&mut self, block: &Block, in_table: bool) {
        match block {
            Block::Paragraph(_) | Block::Heading(_) => {
                let paragraph = block.paragraph().cloned().unwrap_or_default();
                let label = self.counter.label_for_block(block);
                self.paragraph(&paragraph, block, label, in_table, "");
            }
            Block::Table(table) if !in_table => self.table(table),
            Block::Table(table) => {
                // rtf's nested tables are rarely read right; their text is kept
                for cell in table.rows.iter().flat_map(|row| row.cells.iter()) {
                    for inner in &cell.blocks {
                        self.block(inner, true);
                    }
                }
            }
            Block::PageBreak => self.out.push_str("\\pard\\plain\\page\\par\n"),
            Block::SectionBreak => {}
            Block::Preserved { xml } => {
                let text = crate::xml::parse(xml)
                    .map(|d| d.root.deep_text())
                    .unwrap_or_default();
                if !text.trim().is_empty() {
                    let paragraph = Paragraph::with_text(&text);
                    self.paragraph(&paragraph, block, None, in_table, "");
                }
            }
            Block::TextBox(text_box) => {
                // a frame: rtf's own floating paragraph
                let vertical = if text_box.vertical.relative_to == "page" {
                    "\\pvpg"
                } else if text_box.vertical.relative_to == "margin" {
                    "\\pvmrg"
                } else {
                    "\\pvpara"
                };
                let horizontal = if text_box.horizontal.relative_to == "page" {
                    "\\phpg"
                } else if text_box.horizontal.relative_to == "margin" {
                    "\\phmrg"
                } else {
                    "\\phcol"
                };
                let frame = format!(
                    "\\absw{}{horizontal}{vertical}\\posx{}\\posy{}{}",
                    text_box.width,
                    text_box.horizontal.offset,
                    text_box.vertical.offset,
                    if text_box.auto_height {
                        String::new()
                    } else {
                        format!("\\absh-{}", text_box.height)
                    }
                );
                for inner in &text_box.blocks {
                    if let Some(paragraph) = inner.paragraph() {
                        let label = self.counter.label_for_block(inner);
                        self.paragraph(paragraph, inner, label, false, &frame);
                    }
                }
            }
        }
    }

    fn paragraph(
        &mut self,
        paragraph: &Paragraph,
        block: &Block,
        label: Option<String>,
        in_table: bool,
        frame: &str,
    ) {
        let props = resolved_paragraph_props(self.document, paragraph);
        self.out.push_str("\\pard\\plain");
        if in_table {
            self.out.push_str("\\intbl");
        }
        self.out.push_str(frame);
        if let Block::Heading(heading) = block {
            self.out.push_str(&format!(
                "\\outlinelevel{}",
                heading.level.saturating_sub(1)
            ));
        }
        self.out.push_str(match props.alignment {
            Some(Alignment::Center) => "\\qc",
            Some(Alignment::Right) => "\\qr",
            Some(Alignment::Justify) => "\\qj",
            _ => "\\ql",
        });
        let indent = &props.indent;
        self.out.push_str(&format!(
            "\\li{}\\ri{}\\fi{}\\sb{}\\sa{}",
            indent.left.unwrap_or(0),
            indent.right.unwrap_or(0),
            indent.first_line_offset(),
            props.spacing.before.unwrap_or(0),
            props.spacing.after.unwrap_or(0)
        ));
        if let Some(line) = props.spacing.line {
            match line.rule {
                LineRule::Auto => self.out.push_str(&format!("\\sl{}\\slmult1", line.value)),
                LineRule::Exact => self.out.push_str(&format!("\\sl-{}\\slmult0", line.value)),
                LineRule::AtLeast => self.out.push_str(&format!("\\sl{}\\slmult0", line.value)),
            }
        }
        if props.keep_with_next == Some(true) {
            self.out.push_str("\\keepn");
        }
        if props.page_break_before == Some(true) {
            self.out.push_str("\\pagebb");
        }
        for stop in &props.tabs {
            let kind = match stop.kind {
                TabKind::Center => "\\tqc",
                TabKind::Right => "\\tqr",
                TabKind::Decimal => "\\tqdec",
                _ => "",
            };
            let leader = match stop.leader {
                TabLeader::Dot | TabLeader::MiddleDot => "\\tldot",
                TabLeader::Hyphen => "\\tlhyph",
                TabLeader::Underscore | TabLeader::Heavy => "\\tlul",
                TabLeader::None => "",
            };
            self.out
                .push_str(&format!("{kind}{leader}\\tx{}", stop.position));
        }
        self.out.push(' ');
        let style = paragraph.style_id.as_deref();
        if let Some(label) = label {
            let marks = paragraph
                .content
                .first()
                .map(|i| i.marks().to_vec())
                .unwrap_or_default();
            let run = self.run_props(style, &marks);
            self.out
                .push_str(&format!("{{\\listtext{run} {}\\tab}}", escape(&label)));
        }
        for inline in &paragraph.content {
            self.inline(inline, style);
        }
        self.out.push_str(if in_table { "\n" } else { "\\par\n" });
    }

    fn run_props(&mut self, style: Option<&str>, marks: &[Mark]) -> String {
        let run = resolved_run_props(self.document, style, marks);
        let mut out = String::new();
        let font = run
            .font
            .as_ref()
            .map(|f| f.name.clone())
            .unwrap_or_else(|| "Times New Roman".into());
        out.push_str(&format!(
            "\\f{}\\fs{}",
            self.font(&font),
            run.size.unwrap_or(24)
        ));
        for (on, word) in [
            (run.bold, "\\b"),
            (run.italic, "\\i"),
            (run.strike, "\\strike"),
            (run.double_strike, "\\striked1"),
            (run.small_caps, "\\scaps"),
            (run.all_caps, "\\caps"),
            (run.hidden, "\\v"),
        ] {
            if on == Some(true) {
                out.push_str(word);
            }
        }
        match run.underline.as_deref() {
            None | Some("none") => {}
            Some("double") => out.push_str("\\uldb"),
            Some("dotted") => out.push_str("\\uld"),
            Some("wave") => out.push_str("\\ulwave"),
            Some(_) => out.push_str("\\ul"),
        }
        match run.script {
            Some(Script::Superscript) => out.push_str("\\super"),
            Some(Script::Subscript) => out.push_str("\\sub"),
            _ => {}
        }
        if let Some(colour) = &run.color {
            out.push_str(&format!("\\cf{}", self.colour(colour)));
        }
        if let Some(highlight) = &run.highlight {
            let hex = highlight_hex(highlight);
            out.push_str(&format!("\\highlight{}", self.colour(&hex)));
        }
        out
    }

    fn inline(&mut self, inline: &Inline, style: Option<&str>) {
        if inline.is_deleted() {
            return;
        }
        let marks = inline.marks().to_vec();
        let content = match inline {
            Inline::Text { text, .. } => escape(text),
            Inline::Tab { .. } => "\\tab ".into(),
            Inline::Break {
                kind: BreakKind::Page,
                ..
            } => "\\page ".into(),
            Inline::Break {
                kind: BreakKind::Column,
                ..
            } => "\\column ".into(),
            Inline::Break { .. } => "\\line ".into(),
            Inline::Field(field) => format!(
                "{{\\field{{\\*\\fldinst {{{}}}}}{{\\fldrslt {{{}}}}}}}",
                escape(&field.instruction),
                escape(&field.result)
            ),
            Inline::Image(image) => self.picture(image),
            Inline::NoteReference { note_id, .. } => self.note(note_id),
            Inline::Preserved { xml } | Inline::PreservedRun { xml, .. } => escape(
                &crate::xml::parse(xml)
                    .map(|d| d.root.deep_text())
                    .unwrap_or_default(),
            ),
            Inline::BookmarkStart { name, .. } => format!("{{\\*\\bkmkstart {}}}", escape(name)),
            Inline::BookmarkEnd { .. } | Inline::CommentReference { .. } => return,
        };
        let run = self.run_props(style, &marks);
        let link = marks.iter().find_map(|m| match m {
            Mark::Link(target) => target
                .href
                .clone()
                .or_else(|| target.anchor.as_ref().map(|a| format!("#{a}"))),
            _ => None,
        });
        match link {
            Some(href) => self.out.push_str(&format!(
                "{{\\field{{\\*\\fldinst {{HYPERLINK \"{}\"}}}}{{\\fldrslt {{{run} {content}}}}}}}",
                escape(&href)
            )),
            None => self.out.push_str(&format!("{{{run} {content}}}")),
        }
    }

    fn picture(&mut self, image: &Image) -> String {
        let Some(item) = self.document.media_item(&image.media_id) else {
            return String::new();
        };
        let kind = match item.content_type.as_str() {
            "image/png" => "\\pngblip",
            "image/jpeg" => "\\jpegblip",
            _ => return "[picture]".into(),
        };
        let format = crate::clipboard::PictureFormat::sniff(&item.bytes);
        let (wide, high) = format
            .and_then(|f| crate::clipboard::pixel_size(f, &item.bytes))
            .unwrap_or((image.width as u32 / 15, image.height as u32 / 15));
        let mut hex = String::with_capacity(item.bytes.len() * 2 + item.bytes.len() / 64);
        for (index, byte) in item.bytes.iter().enumerate() {
            if index % 64 == 0 {
                hex.push('\n');
            }
            hex.push_str(&format!("{byte:02x}"));
        }
        format!(
            "{{\\pict{kind}\\picw{wide}\\pich{high}\\picwgoal{}\\pichgoal{}{hex}}}",
            image.width, image.height
        )
    }

    fn note(&mut self, note_id: &str) -> String {
        let Some(note) = self.document.note(note_id) else {
            return String::new();
        };
        let alternative = if note.kind == NoteKind::Endnote {
            "\\ftnalt"
        } else {
            ""
        };
        let saved = std::mem::take(&mut self.out);
        for block in &note.blocks {
            if let Some(paragraph) = block.paragraph() {
                self.paragraph(paragraph, block, None, false, "");
            }
        }
        let body = std::mem::replace(&mut self.out, saved);
        let body = body.trim_end().trim_end_matches("\\par").to_string();
        format!(
            "{{\\super\\chftn}}{{\\footnote{alternative}\\pard\\plain{{\\super\\chftn}} {}}}",
            body.trim_start_matches("\\pard\\plain")
        )
    }

    fn table(&mut self, table: &Table) {
        let layout = grid(table);
        let columns = layout.first().map(Vec::len).unwrap_or(0).max(1);
        let share = table.width.unwrap_or(9360) / columns as i32;
        let widths: Vec<i32> = (0..columns)
            .map(|c| {
                table
                    .column_widths
                    .get(c)
                    .copied()
                    .filter(|w| *w > 0)
                    .unwrap_or(share)
            })
            .collect();
        for (row_index, row) in table.rows.iter().enumerate() {
            self.out.push_str("\\trowd\\trgaph108");
            if row.header {
                self.out.push_str("\\trhdr");
            }
            let mut edge = 0;
            let mut cells: Vec<Option<&TableCell>> = Vec::new();
            for (column, slot) in layout[row_index].iter().enumerate() {
                match *slot {
                    Slot::Cell { cell, first: true } => {
                        let model = &row.cells[cell];
                        let span = model.col_span.max(1) as usize;
                        edge += widths[column..(column + span).min(columns)]
                            .iter()
                            .sum::<i32>();
                        let merge = if model.row_span > 1 { "\\clvmgf" } else { "" };
                        let borders =
                            cell_borders(model.borders.as_ref().or(table.borders.as_ref()));
                        let shading = match &model.shading {
                            Some(fill) => format!("\\clcbpat{}", self.colour(fill)),
                            None => String::new(),
                        };
                        self.out
                            .push_str(&format!("{merge}{borders}{shading}\\cellx{edge}"));
                        cells.push(Some(model));
                    }
                    Slot::Covered {
                        row: from,
                        cell,
                        first: true,
                    } => {
                        let covering = &table.rows[from].cells[cell];
                        let span = covering.col_span.max(1) as usize;
                        edge += widths[column..(column + span).min(columns)]
                            .iter()
                            .sum::<i32>();
                        self.out.push_str(&format!(
                            "\\clvmrg{}\\cellx{edge}",
                            cell_borders(covering.borders.as_ref().or(table.borders.as_ref()))
                        ));
                        cells.push(None);
                    }
                    Slot::Empty => {
                        edge += widths[column];
                        self.out.push_str(&format!("\\cellx{edge}"));
                        cells.push(None);
                    }
                    _ => {}
                }
            }
            self.out.push('\n');
            for cell in cells {
                match cell {
                    Some(cell) => {
                        for block in &cell.blocks {
                            self.block(block, true);
                        }
                        if cell.blocks.is_empty() {
                            self.out.push_str("\\pard\\plain\\intbl ");
                        }
                    }
                    None => self.out.push_str("\\pard\\plain\\intbl "),
                }
                self.out.push_str("\\cell\n");
            }
            self.out.push_str("\\row\n");
        }
        self.out.push_str("\\pard\\plain ");
    }
}

fn page_setup(section: &Section) -> String {
    let page = &section.page;
    let margins = &page.margins;
    format!(
        "\\paperw{}\\paperh{}\\margl{}\\margr{}\\margt{}\\margb{}{}",
        page.width,
        page.height,
        margins.left,
        margins.right,
        margins.top,
        margins.bottom,
        if page.orientation == Orientation::Landscape {
            "\\landscape"
        } else {
            ""
        }
    )
}

fn cell_borders(borders: Option<&CellBorders>) -> String {
    let Some(borders) = borders else {
        return String::new();
    };
    let mut out = String::new();
    for (word, border) in [
        ("t", &borders.top),
        ("l", &borders.left),
        ("b", &borders.bottom),
        ("r", &borders.right),
    ] {
        if let Some(border) = border
            .as_ref()
            .filter(|b| b.style != "none" && b.style != "nil")
        {
            let kind = if border.style == "double" {
                "\\brdrdb"
            } else {
                "\\brdrs"
            };
            out.push_str(&format!(
                "\\clbrdr{word}{kind}\\brdrw{}",
                (border.size * 5 / 2).max(1)
            ));
        }
    }
    out
}

fn highlight_hex(color: &str) -> String {
    if color.len() == 6 && color.chars().all(|c| c.is_ascii_hexdigit()) {
        return color.to_string();
    }
    match color {
        "green" => "00FF00",
        "cyan" => "00FFFF",
        "magenta" => "FF00FF",
        "blue" => "0000FF",
        "red" => "FF0000",
        "black" => "000000",
        "white" => "FFFFFF",
        "lightGray" => "C0C0C0",
        "darkGray" => "808080",
        _ => "FFFF00",
    }
    .into()
}

/// rtf escapes for text: braces and backslashes, and unicode beyond
/// ascii as `\uN?`.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            '\t' => out.push_str("\\tab "),
            '\n' => out.push_str("\\line "),
            '\u{A0}' => out.push_str("\\~"),
            c if (c as u32) < 0x80 => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{}?", *unit as i16));
                }
            }
        }
    }
    out
}

// reading -----------------------------------------------------------------

/// windows-1252 characters 0x80 to 0x9f; the rest of the code page is latin-1.
const CP1252: [u16; 32] = [
    0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
];

#[derive(Clone, Default)]
struct State {
    run: RunProps,
    skip: bool,
    destination: Option<&'static str>,
    unicode_skip: usize,
    font_table: bool,
}

/// reads rtf: text, character and paragraph formatting, lists (as list
/// paragraphs), tables, pictures, footnotes and the title.
pub fn read_rtf(bytes: &[u8]) -> Result<Document, crate::OfficeError> {
    if !bytes.starts_with(b"{\\rtf") {
        return Err(crate::OfficeError::NotAPackage(
            "it is not an RTF file".into(),
        ));
    }
    let mut reader = RtfReader {
        document: Document::new(),
        ..Default::default()
    };
    reader.document.blocks.clear();
    reader
        .document
        .numbering
        .definitions
        .push(ListDefinition::standard("rtf-numbers", true));
    reader
        .document
        .numbering
        .definitions
        .push(ListDefinition::standard("rtf-bullets", false));
    reader.parse(bytes);
    reader.end_paragraph();
    if reader.document.blocks.is_empty() {
        reader
            .document
            .blocks
            .push(Block::Paragraph(Paragraph::default()));
    }
    Ok(reader.document)
}

#[derive(Default)]
struct RtfReader {
    document: Document,
    stack: Vec<State>,
    state: State,
    fonts: HashMap<i32, String>,
    font_number: i32,
    colours: Vec<String>,
    colour_parts: (u8, u8, u8),
    paragraph: Paragraph,
    in_table: bool,
    row: TableRow,
    cell: Vec<Block>,
    table: Option<Table>,
    lists: HashMap<i32, String>,
    list: Option<(i32, u8)>,
    bullet_label: bool,
    destination_text: String,
    picture: Vec<u8>,
    picture_size: (i32, i32),
    picture_type: Option<&'static str>,
    pending: u32,
}

impl RtfReader {
    fn parse(&mut self, bytes: &[u8]) {
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'{' => {
                    self.stack.push(self.state.clone());
                    self.state.destination = None;
                    index += 1;
                }
                b'}' => {
                    self.close_group();
                    index += 1;
                }
                b'\\' => index = self.control(bytes, index + 1),
                b'\r' | b'\n' => index += 1,
                byte => {
                    if self.state.destination == Some("pict") {
                        if let Some(value) = (byte as char).to_digit(16) {
                            if self.pending > 15 {
                                self.picture.push(((self.pending - 16) << 4 | value) as u8);
                                self.pending = 0;
                            } else {
                                self.pending = 16 + value;
                            }
                        }
                    } else {
                        self.character(char::from(byte));
                    }
                    index += 1;
                }
            }
            if self.stack.len() > 1000 {
                // hostile nesting: stop reading rather than run out of memory
                break;
            }
        }
    }

    fn close_group(&mut self) {
        let finished = self.state.destination;
        match finished {
            Some("fontname") => {
                let name = self
                    .destination_text
                    .trim()
                    .trim_end_matches(';')
                    .to_string();
                self.fonts.insert(self.font_number, name);
            }
            Some("title") => {
                self.document.metadata.title = Some(self.destination_text.trim().to_string())
            }
            Some("author") => {
                self.document.metadata.author = Some(self.destination_text.trim().to_string())
            }
            Some("listtext") => {
                let label = self.destination_text.trim();
                self.bullet_label = label
                    .chars()
                    .any(|c| "\u{2022}\u{B7}\u{25E6}\u{25AA}-o\u{F0B7}".contains(c))
                    && !label.chars().any(|c| c.is_ascii_digit());
                // a label without list tables (as this writer makes them) still
                // means a list item
                if self.list.is_none() {
                    self.list = Some((0, 0));
                }
            }
            Some("pict") => self.finish_picture(),
            _ => {}
        }
        if finished.is_some() {
            self.destination_text.clear();
        }
        self.state = self.stack.pop().unwrap_or_default();
    }

    fn character(&mut self, c: char) {
        if self.state.unicode_skip > 0 {
            self.state.unicode_skip -= 1;
            return;
        }
        if self.state.destination.is_some() {
            self.destination_text.push(c);
            return;
        }
        if self.state.skip {
            return;
        }
        let marks = self.state.run.to_marks();
        match self.paragraph.content.last_mut() {
            Some(Inline::Text { text, marks: last }) if *last == marks => text.push(c),
            _ => self.paragraph.content.push(Inline::Text {
                text: c.to_string(),
                marks,
            }),
        }
    }

    /// reads one control word or symbol starting after the backslash and
    /// returns where reading goes on.
    fn control(&mut self, bytes: &[u8], start: usize) -> usize {
        let Some(&first) = bytes.get(start) else {
            return start;
        };
        if !first.is_ascii_alphabetic() {
            return self.symbol(bytes, start);
        }
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_alphabetic() && end - start < 32 {
            end += 1;
        }
        let word = String::from_utf8_lossy(&bytes[start..end]).to_string();
        let number_start = end;
        if end < bytes.len() && bytes[end] == b'-' {
            end += 1;
        }
        while end < bytes.len() && bytes[end].is_ascii_digit() && end - number_start < 10 {
            end += 1;
        }
        let parameter = std::str::from_utf8(&bytes[number_start..end])
            .ok()
            .and_then(|n| n.parse::<i32>().ok());
        if end < bytes.len() && bytes[end] == b' ' {
            end += 1;
        }
        self.word(&word, parameter);
        end
    }

    fn symbol(&mut self, bytes: &[u8], start: usize) -> usize {
        match bytes[start] {
            b'\'' => {
                let hex = bytes
                    .get(start + 1..start + 3)
                    .and_then(|h| std::str::from_utf8(h).ok());
                if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    let c = if (0x80..0xA0).contains(&value) {
                        char::from_u32(u32::from(CP1252[usize::from(value - 0x80)])).unwrap_or('?')
                    } else {
                        char::from(value)
                    };
                    self.character(c);
                }
                start + 3
            }
            b'*' => {
                // an optional destination this reader does not know
                self.state.skip = true;
                start + 1
            }
            b'~' => {
                self.character('\u{A0}');
                start + 1
            }
            b'_' => {
                self.character('\u{2011}');
                start + 1
            }
            b'-' => start + 1,
            b'\n' | b'\r' => {
                self.end_paragraph();
                start + 1
            }
            other => {
                self.character(char::from(other));
                start + 1
            }
        }
    }

    fn word(&mut self, word: &str, parameter: Option<i32>) {
        let on = parameter != Some(0);
        let value = parameter.unwrap_or(0);
        let run = &mut self.state.run;
        match word {
            "fonttbl" => {
                self.state.skip = true;
                self.state.font_table = true;
            }
            "colortbl" | "stylesheet" | "info" | "header" | "footer" | "headerl" | "headerr"
            | "headerf" | "footerl" | "footerr" | "footerf" | "fldinst" | "pntext"
            | "listtable" | "listoverridetable" | "rsidtbl" | "generator" | "xmlnstbl"
            | "themedata" | "colorschememapping" | "latentstyles" | "datastore" | "pgdsctbl"
            | "annotation" | "atnid" | "atnauthor" => self.state.skip = true,
            "title" | "author" | "listtext" => {
                self.state.destination = Some(match word {
                    "title" => "title",
                    "author" => "author",
                    _ => "listtext",
                })
            }
            "f" if self.state.font_table => {
                self.font_number = value;
                self.state.destination = Some("fontname");
                self.state.skip = false;
            }
            "f" => {
                run.font = self.fonts.get(&value).map(|name| FontRef {
                    name: name.clone(),
                    theme: None,
                    extra: Vec::new(),
                })
            }
            "red" => self.colour_parts.0 = value as u8,
            "green" => self.colour_parts.1 = value as u8,
            "blue" => {
                self.colour_parts.2 = value as u8;
                let (r, g, b) = self.colour_parts;
                self.colours.push(format!("{r:02X}{g:02X}{b:02X}"));
            }
            "b" => run.bold = Some(on),
            "i" => run.italic = Some(on),
            "ul" => run.underline = Some(if on { "single".into() } else { "none".into() }),
            "uldb" => run.underline = Some("double".into()),
            "ulnone" => run.underline = Some("none".into()),
            "strike" => run.strike = Some(on),
            "scaps" => run.small_caps = Some(on),
            "caps" => run.all_caps = Some(on),
            "v" => run.hidden = Some(on),
            "super" => run.script = Some(Script::Superscript),
            "sub" => run.script = Some(Script::Subscript),
            "nosupersub" => run.script = None,
            "fs" => run.size = Some(value.max(1) as u32),
            "cf" => {
                run.color = self
                    .colours
                    .get((value - 1).max(0) as usize)
                    .filter(|_| value > 0)
                    .cloned()
            }
            "plain" => self.state.run = RunProps::default(),
            "u" => {
                let code = if value < 0 { value + 65536 } else { value } as u32;
                if let Some(c) = char::from_u32(code) {
                    self.character(c);
                }
                self.state.unicode_skip = 1;
            }
            "par" | "sect" => self.end_paragraph(),
            "line" => self.push(Inline::Break {
                kind: BreakKind::Line,
                marks: Vec::new(),
            }),
            "tab" => self.push(Inline::Tab {
                marks: self.state.run.to_marks(),
            }),
            "page" => {
                self.end_paragraph();
                self.document.blocks.push(Block::PageBreak);
            }
            "pard" => {
                self.paragraph.props = ParagraphProps::default();
                self.in_table = false;
                self.list = None;
            }
            "intbl" => self.in_table = true,
            "ql" => self.paragraph.props.alignment = Some(Alignment::Left),
            "qc" => self.paragraph.props.alignment = Some(Alignment::Center),
            "qr" => self.paragraph.props.alignment = Some(Alignment::Right),
            "qj" => self.paragraph.props.alignment = Some(Alignment::Justify),
            "li" => self.paragraph.props.indent.left = Some(value),
            "ri" => self.paragraph.props.indent.right = Some(value),
            "fi" if value < 0 => self.paragraph.props.indent.hanging = Some(-value),
            "fi" => self.paragraph.props.indent.first_line = Some(value),
            "sb" => self.paragraph.props.spacing.before = Some(value),
            "sa" => self.paragraph.props.spacing.after = Some(value),
            "keepn" => self.paragraph.props.keep_with_next = Some(true),
            "ls" => self.list = Some((value, self.list.map(|l| l.1).unwrap_or(0))),
            "ilvl" => {
                self.list = Some((self.list.map(|l| l.0).unwrap_or(1), value.clamp(0, 8) as u8))
            }
            "cell" => self.end_cell(),
            "row" => self.end_row(),
            "pict" => {
                self.state.destination = Some("pict");
                self.picture.clear();
                self.picture_type = None;
                self.pending = 0;
            }
            "pngblip" => self.picture_type = Some("image/png"),
            "jpegblip" => self.picture_type = Some("image/jpeg"),
            "picwgoal" => self.picture_size.0 = value,
            "pichgoal" => self.picture_size.1 = value,
            "footnote" => self.state.skip = true,
            _ => {}
        }
    }

    fn push(&mut self, inline: Inline) {
        if !self.state.skip && self.state.destination.is_none() {
            self.paragraph.content.push(inline);
        }
    }

    fn finish_picture(&mut self) {
        let Some(content_type) = self.picture_type else {
            return;
        };
        let id = format!(
            "rtf-{}.{}",
            self.document.media.len() + 1,
            if content_type == "image/png" {
                "png"
            } else {
                "jpg"
            }
        );
        self.document.media.push(MediaItem {
            id: id.clone(),
            content_type: content_type.into(),
            source_path: None,
            bytes: std::mem::take(&mut self.picture),
        });
        let (width, height) = self.picture_size;
        self.paragraph.content.push(Inline::Image(Image {
            media_id: id,
            width: width.max(15),
            height: height.max(15),
            ..Default::default()
        }));
    }

    fn take_paragraph(&mut self) -> Block {
        let mut paragraph = std::mem::take(&mut self.paragraph);
        let props = paragraph.props.clone();
        self.paragraph.props = props;
        paragraph.content = merge_adjacent_text(paragraph.content);
        if let Some((ls, level)) = self.list {
            let bullet = self.bullet_label;
            let instance = match self.lists.get(&ls) {
                Some(instance) => instance.clone(),
                None => {
                    let definition = if bullet { "rtf-bullets" } else { "rtf-numbers" };
                    let instance = self.document.numbering.restart(definition);
                    self.lists.insert(ls, instance.clone());
                    instance
                }
            };
            paragraph.props.list = Some(ListRef {
                list_id: instance,
                level,
            });
            // the label carried the indent; the list level gives it again
            paragraph.props.indent = Indent::default();
        }
        Block::Paragraph(paragraph)
    }

    fn end_paragraph(&mut self) {
        if self.in_table {
            let block = self.take_paragraph();
            self.cell.push(block);
            return;
        }
        if let Some(table) = self.table.take() {
            self.document.blocks.push(Block::Table(table));
        }
        let block = self.take_paragraph();
        let empty =
            matches!(&block, Block::Paragraph(p) if p.content.is_empty() && p.props.list.is_none());
        if !empty || !self.document.blocks.is_empty() {
            self.document.blocks.push(block);
        }
    }

    fn end_cell(&mut self) {
        if !self.paragraph.content.is_empty() || self.cell.is_empty() {
            let block = self.take_paragraph();
            self.cell.push(block);
        }
        let blocks = std::mem::take(&mut self.cell);
        self.row.cells.push(TableCell {
            blocks,
            ..Default::default()
        });
    }

    fn end_row(&mut self) {
        let row = std::mem::take(&mut self.row);
        self.table.get_or_insert_with(Table::default).rows.push(row);
        self.paragraph = Paragraph::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_text_formatting_lists_and_tables() {
        let mut document = Document::new();
        document
            .numbering
            .definitions
            .push(ListDefinition::standard("n", true));
        let list = document.numbering.add_instance("n");
        let mut item = Paragraph::with_text("first item");
        item.props.list = Some(ListRef {
            list_id: list,
            level: 0,
        });
        document.metadata.title = Some("Café {test}".into());
        document.blocks = vec![
            Block::Paragraph(Paragraph {
                content: vec![
                    Inline::text("Plain, "),
                    Inline::styled("bold", vec![Mark::Bold]),
                    Inline::text(" and “ü”."),
                ],
                ..Default::default()
            }),
            Block::Paragraph(item),
            Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![TableCell::with_text("a"), TableCell::with_text("b")],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            Block::text("after"),
        ];
        let rtf = write_rtf(&document);
        let text = String::from_utf8(rtf.clone()).unwrap();
        assert!(text.starts_with("{\\rtf1"));
        assert!(text.contains("{\\listtext"));
        let back = read_rtf(&rtf).unwrap();
        assert_eq!(back.metadata.title.as_deref(), Some("Café {test}"));
        assert_eq!(
            back.blocks[0].paragraph().unwrap().plain_text(),
            "Plain, bold and “ü”."
        );
        assert!(back.blocks[0].paragraph().unwrap().content.iter().any(|i| matches!(i, Inline::Text { text, marks } if text == "bold" && marks.contains(&Mark::Bold))));
        assert_eq!(
            back.blocks[1].paragraph().unwrap().plain_text(),
            "first item"
        );
        assert!(back.blocks[1].paragraph().unwrap().props.list.is_some());
        let Block::Table(table) = &back.blocks[2] else {
            panic!("{:?}", back.blocks)
        };
        assert_eq!(table.rows[0].cells.len(), 2);
        assert_eq!(back.blocks[3].paragraph().unwrap().plain_text(), "after");
    }

    #[test]
    fn broken_rtf_does_not_panic() {
        for text in [
            "{\\rtf1",
            "{\\rtf1 \\'",
            "{\\rtf1 {{{{",
            "{\\rtf1 \\u-",
            "{\\rtf1}}}}}",
        ] {
            let _ = read_rtf(text.as_bytes());
        }
        assert!(read_rtf(b"hello").is_err());
    }
}
