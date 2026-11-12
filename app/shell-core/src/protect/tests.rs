//! the protect rules run through an edit session, the way the app runs them.
//!
//! the engine itself is tested in core/redact. these tests check what
//! the app adds: after a redaction every save is a clean one and can't be
//! made any other way, the versions kept for undo (which still hold the
//! secret) are gone from the disk once the document is saved or closed,
//! and a locked file can be unlocked, edited and saved locked again.

use std::fs;
use std::path::{Path, PathBuf};

use letters_redact::lopdf::{dictionary, Document, Object, Stream, StringFormat};
use letters_redact::{
    encrypt, redact, save_clean, scrub, LockOptions, PageRect, PermissionOptions, PrintPermission,
    RedactOptions, RedactionArea, ScrubItem, ScrubOptions,
};

use super::clean_save::{clean_writer, rewrite_clean, LockOnSave, UNDECIDED_MESSAGE};
use super::unlock::{inspect_lock, unlock_for_editing, PasswordUsed, UnlockError};
use crate::session::{clean_leftover_sessions, EditSession, SessionError};
use crate::test_pdf::temp_dir;

const SECRET: &str = "QX-SECRET-4417";

fn show(path: &Path) -> Result<PathBuf, String> {
    Ok(path.to_path_buf())
}

/// a one page pdf with the secret in its text, and an author in its info.
fn secret_pdf() -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let text = format!(
        "BT /F1 24 Tf 72 700 Td ({SECRET}) Tj ET BT /F1 12 Tf 72 400 Td (Kept words) Tj ET"
    );
    let content = document.add_object(Stream::new(dictionary! {}, text.into_bytes()));
    let page = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page)],
            "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let info = document.add_object(dictionary! {
        "Author" => Object::String(b"Jane Doe".to_vec(), StringFormat::Literal),
    });
    document.trailer.set("Info", info);
    document
}

fn write_secret_pdf(path: &Path) {
    let mut document = secret_pdf();
    document.save(path).unwrap();
}

/// true when the secret is in the bytes as they are, or in any stream
/// once it is decompressed.
fn holds(bytes: &[u8], secret: &str) -> bool {
    if contains(bytes, secret.as_bytes()) {
        return true;
    }
    let Ok(document) = Document::load_mem(bytes) else {
        return false;
    };
    document.objects.values().any(|object| match object {
        Object::Stream(stream) => {
            let plain = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());
            contains(&plain, secret.as_bytes())
        }
        Object::String(text, _) => contains(text, secret.as_bytes()),
        _ => false,
    })
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// the box over the secret line, in points from the top left.
fn over_the_secret() -> Vec<RedactionArea> {
    vec![RedactionArea {
        page: 0,
        rects: vec![PageRect::new(60.0, 60.0, 300.0, 50.0)],
    }]
}

/// what the shell's apply_redactions does to the working copy.
fn redact_and_clean(document: &mut Document) -> Result<(), letters_redact::RedactError> {
    redact(document, &over_the_secret(), &RedactOptions::default())?;
    rewrite_clean(document)
}

struct Fixture {
    original: PathBuf,
    root: PathBuf,
    session: EditSession,
}

fn fixture(name: &str) -> Fixture {
    let dir = temp_dir(name);
    let original = dir.join("form.pdf");
    write_secret_pdf(&original);
    let root = dir.join("sessions");
    let session = EditSession::start(&original, &root).unwrap();
    Fixture {
        original,
        root,
        session,
    }
}

/// every file under a folder, read.
fn all_files(folder: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(folder) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(all_files(&path));
        } else if let Ok(bytes) = fs::read(&path) {
            found.push((path, bytes));
        }
    }
    found
}

#[test]
fn the_test_file_really_holds_the_secret() {
    let Fixture { original, .. } = fixture("protect-baseline");
    assert!(holds(&fs::read(original).unwrap(), SECRET));
}

#[test]
fn saving_a_redacted_document_writes_no_trace_of_the_secret() {
    let Fixture {
        original,
        mut session,
        ..
    } = fixture("protect-save");
    session.apply(redact_and_clean, show).unwrap();
    session.write_saves_with(clean_writer(LockOnSave::Unlocked), true);

    session.save().unwrap();

    let saved = fs::read(&original).unwrap();
    assert!(!holds(&saved, SECRET));
    assert!(holds(&saved, "Kept words"), "text outside the box stays");
    assert!(!session.is_dirty());
}

