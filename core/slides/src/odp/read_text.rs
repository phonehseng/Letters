//! opendocument text in shapes (`draw:text-box`, or text directly in a
//! custom shape) read into the model's text body. formatting is resolved
//! through the style chains and stored on each paragraph and run, since
//! opendocument styles do not map onto drawingml's inheritance.

use super::styles::{get, Properties, StyleSheet};
use super::units::{color, font_size, length, percent};
use crate::model::{
    Anchor, Autofit, BodyProps, Bullet, BulletFont, BulletSize, ColorRef, FieldKind, Fill,
    ParagraphProps, RunProps, Spacing, TextAlign, TextBody, TextParagraph, TextRun, Typeface,
};
use crate::xml::{Element, Node};

/// what text inherits from its shape: the shape style's paragraph and
/// text properties.
pub struct TextContext<'a> {
    pub styles: &'a StyleSheet,
    pub paragraph: Properties,
    pub text: Properties,
}

pub fn read_body(graphic: &Properties) -> BodyProps {
    let inset = |name: &str| get(graphic, name).and_then(length);
    let autofit = if get(graphic, "draw:fit-to-size")
        .is_some_and(|v| v == "shrink-to-fit" || v == "true")
        || get(graphic, "style:shrink-to-fit") == Some("true")
    {
        Some(Autofit::Shrink {
            font_scale: None,
            line_reduction: None,
        })
    } else if get(graphic, "draw:auto-grow-height") == Some("true") {
        Some(Autofit::ResizeShape)
    } else {
        None
    };
    BodyProps {
        inset_left: inset("fo:padding-left"),
        inset_top: inset("fo:padding-top"),
        inset_right: inset("fo:padding-right"),
        inset_bottom: inset("fo:padding-bottom"),
        anchor: get(graphic, "draw:textarea-vertical-align").map(|a| match a {
            "middle" => Anchor::Middle,
            "bottom" => Anchor::Bottom,
            "justify" => Anchor::Justified,
            _ => Anchor::Top,
        }),
        wrap: get(graphic, "fo:wrap-option").map(|w| w != "no-wrap"),
        autofit,
        ..Default::default()
    }
}

/// the paragraphs inside `container` (a text box or a shape).
pub fn read_paragraphs(container: &Element, context: &TextContext<'_>) -> Vec<TextParagraph> {
    let mut paragraphs = Vec::new();
    collect(container, context, 0, None, &mut paragraphs);
    paragraphs
}

/// walks text:p and text:h, and text:list whose items are at `level`
/// (0 for a list directly in the box); `list` is the list style in force.
fn collect(
    element: &Element,
    context: &TextContext<'_>,
    level: u8,
    list: Option<&str>,
    out: &mut Vec<TextParagraph>,
) {
    for child in element.elements() {
        match child.name.as_str() {
            "text:p" | "text:h" => out.push(paragraph(child, context, level, list)),
            "text:list" => {
                let style = child.attr("text:style-name").or(list);
                for item in child.elements() {
                    collect_item(item, context, level, style, out);
                }
            }
            _ => {}
        }
    }
}

/// a list item's paragraphs are at the item's level; lists inside it go
/// one level deeper.
fn collect_item(
    item: &Element,
    context: &TextContext<'_>,
    level: u8,
    list: Option<&str>,
    out: &mut Vec<TextParagraph>,
) {
    for child in item.elements() {
        match child.name.as_str() {
            "text:p" | "text:h" => out.push(paragraph(child, context, level, list)),
            "text:list" => {
                let style = child.attr("text:style-name").or(list);
                for nested in child.elements() {
                    collect_item(nested, context, level + 1, style, out);
                }
            }
            _ => {}
        }
    }
}
fn paragraph(
    element: &Element,
    context: &TextContext<'_>,
    level: u8,
    list: Option<&str>,
) -> TextParagraph {
    let style = context
        .styles
        .resolve("paragraph", element.attr("text:style-name"));
    let mut paragraph_properties = context.paragraph.clone();
    paragraph_properties.extend(style.paragraph.iter().cloned());
    let mut text_properties = context.text.clone();
    text_properties.extend(style.text.iter().cloned());
    let in_list = list.is_some();
    let mut props = paragraph_props(&paragraph_properties, level, in_list);
    if let Some(list) = list {
        apply_list_level(&mut props, context.styles, list, level);
    }
    let mut runs = Vec::new();
    collect_runs(element, context, &text_properties, &mut runs);
    TextParagraph {
        props,
        end_props: Some(run_props(&text_properties)),
        runs,
        source: None,
    }
}

