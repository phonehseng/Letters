//! text boxes (/FreeText): text typed straight onto the page.
//!
//! the text is stored in /Contents, its font, size and colour in /DA
//! ("default appearance", the same small instruction string forms use),
//! and its alignment in /Q. the appearance draws the text wrapped to the
//! box using the standard fonts' measured widths.

use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use crate::annots;
use crate::appearance::{Appearance, Canvas};
use crate::color::Color;
use crate::common::{self, Basics, Border};
use crate::error::AnnotateError;
use crate::fonts::StandardFont;
use crate::geometry::{Point, Rect};
use crate::page_space::PageSpace;
use crate::pdf_values::{dict_bytes, dict_number, dict_rect, format_number, number_array};
use crate::read::AnnotationInfo;
use crate::text_layout::{self, TextStyle};
use crate::text_string;

/// the smallest and largest text size the ui may ask for, in points.
const MIN_FONT_SIZE: f32 = 1.0;
const MAX_FONT_SIZE: f32 = 400.0;

/// the space kept between the box's edge (or border) and its text.
const PADDING: f32 = 2.0;

/// the distance between baselines, as a multiple of the text size.
/// 1.2 is the usual default of word processors and web browsers.
const LINE_SPACING: f32 = 1.2;

/// how lines of text sit across the box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    /// the /Q value pdf uses for each alignment.
    fn quadding(self) -> i64 {
        match self {
            TextAlign::Left => 0,
            TextAlign::Center => 1,
            TextAlign::Right => 2,
        }
    }

    fn from_quadding(value: f32) -> TextAlign {
        match value as i64 {
            1 => TextAlign::Center,
            2 => TextAlign::Right,
            _ => TextAlign::Left,
        }
    }
}

/// a line drawn around a text box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBorder {
    pub color: Color,
    pub width: f32,
}

fn default_font_size() -> f32 {
    12.0
}

fn default_text_color() -> Color {
    Color::BLACK
}

/// what the app sends to add a text box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBoxParams {
    /// the box, in visible page points. text wraps inside it
    pub rect: Rect,
    pub text: String,
    #[serde(default)]
    pub font: StandardFont,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// the colour of the text
    #[serde(default = "default_text_color")]
    pub color: Color,
    #[serde(default)]
    pub align: TextAlign,
    #[serde(default)]
    pub border: Option<TextBorder>,
    /// the background; none leaves the page showing through
    #[serde(default)]
    pub fill: Option<Color>,
    #[serde(default = "crate::full_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub author: Option<String>,
}

/// adds a text box to a page.
pub fn add_text_box(
    doc: &mut Document,
    page: usize,
    params: &TextBoxParams,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    common::check_rect(params.rect)?;
    check_font_size(params.font_size)?;
    text_layout::check_drawable(&params.text)?;
    if let Some(border) = params.border {
        common::check_stroke_width(border.width)?;
    }
    let basics = Basics {
        // /C is the background of a text box, so it is only set with a fill
        color: params.fill,
        opacity: params.opacity,
        contents: Some(&params.text),
        author: params.author.as_deref(),
    };
    let mut annotation = common::new_dictionary("FreeText", &page, &basics);
    let style = TextBoxStyle {
        font: params.font,
        font_size: params.font_size,
        text_color: params.color,
        align: params.align,
        border: params.border,
        fill: params.fill,
    };
    write_style(&mut annotation, &style);
    annotation.set("Rect", number_array(&page.space.user_rect(params.rect)));
    // acrobat reads /Rotate to keep a text box upright on a turned page
    // when it redraws one itself
    annotation.set("Rotate", Object::Integer(page.space.rotation));
    common::add(doc, &page, annotation)
}

pub(crate) fn check_font_size(size: f32) -> Result<(), AnnotateError> {
    if size.is_finite() && (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&size) {
        Ok(())
    } else {
        Err(AnnotateError::InvalidValue(format!(
            "Text size must be between {MIN_FONT_SIZE} and {MAX_FONT_SIZE} points."
        )))
    }
}

