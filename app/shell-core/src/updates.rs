//! in-app updates, the parts that need no window: when a quiet check is
//! due, and the release notes cut down to the few lines the note shows.
//!
//! the updater itself (tauri-plugin-updater, app/shell/src/updates.rs)
//! asks github releases for latest.json, checks the installer's signature
//! against the public key in tauri.conf.json, and installs it. checking
//! quietly on launch happens at most once a day, so starting letters ten
//! times doesn't ask github ten times.

use std::fs;
use std::path::PathBuf;

/// a quiet check at most this often, in seconds.
pub const CHECK_EVERY_SECONDS: u64 = 24 * 60 * 60;

/// how many characters of the release notes the update note shows.
pub const NOTES_LENGTH: usize = 280;

/// when the last quiet check was, kept in a small file of its own.
pub struct UpdateSchedule {
    file: PathBuf,
}

impl UpdateSchedule {
    pub fn new(file: PathBuf) -> Self {
        UpdateSchedule { file }
    }

    fn last_check(&self) -> Option<u64> {
        fs::read_to_string(&self.file).ok()?.trim().parse().ok()
    }

    /// whether a quiet check is due at `now` (seconds since 1970). when it
    /// is, the time is written down, so the next one waits a day. a clock
    /// put back since the last check counts as due rather than waiting.
    pub fn take_due(&self, now: u64) -> bool {
        if let Some(last) = self.last_check() {
            if last <= now && now - last < CHECK_EVERY_SECONDS {
                return false;
            }
        }
        if let Some(folder) = self.file.parent() {
            let _ = fs::create_dir_all(folder);
        }
        let _ = fs::write(&self.file, now.to_string());
        true
    }
}

/// the release notes as a short note: markdown marks (headings, bullets,
/// bold) taken off, lines joined, and cut at a word near `NOTES_LENGTH`.
pub fn short_notes(body: &str) -> String {
    let lines: Vec<String> = body
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches('#')
                .trim_start_matches(['-', '*', '•'])
                .replace("**", "")
                .trim()
                .to_owned()
        })
        .filter(|line| !line.is_empty())
        .collect();
    let joined = lines.join(" ");
    if joined.chars().count() <= NOTES_LENGTH {
        return joined;
    }
    let cut: String = joined.chars().take(NOTES_LENGTH).collect();
    let at_word = cut.rfind(' ').map(|space| &cut[..space]).unwrap_or(&cut);
    format!("{}…", at_word.trim_end_matches([',', ';', ':', '.']))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_pdf::temp_dir;

    #[test]
    fn a_quiet_check_comes_at_most_once_a_day() {
        let schedule = UpdateSchedule::new(temp_dir("update-schedule").join("update-check.txt"));
        let start = 1_800_000_000;
        assert!(schedule.take_due(start), "the first launch checks");
        assert!(
            !schedule.take_due(start + 60),
            "a second launch a minute later doesn't"
        );
        assert!(!schedule.take_due(start + CHECK_EVERY_SECONDS - 1));
        assert!(
            schedule.take_due(start + CHECK_EVERY_SECONDS),
            "a day later it does"
        );
        // a clock put back is due again rather than waiting for a day that won't come
        assert!(schedule.take_due(start));
    }

    #[test]
    fn release_notes_become_a_short_plain_note() {
        let body = "## What changed\n\n- **Spreadsheets** keep frozen rows when scrolled\n- Tab follows a form's own order\n";
        assert_eq!(
            short_notes(body),
            "What changed Spreadsheets keep frozen rows when scrolled Tab follows a form's own order"
        );
        let long = "word ".repeat(200);
        let note = short_notes(&long);
        assert!(
            note.ends_with('…') && note.chars().count() <= NOTES_LENGTH + 1,
            "{note}"
        );
    }
}
