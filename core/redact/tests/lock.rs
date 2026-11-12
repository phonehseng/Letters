//! passwords: aes-256 locking, unlocking with the right password only, and
//! reading files locked by other programs with the older methods.

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use letters_pdf::PdfError;
use letters_redact::lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use letters_redact::lopdf::{Document, EncryptionState, EncryptionVersion, Permissions};
use letters_redact::{
    encrypt, find_text_areas, has_restrictions, lock_info, needs_password_to_open, open,
    save_clean, unlock, unlock_with_info, FindOptions, LockOptions, PasswordMatch,
    PermissionOptions, PrintPermission, RedactError, TextQuery,
};
use support::*;

const MARKER: &str = "PLAINTEXTMARKER";
const PASSWORD: &str = "correct horse";

fn plain_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let content = format!("BT /F1 14 Tf 72 600 Td ({MARKER}) Tj ET");
    let info = pdf.add(format!("<< /Title ({MARKER} title) >>"));
    let (catalog, _) = add_pages(
        &mut pdf,
        vec![PageSpec::new(
            &format!("/Font << /F1 {font} 0 R >>"),
            &content,
        )],
        "",
    );
    pdf.finish(catalog, &format!("/Info {info} 0 R"))
}

fn has_marker(doc: &Document) -> bool {
    let query = TextQuery::Phrase {
        text: MARKER.into(),
        match_case: true,
    };
    !find_text_areas(doc, &query, &FindOptions::default())
        .unwrap()
        .is_empty()
}

fn locked_file(options: &LockOptions) -> Vec<u8> {
    let mut doc = Document::load_mem(&plain_pdf()).unwrap();
    encrypt(&mut doc, options).unwrap();
    save_clean(&mut doc).unwrap()
}

#[test]
fn a_locked_file_hides_its_text_and_opens_with_the_right_password_only() {
    let dir = temp_dir("lock-aes256");
    let options = LockOptions {
        open_password: PASSWORD.into(),
        owner_password: "the owner".into(),
        permissions: PermissionOptions {
            printing: PrintPermission::None,
            copying: false,
            ..Default::default()
        },
    };
    let locked = locked_file(&options);
    assert_not_in_bytes(&locked, MARKER);
    assert!(locked.windows(5).any(|w| w == b"AESV3"));

    assert_eq!(open(&locked).unwrap_err(), RedactError::Locked);
    assert_eq!(
        unlock(&locked, "wrong horse").unwrap_err(),
        RedactError::WrongPassword
    );
    let doc = unlock(&locked, PASSWORD).unwrap();
    assert!(has_marker(&doc));
    let info = lock_info(&doc).unwrap();
    assert_eq!((info.method.as_str(), info.revision), ("AES-256", 6));
    // the owner password opens it too
    assert!(has_marker(&unlock(&locked, "the owner").unwrap()));

    // an independent reader (pdfium) can open what we wrote, and only
    // with the password
    let worker = start_worker();
    let path = write(&dir, "locked.pdf", &locked);
    assert_eq!(
        worker.open(&path, None).unwrap_err(),
        PdfError::PasswordRequired
    );
    assert_eq!(
        worker.open(&path, Some("wrong horse")).unwrap_err(),
        PdfError::PasswordRequired
    );
    let opened = worker.open(&path, Some(PASSWORD)).unwrap();
    assert_eq!(worker.search(opened.id, MARKER).unwrap().matches.len(), 1);
}

