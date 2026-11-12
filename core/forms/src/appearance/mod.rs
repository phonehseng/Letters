//! drawing what a field looks like (its "appearance stream").
//!
//! a widget's look is a small drawing stored in the file under /AP. a pdf
//! can instead ask the reader to draw it (/NeedAppearances), but readers
//! do that differently or not at all, so letters always draws it: the
//! answer, in the field's own font and size, wrapped, aligned and cut to
//! the box, plus the box's background and border.

mod choices;
mod fonts;
mod text;
mod widget;

pub use choices::*;
pub use fonts::*;
pub use text::*;
use widget::frame_content;
pub use widget::*;

use lopdf::{Document, Object, ObjectId};

use crate::error::FormResult;
use crate::objects::{get, get_dict, number_of, resolve};
use crate::read::{kind_of, Alignment, FieldKind};
use crate::tree::{flags, on_state, options_of, FieldNode};

/// draws every widget of a field again from its current value.
///
/// checkboxes and radio buttons keep the drawings their author made unless
/// `redraw_buttons` is set (a new or resized field) or they have none.
pub fn refresh_field(
    doc: &mut Document,
    field: &FieldNode,
    redraw_buttons: bool,
) -> FormResult<()> {
    let Some(kind) = kind_of(doc, field) else {
        return Ok(());
    };
    match kind {
        FieldKind::Text | FieldKind::MultilineText | FieldKind::Date => {
            refresh_text(doc, field, kind)
        }
        FieldKind::Dropdown => refresh_dropdown(doc, field),
        FieldKind::List => refresh_list(doc, field),
        FieldKind::Checkbox | FieldKind::RadioGroup => {
            refresh_buttons(doc, field, kind, redraw_buttons)
        }
        FieldKind::Signature => refresh_empty_signature(doc, field),
        FieldKind::Button => Ok(()),
    }
}

