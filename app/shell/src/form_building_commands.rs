//! Prepare Form's building tools: the order Tab follows, putting fields
//! in a group, one field on several pages, the values a checkbox stands
//! for and a field starts with, clearing the form, and reset and print
//! buttons. each is one call through `state.edit`, so one undo step in
//! the working copy; reading the tab order changes nothing.

use letters_forms::{self as forms, FieldValue, FormError, NewButton, TabOrder};
use letters_pdf::DocumentId;
use serde::Serialize;
use tauri::State;

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

/// an edit and the names it made or touched, for the ui to announce.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamesEdit {
    document: DocumentState,
    names: Vec<String>,
}

/// the fields of a page in the order Tab visits them.
#[tauri::command(async)]
pub fn forms_tab_order(
    state: State<AppState>,
    id: u64,
    page: u32,
) -> Result<Vec<String>, CommandError> {
    state.read_document(DocumentId(id), |document| {
        Ok(forms::tab_order(document, page)?)
    })
}

#[tauri::command(async)]
pub fn forms_set_tab_order(
    state: State<AppState>,
    id: u64,
    page: u32,
    order: TabOrder,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        forms::set_tab_order(document, page, &order)
    })
}

/// puts top-level fields under one parent: "street" becomes "address.street".
#[tauri::command(async)]
pub fn forms_group_fields(
    state: State<AppState>,
    id: u64,
    group: String,
    names: Vec<String>,
) -> Result<NamesEdit, CommandError> {
    if names.is_empty() {
        return Err(CommandError::failed("Choose at least one field to group."));
    }
    let mut grouped = Vec::new();
    let document = state.edit(DocumentId(id), |document| {
        grouped = forms::group_fields(document, &group, &names)?;
        Ok::<(), FormError>(())
    })?;
    Ok(NamesEdit {
        document,
        names: grouped,
    })
}

/// one field, one answer, at the same place on each of `pages`.
#[tauri::command(async)]
pub fn forms_duplicate_across_pages(
    state: State<AppState>,
    id: u64,
    name: String,
    pages: Vec<u32>,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        forms::duplicate_across_pages(document, &name, &pages)?;
        Ok::<(), FormError>(())
    })
}

/// the value a checkbox, or one radio button, stands for when chosen.
#[tauri::command(async)]
pub fn forms_set_export_value(
    state: State<AppState>,
    id: u64,
    name: String,
    button: usize,
    value: String,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        forms::set_export_value(document, &name, button, &value)
    })
}

/// the answer a field starts with and goes back to when the form is reset.
#[tauri::command(async)]
pub fn forms_set_default_value(
    state: State<AppState>,
    id: u64,
    name: String,
    value: Option<FieldValue>,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        forms::set_default_value(document, &name, value.as_ref())
    })
}

/// puts every field (or only `names`) back to its default answer.
#[tauri::command(async)]
pub fn forms_reset_form(
    state: State<AppState>,
    id: u64,
    names: Option<Vec<String>>,
) -> Result<NamesEdit, CommandError> {
    let mut reset = Vec::new();
    let document = state.edit(DocumentId(id), |document| {
        reset = forms::reset_form(document, names.as_deref()).map_err(|error| error.to_string())?;
        if reset.is_empty() {
            return Err("Every field already has its default answer.".to_owned());
        }
        Ok(())
    })?;
    Ok(NamesEdit {
        document,
        names: reset,
    })
}

/// a reset or print button. there is no submit button (see NO_SUBMIT_NOTE).
#[tauri::command(async)]
pub fn forms_add_button(
    state: State<AppState>,
    id: u64,
    button: NewButton,
) -> Result<NamesEdit, CommandError> {
    let mut name = String::new();
    let document = state.edit(DocumentId(id), |document| {
        name = forms::add_button(document, &button)?;
        Ok::<(), FormError>(())
    })?;
    Ok(NamesEdit {
        document,
        names: vec![name],
    })
}