#[test]
fn permissions_alone_leave_the_file_readable() {
    let dir = temp_dir("lock-permissions");
    let options = LockOptions {
        permissions: PermissionOptions {
            editing: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let locked = locked_file(&options);
    // the content is still encrypted on disk...
    assert_not_in_bytes(&locked, MARKER);
    // ...but anyone can open it: permissions are only a request
    let doc = open(&locked).unwrap();
    assert!(has_marker(&doc));
    assert!(lock_info(&doc).is_some());
    let worker = start_worker();
    let opened = worker
        .open(&write(&dir, "permissions.pdf", &locked), None)
        .unwrap();
    assert_eq!(worker.search(opened.id, MARKER).unwrap().matches.len(), 1);
}

#[test]
fn locking_with_nothing_to_lock_is_refused() {
    let mut doc = Document::load_mem(&plain_pdf()).unwrap();
    let error = encrypt(&mut doc, &LockOptions::default()).unwrap_err();
    assert!(matches!(error, RedactError::CannotLock(_)));
}

/// files locked by other programs with the older methods.
fn legacy_locked(version: impl Fn(&Document) -> EncryptionState) -> Vec<u8> {
    let mut doc = Document::load_mem(&plain_pdf()).unwrap();
    doc.trailer.set(
        "ID",
        letters_redact::lopdf::Object::Array(vec![
            letters_redact::lopdf::Object::string_literal("0123456789abcdef"),
            letters_redact::lopdf::Object::string_literal("0123456789abcdef"),
        ]),
    );
    let state = version(&doc);
    doc.encrypt(&state).unwrap();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn older_rc4_and_aes_128_files_can_be_unlocked() {
    let rc4 = legacy_locked(|doc| {
        EncryptionState::try_from(EncryptionVersion::V2 {
            document: doc,
            owner_password: "owner",
            user_password: PASSWORD,
            key_length: 128,
            permissions: Permissions::all(),
        })
        .unwrap()
    });
    let doc = unlock(&rc4, PASSWORD).unwrap();
    assert!(has_marker(&doc));
    assert_eq!(lock_info(&doc).unwrap().method, "RC4");

    let aes = legacy_locked(|doc| {
        let filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
        EncryptionState::try_from(EncryptionVersion::V4 {
            document: doc,
            encrypt_metadata: true,
            crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "owner",
            user_password: PASSWORD,
            permissions: Permissions::all(),
        })
        .unwrap()
    });
    let doc = unlock(&aes, PASSWORD).unwrap();
    assert!(has_marker(&doc));
    assert_eq!(lock_info(&doc).unwrap().method, "AES-128");
    assert_eq!(
        unlock(&aes, "nope").unwrap_err(),
        RedactError::WrongPassword
    );
}

#[test]
fn a_permissions_only_file_checks_a_typed_password() {
    let options = LockOptions {
        owner_password: "the owner".into(),
        permissions: PermissionOptions {
            copying: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let locked = locked_file(&options);
    assert!(!needs_password_to_open(&locked).unwrap());
    assert!(has_restrictions(&locked).unwrap());
    assert_eq!(
        unlock_with_info(&locked, "").unwrap().matched,
        PasswordMatch::NoPasswordNeeded
    );
    assert_eq!(
        unlock_with_info(&locked, "the owner").unwrap().matched,
        PasswordMatch::OwnerPassword
    );
    assert_eq!(
        unlock_with_info(&locked, "a guess").unwrap_err(),
        RedactError::WrongPassword
    );
    assert_eq!(
        unlock(&locked, "a guess").unwrap_err(),
        RedactError::WrongPassword
    );
    assert!(has_marker(
        &unlock_with_info(&locked, "the owner").unwrap().document
    ));
}

#[test]
fn an_open_password_file_says_which_password_matched() {
    let options = LockOptions {
        open_password: PASSWORD.into(),
        owner_password: "the owner".into(),
        permissions: PermissionOptions::default(),
    };
    let locked = locked_file(&options);
    assert!(needs_password_to_open(&locked).unwrap());
    assert!(!has_restrictions(&locked).unwrap());
    let opened = unlock_with_info(&locked, PASSWORD).unwrap();
    assert_eq!(opened.matched, PasswordMatch::OpenPassword);
    assert!(has_marker(&opened.document));
    assert_eq!(
        unlock_with_info(&locked, "the owner").unwrap().matched,
        PasswordMatch::OwnerPassword
    );
    assert_eq!(
        unlock_with_info(&locked, "").unwrap_err(),
        RedactError::WrongPassword
    );
    assert_eq!(
        unlock_with_info(&locked, "nope").unwrap_err(),
        RedactError::WrongPassword
    );
}

#[test]
fn a_file_that_is_not_locked_needs_no_password() {
    let plain = plain_pdf();
    assert!(!needs_password_to_open(&plain).unwrap());
    assert!(!has_restrictions(&plain).unwrap());
    assert_eq!(
        unlock_with_info(&plain, "").unwrap().matched,
        PasswordMatch::NoPasswordNeeded
    );
    assert_eq!(
        unlock_with_info(&plain, "anything").unwrap_err(),
        RedactError::WrongPassword
    );
}

/// a legacy file locked by another program, with the given open password.
fn legacy(user_password: &'static str, aes: bool) -> Vec<u8> {
    legacy_locked(move |doc| {
        let version = if aes {
            let filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
            EncryptionVersion::V4 {
                document: doc,
                encrypt_metadata: true,
                crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password: "owner",
                user_password,
                permissions: Permissions::PRINTABLE,
            }
        } else {
            EncryptionVersion::V2 {
                document: doc,
                owner_password: "owner",
                user_password,
                key_length: 128,
                permissions: Permissions::PRINTABLE,
            }
        };
        EncryptionState::try_from(version).unwrap()
    })
}

#[test]
fn legacy_files_say_which_password_matched() {
    for aes in [false, true] {
        let with_password = legacy(PASSWORD, aes);
        assert!(needs_password_to_open(&with_password).unwrap());
        assert!(has_restrictions(&with_password).unwrap());
        assert_eq!(
            unlock_with_info(&with_password, PASSWORD).unwrap().matched,
            PasswordMatch::OpenPassword,
            "aes {aes}"
        );
        let as_owner = unlock_with_info(&with_password, "owner").unwrap();
        assert_eq!(as_owner.matched, PasswordMatch::OwnerPassword, "aes {aes}");
        assert!(has_marker(&as_owner.document));
        assert_eq!(
            unlock_with_info(&with_password, "guess").unwrap_err(),
            RedactError::WrongPassword
        );

        let permissions_only = legacy("", aes);
        assert!(!needs_password_to_open(&permissions_only).unwrap());
        assert_eq!(
            unlock_with_info(&permissions_only, "").unwrap().matched,
            PasswordMatch::NoPasswordNeeded
        );
        assert_eq!(
            unlock_with_info(&permissions_only, "owner")
                .unwrap()
                .matched,
            PasswordMatch::OwnerPassword,
            "aes {aes}"
        );
        assert_eq!(
            unlock_with_info(&permissions_only, "guess").unwrap_err(),
            RedactError::WrongPassword,
            "aes {aes}"
        );
    }
}