/// the text of a field's /V, empty when unset.
pub fn stored_text(doc: &Document, field: &FieldNode) -> String {
    match field.inherited(doc, b"V") {
        Some(Object::String(bytes, _)) => crate::objects::decode_text_string(bytes),
        Some(Object::Name(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        _ => String::new(),
    }
}

fn refresh_text(doc: &mut Document, field: &FieldNode, kind: FieldKind) -> FormResult<()> {
    let mut text = stored_text(doc, field);
    if field.has_flag(doc, flags::PASSWORD) {
        text = "*".repeat(text.chars().count());
    }
    let comb_cells = if field.has_flag(doc, flags::COMB)
        && !field.has_flag(doc, flags::MULTILINE | flags::PASSWORD)
    {
        field.max_length(doc)
    } else {
        None
    };
    let layout = TextLayout {
        text,
        multiline: kind == FieldKind::MultilineText,
        comb_cells,
        alignment: Alignment::from_quadding(field.quadding(doc)),
    };
    draw_text_widgets(doc, field, &layout)
}

/// draws a text field showing `shown` instead of its stored value, as a
/// field with a number or percent format does: the file keeps the raw
/// value in /V and the drawing shows it formatted. `red` draws the text
/// in red, for formats that show negative numbers that way.
pub(crate) fn draw_shown_text(
    doc: &mut Document,
    field: &FieldNode,
    shown: &str,
    red: bool,
) -> FormResult<()> {
    let kind = kind_of(doc, field).unwrap_or(FieldKind::Text);
    let comb_cells = if field.has_flag(doc, flags::COMB)
        && !field.has_flag(doc, flags::MULTILINE | flags::PASSWORD)
    {
        field.max_length(doc)
    } else {
        None
    };
    let layout = TextLayout {
        text: shown.to_owned(),
        multiline: kind == FieldKind::MultilineText,
        comb_cells,
        alignment: Alignment::from_quadding(field.quadding(doc)),
    };
    let color = red.then(|| "1 0 0 rg".to_owned());
    draw_text_widgets_in(doc, field, &layout, color)
}

fn draw_text_widgets(doc: &mut Document, field: &FieldNode, layout: &TextLayout) -> FormResult<()> {
    draw_text_widgets_in(doc, field, layout, None)
}

fn draw_text_widgets_in(
    doc: &mut Document,
    field: &FieldNode,
    layout: &TextLayout,
    color: Option<String>,
) -> FormResult<()> {
    let mut da = DefaultAppearance::parse(&field.default_appearance(doc).unwrap_or_default());
    if let Some(color) = color {
        da.color = color;
    }
    let needs_win_ansi = !layout.text.is_ascii();
    let font = resolve_font(doc, da.font.as_deref(), needs_win_ansi)?;
    for widget_id in &field.widgets {
        let Some(widget) = WidgetBox::read(doc, *widget_id) else {
            continue;
        };
        let appearance = text_appearance(&widget, &da, &font, layout)?;
        store_appearance(doc, *widget_id, &widget, &appearance)?;
    }
    Ok(())
}

fn refresh_dropdown(doc: &mut Document, field: &FieldNode) -> FormResult<()> {
    let value = selected_values(doc, field)
        .into_iter()
        .next()
        .unwrap_or_default();
    // the box shows the choice's label, not the value stored for it
    let label = options_of(doc, field)
        .into_iter()
        .find(|(export, _)| *export == value)
        .map(|(_, label)| label)
        .unwrap_or(value);
    let layout = TextLayout {
        text: label,
        multiline: false,
        comb_cells: None,
        alignment: Alignment::from_quadding(field.quadding(doc)),
    };
    draw_text_widgets(doc, field, &layout)
}

/// the chosen values of a choice field's /V (a string or an array).
pub fn selected_values(doc: &Document, field: &FieldNode) -> Vec<String> {
    match field.inherited(doc, b"V") {
        Some(Object::Array(items)) => items
            .iter()
            .filter_map(|item| match resolve(doc, item) {
                Object::String(bytes, _) => Some(crate::objects::decode_text_string(bytes)),
                _ => None,
            })
            .collect(),
        Some(Object::String(bytes, _)) => vec![crate::objects::decode_text_string(bytes)],
        _ => Vec::new(),
    }
}

fn refresh_list(doc: &mut Document, field: &FieldNode) -> FormResult<()> {
    let options = options_of(doc, field);
    let selected = selected_values(doc, field);
    let labels: Vec<String> = options.iter().map(|(_, label)| label.clone()).collect();
    let chosen: Vec<bool> = options
        .iter()
        .map(|(export, _)| selected.contains(export))
        .collect();
    let top_index = field
        .inherited(doc, b"TI")
        .and_then(number_of)
        .unwrap_or(0.0)
        .max(0.0) as usize;
    let da = DefaultAppearance::parse(&field.default_appearance(doc).unwrap_or_default());
    let needs_win_ansi = labels.iter().any(|label| !label.is_ascii());
    let font = resolve_font(doc, da.font.as_deref(), needs_win_ansi)?;
    for widget_id in &field.widgets {
        let Some(widget) = WidgetBox::read(doc, *widget_id) else {
            continue;
        };
        let appearance = list_appearance(&widget, &da, &font, &labels, &chosen, top_index)?;
        store_appearance(doc, *widget_id, &widget, &appearance)?;
    }
    Ok(())
}

fn refresh_buttons(
    doc: &mut Document,
    field: &FieldNode,
    kind: FieldKind,
    redraw: bool,
) -> FormResult<()> {
    let value = field
        .inherited(doc, b"V")
        .and_then(|value| value.as_name().ok())
        .map(<[u8]>::to_vec)
        .unwrap_or_else(|| b"Off".to_vec());
    let da = DefaultAppearance::parse(&field.default_appearance(doc).unwrap_or_default());
    let radio = kind == FieldKind::RadioGroup;
    for widget_id in &field.widgets {
        let state = on_state(doc, *widget_id);
        if redraw || !has_state_drawings(doc, *widget_id, &state) {
            if let Some(widget) = WidgetBox::read(doc, *widget_id) {
                let caption = caption_of(doc, *widget_id);
                let style = CheckStyle::from_caption(caption.as_deref(), radio);
                let on = check_appearance(&widget, &da, style, radio, true);
                let off = check_appearance(&widget, &da, style, radio, false);
                store_state_appearances(doc, *widget_id, &widget, &state, &on, &off)?;
            }
        }
        let shown = if state == value {
            state
        } else {
            b"Off".to_vec()
        };
        doc.get_dictionary_mut(*widget_id)?
            .set("AS", Object::Name(shown));
    }
    Ok(())
}

fn caption_of(doc: &Document, widget: ObjectId) -> Option<String> {
    let dict = doc.get_dictionary(widget).ok()?;
    let characteristics = get_dict(doc, dict, b"MK")?;
    crate::objects::get_text(doc, characteristics, b"CA")
}

/// whether a widget already has drawings for both its on and off states.
fn has_state_drawings(doc: &Document, widget: ObjectId, state: &[u8]) -> bool {
    let Ok(dict) = doc.get_dictionary(widget) else {
        return false;
    };
    let Some(normal) = get_dict(doc, dict, b"AP").and_then(|ap| get(doc, ap, b"N")) else {
        return false;
    };
    match normal.as_dict() {
        Ok(states) => {
            let drawn = |key: &[u8]| {
                matches!(
                    states.get(key),
                    Ok(Object::Reference(_)) | Ok(Object::Stream(_))
                )
            };
            drawn(state) && drawn(b"Off")
        }
        Err(_) => false,
    }
}

/// an unsigned signature field gets an empty drawing (plus its frame), so
/// readers draw its box consistently.
fn refresh_empty_signature(doc: &mut Document, field: &FieldNode) -> FormResult<()> {
    if field.inherited(doc, b"V").is_some() {
        return Ok(());
    }
    for widget_id in &field.widgets {
        let has_drawing = doc
            .get_dictionary(*widget_id)
            .map(|dict| dict.has(b"AP"))
            .unwrap_or(false);
        if has_drawing {
            continue;
        }
        if let Some(widget) = WidgetBox::read(doc, *widget_id) {
            let appearance = Appearance {
                content: frame_content(&widget, false),
                fonts: Vec::new(),
            };
            store_appearance(doc, *widget_id, &widget, &appearance)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FormError;
    use crate::fonts::{FontMetrics, StandardFont};
    use crate::geometry::{Matrix, PdfRect};

    fn plain_box(width: f32, height: f32) -> WidgetBox {
        WidgetBox {
            width,
            height,
            matrix: Matrix::IDENTITY,
            border_width: 0.0,
            border_color: None,
            background: None,
            border_style: b'S',
        }
    }

    fn helvetica() -> AppearanceFont {
        AppearanceFont {
            resource_name: "Helv".into(),
            object: Object::Null,
            metrics: FontMetrics::standard(StandardFont::Helvetica),
        }
    }

    #[test]
    fn reads_default_appearance_strings() {
        let da = DefaultAppearance::parse(b"/Helv 12 Tf 0 0 1 rg");
        assert_eq!(da.font.as_deref(), Some("Helv"));
        assert_eq!(da.size, 12.0);
        assert_eq!(da.color, "0 0 1 rg");
        assert_eq!(da.stroke_color(), "0 0 1 RG");
        assert_eq!(DefaultAppearance::parse(b"garbage(").size, 0.0);
    }

    #[test]
    fn wraps_at_spaces_and_cuts_long_words() {
        let metrics = FontMetrics::standard(StandardFont::Courier);
        // courier at size 10 is 6 points a letter: 5 letters fit in 30 points
        let lines = wrap_lines(b"ab cd efghijkl\nx", &metrics, 10.0, 30.0);
        assert_eq!(
            lines,
            vec![
                b"ab cd".to_vec(),
                b"efghi".to_vec(),
                b"jkl".to_vec(),
                b"x".to_vec()
            ]
        );
    }

    #[test]
    fn auto_size_fits_the_box() {
        let widget = plain_box(100.0, 20.0);
        let da = DefaultAppearance::parse(b"/Helv 0 Tf 0 g");
        let layout = TextLayout {
            text: "Hello".into(),
            multiline: false,
            comb_cells: None,
            alignment: Alignment::Left,
        };
        let appearance = text_appearance(&widget, &da, &helvetica(), &layout).unwrap();
        assert!(appearance.content.contains("(Hello) Tj"));
        // (20 - 2) / 1.15 is about 15.65
        assert!(
            appearance.content.contains("/Helv 15.652 Tf"),
            "{}",
            appearance.content
        );
    }

    #[test]
    fn right_aligned_text_ends_at_the_padding() {
        let widget = plain_box(100.0, 20.0);
        let da = DefaultAppearance::parse(b"/Helv 10 Tf 0 g");
        let layout = TextLayout {
            text: "Hi".into(),
            multiline: false,
            comb_cells: None,
            alignment: Alignment::Right,
        };
        let appearance = text_appearance(&widget, &da, &helvetica(), &layout).unwrap();
        // "Hi" is 9.44 points wide at size 10, so it starts at 100 - 2 - 9.44
        assert!(
            appearance.content.contains("88.56 "),
            "{}",
            appearance.content
        );
    }

    #[test]
    fn comb_fields_put_one_letter_per_cell() {
        let widget = plain_box(100.0, 20.0);
        let da = DefaultAppearance::parse(b"/Helv 10 Tf 0 g");
        let layout = TextLayout {
            text: "ABCD".into(),
            multiline: false,
            comb_cells: Some(5),
            alignment: Alignment::Left,
        };
        let appearance = text_appearance(&widget, &da, &helvetica(), &layout).unwrap();
        assert_eq!(appearance.content.matches(" Tj").count(), 4);
    }

    #[test]
    fn text_outside_winansi_is_refused() {
        let widget = plain_box(100.0, 20.0);
        let da = DefaultAppearance::parse(b"/Helv 10 Tf 0 g");
        let layout = TextLayout {
            text: "日本".into(),
            multiline: false,
            comb_cells: None,
            alignment: Alignment::Left,
        };
        let error = text_appearance(&widget, &da, &helvetica(), &layout).unwrap_err();
        assert!(matches!(error, FormError::UnsupportedCharacters(_)));
    }

    #[test]
    fn turned_widgets_start_at_the_origin() {
        let matrix = upright_matrix(90, 100.0, 20.0);
        let moved = matrix.apply_rect(&PdfRect::from_corners(0.0, 0.0, 100.0, 20.0));
        assert_eq!(
            (moved.left, moved.bottom, moved.right, moved.top),
            (0.0, 0.0, 20.0, 100.0)
        );
    }
}
