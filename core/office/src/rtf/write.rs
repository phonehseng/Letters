//! writing rtf: the resolved formatting of every paragraph and run, lists
//! with their labels as text, tables, frames for text boxes.

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
            // rtf keeps a shape's or group's text as plain paragraphs
            Block::Shape(_) | Block::Group(_) => {
                for inner in block.drawing_blocks() {
                    self.block(inner, in_table);
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
