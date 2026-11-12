//! building a document from plain text or from html, for pasting.
//!
//! html is read through a small, forgiving tokenizer that only knows a
//! safe subset: paragraphs, headings, line breaks, bold, italics,
//! underline, strike, sub and superscript, links (web and mail
//! addresses only), lists, tables, quotes, preformatted text and
//! pictures given as data. scripts, styles and everything they hold are
//! dropped; unknown tags are ignored but their text is kept.

use std::collections::HashMap;

use crate::model::*;

/// one paragraph per line; tabs become tabs and a form feed a page break.
pub fn from_plain_text(text: &str) -> Document {
    let mut document = Document::new();
    document.blocks.clear();
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    for line in text.split('\n') {
        for (index, page) in line.split('\u{C}').enumerate() {
            if index > 0 {
                document.blocks.push(Block::PageBreak);
            }
            document.blocks.push(Block::Paragraph(text_paragraph(page)));
        }
    }
    if text.ends_with('\n') {
        document.blocks.pop();
    }
    if document.blocks.is_empty() {
        document.blocks.push(Block::Paragraph(Paragraph::default()));
    }
    document
}

fn text_paragraph(line: &str) -> Paragraph {
    let mut paragraph = Paragraph::default();
    for (index, piece) in line.split('\t').enumerate() {
        if index > 0 {
            paragraph.content.push(Inline::Tab { marks: Vec::new() });
        }
        if !piece.is_empty() {
            paragraph.content.push(Inline::text(piece));
        }
    }
    paragraph
}

/// a piece of html: a tag or text.
#[derive(Debug, PartialEq)]
enum Token {
    Open {
        name: String,
        attributes: HashMap<String, String>,
        closed: bool,
    },
    Close(String),
    Text(String),
}

/// splits html into tags and text, never failing: whatever cannot be a
/// tag is text.
fn tokenize(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map(|end| &after[end + 3..]).unwrap_or("");
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            rest = rest.find('>').map(|end| &rest[end + 1..]).unwrap_or("");
            continue;
        }
        if rest.starts_with('<') {
            if let Some((token, length)) = read_tag(rest) {
                tokens.push(token);
                rest = &rest[length..];
                continue;
            }
        }
        let end = rest[1..].find('<').map(|i| i + 1).unwrap_or(rest.len());
        tokens.push(Token::Text(decode_entities(&rest[..end])));
        rest = &rest[end..];
    }
    tokens
}

fn read_tag(text: &str) -> Option<(Token, usize)> {
    let end = tag_end(text)?;
    let inner = &text[1..end];
    if let Some(name) = inner.strip_prefix('/') {
        let name = name.trim().to_ascii_lowercase();
        return Some((Token::Close(name), end + 1));
    }
    let closed = inner.ends_with('/');
    let inner = inner.trim_end_matches('/');
    let name_end = inner
        .find(|c: char| c.is_whitespace())
        .unwrap_or(inner.len());
    let name = inner[..name_end].to_ascii_lowercase();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let attributes = read_attributes(&inner[name_end..]);
    Some((
        Token::Open {
            name,
            attributes,
            closed,
        },
        end + 1,
    ))
}

/// the index of the `>` ending the tag at the start of `text`, skipping
/// quoted values.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (index, c) in text.char_indices().skip(1) {
        match (quote, c) {
            (None, '"') | (None, '\'') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            (None, '>') => return Some(index),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

fn read_attributes(text: &str) -> HashMap<String, String> {
    let mut attributes = HashMap::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let name_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_ascii_lowercase();
        rest = rest[name_end..].trim_start();
        let mut value = String::new();
        if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            let (raw, remaining) = match after.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let close = after[1..].find(quote).map(|i| i + 1).unwrap_or(after.len());
                    (&after[1..close], after.get(close + 1..).unwrap_or(""))
                }
                _ => {
                    let close = after.find(char::is_whitespace).unwrap_or(after.len());
                    (&after[..close], &after[close..])
                }
            };
            value = decode_entities(raw);
            rest = remaining.trim_start();
        }
        if !name.is_empty() {
            attributes.insert(name, value);
        }
    }
    attributes
}

