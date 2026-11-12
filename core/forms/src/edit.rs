//! changing and removing fields that are already in the form.

use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::add::{check_name, options_object, place_widget};
use crate::appearance::{refresh_field, selected_values, DefaultAppearance};
use crate::date::{format_script, keystroke_script};
use crate::error::{FormError, FormResult};
use crate::geometry::{number, VisibleRect};
use crate::objects::{
    acroform_id, annotation_pages, check_document, name, page_frame, page_id, page_ids,
    rect_object, references_in, remove_from_array, text_string,
};
use crate::read::{kind_of, Alignment, ChoiceOption, FieldKind};
use crate::tree::{collect_fields, find_field, flags, FieldNode};

/// what to change about a field. everything left out stays as it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldChanges {
    /// a new last part for the name (no dots)
    pub new_name: Option<String>,
    /// which widget `rect` and `page` move, counted from 0. most fields have one
    pub widget: usize,
    /// a new place or size, in points from the visible top left
    pub rect: Option<VisibleRect>,
    /// move the widget to another page (0-based); `rect` is then on that page
    pub page: Option<u32>,
    /// new choices for a dropdown or list
    pub options: Option<Vec<ChoiceOption>>,
    pub required: Option<bool>,
    pub read_only: Option<bool>,
    /// 0 removes the limit
    pub max_length: Option<u32>,
    pub comb: Option<bool>,
    pub multi_select: Option<bool>,
    /// an empty tooltip removes it
    pub tooltip: Option<String>,
    /// 0 fits the text to the field
    pub font_size: Option<f32>,
    pub alignment: Option<Alignment>,
    pub date_format: Option<String>,
}

/// changes a field and draws it again. returns its full name afterwards.
pub fn update_field(doc: &mut Document, name: &str, changes: &FieldChanges) -> FormResult<String> {
    check_document(doc)?;
    let mut field = find_field(doc, name)?;
    let kind = kind_of(doc, &field).ok_or_else(|| FormError::UnknownField(name.to_owned()))?;

    if let Some(new_name) = &changes.new_name {
        field.name = rename(doc, &field, new_name)?;
    }
    if changes.rect.is_some() || changes.page.is_some() {
        move_widget(doc, &field, changes)?;
    }
    if let Some(options) = &changes.options {
        change_options(doc, &field, kind, options)?;
    }
    change_flags(doc, &field, changes)?;
    change_settings(doc, &field, kind, changes)?;

    let field = find_field(doc, &field.name)?;
    refresh_field(doc, &field, true)?;
    Ok(field.name)
}

fn rename(doc: &mut Document, field: &FieldNode, new_part: &str) -> FormResult<String> {
    check_name(new_part)?;
    let full = match field.name.rsplit_once('.') {
        Some((parent, _)) => format!("{parent}.{new_part}"),
        None => new_part.to_owned(),
    };
    if full != field.name && collect_fields(doc).iter().any(|other| other.name == full) {
        return Err(FormError::NameTaken(full));
    }
    doc.get_dictionary_mut(field.id)?
        .set("T", text_string(new_part));
    Ok(full)
}

fn move_widget(doc: &mut Document, field: &FieldNode, changes: &FieldChanges) -> FormResult<()> {
    let widget = *field.widgets.get(changes.widget).ok_or_else(|| {
        FormError::UnknownField(format!("{} (widget {})", field.name, changes.widget + 1))
    })?;
    let pages = annotation_pages(doc);
    let current_page = pages.get(&widget).copied();
    let target_page = changes.page.or(current_page).unwrap_or(0);
    let target_id = page_id(doc, target_page)?;

    let rect = match changes.rect {
        Some(rect) if rect.is_empty() => return Err(FormError::EmptyRect),
        Some(rect) => rect,
        // moving to another page without a new rect keeps the same visible place
        None => {
            let old_page = page_ids(doc)[current_page.unwrap_or(0) as usize];
            let dict = doc.get_dictionary(widget)?;
            let user = crate::objects::get_rect(doc, dict, b"Rect").ok_or(FormError::EmptyRect)?;
            page_frame(doc, old_page).to_visible_rect(&user)
        }
    };
    let frame = page_frame(doc, target_id);
    {
        let dict = doc.get_dictionary_mut(widget)?;
        dict.set("Rect", rect_object(&frame.to_user_rect(&rect)));
        dict.set("P", Object::Reference(target_id));
    }
    if current_page != Some(target_page) {
        if let Some(old) = current_page {
            let old_id = page_ids(doc)[old as usize];
            remove_from_array(doc, old_id, b"Annots", widget)?;
        }
        place_widget(doc, target_id, widget)?;
    }
    Ok(())
}

