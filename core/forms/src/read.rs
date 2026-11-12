//! reading a form: every field with its kind, place, value and settings.

use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::date::pattern_from_script;
use crate::error::FormResult;
use crate::geometry::VisibleRect;
use crate::objects::{
    annotation_pages, check_document, decode_text_string, get, get_dict, get_rect, get_text,
    page_frame, page_ids, resolve,
};
use crate::tree::{collect_fields, flags, on_states, options_of, FieldNode, FieldType};

/// what kind of answer a field takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Text,
    MultilineText,
    Checkbox,
    RadioGroup,
    Dropdown,
    List,
    Date,
    Signature,
    /// a push button: it does something when clicked and holds no answer
    Button,
}

/// how text sits inside a text field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    Left,
    Center,
    Right,
}

impl Alignment {
    pub fn from_quadding(quadding: i64) -> Self {
        match quadding {
            1 => Alignment::Center,
            2 => Alignment::Right,
            _ => Alignment::Left,
        }
    }

    pub fn quadding(self) -> i64 {
        match self {
            Alignment::Left => 0,
            Alignment::Center => 1,
            Alignment::Right => 2,
        }
    }
}

/// one place on a page where a field is shown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetInfo {
    /// 0-based page index, or none when the widget isn't on any page
    pub page: Option<u32>,
    pub rect: VisibleRect,
    /// for a checkbox or radio button: the value it stands for when chosen
    pub on_value: Option<String>,
}

/// one choice of a dropdown, list or radio group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceOption {
    /// what is stored in the file when chosen
    pub value: String,
    /// what the reader sees
    pub label: String,
}

/// everything the ui needs to show and fill one field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInfo {
    /// the full name, parts joined with dots. use it to fill the field.
    pub name: String,
    pub kind: FieldKind,
    pub widgets: Vec<WidgetInfo>,
    /// the answer: the text, the chosen option, or a checkbox's on value
    pub value: Option<String>,
    /// every chosen option of a list (more than one when multi-select)
    pub selected: Vec<String>,
    /// for checkboxes: whether it is ticked
    pub checked: Option<bool>,
    pub default_value: Option<String>,
    pub options: Vec<ChoiceOption>,
    pub required: bool,
    pub read_only: bool,
    pub max_length: Option<u32>,
    pub comb: bool,
    pub password: bool,
    pub multi_select: bool,
    /// a dropdown that also accepts typed answers
    pub editable: bool,
    /// the help text shown when hovering (/TU)
    pub tooltip: Option<String>,
    pub date_format: Option<String>,
    pub alignment: Alignment,
    /// the text size, 0 when the size follows the field
    pub font_size: f32,
    /// for signature fields: whether it holds a signature
    pub signed: bool,
}

/// reads every field of the document's form. a document without a form
/// gives an empty list.
pub fn fields(doc: &Document) -> FormResult<Vec<FieldInfo>> {
    check_document(doc)?;
    let pages = annotation_pages(doc);
    let page_list = page_ids(doc);
    let infos = collect_fields(doc)
        .iter()
        .filter_map(|field| {
            let locate = |widget: ObjectId| widget_page(doc, widget, &pages, &page_list);
            field_info(doc, field, &locate)
        })
        .collect();
    Ok(infos)
}

/// which page a widget is on: from the pages' /Annots lists, or its /P.
pub(crate) fn widget_page(
    doc: &Document,
    widget: ObjectId,
    pages: &std::collections::HashMap<ObjectId, u32>,
    page_list: &[ObjectId],
) -> Option<u32> {
    if let Some(page) = pages.get(&widget) {
        return Some(*page);
    }
    let dict = doc.get_dictionary(widget).ok()?;
    let page_ref = dict.get(b"P").ok()?.as_reference().ok()?;
    page_list
        .iter()
        .position(|id| *id == page_ref)
        .map(|index| index as u32)
}

