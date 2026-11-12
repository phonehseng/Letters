//! adding new fields to a page.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::appearance::{ensure_default_font, refresh_field, DEFAULT_FONT_NAME};
use crate::date::{format_script, keystroke_script, DEFAULT_PATTERN};
use crate::error::{FormError, FormResult};
use crate::geometry::{number, VisibleRect};
use crate::objects::{
    acroform_id, catalog_id, check_document, get_rect, insert_into_array, name, page_frame,
    page_id, push_to_array, rect_object, references_in, text_string,
};
use crate::read::{Alignment, ChoiceOption};
use crate::tree::{collect_fields, find_field, flags};

/// a field to add. `rect` is in points from the page's visible top left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewField {
    /// the name to give it. a name already in use gets "_2", "_3"... added
    pub name: String,
    /// 0-based page index
    pub page: u32,
    /// where it goes. a radio group ignores this; each button has its own
    pub rect: VisibleRect,
    pub kind: NewFieldKind,
    #[serde(default)]
    pub tooltip: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub read_only: bool,
    /// text size in points; 0 fits the text to the field
    #[serde(default)]
    pub font_size: f32,
    #[serde(default = "left")]
    pub alignment: Alignment,
}

fn left() -> Alignment {
    Alignment::Left
}

/// the kind of a new field and what only that kind needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NewFieldKind {
    Text {
        #[serde(default)]
        max_length: Option<u32>,
        /// one character per box; needs `max_length`
        #[serde(default)]
        comb: bool,
    },
    MultilineText,
    Checkbox {
        /// the value stored when ticked, "Yes" when not given
        #[serde(default)]
        on_value: Option<String>,
    },
    RadioGroup {
        buttons: Vec<RadioButton>,
    },
    Dropdown {
        options: Vec<ChoiceOption>,
        /// whether people may also type their own answer
        #[serde(default)]
        editable: bool,
    },
    List {
        options: Vec<ChoiceOption>,
        #[serde(default)]
        multi_select: bool,
    },
    Date {
        /// such as "dd/mm/yyyy"; yyyy-mm-dd when not given
        #[serde(default)]
        format: Option<String>,
    },
    /// an empty box to be signed later
    Signature,
}

/// one button of a new radio group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadioButton {
    pub value: String,
    pub rect: VisibleRect,
}

/// adds a field and draws it, and returns the name it was given.
///
/// the document gets a form (/AcroForm) if it had none, with helvetica as
/// its font. the widget is put into the page's annotation list in reading
/// order (top to bottom, then left to right) and the page is set to tab
/// through fields in rows, so the tab key moves through fields the way
/// people read.
pub fn add_field(doc: &mut Document, spec: &NewField) -> FormResult<String> {
    check_document(doc)?;
    validate(spec)?;
    let page = page_id(doc, spec.page)?;
    let form = ensure_form(doc)?;
    ensure_default_font(doc)?;
    let name = unique_name(doc, &spec.name);

    let field_id = match &spec.kind {
        NewFieldKind::RadioGroup { buttons } => add_radio_group(doc, spec, &name, page, buttons)?,
        _ => {
            let mut dict = field_dict(spec, &name);
            dict.extend(&widget_dict(doc, spec, page, &spec.rect));
            let id = doc.add_object(Object::Dictionary(dict));
            place_widget(doc, page, id)?;
            id
        }
    };
    push_to_array(doc, form, b"Fields", Object::Reference(field_id))?;
    let field = find_field(doc, &name)?;
    refresh_field(doc, &field, true)?;
    Ok(name)
}

fn validate(spec: &NewField) -> FormResult<()> {
    check_name(&spec.name)?;
    match &spec.kind {
        NewFieldKind::RadioGroup { buttons } => {
            if buttons.is_empty() {
                return Err(FormError::NoOptions);
            }
            if buttons.iter().any(|button| button.rect.is_empty()) {
                return Err(FormError::EmptyRect);
            }
            if buttons.iter().any(|button| button.value.trim().is_empty()) {
                return Err(FormError::BadName);
            }
        }
        NewFieldKind::Dropdown { options, .. } | NewFieldKind::List { options, .. }
            if options.is_empty() =>
        {
            return Err(FormError::NoOptions)
        }
        _ if spec.rect.is_empty() => return Err(FormError::EmptyRect),
        _ => {}
    }
    Ok(())
}

/// a field name part can't be empty or hold a dot, because dots join the
/// parts of a full name.
pub(crate) fn check_name(name: &str) -> FormResult<()> {
    if name.trim().is_empty() || name.contains('.') {
        return Err(FormError::BadName);
    }
    Ok(())
}

/// `wanted`, or `wanted_2`, `wanted_3`... whichever is free.
pub(crate) fn unique_name(doc: &Document, wanted: &str) -> String {
    let taken: Vec<String> = collect_fields(doc)
        .into_iter()
        .map(|field| field.name)
        .collect();
    let is_taken = |candidate: &str| {
        taken
            .iter()
            .any(|name| name == candidate || name.starts_with(&format!("{candidate}.")))
    };
    let wanted = wanted.trim();
    if !is_taken(wanted) {
        return wanted.to_owned();
    }
    (2..)
        .map(|counter| format!("{wanted}_{counter}"))
        .find(|candidate| !is_taken(candidate))
        .unwrap_or_default()
}

