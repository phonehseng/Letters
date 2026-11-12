//! strict word mode: what in a document microsoft word (or libreoffice)
//! would not keep or would show differently, with an automatic fix where
//! one exists. the table of editor features the interface uses to decide
//! what to offer in that mode is in `features`.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::model::{Block, Document, Inline, Mark, NumberFormat, SourceFormat, TextBox};

mod drawings;
mod features;

pub use features::{feature_matrix, Feature, Support};

/// where in the document a finding is: a top-level block, and the inline
/// inside it when that matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub block: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline: Option<usize>,
}

/// what an automatic fix does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Fix {
    /// replace xml word cannot read with its text
    KeepTextOnly,
    /// use the nearest of word's highlighter colours
    NearestHighlight { color: String },
    /// numbers instead of a numbering style word does not have
    DecimalNumbers { definition: String },
    /// use a font every copy of word has
    ReplaceFont { from: String, to: String },
    /// move a text box's content out of the box it sits in
    DissolveInnerTextBox,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// a stable name for the kind of finding
    pub code: &'static str,
    /// one plain sentence for the person
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub issues: Vec<Issue>,
}

impl Report {
    /// strict word mode holds while this is true.
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

/// fonts that come with windows or office, so word shows them anywhere
/// without embedding.
const STANDARD_FONTS: &[&str] = &[
    "Aptos",
    "Aptos Display",
    "Arial",
    "Arial Black",
    "Arial Narrow",
    "Book Antiqua",
    "Bookman Old Style",
    "Calibri",
    "Calibri Light",
    "Cambria",
    "Cambria Math",
    "Candara",
    "Century",
    "Century Gothic",
    "Comic Sans MS",
    "Consolas",
    "Constantia",
    "Corbel",
    "Courier New",
    "Franklin Gothic Medium",
    "Garamond",
    "Georgia",
    "Gill Sans MT",
    "Impact",
    "Lucida Console",
    "Lucida Sans Unicode",
    "Microsoft YaHei",
    "MS Gothic",
    "MS Mincho",
    "Palatino Linotype",
    "Segoe Print",
    "Segoe Script",
    "Segoe UI",
    "SimSun",
    "Sylfaen",
    "Symbol",
    "Tahoma",
    "Times New Roman",
    "Trebuchet MS",
    "Verdana",
    "Wingdings",
];

/// word's highlighter colours and their rgb values.
const HIGHLIGHTS: &[(&str, (u8, u8, u8))] = &[
    ("yellow", (255, 255, 0)),
    ("green", (0, 255, 0)),
    ("cyan", (0, 255, 255)),
    ("magenta", (255, 0, 255)),
    ("blue", (0, 0, 255)),
    ("red", (255, 0, 0)),
    ("darkBlue", (0, 0, 128)),
    ("darkCyan", (0, 128, 128)),
    ("darkGreen", (0, 128, 0)),
    ("darkMagenta", (128, 0, 128)),
    ("darkRed", (128, 0, 0)),
    ("darkYellow", (128, 128, 0)),
    ("darkGray", (128, 128, 128)),
    ("lightGray", (192, 192, 192)),
    ("black", (0, 0, 0)),
    ("white", (255, 255, 255)),
];

/// a highlight value word accepts: its own colour names pass through, a
/// hex colour becomes the nearest name.
pub fn word_highlight(color: &str) -> &'static str {
    if let Some((name, _)) = HIGHLIGHTS.iter().find(|(name, _)| *name == color) {
        return name;
    }
    if color == "none" {
        return "none";
    }
    let channel =
        |at: usize| u8::from_str_radix(color.get(at..at + 2).unwrap_or("ff"), 16).unwrap_or(255);
    let (r, g, b) = (
        i32::from(channel(0)),
        i32::from(channel(2)),
        i32::from(channel(4)),
    );
    HIGHLIGHTS
        .iter()
        .min_by_key(|(_, (hr, hg, hb))| {
            let (dr, dg, db) = (r - i32::from(*hr), g - i32::from(*hg), b - i32::from(*hb));
            dr * dr + dg * dg + db * db
        })
        .map(|(name, _)| *name)
        .unwrap_or("yellow")
}

