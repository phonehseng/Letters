//! Fill & Sign: form fields and signatures, one command per operation
//! from core/forms.
//!
//! filling, adding, changing, removing and flattening fields are one
//! call through `state.edit` each, exactly like the page tools: one undo
//! step, and only the working copy changes until Save. finding fields
//! and reading the form go through `state.read_document` and change
//! nothing.
//!
//! signing is the exception. a signature covers the file's exact bytes,
//! so it is made on the saved file and written back exactly through the
//! edit session (see shell-core's forms/signed.rs), never through lopdf.
//!
//! everything happens on this computer: no server, no time stamps, no
//! record of who opened or filled a form.

use std::path::Path;
use std::sync::Mutex;

use letters_forms::{
    self as forms, CertificateRequest, DetectedField, FieldChanges, FieldInfo, FieldValue,
    FlattenOptions, FormError, NewField, SignRequest, SignatureAppearance, SignatureImage,
    SignaturePlacement, SignatureReport, VisibleRect, TRUST_NOTE,
};
use letters_pdf::DocumentId;
use letters_shell_core::forms::{
    data_url, has_calculations, password_blocks_signing, picture_mime, read_chosen_file,
    sign_saved_document, write_chosen_bytes, PictureKind, SavedSignature, SignaturePictures,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

/// a certificate file is a few kilobytes; anything bigger is the wrong file.
const MAX_CERTIFICATE_BYTES: u64 = 1024 * 1024;
/// a photo of a signature can be large; it is shrunk in the ui.
const MAX_PICTURE_BYTES: u64 = 20 * 1024 * 1024;

/// what Fill & Sign keeps while the app runs: the kept signature pictures.
pub struct FormsState {
    pictures: Mutex<SignaturePictures>,
}

impl FormsState {
    pub fn start(app: &AppHandle) -> Self {
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        Self {
            pictures: Mutex::new(SignaturePictures::new(data_dir.join("signatures"))),
        }
    }

    fn pictures(&self) -> std::sync::MutexGuard<'_, SignaturePictures> {
        self.pictures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl From<FormError> for CommandError {
    fn from(error: FormError) -> Self {
        CommandError::failed(error.to_string())
    }
}

/// everything the Fill & Sign tab shows about a document, read once per
/// revision.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormsOverview {
    fields: Vec<FieldInfo>,
    /// why the form can't be filled (an xfa form, say), as the engine words it
    problem: Option<String>,
    /// the form expects scripts to calculate answers, which Letters never runs
    has_calculations: bool,
    signatures: Vec<SignatureReport>,
    /// the saved file is signed, so Save won't write a changed version over it
    protected: bool,
    /// the engine's sentence about what checking a signature does and doesn't mean
    trust_note: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldsEdit {
    document: DocumentState,
    /// the names the fields were given (a name in use gets "_2" added)
    names: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldEdit {
    document: DocumentState,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlattenEdit {
    document: DocumentState,
    flattened: Vec<String>,
}

/// the proposals for one page.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageProposals {
    page: u32,
    fields: Vec<DetectedField>,
}

/// a kept signature with its picture, ready for an `<img>`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignaturePicture {
    #[serde(flatten)]
    saved: SavedSignature,
    picture: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateMade {
    path: String,
    subject: String,
    valid_from: String,
    valid_until: String,
    note: String,
}

/// what the certificate sheet sends to sign.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignSettings {
    certificate_path: String,
    password: String,
    placement: SignaturePlacement,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    location: Option<String>,
    /// a kept signature picture to show in the visible signature
    #[serde(default)]
    signature_id: Option<String>,
    #[serde(default = "yes")]
    show_details: bool,
}

fn yes() -> bool {
    true
}

/// the form, its signatures and what Letters can't do with it.
#[tauri::command(async)]
pub fn forms_overview(state: State<AppState>, id: u64) -> Result<FormsOverview, CommandError> {
    let document_id = DocumentId(id);
    let (fields, problem, calculates) = state.read_document(document_id, |document| {
        let calculates = has_calculations(document);
        Ok(match forms::fields(document) {
            Ok(fields) => (fields, None, calculates),
            Err(error) => (Vec::new(), Some(error.to_string()), calculates),
        })
    })?;
    // checking signatures reads the whole file, so only signed forms pay for it
    let any_signed = fields.iter().any(|field| field.signed);
    let (signatures, protected) = if any_signed {
        signature_reports(&state, document_id)?
    } else {
        (Vec::new(), false)
    };
    Ok(FormsOverview {
        fields,
        problem,
        has_calculations: calculates,
        signatures,
        protected,
        trust_note: TRUST_NOTE,
    })
}

/// checks the signatures of what is shown. a signed file opened without
/// changes gets the session's guard, so Save never rewrites it.
fn signature_reports(
    state: &AppState,
    id: DocumentId,
) -> Result<(Vec<SignatureReport>, bool), CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let (answer, _) = state.documents.with_session(worker, id, |session, _| {
        let bytes = std::fs::read(session.working_copy())
            .map_err(|error| CommandError::failed(error.to_string()))?;
        let reports = forms::verify(&bytes).unwrap_or_default();
        if !session.is_dirty() && !reports.is_empty() {
            session.protect_original(true);
        }
        Ok(((reports, session.original_is_protected()), None))
    })?;
    Ok(answer)
}

/// fills one field. the engine checks the answer first and says what is wrong.
#[tauri::command(async)]
pub fn forms_set_value(
    state: State<AppState>,
    id: u64,
    name: String,
    value: FieldValue,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        forms::set_value(document, &name, &value)
    })
}

