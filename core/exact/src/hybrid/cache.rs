//! decisions per document, so converting the same pdf again on the same
//! word build needs no proof rounds: only the final checking export, which
//! always runs, because a pass is never reported without a measurement.
//!
//! the key is the pdf's bytes, this crate's version and word's build: a
//! change to any of them can change what word does with a candidate.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::candidates::{build_candidate, PageGeometry};
use super::ladder::{page_of, Slot, SlotState, ATTEMPTS};
use super::paragraphs::DetectedParagraph;
use super::PlannedPage;
use crate::calibration::WordModel;

/// bump when the candidates' xml or the ladder changes.
const FORMAT: u32 = 1;

/// one accepted paragraph: which lines, at which attempt, in which frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub page: usize,
    pub paragraph: DetectedParagraph,
    pub attempt: usize,
    pub own_frame: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    format: u32,
    decisions: Vec<Decision>,
}

/// the cache key of a document: its bytes' hash, the crate version, word's build.
pub fn key(pdf_hash: u64, word_build: &str) -> String {
    let text = format!("{pdf_hash:016x}|{}|{word_build}", env!("CARGO_PKG_VERSION"));
    format!("{:016x}", crate::fonts::rebuild::fnv1a(text.as_bytes()))
}

fn path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("hybrid-{key}.json"))
}

pub fn load(dir: &Path, key: &str) -> Option<Vec<Decision>> {
    let text = std::fs::read_to_string(path(dir, key)).ok()?;
    let file: CacheFile = serde_json::from_str(&text).ok()?;
    (file.format == FORMAT).then_some(file.decisions)
}

pub(super) fn save(dir: &Path, key: &str, slots: &[Slot]) -> std::io::Result<()> {
    let decisions: Vec<Decision> = slots
        .iter()
        .filter(|s| matches!(s.state, SlotState::Accepted(_)))
        .map(|s| Decision {
            page: s.page,
            paragraph: s.paragraph.clone(),
            attempt: s.attempt,
            own_frame: s.own_frame,
        })
        .collect();
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_string(&CacheFile {
        format: FORMAT,
        decisions,
    })
    .map_err(std::io::Error::other)?;
    std::fs::write(path(dir, key), text)
}

/// replaces the slots with the cached decisions, rebuilt.
pub(super) fn apply(
    slots: &mut Vec<Slot>,
    decisions: &[Decision],
    pages: &[PlannedPage],
    model: &mut WordModel,
) {
    slots.clear();
    for decision in decisions {
        let page = page_of(pages, decision.page);
        let (level, nudge) = ATTEMPTS[decision.attempt.min(ATTEMPTS.len() - 1)];
        let geometry = PageGeometry {
            width_pt: page.width,
            height_pt: page.height,
        };
        let Some(candidate) = build_candidate(
            decision.page,
            &page.lines,
            &decision.paragraph,
            level,
            &geometry,
            model,
            nudge,
        ) else {
            continue;
        };
        slots.push(Slot {
            page: decision.page,
            paragraph: decision.paragraph.clone(),
            attempt: decision.attempt,
            state: SlotState::Accepted(candidate),
            was_split: false,
            own_frame: decision.own_frame,
            last_verdict: None,
        });
    }
}