/// html character references; unknown ones are left as written.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let end = after.find(';').filter(|end| *end <= 10);
        let decoded = end.and_then(|end| entity(&after[..end]));
        match (end, decoded) {
            (Some(end), Some(c)) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    let code = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{A0}',
        "copy" => '\u{A9}',
        "reg" => '\u{AE}',
        "trade" => '\u{2122}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "hellip" => '\u{2026}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201C}',
        "rdquo" => '\u{201D}',
        "laquo" => '\u{AB}',
        "raquo" => '\u{BB}',
        "bull" => '\u{2022}',
        "eacute" => '\u{E9}',
        _ if name.starts_with("#x") || name.starts_with("#X") => {
            return u32::from_str_radix(&name[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        _ if name.starts_with('#') => {
            return name[1..].parse::<u32>().ok().and_then(char::from_u32)
        }
        _ => return None,
    };
    Some(code)
}

/// gives the bytes of a picture html names by file or address.
pub type PictureLoader = dyn Fn(&str) -> Option<Vec<u8>>;

/// tags whose content is never shown.
const DROPPED: &[&str] = &[
    "script", "style", "head", "title", "template", "noscript", "iframe", "object", "svg", "math",
];

/// one open table while building.
struct TableBuild {
    table: Table,
    row: Option<TableRow>,
    cell: Option<TableCell>,
}

struct HtmlBuilder<'a> {
    /// fetches pictures the html only names (a file or an address)
    loader: Option<&'a PictureLoader>,
    document: Document,
    blocks_stack: Vec<Vec<Block>>,
    paragraph: Option<(Paragraph, Option<u8>)>,
    marks: Vec<(String, Mark)>,
    lists: Vec<String>,
    tables: Vec<TableBuild>,
    preformatted: usize,
    quote_depth: i32,
    dropped: usize,
}

/// a document from pasted html (see the module notes for what is kept).
pub fn from_html(html: &str) -> Document {
    from_html_with_images(html, None)
}

/// like rom_html, with loader asked for the bytes of pictures named
/// by file or address (clipboard html often points at a temporary file).
pub fn from_html_with_images(html: &str, loader: Option<&PictureLoader>) -> Document {
    let mut document = Document::new();
    document.blocks.clear();
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("html-bullets", false));
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("html-numbers", true));
    let mut builder = HtmlBuilder {
        loader,
        document,
        blocks_stack: vec![Vec::new()],
        paragraph: None,
        marks: Vec::new(),
        lists: Vec::new(),
        tables: Vec::new(),
        preformatted: 0,
        quote_depth: 0,
        dropped: 0,
    };
    for token in tokenize(html) {
        builder.token(token);
    }
    builder.finish()
}