/// adds fields, all in one undo step (Add Fields after Find Fields adds many).
#[tauri::command(async)]
pub fn forms_add_fields(
    state: State<AppState>,
    id: u64,
    fields: Vec<NewField>,
) -> Result<FieldsEdit, CommandError> {
    if fields.is_empty() {
        return Err(CommandError::failed("Choose at least one field to add."));
    }
    let mut names = Vec::new();
    let document = state.edit(DocumentId(id), |document| {
        for field in &fields {
            names.push(forms::add_field(document, field)?);
        }
        Ok::<(), FormError>(())
    })?;
    Ok(FieldsEdit { document, names })
}

/// renames, moves, resizes or changes the settings of one field.
#[tauri::command(async)]
pub fn forms_update_field(
    state: State<AppState>,
    id: u64,
    name: String,
    changes: FieldChanges,
) -> Result<FieldEdit, CommandError> {
    let mut new_name = name.clone();
    let document = state.edit(DocumentId(id), |document| {
        new_name = forms::update_field(document, &name, &changes)?;
        Ok::<(), FormError>(())
    })?;
    Ok(FieldEdit {
        document,
        name: new_name,
    })
}

#[tauri::command(async)]
pub fn forms_remove_field(
    state: State<AppState>,
    id: u64,
    name: String,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        forms::remove_field(document, &name)
    })
}

/// proposes fields on flat forms. changes nothing; the person confirms each.
#[tauri::command(async)]
pub fn forms_detect_fields(
    state: State<AppState>,
    id: u64,
    pages: Vec<u32>,
) -> Result<Vec<PageProposals>, CommandError> {
    state.read_document(DocumentId(id), |document| {
        pages
            .iter()
            .map(|page| {
                let fields = forms::detect_fields(document, *page)?;
                Ok(PageProposals {
                    page: *page,
                    fields,
                })
            })
            .collect()
    })
}

/// paints every field into its page. when there is nothing to flatten it
/// says so instead of making an undo step that changes nothing.
#[tauri::command(async)]
pub fn forms_flatten(
    state: State<AppState>,
    id: u64,
    include_signatures: bool,
) -> Result<FlattenEdit, CommandError> {
    let mut flattened = Vec::new();
    let options = FlattenOptions {
        fields: None,
        include_signatures,
    };
    let document = state.edit(DocumentId(id), |document| {
        flattened = forms::flatten(document, &options)
            .map_err(|error| error.to_string())?
            .flattened;
        if flattened.is_empty() {
            return Err("There are no form fields to flatten.".to_owned());
        }
        Ok(())
    })?;
    Ok(FlattenEdit {
        document,
        flattened,
    })
}

/// puts a kept signature picture on a page, as a picture annotation so it
/// can still be moved and resized. a picture proves nothing about who
/// placed it; Sign with a Certificate does that.
#[tauri::command(async)]
pub fn forms_place_signature(
    state: State<AppState>,
    forms_state: State<FormsState>,
    id: u64,
    page: u32,
    rect: VisibleRect,
    signature_id: String,
) -> Result<DocumentState, CommandError> {
    let png = forms_state
        .pictures()
        .read(&signature_id)
        .map_err(CommandError::failed)?;
    let request = SignatureImage {
        page,
        rect,
        png,
        as_annotation: true,
    };
    state.edit(DocumentId(id), |document| {
        forms::place_signature_image(document, &request)
    })
}

