//! Test Connection in Settings.
//!
//! it asks the provider for a short summary of one fixed sentence, so the
//! request goes through exactly the path real requests take (address,
//! key, model, streaming) while sending nothing from any document.

use letters_ai::{
    AiError, AiTools, CancelToken, Document, Page, Paragraph, Rect, SummaryLength, TaskRequest,
};

/// the only words Test Connection ever sends.
pub const TEST_SENTENCE: &str =
    "This is a connection test from Letters. Reply with one short sentence.";

/// a sentence for Settings: where it connected, or the engine's error.
pub fn test_connection(tools: &AiTools, cancel: &CancelToken) -> Result<String, AiError> {
    let document = Document {
        pages: vec![Page {
            number: 1,
            paragraphs: vec![Paragraph {
                text: TEST_SENTENCE.into(),
                rect: Rect::default(),
            }],
        }],
    };
    let plan = tools.plan(
        &document,
        TaskRequest::Summarize {
            pages: None,
            length: SummaryLength::Short,
        },
    )?;
    // the fixed sentence is all that goes, and the person pressed the
    // button that says where it goes, so this is the yes for it
    let acknowledgement = plan.disclosure().acknowledge();
    let info = tools.provider_info();
    tools.run(plan, Some(acknowledgement), &mut |_| {}, cancel)?;
    Ok(if info.sends_content_off_device {
        format!(
            "Connected to {}. The test sent only one fixed sentence, no document text.",
            info.host
        )
    } else {
        format!("Connected to the model on this computer at {}.", info.host)
    })
}