/// number format names word knows besides the ones the model names.
const WORD_NUMBER_FORMATS: &[&str] = &[
    "ordinal",
    "cardinalText",
    "ordinalText",
    "hex",
    "chicago",
    "decimalEnclosedCircle",
    "decimalFullWidth",
    "decimalHalfWidth",
    "aiueo",
    "iroha",
    "chineseCounting",
    "japaneseCounting",
    "koreanCounting",
    "hebrew1",
    "arabicAlpha",
    "hindiNumbers",
    "thaiNumbers",
    "russianLower",
    "russianUpper",
    "numberInDash",
    "ideographDigital",
];

/// everything word would not keep, or would show differently.
pub fn word_compatibility(document: &Document) -> Report {
    let mut report = Report::default();
    let foreign = document.carry.origin == Some(SourceFormat::Odt);
    for (index, block) in document.blocks.iter().enumerate() {
        check_block(block, index, foreign, false, &mut report);
    }
    check_numbering(document, &mut report);
    check_fonts(document, &mut report);
    drawings::check(document, drawings::Target::Word, &mut report);
    for item in &document.media {
        if item.content_type == "image/webp" || item.content_type == "image/svg+xml" {
            report.issues.push(Issue {
                code: "picture-format",
                message: format!(
                    "The picture “{}” is {}; Word 2019 and older can’t show it.",
                    item.id,
                    if item.content_type == "image/webp" {
                        "WebP"
                    } else {
                        "SVG"
                    }
                ),
                location: None,
                fix: None,
            });
        }
    }
    report
}

/// everything libreoffice would not keep from a document, or would show
/// differently: word-only pieces (kept as word xml, so only their text
/// reaches an odt file), fonts that are not there, and the drawings and
/// layout an opendocument file holds less exactly (see `drawings`).
pub fn libreoffice_compatibility(document: &Document) -> Report {
    let mut report = Report::default();
    if document.carry.origin == Some(SourceFormat::Docx) {
        for (index, block) in document.blocks.iter().enumerate() {
            check_word_only(block, index, &mut report);
        }
    }
    check_fonts(document, &mut report);
    drawings::check(document, drawings::Target::LibreOffice, &mut report);
    report
}

fn check_word_only(block: &Block, index: usize, report: &mut Report) {
    let at = |inline: Option<usize>| {
        Some(Location {
            block: index,
            inline,
        })
    };
    if let Block::Preserved { .. } = block {
        report.issues.push(Issue {
            code: "word-only",
            message: "This part came from a Word file and OpenDocument has nothing like it; only its text can be kept.".into(),
            location: at(None),
            fix: Some(Fix::KeepTextOnly),
        });
        return;
    }
    let Some(paragraph) = block.paragraph() else {
        return;
    };
    for (position, inline) in paragraph.content.iter().enumerate() {
        if matches!(
            inline,
            Inline::Preserved { .. } | Inline::PreservedRun { .. }
        ) {
            report.issues.push(Issue {
                code: "word-only",
                message: "This object came from a Word file and OpenDocument has nothing like it; only its text can be kept.".into(),
                location: at(Some(position)),
                fix: Some(Fix::KeepTextOnly),
            });
        }
    }
}

