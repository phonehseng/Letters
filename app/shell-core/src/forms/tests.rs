//! Fill & Sign glue run the way the app runs it: signing through an edit
//! session, the guard on a signed file, kept signature pictures, and
//! spotting forms that calculate.

use std::fs;
use std::path::{Path, PathBuf};

use letters_forms::{
    create_self_signed_certificate, sign, verify, CertificateRequest, KeyKind, SignRequest,
    SignatureAppearance, SignaturePlacement,
};
use lopdf::{dictionary, Document, Object};

use super::*;
use crate::session::{EditSession, SessionError};
use crate::test_pdf::{temp_dir, write_pdf};

/// a reload that always works and hands back the file it was asked to show.
fn show(path: &Path) -> Result<PathBuf, String> {
    Ok(path.to_path_buf())
}

fn turn_first_page(document: &mut Document) -> Result<(), String> {
    let first = *document.get_pages().get(&1).ok_or("no pages")?;
    let page = document
        .get_dictionary_mut(first)
        .map_err(|e| e.to_string())?;
    page.set("Rotate", 90);
    Ok(())
}

/// a self-signed certificate made the way Create a Self-Signed Certificate makes one.
fn certificate() -> Vec<u8> {
    create_self_signed_certificate(&CertificateRequest {
        name: "Ada Lovelace".into(),
        email: Some("ada@example.org".into()),
        organization: None,
        valid_days: 30,
        password: "secret".into(),
        // the smaller key keeps the tests quick; the app makes rsa by default
        key_kind: KeyKind::P256,
    })
    .unwrap()
    .pkcs12
}

fn sign_invisibly(
    pkcs12: Vec<u8>,
) -> impl FnOnce(&[u8]) -> Result<Vec<u8>, letters_forms::FormError> {
    move |bytes| {
        sign(
            bytes,
            &SignRequest {
                pkcs12,
                password: "secret".into(),
                placement: SignaturePlacement::Invisible { name: None },
                reason: Some("I filled in this form".into()),
                location: None,
                name: None,
                contact_info: None,
                appearance: SignatureAppearance::default(),
            },
        )
    }
}

struct Signed {
    original: PathBuf,
    session: EditSession,
}

/// a saved two page document, signed through its session.
fn signed_document(test_name: &str) -> Signed {
    let dir = temp_dir(test_name);
    let original = dir.join("form.pdf");
    write_pdf(&original, 2);
    let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
    sign_saved_document(&mut session, false, sign_invisibly(certificate()), show).unwrap();
    Signed { original, session }
}

fn only_report_is_valid(bytes: &[u8]) -> bool {
    let reports = verify(bytes).unwrap();
    reports.len() == 1 && reports[0].valid && !reports[0].changed_after_signing
}

#[test]
fn signing_the_saved_bytes_keeps_the_signature_valid() {
    let Signed { original, session } = signed_document("sign-valid");

    let on_disk = fs::read(&original).unwrap();
    assert!(only_report_is_valid(&on_disk), "the saved file verifies");
    assert_eq!(
        fs::read(session.working_copy()).unwrap(),
        on_disk,
        "what is shown is the signed file, byte for byte"
    );
    assert!(!session.is_dirty());
    assert!(session.can_undo(), "signing is one undo step");
    assert!(session.original_is_protected());
}

#[test]
fn signing_is_refused_while_there_are_unsaved_changes() {
    let dir = temp_dir("sign-dirty");
    let original = dir.join("form.pdf");
    write_pdf(&original, 1);
    let before = fs::read(&original).unwrap();
    let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
    session.apply(turn_first_page, show).unwrap();

    let result = sign_saved_document(&mut session, false, sign_invisibly(certificate()), show);

    assert_eq!(result, Err(SignError::NotSaved));
    assert!(result
        .unwrap_err()
        .to_string()
        .starts_with("Save the document before signing it."));
    assert_eq!(fs::read(&original).unwrap(), before);
    assert!(!session.original_is_protected());
}

