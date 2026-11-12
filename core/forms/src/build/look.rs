//! how a field looks and behaves when typed in: border, fill, font, text
//! colour, alignment, multiline, password, comb and rich text off.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::appearance::{add_form_font, refresh_field, standard_font_dict, DefaultAppearance};
use crate::error::{FormError, FormResult};
use crate::fonts::StandardFont;
use crate::geometry::number;
use crate::objects::{acroform_id, check_document, get_dict, name, real, resolve};
use crate::read::{kind_of, Alignment, FieldKind};
use crate::tree::{find_field, flags, FieldNode};

/// a colour as gray (one number), rgb (three) or cmyk (four), 0 to 1.
pub type Color = Vec<f32>;

/// the fonts a field can use without embedding anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldFont {
    Helvetica,
    HelveticaBold,
    TimesRoman,
    Courier,
}

impl FieldFont {
    /// the resource name acrobat uses for each standard font, and the font
    fn resource(self) -> (&'static str, StandardFont) {
        match self {
            FieldFont::Helvetica => ("Helv", StandardFont::Helvetica),
            FieldFont::HelveticaBold => ("HeBo", StandardFont::HelveticaBold),
            FieldFont::TimesRoman => ("TiRo", StandardFont::TimesRoman),
            FieldFont::Courier => ("Cour", StandardFont::Courier),
        }
    }
}

/// the border style of a field's box (/BS /S).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BorderStyle {
    Solid,
    Dashed,
    Underline,
}

/// how a field looks and behaves when typed in. anything left out stays.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldAppearance {
    /// the border colour; an empty list removes the border
    #[serde(default)]
    pub border_color: Option<Color>,
    /// the background colour; an empty list makes it see-through
    #[serde(default)]
    pub fill_color: Option<Color>,
    #[serde(default)]
    pub border_width: Option<f32>,
    #[serde(default)]
    pub border_style: Option<BorderStyle>,
    #[serde(default)]
    pub font: Option<FieldFont>,
    /// 0 fits the text to the field
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub text_color: Option<Color>,
    #[serde(default)]
    pub alignment: Option<Alignment>,
    #[serde(default)]
    pub multiline: Option<bool>,
    #[serde(default)]
    pub password: Option<bool>,
    /// one character per box; needs a maximum length
    #[serde(default)]
    pub comb: Option<bool>,
    /// true turns rich text off and drops any rich text copy of the value.
    /// Letters never turns rich text on: it draws plain text only
    #[serde(default)]
    pub rich_text_off: bool,
}

/// the rich text field flag (iso 32000 table 228, bit 26).
const RICH_TEXT: u32 = 1 << 25;

/// the dash pattern of a dashed border: 3 points on, 3 off, the
/// default iso 32000 gives for /BS /D.
const DASH_LENGTH: i64 = 3;

/// changes how a field looks and draws it again. everything is checked
/// before anything changes.
pub fn set_field_appearance(
    doc: &mut Document,
    field_name: &str,
    look: &FieldAppearance,
) -> FormResult<()> {
    check_document(doc)?;
    let field = find_field(doc, field_name)?;
    check_look(doc, &field, look)?;
    for widget in field.widgets.clone() {
        set_widget_look(doc, widget, look)?;
    }
    set_text_style(doc, &field, look)?;
    set_behaviour_flags(doc, &field, look)?;
    let field = find_field(doc, field_name)?;
    refresh_field(doc, &field, true)
}

fn refuse(field: &FieldNode, hint: &str) -> FormError {
    FormError::WrongValueKind {
        field: field.name.clone(),
        hint: hint.to_owned(),
    }
}

fn check_look(doc: &Document, field: &FieldNode, look: &FieldAppearance) -> FormResult<()> {
    let kind = kind_of(doc, field).ok_or_else(|| FormError::UnknownField(field.name.clone()))?;
    let is_text = matches!(
        kind,
        FieldKind::Text | FieldKind::MultilineText | FieldKind::Date
    );
    let changes_typing = look.multiline.is_some() || look.password.is_some() || look.comb.is_some();
    if changes_typing && !is_text {
        return Err(refuse(
            field,
            "Only text fields can be multiline, hidden like a password or split into boxes.",
        ));
    }
    for color in [&look.border_color, &look.fill_color].into_iter().flatten() {
        color_object(color).map_err(|_| refuse(field, COLOR_HINT))?;
    }
    if let Some(color) = &look.text_color {
        if color.is_empty() || color_object(color).is_err() {
            return Err(refuse(
                field,
                "Text needs a colour: 1, 3 or 4 numbers from 0 to 1.",
            ));
        }
    }
    if look.comb == Some(true) && field.max_length(doc).is_none() {
        return Err(refuse(
            field,
            "Give it a maximum length first, so Letters knows how many boxes to draw.",
        ));
    }
    Ok(())
}

const COLOR_HINT: &str = "A colour has 1, 3 or 4 numbers from 0 to 1.";

/// gray, rgb or cmyk as a pdf array; none of the numbers outside 0 to 1.
fn color_object(color: &[f32]) -> Result<Object, ()> {
    let known_length = matches!(color.len(), 0 | 1 | 3 | 4);
    let in_range = color.iter().all(|c| (0.0..=1.0).contains(c));
    if !known_length || !in_range {
        return Err(());
    }
    Ok(Object::Array(color.iter().map(|c| real(*c)).collect()))
}