fn change_options(
    doc: &mut Document,
    field: &FieldNode,
    kind: FieldKind,
    options: &[ChoiceOption],
) -> FormResult<()> {
    if !matches!(kind, FieldKind::Dropdown | FieldKind::List) {
        return Err(FormError::WrongValueKind {
            field: field.name.clone(),
            hint: "Only dropdowns and lists have choices to change.".into(),
        });
    }
    if options.is_empty() {
        return Err(FormError::NoOptions);
    }
    let still_offered: Vec<String> = selected_values(doc, field)
        .into_iter()
        .filter(|value| options.iter().any(|option| &option.value == value))
        .collect();
    let dict = doc.get_dictionary_mut(field.id)?;
    dict.set("Opt", options_object(options));
    // a choice that is no longer offered is dropped from the answer
    dict.remove(b"I");
    match still_offered.len() {
        0 => {
            dict.remove(b"V");
        }
        1 => dict.set("V", text_string(&still_offered[0])),
        _ => dict.set(
            "V",
            Object::Array(
                still_offered
                    .iter()
                    .map(|value| text_string(value))
                    .collect(),
            ),
        ),
    }
    Ok(())
}

fn change_flags(doc: &mut Document, field: &FieldNode, changes: &FieldChanges) -> FormResult<()> {
    let mut field_flags = field.flags(doc);
    let mut set = |flag: u32, on: Option<bool>| match on {
        Some(true) => field_flags |= flag,
        Some(false) => field_flags &= !flag,
        None => {}
    };
    set(flags::REQUIRED, changes.required);
    set(flags::READ_ONLY, changes.read_only);
    set(flags::COMB, changes.comb);
    set(flags::MULTI_SELECT, changes.multi_select);
    // inherited flags are written on the field itself so siblings keep theirs
    doc.get_dictionary_mut(field.id)?
        .set("Ff", Object::Integer(field_flags as i64));
    Ok(())
}

fn change_settings(
    doc: &mut Document,
    field: &FieldNode,
    kind: FieldKind,
    changes: &FieldChanges,
) -> FormResult<()> {
    let da = field.default_appearance(doc).unwrap_or_default();
    let dict = doc.get_dictionary_mut(field.id)?;
    if let Some(max) = changes.max_length {
        if max == 0 {
            dict.remove(b"MaxLen");
        } else {
            dict.set("MaxLen", Object::Integer(max as i64));
        }
    }
    if let Some(tooltip) = &changes.tooltip {
        if tooltip.is_empty() {
            dict.remove(b"TU");
        } else {
            dict.set("TU", text_string(tooltip));
        }
    }
    if let Some(alignment) = changes.alignment {
        dict.set("Q", Object::Integer(alignment.quadding()));
    }
    if let Some(size) = changes.font_size {
        let parsed = DefaultAppearance::parse(&da);
        let font = parsed
            .font
            .unwrap_or_else(|| crate::appearance::DEFAULT_FONT_NAME.to_owned());
        let new_da = format!("/{font} {} Tf {}", number(size.max(0.0)), parsed.color);
        dict.set("DA", Object::string_literal(new_da));
    }
    if let Some(pattern) = &changes.date_format {
        if kind != FieldKind::Date {
            return Err(FormError::WrongValueKind {
                field: field.name.clone(),
                hint: "Only date fields have a date format.".into(),
            });
        }
        let script = |js: String| {
            let mut action = lopdf::Dictionary::new();
            action.set("S", name("JavaScript"));
            action.set("JS", Object::string_literal(js));
            Object::Dictionary(action)
        };
        let mut actions = lopdf::Dictionary::new();
        actions.set("F", script(format_script(pattern)));
        actions.set("K", script(keystroke_script(pattern)));
        dict.set("AA", Object::Dictionary(actions));
    }
    Ok(())
}

/// removes a field and its widgets from the form and from the pages.
pub fn remove_field(doc: &mut Document, name: &str) -> FormResult<()> {
    check_document(doc)?;
    let field = find_field(doc, name)?;
    let pages = annotation_pages(doc);
    let page_list = page_ids(doc);
    for widget in &field.widgets {
        if let Some(page) = pages.get(widget) {
            remove_from_array(doc, page_list[*page as usize], b"Annots", *widget)?;
        }
    }
    detach_field(doc, &field)
}

/// takes a field out of the form and deletes its dictionary and widgets.
/// the caller takes the widgets off their pages first.
pub(crate) fn detach_field(doc: &mut Document, field: &FieldNode) -> FormResult<()> {
    detach(doc, field.id, &field.lineage)?;
    for widget in &field.widgets {
        remove_widget(doc, *widget);
    }
    doc.objects.remove(&field.id);
    Ok(())
}

/// takes a field out of its parent's /Kids (or the form's /Fields), and
/// removes parents that are left with no kids.
fn detach(doc: &mut Document, id: ObjectId, lineage: &[ObjectId]) -> FormResult<()> {
    match lineage.get(1) {
        Some(parent) => {
            remove_from_array(doc, *parent, b"Kids", id)?;
            let left = references_in(doc, doc.get_dictionary(*parent)?, b"Kids");
            if left.is_empty() {
                detach(doc, *parent, &lineage[1..])?;
                doc.objects.remove(parent);
            }
        }
        None => {
            if let Some(form) = acroform_id(doc)? {
                remove_from_array(doc, form, b"Fields", id)?;
            }
        }
    }
    Ok(())
}

/// deletes a widget. its drawings are left in place because other widgets
/// may share them; an unused drawing costs a few bytes and harms nothing.
fn remove_widget(doc: &mut Document, widget: ObjectId) {
    doc.objects.remove(&widget);
}