/// everything about how a text box looks, read from or written to its
/// dictionary.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextBoxStyle {
    pub font: StandardFont,
    pub font_size: f32,
    pub text_color: Color,
    pub align: TextAlign,
    pub border: Option<TextBorder>,
    pub fill: Option<Color>,
}

/// writes the style into /DA, /Q, /BS and /C.
///
/// the border colour goes into /DA as a stroke colour ("RG"), next to
/// the text colour, because a text box has no separate entry for it.
pub(crate) fn write_style(annotation: &mut Dictionary, style: &TextBoxStyle) {
    let mut appearance = format!(
        "/{} {} Tf {}",
        style.font.resource_name(),
        format_number(style.font_size),
        style.text_color.fill_operator()
    );
    if let Some(border) = style.border {
        appearance.push(' ');
        appearance.push_str(&border.color.stroke_operator());
    }
    annotation.set("DA", Object::string_literal(appearance));
    annotation.set("Q", Object::Integer(style.align.quadding()));
    let width = style.border.map(|border| border.width).unwrap_or(0.0);
    common::set_border(
        annotation,
        &Border {
            width,
            dash: Vec::new(),
        },
    );
    match style.fill {
        Some(fill) => annotation.set("C", fill.to_object()),
        None => {
            annotation.remove(b"C");
        }
    }
}

