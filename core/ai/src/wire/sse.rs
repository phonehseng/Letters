//! reading server-sent events.
//!
//! a stream looks like this, one event per blank-line-separated block:
//!
//! ```text
//! data: {"choices":[{"delta":{"content":"Hel"}}]}
//!
//! data: {"choices":[{"delta":{"content":"lo"}}]}
//!
//! data: [DONE]
//! ```
//!
//! lines starting with ':' are comments (servers send them to keep the
//! connection open) and are skipped.

use std::io::{self, BufRead};

/// one event: its name (if the server gave one) and its data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub name: Option<String>,
    pub data: String,
    /// true when the stream ended in the middle of this event, so its
    /// data may be cut short.
    pub truncated: bool,
}

/// reads events from a stream one at a time.
pub struct EventReader<R> {
    reader: R,
    line: String,
}

impl<R: BufRead> EventReader<R> {
    pub fn new(reader: R) -> Self {
        EventReader {
            reader,
            line: String::new(),
        }
    }

    /// the next event, `Ok(None)` at the end of the stream.
    pub fn next_event(&mut self) -> io::Result<Option<Event>> {
        let mut name = None;
        let mut data: Option<String> = None;
        loop {
            self.line.clear();
            let read = self.reader.read_line(&mut self.line)?;
            if read == 0 {
                // the end of the stream also ends a half-written event
                return Ok(data.map(|data| Event {
                    name,
                    data,
                    truncated: true,
                }));
            }
            let line = self.line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if let Some(data) = data.take() {
                    return Ok(Some(Event {
                        name,
                        data,
                        truncated: false,
                    }));
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = split_field(line);
            match field {
                "data" => match data.as_mut() {
                    Some(existing) => {
                        existing.push('\n');
                        existing.push_str(value);
                    }
                    None => data = Some(value.to_string()),
                },
                "event" => name = Some(value.to_string()),
                _ => {}
            }
        }
    }
}

/// "data: x" becomes ("data", "x"); one space after the colon is dropped.
fn split_field(line: &str) -> (&str, &str) {
    match line.split_once(':') {
        Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
        None => (line, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(text: &str) -> Vec<Event> {
        let mut reader = EventReader::new(text.as_bytes());
        let mut found = Vec::new();
        while let Some(event) = reader.next_event().unwrap() {
            found.push(event);
        }
        found
    }

    fn data(text: &str) -> Event {
        Event {
            name: None,
            data: text.to_string(),
            truncated: false,
        }
    }

    #[test]
    fn reads_events_and_skips_comments() {
        let stream = ": keep-alive\n\ndata: one\n\ndata:two\r\n\r\ndata: [DONE]\n\n";
        assert_eq!(
            events(stream),
            vec![data("one"), data("two"), data("[DONE]")]
        );
    }

    #[test]
    fn joins_multi_line_data_and_keeps_names() {
        let stream = "event: error\ndata: a\ndata: b\n\n";
        assert_eq!(
            events(stream),
            vec![Event {
                name: Some("error".into()),
                data: "a\nb".into(),
                truncated: false,
            }]
        );
    }

    #[test]
    fn a_stream_that_just_stops_still_gives_its_last_event() {
        let half = Event {
            truncated: true,
            ..data("half")
        };
        assert_eq!(events("data: one\n\ndata: half"), vec![data("one"), half]);
        assert_eq!(events(""), vec![]);
    }
}