fn paragraph_props(properties: &Properties, level: u8, in_list: bool) -> ParagraphProps {
    let spacing = |name: &str| {
        get(properties, name)
            .and_then(length)
            .map(|emu| Spacing::Points((emu * 100 / 12_700) as i32))
    };
    ParagraphProps {
        level: in_list.then_some(level),
        align: get(properties, "fo:text-align").map(|a| match a {
            "center" => TextAlign::Center,
            "end" | "right" => TextAlign::Right,
            "justify" => TextAlign::Justify,
            _ => TextAlign::Left,
        }),
        margin_left: get(properties, "fo:margin-left").and_then(length),
        margin_right: get(properties, "fo:margin-right").and_then(length),
        indent: get(properties, "fo:text-indent").and_then(length),
        line_spacing: get(properties, "fo:line-height").and_then(|value| {
            percent(value)
                .map(Spacing::Percent)
                .or_else(|| length(value).map(|emu| Spacing::Points((emu * 100 / 12_700) as i32)))
        }),
        space_before: spacing("fo:margin-top"),
        space_after: spacing("fo:margin-bottom"),
        bullet: (!in_list).then_some(Bullet::None),
        ..Default::default()
    }
}

/// the bullet or number of a list level.
fn apply_list_level(props: &mut ParagraphProps, styles: &StyleSheet, list: &str, level: u8) {
    let Some(level_style) = styles
        .lists
        .get(list)
        .and_then(|levels| levels.get(usize::from(level)))
    else {
        return;
    };
    props.bullet = match (&level_style.bullet, &level_style.number_format) {
        (Some(char), _) => Some(Bullet::Char { char: char.clone() }),
        (None, Some(format)) => Some(Bullet::AutoNumber {
            scheme: number_scheme(format, level_style.number_suffix.as_deref()),
            start_at: None,
        }),
        _ => Some(Bullet::None),
    };
    props.bullet_font = level_style
        .bullet_font
        .as_ref()
        .map(|font| BulletFont::Font(Typeface::named(font)));
    props.bullet_size = level_style.bullet_size.map(BulletSize::Percent);
}

/// an odf number format and suffix as a drawingml auto-number scheme.
pub fn number_scheme(format: &str, suffix: Option<&str>) -> String {
    let base = match format {
        "a" => "alphaLc",
        "A" => "alphaUc",
        "i" => "romanLc",
        "I" => "romanUc",
        _ => "arabic",
    };
    let ending = match suffix {
        Some(")") => "ParenR",
        Some(".") => "Period",
        _ => "Plain",
    };
    format!("{base}{ending}")
}