#[test]
fn save_as_after_a_redaction_is_clean_too() {
    let Fixture {
        original,
        mut session,
        ..
    } = fixture("protect-save-as");
    session.apply(redact_and_clean, show).unwrap();
    session.write_saves_with(clean_writer(LockOnSave::Unlocked), true);
    let copy = original.with_file_name("form redacted.pdf");

    session.save_as(&copy).unwrap();

    assert!(!holds(&fs::read(&copy).unwrap(), SECRET));
    assert!(
        holds(&fs::read(&original).unwrap(), SECRET),
        "Save As leaves the file it came from alone"
    );
}

#[test]
fn once_saves_are_clean_no_later_edit_can_make_them_plain_copies() {
    let Fixture {
        original,
        mut session,
        ..
    } = fixture("protect-no-bypass");
    session.apply(redact_and_clean, show).unwrap();
    session.write_saves_with(clean_writer(LockOnSave::Unlocked), true);
    // a later tool leaves an unused object with the secret in the
    // working copy, the way lopdf keeps replaced objects
    session
        .apply(
            |document: &mut Document| {
                document.add_object(Object::String(
                    SECRET.as_bytes().to_vec(),
                    StringFormat::Literal,
                ));
                Ok::<(), String>(())
            },
            show,
        )
        .unwrap();
    assert!(holds(&fs::read(session.working_copy()).unwrap(), SECRET));

    session.save().unwrap();

    assert!(session.saves_are_rewritten());
    assert!(!holds(&fs::read(&original).unwrap(), SECRET));
}

#[test]
fn the_working_copy_after_a_redaction_holds_no_unused_copy_either() {
    let Fixture { mut session, .. } = fixture("protect-working-copy");
    session.apply(redact_and_clean, show).unwrap();
    assert!(!holds(&fs::read(session.working_copy()).unwrap(), SECRET));
}

#[test]
fn saving_deletes_the_undo_versions_that_still_hold_the_secret() {
    let Fixture { mut session, .. } = fixture("protect-forget");
    session.apply(redact_and_clean, show).unwrap();
    session.write_saves_with(clean_writer(LockOnSave::Unlocked), true);
    let before = session.version_files();
    assert_eq!(
        before.len(),
        2,
        "the current version and the one before the redaction"
    );
    let old = before[1].clone();
    assert!(
        holds(&fs::read(&old).unwrap(), SECRET),
        "undo still works before saving"
    );

    session.save().unwrap();

    assert!(
        !old.exists(),
        "the version before the redaction is gone from the disk"
    );
    assert_eq!(session.version_files().len(), 1);
    assert!(!session.can_undo());
    let folder = session.working_copy().parent().unwrap().to_path_buf();
    for (path, bytes) in all_files(&folder) {
        assert!(
            !holds(&bytes, SECRET),
            "{} still holds the secret",
            path.display()
        );
    }
}

#[test]
fn undo_works_until_the_redaction_is_saved() {
    let Fixture { mut session, .. } = fixture("protect-undo");
    session.apply(redact_and_clean, show).unwrap();
    session.write_saves_with(clean_writer(LockOnSave::Unlocked), true);

    session.undo(show).unwrap();

    assert!(holds(&fs::read(session.working_copy()).unwrap(), SECRET));
    assert!(session.can_redo());
}

#[test]
fn closing_deletes_every_copy_and_a_crash_leaves_none_after_the_next_start() {
    let Fixture {
        root, mut session, ..
    } = fixture("protect-close");
    session.apply(redact_and_clean, show).unwrap();
    assert!(all_files(&root)
        .iter()
        .any(|(_, bytes)| holds(bytes, SECRET)));

    session.discard();
    assert!(all_files(&root).is_empty());

    // a run that crashed left a folder with the secret in it
    let leftover = root.join("4000000000-1");
    fs::create_dir_all(&leftover).unwrap();
    write_secret_pdf(&leftover.join("version-0.pdf"));
    clean_leftover_sessions(&root);
    assert!(all_files(&root).is_empty());
}