impl HtmlBuilder<'_> {
    fn token(&mut self, token: Token) {
        match token {
            Token::Open {
                name,
                attributes,
                closed,
            } => {
                if DROPPED.contains(&name.as_str()) {
                    if !closed {
                        self.dropped += 1;
                    }
                    return;
                }
                if self.dropped == 0 {
                    self.open(&name, &attributes);
                    if closed {
                        self.close(&name);
                    }
                }
            }
            Token::Close(name) => {
                if DROPPED.contains(&name.as_str()) {
                    self.dropped = self.dropped.saturating_sub(1);
                } else if self.dropped == 0 {
                    self.close(&name);
                }
            }
            Token::Text(text) if self.dropped == 0 => self.text(&text),
            Token::Text(_) => {}
        }
    }

    fn open(&mut self, name: &str, attributes: &HashMap<String, String>) {
        match name {
            "p" | "div" | "blockquote" | "pre" | "li" | "dt" | "dd" => {
                self.end_paragraph();
                if name == "blockquote" {
                    self.quote_depth += 1;
                }
                if name == "pre" {
                    self.preformatted += 1;
                }
                self.start_paragraph(None);
                if name == "li" {
                    self.set_list_item();
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.end_paragraph();
                let level = name[1..].parse().unwrap_or(1);
                self.start_paragraph(Some(level));
            }
            "br" => self.push_inline(Inline::Break {
                kind: BreakKind::Line,
                marks: Vec::new(),
            }),
            "ul" | "ol" => {
                self.end_paragraph();
                let definition = if name == "ol" {
                    "html-numbers"
                } else {
                    "html-bullets"
                };
                let instance = match self.lists.last() {
                    Some(outer) => outer.clone(),
                    None => self.document.numbering.restart(definition),
                };
                self.lists.push(instance);
            }
            "table" => {
                self.end_paragraph();
                self.tables.push(TableBuild {
                    table: Table::default(),
                    row: None,
                    cell: None,
                });
            }
            "tr" => {
                self.finish_cell();
                if let Some(build) = self.tables.last_mut() {
                    if let Some(row) = build.row.take() {
                        build.table.rows.push(row);
                    }
                    build.row = Some(TableRow::default());
                }
            }
            "td" | "th" => self.start_cell(name == "th", attributes),
            "img" => self.image(attributes),
            _ => {
                if let Some(mark) = mark_for(name, attributes) {
                    self.marks.push((name.to_string(), mark));
                }
            }
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "p" | "div" | "li" | "dt" | "dd" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.end_paragraph()
            }
            "blockquote" => {
                self.end_paragraph();
                self.quote_depth = (self.quote_depth - 1).max(0);
            }
            "pre" => {
                self.end_paragraph();
                self.preformatted = self.preformatted.saturating_sub(1);
            }
            "ul" | "ol" => {
                self.end_paragraph();
                self.lists.pop();
            }
            "td" | "th" => self.finish_cell(),
            "tr" => {
                self.finish_cell();
                if let Some(build) = self.tables.last_mut() {
                    if let Some(row) = build.row.take() {
                        build.table.rows.push(row);
                    }
                }
            }
            "table" => self.finish_table(),
            _ => {
                if let Some(position) = self.marks.iter().rposition(|(tag, _)| tag == name) {
                    self.marks.remove(position);
                }
            }
        }
    }

    fn text(&mut self, text: &str) {
        let text = if self.preformatted > 0 {
            text.to_string()
        } else {
            collapse_whitespace(text)
        };
        if text.trim().is_empty() && self.paragraph.is_none() {
            return;
        }
        if self.preformatted > 0 {
            for (index, line) in text.split('\n').enumerate() {
                if index > 0 {
                    self.push_inline(Inline::Break {
                        kind: BreakKind::Line,
                        marks: Vec::new(),
                    });
                }
                if !line.is_empty() {
                    self.push_text(line);
                }
            }
        } else {
            self.push_text(&text);
        }
    }

    fn push_text(&mut self, text: &str) {
        let mut marks: Vec<Mark> = self.marks.iter().map(|(_, mark)| mark.clone()).collect();
        if self.preformatted > 0 {
            marks.push(Mark::FontFamily {
                name: "Courier New".into(),
                theme: None,
                extra: Vec::new(),
            });
        }
        marks.dedup();
        normalize_marks(&mut marks);
        self.push_inline(Inline::Text {
            text: text.to_string(),
            marks,
        });
    }

    fn push_inline(&mut self, inline: Inline) {
        if self.paragraph.is_none() {
            self.start_paragraph(None);
            if !self.lists.is_empty() {
                self.set_list_item();
            }
        }
        if let Some((paragraph, _)) = self.paragraph.as_mut() {
            // a space at the very start of a paragraph is html indentation
            if let (true, Inline::Text { text, .. }) = (paragraph.content.is_empty(), &inline) {
                if text.trim().is_empty() {
                    return;
                }
            }
            paragraph.content.push(inline);
        }
    }

    fn start_paragraph(&mut self, heading: Option<u8>) {
        let mut paragraph = Paragraph::default();
        if self.quote_depth > 0 {
            paragraph.props.indent.left = Some(720 * self.quote_depth);
        }
        self.paragraph = Some((paragraph, heading));
    }

    fn set_list_item(&mut self) {
        let level = self.lists.len().saturating_sub(1).min(8) as u8;
        let Some(instance) = self.lists.last().cloned() else {
            return;
        };
        // nested lists of another kind keep the outer instance; the level
        // decides the label
        if let Some((paragraph, _)) = self.paragraph.as_mut() {
            paragraph.props.list = Some(ListRef {
                list_id: instance,
                level,
            });
        }
    }

    fn end_paragraph(&mut self) {
        let Some((mut paragraph, heading)) = self.paragraph.take() else {
            return;
        };
        if let Some(Inline::Text { text, .. }) = paragraph.content.last_mut() {
            let trimmed = text.trim_end().len();
            text.truncate(trimmed);
        }
        paragraph.content = merge_adjacent_text(paragraph.content);
        if paragraph.content.is_empty() && heading.is_none() && paragraph.props.list.is_none() {
            return;
        }
        let block = match heading {
            Some(level) => {
                let mut heading = Heading { level, paragraph };
                heading.paragraph.style_id =
                    Some(crate::outline::heading_style(&mut self.document, level));
                Block::Heading(heading)
            }
            None => Block::Paragraph(paragraph),
        };
        self.current_blocks().push(block);
    }

    fn current_blocks(&mut self) -> &mut Vec<Block> {
        if let Some(cell) = self.tables.last_mut().and_then(|build| build.cell.as_mut()) {
            return &mut cell.blocks;
        }
        self.blocks_stack
            .last_mut()
            .expect("there is always a block list")
    }

    fn start_cell(&mut self, header: bool, attributes: &HashMap<String, String>) {
        self.finish_cell();
        let Some(build) = self.tables.last_mut() else {
            return;
        };
        let row = build.row.get_or_insert_with(TableRow::default);
        row.header |= header;
        let span = |name: &str| {
            attributes
                .get(name)
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(1)
                .clamp(1, 100)
        };
        build.cell = Some(TableCell {
            col_span: span("colspan"),
            row_span: span("rowspan"),
            ..Default::default()
        });
    }

    fn finish_cell(&mut self) {
        self.end_paragraph();
        if let Some(build) = self.tables.last_mut() {
            if let Some(mut cell) = build.cell.take() {
                if cell.blocks.is_empty() {
                    cell.blocks.push(Block::Paragraph(Paragraph::default()));
                }
                build
                    .row
                    .get_or_insert_with(TableRow::default)
                    .cells
                    .push(cell);
            }
        }
    }

    fn finish_table(&mut self) {
        self.finish_cell();
        let Some(mut build) = self.tables.pop() else {
            return;
        };
        if let Some(row) = build.row.take() {
            build.table.rows.push(row);
        }
        if !build.table.rows.is_empty() {
            self.current_blocks().push(Block::Table(build.table));
        }
    }

    /// only pictures carried in the html itself (data addresses) can be
    /// kept; others would have to be downloaded, which pasting never does.
    /// pictures given as data in the html, or, when the caller supplied a
    /// loader, files and addresses it can fetch. nothing is downloaded here.
    fn image(&mut self, attributes: &HashMap<String, String>) {
        let Some(source) = attributes.get("src") else {
            return;
        };
        let bytes = match source
            .strip_prefix("data:")
            .and_then(|rest| rest.split_once(','))
        {
            Some((header, data)) if header.starts_with("image/") && header.ends_with(";base64") => {
                crate::base64::decode(data)
            }
            Some(_) => None,
            None => self.loader.and_then(|load| load(source)),
        };
        let Some(bytes) = bytes else {
            return;
        };
        let size = |name: &str| {
            attributes
                .get(name)
                .and_then(|v| v.trim_end_matches("px").parse::<i32>().ok())
                .map(|px| px * 15)
        };
        let alt = attributes.get("alt").cloned();
        let Ok(pasted) = crate::clipboard::add_image(&mut self.document, &bytes, alt) else {
            return;
        };
        let mut inline = pasted.inline;
        if let (Inline::Image(image), Some(width), Some(height)) =
            (&mut inline, size("width"), size("height"))
        {
            image.width = width;
            image.height = height;
        }
        self.push_inline(inline);
    }
    fn finish(mut self) -> Document {
        self.end_paragraph();
        while !self.tables.is_empty() {
            self.finish_table();
        }
        let mut blocks = self.blocks_stack.pop().unwrap_or_default();
        if blocks.is_empty() {
            blocks.push(Block::Paragraph(Paragraph::default()));
        }
        self.document.blocks = blocks;
        self.document
    }
}

