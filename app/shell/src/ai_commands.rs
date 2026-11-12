//! the optional AI Tools' commands, and "Find a Topic", which works on
//! this computer without them.
//!
//! every request is planned first (`ai_plan`), and the ui shows the
//! engine's disclosure; `ai_run` runs only that plan, and only with the
//! person's Send when text would leave the computer (see
//! shell-core/src/ai/desk.rs). the answer streams to the window as
//! "ai-progress" events.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use letters_ai::{
    find_where_offline, AiTools, CancelToken, Document, FoundPassage, LaunchOptions, PageRange,
    ServerProcess, TaskOutput, TaskRequest,
};
use letters_convert::{analyze_structure, CancelFlag, PdfSource, StructureOptions};
use letters_pdf::DocumentId;
use letters_shell_core::ai::connection::test_connection;
use letters_shell_core::ai::desk::{AiDesk, ErrorView, PlanView, ProgressView};
use letters_shell_core::ai::document_text::ai_document;
use letters_shell_core::ai::keys::{save_typed_key, SystemKeyStore};
use letters_shell_core::settings_store::AppSettings;
use serde::Serialize;
use tauri::{Emitter, State, Window};

use crate::settings_commands::SettingsState;
use crate::state::AppState;

/// the credential store entry Letters keeps ai keys under.
const KEY_SERVICE: &str = "Letters AI Tools";

/// what the AI Tools remember while the app runs.
pub struct AiState {
    desk: AiDesk,
    keys: SystemKeyStore,
    /// each document's text, by document and revision, so asking twice
    /// doesn't read the pdf twice
    texts: Mutex<HashMap<(u64, u64), Arc<Document>>>,
    /// a model server Letters started, stopped when the app quits
    server: Mutex<Option<ServerProcess>>,
}

impl Default for AiState {
    fn default() -> Self {
        AiState {
            desk: AiDesk::new(),
            keys: SystemKeyStore::new(KEY_SERVICE),
            texts: Mutex::new(HashMap::new()),
            server: Mutex::new(None),
        }
    }
}

impl AiState {
    /// the settings changed: plans made under the old ones are dropped,
    /// and so are "don't ask again" answers.
    pub fn settings_changed(&self) {
        self.desk.forget_plans();
        self.desk.consent.forget_all();
    }

    /// ends a server Letters started; called when the app quits.
    pub fn stop_server(&self) {
        if let Some(server) = self.server.lock().unwrap().take() {
            server.stop();
        }
    }
}

fn tools(settings: &AppSettings, ai: &AiState) -> Result<AiTools, ErrorView> {
    AiTools::from_settings(&settings.ai, &ai.keys).map_err(ErrorView::from)
}

/// the document's text as the engine reads it, and its file path (the
/// key for "don't ask again").
fn document_text(
    state: &AppState,
    ai: &AiState,
    id: u64,
) -> Result<(Arc<Document>, String), ErrorView> {
    let plain = |message: String| ErrorView {
        message,
        temporary: false,
    };
    let worker = state.worker().map_err(plain)?;
    let ((working, original, revision), _) = state
        .documents
        .with_session(worker, DocumentId(id), |session, _| {
            Ok((
                (
                    session.working_copy().to_path_buf(),
                    session.original_path().to_string_lossy().into_owned(),
                    session.revision(),
                ),
                None,
            ))
        })
        .map_err(|_| plain("This document is no longer open.".into()))?;
    if let Some(text) = ai.texts.lock().unwrap().get(&(id, revision)) {
        return Ok((text.clone(), original));
    }
    let source = PdfSource::open(&working).map_err(|error| plain(error.to_string()))?;
    let structure = analyze_structure(
        &source,
        &StructureOptions::default(),
        &mut |_| {},
        &CancelFlag::new(),
    )
    .map_err(|error| plain(error.to_string()))?;
    let text = Arc::new(ai_document(&structure));
    let mut texts = ai.texts.lock().unwrap();
    texts.retain(|(document, _), _| *document != id);
    texts.insert((id, revision), text.clone());
    Ok((text, original))
}

/// the header of the panel: which provider, and whether text leaves.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub enabled: bool,
    /// set up and ready to plan requests
    pub ready: bool,
    pub provider_name: Option<String>,
    pub destination: Option<String>,
    pub leaves_device: bool,
    /// why it isn't ready, in a sentence
    pub problem: Option<String>,
}

#[tauri::command(async)]
pub fn ai_status(settings: State<SettingsState>, ai: State<AiState>) -> AiStatus {
    let current = settings.store.get();
    if !current.ai.enabled {
        return AiStatus {
            enabled: false,
            ready: false,
            provider_name: None,
            destination: None,
            leaves_device: false,
            problem: None,
        };
    }
    match tools(&current, &ai) {
        Ok(tools) => {
            let info = tools.provider_info();
            AiStatus {
                enabled: true,
                ready: true,
                provider_name: Some(info.name),
                destination: Some(info.host),
                leaves_device: info.sends_content_off_device,
                problem: None,
            }
        }
        Err(error) => AiStatus {
            enabled: true,
            ready: false,
            provider_name: None,
            destination: None,
            leaves_device: false,
            problem: Some(error.message),
        },
    }
}

