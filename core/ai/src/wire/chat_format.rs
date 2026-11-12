//! the json of the common chat-completions format.
//!
//! a request is a list of messages with roles ("system", "user"); an
//! answer comes back either whole (`choices[0].message.content`) or as a
//! stream of small pieces (`choices[0].delta.content`). embeddings go to a
//! sibling endpoint and come back as `data[i].embedding`.

use std::io::BufRead;

use serde_json::{json, Value};

use crate::cancel::CancelToken;
use crate::provider::{ChatRequest, Completion, FinishReason, Usage};
use crate::wire::sse::EventReader;
use crate::AiError;

/// a low temperature: summaries and answers should stick to the text,
/// not be creative.
const TEMPERATURE: f64 = 0.2;

/// the json body for a chat request.
pub fn chat_body(request: &ChatRequest, model: Option<&str>, stream: bool) -> String {
    let mut body = json!({
        "messages": [
            { "role": "system", "content": request.system() },
            { "role": "user", "content": request.user() },
        ],
        "max_tokens": request.max_tokens(),
        "temperature": TEMPERATURE,
        "stream": stream,
    });
    if let Some(model) = model {
        body["model"] = json!(model);
    }
    body.to_string()
}

/// the json body for an embeddings request.
pub fn embeddings_body(texts: &[String], model: Option<&str>) -> String {
    let mut body = json!({ "input": texts });
    if let Some(model) = model {
        body["model"] = json!(model);
    }
    body.to_string()
}

/// reads a whole (not streamed) answer.
pub fn parse_completion(body: &str) -> Result<Completion, AiError> {
    let value: Value = serde_json::from_str(body).map_err(|_| AiError::MalformedResponse)?;
    if let Some(error) = value.get("error") {
        return Err(error_from_message(error));
    }
    let choice = value
        .pointer("/choices/0")
        .ok_or(AiError::MalformedResponse)?;
    let text = choice
        .pointer("/message/content")
        .and_then(Value::as_str)
        .ok_or(AiError::MalformedResponse)?;
    Ok(Completion {
        text: text.to_string(),
        usage: parse_usage(&value),
        finish: parse_finish(choice).unwrap_or(FinishReason::Other),
    })
}

/// reads a streamed answer, passing each piece on as it arrives.
///
/// a stream must end with `[DONE]` or a finish reason; one that simply
/// stops is reported as interrupted rather than passed off as complete.
/// some servers ignore the request to stream and send one json answer,
/// which is accepted too.
pub fn read_stream(
    reader: impl BufRead,
    on_text: &mut dyn FnMut(&str),
    cancel: &CancelToken,
) -> Result<Completion, AiError> {
    let mut reader = reader;
    if starts_with_json(&mut reader)? {
        let mut body = String::new();
        reader.read_to_string(&mut body).map_err(read_error)?;
        let completion = parse_completion(&body)?;
        on_text(&completion.text);
        return Ok(completion);
    }
    let mut events = EventReader::new(reader);
    let mut answer = StreamedAnswer::default();
    loop {
        cancel.check()?;
        let Some(event) = events.next_event().map_err(read_error)? else {
            return answer.finish_at_end_of_stream();
        };
        if event.data.trim() == "[DONE]" {
            return Ok(answer.into_completion());
        }
        let value: Value = match serde_json::from_str(&event.data) {
            Ok(value) => value,
            // half a json object at the very end means the line dropped
            Err(_) if event.truncated => return Err(AiError::StreamInterrupted),
            Err(_) => return Err(AiError::MalformedResponse),
        };
        if let Some(error) = value.get("error") {
            return Err(error_from_message(error));
        }
        answer.add(&value, on_text);
    }
}

/// what has arrived of a streamed answer so far.
#[derive(Default)]
struct StreamedAnswer {
    text: String,
    usage: Option<Usage>,
    finish: Option<FinishReason>,
}

impl StreamedAnswer {
    fn add(&mut self, value: &Value, on_text: &mut dyn FnMut(&str)) {
        if let Some(usage) = parse_usage(value) {
            self.usage = Some(usage);
        }
        let Some(choice) = value.pointer("/choices/0") else {
            return;
        };
        if let Some(piece) = choice.pointer("/delta/content").and_then(Value::as_str) {
            if !piece.is_empty() {
                self.text.push_str(piece);
                on_text(piece);
            }
        }
        if let Some(finish) = parse_finish(choice) {
            self.finish = Some(finish);
        }
    }

