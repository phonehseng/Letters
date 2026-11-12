//! the measured facts about one word build (recipe 9), and where they come
//! from: the built-in tables, a cache folder, or new probe exports.
//!
//! only two kinds of facts are measured, once per word build:
//! - "steps": the pdf writer's step pixels per axis and page size,
//!   keyed `x:<page width px>` and `y:<page height px>`
//! - "baseline": for each exact line height in twips, 60 baseline offsets
//!   (pixels below the frame's top pixel), indexed by y twips mod 60
//!
//! printed point values never go into the cache: they depend on the page
//! size, which is why the round 3 cache disagreed with itself on 0.9% of y
//! values.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::word::writer_model::{
    baseline_offset_pixels, estimated_step_table, StepTable, TWIPS_PERIOD,
};

/// the word build the built-in tables were measured on.
pub const MEASURED_BUILD: &str = "16.0.20527.20150";
static MEASURED_TABLES: &str = include_str!("../../calibration/word-16.0.20527.20150-model.json");

/// an axis of the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    /// the cache key of a step table: `x:<page px>`, with `:landscape` for
    /// sections marked landscape. word prints a different lattice for a
    /// page of the same size when its section says `w:orient="landscape"`
    /// (measured on 720 x 405 pt, see README), so the two never share a table.
    fn key(self, page_pixels: i64, landscape: bool) -> String {
        let axis = match self {
            Axis::X => "x",
            Axis::Y => "y",
        };
        let orientation = if landscape { ":landscape" } else { "" };
        format!("{axis}:{page_pixels}{orientation}")
    }
}

/// a section is written landscape when its page is wider than tall (recipe 10).
pub fn is_landscape(width_pt: f64, height_pt: f64) -> bool {
    width_pt > height_pt
}

/// the file format, the same as the prototype's cache files.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelFacts {
    #[serde(default)]
    pub steps: BTreeMap<String, Vec<i64>>,
    #[serde(default)]
    pub baseline: BTreeMap<String, Vec<i64>>,
}

/// the facts the planner uses for one conversion.
#[derive(Clone, Debug)]
pub struct WordModel {
    /// the word build these facts are for
    pub build: String,
    pub facts: ModelFacts,
    /// step tables that were estimated, not measured (no word to measure with)
    pub estimated: Vec<String>,
}

impl WordModel {
    /// the tables shipped with Letters, measured on [`MEASURED_BUILD`].
    pub fn built_in() -> Self {
        let facts: ModelFacts =
            serde_json::from_str(MEASURED_TABLES).expect("the built-in tables are valid json");
        Self {
            build: MEASURED_BUILD.to_string(),
            facts,
            estimated: Vec::new(),
        }
    }

    /// the facts for a word build: the cache file if there is one, else
    /// the built-in tables when they were measured on this build, else
    /// nothing (everything must be probed).
    pub fn for_build(build: &str, cache_dir: Option<&Path>) -> Self {
        let mut model = if build == MEASURED_BUILD {
            Self::built_in()
        } else {
            Self {
                build: build.to_string(),
                facts: ModelFacts::default(),
                estimated: Vec::new(),
            }
        };
        if let Some(path) = cache_dir.map(|dir| cache_path(dir, build)) {
            if let Some(cached) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<ModelFacts>(&text).ok())
            {
                model.facts.steps.extend(cached.steps);
                model.facts.baseline.extend(cached.baseline);
            }
        }
        model
    }

    /// saves the facts to the cache folder.
    pub fn save(&self, cache_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(cache_dir)?;
        let text = serde_json::to_string_pretty(&self.facts).map_err(std::io::Error::other)?;
        std::fs::write(cache_path(cache_dir, &self.build), text)
    }

    pub fn has_steps(&self, axis: Axis, page_pixels: i64, landscape: bool) -> bool {
        self.facts
            .steps
            .contains_key(&axis.key(page_pixels, landscape))
    }

    pub fn set_steps(&mut self, axis: Axis, page_pixels: i64, landscape: bool, steps: Vec<i64>) {
        self.facts
            .steps
            .insert(axis.key(page_pixels, landscape), steps);
    }

    pub fn has_baseline(&self, line_twips: i64) -> bool {
        self.facts.baseline.contains_key(&line_twips.to_string())
    }

    pub fn set_baseline(&mut self, line_twips: i64, offsets: Vec<i64>) {
        self.facts.baseline.insert(line_twips.to_string(), offsets);
    }

    /// the step table of an axis and page size; an estimate (remembered in
    /// `estimated`) when it was never measured.
    pub fn table(&mut self, axis: Axis, page_pixels: i64, landscape: bool) -> StepTable {
        let key = axis.key(page_pixels, landscape);
        if let Some(steps) = self.facts.steps.get(&key) {
            return StepTable::new(steps.clone());
        }
        if !self.estimated.contains(&key) {
            self.estimated.push(key);
        }
        estimated_step_table(page_pixels)
    }

    /// pixels from the frame's top pixel to its baseline: the measured
    /// table when there is one (the prototype uses it), else the closed
    /// rule that matched 960 of 960 measured frames (recipe 6.6).
    pub fn baseline_offset(&self, line_twips: i64, y_twips: i64) -> i64 {
        match self.facts.baseline.get(&line_twips.to_string()) {
            Some(offsets) if offsets.len() == TWIPS_PERIOD as usize => {
                offsets[y_twips.rem_euclid(TWIPS_PERIOD) as usize]
            }
            _ => baseline_offset_pixels(line_twips, y_twips),
        }
    }
}

