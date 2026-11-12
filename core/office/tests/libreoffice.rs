//! checks against libreoffice. ignored by default because they need it
//! installed (or unpacked; see `bridge::libreoffice_path`); run with
//! `cargo test -p letters-office --test libreoffice -- --ignored`.
//! libreoffice runs headless with a private profile and is waited for.

mod support;

use std::time::Instant;

use letters_office::bridge::libreoffice_convert;
use letters_office::model::*;
use letters_office::{read_docx, read_odt, write_docx, write_odt};
use support::fixtures::{rich_docx, text_box_document};
use support::png;

fn timed<T>(label: &str, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = work();
    eprintln!("{label}: {:?}", started.elapsed());
    result
}

/// pages in a pdf, by its page objects.
fn pdf_pages(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.matches("/Type/Page").count() + text.matches("/Type /Page").count()
        - text.matches("/Type/Pages").count()
        - text.matches("/Type /Pages").count()
}

fn boxes(document: &Document) -> Vec<TextBox> {
    document
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::TextBox(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn floating_picture_document() -> Document {
    let mut document = Document::new();
    document.media.push(MediaItem {
        id: "dot.png".into(),
        content_type: "image/png".into(),
        source_path: None,
        bytes: png(),
    });
    document.blocks = vec![Block::Paragraph(Paragraph {
        content: vec![
            Inline::text("A floating picture "),
            Inline::Image(Image {
                media_id: "dot.png".into(),
                width: 1440,
                height: 720,
                placement: Placement::Floating {
                    horizontal: FloatPosition::new("page", 2880),
                    vertical: FloatPosition::new("page", 4320),
                    wrap: Wrap::Square,
                },
                ..Default::default()
            }),
            Inline::text(" and an inline one "),
            Inline::Image(Image {
                media_id: "dot.png".into(),
                width: 720,
                height: 720,
                ..Default::default()
            }),
        ],
        ..Default::default()
    })];
    document
}

#[test]
#[ignore = "needs LibreOffice"]
fn libreoffice_reads_our_odt() {
    let from_word = read_docx(&rich_docx()).unwrap();
    let odt = write_odt(&from_word).unwrap();
    let pdf = timed("odt to pdf", || {
        libreoffice_convert(&odt, "odt", "pdf", "pdf").unwrap()
    });
    assert!(pdf.starts_with(b"%PDF"));
    assert_eq!(pdf_pages(&pdf), 2, "two sections, two pages");
    let text = String::from_utf8(timed("odt to text", || {
        libreoffice_convert(&odt, "odt", "txt:Text (encoded):UTF8", "txt").unwrap()
    }))
    .unwrap();
    eprintln!("libreoffice text of our odt:\n{text}");
    // libreoffice prints tracked deletions too ("The oldnew word")
    for expected in [
        "Introduction",
        "Glowing words",
        "first",
        "nested",
        "new word",
        "wide",
        "tall",
        "Last words",
    ] {
        assert!(text.contains(expected), "{expected} missing");
    }
}

#[test]
#[ignore = "needs LibreOffice"]
fn we_read_an_odt_libreoffice_wrote() {
    let docx = write_docx(&text_box_document()).unwrap();
    let odt = timed("docx to odt", || {
        libreoffice_convert(&docx, "docx", "odt", "odt").unwrap()
    });
    let document = read_odt(&odt).unwrap();
    let found = boxes(&document);
    eprintln!("text boxes libreoffice wrote: {found:#?}");
    assert_eq!(found.len(), 2);
    assert_eq!(
        found[0].blocks[0].paragraph().unwrap().plain_text(),
        "Boxed text"
    );
    assert!(
        (found[0].horizontal.offset - 2880).abs() <= 2,
        "{}",
        found[0].horizontal.offset
    );
    assert!(
        (found[0].vertical.offset - 2160).abs() <= 2,
        "{}",
        found[0].vertical.offset
    );
    let texts: Vec<String> = document
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .map(|p| p.plain_text())
        .collect();
    assert!(
        texts.contains(&"Before the boxes.".to_string())
            && texts.contains(&"After the boxes.".to_string())
    );
}

#[test]
#[ignore = "needs LibreOffice"]
fn text_boxes_and_pictures_survive_a_libreoffice_round_trip() {
    let original = text_box_document();
    let resaved = timed("odt to odt", || {
        libreoffice_convert(&write_odt(&original).unwrap(), "odt", "odt", "odt").unwrap()
    });
    let back = read_odt(&resaved).unwrap();
    let (before, after) = (boxes(&original), boxes(&back));
    eprintln!("after libreoffice: {after:#?}");
    assert_eq!(after.len(), 2);
    for (a, b) in before.iter().zip(&after) {
        assert!((a.horizontal.offset - b.horizontal.offset).abs() <= 2);
        assert!((a.vertical.offset - b.vertical.offset).abs() <= 2);
        assert!((a.width - b.width).abs() <= 2);
        assert_eq!(a.blocks.len(), b.blocks.len());
    }
    let pictures = floating_picture_document();
    let resaved = libreoffice_convert(&write_odt(&pictures).unwrap(), "odt", "odt", "odt").unwrap();
    let back = read_odt(&resaved).unwrap();
    let images: Vec<Image> = back
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .flat_map(|p| &p.content)
        .filter_map(|i| match i {
            Inline::Image(image) => Some(image.clone()),
            _ => None,
        })
        .collect();
    eprintln!("pictures after libreoffice: {images:#?}");
    assert_eq!(images.len(), 2);
    let Placement::Floating {
        horizontal,
        vertical,
        ..
    } = &images[0].placement
    else {
        panic!("not floating")
    };
    assert!((horizontal.offset - 2880).abs() <= 2 && (vertical.offset - 4320).abs() <= 2);
    assert_eq!(images[1].placement, Placement::Inline);
    // libreoffice stores pictures again in its own encoding; the picture stays
    assert!(!back.media.is_empty());
}

#[test]
#[ignore = "needs LibreOffice"]
fn libreoffice_prints_and_converts_old_files_when_word_is_missing() {
    let docx = write_docx(&text_box_document()).unwrap();
    let pdf = timed("docx to pdf", || {
        libreoffice_convert(&docx, "docx", "pdf", "pdf").unwrap()
    });
    assert!(pdf.starts_with(b"%PDF"));
    let doc = timed("docx to doc", || {
        libreoffice_convert(&docx, "docx", "doc:MS Word 97", "doc").unwrap()
    });
    assert!(doc.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]));
    let back = timed("doc to docx", || {
        libreoffice_convert(&doc, "doc", "docx:MS Word 2007 XML", "docx").unwrap()
    });
    let document = read_docx(&back).unwrap();
    let texts: Vec<String> = document
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .map(|p| p.plain_text())
        .collect();
    assert!(texts.iter().any(|t| t == "Before the boxes."), "{texts:?}");
}