#[test]
fn removing_hidden_information_saves_without_the_author() {
    let Fixture {
        original,
        mut session,
        ..
    } = fixture("protect-scrub");
    let items = ScrubOptions {
        items: vec![ScrubItem::DocumentInfo, ScrubItem::XmpMetadata],
    };
    session
        .apply(
            |document: &mut Document| {
                scrub(document, &items)?;
                rewrite_clean(document)
            },
            show,
        )
        .unwrap();
    session.write_saves_with(clean_writer(LockOnSave::Unlocked), true);

    session.save().unwrap();

    assert!(!holds(&fs::read(&original).unwrap(), "Jane Doe"));
}

/// a copy of the secret pdf locked with these options, saved cleanly.
fn locked_pdf(path: &Path, options: &LockOptions) {
    let mut document = secret_pdf();
    encrypt(&mut document, options).unwrap();
    fs::write(path, save_clean(&mut document).unwrap()).unwrap();
}

fn open_password(password: &str, printing: PrintPermission) -> LockOptions {
    LockOptions {
        open_password: password.to_owned(),
        owner_password: String::new(),
        permissions: PermissionOptions {
            printing,
            ..PermissionOptions::default()
        },
    }
}

fn turn_page(document: &mut Document) -> Result<(), String> {
    let page = *document.get_pages().get(&1).ok_or("no page")?;
    document
        .get_dictionary_mut(page)
        .map_err(|error| error.to_string())?
        .set("Rotate", 90);
    Ok(())
}

#[test]
fn a_locked_file_is_unlocked_edited_and_saved_locked_again() {
    let dir = temp_dir("protect-unlock");
    let original = dir.join("locked.pdf");
    locked_pdf(
        &original,
        &open_password("open sesame", PrintPermission::None),
    );
    let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
    assert!(matches!(
        session.apply(turn_page, show),
        Err(crate::session::ApplyError::Session(
            SessionError::PasswordProtected
        ))
    ));
    let bytes = fs::read(session.working_copy()).unwrap();
    let lock = inspect_lock(&bytes).unwrap().unwrap();
    assert!(lock.needs_open_password);
    assert_eq!(lock.method, "AES-256");
    assert_eq!(lock.permissions.printing, PrintPermission::None);

    assert!(matches!(
        unlock_for_editing(&bytes, "wrong"),
        Err(UnlockError::Engine(
            letters_redact::RedactError::WrongPassword
        ))
    ));
    let unlocked = unlock_for_editing(&bytes, "open sesame").unwrap();
    assert_eq!(unlocked.used, PasswordUsed::Open);
    session.start_over_unlocked(&unlocked.bytes, show).unwrap();
    assert!(!session.is_dirty());
    assert!(inspect_lock(&fs::read(session.working_copy()).unwrap())
        .unwrap()
        .is_none());

    session.apply(turn_page, show).unwrap();
    let keep = LockOptions {
        open_password: "open sesame".to_owned(),
        owner_password: String::new(),
        permissions: lock.permissions.clone(),
    };
    session.write_saves_with(clean_writer(LockOnSave::Lock(keep)), false);
    session.save().unwrap();

    let saved = fs::read(&original).unwrap();
    assert!(!holds(&saved, SECRET), "the saved file is encrypted");
    assert_eq!(
        letters_redact::open(&saved).err(),
        Some(letters_redact::RedactError::Locked)
    );
    let reopened = letters_redact::unlock(&saved, "open sesame").unwrap();
    let page = *reopened.get_pages().get(&1).unwrap();
    let rotate = reopened
        .get_dictionary(page)
        .unwrap()
        .get(b"Rotate")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(rotate, 90, "the edit made after unlocking was saved");
    let relocked = inspect_lock(&saved).unwrap().unwrap();
    assert_eq!(relocked.method, "AES-256");
    assert_eq!(
        relocked.permissions.printing,
        PrintPermission::None,
        "the permissions were kept"
    );
}

