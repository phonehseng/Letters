//! "Don’t ask again for this provider and document".
//!
//! before text leaves the computer the person sees exactly what goes
//! where and says Send. they may tick a box so the same provider can read
//! the same document again without asking; that choice is about one pair
//! only. there is deliberately no way to say "never ask" for everything,
//! and the memory lasts only while the app runs.

use std::collections::HashSet;
use std::sync::Mutex;

/// the pairs of (where text goes, which document) the person allowed.
#[derive(Default)]
pub struct ConsentMemory {
    allowed: Mutex<HashSet<(String, String)>>,
}

impl ConsentMemory {
    pub fn new() -> Self {
        Self::default()
    }

    /// `destination` is the host the disclosure names; `document` is the
    /// document's file path.
    pub fn remember(&self, destination: &str, document: &str) {
        self.lock().insert(key(destination, document));
    }

    pub fn is_remembered(&self, destination: &str, document: &str) -> bool {
        self.lock().contains(&key(destination, document))
    }

    /// forgets everything, for when the provider settings change.
    pub fn forget_all(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<(String, String)>> {
        self.allowed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// hosts are not case sensitive; paths are compared as given.
fn key(destination: &str, document: &str) -> (String, String) {
    (destination.to_ascii_lowercase(), document.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_yes_covers_one_provider_and_one_document_only() {
        let memory = ConsentMemory::new();
        memory.remember("api.example.com", "C:/a.pdf");
        assert!(memory.is_remembered("API.example.com", "C:/a.pdf"));
        assert!(!memory.is_remembered("api.example.com", "C:/b.pdf"));
        assert!(!memory.is_remembered("other.example.org", "C:/a.pdf"));
    }

    #[test]
    fn changing_settings_forgets() {
        let memory = ConsentMemory::new();
        memory.remember("api.example.com", "C:/a.pdf");
        memory.forget_all();
        assert!(!memory.is_remembered("api.example.com", "C:/a.pdf"));
    }
}