#[test]
fn a_wrong_password_changes_nothing() {
    let dir = temp_dir("sign-password");
    let original = dir.join("form.pdf");
    write_pdf(&original, 1);
    let before = fs::read(&original).unwrap();
    let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
    let pkcs12 = certificate();

    let result = sign_saved_document(
        &mut session,
        false,
        |bytes| {
            sign(
                bytes,
                &SignRequest {
                    pkcs12,
                    password: "not it".into(),
                    placement: SignaturePlacement::Invisible { name: None },
                    reason: None,
                    location: None,
                    name: None,
                    contact_info: None,
                    appearance: SignatureAppearance::default(),
                },
            )
        },
        show,
    );

    assert!(matches!(result, Err(SignError::Sign(_))));
    assert_eq!(fs::read(&original).unwrap(), before);
    assert!(!session.can_undo());
}

#[test]
fn a_later_change_is_appended_and_saved_over_the_signed_file() {
    let Signed {
        original,
        mut session,
    } = signed_document("sign-then-edit");
    let signed = fs::read(&original).unwrap();

    session.apply(turn_first_page, show).unwrap();
    session.save().unwrap();

    let saved = fs::read(&original).unwrap();
    assert!(
        saved.starts_with(&signed),
        "the signed bytes are kept as they are"
    );
    let reports = verify(&saved).unwrap();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].valid, "the signature still checks out");
    assert!(reports[0].changed_after_signing, "and the change shows");
    assert!(session.original_is_protected());
}

#[test]
fn a_clean_rewrite_of_a_signed_file_is_refused_and_save_as_makes_a_copy() {
    let Signed {
        original,
        mut session,
    } = signed_document("sign-rewrite");
    let signed = fs::read(&original).unwrap();
    // protect's clean save writer (after a redaction) rewrites every save
    session.write_saves_with(Box::new(|path: &Path| Ok(fs::read(path).unwrap())), false);
    session.apply(turn_first_page, show).unwrap();

    let result = session.save();

    assert!(matches!(result, Err(SessionError::SignedOriginal { .. })));
    assert!(result.unwrap_err().to_string().contains("Save As…"));
    assert_eq!(
        fs::read(&original).unwrap(),
        signed,
        "the signed file is untouched"
    );
    let copy = original.with_file_name("form edited.pdf");
    session.save_as(&copy).unwrap();
    assert!(!session.original_is_protected(), "the copy was rewritten");
    assert_eq!(fs::read(&original).unwrap(), signed);
}

#[test]
fn undoing_the_signature_keeps_the_signed_file_safe() {
    let Signed {
        original,
        mut session,
    } = signed_document("sign-undo");
    let signed = fs::read(&original).unwrap();

    session.undo(show).unwrap();

    assert!(session.is_dirty(), "the file on disk is the signed one");
    assert!(matches!(
        session.save(),
        Err(SessionError::SignedOriginal { .. })
    ));
    assert_eq!(fs::read(&original).unwrap(), signed);
}

#[test]
fn save_as_of_an_unchanged_signed_document_keeps_the_guard() {
    let Signed {
        original,
        mut session,
    } = signed_document("sign-copy");
    let copy = original.with_file_name("copy.pdf");

    session.save_as(&copy).unwrap();

    assert!(
        only_report_is_valid(&fs::read(&copy).unwrap()),
        "an exact copy is still signed"
    );
    assert!(session.original_is_protected());
}

#[test]
fn a_second_signature_keeps_the_first_valid() {
    let Signed {
        original,
        mut session,
    } = signed_document("sign-twice");

    sign_saved_document(&mut session, false, sign_invisibly(certificate()), show).unwrap();

    let reports = verify(&fs::read(&original).unwrap()).unwrap();
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|report| report.valid));
}

// ---- kept signature pictures ----

/// a 3 by 2 pixel png, made with the same crate the engine reads pictures with.
fn tiny_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 3, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0; 3 * 2 * 4]).unwrap();
    }
    bytes
}

#[test]
fn signature_pictures_are_kept_listed_read_and_deleted() {
    let store = SignaturePictures::new(temp_dir("pictures").join("signatures"));
    assert!(store.list().is_empty(), "a missing folder means none yet");

    let drawn = store.add(&tiny_png(), PictureKind::Drawn).unwrap();
    let typed = store.add(&tiny_png(), PictureKind::Typed).unwrap();

    assert_eq!((drawn.width, drawn.height), (3, 2));
    let listed: Vec<String> = store.list().into_iter().map(|saved| saved.id).collect();
    assert_eq!(
        listed,
        vec![typed.id.clone(), drawn.id.clone()],
        "newest first"
    );
    assert_eq!(store.read(&drawn.id).unwrap(), tiny_png());

    store.delete(&drawn.id).unwrap();
    assert_eq!(store.list().len(), 1);
    assert!(store.read(&drawn.id).is_err());
}