/// the fill colour operator for a colour (g, rg or k by its length).
fn color_operator(color: &[f32]) -> String {
    let numbers: Vec<String> = color.iter().map(|c| number(*c)).collect();
    let operator = match color.len() {
        1 => "g",
        3 => "rg",
        _ => "k",
    };
    format!("{} {operator}", numbers.join(" "))
}

/// a widget's border and background (/MK) and its border style (/BS).
fn set_widget_look(doc: &mut Document, widget: ObjectId, look: &FieldAppearance) -> FormResult<()> {
    let mut dict = doc.get_dictionary(widget)?.clone();
    let mut characteristics = own_dictionary(doc, &dict, b"MK");
    if let Some(border) = &look.border_color {
        characteristics.set(
            "BC",
            color_object(border).unwrap_or(Object::Array(Vec::new())),
        );
    }
    if let Some(fill) = &look.fill_color {
        characteristics.set(
            "BG",
            color_object(fill).unwrap_or(Object::Array(Vec::new())),
        );
    }
    dict.set("MK", Object::Dictionary(characteristics));
    if look.border_width.is_some() || look.border_style.is_some() {
        let mut border = own_dictionary(doc, &dict, b"BS");
        if let Some(width) = look.border_width {
            border.set("W", real(width.max(0.0)));
        }
        if let Some(style) = look.border_style {
            set_border_style(&mut border, style);
        }
        dict.set("BS", Object::Dictionary(border));
    }
    *doc.get_dictionary_mut(widget)? = dict;
    Ok(())
}

fn set_border_style(border: &mut Dictionary, style: BorderStyle) {
    let code = match style {
        BorderStyle::Solid => "S",
        BorderStyle::Dashed => "D",
        BorderStyle::Underline => "U",
    };
    border.set("S", name(code));
    if style == BorderStyle::Dashed {
        border.set("D", Object::Array(vec![Object::Integer(DASH_LENGTH)]));
    }
}

/// a copy of a dictionary entry, followed if it is a reference.
fn own_dictionary(doc: &Document, owner: &Dictionary, key: &[u8]) -> Dictionary {
    owner
        .get(key)
        .ok()
        .and_then(|value| resolve(doc, value).as_dict().ok())
        .cloned()
        .unwrap_or_default()
}

/// the font, size and colour in /DA, and the alignment (/Q).
fn set_text_style(doc: &mut Document, field: &FieldNode, look: &FieldAppearance) -> FormResult<()> {
    let changes_text = look.font.is_some() || look.font_size.is_some() || look.text_color.is_some();
    if changes_text {
        let current = DefaultAppearance::parse(&field.default_appearance(doc).unwrap_or_default());
        let font_name = match look.font {
            Some(font) => {
                let (resource, standard) = font.resource();
                ensure_form_font(doc, resource, standard)?;
                resource.to_owned()
            }
            None => current.font.clone().unwrap_or_else(|| "Helv".to_owned()),
        };
        let size = look.font_size.unwrap_or(current.size).max(0.0);
        let color = match look.text_color.as_deref() {
            Some(color) => color_operator(color),
            None => current.color,
        };
        let da = format!("/{font_name} {} Tf {color}", number(size));
        doc.get_dictionary_mut(field.id)?
            .set("DA", Object::string_literal(da));
    }
    if let Some(alignment) = look.alignment {
        doc.get_dictionary_mut(field.id)?
            .set("Q", Object::Integer(alignment.quadding()));
    }
    Ok(())
}

/// multiline, password, comb and rich text: field flags (/Ff).
fn set_behaviour_flags(
    doc: &mut Document,
    field: &FieldNode,
    look: &FieldAppearance,
) -> FormResult<()> {
    let before = field.flags(doc);
    let mut field_flags = before;
    let settings = [
        (flags::MULTILINE, look.multiline),
        (flags::PASSWORD, look.password),
        (flags::COMB, look.comb),
        (RICH_TEXT, look.rich_text_off.then_some(false)),
    ];
    for (flag, wanted) in settings {
        match wanted {
            Some(true) => field_flags |= flag,
            Some(false) => field_flags &= !flag,
            None => {}
        }
    }
    let dict = doc.get_dictionary_mut(field.id)?;
    if field_flags != before {
        dict.set("Ff", Object::Integer(field_flags as i64));
    }
    if look.rich_text_off {
        // the rich text value and default style would contradict plain text
        dict.remove(b"RV");
        dict.remove(b"DS");
    }
    Ok(())
}

/// adds a standard font to the form's fonts under `resource`, unless it is there.
fn ensure_form_font(doc: &mut Document, resource: &str, font: StandardFont) -> FormResult<()> {
    let form_id = acroform_id(doc)?.ok_or(FormError::NoForm)?;
    let present = crate::objects::acroform(doc)
        .and_then(|form| get_dict(doc, form, b"DR"))
        .and_then(|resources| get_dict(doc, resources, b"Font"))
        .is_some_and(|fonts| fonts.has(resource.as_bytes()));
    if !present {
        let id = doc.add_object(standard_font_dict(font));
        add_form_font(doc, form_id, resource, id)?;
    }
    Ok(())
}