fn collect_runs(
    element: &Element,
    context: &TextContext<'_>,
    inherited: &Properties,
    runs: &mut Vec<TextRun>,
) {
    for node in &element.children {
        match node {
            Node::Text(text) => push_text(runs, text, inherited),
            Node::Element(child) => match child.name.as_str() {
                "text:span" | "text:a" => {
                    let style = context
                        .styles
                        .resolve("text", child.attr("text:style-name"));
                    let mut properties = inherited.clone();
                    properties.extend(style.text.iter().cloned());
                    collect_runs(child, context, &properties, runs);
                }
                "text:s" => {
                    let count: usize = child
                        .attr("text:c")
                        .and_then(|c| c.parse().ok())
                        .unwrap_or(1);
                    push_text(runs, &" ".repeat(count), inherited);
                }
                "text:tab" => push_text(runs, "\t", inherited),
                "text:line-break" => runs.push(TextRun::Break {
                    props: run_props(inherited),
                }),
                "text:page-number" => runs.push(TextRun::Field {
                    kind: FieldKind::SlideNumber,
                    id: None,
                    text: child.deep_text(),
                    props: run_props(inherited),
                }),
                "text:date" | "presentation:date-time" => runs.push(TextRun::Field {
                    kind: FieldKind::DateTime("datetime".into()),
                    id: None,
                    text: child.deep_text(),
                    props: run_props(inherited),
                }),
                _ => push_text(runs, &child.deep_text(), inherited),
            },
        }
    }
}

/// adds text to the last run when it has the same formatting.
fn push_text(runs: &mut Vec<TextRun>, text: &str, properties: &Properties) {
    if text.is_empty() {
        return;
    }
    let props = run_props(properties);
    if let Some(TextRun::Text {
        text: last,
        props: last_props,
    }) = runs.last_mut()
    {
        if *last_props == props {
            last.push_str(text);
            return;
        }
    }
    runs.push(TextRun::Text {
        text: text.to_string(),
        props,
    });
}

pub fn run_props(properties: &Properties) -> RunProps {
    let face = |name: &str, fallback: &str| {
        get(properties, name)
            .or_else(|| get(properties, fallback))
            .map(|font| Typeface::named(font.trim_matches('\'')))
    };
    RunProps {
        size: get(properties, "fo:font-size").and_then(font_size),
        bold: get(properties, "fo:font-weight")
            .map(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600)),
        italic: get(properties, "fo:font-style").map(|s| s == "italic" || s == "oblique"),
        underline: get(properties, "style:text-underline-style").map(|u| {
            if u == "none" {
                "none".into()
            } else {
                "sng".into()
            }
        }),
        strike: get(properties, "style:text-line-through-style").map(|s| {
            if s == "none" {
                "noStrike".into()
            } else {
                "sngStrike".into()
            }
        }),
        caps: get(properties, "fo:text-transform")
            .filter(|t| *t == "uppercase")
            .map(|_| "all".to_string())
            .or_else(|| {
                get(properties, "fo:font-variant")
                    .filter(|v| *v == "small-caps")
                    .map(|_| "small".into())
            }),
        baseline: get(properties, "style:text-position").and_then(text_position),
        spacing: get(properties, "fo:letter-spacing")
            .and_then(length)
            .map(|emu| (emu * 100 / 12_700) as i32),
        fill: get(properties, "fo:color")
            .and_then(color)
            .map(|hex| Fill::Solid {
                color: ColorRef::rgb(&hex),
            }),
        latin: face("style:font-name", "fo:font-family"),
        east_asian: face("style:font-name-asian", "style:font-family-asian"),
        complex: face("style:font-name-complex", "style:font-family-complex"),
        lang: get(properties, "fo:language").map(|language| match get(properties, "fo:country") {
            Some(country) => format!("{language}-{country}"),
            None => language.to_string(),
        }),
        ..Default::default()
    }
}

/// "super 58%", "sub", "33% 58%" as a baseline shift in thousandths of
/// a percent.
fn text_position(value: &str) -> Option<i32> {
    let first = value.split_whitespace().next()?;
    match first {
        "super" => Some(30_000),
        "sub" => Some(-25_000),
        "0%" => None,
        other => percent(other),
    }
}

/// the text body of a shape element.
pub fn read_text_body(
    container: &Element,
    graphic: &Properties,
    context: &TextContext<'_>,
) -> TextBody {
    TextBody {
        body: read_body(graphic),
        list_style: Default::default(),
        paragraphs: read_paragraphs(container, context),
    }
}