/// the form dictionary, made when the document has none.
pub(crate) fn ensure_form(doc: &mut Document) -> FormResult<ObjectId> {
    if let Some(id) = acroform_id(doc)? {
        let form = doc.get_dictionary_mut(id)?;
        if !form.has(b"Fields") {
            form.set("Fields", Object::Array(Vec::new()));
        }
        if !form.has(b"DA") {
            form.set(
                "DA",
                Object::string_literal(format!("/{DEFAULT_FONT_NAME} 0 Tf 0 g")),
            );
        }
        return Ok(id);
    }
    let mut form = Dictionary::new();
    form.set("Fields", Object::Array(Vec::new()));
    form.set(
        "DA",
        Object::string_literal(format!("/{DEFAULT_FONT_NAME} 0 Tf 0 g")),
    );
    let id = doc.add_object(Object::Dictionary(form));
    let catalog = catalog_id(doc)?;
    doc.get_dictionary_mut(catalog)?
        .set("AcroForm", Object::Reference(id));
    Ok(id)
}

/// the field part of the dictionary: type, name, flags and settings.
fn field_dict(spec: &NewField, field_name: &str) -> Dictionary {
    let mut dict = Dictionary::new();
    let mut field_flags = 0;
    if spec.required {
        field_flags |= flags::REQUIRED;
    }
    if spec.read_only {
        field_flags |= flags::READ_ONLY;
    }
    let field_type = match &spec.kind {
        NewFieldKind::Text { max_length, comb } => {
            if let Some(max) = max_length.filter(|max| *max > 0) {
                dict.set("MaxLen", Object::Integer(max as i64));
                if *comb {
                    field_flags |= flags::COMB;
                }
            }
            "Tx"
        }
        NewFieldKind::MultilineText => {
            field_flags |= flags::MULTILINE;
            "Tx"
        }
        NewFieldKind::Date { format } => {
            let pattern = format.clone().unwrap_or_else(|| DEFAULT_PATTERN.to_owned());
            dict.set("AA", date_actions(&pattern));
            "Tx"
        }
        NewFieldKind::Checkbox { .. } => {
            dict.set("V", name_object("Off"));
            "Btn"
        }
        NewFieldKind::RadioGroup { .. } => {
            field_flags |= flags::RADIO | flags::NO_TOGGLE_TO_OFF;
            dict.set("V", name_object("Off"));
            "Btn"
        }
        NewFieldKind::Dropdown { options, editable } => {
            field_flags |= flags::COMBO;
            if *editable {
                field_flags |= flags::EDIT;
            }
            dict.set("Opt", options_object(options));
            "Ch"
        }
        NewFieldKind::List {
            options,
            multi_select,
        } => {
            if *multi_select {
                field_flags |= flags::MULTI_SELECT;
            }
            dict.set("Opt", options_object(options));
            "Ch"
        }
        NewFieldKind::Signature => "Sig",
    };
    dict.set("FT", name(field_type));
    dict.set("T", text_string(field_name));
    if field_flags != 0 {
        dict.set("Ff", Object::Integer(field_flags as i64));
    }
    if let Some(tooltip) = spec.tooltip.as_deref().filter(|tip| !tip.is_empty()) {
        dict.set("TU", text_string(tooltip));
    }
    if field_type != "Sig" {
        let size = number(spec.font_size.max(0.0));
        dict.set(
            "DA",
            Object::string_literal(format!("/{DEFAULT_FONT_NAME} {size} Tf 0 g")),
        );
    }
    if spec.alignment != Alignment::Left {
        dict.set("Q", Object::Integer(spec.alignment.quadding()));
    }
    dict
}

fn name_object(value: &str) -> Object {
    Object::Name(value.as_bytes().to_vec())
}

/// the format and keystroke scripts acrobat uses to mark a date field.
/// letters never runs them; they are there so other pdf apps know.
fn date_actions(pattern: &str) -> Object {
    let script = |js: String| {
        let mut action = Dictionary::new();
        action.set("S", name("JavaScript"));
        action.set("JS", Object::string_literal(js));
        Object::Dictionary(action)
    };
    let mut actions = Dictionary::new();
    actions.set("F", script(format_script(pattern)));
    actions.set("K", script(keystroke_script(pattern)));
    Object::Dictionary(actions)
}

pub(crate) fn options_object(options: &[ChoiceOption]) -> Object {
    Object::Array(
        options
            .iter()
            .map(|option| {
                if option.value == option.label || option.label.is_empty() {
                    text_string(&option.value)
                } else {
                    Object::Array(vec![text_string(&option.value), text_string(&option.label)])
                }
            })
            .collect(),
    )
}