pub(crate) fn kind_of(doc: &Document, field: &FieldNode) -> Option<FieldKind> {
    let field_flags = field.flags(doc);
    let kind = match field.field_type(doc)? {
        FieldType::Text if date_pattern(doc, field).is_some() => FieldKind::Date,
        FieldType::Text if field_flags & flags::MULTILINE != 0 => FieldKind::MultilineText,
        FieldType::Text => FieldKind::Text,
        FieldType::Button if field_flags & flags::PUSH_BUTTON != 0 => FieldKind::Button,
        FieldType::Button if field_flags & flags::RADIO != 0 => FieldKind::RadioGroup,
        FieldType::Button => FieldKind::Checkbox,
        FieldType::Choice if field_flags & flags::COMBO != 0 => FieldKind::Dropdown,
        FieldType::Choice => FieldKind::List,
        FieldType::Signature => FieldKind::Signature,
    };
    Some(kind)
}

fn field_info(
    doc: &Document,
    field: &FieldNode,
    locate: &dyn Fn(ObjectId) -> Option<u32>,
) -> Option<FieldInfo> {
    let kind = kind_of(doc, field)?;
    let field_flags = field.flags(doc);
    let has = |flag: u32| field_flags & flag != 0;
    let dict = doc.get_dictionary(field.id).ok()?;

    let widgets = field
        .widgets
        .iter()
        .enumerate()
        .filter_map(|(index, widget)| widget_info(doc, field, kind, *widget, index, locate))
        .collect();
    let (value, selected, checked) = current_value(doc, field, kind);
    let options = match kind {
        FieldKind::Dropdown | FieldKind::List => options_of(doc, field)
            .into_iter()
            .map(|(value, label)| ChoiceOption { value, label })
            .collect(),
        FieldKind::RadioGroup => radio_options(doc, field),
        _ => Vec::new(),
    };
    let default_value = field.inherited(doc, b"DV").and_then(value_text);
    let font_size = field
        .default_appearance(doc)
        .map(|da| crate::appearance::DefaultAppearance::parse(&da).size)
        .unwrap_or(0.0);

    Some(FieldInfo {
        name: field.name.clone(),
        kind,
        widgets,
        value,
        selected,
        checked,
        default_value,
        options,
        required: has(flags::REQUIRED),
        read_only: has(flags::READ_ONLY),
        max_length: field.max_length(doc),
        comb: has(flags::COMB) && matches!(kind, FieldKind::Text | FieldKind::Date),
        password: has(flags::PASSWORD) && kind == FieldKind::Text,
        multi_select: has(flags::MULTI_SELECT) && kind == FieldKind::List,
        editable: has(flags::EDIT) && kind == FieldKind::Dropdown,
        tooltip: get_text(doc, dict, b"TU"),
        date_format: date_pattern(doc, field),
        alignment: Alignment::from_quadding(field.quadding(doc)),
        font_size,
        signed: kind == FieldKind::Signature && field.inherited(doc, b"V").is_some(),
    })
}

fn widget_info(
    doc: &Document,
    field: &FieldNode,
    kind: FieldKind,
    widget: ObjectId,
    index: usize,
    locate: &dyn Fn(ObjectId) -> Option<u32>,
) -> Option<WidgetInfo> {
    let dict = doc.get_dictionary(widget).ok()?;
    let user_rect = get_rect(doc, dict, b"Rect")?;
    let page = locate(widget);
    let rect = match page.and_then(|page| page_ids(doc).get(page as usize).copied()) {
        Some(page_id) => page_frame(doc, page_id).to_visible_rect(&user_rect),
        None => VisibleRect::new(
            user_rect.left,
            user_rect.bottom,
            user_rect.width(),
            user_rect.height(),
        ),
    };
    let on_value = match kind {
        FieldKind::Checkbox | FieldKind::RadioGroup => {
            Some(export_value(doc, field, widget, index))
        }
        _ => None,
    };
    Some(WidgetInfo {
        page,
        rect,
        on_value,
    })
}

/// the value a checkbox or radio widget stands for. normally the name of
/// its on state; when the field has /Opt, the matching entry of /Opt.
pub(crate) fn export_value(
    doc: &Document,
    field: &FieldNode,
    widget: ObjectId,
    index: usize,
) -> String {
    let state = on_states(doc, widget)
        .into_iter()
        .next()
        .unwrap_or_default();
    let options = options_of(doc, field);
    if let Some((export, _)) = options.get(index) {
        return export.clone();
    }
    String::from_utf8_lossy(&state).into_owned()
}

