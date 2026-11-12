//! what each open document's protection is.
//!
//! the shell keeps one `ProtectionTable` for every open document: the
//! password save will apply, how the file was locked before it was
//! unlocked, whether anything was removed (so the ui can say saving is a
//! clean one), and the before-and-after copies the redaction review has
//! open. the entry is forgotten when the document closes, and with it any
//! password typed for it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use letters_redact::{LockOptions, PermissionOptions};
use serde::Serialize;

use super::clean_save::LockOnSave;
use super::permissions::restricts_anything;
use super::unlock::{FileLock, PasswordUsed};

/// one document's protection.
#[derive(Clone, Debug, Default)]
pub struct Protection {
    /// what save does about a password. None while nobody touched it: a
    /// file that was never locked saves without one, and a locked one
    /// that was never unlocked stays exactly as it is
    pub lock: Option<LockOnSave>,
    /// how the file was locked, once it was unlocked for editing
    pub unlocked_from: Option<(FileLock, PasswordUsed)>,
    /// the document was opened with a password (the viewer asked for one)
    pub opened_with_password: bool,
    /// a redaction or a scrub was applied in this session
    pub removed_content: bool,
    /// the dry runs of the redaction review: engine document ids and files
    pub previews: Vec<(u64, PathBuf)>,
    /// how the working copy is locked, once it was looked at. only the
    /// copy made at opening can be locked (nothing can be edited before
    /// it is unlocked), so it is read once and kept
    pub working_lock: Option<Option<FileLock>>,
}

impl Protection {
    /// true when the file on disk will (or does) need a password to open.
    /// no picture of such a document is kept in the cache.
    pub fn hides_thumbnail(&self) -> bool {
        match &self.lock {
            Some(lock) => lock.needs_open_password(),
            None => self.opened_with_password,
        }
    }

    /// what the ui shows. `working_lock` is how the working copy itself is
    /// locked now (only before it is unlocked).
    pub fn status(&self, working_lock: Option<&FileLock>, clean_save: bool) -> ProtectionStatus {
        let (open_password_on_save, restrictions_on_save, owner_password_set) = match &self.lock {
            Some(LockOnSave::Lock(options)) => lock_parts(options),
            Some(LockOnSave::Unlocked) | None => (false, None, false),
            Some(LockOnSave::Undecided) => (false, None, false),
        };
        ProtectionStatus {
            locked: working_lock.is_some(),
            lock: working_lock
                .cloned()
                .or_else(|| self.unlocked_from.as_ref().map(|(lock, _)| lock.clone())),
            unlocked_with: self.unlocked_from.as_ref().map(|(_, used)| *used),
            changed: self.lock.is_some(),
            open_password_on_save,
            restrictions_on_save,
            owner_password_set,
            undecided: self.lock == Some(LockOnSave::Undecided),
            clean_save,
            removed_content: self.removed_content,
        }
    }
}

/// a change to the password settings, as the Protect tab sends it. a
/// field left out keeps what is there, so setting a password to open
/// keeps the restrictions, and the other way round.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LockChange {
    /// the password to open the file; an empty one removes it
    pub open_password: Option<String>,
    pub permissions: Option<PermissionOptions>,
    /// the password that lifts the restrictions; empty means one nobody knows
    pub owner_password: Option<String>,
    /// no password and no restrictions at all
    pub remove_all: bool,
}

/// what save does after `change`, starting from `current`.
pub fn merge_lock(current: Option<&LockOnSave>, change: &LockChange) -> LockOnSave {
    if change.remove_all {
        return LockOnSave::Unlocked;
    }
    let mut options = match current {
        Some(LockOnSave::Lock(options)) => options.clone(),
        // an undecided open password is decided by this change
        _ => LockOptions::default(),
    };
    if let Some(password) = &change.open_password {
        options.open_password = password.clone();
    }
    if let Some(permissions) = &change.permissions {
        options.permissions = permissions.clone();
    }
    if let Some(owner) = &change.owner_password {
        options.owner_password = owner.clone();
    }
    let nothing_left =
        options.open_password.is_empty() && !restricts_anything(&options.permissions);
    if nothing_left {
        LockOnSave::Unlocked
    } else {
        LockOnSave::Lock(options)
    }
}

/// what a lock will do, in the parts the ui shows.
fn lock_parts(options: &LockOptions) -> (bool, Option<PermissionOptions>, bool) {
    let restrictions =
        restricts_anything(&options.permissions).then(|| options.permissions.clone());
    (
        !options.open_password.is_empty(),
        restrictions,
        !options.owner_password.is_empty(),
    )
}

/// a document's protection as the Protect tab shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionStatus {
    /// the working copy is still locked: it can be read but not changed
    /// until it is unlocked
    pub locked: bool,
    /// how the file is (or was) locked
    pub lock: Option<FileLock>,
    /// the password that unlocked it, once unlocked
    pub unlocked_with: Option<PasswordUsed>,
    /// the password settings were changed (or kept) for the next save
    pub changed: bool,
    pub open_password_on_save: bool,
    /// the permissions the next save asks for, when anything is restricted
    pub restrictions_on_save: Option<PermissionOptions>,
    pub owner_password_set: bool,
    /// save waits for a choice about the open password
    pub undecided: bool,
    /// saving writes the whole file from scratch
    pub clean_save: bool,
    /// content was removed in this session, so saving deletes the
    /// versions kept for undo
    pub removed_content: bool,
}

/// every open document's protection, by engine document id.
#[derive(Default)]
pub struct ProtectionTable {
    entries: Mutex<HashMap<u64, Protection>>,
}

impl ProtectionTable {
    /// a copy of one document's protection (a fresh one if none is kept).
    pub fn get(&self, id: u64) -> Protection {
        self.lock_entries().get(&id).cloned().unwrap_or_default()
    }

    /// changes one document's protection and returns what `change` returned.
    pub fn update<T>(&self, id: u64, change: impl FnOnce(&mut Protection) -> T) -> T {
        change(self.lock_entries().entry(id).or_default())
    }

    /// forgets a document (it was closed). returns its review copies, for
    /// the caller to close in the engine and delete.
    pub fn forget(&self, id: u64) -> Vec<(u64, PathBuf)> {
        self.lock_entries()
            .remove(&id)
            .map(|protection| protection.previews)
            .unwrap_or_default()
    }

    fn lock_entries(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Protection>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
