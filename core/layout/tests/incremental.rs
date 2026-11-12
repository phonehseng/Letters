//! incremental relayout gives exactly what a full layout gives, after any
//! sequence of edits (a property test over seeded random edits), and the
//! timings of both on a 50-page document.

#[path = "oracle/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use corpus::{prose, sentence, Seeded};
use letters_layout::{layout, LayoutEngine, LayoutOptions, LayoutResult};
use letters_office::model::*;

/// a long document: paragraphs, headings-like bold lines, a list and a
/// table, enough for about 50 pages.
fn long_document(rng: &mut Seeded) -> Document {
    let mut document = corpus::base_document("Times New Roman", 24, 0, LineSpacing::DOUBLE);
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("1", true));
    document.numbering.add_instance("1");
    for index in 0..170 {
        let words = 40 + rng.below(110) as usize;
        let mut paragraph = Paragraph::with_text(&prose(rng, words));
        paragraph.props.indent.first_line = Some(720);
        if index % 17 == 5 {
            paragraph.props.list = Some(ListRef {
                list_id: "1".into(),
                level: 0,
            });
        }
        document.blocks.push(Block::Paragraph(paragraph));
        if index % 40 == 20 {
            document.blocks.push(Block::Table(Table {
                column_widths: vec![3120, 3120, 3120],
                rows: (0..4)
                    .map(|_| TableRow {
                        cells: (0..3)
                            .map(|_| TableCell::with_text(&sentence(rng, 6)))
                            .collect(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }));
        }
    }
    document
}

/// one random edit of the kinds an editor makes.
fn edit(document: &mut Document, rng: &mut Seeded) {
    let count = document.blocks.len();
    let index = rng.below(count as u64) as usize;
    match rng.below(5) {
        0 => {
            // type a few words into a paragraph
            if let Some(paragraph) = document.blocks[index].paragraph_mut() {
                let words = 1 + rng.below(8) as usize;
                paragraph
                    .content
                    .push(Inline::text(&format!(" {}", sentence(rng, words))));
            }
        }
        1 => {
            // delete the end of a paragraph
            if let Some(paragraph) = document.blocks[index].paragraph_mut() {
                if let Some(Inline::Text { text, .. }) = paragraph.content.first_mut() {
                    let keep = text.chars().count() / 2;
                    *text = text.chars().take(keep).collect();
                }
            }
        }
        2 => {
            // a new paragraph
            let words = 5 + rng.below(60) as usize;
            document
                .blocks
                .insert(index, Block::text(&prose(rng, words)));
        }
        3 => {
            // remove a paragraph
            if count > 2 && document.blocks[index].paragraph().is_some() {
                document.blocks.remove(index);
            }
        }
        _ => {
            // bold a paragraph, or put it in the list
            if let Some(paragraph) = document.blocks[index].paragraph_mut() {
                if rng.below(2) == 0 {
                    for inline in &mut paragraph.content {
                        if let Some(marks) = inline.marks_mut() {
                            marks.push(Mark::Bold);
                        }
                    }
                } else {
                    paragraph.props.list = Some(ListRef {
                        list_id: "1".into(),
                        level: 0,
                    });
                }
            }
        }
    }
}

/// the result with font numbers replaced by family names, so two layouts
/// that loaded fonts in a different order compare equal.
fn normalised(result: &LayoutResult) -> String {
    let mut pages = result.pages.clone();
    for page in &mut pages {
        for line in &mut page.lines {
            for run in &mut line.runs {
                let family = &result.fonts[run.font as usize];
                run.text = format!(
                    "{}|{}|{}|{}",
                    family.family, family.bold, family.italic, run.text
                );
                run.font = 0;
            }
        }
    }
    format!("{pages:?}")
}

#[test]
fn incremental_layout_equals_full_layout_after_random_edits() {
    let mut rng = Seeded(2026);
    let mut document = long_document(&mut rng);
    // keep the test quick in debug builds: a shorter document
    document.blocks.truncate(60);
    let mut engine = LayoutEngine::new(LayoutOptions::default());
    let first = engine.layout(&document);
    assert_eq!(
        normalised(&first),
        normalised(&layout(&document, &LayoutOptions::default()))
    );
    for step in 0..12 {
        edit(&mut document, &mut rng);
        let incremental = engine.layout(&document);
        let full = layout(&document, &LayoutOptions::default());
        assert_eq!(
            normalised(&incremental),
            normalised(&full),
            "differs after edit {step}"
        );
        assert!(engine.stats.reused > 0, "nothing reused after edit {step}");
    }
}

/// full and incremental layout times on a 50-page document. run with
/// `cargo test -p letters-layout --release --test incremental -- --ignored --nocapture`
#[test]
#[ignore = "timing; run in a release build"]
fn timings_on_a_fifty_page_document() {
    let mut rng = Seeded(7);
    let mut document = long_document(&mut rng);
    let options = LayoutOptions::default();
    // the first layout also loads the fonts: time a second one as "full"
    let _ = layout(&document, &options);
    let started = std::time::Instant::now();
    let full = layout(&document, &options);
    let full_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut engine = LayoutEngine::new(options);
    let _ = engine.layout(&document);
    let mut worst: f64 = 0.0;
    let mut total = 0.0;
    for _ in 0..10 {
        let index = rng.below(document.blocks.len() as u64) as usize;
        if let Some(paragraph) = document.blocks[index].paragraph_mut() {
            paragraph.content.push(Inline::text(" typed"));
        }
        let started = std::time::Instant::now();
        let _ = engine.layout(&document);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        worst = worst.max(ms);
        total += ms;
    }
    println!(
        "{} pages: full layout {full_ms:.1} ms; incremental after one edit: mean {:.1} ms, worst {worst:.1} ms ({} of {} blocks prepared again)",
        full.pages.len(),
        total / 10.0,
        engine.stats.prepared,
        engine.stats.blocks
    );
}