#[tauri::command(async)]
pub fn ai_plan(
    state: State<AppState>,
    settings: State<SettingsState>,
    ai: State<AiState>,
    id: u64,
    request: TaskRequest,
) -> Result<PlanView, ErrorView> {
    let tools = tools(&settings.store.get(), &ai)?;
    let (text, key) = document_text(&state, &ai, id)?;
    ai.desk
        .plan(&tools, &text, &key, request)
        .map_err(ErrorView::from)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiProgressEvent {
    plan_id: u64,
    #[serde(flatten)]
    progress: ProgressView,
}

/// runs a plan. `confirmed` is the person's Send in the confirmation;
/// `remember` is its "Don’t ask again for this provider and document".
#[tauri::command(async)]
pub fn ai_run(
    window: Window,
    settings: State<SettingsState>,
    ai: State<AiState>,
    plan_id: u64,
    confirmed: bool,
    remember: bool,
) -> Result<TaskOutput, ErrorView> {
    let tools = tools(&settings.store.get(), &ai)?;
    let label = window.label().to_string();
    ai.desk
        .run(&tools, plan_id, confirmed, remember, &mut |progress| {
            let _ = window.emit_to(
                label.as_str(),
                "ai-progress",
                AiProgressEvent { plan_id, progress },
            );
        })
        .map_err(ErrorView::from)
}

#[tauri::command(async)]
pub fn ai_stop(ai: State<AiState>, plan_id: u64) {
    ai.desk.stop(plan_id);
}

/// "Find a Topic": word matches on this computer, no provider, no ai.
#[tauri::command(async)]
pub fn ai_find_offline(
    state: State<AppState>,
    ai: State<AiState>,
    id: u64,
    topic: String,
    pages: Option<PageRange>,
) -> Result<Vec<FoundPassage>, ErrorView> {
    let (text, _) = document_text(&state, &ai, id)?;
    find_where_offline(&text, &topic, pages, 8).map_err(ErrorView::from)
}

/// Settings' Test Connection: one fixed sentence, no document text.
#[tauri::command(async)]
pub fn ai_test_connection(
    settings: State<SettingsState>,
    ai: State<AiState>,
) -> Result<String, ErrorView> {
    let tools = tools(&settings.store.get(), &ai)?;
    test_connection(&tools, &CancelToken::new()).map_err(ErrorView::from)
}

/// keeps a typed key in the system's credential store (an empty one
/// forgets it). returns whether a key is now kept.
#[tauri::command(async)]
pub fn ai_save_key(
    settings: State<SettingsState>,
    ai: State<AiState>,
    key: String,
) -> Result<bool, ErrorView> {
    let name = key_name(&settings.store.get());
    let kept = save_typed_key(&ai.keys, &name, &key).map_err(ErrorView::from)?;
    ai.settings_changed();
    Ok(kept)
}

#[tauri::command(async)]
pub fn ai_has_key(settings: State<SettingsState>, ai: State<AiState>) -> bool {
    use letters_ai::KeyStore;
    let name = key_name(&settings.store.get());
    matches!(ai.keys.load(&name), Ok(Some(_)))
}

fn key_name(settings: &AppSettings) -> String {
    match &settings.ai.provider {
        Some(letters_ai::ProviderConfig::RemoteApi(config)) => config.key_name.clone(),
        _ => letters_ai::DEFAULT_KEY_NAME.to_string(),
    }
}

/// starts the model server program the person chose, on 127.0.0.1, and
/// points the local provider at it. returns its address.
#[tauri::command(async)]
pub fn ai_start_server(
    settings: State<SettingsState>,
    ai: State<AiState>,
) -> Result<String, ErrorView> {
    ai.stop_server();
    let current = settings.store.get();
    let mut options =
        LaunchOptions::new(&current.ai_server.executable, &current.ai_server.model_file);
    if let Some(letters_ai::ProviderConfig::LocalServer(config)) = &current.ai.provider {
        options.context_tokens = Some(config.context_tokens);
    }
    let server = ServerProcess::start(&options, &CancelToken::new()).map_err(ErrorView::from)?;
    let address = server.address();
    let config = server.config();
    *ai.server.lock().unwrap() = Some(server);
    settings
        .store
        .update(|stored| {
            if let Some(letters_ai::ProviderConfig::LocalServer(local)) = &mut stored.ai.provider {
                local.address = config.address.clone();
            } else {
                stored.ai.provider = Some(letters_ai::ProviderConfig::LocalServer(config));
            }
        })
        .map_err(|message| ErrorView {
            message,
            temporary: false,
        })?;
    ai.settings_changed();
    Ok(address)
}

#[tauri::command(async)]
pub fn ai_stop_server(ai: State<AiState>) {
    ai.stop_server();
}

#[tauri::command(async)]
pub fn ai_server_running(ai: State<AiState>) -> bool {
    ai.server
        .lock()
        .unwrap()
        .as_mut()
        .is_some_and(|server| server.is_running())
}
