//! reading rtf into the model: text, character and paragraph formatting,
//! lists, tables and pictures.

use std::collections::HashMap;

use crate::model::*;

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
    /// how many plain characters follow each \u for readers that can't
    /// show it (\ucN). none means the rtf default, one. like other
    /// properties it holds for the group it was set in
    fallback_length: Option<usize>,
    font_table: bool,
}

impl State {
    fn fallback_length(&self) -> usize {
        self.fallback_length.unwrap_or(1)
    }
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
    /// the first half of a character outside the basic plane (an emoji):
    /// \u gives utf-16 units, so it waits here for its second half
    high_surrogate: Option<u16>,
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
        // a first half whose second half never came can't be shown
        if self.high_surrogate.take().is_some() {
            self.add_character('\u{FFFD}');
        }
        self.add_character(c);
    }

    /// one utf-16 unit from \u: a whole character, or one half of a pair.
    fn unicode_unit(&mut self, unit: u16) {
        const HIGH: std::ops::RangeInclusive<u16> = 0xD800..=0xDBFF;
        const LOW: std::ops::RangeInclusive<u16> = 0xDC00..=0xDFFF;
        if HIGH.contains(&unit) {
            if self.high_surrogate.replace(unit).is_some() {
                self.add_character('\u{FFFD}');
            }
            return;
        }
        let c = match (self.high_surrogate.take(), LOW.contains(&unit)) {
            (Some(high), true) => char::decode_utf16([high, unit]).next().and_then(Result::ok),
            (Some(_), false) => {
                self.add_character('\u{FFFD}');
                char::from_u32(u32::from(unit))
            }
            (None, _) => char::from_u32(u32::from(unit)),
        };
        self.add_character(c.unwrap_or('\u{FFFD}'));
    }

    fn add_character(&mut self, c: char) {
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
                // the value is a utf-16 unit, written as a signed 16-bit number
                let unit = if value < 0 { value + 65536 } else { value } as u16;
                self.unicode_unit(unit);
                self.state.unicode_skip = self.state.fallback_length();
            }
            "uc" => self.state.fallback_length = Some(value.max(0) as usize),
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
