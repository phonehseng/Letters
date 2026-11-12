//! telling the person, before anything is sent, exactly what will leave
//! this computer and where it goes.
//!
//! every task goes through two steps:
//! 1. `plan` works out what would be sent and returns a `Disclosure`
//!    describing it. nothing has been sent yet.
//! 2. `run` takes the plan back together with an `Acknowledgement` of that
//!    same disclosure. only then is a `Permit` made, and providers refuse to
//!    send without one.
//!
//! an acknowledgement can only come from `Disclosure::acknowledge`, it names
//! one disclosure by its id, and every plan gets a new id. so a "yes" to
//! sending two pages can't be reused to send a whole book.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use crate::provider::{Permit, ProviderInfo};
use crate::AiError;

/// how much of the text is shown as a preview.
const PREVIEW_CHARS: usize = 160;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// names one disclosure. unique for as long as the app runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct DisclosureId(u64);

impl DisclosureId {
    fn next() -> Self {
        DisclosureId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

/// what a planned task would send, and where.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disclosure {
    pub id: DisclosureId,
    /// true when the text would leave this computer.
    pub leaves_device: bool,
    /// the host it would go to.
    pub destination: String,
    pub provider_name: String,
    /// the pages whose text would be sent, in order.
    pub pages: Vec<u32>,
    pub characters: usize,
    pub words: usize,
    /// whether the person's own words (a question or a search) go too.
    pub query: Option<QueryKind>,
    /// the start of the text, so the person can see what it is.
    pub preview: String,
    /// the sentence to show, from `sentence()`.
    pub sentence: String,
}

impl Disclosure {
    pub(crate) fn new(
        info: &ProviderInfo,
        pages: Vec<u32>,
        texts: &[&str],
        query: Option<(QueryKind, &str)>,
    ) -> Disclosure {
        let query = query.filter(|(_, words)| !words.trim().is_empty());
        let characters = texts.iter().map(|t| t.chars().count()).sum::<usize>()
            + query.map_or(0, |(_, words)| words.chars().count());
        let words = texts.iter().map(|t| t.split_whitespace().count()).sum();
        let mut disclosure = Disclosure {
            id: DisclosureId::next(),
            leaves_device: info.sends_content_off_device,
            destination: info.host.clone(),
            provider_name: info.name.clone(),
            pages,
            characters,
            words,
            query: query.map(|(kind, _)| kind),
            preview: preview(texts),
            sentence: String::new(),
        };
        disclosure.sentence = disclosure.describe();
        disclosure
    }

    /// true when the task would send something at all.
    pub fn sends_anything(&self) -> bool {
        !self.pages.is_empty()
    }

    /// true when the ui must ask before running: text would leave.
    pub fn needs_confirmation(&self) -> bool {
        self.leaves_device && self.sends_anything()
    }

    /// the person's "yes" to this disclosure, and only this one.
    pub fn acknowledge(&self) -> Acknowledgement {
        Acknowledgement { id: self.id }
    }

    /// the plain-english sentence for the ui.
    pub fn sentence(&self) -> &str {
        &self.sentence
    }

    fn describe(&self) -> String {
        if !self.leaves_device {
            return "This stays on this computer.".to_string();
        }
        if !self.sends_anything() {
            return "Nothing will be sent.".to_string();
        }
        let query = match self.query {
            Some(QueryKind::Question) => "your question and ",
            Some(QueryKind::Search) => "what you searched for and ",
            None => "",
        };
        format!(
            "This will send {query}the text of {} (about {}) to {}.",
            describe_pages(&self.pages),
            describe_words(self.words),
            self.destination
        )
    }
}

/// what kind of words of the person's own go with the document text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QueryKind {
    Question,
    Search,
}

/// the person's confirmation of one disclosure.
///
/// it has no public constructor: the only way to get one is
/// `Disclosure::acknowledge`, which the ui calls when the person agrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Acknowledgement {
    id: DisclosureId,
}

/// checks that a planned task may run now, and makes its permit.
///
/// - text that leaves the computer needs an acknowledgement of exactly
///   this disclosure;
/// - the provider must still be the one the disclosure described, so a
///   settings change in between can't redirect the text.
pub(crate) fn authorise(
    disclosure: &Disclosure,
    acknowledgement: Option<&Acknowledgement>,
    provider_now: &ProviderInfo,
) -> Result<Permit, AiError> {
    let same_provider = provider_now.host == disclosure.destination
        && provider_now.sends_content_off_device == disclosure.leaves_device;
    if !same_provider {
        return Err(AiError::ProviderChanged);
    }
    match acknowledgement {
        Some(ack) if ack.id != disclosure.id => return Err(AiError::ConsentMismatch),
        None if disclosure.needs_confirmation() => return Err(AiError::ConsentRequired),
        _ => {}
    }
    Ok(Permit::for_provider(provider_now))
}

fn preview(texts: &[&str]) -> String {
    let joined = texts
        .iter()
        .flat_map(|t| t.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ");
    if joined.chars().count() <= PREVIEW_CHARS {
        return joined;
    }
    let cut: String = joined.chars().take(PREVIEW_CHARS).collect();
    // end on a whole word
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}…")
}

