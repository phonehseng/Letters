//! the tips a person has dismissed, kept between runs so a tip never
//! comes back once it was waved away. a tip can be dismissed where it
//! was offered (its id, "add-total:0:A10") or as a whole kind
//! ("add-total"), for people who know the trick already.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use letters_sheets::help::suggest::{suggest, RecentAction, Suggestion};
use letters_sheets::{Range, Workbook};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Dismissed {
    ids: Vec<String>,
    kinds: Vec<String>,
}

pub struct TipStore {
    file: PathBuf,
    dismissed: Mutex<Dismissed>,
}

/// the kind of a tip from its id: the part before the first colon.
fn kind_of(id: &str) -> &str {
    id.split(':').next().unwrap_or(id)
}

impl TipStore {
    pub fn load(file: PathBuf) -> TipStore {
        let dismissed = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        TipStore {
            file,
            dismissed: Mutex::new(dismissed),
        }
    }

    /// the tips for the selection, without any the person dismissed.
    pub fn tips_for(
        &self,
        book: &Workbook,
        sheet: usize,
        selection: Range,
        recent: &[RecentAction],
    ) -> Vec<Suggestion> {
        let dismissed = self.lock().clone();
        suggest(book, sheet, selection, recent, &dismissed.ids)
            .into_iter()
            .filter(|tip| !dismissed.kinds.iter().any(|kind| kind == kind_of(&tip.id)))
            .collect()
    }

    /// `whole_kind`: never offer this kind of tip again.
    pub fn dismiss(&self, id: &str, whole_kind: bool) -> Result<(), String> {
        let mut dismissed = self.lock();
        if whole_kind {
            dismissed.kinds.push(kind_of(id).to_string());
        } else {
            dismissed.ids.push(id.to_string());
        }
        if let Some(folder) = self.file.parent() {
            fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec(&*dismissed).map_err(|error| error.to_string())?;
        crate::writer::save::write_atomically(&bytes, &self.file)
    }

    /// Settings' "Show Tips Again".
    pub fn forget_all(&self) -> Result<(), String> {
        *self.lock() = Dismissed::default();
        let _ = fs::remove_file(&self.file);
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Dismissed> {
        self.dismissed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_sheets::Value;

    fn numbers_above_an_empty_cell() -> Workbook {
        let mut book = Workbook::new();
        for row in 0..4 {
            book.set_value(0, row, 0, Value::Number(f64::from(row) + 1.0));
        }
        book
    }

    fn store(name: &str) -> TipStore {
        let file = std::env::temp_dir()
            .join("letters-sheets-tests")
            .join(format!("tips-{name}-{}.json", std::process::id()));
        let _ = fs::remove_file(&file);
        TipStore::load(file)
    }

    #[test]
    fn a_dismissed_kind_stays_away_after_a_restart() {
        let tips = store("kind");
        let book = numbers_above_an_empty_cell();
        let below = Range::parse("A5").unwrap();
        let offered = tips.tips_for(&book, 0, below, &[]);
        let total = offered
            .iter()
            .find(|tip| tip.id.starts_with("add-total"))
            .unwrap();
        tips.dismiss(&total.id, true).unwrap();
        let again = TipStore::load(tips.file.clone());
        assert!(again
            .tips_for(&book, 0, below, &[])
            .iter()
            .all(|tip| !tip.id.starts_with("add-total")));
    }
}
