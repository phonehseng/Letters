//! how a file is locked, and unlocking it for editing.
//!
//! the viewer can show a locked pdf once it has the password, but edits
//! work on the working copy with lopdf, which needs the document
//! decrypted. unlocking writes a decrypted copy of the working copy into
//! the session's own folder in the cache (deleted when the document is
//! closed, and at the next start after a crash) and remembers how the
//! file was locked, so saving can lock it again.
//!
//! two kinds of lock (see core/redact's lock.rs):
//! - an open password really encrypts the file. the person must know it
//!   (or the owner password) to unlock.
//! - permissions alone are a request: anyone can open the file. when the
//!   author asked that it not be edited, Letters asks for the owner
//!   password, the one that set those limits, instead of ignoring them.

use std::fmt;

use letters_redact::lopdf::{Dictionary, Document, Object};
use letters_redact::{save_clean, PermissionOptions, RedactError};
use serde::Serialize;

use super::permissions::{bits_from_p, options_from_bits};

/// how a file is locked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileLock {
    /// "AES-256", "AES-128" or "RC4"
    pub method: String,
    pub revision: i64,
    /// false when only permissions are set: anyone can open it
    pub needs_open_password: bool,
    /// what people may do without the owner password
    pub permissions: PermissionOptions,
}

/// which password unlocked the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PasswordUsed {
    /// the password needed to open it
    Open,
    /// the owner password, which also lifts the permissions
    Owner,
    /// none was needed: only permissions were set, and editing is allowed
    None,
}

/// a locked file, unlocked.
pub struct Unlocked {
    /// the whole document, decrypted, written from scratch
    pub bytes: Vec<u8>,
    pub lock: FileLock,
    pub used: PasswordUsed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnlockError {
    NotLocked,
    /// editing is restricted and no owner password was given (or a wrong one)
    OwnerPasswordNeeded,
    Engine(RedactError),
}

impl fmt::Display for UnlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotLocked => write!(f, "This PDF isn’t locked, so it can be changed already."),
            Self::OwnerPasswordNeeded => write!(
                f,
                "The author of this PDF asked that it not be changed. To change it anyway, enter the owner password, the one that set that limit."
            ),
            Self::Engine(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for UnlockError {}

impl From<RedactError> for UnlockError {
    fn from(error: RedactError) -> Self {
        Self::Engine(error)
    }
}

/// how the pdf in `bytes` is locked, or None when it isn't.
pub fn inspect_lock(bytes: &[u8]) -> Result<Option<FileLock>, RedactError> {
    let document =
        Document::load_mem(bytes).map_err(|error| RedactError::Damaged(error.to_string()))?;
    // lopdf opens a file that needs no password to open by itself: what
    // it was locked with is then in its encryption state
    if let Some(state) = &document.encryption_state {
        return Ok(Some(FileLock {
            method: letters_redact::lock_info(&document)
                .map(|info| info.method)
                .unwrap_or_else(|| "RC4".to_owned()),
            revision: state.revision(),
            needs_open_password: false,
            permissions: options_from_bits(state.permissions(), state.revision()),
        }));
    }
    // still locked: the open password is needed. the /Encrypt entry says
    // how, and /P is stored in the clear
    let Ok(encrypt) = document.get_encrypted() else {
        return Ok(None);
    };
    let revision = encrypt.get(b"R").and_then(Object::as_i64).unwrap_or(0);
    let p = encrypt.get(b"P").and_then(Object::as_i64).unwrap_or(-1);
    Ok(Some(FileLock {
        method: method_of(encrypt),
        revision,
        needs_open_password: true,
        permissions: options_from_bits(bits_from_p(p), revision),
    }))
}

/// the name of the encryption an /Encrypt dictionary describes.
fn method_of(encrypt: &Dictionary) -> String {
    let version = encrypt.get(b"V").and_then(Object::as_i64).unwrap_or(0);
    let filter_method = encrypt
        .get(b"CF")
        .and_then(Object::as_dict)
        .and_then(|filters| filters.get(b"StdCF"))
        .and_then(Object::as_dict)
        .and_then(|filter| filter.get(b"CFM"))
        .and_then(Object::as_name)
        .map(|name| name.to_vec())
        .unwrap_or_default();
    let method = match (version, filter_method.as_slice()) {
        (5, _) | (_, b"AESV3") => "AES-256",
        (4, b"AESV2") => "AES-128",
        _ => "RC4",
    };
    method.to_owned()
}

/// unlocks `bytes` with `password` for editing.
pub fn unlock_for_editing(bytes: &[u8], password: &str) -> Result<Unlocked, UnlockError> {
    let lock = inspect_lock(bytes)?.ok_or(UnlockError::NotLocked)?;
    let used = which_password(bytes, password, &lock)?;
    let mut document = letters_redact::unlock(bytes, password)?;
    // nothing of the old lock may follow the document into the cache
    document.encryption_state = None;
    let bytes = save_clean(&mut document)?;
    Ok(Unlocked { bytes, lock, used })
}

/// checks the password against the lock, and says which one it is.
fn which_password(
    bytes: &[u8],
    password: &str,
    lock: &FileLock,
) -> Result<PasswordUsed, UnlockError> {
    if lock.needs_open_password {
        let document =
            Document::load_mem(bytes).map_err(|error| RedactError::Damaged(error.to_string()))?;
        if document.authenticate_user_password(password).is_ok() {
            return Ok(PasswordUsed::Open);
        }
        if document.authenticate_owner_password(password).is_ok() {
            return Ok(PasswordUsed::Owner);
        }
        return Err(UnlockError::Engine(RedactError::WrongPassword));
    }
    // anyone can open it. only the owner password lifts a ban on editing
    if !password.is_empty() && is_owner_password(bytes, password) {
        return Ok(PasswordUsed::Owner);
    }
    if lock.permissions.editing {
        return Ok(PasswordUsed::None);
    }
    Err(UnlockError::OwnerPasswordNeeded)
}

/// true when `password` is the owner password of a file that opens
/// without one. lopdf decrypts such a file as it loads and drops its
/// /Encrypt entry, so the entry is rebuilt from the encryption state
/// into a small document of its own, just to check the password.
fn is_owner_password(bytes: &[u8], password: &str) -> bool {
    let Ok(document) = Document::load_mem(bytes) else {
        return false;
    };
    let Some(Ok(encrypt)) = document
        .encryption_state
        .as_ref()
        .map(|state| state.encode())
    else {
        return false;
    };
    let mut probe = Document::with_version("1.7");
    if let Ok(id) = document.trailer.get(b"ID") {
        probe.trailer.set("ID", id.clone());
    }
    let encrypt_id = probe.add_object(Object::Dictionary(encrypt));
    probe.trailer.set("Encrypt", Object::Reference(encrypt_id));
    probe.authenticate_owner_password(password).is_ok()
}