fn check_block(block: &Block, index: usize, foreign: bool, in_box: bool, report: &mut Report) {
    let at = |inline: Option<usize>| {
        Some(Location {
            block: index,
            inline,
        })
    };
    match block {
        Block::Preserved { .. } if foreign => report.issues.push(Issue {
            code: "odt-only",
            message: "This part came from an OpenDocument file and Word has nothing like it; only its text can be kept.".into(),
            location: at(None),
            fix: Some(Fix::KeepTextOnly),
        }),
        Block::TextBox(text_box) => {
            if in_box {
                report.issues.push(Issue {
                    code: "nested-text-box",
                    message: "Word can’t put a text box inside another text box.".into(),
                    location: at(None),
                    fix: Some(Fix::DissolveInnerTextBox),
                });
            }
            check_box_position(text_box, index, report);
            for inner in &text_box.blocks {
                check_block(inner, index, foreign, true, report);
            }
        }
        Block::Table(table) => {
            for cell in table.rows.iter().flat_map(|row| row.cells.iter()) {
                for inner in &cell.blocks {
                    check_block(inner, index, foreign, in_box, report);
                }
            }
        }
        _ => {
            let Some(paragraph) = block.paragraph() else { return };
            for (position, inline) in paragraph.content.iter().enumerate() {
                if foreign && matches!(inline, Inline::Preserved { .. }) {
                    report.issues.push(Issue {
                        code: "odt-only",
                        message: "This object came from an OpenDocument file and Word has nothing like it; only its text can be kept.".into(),
                        location: at(Some(position)),
                        fix: Some(Fix::KeepTextOnly),
                    });
                }
                for mark in inline.marks() {
                    if let Mark::Highlight { color } = mark {
                        let nearest = word_highlight(color);
                        if nearest != color {
                            report.issues.push(Issue {
                                code: "highlight-colour",
                                message: format!("Word’s highlighter has 16 colours; this one becomes “{nearest}”."),
                                location: at(Some(position)),
                                fix: Some(Fix::NearestHighlight { color: nearest.into() }),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn check_box_position(text_box: &TextBox, index: usize, report: &mut Report) {
    let horizontal_ok = matches!(
        text_box.horizontal.relative_to.as_str(),
        "page" | "margin" | "column" | "character" | "leftMargin" | "rightMargin"
    );
    let vertical_ok = matches!(
        text_box.vertical.relative_to.as_str(),
        "page" | "margin" | "paragraph" | "line" | "topMargin" | "bottomMargin"
    );
    if !(horizontal_ok && vertical_ok) {
        report.issues.push(Issue {
            code: "position-anchor",
            message: "Word measures this text box’s position from a slightly different place, so it may move a little.".into(),
            location: Some(Location { block: index, inline: None }),
            fix: None,
        });
    }
}

fn check_numbering(document: &Document, report: &mut Report) {
    for definition in &document.numbering.definitions {
        for level in &definition.levels {
            if let NumberFormat::Other(name) = &level.format {
                if !WORD_NUMBER_FORMATS.contains(&name.as_str()) {
                    report.issues.push(Issue {
                        code: "number-format",
                        message: format!("Word doesn’t have the list numbering “{name}”; it would show plain numbers."),
                        location: None,
                        fix: Some(Fix::DecimalNumbers { definition: definition.id.clone() }),
                    });
                    break;
                }
            }
        }
    }
}

/// every font the document names, in styles and in its text.
pub fn fonts_used(document: &Document) -> BTreeSet<String> {
    let mut fonts = BTreeSet::new();
    let mut add_marks = |marks: &[Mark]| {
        for mark in marks {
            if let Mark::FontFamily {
                name, theme: None, ..
            } = mark
            {
                fonts.insert(name.clone());
            }
        }
    };
    visit_inlines(&document.blocks, &mut |inline| add_marks(inline.marks()));
    for style in &document.styles.styles {
        if let Some(font) = style.run.font.as_ref().filter(|f| f.theme.is_none()) {
            fonts.insert(font.name.clone());
        }
    }
    if let Some(font) = document
        .styles
        .defaults
        .run
        .font
        .as_ref()
        .filter(|f| f.theme.is_none())
    {
        fonts.insert(font.name.clone());
    }
    fonts
}

fn visit_inlines(blocks: &[Block], visit: &mut dyn FnMut(&Inline)) {
    for block in blocks {
        match block {
            Block::Table(table) => {
                for cell in table.rows.iter().flat_map(|row| row.cells.iter()) {
                    visit_inlines(&cell.blocks, visit);
                }
            }
            Block::TextBox(text_box) => visit_inlines(&text_box.blocks, visit),
            _ => {
                if let Some(paragraph) = block.paragraph() {
                    paragraph.content.iter().for_each(&mut *visit);
                }
            }
        }
    }
}

/// a standard font close in kind to `name`.
pub fn nearest_standard_font(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.contains("mono") || lower.contains("courier") || lower.contains("code") {
        "Courier New"
    } else if lower.contains("sans")
        || lower.contains("helvetica")
        || lower.contains("arial")
        || lower.contains("dejavu")
    {
        "Arial"
    } else {
        "Times New Roman"
    }
}

fn check_fonts(document: &Document, report: &mut Report) {
    let embedded = document
        .carry
        .parts
        .iter()
        .any(|part| part.name.starts_with("word/fonts/"));
    if embedded {
        return;
    }
    for font in fonts_used(document) {
        if !STANDARD_FONTS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(&font))
        {
            let to = nearest_standard_font(&font);
            report.issues.push(Issue {
                code: "font-not-embedded",
                message: format!("“{font}” isn’t embedded and doesn’t come with Word, so Word may show “{to}” or another font instead."),
                location: None,
                fix: Some(Fix::ReplaceFont { from: font.clone(), to: to.into() }),
            });
        }
    }
}

/// applies every automatic fix the report offers and returns how many
/// were applied.
pub fn apply_fixes(document: &mut Document, report: &Report) -> usize {
    let mut applied = 0;
    // fixes that change block positions go last, from the end backwards
    let mut dissolve: Vec<usize> = Vec::new();
    for issue in &report.issues {
        let Some(fix) = &issue.fix else { continue };
        let done = match fix {
            Fix::KeepTextOnly => keep_text_only(document, issue.location),
            Fix::NearestHighlight { color } => {
                let Some(location) = issue.location else {
                    continue;
                };
                set_highlights(&mut document.blocks[location.block], color);
                true
            }
            Fix::DecimalNumbers { definition } => {
                if let Some(definition) = document
                    .numbering
                    .definitions
                    .iter_mut()
                    .find(|d| d.id == *definition)
                {
                    for level in &mut definition.levels {
                        if matches!(level.format, NumberFormat::Other(_)) {
                            level.format = NumberFormat::Decimal;
                        }
                    }
                }
                true
            }
            Fix::ReplaceFont { from, to } => {
                replace_font(document, from, to);
                true
            }
            Fix::DissolveInnerTextBox => {
                if let Some(location) = issue.location {
                    dissolve.push(location.block);
                }
                true
            }
        };
        applied += usize::from(done);
    }
    dissolve.sort_unstable();
    dissolve.dedup();
    for block in dissolve.into_iter().rev() {
        if let Some(Block::TextBox(outer)) = document.blocks.get_mut(block) {
            let mut flat = Vec::new();
            for inner in outer.blocks.drain(..) {
                match inner {
                    Block::TextBox(nested) => flat.extend(nested.blocks),
                    other => flat.push(other),
                }
            }
            outer.blocks = flat;
        }
    }
    applied
}

fn keep_text_only(document: &mut Document, location: Option<Location>) -> bool {
    let Some(location) = location else {
        return false;
    };
    let Some(block) = document.blocks.get_mut(location.block) else {
        return false;
    };
    let text_of = |xml: &str| {
        crate::xml::parse(xml)
            .map(|d| d.root.deep_text())
            .unwrap_or_default()
    };
    match (location.inline, &mut *block) {
        (None, Block::Preserved { xml }) => {
            let text = text_of(xml);
            *block = Block::text(&text);
            true
        }
        (Some(index), _) => {
            let Some(paragraph) = block.paragraph_mut() else {
                return false;
            };
            if let Some(Inline::Preserved { xml }) = paragraph.content.get(index) {
                let text = text_of(xml);
                paragraph.content[index] = Inline::text(&text);
                return true;
            }
            false
        }
        _ => false,
    }
}

fn set_highlights(block: &mut Block, color: &str) {
    let Some(paragraph) = block.paragraph_mut() else {
        return;
    };
    for inline in &mut paragraph.content {
        if let Some(marks) = inline.marks_mut() {
            for mark in marks.iter_mut() {
                if let Mark::Highlight { color: existing } = mark {
                    if word_highlight(existing) == color {
                        *existing = color.to_string();
                    }
                }
            }
        }
    }
}

fn replace_font(document: &mut Document, from: &str, to: &str) {
    fn blocks(list: &mut [Block], from: &str, to: &str) {
        for block in list {
            match block {
                Block::Table(table) => {
                    for cell in table.rows.iter_mut().flat_map(|row| row.cells.iter_mut()) {
                        blocks(&mut cell.blocks, from, to);
                    }
                }
                Block::TextBox(text_box) => blocks(&mut text_box.blocks, from, to),
                _ => {
                    let Some(paragraph) = block.paragraph_mut() else {
                        continue;
                    };
                    for inline in &mut paragraph.content {
                        for mark in inline.marks_mut().into_iter().flatten() {
                            if let Mark::FontFamily { name, .. } = mark {
                                if name == from {
                                    *name = to.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    blocks(&mut document.blocks, from, to);
    for font in document
        .styles
        .styles
        .iter_mut()
        .filter_map(|style| style.run.font.as_mut())
        .chain(document.styles.defaults.run.font.as_mut())
    {
        if font.name == from {
            font.name = to.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Paragraph;

    #[test]
    fn highlights_snap_to_words_colours() {
        assert_eq!(word_highlight("yellow"), "yellow");
        assert_eq!(word_highlight("FFFF33"), "yellow");
        assert_eq!(word_highlight("102080"), "darkBlue");
    }

    #[test]
    fn a_report_with_fixes() {
        let mut document = Document::new();
        document.carry.origin = Some(SourceFormat::Odt);
        document.blocks = vec![
            Block::Paragraph(Paragraph {
                content: vec![Inline::styled(
                    "marked",
                    vec![
                        Mark::FontFamily {
                            name: "Liberation Serif".into(),
                            theme: None,
                            extra: vec![],
                        },
                        Mark::Highlight {
                            color: "FFFF33".into(),
                        },
                    ],
                )],
                ..Default::default()
            }),
            Block::Preserved {
                xml: "<text:section><text:p>kept text</text:p></text:section>".into(),
            },
            Block::TextBox(TextBox::new(
                0,
                0,
                100,
                100,
                vec![Block::TextBox(TextBox::new(
                    0,
                    0,
                    50,
                    50,
                    vec![Block::text("inner")],
                ))],
            )),
        ];
        let report = word_compatibility(&document);
        let codes: Vec<&str> = report.issues.iter().map(|i| i.code).collect();
        assert!(codes.contains(&"highlight-colour") && codes.contains(&"odt-only"));
        assert!(codes.contains(&"font-not-embedded") && codes.contains(&"nested-text-box"));
        assert_eq!(apply_fixes(&mut document, &report), report.issues.len());
        document.carry.origin = None;
        assert!(
            word_compatibility(&document).is_clean(),
            "{:?}",
            word_compatibility(&document)
        );
        assert_eq!(
            document.blocks[1].paragraph().unwrap().plain_text(),
            "kept text"
        );
    }

    #[test]
    fn the_matrix_names_each_feature_once() {
        let matrix = feature_matrix();
        let keys: BTreeSet<&str> = matrix.iter().map(|f| f.key).collect();
        assert_eq!(keys.len(), matrix.len());
        assert!(matrix.iter().any(|f| !f.word_exact()));
    }
}
