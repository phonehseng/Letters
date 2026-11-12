//! one-step page and paragraph setups for the paper styles students are
//! asked for: mla 9 and apa 7.
//!
//! each preset changes the styles (so new paragraphs follow it too),
//! clears direct formatting that would fight it, sets the margins and
//! adds the header the style asks for.

use crate::indent::{set_hanging_indent, HALF_INCH};
use crate::model::{
    Alignment, Block, Document, Field, FontRef, HeaderFooter, HeaderFooterKind, Inline,
    LineSpacing, Margins, Mark, Paragraph, Style, StyleKind, TabKind, TabStop, TWIPS_PER_INCH,
};

/// what the mla heading block and running header need.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MlaDetails {
    pub student_name: String,
    pub instructor: String,
    pub course: String,
    /// as it should appear, "11 November 2026"
    pub date: String,
    pub title: String,
    /// insert the name/instructor/course/date block and title at the top
    pub insert_heading: bool,
}

impl MlaDetails {
    /// the last word of the name, which mla puts before the page number.
    pub fn last_name(&self) -> &str {
        self.student_name.split_whitespace().last().unwrap_or("")
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApaDetails {
    pub title: String,
    pub author: String,
    pub affiliation: String,
    pub course: String,
    pub instructor: String,
    pub due_date: String,
    /// professional papers carry a running head; student papers do not
    pub running_head: Option<String>,
    /// insert the title page (and a page break after it) at the top
    pub insert_title_page: bool,
}

const TIMES: &str = "Times New Roman";
const TWELVE_POINT: u32 = 24;

/// mla 9: one-inch margins, times new roman 12, double spacing
/// everywhere, half-inch first-line indents, and "Lastname 1" at the top
/// right of every page.
pub fn apply_mla(document: &mut Document, details: &MlaDetails) {
    set_one_inch_margins(document);
    let normal = normal_style(document);
    set_body_style(normal, Some(HALF_INCH));
    clear_conflicting_formatting(document);
    let header = vec![Block::Paragraph(Paragraph {
        props: right_aligned(),
        content: vec![
            Inline::text(&format!("{} ", details.last_name())),
            Inline::Field(Field::page_number()),
        ],
        ..Default::default()
    })];
    set_header(document, "mla-header", header);
    if details.insert_heading {
        let mut top: Vec<Block> = [
            &details.student_name,
            &details.instructor,
            &details.course,
            &details.date,
        ]
        .iter()
        .map(|line| Block::Paragraph(no_indent(line, None)))
        .collect();
        top.push(Block::Paragraph(no_indent(
            &details.title,
            Some(Alignment::Center),
        )));
        document.blocks.splice(0..0, top);
    }
}

/// apa 7: one-inch margins, a 12 point font, double spacing, half-inch
/// first-line indents, the page number top right (with the running head
/// top left for professional papers), apa's heading levels, a title page
/// and hanging indents for the reference list.
pub fn apply_apa(document: &mut Document, details: &ApaDetails) {
    set_one_inch_margins(document);
    let normal = normal_style(document);
    set_body_style(normal, Some(HALF_INCH));
    clear_conflicting_formatting(document);
    set_apa_headings(document);
    let header = apa_header(document, details);
    set_header(document, "apa-header", header);
    apply_reference_indents(document);
    if details.insert_title_page {
        let title_page = apa_title_page(details);
        document.blocks.splice(0..0, title_page);
    }
}

fn set_one_inch_margins(document: &mut Document) {
    document.normalize_sections();
    for section in &mut document.sections {
        section.page.margins = Margins {
            gutter: 0,
            ..Margins::ONE_INCH
        };
    }
}

/// the default paragraph style, created if the document has none.
fn normal_style(document: &mut Document) -> &mut Style {
    let index = match document
        .styles
        .styles
        .iter()
        .position(|style| style.kind == StyleKind::Paragraph && style.is_default)
    {
        Some(index) => index,
        None => {
            let mut style = Style::paragraph("Normal", "Normal");
            style.is_default = true;
            document.styles.styles.push(style);
            document.styles.styles.len() - 1
        }
    };
    &mut document.styles.styles[index]
}

fn set_body_style(style: &mut Style, first_line: Option<i32>) {
    style.run.font = Some(FontRef {
        name: TIMES.into(),
        theme: None,
        extra: Vec::new(),
    });
    style.run.size = Some(TWELVE_POINT);
    style.paragraph.spacing.line = Some(LineSpacing::DOUBLE);
    style.paragraph.spacing.before = Some(0);
    style.paragraph.spacing.after = Some(0);
    style.paragraph.indent.first_line = first_line;
    style.paragraph.indent.hanging = None;
}

/// direct fonts, sizes, spacing and indents on body text would override
/// the preset, so they go. bold, italics and the like stay.
fn clear_conflicting_formatting(document: &mut Document) {
    for block in &mut document.blocks {
        let Some(paragraph) = block.paragraph_mut() else {
            continue;
        };
        paragraph.props.spacing = Default::default();
        if paragraph.props.list.is_none() {
            paragraph.props.indent = Default::default();
        }
        for inline in &mut paragraph.content {
            if let Some(marks) = inline.marks_mut() {
                marks.retain(|mark| {
                    !matches!(mark, Mark::FontFamily { .. } | Mark::FontSize { .. })
                });
            }
        }
    }
    // heading styles follow the body font in both styles
    for style in &mut document.styles.styles {
        if style.paragraph.outline_level.is_some()
            || style.name.to_ascii_lowercase().starts_with("heading")
        {
            style.run.font = None;
            style.run.size = None;
            style.paragraph.spacing.line = None;
        }
    }
}

fn right_aligned() -> crate::model::ParagraphProps {
    crate::model::ParagraphProps {
        alignment: Some(Alignment::Right),
        indent: crate::model::Indent {
            first_line: Some(0),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// a paragraph without the first-line indent (heading block lines,
/// title page lines).
fn no_indent(text: &str, alignment: Option<Alignment>) -> Paragraph {
    let mut paragraph = Paragraph::with_text(text);
    paragraph.props.indent.first_line = Some(0);
    paragraph.props.alignment = alignment;
    paragraph
}

/// replaces the default header of every section.
fn set_header(document: &mut Document, id: &str, blocks: Vec<Block>) {
    document.headers_footers.retain(|part| part.id != id);
    document.headers_footers.push(HeaderFooter {
        id: id.into(),
        kind: HeaderFooterKind::Header,
        blocks,
    });
    for section in &mut document.sections {
        section.headers.default = Some(id.into());
        section.headers.first = None;
        section.headers.even = None;
        section.different_first_page = false;
    }
}

fn apa_header(document: &Document, details: &ApaDetails) -> Vec<Block> {
    let mut paragraph = Paragraph::default();
    paragraph.props.indent.first_line = Some(0);
    match &details.running_head {
        Some(head) => {
            // running head on the left, page number on a right tab stop
            let width = document
                .sections
                .first()
                .map(|s| s.page.text_width())
                .unwrap_or(6 * TWIPS_PER_INCH);
            paragraph.props.tabs.push(TabStop {
                position: width,
                kind: TabKind::Right,
                leader: Default::default(),
            });
            paragraph.content = vec![
                Inline::text(&head.to_uppercase()),
                Inline::Tab { marks: vec![] },
                Inline::Field(Field::page_number()),
            ];
        }
        None => {
            paragraph.props.alignment = Some(Alignment::Right);
            paragraph.content = vec![Inline::Field(Field::page_number())];
        }
    }
    vec![Block::Paragraph(paragraph)]
}

/// apa's levels: 1 centred bold, 2 flush left bold, 3 flush left bold
/// italic, all in the body font and size.
fn set_apa_headings(document: &mut Document) {
    for level in 1..=3u8 {
        let id = crate::outline::heading_style(document, level);
        let Some(style) = document
            .styles
            .styles
            .iter_mut()
            .find(|style| style.id == id)
        else {
            continue;
        };
        style.run.bold = Some(true);
        style.run.italic = Some(level == 3);
        style.run.size = None;
        style.run.font = None;
        style.run.color = None;
        style.paragraph.alignment = Some(if level == 1 {
            Alignment::Center
        } else {
            Alignment::Left
        });
        style.paragraph.indent.first_line = Some(0);
        style.paragraph.spacing.before = Some(0);
        style.paragraph.spacing.after = Some(0);
    }
}

/// every paragraph after a heading called "References" gets a half-inch
/// hanging indent, until the next heading.
pub fn apply_reference_indents(document: &mut Document) -> usize {
    let mut inside = false;
    let mut changed = 0;
    for block in &mut document.blocks {
        match block {
            Block::Heading(heading) => {
                let text = heading.paragraph.plain_text();
                inside = matches!(
                    text.trim().to_lowercase().as_str(),
                    "references" | "works cited" | "bibliography"
                );
            }
            Block::Paragraph(paragraph) if inside && !paragraph.plain_text().trim().is_empty() => {
                set_hanging_indent(paragraph, HALF_INCH);
                changed += 1;
            }
            _ => {}
        }
    }
    changed
}

fn apa_title_page(details: &ApaDetails) -> Vec<Block> {
    let mut blocks = vec![
        Block::Paragraph(no_indent("", Some(Alignment::Center))),
        Block::Paragraph(no_indent("", Some(Alignment::Center))),
        Block::Paragraph(no_indent("", Some(Alignment::Center))),
    ];
    let mut title = no_indent("", Some(Alignment::Center));
    title.content = vec![Inline::styled(&details.title, vec![Mark::Bold])];
    blocks.push(Block::Paragraph(title));
    blocks.push(Block::Paragraph(no_indent("", Some(Alignment::Center))));
    for line in [
        &details.author,
        &details.affiliation,
        &details.course,
        &details.instructor,
        &details.due_date,
    ] {
        if !line.is_empty() {
            blocks.push(Block::Paragraph(no_indent(line, Some(Alignment::Center))));
        }
    }
    blocks.push(Block::PageBreak);
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Heading;

    fn essay() -> Document {
        let mut document = Document::new();
        let mut styled = Paragraph::with_text("Body with a direct font.");
        styled.content = vec![Inline::styled(
            "Body with a direct font.",
            vec![
                Mark::Italic,
                Mark::FontFamily {
                    name: "Comic Sans MS".into(),
                    theme: None,
                    extra: vec![],
                },
            ],
        )];
        styled.props.spacing.line = Some(LineSpacing::SINGLE);
        document.blocks = vec![
            Block::Paragraph(styled),
            Block::Heading(Heading {
                level: 1,
                paragraph: Paragraph::with_text("References"),
            }),
            Block::text("Author, A. (2020). A book. Publisher."),
        ];
        document
    }

    #[test]
    fn mla_sets_everything_it_asks_for() {
        let mut document = essay();
        let details = MlaDetails {
            student_name: "Ada Lovelace".into(),
            instructor: "Prof. Babbage".into(),
            course: "ENG 101".into(),
            date: "11 November 2026".into(),
            title: "On Engines".into(),
            insert_heading: true,
        };
        apply_mla(&mut document, &details);
        let normal = document.style("Normal").unwrap();
        assert_eq!(normal.run.font.as_ref().unwrap().name, "Times New Roman");
        assert_eq!(normal.run.size, Some(24));
        assert_eq!(normal.paragraph.spacing.line, Some(LineSpacing::DOUBLE));
        assert_eq!(normal.paragraph.indent.first_line, Some(720));
        assert_eq!(document.sections[0].page.margins.left, 1440);
        let header = document.header_footer("mla-header").unwrap();
        let text = header.blocks[0].paragraph().unwrap().plain_text();
        assert_eq!(text, "Lovelace 1");
        assert_eq!(
            document.blocks[0].paragraph().unwrap().plain_text(),
            "Ada Lovelace"
        );
        assert_eq!(
            document.blocks[4].paragraph().unwrap().props.alignment,
            Some(Alignment::Center)
        );
        // the direct font went, the italics stayed
        let body = document.blocks[5].paragraph().unwrap();
        assert_eq!(body.content[0].marks(), &[Mark::Italic]);
        assert!(body.props.spacing.line.is_none());
    }

    #[test]
    fn apa_title_page_headings_and_references() {
        let mut document = essay();
        let details = ApaDetails {
            title: "On Engines".into(),
            author: "Ada Lovelace".into(),
            affiliation: "Department of Mathematics, University".into(),
            course: "MATH 101".into(),
            instructor: "Dr. Babbage".into(),
            due_date: "November 11, 2026".into(),
            running_head: Some("On engines".into()),
            insert_title_page: true,
        };
        apply_apa(&mut document, &details);
        let reference = document.blocks.last().unwrap().paragraph().unwrap();
        assert_eq!(reference.props.indent.hanging, Some(720));
        let header = document.header_footer("apa-header").unwrap().blocks[0]
            .paragraph()
            .unwrap();
        assert!(header.plain_text().starts_with("ON ENGINES\t"));
        let heading = document
            .style(&crate::outline::heading_style(&mut document.clone(), 1))
            .unwrap()
            .clone();
        assert_eq!(heading.paragraph.alignment, Some(Alignment::Center));
        assert_eq!(heading.run.bold, Some(true));
        assert!(document
            .blocks
            .iter()
            .any(|b| matches!(b, Block::PageBreak)));
        let title = document.blocks[3].paragraph().unwrap();
        assert_eq!(title.content[0].marks(), &[Mark::Bold]);
    }
}
