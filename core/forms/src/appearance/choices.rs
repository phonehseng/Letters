//! drawing checkboxes and radio buttons (as vector marks, so they look the
//! same everywhere) and list boxes.

use super::fonts::{AppearanceFont, DefaultAppearance};
use super::text::{text_start, AUTO_SIZE_MULTILINE_START, LINE_SPACING};
use super::widget::{circle_path, frame_content, Appearance, WidgetBox};
use crate::error::{FormError, FormResult};
use crate::geometry::number;
use crate::text::{encode_win_ansi, literal_string};

/// the check styles of /MK /CA, named by their zapfdingbats letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStyle {
    Check,
    Circle,
    Cross,
    Diamond,
    Square,
    Star,
}

impl CheckStyle {
    pub fn from_caption(caption: Option<&str>, radio: bool) -> Self {
        match caption.and_then(|text| text.chars().next()) {
            Some('l') => CheckStyle::Circle,
            Some('8') => CheckStyle::Cross,
            Some('u') => CheckStyle::Diamond,
            Some('n') => CheckStyle::Square,
            Some('H') => CheckStyle::Star,
            Some('4') => CheckStyle::Check,
            _ if radio => CheckStyle::Circle,
            _ => CheckStyle::Check,
        }
    }
}

/// draws a checkbox or radio button. the mark is drawn with lines and
/// curves rather than a font, so it looks the same in every reader.
pub fn check_appearance(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    style: CheckStyle,
    round: bool,
    on: bool,
) -> Appearance {
    let mut content = frame_content(widget, round);
    if on {
        content.push_str("q\n");
        content.push_str(&check_mark(widget, da, style));
        content.push_str("Q\n");
    }
    Appearance {
        content,
        fonts: Vec::new(),
    }
}

fn check_mark(widget: &WidgetBox, da: &DefaultAppearance, style: CheckStyle) -> String {
    let side = widget.width.min(widget.height);
    let (cx, cy) = (widget.width / 2.0, widget.height / 2.0);
    let n = number;
    // a point of the mark, given in a unit square centred on the box
    let at = |u: f32, v: f32| format!("{} {}", n(cx + (u - 0.5) * side), n(cy + (v - 0.5) * side));
    match style {
        CheckStyle::Check => format!(
            "{} {} w 1 J 1 j\n{} m {} l {} l S\n",
            da.stroke_color(),
            n(side * 0.12),
            at(0.2, 0.52),
            at(0.42, 0.28),
            at(0.8, 0.76)
        ),
        CheckStyle::Cross => format!(
            "{} {} w 1 J\n{} m {} l {} m {} l S\n",
            da.stroke_color(),
            n(side * 0.1),
            at(0.25, 0.25),
            at(0.75, 0.75),
            at(0.25, 0.75),
            at(0.75, 0.25)
        ),
        CheckStyle::Circle => format!("{}\n{}f\n", da.color, circle_path(cx, cy, side * 0.25)),
        CheckStyle::Square => format!(
            "{}\n{} {} {} {} re f\n",
            da.color,
            n(cx - side * 0.25),
            n(cy - side * 0.25),
            n(side * 0.5),
            n(side * 0.5)
        ),
        CheckStyle::Diamond => format!(
            "{}\n{} m {} l {} l {} l h f\n",
            da.color,
            at(0.5, 0.2),
            at(0.8, 0.5),
            at(0.5, 0.8),
            at(0.2, 0.5)
        ),
        CheckStyle::Star => {
            let points: Vec<String> = (0..10)
                .map(|index| {
                    let radius = if index % 2 == 0 { 0.32 } else { 0.13 };
                    let angle =
                        std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::PI / 5.0;
                    at(0.5 + radius * angle.cos(), 0.5 + radius * angle.sin())
                })
                .collect();
            format!(
                "{}\n{} m {} l h f\n",
                da.color,
                points[0],
                points[1..].join(" l ")
            )
        }
    }
}

/// draws a list box: one line per choice, chosen ones highlighted.
pub fn list_appearance(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    labels: &[String],
    chosen: &[bool],
    top_index: usize,
) -> FormResult<Appearance> {
    let size = if da.size > 0.0 {
        da.size
    } else {
        AUTO_SIZE_MULTILINE_START
    };
    let leading = size * LINE_SPACING;
    let padding = widget.border_width + 2.0;
    let mut content = frame_content(widget, false);
    content.push_str("/Tx BMC\nq\n");
    let inset = widget.border_width + 1.0;
    content.push_str(&format!(
        "{} {} {} {} re W n\n",
        number(inset),
        number(inset),
        number((widget.width - 2.0 * inset).max(0.0)),
        number((widget.height - 2.0 * inset).max(0.0))
    ));
    let (ascent, _) = font.metrics.standard.ascent_descent();
    let mut top = widget.height - inset;
    for (index, label) in labels.iter().enumerate().skip(top_index) {
        if top < 0.0 {
            break;
        }
        let bytes = encode_win_ansi(label).map_err(FormError::UnsupportedCharacters)?;
        if chosen.get(index).copied().unwrap_or(false) {
            // the highlight colour acrobat uses for chosen list items
            content.push_str(&format!(
                "0.6 0.75 0.86 rg\n{} {} {} {} re f\n",
                number(inset),
                number(top - leading),
                number(widget.width - 2.0 * inset),
                number(leading)
            ));
        }
        let baseline = top - (leading - size) / 2.0 - ascent * size / 1000.0;
        content.push_str(&format!(
            "{}{} {} Td {} Tj\nET\n",
            text_start(font, da, size),
            number(padding),
            number(baseline),
            literal_string(&bytes)
        ));
        top -= leading;
    }
    content.push_str("Q\nEMC\n");
    Ok(Appearance {
        content,
        fonts: vec![(font.resource_name.clone(), font.object.clone())],
    })
}
