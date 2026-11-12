//! pictures of a person's signature, kept for reuse.
//!
//! a drawn, typed or chosen signature is made into a png by the ui and
//! kept here, in the app's data folder on this computer, so the next
//! form can be signed with one click. they are never put into a document
//! until the person places one, and deleting one here removes the file.
//!
//! the folder holds one png per signature and a small list
//! (`signatures.json`) saying what kind each one is and when it was made.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// a signature picture bigger than this is refused. a signature drawn at
/// a high resolution is well under 1 mb.
pub const MAX_PICTURE_BYTES: usize = 4 * 1024 * 1024;

/// how many signatures are kept. the oldest go first once there are more.
pub const MAX_SAVED: usize = 12;

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
const LIST_FILE: &str = "signatures.json";

/// keeps ids made in the same millisecond apart.
static NEXT_NUMBER: AtomicU32 = AtomicU32::new(1);

/// how a signature picture was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PictureKind {
    Drawn,
    Typed,
    Image,
}

/// one kept signature, as the ui lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSignature {
    pub id: String,
    pub kind: PictureKind,
    /// seconds since 1970
    pub created_at: u64,
    /// the picture's size in pixels, so it can be placed in proportion
    pub width: u32,
    pub height: u32,
}

pub struct SignaturePictures {
    folder: PathBuf,
}

impl SignaturePictures {
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    /// the kept signatures, newest first. entries whose picture is gone
    /// are left out, so a hand-deleted file never shows as broken.
    pub fn list(&self) -> Vec<SavedSignature> {
        let mut list: Vec<SavedSignature> = self
            .read_list()
            .into_iter()
            .filter(|saved| self.picture_path(&saved.id).is_file())
            .collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        list
    }

    /// keeps a new signature picture and returns its entry.
    pub fn add(&self, png: &[u8], kind: PictureKind) -> Result<SavedSignature, String> {
        let (width, height) = png_size(png)?;
        fs::create_dir_all(&self.folder).map_err(cannot_keep)?;
        let created_at = seconds_now();
        let id = format!(
            "{}-{}",
            millis_now(),
            NEXT_NUMBER.fetch_add(1, Ordering::Relaxed)
        );
        fs::write(self.picture_path(&id), png).map_err(cannot_keep)?;
        let saved = SavedSignature {
            id,
            kind,
            created_at,
            width,
            height,
        };
        let mut list = self.list();
        list.insert(0, saved.clone());
        // the oldest are let go once there are too many to choose from
        for old in list.drain(MAX_SAVED.min(list.len())..) {
            let _ = fs::remove_file(self.picture_path(&old.id));
        }
        self.write_list(&list)?;
        Ok(saved)
    }

    /// the png of a kept signature.
    pub fn read(&self, id: &str) -> Result<Vec<u8>, String> {
        check_id(id)?;
        fs::read(self.picture_path(id)).map_err(|_| {
            "That signature can’t be found any more. Make it again, then try again.".to_owned()
        })
    }

    /// deletes a kept signature: its picture file and its entry.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        check_id(id)?;
        let path = self.picture_path(id);
        if path.exists() {
            fs::remove_file(&path).map_err(|error| {
                format!("The signature couldn’t be deleted. Try again. ({error})")
            })?;
        }
        let list: Vec<SavedSignature> = self.list().into_iter().filter(|s| s.id != id).collect();
        self.write_list(&list)
    }

    fn picture_path(&self, id: &str) -> PathBuf {
        self.folder.join(format!("signature-{id}.png"))
    }

    fn read_list(&self) -> Vec<SavedSignature> {
        fs::read_to_string(self.folder.join(LIST_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn write_list(&self, list: &[SavedSignature]) -> Result<(), String> {
        let text = serde_json::to_string_pretty(list).map_err(|error| error.to_string())?;
        write_quietly(&self.folder.join(LIST_FILE), &text).map_err(cannot_keep)
    }
}

/// ids are made here from digits and a dash. anything else (a path, say)
/// is refused, so no file outside the folder can be read or deleted.
fn check_id(id: &str) -> Result<(), String> {
    let well_formed =
        !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_digit() || c == '-');
    if well_formed {
        Ok(())
    } else {
        Err("That signature can’t be found.".to_owned())
    }
}

/// the width and height from a png's header, refusing anything else.
fn png_size(png: &[u8]) -> Result<(u32, u32), String> {
    if png.len() > MAX_PICTURE_BYTES {
        return Err(
            "This signature picture is too big to keep. Make it smaller, then try again.".into(),
        );
    }
    // the header chunk comes first: 8 bytes of signature, 4 of length, "IHDR"
    if png.len() < 24 || !png.starts_with(PNG_SIGNATURE) || &png[12..16] != b"IHDR" {
        return Err("Letters can’t read this signature picture. Make it again.".into());
    }
    let number = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    let (width, height) = (number(16), number(20));
    if width == 0 || height == 0 {
        return Err("This signature picture is empty. Make it again.".into());
    }
    Ok((width, height))
}

fn cannot_keep(error: impl std::fmt::Display) -> String {
    format!("The signature couldn’t be kept on this computer. Check that it has free space, then try again. ({error})")
}

fn write_quietly(path: &Path, text: &str) -> std::io::Result<()> {
    let temporary = path.with_extension("json.saving");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

fn seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .unwrap_or(0)
}

fn millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_millis())
        .unwrap_or(0)
}
