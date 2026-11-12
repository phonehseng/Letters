//! Fill & Sign's form logic: filling a field the way a form app does
//! (its format, its checks, then every calculation again) and setting a
//! field's calculation, format and checks in Prepare Form.
//!
//! each change is one call through `state.edit`, so it is one undo step
//! in the working copy. the engine (core/forms' logic) reads and writes
//! the standard acrobat functions; Letters never runs any other script.

use letters_forms::{
    self as forms, ChangedField, FieldLogic, FieldLogicChanges, FieldValue, FormError, FormLogic,
    CONDITIONAL_VISIBILITY_NOTE, NO_SUBMIT_NOTE,
};
use letters_pdf::DocumentId;
use letters_shell_core::forms::unchanged_drawings::fill_field_keeping_drawings;
use serde::Serialize;
use tauri::State;

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

/// the form's logic, with the two things Prepare Form says it doesn't offer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormLogicView {
    #[serde(flatten)]
    logic: FormLogic,
    no_submit_note: &'static str,
    conditional_visibility_note: &'static str,
}

/// what filling a field changed: the field itself, then each calculated
/// field worked out again, with the value as stored and as shown.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilledField {
    document: DocumentState,
    changed: Vec<ChangedField>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicEdit {
    document: DocumentState,
    logic: FieldLogic,
}

/// every field's calculation, format and checks, as the engine reads them.
#[tauri::command(async)]
pub fn forms_logic(state: State<AppState>, id: u64) -> Result<FormLogicView, CommandError> {
    state.read_document(DocumentId(id), |document| {
        Ok(FormLogicView {
            logic: forms::form_logic(document)?,
            no_submit_note: NO_SUBMIT_NOTE,
            conditional_visibility_note: CONDITIONAL_VISIBILITY_NOTE,
        })
    })
}

/// fills one field through its format and checks, then recalculates. a
/// refused answer is the form's own message (or a plain sentence) and
/// changes nothing.
#[tauri::command(async)]
pub fn forms_fill_field(
    state: State<AppState>,
    id: u64,
    name: String,
    value: FieldValue,
) -> Result<FilledField, CommandError> {
    let mut changed = Vec::new();
    let document = state.edit(DocumentId(id), |document| {
        // fields drawn again exactly as they were are left alone, so a signed
        // form's save lists only what really changed (shell-core's
        // forms/unchanged_drawings.rs)
        changed = fill_field_keeping_drawings(document, &name, &value)?.changed;
        Ok::<(), FormError>(())
    })?;
    Ok(FilledField { document, changed })
}

/// replaces a field's calculation, format and checks.
#[tauri::command(async)]
pub fn forms_set_field_logic(
    state: State<AppState>,
    id: u64,
    name: String,
    changes: FieldLogicChanges,
) -> Result<LogicEdit, CommandError> {
    let mut logic = None;
    let document = state.edit(DocumentId(id), |document| {
        logic = Some(forms::set_field_logic(document, &name, &changes)?);
        Ok::<(), FormError>(())
    })?;
    let logic = logic.ok_or_else(|| CommandError::failed("The rules couldn’t be read back."))?;
    Ok(LogicEdit { document, logic })
}

/// works every calculation out again, for a form opened with stale totals.
#[tauri::command(async)]
pub fn forms_recalculate(state: State<AppState>, id: u64) -> Result<FilledField, CommandError> {
    let mut changed = Vec::new();
    let document = state.edit(DocumentId(id), |document| {
        changed = forms::recalculate(document)
            .map_err(|error| error.to_string())?
            .changed;
        if changed.is_empty() {
            return Err("Every calculated field is already up to date.".to_owned());
        }
        Ok(())
    })?;
    Ok(FilledField { document, changed })
}