/// reads the style back, filling gaps with the defaults other apps use.
pub(crate) fn read_style(doc: &Document, annotation: &Dictionary) -> TextBoxStyle {
    let appearance = dict_bytes(doc, annotation, b"DA")
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();
    let parsed = parse_default_appearance(&appearance);
    let text_color = parsed.text_color.unwrap_or(Color::BLACK);
    let border_width = common::border(doc, annotation, 0.0).width;
    let border = (border_width > 0.0).then(|| TextBorder {
        color: parsed.stroke_color.unwrap_or(text_color),
        width: border_width,
    });
    TextBoxStyle {
        font: parsed.font.unwrap_or_default(),
        font_size: parsed
            .font_size
            .filter(|size| (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(size))
            .unwrap_or(default_font_size()),
        text_color,
        align: dict_number(doc, annotation, b"Q")
            .map(TextAlign::from_quadding)
            .unwrap_or_default(),
        border,
        fill: common::color(doc, annotation, b"C"),
    }
}

/// the text of a text box.
pub(crate) fn text(doc: &Document, annotation: &Dictionary) -> String {
    dict_bytes(doc, annotation, b"Contents")
        .map(text_string::decode)
        .unwrap_or_default()
}

/// what a /DA string says.
#[derive(Debug, Default, PartialEq)]
struct DefaultAppearance {
    font: Option<StandardFont>,
    font_size: Option<f32>,
    text_color: Option<Color>,
    stroke_color: Option<Color>,
}

/// reads a /DA string such as "/Helv 12 Tf 0 0 1 rg". it is a tiny piece
/// of content stream: operands followed by an operator.
fn parse_default_appearance(text: &str) -> DefaultAppearance {
    let mut result = DefaultAppearance::default();
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let numbers_before = |index: usize, count: usize| -> Option<Vec<f32>> {
        let start = index.checked_sub(count)?;
        tokens[start..index]
            .iter()
            .map(|token| token.parse().ok())
            .collect()
    };
    let to_byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let colour = |values: &[f32]| match values {
        [grey] => Some(Color::new(to_byte(*grey), to_byte(*grey), to_byte(*grey))),
        [r, g, b] => Some(Color::new(to_byte(*r), to_byte(*g), to_byte(*b))),
        [c, m, y, k] => Some(Color::new(
            to_byte(1.0 - (c + k).min(1.0)),
            to_byte(1.0 - (m + k).min(1.0)),
            to_byte(1.0 - (y + k).min(1.0)),
        )),
        _ => None,
    };
    for (index, token) in tokens.iter().enumerate() {
        match *token {
            "Tf" if index >= 2 => {
                result.font = StandardFont::from_name(tokens[index - 2]);
                result.font_size = tokens[index - 1].parse().ok();
            }
            "g" => result.text_color = numbers_before(index, 1).and_then(|v| colour(&v)),
            "rg" => result.text_color = numbers_before(index, 3).and_then(|v| colour(&v)),
            "k" => result.text_color = numbers_before(index, 4).and_then(|v| colour(&v)),
            "G" => result.stroke_color = numbers_before(index, 1).and_then(|v| colour(&v)),
            "RG" => result.stroke_color = numbers_before(index, 3).and_then(|v| colour(&v)),
            "K" => result.stroke_color = numbers_before(index, 4).and_then(|v| colour(&v)),
            _ => {}
        }
    }
    result
}

/// draws the box: background, border, then the wrapped text, clipped to
/// the inside of the box so long text never spills onto the page.
pub(crate) fn draw(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Result<Appearance, AnnotateError> {
    let frame = dict_rect(doc, annotation, b"Rect")
        .map(|rect| space.visible_rect(rect))
        .filter(|rect| !rect.is_empty())
        .ok_or(AnnotateError::EmptyArea)?;
    let style = read_style(doc, annotation);
    let border_width = style.border.map(|border| border.width).unwrap_or(0.0);
    let inside = inset(frame, border_width + PADDING);
    let text_style = TextStyle {
        font: style.font,
        size: style.font_size,
    };
    let lines = text_layout::wrap(&text(doc, annotation), text_style, inside.width)?;

    let mut canvas = Canvas::new(frame);
    canvas.opacity(common::opacity(doc, annotation), false);
    if let Some(fill) = style.fill {
        canvas.fill_color(fill);
        canvas.rect(frame);
        canvas.fill();
    }
    if let Some(border) = style.border {
        canvas.stroke_color(border.color);
        canvas.line_width(border.width);
        canvas.rect(inset(frame, border.width / 2.0));
        canvas.stroke();
    }
    canvas.save();
    canvas.clip(inset(frame, border_width));
    canvas.fill_color(style.text_color);
    let (ascent, _) = style.font.ascent_and_descent();
    let first_baseline = inside.y + ascent * style.font_size / 1000.0;
    for (number, line) in lines.iter().enumerate() {
        let left = match style.align {
            TextAlign::Left => inside.x,
            TextAlign::Center => inside.x + (inside.width - line.width) / 2.0,
            TextAlign::Right => inside.right() - line.width,
        };
        let baseline = first_baseline + number as f32 * style.font_size * LINE_SPACING;
        if !line.encoded.is_empty() {
            canvas.text(
                style.font,
                style.font_size,
                Point::new(left, baseline),
                &line.encoded,
            );
        }
    }
    canvas.restore();
    Ok(canvas.finish())
}

/// a rectangle shrunk by `amount` on every side, never below nothing.
fn inset(rect: Rect, amount: f32) -> Rect {
    let amount = amount.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
    rect.grow(-amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_default_appearance_strings_from_other_apps() {
        let parsed = parse_default_appearance("0 0 1 rg /Helv 14 Tf 1 0 0 RG");
        assert_eq!(parsed.font, Some(StandardFont::Helvetica));
        assert_eq!(parsed.font_size, Some(14.0));
        assert_eq!(parsed.text_color, Some(Color::new(0, 0, 255)));
        assert_eq!(parsed.stroke_color, Some(Color::new(255, 0, 0)));
        let grey = parse_default_appearance("/TiRo 9.5 Tf 0.5 g");
        assert_eq!(grey.font, Some(StandardFont::TimesRoman));
        assert_eq!(grey.text_color, Some(Color::new(128, 128, 128)));
    }

    #[test]
    fn style_round_trips_through_the_dictionary() {
        let doc = Document::new();
        let style = TextBoxStyle {
            font: StandardFont::CourierBold,
            font_size: 9.0,
            text_color: Color::new(0, 128, 0),
            align: TextAlign::Right,
            border: Some(TextBorder {
                color: Color::new(255, 0, 0),
                width: 1.5,
            }),
            fill: Some(Color::WHITE),
        };
        let mut annotation = Dictionary::new();
        write_style(&mut annotation, &style);
        assert_eq!(read_style(&doc, &annotation), style);
    }
}