/// the state name a field's /V holds for a chosen export value.
pub(crate) fn state_for_export(
    doc: &Document,
    field: &FieldNode,
    export: &str,
) -> Option<(usize, Vec<u8>)> {
    field
        .widgets
        .iter()
        .enumerate()
        .find_map(|(index, widget)| {
            (export_value(doc, field, *widget, index) == export).then(|| {
                (
                    index,
                    on_states(doc, *widget)
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| export.as_bytes().to_vec()),
                )
            })
        })
}

fn radio_options(doc: &Document, field: &FieldNode) -> Vec<ChoiceOption> {
    let mut options: Vec<ChoiceOption> = Vec::new();
    for (index, widget) in field.widgets.iter().enumerate() {
        let value = export_value(doc, field, *widget, index);
        if !options.iter().any(|option| option.value == value) {
            options.push(ChoiceOption {
                label: value.clone(),
                value,
            });
        }
    }
    options
}

/// the current answer as (value, selected list, checked).
fn current_value(
    doc: &Document,
    field: &FieldNode,
    kind: FieldKind,
) -> (Option<String>, Vec<String>, Option<bool>) {
    let stored = field.inherited(doc, b"V");
    match kind {
        FieldKind::Checkbox | FieldKind::RadioGroup => {
            let state = stored
                .and_then(|value| value.as_name().ok())
                .filter(|state| *state != b"Off")
                .map(<[u8]>::to_vec);
            let export = state.as_ref().map(|state| state_export(doc, field, state));
            let checked = (kind == FieldKind::Checkbox).then_some(export.is_some());
            (export.clone(), export.into_iter().collect(), checked)
        }
        FieldKind::List | FieldKind::Dropdown => {
            let selected: Vec<String> = match stored {
                Some(Object::Array(items)) => items
                    .iter()
                    .filter_map(|item| value_text(resolve(doc, item)))
                    .collect(),
                Some(other) => value_text(other).into_iter().collect(),
                None => Vec::new(),
            };
            (selected.first().cloned(), selected, None)
        }
        FieldKind::Signature | FieldKind::Button => (None, Vec::new(), None),
        _ => {
            let text = stored.and_then(value_text);
            (text.clone(), text.into_iter().collect(), None)
        }
    }
}

/// turns the state name stored in /V into the export value the ui uses.
fn state_export(doc: &Document, field: &FieldNode, state: &[u8]) -> String {
    for (index, widget) in field.widgets.iter().enumerate() {
        if on_states(doc, *widget)
            .iter()
            .any(|own| own.as_slice() == state)
        {
            return export_value(doc, field, *widget, index);
        }
    }
    String::from_utf8_lossy(state).into_owned()
}

/// a stored value as text: strings are decoded, names spelled out, rich
/// text streams read as their plain text.
fn value_text(value: &Object) -> Option<String> {
    match value {
        Object::String(bytes, _) => Some(decode_text_string(bytes)),
        Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Object::Stream(stream) => stream
            .decompressed_content()
            .ok()
            .map(|bytes| decode_text_string(&bytes)),
        _ => None,
    }
}

/// the date pattern of a text field made into a date field by acrobat's
/// format script, if it is one.
pub(crate) fn date_pattern(doc: &Document, field: &FieldNode) -> Option<String> {
    let actions = field.lineage.iter().find_map(|id| {
        let dict = doc.get_dictionary(*id).ok()?;
        get_dict(doc, dict, b"AA")
    })?;
    for key in [b"F".as_slice(), b"K".as_slice()] {
        let Some(action) = get_dict(doc, actions, key) else {
            continue;
        };
        let script = match get(doc, action, b"JS") {
            Some(Object::String(bytes, _)) => decode_text_string(bytes),
            Some(Object::Stream(stream)) => stream
                .decompressed_content()
                .map(|bytes| decode_text_string(&bytes))
                .unwrap_or_default(),
            _ => continue,
        };
        if let Some(pattern) = pattern_from_script(&script) {
            return Some(pattern);
        }
    }
    None
}