#[test]
fn only_pngs_are_kept_and_ids_cannot_reach_other_files() {
    let folder = temp_dir("pictures-bad").join("signatures");
    let store = SignaturePictures::new(folder.clone());

    assert!(store.add(b"not a picture", PictureKind::Image).is_err());
    assert!(store.read("../settings").is_err());
    assert!(store.delete("..\\..\\recent-files").is_err());
}

#[test]
fn the_oldest_pictures_go_once_there_are_too_many() {
    let store = SignaturePictures::new(temp_dir("pictures-many").join("signatures"));
    let first = store.add(&tiny_png(), PictureKind::Drawn).unwrap();
    for _ in 0..signature_pictures::MAX_SAVED {
        store.add(&tiny_png(), PictureKind::Drawn).unwrap();
    }

    assert_eq!(store.list().len(), signature_pictures::MAX_SAVED);
    assert!(store.read(&first.id).is_err(), "its file is removed too");
}

// ---- forms that calculate ----

/// a one page document whose form holds `fields` (and `extra` in /AcroForm).
fn form_with(fields: Vec<lopdf::Dictionary>, extra: Option<(&str, Object)>) -> Document {
    let mut document = Document::with_version("1.7");
    let pages = document.new_object_id();
    let page = document.add_object(dictionary! { "Type" => "Page", "Parent" => pages });
    document.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let field_refs: Vec<Object> = fields
        .into_iter()
        .map(|field| document.add_object(field).into())
        .collect();
    let mut form = dictionary! { "Fields" => field_refs };
    if let Some((key, value)) = extra {
        form.set(key, value);
    }
    let form = document.add_object(form);
    let catalog = document
        .add_object(dictionary! { "Type" => "Catalog", "Pages" => pages, "AcroForm" => form });
    document.trailer.set("Root", catalog);
    document
}

fn javascript(script: &str) -> Object {
    Object::Dictionary(dictionary! { "S" => "JavaScript", "JS" => Object::string_literal(script) })
}

#[test]
fn a_plain_form_or_no_form_has_no_calculations() {
    let mut no_form = Document::with_version("1.7");
    let catalog = no_form.add_object(dictionary! { "Type" => "Catalog" });
    no_form.trailer.set("Root", catalog);
    assert!(!has_calculations(&no_form));

    let plain = form_with(
        vec![dictionary! { "FT" => "Tx", "T" => Object::string_literal("Name") }],
        None,
    );
    assert!(!has_calculations(&plain));
}

#[test]
fn a_date_format_script_is_not_a_calculation() {
    let date = dictionary! {
        "FT" => "Tx", "T" => Object::string_literal("Date"),
        "AA" => dictionary! { "F" => javascript("AFDate_FormatEx(\"dd/mm/yyyy\");") },
    };
    assert!(!has_calculations(&form_with(vec![date], None)));
}

#[test]
fn a_calculation_order_or_a_calculate_action_is_found() {
    let total = dictionary! {
        "FT" => "Tx", "T" => Object::string_literal("Total"),
        "AA" => dictionary! { "C" => javascript("AFSimple_Calculate(\"SUM\", [\"A\", \"B\"]);") },
    };
    assert!(has_calculations(&form_with(vec![total.clone()], None)));

    let order = Object::Array(vec![Object::Integer(0)]);
    let plain = dictionary! { "FT" => "Tx", "T" => Object::string_literal("A") };
    assert!(has_calculations(&form_with(
        vec![plain],
        Some(("CO", order))
    )));

    // deep in the tree, as forms with grouped names are
    let mut document = form_with(vec![], None);
    let child = document.add_object(total);
    let parent = document.add_object(
        dictionary! { "T" => Object::string_literal("Sums"), "Kids" => vec![child.into()] },
    );
    let form = document
        .catalog()
        .unwrap()
        .get(b"AcroForm")
        .unwrap()
        .as_reference()
        .unwrap();
    document
        .get_dictionary_mut(form)
        .unwrap()
        .set("Fields", vec![Object::Reference(parent)]);
    assert!(has_calculations(&document));
}

