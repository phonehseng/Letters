//! converting and optimising copies the way Prepare & Check does, checked
//! with core/standards' own checker.

use std::fs;

use letters_standards::{check_pdfa_file, OptimizeOptions, PdfaLevel};

use super::*;
use crate::test_pdf::{temp_dir, write_pdf};

#[test]
fn a_plain_document_converts_to_pdfa_2b_and_the_source_is_untouched() {
    let dir = temp_dir("standards-convert");
    let source = dir.join("essay.pdf");
    write_pdf(&source, 2);
    let before = fs::read(&source).unwrap();

    let converted = convert_copy(&source, PdfaLevel::A2b, None, None).unwrap();

    assert!(
        converted.report.compliant,
        "{:?}",
        converted.report.blockers
    );
    assert!(
        converted.report.output_intent_added,
        "device rgb needs an rgb output intent"
    );
    let check = check_pdfa_file(&converted.bytes, PdfaLevel::A2b).unwrap();
    assert!(check.compliant, "{:?}", check.findings);
    assert_eq!(fs::read(&source).unwrap(), before);
}

#[test]
fn the_part_two_and_three_levels_convert_a_plain_document() {
    let dir = temp_dir("standards-levels");
    let source = dir.join("essay.pdf");
    write_pdf(&source, 1);
    // pdf/a-1b is left out: a source saved with a cross-reference stream
    // (as lopdf saves it) keeps that stream object through the conversion,
    // and the checker rightly refuses it for part 1 (core/standards,
    // reported in docs/worklogs/shell-core-standards.md)
    for level in [PdfaLevel::A2b, PdfaLevel::A2u, PdfaLevel::A3b] {
        let converted = convert_copy(&source, level, None, None).unwrap();
        let check = check_pdfa_file(&converted.bytes, level).unwrap();
        assert!(check.compliant, "{level:?}: {:?}", check.findings);
    }
}

#[test]
fn a_signed_document_is_not_converted_because_its_signatures_would_break() {
    let dir = temp_dir("standards-signed");
    let source = dir.join("signed.pdf");
    write_pdf(&source, 1);
    // a signature dictionary's byte range is what marks a signed file
    let mut bytes = fs::read(&source).unwrap();
    bytes.extend_from_slice(b"\n% /ByteRange [0 10 20 30]\n");
    fs::write(&source, bytes).unwrap();

    let refused = convert_copy(&source, PdfaLevel::A2b, None, None);

    assert_eq!(refused.err().as_deref(), Some(SIGNED_SOURCE));
}

#[test]
fn a_file_that_is_not_a_pdf_is_refused_with_a_sentence() {
    let dir = temp_dir("standards-not-pdf");
    let source = dir.join("notes.pdf");
    fs::write(&source, b"just some text").unwrap();
    assert!(convert_copy(&source, PdfaLevel::A2b, None, None).is_err());
    assert!(optimized_copy(&source, &OptimizeOptions::default()).is_err());
}

#[test]
fn an_optimised_copy_reports_what_it_saved_and_what_it_did_not_do() {
    let dir = temp_dir("standards-optimise");
    let source = dir.join("essay.pdf");
    write_pdf(&source, 3);

    let report = optimized_copy(&source, &OptimizeOptions::default()).unwrap();

    assert_eq!(report.bytes_after as usize, report.bytes.len());
    assert!(
        !report.not_done.is_empty(),
        "what wasn't done is always said"
    );
    assert!(lopdf::Document::load_mem(&report.bytes).is_ok());
}

#[test]
fn see_through_pixels_are_laid_on_white() {
    let rgba = [255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 128];
    assert_eq!(
        rgb_on_white(&rgba),
        vec![255, 0, 0, 255, 255, 255, 127, 127, 127]
    );
}
