//! office documents to pdf through the programs on this computer. the
//! tests that need microsoft office are marked ignored; run them with
//! `cargo test -p letters-convert --test office -- --ignored`.

mod support;

use std::path::{Path, PathBuf};

use letters_convert::*;
use support::*;

/// a word document written by hand: two pages, with a page break.
fn write_docx(path: &Path) {
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let relationships = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:rPr><w:b/><w:sz w:val="36"/></w:rPr><w:t>Letters test document</w:t></w:r></w:p>
<w:p><w:r><w:t>The first page.</w:t></w:r></w:p>
<w:p><w:r><w:br w:type="page"/></w:r></w:p>
<w:p><w:r><w:t>The second page.</w:t></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>
</w:body></w:document>"#;
    write_stored_zip(
        path,
        &[
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", relationships),
            ("word/document.xml", document),
        ],
    );
}

fn convert(input: &Path, name: &str) -> (PathBuf, OfficeConversion) {
    let converters = find_office_converters();
    let output = input.with_file_name(format!("{name}.pdf"));
    let result = office_to_pdf(
        input,
        &output,
        &converters,
        &OfficeOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    (output, result)
}

#[test]
fn what_is_installed_is_found_quickly() {
    // looking only reads the registry and the disk; it starts nothing
    let started = std::time::Instant::now();
    let converters = find_office_converters();
    assert!(started.elapsed().as_secs() < 10);
    println!("found: {:?}", converters.available);
}

fn still_running(pid: u32) -> bool {
    process_ids("WINWORD.EXE")
        .into_iter()
        .chain(process_ids("EXCEL.EXE"))
        .chain(process_ids("POWERPNT.EXE"))
        .any(|p| p == pid)
}

#[test]
#[ignore = "needs Microsoft Word"]
fn word_turns_a_docx_into_a_pdf() {
    let dir = temp_dir("word");
    let input = dir.join("Essay.docx");
    write_docx(&input);
    let before = process_ids("WINWORD.EXE");
    let people_using_word = interactive_word();
    let (pdf, result) = convert(&input, "Essay");
    assert_eq!(result.program, "Microsoft Word");
    let pages = pdf_pages_text(&pdf);
    assert_eq!(pages.len(), 2);
    assert!(pages[0].contains("Letters test document"), "{:?}", pages[0]);
    assert!(pages[0].contains("The first page."));
    assert!(pages[1].contains("The second page."));
    // the word it started is gone again, and a word someone has open
    // with their own documents is untouched (other programs' automation
    // copies come and go on their own, so they aren't compared)
    let ours = result.helper_process.expect("word was started for this");
    assert!(!before.contains(&ours));
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(!still_running(ours), "our copy of word was left running");
    let now = process_ids("WINWORD.EXE");
    assert!(
        people_using_word.iter().all(|p| now.contains(p)),
        "a word someone was using was closed"
    );
}

/// copies of word started by a person (not for automation).
fn interactive_word() -> Vec<u32> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name='WINWORD.EXE'\" | Where-Object { $_.CommandLine -notlike '*/Automation*' -and $_.CommandLine -notlike '*/Embedding*' } | ForEach-Object { $_.ProcessId }",
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

#[test]
#[ignore = "needs Microsoft Word"]
fn word_also_reads_rtf() {
    let dir = temp_dir("rtf");
    let input = dir.join("Note.rtf");
    std::fs::write(
        &input,
        "{\\rtf1\\ansi\\deff0 {\\fonttbl {\\f0 Arial;}}\\f0\\fs24 Hello from RTF.\\par}",
    )
    .unwrap();
    let (pdf, _) = convert(&input, "Note");
    let pages = pdf_pages_text(&pdf);
    assert_eq!(pages.len(), 1);
    assert!(pages[0].contains("Hello from RTF."), "{:?}", pages[0]);
}

#[test]
#[ignore = "needs Microsoft Word"]
fn a_damaged_document_gives_a_sentence() {
    let dir = temp_dir("damaged");
    let input = dir.join("Broken.docx");
    std::fs::write(&input, b"this is not a word document").unwrap();
    let converters = find_office_converters();
    let error = office_to_pdf(
        &input,
        &dir.join("Broken.pdf"),
        &converters,
        &OfficeOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap_err();
    assert!(
        matches!(error, ConvertError::ConverterFailed { .. }),
        "{error:?}"
    );
    assert!(
        error
            .to_string()
            .starts_with("Microsoft Word couldn’t convert this file."),
        "{error}"
    );
    assert!(!dir.join("Broken.pdf").exists());
}

#[test]
#[ignore = "needs Microsoft Excel"]
fn excel_turns_our_spreadsheet_into_a_pdf() {
    let dir = temp_dir("excel-pdf");
    let pdf = dir.join("table.pdf");
    let mut page = TestPage::letter();
    page.text(Font::HelveticaBold, 14.0, 72.0, 80.0, "Scores");
    for (row, (a, b)) in [("Name", "Score"), ("Ana", "9"), ("Ben", "7")]
        .iter()
        .enumerate()
    {
        let y = 110.0 + row as f32 * 16.0;
        page.text(Font::Helvetica, 10.0, 72.0, y, a);
        page.text(Font::Helvetica, 10.0, 200.0, y, b);
        page.text(
            Font::Helvetica,
            10.0,
            300.0,
            y,
            if row == 0 { "Grade" } else { "A" },
        );
    }
    write_pdf(&pdf, &[page]);
    let source = PdfSource::open(&pdf).unwrap();
    let xlsx = dir.join("Scores.xlsx");
    pdf_to_xlsx(
        &source,
        &XlsxExportOptions::default(),
        &xlsx,
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap();
    let (back, result) = convert(&xlsx, "Scores again");
    assert_eq!(result.program, "Microsoft Excel");
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(!still_running(
        result.helper_process.expect("excel was started for this")
    ));
    let text = pdf_pages_text(&back).join("\n");
    assert!(
        text.contains("Scores") && text.contains("Ana") && text.contains("Grade"),
        "{text}"
    );
}

#[test]
#[ignore = "needs Microsoft PowerPoint"]
fn powerpoint_turns_our_slides_into_a_pdf() {
    let dir = temp_dir("powerpoint-pdf");
    let pdf = dir.join("slides.pdf");
    let mut page = TestPage::letter();
    page.text(Font::HelveticaBold, 24.0, 72.0, 100.0, "Round trip");
    page.fill_rect(72.0, 200.0, 200.0, 100.0, [30, 120, 220]);
    write_pdf(&pdf, &[page]);
    let source = PdfSource::open(&pdf).unwrap();
    let pptx = dir.join("Slides.pptx");
    pdf_to_pptx(
        &source,
        &mut TestRenderer::new(),
        &PptxExportOptions::default(),
        &pptx,
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap();
    let (back, result) = convert(&pptx, "Slides again");
    assert_eq!(result.program, "Microsoft PowerPoint");
    let pages = pdf_pages_text(&back);
    assert_eq!(pages.len(), 1);
    assert!(pages[0].contains("Round trip"), "{:?}", pages[0]);
}

#[test]
#[ignore = "needs Microsoft Word"]
fn cancelling_stops_word() {
    let dir = temp_dir("word-cancel");
    let input = dir.join("Essay.docx");
    write_docx(&input);
    let cancel = CancelFlag::new();
    let stopper = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        stopper.cancel();
    });
    let result = office_to_pdf(
        &input,
        &dir.join("Essay.pdf"),
        &find_office_converters(),
        &OfficeOptions::default(),
        &cancel,
    );
    assert_eq!(result.err(), Some(ConvertError::Cancelled));
    assert!(!dir.join("Essay.pdf").exists());
}