#[test]
fn a_file_unlocked_with_its_owner_password_waits_for_a_choice_before_saving() {
    let dir = temp_dir("protect-undecided");
    let original = dir.join("locked.pdf");
    let options = LockOptions {
        owner_password: "boss".to_owned(),
        ..open_password("open sesame", PrintPermission::Full)
    };
    locked_pdf(&original, &options);
    let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
    let unlocked = unlock_for_editing(&fs::read(session.working_copy()).unwrap(), "boss").unwrap();
    assert_eq!(unlocked.used, PasswordUsed::Owner);
    session.start_over_unlocked(&unlocked.bytes, show).unwrap();
    session.apply(turn_page, show).unwrap();
    session.write_saves_with(clean_writer(LockOnSave::Undecided), false);
    let before = fs::read(&original).unwrap();

    let result = session.save();

    match result {
        Err(SessionError::SaveRefused { reason, .. }) => assert_eq!(reason, UNDECIDED_MESSAGE),
        other => panic!("expected the save to wait, got {other:?}"),
    }
    assert_eq!(fs::read(&original).unwrap(), before, "nothing was written");
    assert!(session.is_dirty());
}

#[test]
fn a_file_whose_author_forbade_editing_needs_the_owner_password() {
    let dir = temp_dir("protect-owner");
    let path = dir.join("restricted.pdf");
    let options = LockOptions {
        open_password: String::new(),
        owner_password: "boss".to_owned(),
        permissions: PermissionOptions {
            editing: false,
            ..PermissionOptions::default()
        },
    };
    locked_pdf(&path, &options);
    let bytes = fs::read(&path).unwrap();
    let lock = inspect_lock(&bytes).unwrap().unwrap();
    assert!(!lock.needs_open_password);
    assert!(!lock.permissions.editing);

    assert_eq!(
        unlock_for_editing(&bytes, "").err(),
        Some(UnlockError::OwnerPasswordNeeded)
    );
    assert_eq!(
        unlock_for_editing(&bytes, "guess").err(),
        Some(UnlockError::OwnerPasswordNeeded)
    );
    let unlocked = unlock_for_editing(&bytes, "boss").unwrap();
    assert_eq!(unlocked.used, PasswordUsed::Owner);
}

#[test]
fn a_file_that_only_restricts_printing_unlocks_without_a_password() {
    let dir = temp_dir("protect-no-password");
    let path = dir.join("restricted.pdf");
    locked_pdf(&path, &open_password("", PrintPermission::LowQuality));
    let unlocked = unlock_for_editing(&fs::read(&path).unwrap(), "").unwrap();
    assert_eq!(unlocked.used, PasswordUsed::None);
    assert_eq!(
        unlocked.lock.permissions.printing,
        PrintPermission::LowQuality
    );
}

#[test]
fn a_password_change_keeps_the_restrictions_and_the_other_way_round() {
    use super::protection::{merge_lock, LockChange};
    let set_password = LockChange {
        open_password: Some("pw".into()),
        ..LockChange::default()
    };
    let restricted = PermissionOptions {
        copying: false,
        ..PermissionOptions::default()
    };
    let restrict = LockChange {
        permissions: Some(restricted.clone()),
        ..LockChange::default()
    };

    let with_password = merge_lock(None, &set_password);
    let both = merge_lock(Some(&with_password), &restrict);
    match &both {
        LockOnSave::Lock(options) => {
            assert_eq!(options.open_password, "pw");
            assert_eq!(options.permissions, restricted);
        }
        other => panic!("expected a lock, got {other:?}"),
    }
    let no_password = LockChange {
        open_password: Some(String::new()),
        ..LockChange::default()
    };
    assert!(matches!(
        merge_lock(Some(&both), &no_password),
        LockOnSave::Lock(_)
    ));
    let remove = LockChange {
        remove_all: true,
        ..LockChange::default()
    };
    assert_eq!(merge_lock(Some(&both), &remove), LockOnSave::Unlocked);
    assert_eq!(
        merge_lock(Some(&LockOnSave::Undecided), &no_password),
        LockOnSave::Unlocked,
        "an undecided password is decided by the change"
    );
}

#[test]
fn an_open_password_hides_the_thumbnail() {
    use super::protection::Protection;
    let mut protection = Protection {
        opened_with_password: true,
        ..Protection::default()
    };
    assert!(protection.hides_thumbnail());
    protection.lock = Some(LockOnSave::Unlocked);
    assert!(!protection.hides_thumbnail(), "the password was removed");
    protection.lock = Some(LockOnSave::Lock(open_password("pw", PrintPermission::Full)));
    assert!(protection.hides_thumbnail());
}
