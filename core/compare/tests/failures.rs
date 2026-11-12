//! locked and damaged files, missing renderers, progress and cancelling.

mod support;

use std::sync::atomic::{AtomicBool, Ordering};

use letters_compare::*;
use lopdf::{Document, EncryptionState, EncryptionVersion, Object, Permissions};
use support::*;

#[test]
fn sources_and_results_can_move_to_a_background_thread() {
    // the shell compares on a background thread and hands the result back
    let old = LopdfSource::from_bytes(&pdf_bytes(&pdf(vec![numbered_page(1, 3)]))).unwrap();
    let new = LopdfSource::from_bytes(&pdf_bytes(&pdf(vec![numbered_page(1, 3)]))).unwrap();
    let result = std::thread::spawn(move || compare_documents(&old, &new))
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(result.summary.total, 0);
}

/// a pdf locked with a password, as it comes back from disk.
fn locked_pdf() -> Vec<u8> {
    let mut doc = Document::load_mem(&pdf_bytes(&pdf(vec![numbered_page(1, 3)]))).unwrap();
    // encryption mixes in the file's id, so the file needs one
    let id = Object::string_literal(b"0123456789abcdef".to_vec());
    doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
    let version = EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner",
        user_password: "secret",
        key_length: 128,
        permissions: Permissions::all(),
    };
    let state = EncryptionState::try_from(version).unwrap();
    doc.encrypt(&state).unwrap();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn a_locked_file_asks_to_be_unlocked_first() {
    let error = LopdfSource::from_bytes(&locked_pdf()).err().unwrap();
    assert_eq!(error, CompareError::Locked);
    assert_eq!(
        error.to_string(),
        "This PDF is locked with a password. Unlock it first, then compare again."
    );
}

#[test]
fn a_damaged_file_says_it_cannot_be_read() {
    let error = LopdfSource::from_bytes(b"this is not a pdf").err().unwrap();
    assert!(matches!(error, CompareError::CannotRead(_)), "{error:?}");
    assert!(error.to_string().starts_with("This file can’t be read"));
}

#[test]
fn without_a_renderer_text_is_still_compared_and_the_result_says_why_pictures_were_not() {
    let old = LopdfSource::from_bytes(&pdf_bytes(&pdf(vec![numbered_page(1, 3)]))).unwrap();
    let new = LopdfSource::from_bytes(&pdf_bytes(&pdf(vec![numbered_page(2, 3)]))).unwrap();
    let result = compare_documents(&old, &new).unwrap();
    assert!(result.summary.text > 0);
    assert!(
        result
            .notes
            .iter()
            .any(|note| note.starts_with("Pictures and drawings weren’t compared")),
        "{:?}",
        result.notes
    );
    let error = overlay_image(&old, &new, &result.page_pairs[0], 1.0)
        .err()
        .unwrap();
    assert_eq!(error, CompareError::NoRenderer);
}

#[test]
fn progress_is_reported_page_by_page_in_order() {
    let spec = pdf((0..4).map(|seed| numbered_page(seed, 3)).collect());
    let old = LopdfSource::from_bytes(&pdf_bytes(&spec)).unwrap();
    let new = LopdfSource::from_bytes(&pdf_bytes(&spec)).unwrap();
    let mut seen = Vec::new();
    compare(
        &old,
        &new,
        &CompareOptions::default(),
        &mut |progress| seen.push(progress),
        &AtomicBool::new(false),
    )
    .unwrap();
    let stages: Vec<Stage> = seen.iter().map(|progress| progress.stage).collect();
    assert_eq!(
        stages.len(),
        12,
        "four pages read on each side, then four pairs compared"
    );
    assert!(stages[..4].iter().all(|stage| *stage == Stage::ReadingOld));
    assert!(stages[4..8].iter().all(|stage| *stage == Stage::ReadingNew));
    assert!(stages[8..]
        .iter()
        .all(|stage| *stage == Stage::ComparingPages));
    let last = seen.last().unwrap();
    assert_eq!((last.done, last.total), (4, 4));
}

#[test]
fn a_comparison_can_be_cancelled_between_pages() {
    let spec = pdf((0..6).map(|seed| numbered_page(seed, 3)).collect());
    let old = LopdfSource::from_bytes(&pdf_bytes(&spec)).unwrap();
    let new = LopdfSource::from_bytes(&pdf_bytes(&spec)).unwrap();
    let cancel = AtomicBool::new(false);
    let mut pages_seen = 0;
    let outcome = compare(
        &old,
        &new,
        &CompareOptions::default(),
        &mut |_| {
            pages_seen += 1;
            if pages_seen == 2 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert_eq!(outcome.err(), Some(CompareError::Cancelled));
    assert_eq!(pages_seen, 2, "it stops before the next page");
    assert_eq!(
        CompareError::Cancelled.to_string(),
        "The comparison was stopped."
    );
}

#[test]
fn asking_for_a_page_that_does_not_exist_is_an_error_counted_from_one() {
    let source = LopdfSource::from_bytes(&pdf_bytes(&pdf(vec![numbered_page(1, 3)]))).unwrap();
    assert_eq!(source.page_size(3).err(), Some(CompareError::NoSuchPage(4)));
    assert_eq!(
        read_words(&source, 5).err(),
        Some(CompareError::NoSuchPage(6))
    );
}
