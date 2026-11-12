//! a long document reads and writes quickly.

mod support;

use std::time::{Duration, Instant};

use letters_office::model::*;
use letters_office::{read_docx, read_odt, write_docx, write_odt};

/// 2,000 paragraphs of essay-like text with a heading every 50, list
/// items, bold words and a footnote now and then.
fn long_document() -> Document {
    let mut document = Document::new();
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("0", true));
    let list = document.numbering.add_instance("0");
    document.blocks.clear();
    for index in 0..2000 {
        if index % 50 == 0 {
            document
                .blocks
                .push(Block::text(&format!("Section {}", index / 50 + 1)));
            let last = document.blocks.len() - 1;
            letters_office::outline::apply_heading(&mut document, last, 1);
            continue;
        }
        let mut paragraph = Paragraph {
            content: vec![
                Inline::text("This is sentence one of paragraph "),
                Inline::styled(&index.to_string(), vec![Mark::Bold]),
                Inline::text(". It goes on for a while, the way paragraphs in a student essay do, with a clause or two."),
            ],
            ..Default::default()
        };
        if index % 10 == 3 {
            paragraph.props.list = Some(ListRef {
                list_id: list.clone(),
                level: 0,
            });
        }
        if index % 100 == 7 {
            let id = format!("n{index}");
            paragraph.content.push(Inline::NoteReference {
                note_id: id.clone(),
                marks: vec![],
            });
            document.notes.push(Note {
                id,
                kind: NoteKind::Footnote,
                special: None,
                blocks: vec![Block::text("A note.")],
            });
        }
        document.blocks.push(Block::Paragraph(paragraph));
    }
    document
}

fn timed<T>(label: &str, work: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let result = work();
    let elapsed = started.elapsed();
    eprintln!("{label}: {elapsed:?}");
    (result, elapsed)
}

#[test]
fn two_thousand_paragraphs_read_and_write_fast() {
    let document = long_document();
    assert_eq!(document.blocks.len(), 2000);

    let (docx, write_docx_time) = timed("write docx", || write_docx(&document).unwrap());
    let (from_docx, read_docx_time) = timed("read docx", || read_docx(&docx).unwrap());
    assert_eq!(from_docx.blocks.len(), 2000);
    assert_eq!(letters_office::outline::outline(&from_docx).len(), 40);

    let (odt, write_odt_time) = timed("write odt", || write_odt(&document).unwrap());
    let (from_odt, read_odt_time) = timed("read odt", || read_odt(&odt).unwrap());
    assert_eq!(from_odt.blocks.len(), 2000);

    let (counts, count_time) = timed("count", || letters_office::counts::count(&from_docx));
    assert!(counts.words > 50_000);
    eprintln!(
        "{} words, about {} pages, {} kB as docx, {} kB as odt",
        counts.words,
        counts.pages_estimate,
        docx.len() / 1024,
        odt.len() / 1024
    );

    // generous limits for unoptimised test builds on a slow machine; the
    // printed times are the real measure
    let limit = Duration::from_secs(10);
    for time in [
        write_docx_time,
        read_docx_time,
        write_odt_time,
        read_odt_time,
        count_time,
    ] {
        assert!(time < limit, "{time:?}");
    }
}
