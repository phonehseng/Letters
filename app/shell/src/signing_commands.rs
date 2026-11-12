//! signing extras and checking: a trusted time from the time stamp server
//! chosen in Settings, certifying, locking fields, the signing order,
//! validation data, the audit trail, and Verify's full report.
//!
//! every change here covers or extends the file's exact bytes, so like
//! signing it is made on the saved file and written back exactly through
//! the edit session (`change_saved_bytes`, shell-core's forms/signed.rs).
//! the only network use is a time stamp request, which sends a hash to
//! the server the person chose; the ui shows TIMESTAMP_DISCLOSURE first.

use letters_forms::{
    self as forms, AuditEvent, AuditTrailReport, CertificationLevel, DocumentVerification,
    FieldLock, FormResult, SigningOptions, TimestampAuthority, ValidationData, WorkflowStep,
    AUDIT_TRAIL_NOTE, CLOCK_TIME_NOTE, TIMESTAMP_DISCLOSURE, TRUST_NOTE,
};
use letters_pdf::DocumentId;
use letters_shell_core::forms::{password_blocks_signing, sign_saved_document};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::settings_commands::SettingsState;
use crate::state::AppState;

/// the sentences the signing tools show word for word, and what they need
/// to know before anything is sent.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningNotes {
    timestamp_disclosure: &'static str,
    clock_time_note: &'static str,
    audit_trail_note: &'static str,
    trust_note: &'static str,
    /// the server chosen in Settings, empty when there is none
    timestamp_server: String,
    /// the name the audit trail records (the name on comments)
    actor: String,
}

/// the extras chosen in Sign with a Certificate.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SigningChoices {
    /// ask the server chosen in Settings for a trusted time
    trusted_time: bool,
    certify: Option<CertificationLevel>,
    lock: Option<FieldLock>,
    follow_signing_order: bool,
    /// put the certificates the signature carries in the document
    validation_data: bool,
    /// record the signature in the document's audit trail
    audit_trail: bool,
}

/// Verify's report: the engine's, and the notes it is read with.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    #[serde(flatten)]
    report: DocumentVerification,
    trust_note: &'static str,
}

fn timestamp_authority(settings: &SettingsState) -> Result<TimestampAuthority, CommandError> {
    let url = settings
        .store
        .get()
        .signing
        .timestamp_server
        .trim()
        .to_owned();
    if url.is_empty() {
        return Err(CommandError::failed(
            "Choose a time stamp server under Settings, Signing first. Without one, the signature uses this computer’s clock.",
        ));
    }
    Ok(TimestampAuthority {
        url,
        username: None,
        password: None,
    })
}

/// the engine's options for what was chosen.
pub fn signing_options(
    state: &AppState,
    settings: &SettingsState,
    choices: &SigningChoices,
) -> Result<SigningOptions, CommandError> {
    let timestamp = match choices.trusted_time {
        true => Some(timestamp_authority(settings)?),
        false => None,
    };
    Ok(SigningOptions {
        timestamp,
        certify: choices.certify,
        lock: choices.lock.clone(),
        validation: choices.validation_data.then(ValidationData::default),
        follow_signing_order: choices.follow_signing_order,
        audit_actor: choices.audit_trail.then(|| state.author_name()),
    })
}

/// changes the saved file's exact bytes with `change` and makes the result
/// the saved file and what is shown, as signing does. refused while there
/// are unsaved changes or a password, with the same sentences.
pub fn change_saved_bytes(
    state: &AppState,
    id: u64,
    change: impl FnOnce(&[u8]) -> FormResult<Vec<u8>>,
) -> Result<DocumentState, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let password_blocked = password_blocks_signing(&state.protection.get(id));
    let (_, document) = state
        .documents
        .with_session(worker, DocumentId(id), |session, show| {
            let shown = sign_saved_document(session, password_blocked, change, |path| show(path))
                .map_err(|error| CommandError::failed(error.to_string()))?;
            Ok(((), Some(shown)))
        })?;
    Ok(document)
}

/// the bytes of what is shown now, for checks that read the whole file.
fn shown_bytes(state: &AppState, id: u64) -> Result<Vec<u8>, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let (bytes, _) = state
        .documents
        .with_session(worker, DocumentId(id), |session, _| {
            let bytes = std::fs::read(session.working_copy())
                .map_err(|error| CommandError::failed(error.to_string()))?;
            Ok((bytes, None))
        })?;
    Ok(bytes)
}

#[tauri::command(async)]
pub fn forms_signing_notes(state: State<AppState>, settings: State<SettingsState>) -> SigningNotes {
    SigningNotes {
        timestamp_disclosure: TIMESTAMP_DISCLOSURE,
        clock_time_note: CLOCK_TIME_NOTE,
        audit_trail_note: AUDIT_TRAIL_NOTE,
        trust_note: TRUST_NOTE,
        timestamp_server: settings.store.get().signing.timestamp_server,
        actor: state.author_name(),
    }
}

/// the signature fields in signing order, who has signed and who is next.
#[tauri::command(async)]
pub fn forms_signing_workflow(
    state: State<AppState>,
    id: u64,
) -> Result<Vec<WorkflowStep>, CommandError> {
    state.read_document(DocumentId(id), |document| {
        Ok(forms::signing_workflow(document)?)
    })
}

#[tauri::command(async)]
pub fn forms_set_signing_order(
    state: State<AppState>,
    id: u64,
    fields: Vec<String>,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        forms::set_signing_order(document, &fields)
    })
}

/// the certificates every signature carries, stored in the document so it
/// can be checked later. nothing is fetched.
#[tauri::command(async)]
pub fn forms_add_validation_data(
    state: State<AppState>,
    id: u64,
) -> Result<DocumentState, CommandError> {
    change_saved_bytes(&state, id, |bytes| {
        forms::add_validation_data(bytes, &ValidationData::default())
    })
}

/// every signature with its verdict, time source, certification and the
/// changes of each revision.
#[tauri::command(async)]
pub fn forms_verify_document(
    state: State<AppState>,
    id: u64,
) -> Result<Verification, CommandError> {
    let bytes = shown_bytes(&state, id)?;
    Ok(Verification {
        report: forms::verify_document(&bytes)?,
        trust_note: TRUST_NOTE,
    })
}

#[tauri::command(async)]
pub fn forms_audit_trail(
    state: State<AppState>,
    id: u64,
) -> Result<AuditTrailReport, CommandError> {
    let bytes = shown_bytes(&state, id)?;
    Ok(forms::audit_trail(&bytes)?)
}

/// adds an entry to the audit trail (starting it), under the name the
/// person set in Letters.
#[tauri::command(async)]
pub fn forms_append_audit_entry(
    state: State<AppState>,
    id: u64,
    event: AuditEvent,
) -> Result<DocumentState, CommandError> {
    let actor = state.author_name();
    change_saved_bytes(&state, id, |bytes| {
        forms::append_audit_entry(bytes, &actor, &event)
    })
}

/// readable pages listing the trail, added at the end. they are page
/// content, which a certification forbids, so the ui offers them before.
#[tauri::command(async)]
pub fn forms_add_audit_pages(
    state: State<AppState>,
    id: u64,
) -> Result<DocumentState, CommandError> {
    change_saved_bytes(&state, id, forms::add_audit_page)
}
