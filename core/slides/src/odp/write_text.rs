//! a text body written as opendocument paragraphs, lists and spans, with
//! the effective formatting (inheritance resolved) in automatic styles so
//! the text looks the same wherever the deck came from.

use super::units::{write_length, write_points};
use super::write_styles::StyleRegistry;
use crate::color::{resolve, ColorContext};
use crate::inherit::{effective_paragraph, effective_run, resolve_typeface, Chain};
use crate::model::{
    Bullet, BulletFont, BulletSize, FieldKind, Fill, Object, ParagraphProps, RunProps, Spacing,
    TextAlign, TextBody, TextParagraph, TextRun,
};
use crate::xml::{escape_attribute, escape_text};

/// the size text has when nothing sets one (as in `render::text`)
const DEFAULT_SIZE: i32 = 1_800;

pub struct TextWriter<'a, 'b> {
    pub chain: &'a Chain<'a>,
    pub colors: &'a ColorContext<'a>,
    pub styles: &'b mut StyleRegistry,
}

impl TextWriter<'_, '_> {
    /// the paragraphs of a body, for inside a `draw:text-box` or a shape.
    pub fn paragraphs(&mut self, object: &Object, body: &TextBody) -> String {
        let mut out = String::new();
        let mut previous_list: Option<String> = None;
        for paragraph in &body.paragraphs {
            let effective = effective_paragraph(self.chain, object, body, paragraph);
            let paragraph_xml = self.paragraph(&effective, paragraph);
            match self.list_style(&effective) {
                Some(list) => {
                    let continues = previous_list.as_deref() == Some(list.as_str());
                    out.push_str(&nested_list(
                        &list,
                        effective.level.unwrap_or(0),
                        continues,
                        &paragraph_xml,
                    ));
                    previous_list = Some(list);
                }
                None => {
                    out.push_str(&paragraph_xml);
                    previous_list = None;
                }
            }
        }
        out
    }

    fn paragraph(&mut self, effective: &ParagraphProps, paragraph: &TextParagraph) -> String {
        let style = self
            .styles
            .style("paragraph", None, &paragraph_properties(effective));
        let mut out = format!("<text:p text:style-name=\"{style}\">");
        for run in &paragraph.runs {
            let props = effective_run(effective, run.props());
            out.push_str(&self.run(run, &props));
        }
        out.push_str("</text:p>");
        out
    }

    fn run(&mut self, run: &TextRun, props: &RunProps) -> String {
        let style = self
            .styles
            .style("text", None, &self.text_properties(props));
        let content = match run {
            TextRun::Text { text, .. } | TextRun::Preserved { text, .. } => text_content(text),
            TextRun::Break { .. } => return "<text:line-break/>".into(),
            TextRun::Field {
                kind: FieldKind::SlideNumber,
                text,
                ..
            } => {
                format!("<text:page-number>{}</text:page-number>", escape_text(text))
            }
            TextRun::Field {
                kind: FieldKind::DateTime(_),
                text,
                ..
            } => format!("<text:date>{}</text:date>", escape_text(text)),
            TextRun::Field { text, .. } => text_content(text),
        };
        format!("<text:span text:style-name=\"{style}\">{content}</text:span>")
    }

    /// `style:text-properties` for effective run properties.
    pub fn text_properties(&self, props: &RunProps) -> String {
        let mut attributes = String::new();
        let mut add = |name: &str, value: &str| {
            attributes.push_str(&format!(" {name}=\"{}\"", escape_attribute(value)))
        };
        let size = props.size.unwrap_or(DEFAULT_SIZE);
        add("fo:font-size", &write_points(size));
        let face = |typeface: &Option<crate::model::Typeface>, default: &str| {
            resolve_typeface(
                self.chain.theme,
                typeface.as_ref().map_or(default, |t| t.typeface.as_str()),
            )
        };
        add("fo:font-family", &face(&props.latin, "+mn-lt"));
        let east = face(&props.east_asian, "+mn-ea");
        if !east.is_empty() && !east.starts_with('+') {
            add("style:font-family-asian", &east);
        }
        add(
            "fo:font-weight",
            if props.bold == Some(true) {
                "bold"
            } else {
                "normal"
            },
        );
        add(
            "fo:font-style",
            if props.italic == Some(true) {
                "italic"
            } else {
                "normal"
            },
        );
        if let Some(Fill::Solid { color }) = &props.fill {
            add(
                "fo:color",
                &format!("#{}", resolve(color, self.colors).to_hex()),
            );
        }
        if props.underline.as_deref().is_some_and(|u| u != "none") {
            add("style:text-underline-style", "solid");
            add("style:text-underline-width", "auto");
            add("style:text-underline-color", "font-color");
        }
        if props.strike.as_deref().is_some_and(|s| s != "noStrike") {
            add("style:text-line-through-style", "solid");
        }
        match props.caps.as_deref() {
            Some("all") => add("fo:text-transform", "uppercase"),
            Some("small") => add("fo:font-variant", "small-caps"),
            _ => {}
        }
        if let Some(shift) = props.baseline.filter(|b| *b != 0) {
            add("style:text-position", &format!("{}% 100%", shift / 1000));
        }
        if let Some(spacing) = props.spacing.filter(|s| *s != 0) {
            add("fo:letter-spacing", &write_points(spacing));
        }
        format!("<style:text-properties{attributes}/>")
    }