fn mark_for(name: &str, attributes: &HashMap<String, String>) -> Option<Mark> {
    Some(match name {
        "b" | "strong" => Mark::Bold,
        "i" | "em" | "cite" | "var" => Mark::Italic,
        "u" | "ins" => Mark::Underline {
            style: "single".into(),
        },
        "s" | "strike" | "del" => Mark::Strike,
        "sub" => Mark::Subscript,
        "sup" => Mark::Superscript,
        "code" | "kbd" | "samp" | "tt" => Mark::FontFamily {
            name: "Courier New".into(),
            theme: None,
            extra: Vec::new(),
        },
        "a" => {
            let href = attributes.get("href")?;
            if !crate::export::safe_href(href) {
                return None;
            }
            match href.strip_prefix('#') {
                Some(anchor) => Mark::Link(LinkTarget {
                    anchor: Some(anchor.into()),
                    ..Default::default()
                }),
                None => Mark::Link(LinkTarget {
                    href: Some(href.trim().into()),
                    ..Default::default()
                }),
            }
        }
        _ => return None,
    })
}

/// html collapses runs of white space into one space.
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() && c != '\u{A0}' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_survives_nonsense() {
        for html in [
            "<",
            "<<>>",
            "<p",
            "</",
            "<a href='x",
            "&",
            "&#xZZ;",
            "<!--",
            "<p>a</b>c</p>",
            "<1>",
            "\u{0}",
        ] {
            let _ = from_html(html);
        }
        assert_eq!(decode_entities("a &amp; b &nope; &#65;"), "a & b &nope; A");
    }
}
