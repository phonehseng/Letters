//! passwords and permissions.
//!
//! Letters locks files with the strongest method pdf has: the standard
//! security handler revision 6 (pdf 2.0), with aes-256 for every string
//! and stream. the key derivation (sha-256, sha-384 and sha-512 mixed
//! with aes, algorithm 2.B), the /U /O /UE /OE /Perms entries and the
//! random iv of each string and stream come from lopdf, which implements
//! them; this file chooses the settings and words the errors.
//!
//! two kinds of protection, and they are not alike:
//! - the open password is real encryption. without it the file is noise.
//! - permissions (printing, copying, editing...) are only a request. the
//!   file can be opened without the owner password, so the content is
//!   readable, and many programs ignore the request. they keep honest
//!   people honest and nothing more.
//!
//! opening locked files works for the methods lopdf can read: rc4 with
//! 40 to 128 bit keys (revisions 2 to 4), aes-128 (revision 4) and
//! aes-256 (revisions 5 and 6). certificate (public key) security and
//! other custom handlers are not supported.

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{
    dictionary, Document, EncryptionState, EncryptionVersion, LoadOptions, Object, Permissions,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::RedactError;
use crate::scrub::random_id;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrintPermission {
    None,
    /// printing allowed, but possibly only at low quality
    LowQuality,
    #[default]
    Full,
}

/// what people may do with the file without the owner password.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PermissionOptions {
    pub printing: PrintPermission,
    /// copying text and images out of the file
    pub copying: bool,
    /// changing the document's content
    pub editing: bool,
    /// adding comments and filling in forms
    pub commenting: bool,
    /// filling in forms, even when commenting is not allowed
    pub filling_forms: bool,
    /// inserting, removing and turning pages
    pub assembling: bool,
}

impl Default for PermissionOptions {
    fn default() -> Self {
        Self {
            printing: PrintPermission::Full,
            copying: true,
            editing: true,
            commenting: true,
            filling_forms: true,
            assembling: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LockOptions {
    /// needed to open the file at all. empty means anyone can open it and
    /// only the permissions apply
    pub open_password: String,
    /// needed to change the permissions. when empty, a long random one is
    /// made up, so nobody can lift the permissions with an empty password
    pub owner_password: String,
    pub permissions: PermissionOptions,
}

/// how a file is (or was) locked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockInfo {
    /// "AES-256", "AES-128" or "RC4"
    pub method: String,
    pub revision: i64,
}

/// encrypts the document in place. save it with `save_clean` afterwards.
/// unused objects are dropped and the rest numbered again first, because
/// that has to happen before encryption, not after.
pub fn encrypt(doc: &mut Document, options: &LockOptions) -> Result<(), RedactError> {
    if doc.is_encrypted() {
        return Err(RedactError::CannotLock("it is already locked".into()));
    }
    if options.open_password.is_empty() && options.permissions == PermissionOptions::default() {
        return Err(RedactError::CannotLock(
            "there is no password and nothing is restricted".into(),
        ));
    }
    doc.prune_objects();
    doc.renumber_objects();
    if !doc.trailer.has(b"ID") {
        doc.trailer
            .set("ID", Object::Array(vec![random_id(), random_id()]));
    }
    let owner_password = if options.owner_password.is_empty() {
        random_password()?
    } else {
        options.owner_password.clone()
    };
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|error| RedactError::CannotLock(error.to_string()))?;
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let version = EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: &owner_password,
        user_password: &options.open_password,
        permissions: permission_bits(&options.permissions),
    };
    let state = EncryptionState::try_from(version)
        .map_err(|error| RedactError::CannotLock(error.to_string()))?;
    mark_aes_256_version(doc);
    doc.encrypt(&state)
        .map_err(|error| RedactError::CannotLock(error.to_string()))?;
    doc.encryption_state = None;
    Ok(())
}

fn permission_bits(options: &PermissionOptions) -> Permissions {
    let mut bits = Permissions::COPYABLE_FOR_ACCESSIBILITY;
    let mut allow = |flag: Permissions, yes: bool| {
        if yes {
            bits |= flag;
        }
    };
    allow(
        Permissions::PRINTABLE,
        options.printing != PrintPermission::None,
    );
    allow(
        Permissions::PRINTABLE_IN_HIGH_QUALITY,
        options.printing == PrintPermission::Full,
    );
    allow(Permissions::COPYABLE, options.copying);
    allow(Permissions::MODIFIABLE, options.editing);
    allow(Permissions::ANNOTABLE, options.commenting);
    allow(
        Permissions::FILLABLE,
        options.filling_forms || options.commenting,
    );
    allow(Permissions::ASSEMBLABLE, options.assembling);
    bits
}

/// a 32 letter password nobody will guess.
fn random_password() -> Result<String, RedactError> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|error| RedactError::CannotLock(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// aes-256 belongs to pdf 2.0, and to pdf 1.7 with adobe's extension
/// level 8. the file says which, so older readers know what they face.
fn mark_aes_256_version(doc: &mut Document) {
    if doc.version.as_str() < "1.7" {
        doc.version = "1.7".to_owned();
    }
    if doc.version.as_str() < "2.0" {
        if let Ok(catalog) = doc.catalog_mut() {
            let extension = dictionary! {
                "BaseVersion" => Object::Name(b"1.7".to_vec()),
                "ExtensionLevel" => 8,
            };
            catalog.set("Extensions", dictionary! { "ADBE" => extension });
        }
    }
}

/// opens a pdf from its bytes. a locked file is refused with `Locked`,
/// so the app can ask for the password and call `unlock`.
pub fn open(bytes: &[u8]) -> Result<Document, RedactError> {
    let doc = Document::load_mem(bytes).map_err(|error| RedactError::Damaged(error.to_string()))?;
    if doc.is_encrypted() {
        return Err(RedactError::Locked);
    }
    Ok(doc)
}

/// opens a locked pdf with its password (the open password or the owner
/// password). the document comes back decrypted.
pub fn unlock(bytes: &[u8], password: &str) -> Result<Document, RedactError> {
    match Document::load_mem_with_options(bytes, LoadOptions::with_password(password)) {
        Ok(doc) if doc.is_encrypted() => Err(RedactError::WrongPassword),
        Ok(doc) => Ok(doc),
        Err(lopdf::Error::InvalidPassword) => Err(RedactError::WrongPassword),
        Err(lopdf::Error::Decryption(error)) => {
            Err(RedactError::UnsupportedLock(error.to_string()))
        }
        Err(lopdf::Error::Unimplemented(what)) => {
            Err(RedactError::UnsupportedLock(what.to_owned()))
        }
        Err(error) => Err(RedactError::Damaged(error.to_string())),
    }
}

/// how the file was locked when it was opened, if it was.
pub fn lock_info(doc: &Document) -> Option<LockInfo> {
    let state = doc.encryption_state.as_ref()?;
    let method = match state.version() {
        5 => "AES-256",
        4 => match state
            .crypt_filters()
            .values()
            .next()
            .map(|filter| filter.method().to_vec())
        {
            Some(method) if method == b"AESV2" => "AES-128",
            _ => "RC4",
        },
        _ => "RC4",
    };
    Some(LockInfo {
        method: method.to_owned(),
        revision: state.revision(),
    })
}