    /// a list style for a paragraph's bullet or number, or `None`.
    fn list_style(&mut self, effective: &ParagraphProps) -> Option<String> {
        let level = effective.level.unwrap_or(0) + 1;
        let bullet_font = match &effective.bullet_font {
            Some(BulletFont::Font(typeface)) => format!(
                " fo:font-family=\"{}\"",
                escape_attribute(&resolve_typeface(self.chain.theme, &typeface.typeface))
            ),
            _ => String::new(),
        };
        let size = match effective.bullet_size {
            Some(BulletSize::Percent(thousandths)) => {
                format!(" fo:font-size=\"{}%\"", thousandths / 1000)
            }
            _ => " fo:font-size=\"100%\"".to_string(),
        };
        let properties = format!(
            "<style:list-level-properties text:list-level-position-and-space-mode=\"label-alignment\"><style:list-level-label-alignment text:label-followed-by=\"listtab\"/></style:list-level-properties><style:text-properties{bullet_font}{size}/>"
        );
        let level_xml = match effective.bullet.as_ref()? {
            Bullet::Char { char } => format!(
                "<text:list-level-style-bullet text:level=\"{level}\" text:bullet-char=\"{}\">{properties}</text:list-level-style-bullet>",
                escape_attribute(char)
            ),
            Bullet::AutoNumber { scheme, start_at } => format!(
                "<text:list-level-style-number text:level=\"{level}\" style:num-format=\"{}\" style:num-suffix=\"{}\" text:start-value=\"{}\">{properties}</text:list-level-style-number>",
                number_format(scheme),
                number_suffix(scheme),
                start_at.unwrap_or(1)
            ),
            Bullet::Picture { .. } => format!(
                "<text:list-level-style-bullet text:level=\"{level}\" text:bullet-char=\"•\">{properties}</text:list-level-style-bullet>"
            ),
            Bullet::None => return None,
        };
        Some(self.styles.list(&level_xml))
    }
}

/// text with runs of spaces and tabs written as odf wants them.
fn text_content(text: &str) -> String {
    let mut out = String::new();
    let mut spaces = 0;
    let flush = |out: &mut String, spaces: &mut usize| {
        match *spaces {
            0 => {}
            1 => out.push(' '),
            count => out.push_str(&format!(" <text:s text:c=\"{}\"/>", count - 1)),
        }
        *spaces = 0;
    };
    for ch in text.chars() {
        match ch {
            ' ' => spaces += 1,
            '\t' => {
                flush(&mut out, &mut spaces);
                out.push_str("<text:tab/>");
            }
            other => {
                flush(&mut out, &mut spaces);
                out.push_str(&escape_text(&other.to_string()));
            }
        }
    }
    flush(&mut out, &mut spaces);
    out
}

/// a paragraph inside as many nested lists as its level needs.
fn nested_list(list: &str, level: u8, continues: bool, paragraph: &str) -> String {
    let continuation = if continues {
        " text:continue-numbering=\"true\""
    } else {
        ""
    };
    let mut out = format!("<text:list text:style-name=\"{list}\"{continuation}><text:list-item>");
    for _ in 0..level {
        out.push_str("<text:list><text:list-item>");
    }
    out.push_str(paragraph);
    for _ in 0..level {
        out.push_str("</text:list-item></text:list>");
    }
    out.push_str("</text:list-item></text:list>");
    out
}

/// `style:paragraph-properties` for effective paragraph properties.
pub fn paragraph_properties(props: &ParagraphProps) -> String {
    let size = props
        .run
        .as_ref()
        .and_then(|r| r.size)
        .unwrap_or(DEFAULT_SIZE);
    let mut attributes = String::new();
    let mut add = |name: &str, value: String| attributes.push_str(&format!(" {name}=\"{value}\""));
    add(
        "fo:text-align",
        match props.align.unwrap_or(TextAlign::Left) {
            TextAlign::Center => "center",
            TextAlign::Right => "end",
            TextAlign::Justify
            | TextAlign::Distributed
            | TextAlign::JustifyLow
            | TextAlign::Thai => "justify",
            TextAlign::Left => "start",
        }
        .into(),
    );
    add(
        "fo:margin-left",
        write_length(props.margin_left.unwrap_or(0)),
    );
    add(
        "fo:margin-right",
        write_length(props.margin_right.unwrap_or(0)),
    );
    add("fo:text-indent", write_length(props.indent.unwrap_or(0)));
    if let Some(spacing) = props.line_spacing {
        add("fo:line-height", line_height(spacing));
    }
    add("fo:margin-top", spacing_points(props.space_before, size));
    add("fo:margin-bottom", spacing_points(props.space_after, size));
    format!("<style:paragraph-properties{attributes}/>")
}

fn line_height(spacing: Spacing) -> String {
    match spacing {
        Spacing::Percent(thousandths) => format!("{}%", thousandths / 1000),
        Spacing::Points(hundredths) => write_points(hundredths),
    }
}

/// paragraph spacing in points; a percentage is of the line (1.2 times
/// the size, as `layout` measures it)
fn spacing_points(spacing: Option<Spacing>, size: i32) -> String {
    let hundredths = match spacing {
        Some(Spacing::Points(value)) => value,
        Some(Spacing::Percent(thousandths)) => {
            let line = f64::from(size) * crate::layout::SINGLE_LINE_FACTOR;
            (line * f64::from(thousandths) / 100_000.0).round() as i32
        }
        None => 0,
    };
    write_points(hundredths)
}

fn number_format(scheme: &str) -> &'static str {
    if scheme.starts_with("alphaLc") {
        "a"
    } else if scheme.starts_with("alphaUc") {
        "A"
    } else if scheme.starts_with("romanLc") {
        "i"
    } else if scheme.starts_with("romanUc") {
        "I"
    } else {
        "1"
    }
}

fn number_suffix(scheme: &str) -> &'static str {
    if scheme.ends_with("ParenR") || scheme.ends_with("ParenBoth") {
        ")"
    } else if scheme.ends_with("Period") {
        "."
    } else {
        ""
    }
}
