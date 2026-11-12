//! the text of an object as the slide editor types into it: each
//! paragraph's words with its effective font, size, colour and spacing
//! (worked out through placeholders and the theme, as the renderer does),
//! and the box the text sits in. the editor draws its typing field from
//! this, so the words appear where the slide draws them.

use letters_slides::color::{resolve, ColorContext};
use letters_slides::geometry::outline;
use letters_slides::inherit::{
    effective_body, effective_paragraph, effective_run, resolve_typeface, Chain,
};
use letters_slides::model::{
    Anchor, Autofit, Bullet, Fill, GraphicKind, Object, RunProps, Spacing, TextAlign, TextBody,
    TextParagraph, TextRun,
};
use serde::Serialize;

use super::view::points;

/// powerpoint's insets when a body sets none: 0.1 inch at the sides and
/// 0.05 inch above and below (the renderer's own defaults, render/text.rs)
const DEFAULT_INSET_SIDE: i64 = 91_440;
const DEFAULT_INSET_TOP: i64 = 45_720;
/// text with no size is 18 pt (ecma-376 part 1, 21.1.2.3.9)
const DEFAULT_SIZE: i32 = 1_800;
/// a hundred thousand is 100 % in pptx's percentages
const FULL: f64 = 100_000.0;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextView {
    pub paragraphs: Vec<ParagraphView>,
    /// where the text may go, in the shape's own points: left, top, right, bottom
    pub area: [f64; 4],
    /// "top", "middle" or "bottom"
    pub anchor: &'static str,
    pub wrap: bool,
    /// the stored shrink-to-fit scale (1 is full size)
    pub font_scale: f64,
    /// the grey words an empty placeholder shows while editing ("Click to add title")
    pub prompt: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphView {
    pub text: String,
    pub level: u8,
    /// "left", "center", "right" or "justify"
    pub align: &'static str,
    pub margin_left: f64,
    pub indent: f64,
    /// the line pitch as a multiple of the size, or fixed points
    pub line_height: LineHeight,
    pub space_before: f64,
    pub space_after: f64,
    /// the bullet character or a number's scheme, if the paragraph has one
    pub bullet: Option<String>,
    pub font: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum LineHeight {
    Multiple(f64),
    Points(f64),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableView {
    /// column widths and row heights in points
    pub columns: Vec<f64>,
    pub rows: Vec<RowView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowView {
    pub height: f64,
    pub cells: Vec<CellView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellView {
    pub text: String,
    pub column_span: u32,
    pub row_span: u32,
    /// covered by a neighbour's merge: drawn as part of it
    pub covered: bool,
}

/// the text of a shape, or `None` for objects without text.
pub fn text_view(
    chain: &Chain<'_>,
    colors: &ColorContext<'_>,
    object: &Object,
    width: f64,
    height: f64,
) -> Option<TextView> {
    let Object::Shape(shape) = object else {
        return None;
    };
    let empty = TextBody::default();
    let body = shape.text.as_ref().unwrap_or(&empty);
    // a shape without text still takes typing (powerpoint adds text to any shape)
    let effective = effective_body(chain, object, body);
    let shape_outline = outline(shape.geometry.as_ref(), width, height);
    let insets = [
        effective.inset_left.unwrap_or(DEFAULT_INSET_SIDE),
        effective.inset_top.unwrap_or(DEFAULT_INSET_TOP),
        effective.inset_right.unwrap_or(DEFAULT_INSET_SIDE),
        effective.inset_bottom.unwrap_or(DEFAULT_INSET_TOP),
    ]
    .map(points);
    let [left, top, right, bottom] = shape_outline.text_rect;
    let default_color = shape
        .style
        .as_ref()
        .and_then(|style| style.font.color.as_ref())
        .map(|color| resolve(color, colors).to_hex());
    let paragraphs = paragraphs_of(chain, colors, object, body, default_color.as_deref());
    Some(TextView {
        paragraphs,
        area: [
            left + insets[0],
            top + insets[1],
            right - insets[2],
            bottom - insets[3],
        ],
        anchor: match effective.anchor.unwrap_or(Anchor::Top) {
            Anchor::Middle => "middle",
            Anchor::Bottom => "bottom",
            _ => "top",
        },
        wrap: effective.wrap.unwrap_or(true),
        font_scale: match effective.autofit {
            Some(Autofit::Shrink {
                font_scale: Some(scale),
                ..
            }) => f64::from(scale) / FULL,
            _ => 1.0,
        },
        prompt: shape
            .placeholder
            .as_ref()
            .filter(|_| body.plain_text().trim().is_empty())
            .map(|placeholder| prompt_for(placeholder.kind.pptx_name()).to_string()),
    })
}

/// powerpoint's words for an empty placeholder, by its type.
fn prompt_for(kind: &str) -> &'static str {
    match kind {
        "title" | "ctrTitle" => "Click to add title",
        "subTitle" => "Click to add subtitle",
        "pic" => "Click to add picture",
        "dt" | "ftr" | "sldNum" | "hdr" => "",
        _ => "Click to add text",
    }
}

fn paragraphs_of(
    chain: &Chain<'_>,
    colors: &ColorContext<'_>,
    object: &Object,
    body: &TextBody,
    default_color: Option<&str>,
) -> Vec<ParagraphView> {
    let empty = [TextParagraph::default()];
    let paragraphs = if body.paragraphs.is_empty() {
        &empty[..]
    } else {
        &body.paragraphs[..]
    };
    paragraphs
        .iter()
        .map(|paragraph| paragraph_view(chain, colors, object, body, paragraph, default_color))
        .collect()
}

fn paragraph_view(
    chain: &Chain<'_>,
    colors: &ColorContext<'_>,
    object: &Object,
    body: &TextBody,
    paragraph: &TextParagraph,
    default_color: Option<&str>,
) -> ParagraphView {
    let props = effective_paragraph(chain, object, body, paragraph);
    // the editor types in one style per paragraph: its first run's, or
    // the end mark's when it is empty (what powerpoint types with there)
    let first = paragraph
        .runs
        .first()
        .map(TextRun::props)
        .or(paragraph.end_props.as_ref())
        .cloned()
        .unwrap_or_default();
    let run = effective_run(&props, &first);
    ParagraphView {
        text: paragraph.plain_text(),
        level: paragraph.level(),
        align: match props.align.unwrap_or(TextAlign::Left) {
            TextAlign::Center => "center",
            TextAlign::Right => "right",
            TextAlign::Justify | TextAlign::Distributed | TextAlign::JustifyLow => "justify",
            _ => "left",
        },
        margin_left: props.margin_left.map_or(0.0, points),
        indent: props.indent.map_or(0.0, points),
        line_height: match props.line_spacing.unwrap_or(Spacing::Percent(100_000)) {
            Spacing::Percent(share) => LineHeight::Multiple(f64::from(share) / FULL),
            Spacing::Points(hundredths) => LineHeight::Points(f64::from(hundredths) / 100.0),
        },
        space_before: spacing_points(props.space_before, &run),
        space_after: spacing_points(props.space_after, &run),
        bullet: match &props.bullet {
            Some(Bullet::Char { char }) => Some(char.clone()),
            Some(Bullet::AutoNumber { scheme, .. }) => Some(scheme.clone()),
            _ => None,
        },
        font: typeface(chain, &run),
        size: f64::from(run.size.unwrap_or(DEFAULT_SIZE)) / 100.0,
        bold: run.bold.unwrap_or(false),
        italic: run.italic.unwrap_or(false),
        underline: run.underline.as_deref().is_some_and(|u| u != "none"),
        color: format!("#{}", run_color(&run, colors, default_color)),
    }
}

/// spacing before or after in points; a percentage is of the text's size
/// (with powerpoint's 1.2 line pitch, as the renderer's layout does).
fn spacing_points(spacing: Option<Spacing>, run: &RunProps) -> f64 {
    let size = f64::from(run.size.unwrap_or(DEFAULT_SIZE)) / 100.0;
    match spacing {
        Some(Spacing::Points(hundredths)) => f64::from(hundredths) / 100.0,
        Some(Spacing::Percent(share)) => size * 1.2 * f64::from(share) / FULL,
        None => 0.0,
    }
}

fn typeface(chain: &Chain<'_>, run: &RunProps) -> String {
    let name = run
        .latin
        .as_ref()
        .map_or("+mn-lt", |face| face.typeface.as_str());
    resolve_typeface(chain.theme, name)
}

fn run_color(run: &RunProps, colors: &ColorContext<'_>, default_color: Option<&str>) -> String {
    match &run.fill {
        Some(Fill::Solid { color }) => resolve(color, colors).to_hex(),
        Some(Fill::Gradient(gradient)) => gradient.stops.first().map_or_else(
            || "000000".into(),
            |stop| resolve(&stop.color, colors).to_hex(),
        ),
        _ => default_color.unwrap_or("000000").to_string(),
    }
}

/// a table's grid and the words in each cell.
pub fn table_view(object: &Object) -> Option<TableView> {
    let Object::Frame(frame) = object else {
        return None;
    };
    let GraphicKind::Table(table) = &frame.kind else {
        return None;
    };
    Some(TableView {
        columns: table.columns.iter().copied().map(points).collect(),
        rows: table
            .rows
            .iter()
            .map(|row| RowView {
                height: points(row.height),
                cells: row
                    .cells
                    .iter()
                    .map(|cell| CellView {
                        text: cell.text.plain_text(),
                        column_span: cell.grid_span,
                        row_span: cell.row_span,
                        covered: cell.h_merge || cell.v_merge,
                    })
                    .collect(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::deck::SlideDeck;
    use crate::slides::test_decks::deck_path;
    use crate::slides::view::slide_view;

    #[test]
    fn title_text_takes_the_theme_heading_font() {
        let deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let presentation = &deck.presentation;
        let theme = presentation.themes.first().unwrap();
        let view = slide_view(presentation, &presentation.slides[0]);
        let title = view
            .objects
            .iter()
            .find(|o| matches!(o.placeholder, Some("title" | "ctrTitle")))
            .and_then(|o| o.text.as_ref())
            .unwrap();
        assert_eq!(title.paragraphs[0].font, theme.fonts.major_latin);
        assert!(title.paragraphs[0].size > 20.0);
        assert!(title.paragraphs[0].color.starts_with('#'));
    }

    #[test]
    fn bullets_and_levels_are_read() {
        let deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let presentation = &deck.presentation;
        let slide = &presentation.slides[1];
        let view = slide_view(presentation, slide);
        let body = view
            .objects
            .iter()
            .filter_map(|o| o.text.as_ref())
            .find(|t| t.paragraphs.len() > 1)
            .unwrap();
        assert!(body.paragraphs.iter().any(|p| p.bullet.is_some()));
        assert!(body.area[2] > body.area[0]);
    }

    #[test]
    fn empty_placeholders_offer_powerpoints_prompt() {
        assert_eq!(prompt_for("ctrTitle"), "Click to add title");
        assert_eq!(prompt_for("body"), "Click to add text");
        assert_eq!(prompt_for("subTitle"), "Click to add subtitle");
    }
}