// ---- chosen files ----

#[test]
fn pictures_become_data_addresses() {
    assert_eq!(files::base64(b""), "");
    assert_eq!(files::base64(b"f"), "Zg==");
    assert_eq!(files::base64(b"fo"), "Zm8=");
    assert_eq!(files::base64(b"foo"), "Zm9v");
    assert_eq!(files::base64(b"foobar"), "Zm9vYmFy");
    assert_eq!(
        data_url("image/png", b"\x89PNG"),
        "data:image/png;base64,iVBORw=="
    );
    assert_eq!(picture_mime(Path::new("Sig.JPG")), Some("image/jpeg"));
    assert_eq!(picture_mime(Path::new("sig.gif")), None);
}

#[test]
fn chosen_files_are_read_within_a_limit_and_written_in_one_step() {
    let dir = temp_dir("chosen-files");
    let path = dir.join("Ada.p12");

    write_chosen_bytes(&path, b"certificate bytes").unwrap();

    assert_eq!(read_chosen_file(&path, 1024).unwrap(), b"certificate bytes");
    let too_big = read_chosen_file(&path, 4).unwrap_err();
    assert!(too_big.contains("too big"), "{too_big}");
    assert!(read_chosen_file(&dir.join("gone.p12"), 1024).is_err());
    let leftovers = fs::read_dir(&dir).unwrap().count();
    assert_eq!(leftovers, 1, "no temporary file is left behind");
}

// ---- signing and Protect (passwords, clean saves) ----

mod with_protect {
    use std::fs;
    use std::path::Path;

    use letters_forms::verify;
    use letters_redact::lopdf::{dictionary, Document, Object, Stream};
    use letters_redact::{redact, LockOptions, PageRect, RedactOptions, RedactionArea};

    use super::{certificate, show, sign_invisibly};
    use crate::forms::{password_blocks_signing, sign_saved_document, SignError, PASSWORD_MESSAGE};
    use crate::protect::clean_save::{clean_writer, rewrite_clean, LockOnSave};
    use crate::protect::protection::Protection;
    use crate::protect::unlock::inspect_lock;
    use crate::session::EditSession;
    use crate::test_pdf::temp_dir;

    const SECRET: &str = "QX-SIGNED-SECRET-981";

    /// a one page pdf with a secret line and a line to keep.
    fn write_secret_pdf(path: &Path) {
        let mut document = Document::with_version("1.7");
        let pages = document.new_object_id();
        let font = document.add_object(
            dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
        );
        let text = format!(
            "BT /F1 24 Tf 72 700 Td ({SECRET}) Tj ET BT /F1 12 Tf 72 400 Td (Kept words) Tj ET"
        );
        let content = document.add_object(Stream::new(dictionary! {}, text.into_bytes()));
        let page = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "Contents" => content,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        document.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        document.trailer.set("Root", catalog);
        document.save(path).unwrap();
    }

