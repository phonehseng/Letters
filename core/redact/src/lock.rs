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

/// which password opened a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PasswordMatch {
    /// the password needed to open the file
    OpenPassword,
    /// the owner password, which also lifts the permissions
    OwnerPassword,
    /// the file opens without a password (it has permissions only, or is
    /// not locked at all) and no password was given
    NoPasswordNeeded,
}

/// a document opened by `unlock_with_info`, and how.
#[derive(Clone, Debug)]
pub struct Unlocked {
    pub document: Document,
    pub matched: PasswordMatch,
}

/// opens a locked pdf with its password (the open password or the owner
/// password). the document comes back decrypted. see `unlock_with_info`
/// for which password matched.
pub fn unlock(bytes: &[u8], password: &str) -> Result<Document, RedactError> {
    unlock_with_info(bytes, password).map(|unlocked| unlocked.document)
}

/// opens a pdf with a password and says which password it was.
///
/// lopdf on its own tries the empty password first, so a file with
/// permissions only would open with any password at all. here a password
/// that was typed must really be the open or the owner password, or it
/// is refused, because the app uses the owner password to decide whether
/// the permissions may be lifted. an empty password is accepted only for
/// a file that needs none.
pub fn unlock_with_info(bytes: &[u8], password: &str) -> Result<Unlocked, RedactError> {
    let doc = load(bytes)?;
    if doc.is_encrypted() {
        return unlock_open_password(bytes, &doc, password);
    }
    let Some(probe) = security_probe(&doc) else {
        // not locked at all: only "no password" is right
        return match password.is_empty() {
            true => Ok(Unlocked {
                document: doc,
                matched: PasswordMatch::NoPasswordNeeded,
            }),
            false => Err(RedactError::WrongPassword),
        };
    };
    let matched = if password.is_empty() {
        PasswordMatch::NoPasswordNeeded
    } else if probe.authenticate_owner_password(password).is_ok() {
        PasswordMatch::OwnerPassword
    } else if probe.authenticate_user_password(password).is_ok() {
        PasswordMatch::OpenPassword
    } else {
        return Err(RedactError::WrongPassword);
    };
    Ok(Unlocked {
        document: doc,
        matched,
    })
}

/// a file that cannot be opened without a password: the password is
/// checked against both entries before the file is decrypted with it.
fn unlock_open_password(
    bytes: &[u8],
    locked: &Document,
    password: &str,
) -> Result<Unlocked, RedactError> {
    if password.is_empty() {
        return Err(RedactError::WrongPassword);
    }
    let matched = if locked.authenticate_owner_password(password).is_ok() {
        PasswordMatch::OwnerPassword
    } else if locked.authenticate_user_password(password).is_ok() {
        PasswordMatch::OpenPassword
    } else {
        return Err(RedactError::WrongPassword);
    };
    match Document::load_mem_with_options(bytes, LoadOptions::with_password(password)) {
        Ok(doc) if doc.is_encrypted() => Err(RedactError::WrongPassword),
        Ok(document) => Ok(Unlocked { document, matched }),
        Err(error) => Err(describe_load_error(error)),
    }
}

/// lopdf decrypts a file with an empty open password while loading it and
/// drops its /Encrypt dictionary. to check a password against such a file
/// the dictionary is rebuilt from what lopdf kept, in a small document of
/// its own that holds only the trailer and that dictionary.
fn security_probe(doc: &Document) -> Option<Document> {
    let state = doc.encryption_state.as_ref()?;
    let dict = state.encode().ok()?;
    let mut probe = Document::new();
    probe.trailer = doc.trailer.clone();
    probe.objects.insert((1, 0), Object::Dictionary(dict));
    probe.trailer.set("Encrypt", Object::Reference((1, 0)));
    Some(probe)
}

fn load(bytes: &[u8]) -> Result<Document, RedactError> {
    Document::load_mem(bytes).map_err(describe_load_error)
}

fn describe_load_error(error: lopdf::Error) -> RedactError {
    match error {
        lopdf::Error::InvalidPassword => RedactError::WrongPassword,
        lopdf::Error::Decryption(error) => RedactError::UnsupportedLock(error.to_string()),
        lopdf::Error::Unimplemented(what) => RedactError::UnsupportedLock(what.to_owned()),
        error => RedactError::Damaged(error.to_string()),
    }
}

/// true when the file can't be opened without a password.
///
/// a file without /Encrypt is answered from its trailer alone. a locked
/// file is loaded, which is quick when it needs a password (nothing can
/// be decrypted yet) but decrypts a file that has permissions only.
pub fn needs_password_to_open(bytes: &[u8]) -> Result<bool, RedactError> {
    if !declares_encryption(bytes)? {
        return Ok(false);
    }
    Ok(load(bytes)?.is_encrypted())
}

/// true when the file asks readers to limit printing, copying, editing,
/// commenting, form filling or page assembly. such limits are a request
/// that other programs may ignore (see the top of this file).
pub fn has_restrictions(bytes: &[u8]) -> Result<bool, RedactError> {
    if !declares_encryption(bytes)? {
        return Ok(false);
    }
    let doc = load(bytes)?;
    let bits = match doc.encryption_state.as_ref() {
        Some(state) => state.permissions().bits(),
        None => doc
            .get_encrypted()
            .ok()
            .and_then(|dict| dict.get(b"P").ok())
            .and_then(|p| p.as_i64().ok())
            // /P is a signed 32 bit number; its low bits are the flags
            .map(|p| p as u32 as u64)
            .unwrap_or(0),
    };
    Ok(!Permissions::from_bits_truncate(bits).contains(ALL_PERMISSIONS))
}

/// every permission a reader can be asked to withhold.
const ALL_PERMISSIONS: Permissions = Permissions::PRINTABLE
    .union(Permissions::MODIFIABLE)
    .union(Permissions::COPYABLE)
    .union(Permissions::ANNOTABLE)
    .union(Permissions::FILLABLE)
    .union(Permissions::ASSEMBLABLE)
    .union(Permissions::PRINTABLE_IN_HIGH_QUALITY);

/// reads only the trailer and the info dictionary, which is cheap.
fn declares_encryption(bytes: &[u8]) -> Result<bool, RedactError> {
    Document::load_metadata_mem(bytes)
        .map(|metadata| metadata.encrypted)
        .map_err(describe_load_error)
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
