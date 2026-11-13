// Convert File…: which types each kind of file is offered, and real
// conversions between the types Letters writes itself, read back.

use super::*;
use letters_convert::OfficeConverters;
use std::path::PathBuf;

/// an empty folder of its own for each test.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("letters-convert-file-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn formats(choices: &ConvertChoices) -> Vec<TargetFormat> {
    choices.targets.iter().map(|target| target.format).collect()
}

#[test]
fn each_kind_of_file_is_offered_the_other_types_of_its_family() {
    let none = OfficeConverters::default();
    let essay = choices_with(Path::new("Essay.docx"), &none);
    assert_eq!(essay.source, ConvertSource::Document);
    assert_eq!(formats(&essay), [TargetFormat::Odt, TargetFormat::Pdf]);
    let notes = choices_with(Path::new("Notes.rtf"), &none);
    assert_eq!(
        formats(&notes),
        [TargetFormat::Docx, TargetFormat::Odt, TargetFormat::Pdf]
    );
    let grades = choices_with(Path::new("Grades.ODS"), &none);
    assert_eq!(
        formats(&grades),
        [TargetFormat::Xlsx, TargetFormat::Csv, TargetFormat::Pdf]
    );
    let talk = choices_with(Path::new("Talk.pptx"), &none);
    assert_eq!(formats(&talk), [TargetFormat::Odp, TargetFormat::Pdf]);
}

#[test]
fn pdfs_and_pictures_are_sent_to_their_own_tools() {
    let none = OfficeConverters::default();
    let pdf = choices_with(Path::new("Lab Report.pdf"), &none);
    assert_eq!(pdf.source, ConvertSource::Pdf);
    assert!(pdf.targets.is_empty());
    assert_eq!(source_of(Path::new("scan.JPG")), ConvertSource::Picture);
    assert_eq!(source_of(Path::new("notes.zip")), ConvertSource::Other);
}

#[test]
fn a_pdf_without_office_or_libreoffice_is_offered_but_says_what_it_needs() {
    let essay = choices_with(Path::new("Essay.odt"), &OfficeConverters::default());
    let pdf = essay.targets.last().unwrap();
    assert_eq!(pdf.format, TargetFormat::Pdf);
    assert!(!pdf.available);
    assert!(pdf.note.contains("LibreOffice"), "{}", pdf.note);
}

#[test]
fn a_word_document_becomes_an_opendocument_text_with_the_same_words() {
    let dir = folder("docx-odt");
    let mut document = letters_office::model::Document::new();
    document.blocks = vec![letters_office::model::Block::text(
        "Photosynthesis needs light.",
    )];
    let docx = dir.join("Essay.docx");
    std::fs::write(&docx, letters_office::write_docx(&document).unwrap()).unwrap();
    // typed without an extension: the chosen type adds it
    let made = convert(&docx, TargetFormat::Odt, &dir.join("Essay")).unwrap();
    assert_eq!(made.path, dir.join("Essay.odt").to_string_lossy());
    let bytes = std::fs::read(dir.join("Essay.odt")).unwrap();
    assert_eq!(letters_office::detect(&bytes).unwrap(), FileKind::Odt);
    let back = letters_office::open(&bytes).unwrap();
    assert_eq!(back.blocks.len(), 1);
    // the original is only read
    assert!(docx.is_file());
}

#[test]
fn a_csv_becomes_a_workbook_and_an_opendocument_spreadsheet() {
    let dir = folder("csv");
    let csv = dir.join("Grades.csv");
    std::fs::write(&csv, "Name,Score\nAda,91\nGrace,88\n").unwrap();
    for format in [TargetFormat::Xlsx, TargetFormat::Ods] {
        let made = convert(
            &csv,
            format,
            &dir.join(format!("Grades.{}", format.extension())),
        )
        .unwrap();
        let (book, _) = read_workbook(Path::new(&made.path)).unwrap();
        assert_eq!(book.sheets.len(), 1, "{format:?}");
    }
}

#[test]
fn a_powerpoint_becomes_an_opendocument_presentation_and_back() {
    let dir = folder("pptx-odp");
    let pptx = crate::slides::test_decks::deck_path("lo_shapes.pptx");
    let odp = convert(&pptx, TargetFormat::Odp, &dir.join("Talk.odp")).unwrap();
    assert!(SlideDeck::open(Path::new(&odp.path)).is_ok());
    let again = convert(
        Path::new(&odp.path),
        TargetFormat::Pptx,
        &dir.join("Talk.pptx"),
    )
    .unwrap();
    assert!(SlideDeck::open(Path::new(&again.path)).is_ok());
}

#[test]
fn converting_onto_the_file_itself_is_refused() {
    let dir = folder("itself");
    let csv = dir.join("Grades.csv");
    std::fs::write(&csv, "a,b\n").unwrap();
    let error = convert(&csv, TargetFormat::Csv, &csv).unwrap_err();
    assert!(error.contains("different name"), "{error}");
    assert_eq!(std::fs::read_to_string(&csv).unwrap(), "a,b\n");
}

#[test]
fn a_type_from_another_family_is_refused_in_words() {
    let dir = folder("family");
    let csv = dir.join("Grades.csv");
    std::fs::write(&csv, "a,b\n").unwrap();
    let error = convert(&csv, TargetFormat::Docx, &dir.join("Grades.docx")).unwrap_err();
    assert!(error.contains(".docx"), "{error}");
}
