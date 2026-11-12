//! the key a path is stored and compared under: two spellings that name the
//! same file on this system get the same key, and two different files never do.
//!
//! - windows: the file system ignores case and takes either slash, so
//!   "C:/Notes/a.pdf" and "c:\notes\A.PDF" are one file.
//! - macos: names are kept in one unicode form (macos hands out "é" as e
//!   plus an accent from some places and as one letter from others), and
//!   case is ignored only on volumes that ignore it. the usual macos volume
//!   does; a case-sensitive one tells "Notes" from "notes", and so do we.
//! - linux and the rest: names are compared exactly, as the file system does.

#[cfg(target_os = "macos")]
use unicode_normalization::UnicodeNormalization;

/// the key `path` is stored and compared under on this system.
pub fn path_key(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('/', "\\").to_lowercase()
    }
    #[cfg(target_os = "macos")]
    {
        let composed: String = path.nfc().collect();
        if mac::ignores_case(&composed) {
            composed.to_lowercase()
        } else {
            composed
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        path.to_string()
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::path::Path;
    use std::sync::Mutex;

    /// each volume's answer, asked once: the search filters ask for every
    /// document they look at
    static ANSWERS: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

    /// whether the volume holding `path` ignores case. a path that doesn't
    /// exist (yet) is answered for the volume of its nearest existing folder.
    pub fn ignores_case(path: &str) -> bool {
        let volume = volume_of(path);
        let mut answers = ANSWERS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let answers = answers.get_or_insert_with(HashMap::new);
        if let Some(answer) = answers.get(&volume) {
            return *answer;
        }
        let answer = !volume_is_case_sensitive(path);
        answers.insert(volume, answer);
        answer
    }

    /// other volumes are mounted under /Volumes; everything else is the
    /// startup volume.
    fn volume_of(path: &str) -> String {
        match path.strip_prefix("/Volumes/") {
            Some(rest) => format!("/Volumes/{}", rest.split('/').next().unwrap_or("")),
            None => "/".to_string(),
        }
    }

    fn volume_is_case_sensitive(path: &str) -> bool {
        let mut existing = Path::new(path);
        while !existing.exists() {
            match existing.parent() {
                Some(parent) => existing = parent,
                None => return false,
            }
        }
        let Ok(c_path) = CString::new(existing.to_string_lossy().as_bytes()) else {
            return false;
        };
        // 1 when the volume tells case apart, 0 when it doesn't, -1 when it
        // can't say: then the macos default, case ignored, is assumed
        let answer = unsafe { libc::pathconf(c_path.as_ptr(), libc::_PC_CASE_SENSITIVE) };
        answer == 1
    }
}

#[cfg(test)]
mod tests {
    use super::path_key;

    #[test]
    fn the_same_spelling_is_the_same_key() {
        assert_eq!(
            path_key("/library/Notes.pdf"),
            path_key("/library/Notes.pdf")
        );
    }

    #[test]
    fn different_files_get_different_keys() {
        assert_ne!(
            path_key("/library/notes.pdf"),
            path_key("/library/notes 2.pdf")
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_ignores_case_and_slash_direction() {
        assert_eq!(
            path_key("C:/Library/Notes.pdf"),
            path_key("c:\\library\\notes.PDF")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_treats_both_unicode_forms_of_a_name_as_one() {
        // "é" as one letter, and as "e" followed by a combining accent
        assert_eq!(
            path_key("/tmp/caf\u{e9}.pdf"),
            path_key("/tmp/cafe\u{301}.pdf")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_follows_the_volume_on_case() {
        // the temporary folder is on the startup volume; whatever it answers,
        // the key must agree with it
        let folder = std::env::temp_dir().join("letters-path-key");
        std::fs::create_dir_all(&folder).unwrap();
        let upper = folder.join("Notes.pdf");
        let lower = folder.join("notes.pdf");
        std::fs::write(&upper, b"x").unwrap();
        let same_file = lower.exists();
        let same_key = path_key(&upper.to_string_lossy()) == path_key(&lower.to_string_lossy());
        assert_eq!(same_key, same_file);
        let _ = std::fs::remove_file(&upper);
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    #[test]
    fn linux_tells_case_apart_as_its_file_systems_do() {
        assert_ne!(
            path_key("/library/Notes.pdf"),
            path_key("/library/notes.pdf")
        );
    }
}