/// the kept signature pictures, newest first.
#[tauri::command(async)]
pub fn forms_signature_pictures(forms_state: State<FormsState>) -> Vec<SignaturePicture> {
    let pictures = forms_state.pictures();
    pictures
        .list()
        .into_iter()
        .filter_map(|saved| {
            let png = pictures.read(&saved.id).ok()?;
            Some(SignaturePicture {
                picture: data_url("image/png", &png),
                saved,
            })
        })
        .collect()
}

/// keeps a signature picture the ui made (drawn, typed or from a file).
#[tauri::command(async)]
pub fn forms_keep_signature_picture(
    forms_state: State<FormsState>,
    png: Vec<u8>,
    kind: PictureKind,
) -> Result<SignaturePicture, CommandError> {
    let saved = forms_state
        .pictures()
        .add(&png, kind)
        .map_err(CommandError::failed)?;
    Ok(SignaturePicture {
        picture: data_url("image/png", &png),
        saved,
    })
}

#[tauri::command(async)]
pub fn forms_delete_signature_picture(
    forms_state: State<FormsState>,
    signature_id: String,
) -> Result<(), CommandError> {
    forms_state
        .pictures()
        .delete(&signature_id)
        .map_err(CommandError::failed)
}

/// a png or jpeg the person chose, as a data address the ui can draw
/// (and make the white of transparent) before keeping it.
#[tauri::command(async)]
pub fn forms_read_picture(path: String) -> Result<String, CommandError> {
    let path = Path::new(&path);
    let mime = picture_mime(path)
        .ok_or_else(|| CommandError::failed("Choose a PNG or JPEG picture of your signature."))?;
    let bytes = read_chosen_file(path, MAX_PICTURE_BYTES).map_err(CommandError::failed)?;
    Ok(data_url(mime, &bytes))
}

/// makes a self-signed certificate and saves it where the save panel said.
#[tauri::command(async)]
pub fn forms_create_certificate(
    request: CertificateRequest,
    path: String,
) -> Result<CertificateMade, CommandError> {
    let made = forms::create_self_signed_certificate(&request)?;
    let path = with_p12_extension(Path::new(&path));
    write_chosen_bytes(&path, &made.pkcs12).map_err(CommandError::failed)?;
    Ok(CertificateMade {
        path: path.to_string_lossy().into_owned(),
        subject: made.subject,
        valid_from: made.valid_from,
        valid_until: made.valid_until,
        note: made.note,
    })
}

/// signs the saved document with a certificate file. the password is
/// used for this one signature and never kept.
#[tauri::command(async)]
pub fn forms_sign(
    state: State<AppState>,
    forms_state: State<FormsState>,
    id: u64,
    settings: SignSettings,
) -> Result<DocumentState, CommandError> {
    let pkcs12 = read_chosen_file(Path::new(&settings.certificate_path), MAX_CERTIFICATE_BYTES)
        .map_err(CommandError::failed)?;
    let image_png = match &settings.signature_id {
        Some(signature) => Some(
            forms_state
                .pictures()
                .read(signature)
                .map_err(CommandError::failed)?,
        ),
        None => None,
    };
    let request = SignRequest {
        pkcs12,
        password: settings.password,
        placement: settings.placement,
        reason: settings.reason.filter(|reason| !reason.trim().is_empty()),
        location: settings
            .location
            .filter(|location| !location.trim().is_empty()),
        name: None,
        contact_info: None,
        appearance: SignatureAppearance {
            image_png,
            show_details: settings.show_details,
        },
    };
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    // a password set under Protect (or one the file still has) can't go with a signature yet
    let password_blocked = password_blocks_signing(&state.protection.get(id));
    let (_, document) = state
        .documents
        .with_session(worker, DocumentId(id), |session, show| {
            let shown = sign_saved_document(
                session,
                password_blocked,
                |bytes| forms::sign(bytes, &request),
                |path| show(path),
            )
            .map_err(|error| CommandError::failed(error.to_string()))?;
            Ok(((), Some(shown)))
        })?;
    Ok(document)
}

/// after Save a Copy: the copy is the person's to change, so its guard goes.
#[tauri::command(async)]
pub fn forms_release_signature_guard(state: State<AppState>, id: u64) -> Result<(), CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    state
        .documents
        .with_session(worker, DocumentId(id), |session, _| {
            session.protect_original(false);
            Ok(((), None))
        })?;
    Ok(())
}

/// adds ".p12" to a certificate name typed without it.
fn with_p12_extension(path: &Path) -> std::path::PathBuf {
    let known = path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("p12") || extension.eq_ignore_ascii_case("pfx")
    });
    if known {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".p12");
    std::path::PathBuf::from(name)
}
