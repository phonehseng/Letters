//! incremental relayout gives exactly what a full layout gives, after any
//! sequence of edits (a property test over seeded random edits, on a
//! document with lists, tables that cross pages, floating pictures, text
//! boxes and footnotes), and the timings of both on a 300-page document.

#[path = "oracle/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use corpus::{prose, sentence, Seeded};
use letters_layout::{layout, LayoutEngine, LayoutOptions, LayoutResult};
use letters_office::model::*;

fn table(rng: &mut Seeded, rows: usize, header: bool) -> Block {
    let mut table = Table {
        column_widths: vec![3120, 3120, 3120],
        rows: (0..rows)
            .map(|_| TableRow {
                cells: (0..3)
                    .map(|_| {
                        let words = 2 + rng.below(18) as usize;
                        TableCell::with_text(&sentence(rng, words))
                    })
                    .collect(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    table.rows[0].header = header;
    Block::Table(table)
}

fn floating_picture(rng: &mut Seeded) -> Inline {
    Inline::Image(Image {
        media_id: "pic".into(),
        width: 1800,
        height: 1200,
        placement: Placement::Floating {
            horizontal: FloatPosition::new("margin", 720 * rng.below(5) as i32),
            vertical: FloatPosition::new("paragraph", 120),
            wrap: Wrap::Square,
        },
        ..Default::default()
    })
}

/// a long document: paragraphs, a list, tables (some long, with header
/// rows), floating pictures, text boxes and footnotes; `paragraphs` sets
/// the length (170 is about 50 pages).
fn long_document(rng: &mut Seeded, paragraphs: usize) -> Document {
    let mut document = corpus::base_document("Times New Roman", 24, 0, LineSpacing::DOUBLE);
    corpus::add_media(&mut document, "pic");
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("1", true));
    document.numbering.add_instance("1");
    let mut note = 0;
    for index in 0..paragraphs {
        let words = 40 + rng.below(110) as usize;
        let mut paragraph = Paragraph::with_text(&prose(rng, words));
        paragraph.props.indent.first_line = Some(720);
        if index % 17 == 5 {
            paragraph.props.list = Some(ListRef {
                list_id: "1".into(),
                level: 0,
            });
        }
        if index % 23 == 9 {
            paragraph.content.insert(0, floating_picture(rng));
        }
        if index % 11 == 3 {
            note += 1;
            let id = format!("n{note}");
            paragraph.content.push(Inline::NoteReference {
                note_id: id.clone(),
                marks: vec![Mark::Superscript],
            });
            let words = 8 + rng.below(40) as usize;
            document.notes.push(Note {
                id,
                kind: NoteKind::Footnote,
                special: None,
                blocks: vec![Block::text(&prose(rng, words))],
            });
        }
        document.blocks.push(Block::Paragraph(paragraph));
        if index % 40 == 20 {
            document.blocks.push(table(rng, 4, false));
        }
        if index % 60 == 35 {
            // a long table with a header row that crosses a page
            document.blocks.push(table(rng, 30, true));
        }
        if index % 50 == 44 {
            let mut text_box =
                TextBox::new(0, 0, 2880, 1080, vec![Block::text(&sentence(rng, 12))]);
            text_box.horizontal = FloatPosition::new("margin", 2880);
            text_box.vertical = FloatPosition::new("paragraph", 0);
            document.blocks.push(Block::TextBox(text_box));
        }
    }
    document
}

/// one random edit of the kinds an editor makes.
fn edit(document: &mut Document, rng: &mut Seeded) {
    let count = document.blocks.len();
    let index = rng.below(count as u64) as usize;
    match rng.below(7) {
        0 | 5 => {
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
                if let Some(Inline::Text { text, .. }) = paragraph.content.last_mut() {
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
        4 => {
            // a word typed into a table cell
            if let Block::Table(table) = &mut document.blocks[index] {
                let row = rng.below(table.rows.len() as u64) as usize;
                if let Some(paragraph) = table.rows[row].cells[0]
                    .blocks
                    .first_mut()
                    .and_then(Block::paragraph_mut)
                {
                    paragraph.content.push(Inline::text(" more words here"));
                }
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
/// that loaded fonts in a different order compare equal, and line ids
/// left out (a full layout numbers blocks, the engine keeps their ids).
fn normalised(result: &LayoutResult) -> String {
    let mut pages = result.pages.clone();
    for page in &mut pages {
        for line in &mut page.lines {
            line.id = 0;
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

/// where two layouts first differ, for a failure message a person can
/// act on: the page, and the first line on it that differs.
fn first_difference(incremental: &LayoutResult, full: &LayoutResult) -> String {
    if incremental.pages.len() != full.pages.len() {
        return format!(
            "{} pages against {}",
            incremental.pages.len(),
            full.pages.len()
        );
    }
    for (index, (ours, theirs)) in incremental.pages.iter().zip(&full.pages).enumerate() {
        let lines = ours.lines.iter().zip(&theirs.lines);
        for (a, b) in lines {
            let same = a.block == b.block && a.start == b.start && (a.top - b.top).abs() < 1e-9;
            if !same {
                return format!(
                    "page {index}: line {:?}@{} top {:.2} against {:?}@{} top {:.2}",
                    a.block, a.start, a.top, b.block, b.start, b.top
                );
            }
        }
        if ours.lines.len() != theirs.lines.len() || ours.number != theirs.number {
            return format!(
                "page {index}: {} lines (number {}) against {} (number {})",
                ours.lines.len(),
                ours.number,
                theirs.lines.len(),
                theirs.number
            );
        }
        if format!("{:?}{:?}", ours.rects, ours.images)
            != format!("{:?}{:?}", theirs.rects, theirs.images)
        {
            return format!("page {index}: rules or pictures differ");
        }
    }
    "the pages agree line by line; a run differs".into()
}

#[test]
fn incremental_layout_equals_full_layout_after_random_edits() {
    let mut rng = Seeded(2026);
    // a shorter document keeps the test quick in debug builds
    let mut document = long_document(&mut rng, 70);
    let mut engine = LayoutEngine::new(LayoutOptions::default());
    let first = engine.layout(&document);
    assert_eq!(
        normalised(&first),
        normalised(&layout(&document, &LayoutOptions::default()))
    );
    let mut stopped_early = 0;
    for step in 0..24 {
        edit(&mut document, &mut rng);
        let incremental = engine.layout(&document);
        let full = layout(&document, &LayoutOptions::default());
        assert!(
            normalised(&incremental) == normalised(&full),
            "differs after edit {step} ({:?}): {}",
            engine.stats,
            first_difference(&incremental, &full)
        );
        assert!(engine.stats.reused > 0, "nothing reused after edit {step}");
        if engine.stats.pages_reused > 0 {
            stopped_early += 1;
        }
    }
    assert!(
        stopped_early > 12,
        "pages were reused after only {stopped_early} of 24 edits"
    );
}

#[test]
fn line_ids_survive_an_edit_before_them() {
    let mut rng = Seeded(99);
    let mut document = long_document(&mut rng, 30);
    let mut engine = LayoutEngine::new(LayoutOptions::default());
    let before = engine.layout(&document);
    let last_block = document.blocks.len() as u32 - 1;
    let id_of = |result: &LayoutResult, block: u32| {
        result
            .pages
            .iter()
            .flat_map(|page| &page.lines)
            .find(|line| line.block.first() == Some(&block))
            .map(|line| line.id)
    };
    let old = id_of(&before, last_block).expect("a line of the last block");
    // a paragraph inserted at the start moves every block one on
    document
        .blocks
        .insert(0, Block::text("A new first paragraph."));
    let after = engine.layout(&document);
    assert_eq!(id_of(&after, last_block + 1), Some(old));
    assert!(!engine.changes.changed.is_empty());
}

#[test]
fn an_unchanged_document_changes_no_page() {
    let mut rng = Seeded(5);
    let document = long_document(&mut rng, 20);
    let mut engine = LayoutEngine::new(LayoutOptions::default());
    let _ = engine.layout(&document);
    let _ = engine.layout(&document);
    assert!(engine.changes.changed.is_empty());
    assert_eq!(engine.stats.prepared, 0);
}

/// full and incremental layout times on a 300-page document. run with
/// `cargo test -p letters-layout --release --test incremental -- --ignored --nocapture`
#[test]
#[ignore = "timing; run in a release build"]
fn timings_on_a_three_hundred_page_document() {
    let mut rng = Seeded(7);
    let mut document = long_document(&mut rng, 1000);
    let options = LayoutOptions::default();
    // the first layout also loads the fonts: time a second one as "full"
    let _ = layout(&document, &options);
    let started = std::time::Instant::now();
    let full = layout(&document, &options);
    let full_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut engine = LayoutEngine::new(options);
    let started = std::time::Instant::now();
    let _ = engine.layout(&document);
    let first_engine_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut times = Vec::new();
    let mut reused = 0;
    for _ in 0..20 {
        let index = rng.below(document.blocks.len() as u64) as usize;
        if let Some(paragraph) = document.blocks[index].paragraph_mut() {
            // one character typed
            paragraph.content.push(Inline::text("x"));
        }
        let started = std::time::Instant::now();
        let _ = engine.layout(&document);
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        reused += engine.stats.pages_reused;
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{} pages: full layout {full_ms:.1} ms (engine's first {first_engine_ms:.1} ms); one character typed: median {:.1} ms, worst {:.1} ms; on average {} of {} pages reused",
        full.pages.len(),
        times[times.len() / 2],
        times[times.len() - 1],
        reused / times.len(),
        full.pages.len()
    );
}
