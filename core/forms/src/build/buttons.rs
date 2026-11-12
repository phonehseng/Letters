//! reset and print buttons: push buttons with the standard /ResetForm and
//! /Named /Print actions (iso 32000 12.6.4), no scripts.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::add::{check_name, ensure_form, place_widget, unique_name};
use crate::appearance::ensure_default_font;
use crate::error::{FormError, FormResult};
use crate::fonts::StandardFont;
use crate::geometry::{number, VisibleRect};
use crate::objects::{
    check_document, name, page_frame, page_id, push_to_array, real, rect_object, text_string,
};
use crate::tree::{find_field, flags};

/// what a button does. both are standard actions every reader knows; no
/// scripts are involved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ButtonAction {
    /// clears the form (or only these fields) back to its defaults
    ResetForm {
        #[serde(default)]
        fields: Option<Vec<String>>,
    },
    /// opens the print dialog
    Print,
}

/// a push button to add.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewButton {
    pub name: String,
    pub page: u32,
    pub rect: VisibleRect,
    /// the words on the button, such as "Clear form"
    pub label: String,
    pub action: ButtonAction,
}

/// adds a reset or print button and returns its name.
pub fn add_button(doc: &mut Document, button: &NewButton) -> FormResult<String> {
    check_document(doc)?;
    check_name(&button.name)?;
    if button.rect.is_empty() {
        return Err(FormError::EmptyRect);
    }
    let page = page_id(doc, button.page)?;
    let form = ensure_form(doc)?;
    let font = ensure_default_font(doc)?;
    let field_name = unique_name(doc, &button.name);
    let frame = page_frame(doc, page);
    let mut action = Dictionary::new();
    match &button.action {
        ButtonAction::ResetForm { fields } => {
            action.set("S", name("ResetForm"));
            if let Some(fields) = fields {
                for field in fields {
                    find_field(doc, field)?;
                }
                action.set(
                    "Fields",
                    Object::Array(fields.iter().map(|field| text_string(field)).collect()),
                );
            }
        }
        ButtonAction::Print => {
            action.set("S", name("Named"));
            action.set("N", name("Print"));
        }
    }
    let mut characteristics = Dictionary::new();
    characteristics.set("BC", Object::Array(vec![real(0.4)]));
    characteristics.set("BG", Object::Array(vec![real(0.9)]));
    characteristics.set("CA", text_string(&button.label));
    if frame.rotation != 0 {
        characteristics.set("R", Object::Integer(frame.rotation as i64));
    }
    let user_rect = frame.to_user_rect(&button.rect);
    let mut dict = Dictionary::new();
    dict.set("FT", name("Btn"));
    dict.set("Ff", Object::Integer(flags::PUSH_BUTTON as i64));
    dict.set("T", text_string(&field_name));
    dict.set("Type", name("Annot"));
    dict.set("Subtype", name("Widget"));
    dict.set("Rect", rect_object(&user_rect));
    dict.set("P", Object::Reference(page));
    dict.set("F", Object::Integer(4));
    dict.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
    dict.set("MK", Object::Dictionary(characteristics));
    dict.set("A", Object::Dictionary(action));
    let id = doc.add_object(Object::Dictionary(dict));
    draw_button(doc, id, &button.label, font)?;
    place_widget(doc, page, id)?;
    push_to_array(doc, form, b"Fields", Object::Reference(id))?;
    Ok(field_name)
}

/// a grey box with a border and the label centred in helvetica.
fn draw_button(
    doc: &mut Document,
    widget_id: ObjectId,
    label: &str,
    font: ObjectId,
) -> FormResult<()> {
    let Some(widget) = crate::appearance::WidgetBox::read(doc, widget_id) else {
        return Ok(());
    };
    let bytes = crate::text::encode_win_ansi(label).map_err(FormError::UnsupportedCharacters)?;
    let metrics = crate::fonts::FontMetrics::standard(StandardFont::Helvetica);
    // a caption a little over half the button's height, 4 to 12 points
    let size = (widget.height * 0.55).clamp(4.0, 12.0);
    let width = metrics.text_width(&bytes, size);
    let size = if width > widget.width - 4.0 && width > 0.0 {
        (size * (widget.width - 4.0) / width).max(4.0)
    } else {
        size
    };
    let width = metrics.text_width(&bytes, size);
    let x = (widget.width - width) / 2.0;
    let y = (widget.height - size * 0.7) / 2.0;
    let content = format!(
        "0.9 g 0 0 {w} {h} re f\n0.4 G 1 w 0.5 0.5 {w1} {h1} re S\n/Tx BMC\nBT /Helv {s} Tf 0 g {x} {y} Td {text} Tj ET\nEMC\n",
        w = number(widget.width),
        h = number(widget.height),
        w1 = number(widget.width - 1.0),
        h1 = number(widget.height - 1.0),
        s = number(size),
        x = number(x),
        y = number(y),
        text = crate::text::literal_string(&bytes)
    );
    let appearance = crate::appearance::Appearance {
        content,
        fonts: vec![("Helv".to_owned(), Object::Reference(font))],
    };
    crate::appearance::store_appearance(doc, widget_id, &widget, &appearance)
}
