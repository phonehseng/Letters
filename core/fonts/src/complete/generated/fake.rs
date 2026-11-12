//! a generator with canned replies, for tests and the evaluation harness;
//! it never contacts anything.

use super::*;

/// a generator with canned replies, for tests and the evaluation
/// harness. it never contacts anything.
pub struct FakeGenerator {
    pub info: GeneratorInfo,
    /// what it answers, keyed by nothing: every call gets the next reply
    pub replies: std::sync::Mutex<Vec<String>>,
    /// every request body it was given, for tests to inspect
    pub seen: std::sync::Mutex<Vec<String>>,
}

impl FakeGenerator {
    pub fn new(leaves_device: bool, replies: Vec<String>) -> FakeGenerator {
        FakeGenerator {
            info: GeneratorInfo {
                name: "Test service".into(),
                host: "glyphs.example.org".into(),
                leaves_device,
            },
            replies: std::sync::Mutex::new(replies),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// a reply drawing the given glyphs (em units) as the format asks.
    pub fn reply_for(glyphs: &BTreeMap<char, GlyphData>) -> String {
        let items: Vec<serde_json::Value> = glyphs
            .iter()
            .map(|(c, g)| {
                serde_json::json!({
                    "char": c.to_string(),
                    "advance": (g.advance * GRID).round(),
                    "path": g.shape.to_path_data(GRID),
                })
            })
            .collect();
        serde_json::json!({ "glyphs": items }).to_string()
    }
}

impl GlyphGenerator for FakeGenerator {
    fn describe(&self) -> GeneratorInfo {
        self.info.clone()
    }

    fn generate(
        &self,
        request: &GlyphRequest,
        permit: &GlyphPermit,
    ) -> Result<String, GenerationError> {
        permit.check(&self.info)?;
        self.seen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(request.body());
        let mut replies = self.replies.lock().unwrap_or_else(|p| p.into_inner());
        if replies.is_empty() {
            return Err(GenerationError::Provider("no reply".into()));
        }
        Ok(replies.remove(0))
    }
}