/// "page 4", "pages 3–7", "pages 1–3, 7 and 9".
pub fn describe_pages(pages: &[u32]) -> String {
    let runs = page_runs(pages);
    let parts: Vec<String> = runs
        .iter()
        .map(|(first, last)| {
            if first == last {
                first.to_string()
            } else {
                format!("{first}–{last}")
            }
        })
        .collect();
    let single = runs.len() == 1 && runs[0].0 == runs[0].1;
    let noun = if single { "page" } else { "pages" };
    match parts.split_last() {
        None => "no pages".to_string(),
        Some((last, [])) => format!("{noun} {last}"),
        Some((last, rest)) => format!("{noun} {} and {last}", rest.join(", ")),
    }
}

fn page_runs(pages: &[u32]) -> Vec<(u32, u32)> {
    let mut sorted = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for page in sorted {
        match runs.last_mut() {
            Some((_, last)) if *last + 1 == page => *last = page,
            _ => runs.push((page, page)),
        }
    }
    runs
}

/// "12 words", "about 340 words", "2,400 words": rounded so the number
/// reads as the estimate it is.
pub fn describe_words(words: usize) -> String {
    let rounded = match words {
        0..=19 => words,
        20..=999 => (words + 5) / 10 * 10,
        _ => (words + 50) / 100 * 100,
    };
    let noun = if rounded == 1 { "word" } else { "words" };
    format!("{} {noun}", with_commas(rounded))
}

fn with_commas(number: usize) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderKind;

    fn remote() -> ProviderInfo {
        ProviderInfo {
            name: "Online service".into(),
            kind: ProviderKind::Remote,
            sends_content_off_device: true,
            host: "api.example.com".into(),
            context_tokens: 8192,
            supports_embeddings: false,
            thinking_tokens: 0,
        }
    }

    fn local() -> ProviderInfo {
        ProviderInfo {
            name: "Model on this computer".into(),
            kind: ProviderKind::Local,
            sends_content_off_device: false,
            host: "127.0.0.1".into(),
            ..remote()
        }
    }

    #[test]
    fn the_sentence_says_what_goes_where() {
        let words = "word ".repeat(2400);
        let disclosure = Disclosure::new(&remote(), vec![3, 4, 5, 6, 7], &[&words], None);
        assert_eq!(
            disclosure.sentence(),
            "This will send the text of pages 3–7 (about 2,400 words) to api.example.com."
        );
        let asked = Disclosure::new(
            &remote(),
            vec![2, 5, 9],
            &["a b c"],
            Some((QueryKind::Question, "Why?")),
        );
        assert_eq!(
            asked.sentence(),
            "This will send your question and the text of pages 2, 5 and 9 (about 3 words) to api.example.com."
        );
    }

    #[test]
    fn local_work_says_it_stays() {
        let disclosure = Disclosure::new(&local(), vec![1], &["text"], None);
        assert_eq!(disclosure.sentence(), "This stays on this computer.");
        assert!(!disclosure.needs_confirmation());
    }

    #[test]
    fn page_and_word_wording() {
        assert_eq!(describe_pages(&[4]), "page 4");
        assert_eq!(describe_pages(&[4, 5]), "pages 4–5");
        assert_eq!(describe_pages(&[9, 1, 2, 3, 7]), "pages 1–3, 7 and 9");
        assert_eq!(describe_words(1), "1 word");
        assert_eq!(describe_words(344), "340 words");
        assert_eq!(describe_words(12_345), "12,300 words");
    }

    #[test]
    fn the_preview_is_short_and_ends_on_a_word() {
        let long = "lorem ipsum ".repeat(50);
        let disclosure = Disclosure::new(&remote(), vec![1], &[&long], None);
        assert!(disclosure.preview.chars().count() <= PREVIEW_CHARS + 1);
        assert!(disclosure.preview.ends_with("ipsum…") || disclosure.preview.ends_with("lorem…"));
    }

    #[test]
    fn remote_text_needs_a_matching_yes() {
        let disclosure = Disclosure::new(&remote(), vec![1], &["text"], None);
        assert_eq!(
            authorise(&disclosure, None, &remote()).err(),
            Some(AiError::ConsentRequired)
        );
        let other = Disclosure::new(&remote(), vec![1], &["text"], None);
        assert_eq!(
            authorise(&disclosure, Some(&other.acknowledge()), &remote()).err(),
            Some(AiError::ConsentMismatch)
        );
        assert!(authorise(&disclosure, Some(&disclosure.acknowledge()), &remote()).is_ok());
    }

    #[test]
    fn a_changed_provider_cancels_the_yes() {
        let disclosure = Disclosure::new(&local(), vec![1], &["text"], None);
        let ack = disclosure.acknowledge();
        // the person switched to an online service after planning
        assert_eq!(
            authorise(&disclosure, Some(&ack), &remote()).err(),
            Some(AiError::ProviderChanged)
        );
        // the same host, but it now resolves off the computer
        let moved = ProviderInfo {
            sends_content_off_device: true,
            ..local()
        };
        assert_eq!(
            authorise(&disclosure, Some(&ack), &moved).err(),
            Some(AiError::ProviderChanged)
        );
    }

    #[test]
    fn every_plan_gets_its_own_id() {
        let a = Disclosure::new(&local(), vec![], &[], None);
        let b = Disclosure::new(&local(), vec![], &[], None);
        assert_ne!(a.id, b.id);
    }
}
