//! damaged, hostile and unsupported files give clear errors, never a
//! panic.

mod support;

use letters_office::{open, read_docx, read_odt, write_docx, write_odt, OfficeError};
use support::fixtures::rich_docx;
use support::*;

/// a small deterministic random number generator, so failures repeat.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// opening must not panic; when it works, saving must not panic either.
fn try_everything(bytes: &[u8]) {
    if let Ok(document) = open(bytes) {
        let _ = write_docx(&document);
        let _ = write_odt(&document);
        let _ = letters_office::export::to_html(&document);
        let _ = letters_office::counts::count(&document);
    }
}

#[test]
fn truncated_packages_never_panic() {
    let samples = [rich_docx(), odt(&p_odt("text"), "")];
    for bytes in &samples {
        let mut length = 0;
        while length < bytes.len() {
            try_everything(&bytes[..length]);
            length += 61;
        }
    }
}

fn p_odt(text: &str) -> String {
    format!("<text:p>{text}</text:p>")
}

#[test]
fn flipped_bytes_never_panic() {
    let original = rich_docx();
    let mut random = Lcg(42);
    for _ in 0..300 {
        let mut bytes = original.clone();
        for _ in 0..4 {
            let index = (random.next() as usize) % bytes.len();
            bytes[index] ^= (random.next() & 0xFF) as u8 | 1;
        }
        try_everything(&bytes);
    }
}

/// every part of the rich document replaced by broken xml in turn.
#[test]
fn broken_xml_in_any_part_is_an_error_or_survivable() {
    let original = parts_of(&rich_docx());
    for victim in 0..original.len() {
        for damage in [
            "<w:p><w:r>",
            "not xml at all",
            "<a></b>",
            "",
            "<x:y xmlns:x=\"\"/>&bogus;",
        ] {
            let parts: Vec<(&str, Vec<u8>)> = original
                .iter()
                .enumerate()
                .map(|(index, (name, bytes))| {
                    let bytes = if index == victim {
                        damage.as_bytes().to_vec()
                    } else {
                        bytes.clone()
                    };
                    (name.as_str(), bytes)
                })
                .collect();
            try_everything(&zip(&parts));
        }
    }
}

#[test]
fn broken_main_document_names_the_part() {
    let original = parts_of(&rich_docx());
    let parts: Vec<(&str, Vec<u8>)> = original
        .iter()
        .map(|(name, bytes)| {
            let bytes = if name == "word/document.xml" {
                b"<w:document><w:body>".to_vec()
            } else {
                bytes.clone()
            };
            (name.as_str(), bytes)
        })
        .collect();
    match read_docx(&zip(&parts)) {
        Err(OfficeError::BadXml { part, detail }) => {
            assert_eq!(part, "word/document.xml");
            assert!(detail.contains("ends before"), "{detail}");
        }
        other => panic!("expected a clear error, got {other:?}"),
    }
}

#[test]
fn missing_main_document_is_damaged() {
    let original = parts_of(&rich_docx());
    let parts: Vec<(&str, Vec<u8>)> = original
        .iter()
        .filter(|(name, _)| name != "word/document.xml")
        .map(|(name, bytes)| (name.as_str(), bytes.clone()))
        .collect();
    assert!(matches!(
        read_docx(&zip(&parts)),
        Err(OfficeError::Damaged(_))
    ));
    let odt_without_content = zip(&[(
        "mimetype",
        b"application/vnd.oasis.opendocument.text".to_vec(),
    )]);
    assert!(matches!(
        read_odt(&odt_without_content),
        Err(OfficeError::Damaged(_))
    ));
}

fn ooxml_with(content_type: &str) -> Vec<u8> {
    let types = format!(
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override PartName=\"/x.xml\" ContentType=\"{content_type}\"/></Types>"
    );
    zip(&[("[Content_Types].xml", types.into_bytes())])
}

#[test]
fn spreadsheets_and_presentations_are_not_supported_yet() {
    let cases = [
        (ooxml_with("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"), ".xlsx"),
        (ooxml_with("application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"), ".pptx"),
        (zip(&[("mimetype", b"application/vnd.oasis.opendocument.spreadsheet".to_vec())]), ".ods"),
        (zip(&[("mimetype", b"application/vnd.oasis.opendocument.presentation".to_vec())]), ".odp"),
    ];
    for (bytes, extension) in cases {
        match open(&bytes) {
            Err(OfficeError::NotSupportedYet(what)) => {
                assert!(what.contains(extension), "{what}");
                let message = OfficeError::NotSupportedYet(what).to_string();
                assert!(
                    message.contains("can’t open") && message.contains("yet"),
                    "{message}"
                );
            }
            other => panic!("{extension}: {other:?}"),
        }
    }
}

#[test]
fn old_and_locked_word_files_are_explained() {
    let mut ole = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    ole.extend_from_slice(&[0; 512]);
    assert_eq!(open(&ole), Err(OfficeError::LegacyWordFormat));
    let marker: Vec<u8> = "EncryptionInfo"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    ole.extend_from_slice(&marker);
    assert_eq!(open(&ole), Err(OfficeError::Encrypted));
    assert!(matches!(
        open(b"just some text"),
        Err(OfficeError::NotAPackage(_))
    ));
    assert!(matches!(open(&[]), Err(OfficeError::NotAPackage(_))));
}

#[test]
fn deeply_nested_xml_is_refused_cleanly() {
    let body = "<w:tbl><w:tr><w:tc>".repeat(200) + &"</w:tc></w:tr></w:tbl>".repeat(200);
    let bytes = Docx::new(&body).build();
    assert!(matches!(read_docx(&bytes), Err(OfficeError::BadXml { .. })));
}

#[test]
fn dangling_references_still_save() {
    // a note reference without a note, a list without a definition and a
    // style based on itself
    let body = "<w:p><w:pPr><w:pStyle w:val=\"Loop\"/><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"9\"/></w:numPr></w:pPr>\
        <w:r><w:t>x</w:t></w:r><w:r><w:footnoteReference w:id=\"5\"/></w:r><w:r><w:commentReference w:id=\"3\"/></w:r></w:p>";
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"Loop\"><w:name w:val=\"Loop\"/><w:basedOn w:val=\"Loop\"/></w:style>";
    let document = read_docx(&Docx::new(body).styles(styles).build()).unwrap();
    assert!(letters_office::numbering::list_labels(&document)[0].is_none());
    let saved = write_docx(&document).unwrap();
    let again = read_docx(&saved).unwrap();
    assert_eq!(again.blocks[0].paragraph().unwrap().plain_text(), "x");
    let body = part_text(&saved, "word/document.xml");
    assert!(
        !body.contains("footnoteReference"),
        "a reference to a missing note must not be written"
    );
    let _ = write_odt(&document).unwrap();
}