/// the widget part of the dictionary: where it is and how its box looks.
fn widget_dict(doc: &Document, spec: &NewField, page: ObjectId, rect: &VisibleRect) -> Dictionary {
    let frame = page_frame(doc, page);
    let mut dict = Dictionary::new();
    dict.set("Type", name("Annot"));
    dict.set("Subtype", name("Widget"));
    dict.set("Rect", rect_object(&frame.to_user_rect(rect)));
    dict.set("P", Object::Reference(page));
    // 4 = print: answers appear when the page is printed
    dict.set("F", Object::Integer(4));

    let mut characteristics = Dictionary::new();
    if frame.rotation != 0 {
        // turn the drawing with the page so the answer reads upright
        characteristics.set("R", Object::Integer(frame.rotation as i64));
    }
    let gray = |level: f32| Object::Array(vec![crate::objects::real(level)]);
    match &spec.kind {
        NewFieldKind::Checkbox { on_value } => {
            characteristics.set("BC", gray(0.0));
            characteristics.set("BG", gray(1.0));
            characteristics.set("CA", Object::string_literal("4"));
            dict.set("AS", name_object("Off"));
            let state = on_value
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("Yes");
            dict.set("AP", placeholder_states(state));
        }
        NewFieldKind::RadioGroup { .. } => {
            characteristics.set("BC", gray(0.0));
            characteristics.set("BG", gray(1.0));
            characteristics.set("CA", Object::string_literal("l"));
        }
        NewFieldKind::Dropdown { .. } | NewFieldKind::List { .. } => {
            characteristics.set("BC", gray(0.6));
        }
        NewFieldKind::Text { comb: true, .. } => {
            characteristics.set("BC", gray(0.6));
        }
        _ => {}
    }
    let mut border = Dictionary::new();
    border.set("W", Object::Integer(1));
    border.set("S", name("S"));
    dict.set("BS", Object::Dictionary(border));
    dict.set("MK", Object::Dictionary(characteristics));
    dict
}

fn add_radio_group(
    doc: &mut Document,
    spec: &NewField,
    name: &str,
    page: ObjectId,
    buttons: &[RadioButton],
) -> FormResult<ObjectId> {
    let parent = doc.add_object(Object::Dictionary(field_dict(spec, name)));
    let mut kids = Vec::new();
    let mut used: Vec<&str> = Vec::new();
    for button in buttons {
        if used.contains(&button.value.as_str()) {
            return Err(FormError::NameTaken(button.value.clone()));
        }
        used.push(&button.value);
        let mut widget = widget_dict(doc, spec, page, &button.rect);
        widget.set("Parent", Object::Reference(parent));
        widget.set("AS", name_object("Off"));
        // the drawing for the on state is named after the button's value
        widget.set("AP", placeholder_states(&button.value));
        let id = doc.add_object(Object::Dictionary(widget));
        place_widget(doc, page, id)?;
        kids.push(Object::Reference(id));
    }
    doc.get_dictionary_mut(parent)?
        .set("Kids", Object::Array(kids));
    Ok(parent)
}

/// an /AP that names a button's on state. the drawings themselves are
/// made right after by `refresh_field`, which reads the state name here.
fn placeholder_states(state: &str) -> Object {
    let mut states = Dictionary::new();
    states.set(state.as_bytes().to_vec(), Object::Null);
    let mut appearances = Dictionary::new();
    appearances.set("N", Object::Dictionary(states));
    Object::Dictionary(appearances)
}

/// puts a widget into its page's /Annots in reading order and asks the
/// page to tab through fields row by row.
pub(crate) fn place_widget(doc: &mut Document, page: ObjectId, widget: ObjectId) -> FormResult<()> {
    let frame = page_frame(doc, page);
    let position_of = |doc: &Document, id: ObjectId| {
        let dict = doc.get_dictionary(id).ok()?;
        Some(frame.to_visible_rect(&get_rect(doc, dict, b"Rect")?))
    };
    let Some(new_rect) = position_of(doc, widget) else {
        return push_to_array(doc, page, b"Annots", Object::Reference(widget));
    };
    let annotations = references_in(doc, doc.get_dictionary(page)?, b"Annots");
    let position = annotations.iter().position(|id| {
        position_of(doc, *id)
            .map(|rect| comes_before(&new_rect, &rect))
            .unwrap_or(false)
    });
    insert_into_array(doc, page, b"Annots", position, Object::Reference(widget))?;
    let page_dict = doc.get_dictionary_mut(page)?;
    if !page_dict.has(b"Tabs") {
        page_dict.set("Tabs", name("R"));
    }
    Ok(())
}

/// reading order: rows from the top, then left to right inside a row.
/// boxes whose tops are within half a box height count as one row.
fn comes_before(new: &VisibleRect, existing: &VisibleRect) -> bool {
    let row_tolerance = new.height.min(existing.height) / 2.0;
    if (new.y - existing.y).abs() <= row_tolerance {
        new.x < existing.x
    } else {
        new.y < existing.y
    }
}
