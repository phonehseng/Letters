//! the plan, confirm, run steps with a provider that answers from memory,
//! so nothing is ever sent anywhere.

use std::sync::Mutex;

use letters_ai::{
    AiError, AiTools, CancelToken, ChatRequest, Completion, FinishReason, Permit, Provider,
    ProviderInfo, ProviderKind, SummaryLength, TaskOutput, TaskRequest,
};

use super::connection::{test_connection, TEST_SENTENCE};
use super::desk::{AiDesk, ProgressView};
use super::document_text::ai_document;
use crate::export::easy_edit::tests::handbook;

/// a provider that answers every request with the same words and keeps
/// what it was sent.
struct Fake {
    leaves_device: bool,
    sent: Mutex<Vec<String>>,
}

impl Fake {
    fn new(leaves_device: bool) -> Fake {
        Fake {
            leaves_device,
            sent: Mutex::new(Vec::new()),
        }
    }
}

impl Provider for Fake {
    fn describe(&self) -> ProviderInfo {
        ProviderInfo {
            name: if self.leaves_device {
                "Online service".into()
            } else {
                "Model on this computer".into()
            },
            kind: if self.leaves_device {
                ProviderKind::Remote
            } else {
                ProviderKind::Local
            },
            sends_content_off_device: self.leaves_device,
            host: if self.leaves_device {
                "api.example.com".into()
            } else {
                "127.0.0.1".into()
            },
            context_tokens: 8192,
            supports_embeddings: false,
            thinking_tokens: 0,
        }
    }

    fn complete(
        &self,
        request: &ChatRequest,
        permit: &Permit,
        on_text: &mut dyn FnMut(&str),
        _cancel: &CancelToken,
    ) -> Result<Completion, AiError> {
        permit.check(&self.describe())?;
        self.sent.lock().unwrap().push(request.user().to_string());
        let text = "The handbook explains how to read well.";
        on_text(text);
        Ok(Completion {
            text: text.into(),
            usage: None,
            finish: FinishReason::Stop,
        })
    }

    fn embed(
        &self,
        _texts: &[String],
        _permit: &Permit,
        _cancel: &CancelToken,
    ) -> Result<Option<Vec<Vec<f32>>>, AiError> {
        Ok(None)
    }
}

fn summarize() -> TaskRequest {
    TaskRequest::Summarize {
        pages: None,
        length: SummaryLength::Short,
    }
}

#[test]
fn text_that_leaves_needs_a_send_first() {
    let tools = AiTools::with_provider(Box::new(Fake::new(true)));
    let desk = AiDesk::new();
    let document = ai_document(&handbook());
    let plan = desk
        .plan(&tools, &document, "C:/handbook.pdf", summarize())
        .unwrap();
    assert!(plan.ask_first);
    assert!(
        plan.disclosure
            .sentence
            .starts_with("This will send the text of pages 1–2"),
        "{}",
        plan.disclosure.sentence
    );
    assert!(plan.disclosure.sentence.ends_with("to api.example.com."));
    // without the person's Send nothing runs, and the plan is used up
    assert_eq!(
        desk.run(&tools, plan.plan_id, false, false, &mut |_| {}),
        Err(AiError::ConsentRequired)
    );
    let again = desk
        .plan(&tools, &document, "C:/handbook.pdf", summarize())
        .unwrap();
    let mut streamed = String::new();
    let output = desk
        .run(&tools, again.plan_id, true, false, &mut |step| {
            if let ProgressView::Text { text } = step {
                streamed.push_str(&text);
            }
        })
        .unwrap();
    assert!(matches!(output, TaskOutput::Summary(_)));
    assert!(streamed.contains("read well"));
}

#[test]
fn dont_ask_again_is_for_one_provider_and_document() {
    let tools = AiTools::with_provider(Box::new(Fake::new(true)));
    let desk = AiDesk::new();
    let document = ai_document(&handbook());
    let first = desk
        .plan(&tools, &document, "C:/a.pdf", summarize())
        .unwrap();
    desk.run(&tools, first.plan_id, true, true, &mut |_| {})
        .unwrap();
    // the same document again: no question
    let second = desk
        .plan(&tools, &document, "C:/a.pdf", summarize())
        .unwrap();
    assert!(!second.ask_first);
    assert!(desk
        .run(&tools, second.plan_id, false, false, &mut |_| {})
        .is_ok());
    // another document asks again
    let other = desk
        .plan(&tools, &document, "C:/b.pdf", summarize())
        .unwrap();
    assert!(other.ask_first);
}

#[test]
fn a_model_on_this_computer_never_asks() {
    let tools = AiTools::with_provider(Box::new(Fake::new(false)));
    let desk = AiDesk::new();
    let plan = desk
        .plan(&tools, &ai_document(&handbook()), "C:/a.pdf", summarize())
        .unwrap();
    assert!(!plan.ask_first);
    assert_eq!(plan.disclosure.sentence, "This stays on this computer.");
    assert!(desk
        .run(&tools, plan.plan_id, false, false, &mut |_| {})
        .is_ok());
}

#[test]
fn a_plan_runs_once_and_stop_drops_a_waiting_one() {
    let tools = AiTools::with_provider(Box::new(Fake::new(false)));
    let desk = AiDesk::new();
    let document = ai_document(&handbook());
    let plan = desk
        .plan(&tools, &document, "C:/a.pdf", summarize())
        .unwrap();
    desk.run(&tools, plan.plan_id, false, false, &mut |_| {})
        .unwrap();
    assert_eq!(
        desk.run(&tools, plan.plan_id, false, false, &mut |_| {}),
        Err(AiError::ConsentMismatch)
    );
    let waiting = desk
        .plan(&tools, &document, "C:/a.pdf", summarize())
        .unwrap();
    desk.stop(waiting.plan_id);
    assert!(desk
        .run(&tools, waiting.plan_id, true, false, &mut |_| {})
        .is_err());
}

#[test]
fn test_connection_sends_only_the_fixed_sentence() {
    let fake = Fake::new(true);
    let tools = AiTools::with_provider(Box::new(fake));
    let sentence = test_connection(&tools, &CancelToken::new()).unwrap();
    assert!(sentence.starts_with("Connected to api.example.com."));
    // the provider is inside the tools now; plan again to see what goes
    let plan = tools
        .plan(
            &letters_ai::Document {
                pages: vec![letters_ai::Page {
                    number: 1,
                    paragraphs: vec![letters_ai::Paragraph {
                        text: TEST_SENTENCE.into(),
                        rect: letters_ai::Rect::default(),
                    }],
                }],
            },
            summarize(),
        )
        .unwrap();
    assert_eq!(plan.disclosure().preview, TEST_SENTENCE);
}
