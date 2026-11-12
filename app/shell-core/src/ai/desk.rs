//! the ai engine's plan, show, confirm, run steps, held between the ui's
//! calls.
//!
//! the ui asks for a plan and gets the disclosure. the plan waits here,
//! under its disclosure's id, until the ui says Send (or the person had
//! ticked "Don’t ask again" for this provider and document). only then is
//! the engine's acknowledgement made and the plan run. a plan runs once.

use std::collections::HashMap;
use std::sync::Mutex;

use letters_ai::{
    AiError, AiTools, CancelToken, Disclosure, Document, PlannedTask, Progress, Stage, TaskOutput,
    TaskRequest,
};
use serde::Serialize;

use super::consent::ConsentMemory;

/// what the ui is shown about a plan before anything is sent.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub plan_id: u64,
    pub disclosure: Disclosure,
    /// true when the confirmation must be shown: text would leave this
    /// computer and the person has not said yes for this pair before
    pub ask_first: bool,
}

/// a step of a running task, for the panel.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProgressView {
    Step {
        done: usize,
        total: usize,
        stage: &'static str,
    },
    /// the next words of the answer
    Text { text: String },
}

impl From<Progress> for ProgressView {
    fn from(progress: Progress) -> Self {
        match progress {
            Progress::Step { done, total, stage } => ProgressView::Step {
                done,
                total,
                stage: stage_words(stage),
            },
            Progress::Text(text) => ProgressView::Text { text },
        }
    }
}

fn stage_words(stage: Stage) -> &'static str {
    match stage {
        Stage::Searching => "Finding the passages that matter…",
        Stage::Reading => "Reading…",
        Stage::Combining => "Putting the notes together…",
        Stage::Writing => "Writing…",
    }
}

/// an error for the panel: the engine's sentence and whether Try Again
/// (rather than Open Settings) is the way out.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorView {
    pub message: String,
    pub temporary: bool,
}

impl From<AiError> for ErrorView {
    fn from(error: AiError) -> Self {
        ErrorView {
            temporary: error.is_temporary(),
            message: error.to_string(),
        }
    }
}

struct Waiting {
    task: PlannedTask,
    document: String,
}

/// the plans waiting for a yes, the tasks running, and the remembered yeses.
#[derive(Default)]
pub struct AiDesk {
    waiting: Mutex<HashMap<u64, Waiting>>,
    running: Mutex<HashMap<u64, CancelToken>>,
    pub consent: ConsentMemory,
}

impl AiDesk {
    pub fn new() -> Self {
        Self::default()
    }

    /// works out what `request` would send. `document_key` names the
    /// document for the "don't ask again" memory (its file path).
    pub fn plan(
        &self,
        tools: &AiTools,
        document: &Document,
        document_key: &str,
        request: TaskRequest,
    ) -> Result<PlanView, AiError> {
        let task = tools.plan(document, request)?;
        let disclosure = task.disclosure().clone();
        let remembered = self
            .consent
            .is_remembered(&disclosure.destination, document_key);
        let view = PlanView {
            plan_id: disclosure.id.value(),
            ask_first: disclosure.needs_confirmation() && !remembered,
            disclosure,
        };
        lock(&self.waiting).insert(
            view.plan_id,
            Waiting {
                task,
                document: document_key.to_string(),
            },
        );
        Ok(view)
    }

    /// runs a plan. `confirmed` is the person's Send; `remember` is the
    /// "Don’t ask again for this provider and document" box.
    pub fn run(
        &self,
        tools: &AiTools,
        plan_id: u64,
        confirmed: bool,
        remember: bool,
        progress: &mut dyn FnMut(ProgressView),
    ) -> Result<TaskOutput, AiError> {
        let waiting = lock(&self.waiting)
            .remove(&plan_id)
            .ok_or(AiError::ConsentMismatch)?;
        let disclosure = waiting.task.disclosure().clone();
        let remembered = self
            .consent
            .is_remembered(&disclosure.destination, &waiting.document);
        let acknowledgement = if disclosure.needs_confirmation() {
            if !(confirmed || remembered) {
                return Err(AiError::ConsentRequired);
            }
            if confirmed && remember {
                self.consent
                    .remember(&disclosure.destination, &waiting.document);
            }
            Some(disclosure.acknowledge())
        } else {
            None
        };
        let cancel = CancelToken::new();
        lock(&self.running).insert(plan_id, cancel.clone());
        let result = tools.run(
            waiting.task,
            acknowledgement,
            &mut |step| progress(step.into()),
            &cancel,
        );
        lock(&self.running).remove(&plan_id);
        result
    }

    /// Stop: the task ends at its next checkpoint. a plan still waiting
    /// for a yes is dropped.
    pub fn stop(&self, plan_id: u64) {
        if let Some(cancel) = lock(&self.running).get(&plan_id) {
            cancel.cancel();
        }
        lock(&self.waiting).remove(&plan_id);
    }

    /// drops every waiting plan, for when settings change.
    pub fn forget_plans(&self) {
        lock(&self.waiting).clear();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
