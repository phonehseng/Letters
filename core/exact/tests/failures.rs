//! failure paths and honest reports, without word: locked and broken
//! files, pdfs with no text, word missing (simulated), cancelling, and the
//! notices a person must see.

use std::path::{Path, PathBuf};

use letters_exact::{convert, CancelFlag, ConvertOptions, ExactError};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use lopdf::{dictionary, Document, Object, Stream};

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
        .join(name)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("letters-exact-failures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn without_word() -> ConvertOptions {
    ConvertOptions {
        use_word: false,
        ..ConvertOptions::default()
    }
}

fn locked_copy(user_password: &str, name: &str) -> PathBuf {
    let mut document = Document::load(corpus("form.pdf")).unwrap();
    let version = EncryptionVersion::V2 {
        document: &document,
        owner_password: "owner",
        user_password,
        key_length: 128,
        permissions: Permissions::all(),
    };
    let state = EncryptionState::try_from(version).unwrap();
    document.encrypt(&state).unwrap();
    let path = scratch(name);
    document.save(&path).unwrap();
    path
}

#[test]
fn a_locked_pdf_must_be_unlocked_first() {
    for (password, name) in [("secret", "locked.pdf"), ("", "owner-locked.pdf")] {
        let path = locked_copy(password, name);
        let error = convert(&path, &scratch("locked.docx"), &without_word()).unwrap_err();
        assert_eq!(error, ExactError::Locked, "{name}");
        assert!(error.to_string().contains("Unlock it first"));
    }
}

#[test]
fn a_file_that_is_not_a_pdf_is_refused() {
    let path = scratch("notes.pdf");
    std::fs::write(&path, b"just some notes").unwrap();
    let error = convert(&path, &scratch("notes.docx"), &without_word()).unwrap_err();
    assert!(matches!(error, ExactError::CannotOpen(_)), "{error:?}");
    let missing = convert(
        &scratch("not-there.pdf"),
        &scratch("x.docx"),
        &without_word(),
    )
    .unwrap_err();
    assert!(matches!(missing, ExactError::CannotOpen(_)));
}

#[test]
fn a_pdf_without_text_has_nothing_to_convert() {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let content = document.add_object(Stream::new(
        dictionary! {},
        b"0 0 1 rg 10 10 100 100 re f".to_vec(),
    ));
    let page = document.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let path = scratch("drawing.pdf");
    document.save(&path).unwrap();
    let error = convert(&path, &scratch("drawing.docx"), &without_word()).unwrap_err();
    assert_eq!(error, ExactError::NoText);
}

#[test]
fn without_word_the_conversion_still_works_and_says_so() {
    let options = ConvertOptions {
        verify: true,
        ..without_word()
    };
    let docx = scratch("syllabus.docx");
    let report = convert(&corpus("syllabus.pdf"), &docx, &options).unwrap();
    assert!(docx.is_file());
    assert!(!report.word.available);
    assert_eq!(
        report.word.note,
        "Not checked against Word on this computer."
    );
    assert!(report
        .warnings
        .iter()
        .any(|w| w == "Not checked against Word on this computer."));
    assert!(report.verification.is_none());
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("couldn’t be checked")));
    assert!(report.calibration.built_in_tables);
    assert!(report.calibration.estimated_steps.is_empty());
}

#[test]
fn a_cancelled_conversion_stops() {
    let cancel = CancelFlag::new();
    cancel.cancel();
    let options = ConvertOptions {
        cancel,
        ..without_word()
    };
    assert_eq!(
        convert(&corpus("form.pdf"), &scratch("form.docx"), &options).unwrap_err(),
        ExactError::Cancelled
    );
}

#[test]
fn restricted_and_missing_fonts_are_named_in_plain_sentences() {
    let report = convert(
        &corpus("restricted_font.pdf"),
        &scratch("restricted.docx"),
        &without_word(),
    )
    .unwrap();
    assert!(
        report.warnings.contains(
            &"“LettersLockedSans-Regular” doesn’t allow embedding, so a similar font was used."
                .to_string()
        ),
        "{:?}",
        report.warnings
    );
    let face = &report.fonts[0];
    assert_eq!(face.fs_type, Some(2));
    assert!(face.substitute.as_deref().unwrap().contains("Arimo"));
    let form = convert(&corpus("form.pdf"), &scratch("form2.docx"), &without_word()).unwrap();
    assert!(
        form.warnings
            .iter()
            .any(|w| w == "“Helvetica” isn’t included in the PDF, so a similar font was used."),
        "{:?}",
        form.warnings
    );
}

#[test]
fn unmeasured_page_sizes_are_estimated_and_reported() {
    // letter and a4 are measured; a 500 x 800 pt page is not
    let mut document = Document::load(corpus("form.pdf")).unwrap();
    let pages: Vec<_> = document.get_pages().into_values().collect();
    for page in pages {
        let dict = document.get_dictionary_mut(page).unwrap();
        dict.set("MediaBox", vec![0.into(), 0.into(), 500.into(), 800.into()]);
    }
    let path = scratch("odd-size.pdf");
    document.save(&path).unwrap();
    let report = convert(&path, &scratch("odd-size.docx"), &without_word()).unwrap();
    assert_eq!(report.calibration.estimated_steps, ["x:4167", "y:6667"]);
    assert!(report.warnings.iter().any(|w| w.contains("estimated")));
}

#[test]
fn every_page_reports_what_it_holds() {
    let report = convert(
        &corpus("lab_report.pdf"),
        &scratch("lab.docx"),
        &without_word(),
    )
    .unwrap();
    assert_eq!(report.pages.len(), 3);
    assert_eq!(report.pages.iter().map(|p| p.pictures).sum::<usize>(), 2);
    assert_eq!(report.pages.iter().map(|p| p.jpeg_kept).sum::<usize>(), 1);
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"wordBuild\""));
}