    fn finish_at_end_of_stream(self) -> Result<Completion, AiError> {
        if self.finish.is_some() {
            Ok(self.into_completion())
        } else {
            Err(AiError::StreamInterrupted)
        }
    }

    fn into_completion(self) -> Completion {
        Completion {
            text: self.text,
            usage: self.usage,
            finish: self.finish.unwrap_or(FinishReason::Other),
        }
    }
}

/// reads the embeddings answer, checking one vector came back per text.
pub fn parse_embeddings(body: &str, expected: usize) -> Result<Vec<Vec<f32>>, AiError> {
    let value: Value = serde_json::from_str(body).map_err(|_| AiError::MalformedResponse)?;
    if let Some(error) = value.get("error") {
        return Err(error_from_message(error));
    }
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(AiError::MalformedResponse)?;
    let mut vectors: Vec<Option<Vec<f32>>> = vec![None; expected];
    for (position, item) in items.iter().enumerate() {
        // servers usually say which input each vector belongs to; fall
        // back to the order they arrive in
        let index = item
            .get("index")
            .and_then(Value::as_u64)
            .map_or(position, |i| i as usize);
        let numbers = item
            .get("embedding")
            .and_then(Value::as_array)
            .ok_or(AiError::MalformedResponse)?;
        let vector: Option<Vec<f32>> = numbers
            .iter()
            .map(|n| n.as_f64().map(|n| n as f32))
            .collect();
        let slot = vectors.get_mut(index).ok_or(AiError::MalformedResponse)?;
        *slot = Some(vector.ok_or(AiError::MalformedResponse)?);
    }
    vectors
        .into_iter()
        .map(|v| v.ok_or(AiError::MalformedResponse))
        .collect()
}