    /// the secret anywhere in the bytes, or in any stream once unpacked.
    fn holds_secret(bytes: &[u8]) -> bool {
        let needle = SECRET.as_bytes();
        let found = |haystack: &[u8]| {
            haystack
                .windows(needle.len())
                .any(|window| window == needle)
        };
        if found(bytes) {
            return true;
        }
        let Ok(document) = Document::load_mem(bytes) else {
            return false;
        };
        document.objects.values().any(|object| match object {
            Object::Stream(stream) => found(
                &stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone()),
            ),
            Object::String(text, _) => found(text),
            _ => false,
        })
    }

    /// what the shell's apply_redactions does to the working copy.
    fn redact_and_clean(document: &mut Document) -> Result<(), letters_redact::RedactError> {
        let area = RedactionArea {
            page: 0,
            rects: vec![PageRect::new(60.0, 60.0, 300.0, 50.0)],
        };
        redact(document, &[area], &RedactOptions::default())?;
        rewrite_clean(document)
    }

    fn locked() -> LockOnSave {
        LockOnSave::Lock(LockOptions {
            open_password: "open sesame".into(),
            ..LockOptions::default()
        })
    }

    #[test]
    fn protect_s_record_says_when_a_password_stands_in_the_way() {
        let mut protection = Protection::default();
        assert!(!password_blocks_signing(&protection));
        protection.lock = Some(locked());
        assert!(password_blocks_signing(&protection));
        protection.lock = Some(LockOnSave::Undecided);
        assert!(password_blocks_signing(&protection));
        protection.lock = Some(LockOnSave::Unlocked);
        assert!(
            !password_blocks_signing(&protection),
            "the password was removed"
        );
        protection.lock = None;
        protection.opened_with_password = true;
        assert!(
            password_blocks_signing(&protection),
            "the file on disk still has one"
        );
    }

    #[test]
    fn a_password_set_then_signing_is_refused_and_the_locked_file_is_untouched() {
        let dir = temp_dir("sign-password-set");
        let original = dir.join("form.pdf");
        write_secret_pdf(&original);
        let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
        // what the Protect tab's Set Password does, then Save
        session.write_saves_with(clean_writer(locked()), false);
        session.mark_unsaved();
        session.save().unwrap();
        let saved = fs::read(&original).unwrap();
        assert!(
            matches!(inspect_lock(&saved), Ok(Some(_))),
            "the saved file is encrypted"
        );
        let protection = Protection {
            lock: Some(locked()),
            ..Protection::default()
        };

        let result = sign_saved_document(
            &mut session,
            password_blocks_signing(&protection),
            sign_invisibly(certificate()),
            show,
        );

        assert_eq!(result, Err(SignError::PasswordProtected));
        assert_eq!(result.unwrap_err().to_string(), PASSWORD_MESSAGE);
        assert_eq!(
            fs::read(&original).unwrap(),
            saved,
            "still the locked file, byte for byte"
        );
        assert!(!session.original_is_protected());
    }

    #[test]
    fn a_locked_file_on_disk_is_never_signed_even_without_protect_s_record() {
        let dir = temp_dir("sign-locked-disk");
        let original = dir.join("form.pdf");
        write_secret_pdf(&original);
        let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
        session.write_saves_with(clean_writer(locked()), false);
        session.mark_unsaved();
        session.save().unwrap();
        let saved = fs::read(&original).unwrap();

        let result = sign_saved_document(&mut session, false, sign_invisibly(certificate()), show);

        assert_eq!(result, Err(SignError::PasswordProtected));
        assert_eq!(fs::read(&original).unwrap(), saved);
    }

    #[test]
    fn redacting_then_signing_signs_the_clean_file_and_keeps_no_secret() {
        let dir = temp_dir("sign-redacted");
        let original = dir.join("form.pdf");
        write_secret_pdf(&original);
        let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
        session.apply(redact_and_clean, show).unwrap();
        session.write_saves_with(clean_writer(LockOnSave::Unlocked), true);
        session.save().unwrap();
        // a later tool leaves an unused object holding the secret in the
        // working copy; the clean file on disk doesn't have it
        session
            .apply(
                |document: &mut Document| {
                    document.add_object(Stream::new(dictionary! {}, SECRET.as_bytes().to_vec()));
                    Ok::<(), String>(())
                },
                show,
            )
            .unwrap();
        session.save().unwrap();
        assert!(
            holds_secret(&fs::read(session.working_copy()).unwrap()),
            "the working copy still holds it"
        );

        sign_saved_document(&mut session, false, sign_invisibly(certificate()), show).unwrap();

        let signed = fs::read(&original).unwrap();
        assert!(!holds_secret(&signed), "the signed file holds no secret");
        let reports = verify(&signed).unwrap();
        assert!(reports.len() == 1 && reports[0].valid && !reports[0].changed_after_signing);
        for version in session.version_files() {
            assert!(
                !holds_secret(&fs::read(&version).unwrap()),
                "no cached version holds it: {version:?}"
            );
        }
        assert!(
            session.saves_are_rewritten(),
            "later saves are still clean ones"
        );

        // an exact copy keeps the signature, although saves are rewritten
        let copy = original.with_file_name("copy.pdf");
        session.save_as(&copy).unwrap();
        let copied = fs::read(&copy).unwrap();
        assert_eq!(copied, signed);
        assert!(verify(&copied).unwrap()[0].valid);
    }
}