/// `word-<build>-model.json` in the cache folder.
pub fn cache_path(cache_dir: &Path, build: &str) -> PathBuf {
    cache_dir.join(format!("word-{}-model.json", build.replace(' ', "_")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_tables_hold_the_recipe_page_sizes() {
        let model = WordModel::built_in();
        for key in [
            "x:5100", "y:6600", "x:4961", "y:7016", "x:8400", "y:5100", "x:6000", "y:3375",
        ] {
            assert!(model.facts.steps.contains_key(key), "{key}");
        }
        assert_eq!(model.facts.steps["x:6000"], [633, 1982, 3331, 4680]);
    }

    #[test]
    fn built_in_baselines_agree_with_the_closed_rule() {
        // the measured tables and the rule of recipe 6.6, for every phase
        let model = WordModel::built_in();
        for (height, offsets) in &model.facts.baseline {
            let height: i64 = height.parse().unwrap();
            for (residue, offset) in offsets.iter().enumerate() {
                let y = 1200 + residue as i64;
                assert_eq!(
                    *offset,
                    baseline_offset_pixels(height, y),
                    "line {height}, t mod 60 = {residue}"
                );
            }
        }
    }

    #[test]
    fn unknown_sizes_are_estimated_and_remembered() {
        let mut model = WordModel::built_in();
        let table = model.table(Axis::X, 7000, false);
        assert!(!table.steps.is_empty());
        assert_eq!(model.estimated, ["x:7000"]);
    }

    #[test]
    fn landscape_sections_have_their_own_tables() {
        let mut model = WordModel::for_build("16.0.99999.1", None);
        model.set_steps(Axis::X, 6000, false, vec![633]);
        assert!(model.has_steps(Axis::X, 6000, false));
        assert!(!model.has_steps(Axis::X, 6000, true));
        model.table(Axis::X, 6000, true);
        assert_eq!(model.estimated, ["x:6000:landscape"]);
        assert!(is_landscape(720.0, 405.0));
        assert!(!is_landscape(612.0, 792.0));
    }

    #[test]
    fn the_cache_adds_to_the_built_in_tables() {
        let dir = std::env::temp_dir().join(format!("letters-exact-cache-{}", std::process::id()));
        let mut model = WordModel::built_in();
        model.set_steps(Axis::Y, 1234, false, vec![100, 200]);
        model.save(&dir).unwrap();
        let again = WordModel::for_build(MEASURED_BUILD, Some(&dir));
        assert!(again.has_steps(Axis::Y, 1234, false));
        assert!(again.has_steps(Axis::X, 5100, false));
        let other = WordModel::for_build("16.0.99999.1", Some(&dir));
        assert!(!other.has_steps(Axis::X, 5100, false));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