fn parse_usage(value: &Value) -> Option<Usage> {
    let usage = value.get("usage")?;
    Some(Usage {
        prompt_tokens: usage.get("prompt_tokens")?.as_u64()? as usize,
        completion_tokens: usage
            .get("completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
    })
}

fn parse_finish(choice: &Value) -> Option<FinishReason> {
    match choice.get("finish_reason")?.as_str()? {
        "stop" => Some(FinishReason::Stop),
        "length" => Some(FinishReason::Length),
        _ => Some(FinishReason::Other),
    }
}

/// an `{"error": ...}` object inside an otherwise fine answer.
///
/// the server's message is only looked at, never passed on: it might
/// quote the key back.
fn error_from_message(error: &Value) -> AiError {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .unwrap_or("");
    if mentions_context(message) {
        AiError::OutOfContext
    } else {
        AiError::ServerError(500)
    }
}

/// true for the various ways servers say "the prompt is too long".
pub fn mentions_context(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("context")
        || message.contains("too long")
        || message.contains("maximum") && message.contains("token")
}

/// true when a server's error text is about an unknown model.
pub fn mentions_missing_model(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("model")
        && [
            "not found",
            "does not exist",
            "doesn't exist",
            "unknown",
            "no such",
            "not exist",
        ]
        .iter()
        .any(|phrase| message.contains(phrase))
}

/// peeks at the first non-blank character without using it up.
fn starts_with_json(reader: &mut impl BufRead) -> Result<bool, AiError> {
    loop {
        let buffer = reader.fill_buf().map_err(read_error)?;
        if buffer.is_empty() {
            return Ok(false);
        }
        let blanks = buffer
            .iter()
            .take_while(|byte| byte.is_ascii_whitespace())
            .count();
        if blanks < buffer.len() {
            let first = buffer[blanks];
            reader.consume(blanks);
            return Ok(first == b'{');
        }
        let all = buffer.len();
        reader.consume(all);
    }
}

/// a failed read in the middle of an answer.
pub fn read_error(error: std::io::Error) -> AiError {
    // the http client reports its own timeouts wrapped inside an io error
    let wrapped_timeout = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ureq::Error>())
        .is_some_and(|inner| matches!(inner, ureq::Error::Timeout(_)));
    match error.kind() {
        _ if wrapped_timeout => AiError::TimedOut,
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => AiError::TimedOut,
        _ => AiError::StreamInterrupted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::build_request;

    fn stream(text: &str) -> (Result<Completion, AiError>, String) {
        let mut seen = String::new();
        let result = read_stream(
            text.as_bytes(),
            &mut |piece| seen.push_str(piece),
            &CancelToken::new(),
        );
        (result, seen)
    }

    fn delta(text: &str) -> String {
        format!(
            "data: {}\n\n",
            json!({"choices":[{"delta":{"content":text},"finish_reason":null}]})
        )
    }

    #[test]
    fn the_request_body_has_both_roles() {
        let request = build_request("Do it.", &[("[page 1]".into(), "Text.".into())], None, 99);
        let body: Value =
            serde_json::from_str(&chat_body(&request, Some("my-local-model"), true)).unwrap();
        assert_eq!(body["model"], "my-local-model");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 99);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        let without_model: Value = serde_json::from_str(&chat_body(&request, None, false)).unwrap();
        assert!(without_model.get("model").is_none());
    }

    #[test]
    fn a_complete_stream_is_put_together() {
        let text = format!(
            "{}{}data: {}\n\ndata: [DONE]\n\n",
            delta("Hel"),
            delta("lo"),
            json!({"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":2}})
        );
        let (result, seen) = stream(&text);
        let completion = result.unwrap();
        assert_eq!(completion.text, "Hello");
        assert_eq!(seen, "Hello");
        assert_eq!(completion.finish, FinishReason::Stop);
        assert_eq!(completion.usage.unwrap().prompt_tokens, 12);
    }

    #[test]
    fn a_stream_that_stops_early_is_interrupted() {
        let (result, seen) = stream(&delta("Hal"));
        assert_eq!(result, Err(AiError::StreamInterrupted));
        assert_eq!(seen, "Hal", "what arrived was still shown");
        let (cut, _) = stream(&format!("{}data: {{\"choices\":[{{\"del", delta("a")));
        assert_eq!(cut, Err(AiError::StreamInterrupted));
    }

    #[test]
    fn a_finish_reason_without_done_is_enough() {
        let text = format!(
            "{}data: {}\n\n",
            delta("Hi"),
            json!({"choices":[{"delta":{},"finish_reason":"length"}]})
        );
        assert_eq!(stream(&text).0.unwrap().finish, FinishReason::Length);
    }

    #[test]
    fn garbage_in_a_stream_is_malformed() {
        assert_eq!(
            stream("data: not json\n\n").0,
            Err(AiError::MalformedResponse)
        );
    }

    #[test]
    fn an_error_inside_a_stream_is_understood() {
        let text = format!(
            "data: {}\n\n",
            json!({"error":{"message":"the request exceeds the available context size"}})
        );
        assert_eq!(stream(&text).0, Err(AiError::OutOfContext));
    }

    #[test]
    fn a_server_that_ignores_streaming_still_works() {
        let body = json!({"choices":[{"message":{"content":"Whole."},"finish_reason":"stop"}]})
            .to_string();
        let (result, seen) = stream(&format!("\n  {body}"));
        assert_eq!(result.unwrap().text, "Whole.");
        assert_eq!(seen, "Whole.");
    }

    #[test]
    fn cancelling_stops_reading() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let result = read_stream(delta("x").as_bytes(), &mut |_| {}, &cancel);
        assert_eq!(result, Err(AiError::Cancelled));
    }

    #[test]
    fn embeddings_are_matched_to_their_inputs() {
        let body =
            json!({"data":[{"index":1,"embedding":[0.0,1.0]},{"index":0,"embedding":[1.0,0.0]}]})
                .to_string();
        assert_eq!(
            parse_embeddings(&body, 2).unwrap(),
            vec![vec![1.0, 0.0], vec![0.0, 1.0]]
        );
        assert_eq!(parse_embeddings(&body, 3), Err(AiError::MalformedResponse));
        assert_eq!(
            parse_embeddings("{\"data\":[{\"embedding\":[\"x\"]}]}", 1),
            Err(AiError::MalformedResponse)
        );
    }

    #[test]
    fn server_messages_are_recognised() {
        assert!(mentions_context(
            "This model's maximum context length is 4096 tokens"
        ));
        assert!(mentions_missing_model(
            "The model `my-model` does not exist"
        ));
        assert!(!mentions_missing_model("rate limit reached"));
    }
}
