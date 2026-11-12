//! checks against microsoft word itself. ignored by default because they
//! need word installed (windows only); run them with
//! `cargo test -p letters-office -- --ignored`.
//!
//! the powershell helper in tests/tools starts a private hidden word,
//! opens files read-only and quits only that instance.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use letters_office::model::*;
use letters_office::{read_docx, write_docx};
use serde_json::Value;
use support::png;

fn work_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("letters-office-word-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tools/word_check.ps1")
}

fn run_word(args: &[&str]) {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(script())
        .args(args)
        .output()
        .expect("powershell runs");
    assert!(
        output.status.success(),
        "word helper failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// what word sees in a file.
fn word_report(file: &Path) -> Value {
    let out = file.with_extension("json");
    run_word(&[
        "-Mode",
        "verify",
        "-Path",
        file.to_str().unwrap(),
        "-Out",
        out.to_str().unwrap(),
    ]);
    let text = std::fs::read_to_string(&out).unwrap();
    serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap()
}

fn paragraphs(report: &Value) -> Vec<(String, String, i64)> {
    report["paragraphs"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|p| {
                    (
                        p["text"].as_str().unwrap_or("").to_string(),
                        p["style"].as_str().unwrap_or("").to_string(),
                        p["outline"].as_i64().unwrap_or(0),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn heading_style(id: &str, name: &str, level: u8) -> Style {
    let mut style = Style::paragraph(id, name);
    style.based_on = Some("Normal".into());
    style.paragraph.outline_level = Some(level - 1);
    style.paragraph.keep_with_next = Some(true);
    style.run.bold = Some(true);
    style.run.size = Some(if level == 1 { 32 } else { 26 });
    style
}

/// a document made from nothing with most features in it.
fn built_document() -> Document {
    let mut document = Document::new();
    document.metadata.title = Some("Written by Letters".into());
    document.metadata.author = Some("Ada".into());
    document
        .styles
        .styles
        .push(heading_style("Heading1", "heading 1", 1));
    document
        .styles
        .styles
        .push(heading_style("Heading2", "heading 2", 2));
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("0", true));
    let list = document.numbering.add_instance("0");
    document.media.push(MediaItem {
        id: "dot.png".into(),
        content_type: "image/png".into(),
        source_path: None,
        bytes: png(),
    });
    let heading = |text: &str, id: &str, level: u8| {
        let mut paragraph = Paragraph::with_text(text);
        paragraph.style_id = Some(id.into());
        Block::Heading(Heading { level, paragraph })
    };
    let item = |text: &str, level: u8| {
        let mut paragraph = Paragraph::with_text(text);
        paragraph.props.list = Some(ListRef {
            list_id: list.clone(),
            level,
        });
        Block::Paragraph(paragraph)
    };
    let change = |author: &str| Change {
        id: String::new(),
        author: author.into(),
        date: Some("2026-11-11T10:00:00Z".into()),
    };
    document.blocks = vec![
        heading("Essay Title", "Heading1", 1),
        Block::Paragraph(Paragraph {
            content: vec![
                Inline::text("Body text with "),
                Inline::styled("bold", vec![Mark::Bold]),
                Inline::text(" and a note"),
                Inline::NoteReference {
                    note_id: "n1".into(),
                    marks: vec![],
                },
                Inline::text(". "),
                Inline::styled("Old", vec![Mark::Deletion(change("Ben"))]),
                Inline::styled("New", vec![Mark::Insertion(change("Ben"))]),
                Inline::text(" words, "),
                Inline::styled("commented", vec![Mark::Comment { id: "c1".into() }]),
                Inline::text(" and "),
                Inline::styled(
                    "a link",
                    vec![Mark::Link(LinkTarget {
                        href: Some("https://example.org/".into()),
                        ..Default::default()
                    })],
                ),
                Inline::text("."),
            ],
            ..Default::default()
        }),
        heading("Background", "Heading2", 2),
        item("First item", 0),
        item("Nested item", 1),
        item("Second item", 0),
        Block::Paragraph(Paragraph {
            content: vec![
                Inline::text("A picture: "),
                Inline::Image(Image {
                    media_id: "dot.png".into(),
                    width: 720,
                    height: 720,
                    description: Some("a dot".into()),
                    name: None,
                    placement: Placement::Inline,
                    marks: vec![],
                    original: None,
                }),
            ],
            ..Default::default()
        }),
        Block::text("The end."),
    ];
    document.notes.push(Note {
        id: "n1".into(),
        kind: NoteKind::Footnote,
        special: None,
        blocks: vec![Block::text("Footnote text.")],
    });
    document.comments.push(Comment {
        id: "c1".into(),
        author: "Prof".into(),
        initials: Some("P".into()),
        date: Some("2026-11-11T09:00:00Z".into()),
        blocks: vec![Block::text("Explain this.")],
    });
    document.headers_footers.push(HeaderFooter {
        id: "h".into(),
        kind: HeaderFooterKind::Header,
        blocks: vec![Block::text("Running head")],
    });
    document.headers_footers.push(HeaderFooter {
        id: "f".into(),
        kind: HeaderFooterKind::Footer,
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![Inline::text("Page "), Inline::Field(Field::page_number())],
            ..Default::default()
        })],
    });
    document.sections[0].headers.default = Some("h".into());
    document.sections[0].footers.default = Some("f".into());
    document
}

#[test]
#[ignore = "needs Microsoft Word"]
fn word_opens_a_document_we_built_without_repairing_it() {
    let dir = work_dir("built");
    let file = dir.join("built.docx");
    let started = Instant::now();
    std::fs::write(&file, write_docx(&built_document()).unwrap()).unwrap();
    let report = word_report(&file);
    eprintln!("word report ({:?}): {report:#}", started.elapsed());
    assert_eq!(
        report["repaired"],
        Value::Bool(false),
        "word repaired the file"
    );
    let found = paragraphs(&report);
    let texts: Vec<&str> = found.iter().map(|(text, _, _)| text.as_str()).collect();
    assert_eq!(found.len(), 8, "{texts:?}");
    assert_eq!(found[0].1, "Heading 1");
    assert_eq!(found[0].2, 1);
    assert_eq!(found[2].1, "Heading 2");
    assert_eq!(texts[0], "Essay Title");
    assert!(texts[1].starts_with("Body text with bold and a note"));
    assert_eq!(texts[3], "First item");
    assert_eq!(report["footnotes"], 1);
    assert_eq!(report["comments"], 1);
    assert_eq!(report["revisions"], 2);
    assert_eq!(report["pictures"], 1);
    assert_eq!(report["headerText"], "Running head");
    assert!(report["footerText"].as_str().unwrap().starts_with("Page "));
}

/// the mla preset, and the same document after a trip through odt,
/// both open cleanly in word.
#[test]
#[ignore = "needs Microsoft Word"]
fn word_opens_the_mla_preset_and_an_odt_conversion() {
    let mut document = built_document();
    let details = letters_office::presets::MlaDetails {
        student_name: "Ada Lovelace".into(),
        instructor: "Prof. Babbage".into(),
        course: "ENG 101".into(),
        date: "11 November 2026".into(),
        title: "On Engines".into(),
        insert_heading: true,
    };
    letters_office::presets::apply_mla(&mut document, &details);
    let dir = work_dir("mla");
    let file = dir.join("mla.docx");
    std::fs::write(&file, write_docx(&document).unwrap()).unwrap();
    let report = word_report(&file);
    eprintln!("word report of the mla preset: {report:#}");
    assert_eq!(report["repaired"], Value::Bool(false));
    assert_eq!(report["headerText"], "Lovelace 1");
    assert_eq!(paragraphs(&report)[0].0, "Ada Lovelace");

    let through_odt =
        letters_office::read_odt(&letters_office::write_odt(&built_document()).unwrap()).unwrap();
    let file = dir.join("from-odt.docx");
    std::fs::write(&file, write_docx(&through_odt).unwrap()).unwrap();
    let report = word_report(&file);
    eprintln!("word report of the odt conversion: {report:#}");
    assert_eq!(report["repaired"], Value::Bool(false));
    let found = paragraphs(&report);
    assert_eq!(found.len(), 8);
    assert_eq!(found[0].1, "Heading 1");
    assert_eq!(report["footnotes"], 1);
    assert_eq!(report["comments"], 1);
    assert_eq!(report["revisions"], 2);
    assert_eq!(report["pictures"], 1);
}

/// the control: a file with broken structure must be reported, or a
/// "not repaired" answer above would mean nothing.
#[test]
#[ignore = "needs Microsoft Word"]
fn the_helper_notices_a_file_word_cannot_open_cleanly() {
    let good = write_docx(&built_document()).unwrap();
    let package = letters_office::package::Package::read(&good).unwrap();
    let parts: Vec<(&str, Vec<u8>)> = package
        .parts
        .iter()
        .map(|part| {
            let bytes = if part.name == "word/document.xml" {
                String::from_utf8(part.bytes.clone())
                    .unwrap()
                    .replacen("<w:body>", "<w:body><w:tbl><w:tr><w:p/></w:tr></w:tbl>", 1)
                    .into_bytes()
            } else {
                part.bytes.clone()
            };
            (part.name.as_str(), bytes)
        })
        .collect();
    let dir = work_dir("control");
    let file = dir.join("broken.docx");
    std::fs::write(&file, support::zip(&parts)).unwrap();
    let report = word_report(&file);
    eprintln!("word report of a broken file: {report:#}");
    assert_eq!(report["repaired"], Value::Bool(true));
}

#[test]
#[ignore = "needs Microsoft Word"]
fn word_opens_tables_with_merged_cells() {
    let mut document = Document::new();
    let wide = TableCell {
        col_span: 2,
        ..TableCell::with_text("wide")
    };
    let tall = TableCell {
        row_span: 2,
        ..TableCell::with_text("tall")
    };
    let border = Border {
        style: "single".into(),
        size: 4,
        color: None,
    };
    let borders = CellBorders {
        top: Some(border.clone()),
        left: Some(border.clone()),
        bottom: Some(border.clone()),
        right: Some(border.clone()),
        inside_horizontal: Some(border.clone()),
        inside_vertical: Some(border),
    };
    document.blocks = vec![
        Block::text("Before the table."),
        Block::Table(Table {
            borders: Some(borders),
            rows: vec![
                TableRow {
                    header: true,
                    cells: vec![wide, tall],
                    ..Default::default()
                },
                TableRow {
                    cells: vec![TableCell::with_text("c"), TableCell::with_text("d")],
                    ..Default::default()
                },
                TableRow {
                    cells: vec![
                        TableCell::with_text("e"),
                        TableCell::with_text("f"),
                        TableCell::with_text("g"),
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        Block::text("After the table."),
    ];
    let dir = work_dir("table");
    let file = dir.join("table.docx");
    std::fs::write(&file, write_docx(&document).unwrap()).unwrap();
    let report = word_report(&file);
    eprintln!("word report: {report:#}");
    assert_eq!(report["repaired"], Value::Bool(false));
    assert_eq!(report["tables"][0]["rows"], 3);
    assert_eq!(report["tables"][0]["columns"], 3);
    let texts: Vec<String> = paragraphs(&report)
        .into_iter()
        .map(|(text, _, _)| text)
        .collect();
    for expected in [
        "wide",
        "tall",
        "c",
        "d",
        "e",
        "f",
        "g",
        "Before the table.",
        "After the table.",
    ] {
        assert!(
            texts.iter().any(|t| t == expected),
            "{expected} missing from {texts:?}"
        );
    }
}

#[test]
#[ignore = "needs Microsoft Word"]
fn a_document_word_saved_reads_and_survives_our_save() {
    let dir = work_dir("from-word");
    let picture = dir.join("dot.png");
    std::fs::write(&picture, png()).unwrap();
    let file = dir.join("from-word.docx");
    run_word(&[
        "-Mode",
        "create",
        "-Path",
        file.to_str().unwrap(),
        "-Image",
        picture.to_str().unwrap(),
    ]);

    let bytes = std::fs::read(&file).unwrap();
    let started = Instant::now();
    let document = read_docx(&bytes).unwrap();
    eprintln!("read word's file in {:?}", started.elapsed());
    let headings: Vec<(u8, String)> = document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Heading(heading) => Some((heading.level, heading.paragraph.plain_text())),
            _ => None,
        })
        .collect();
    assert_eq!(
        headings,
        vec![
            (1, "Essay Title".to_string()),
            (2, "Background".to_string())
        ]
    );
    let lists: Vec<String> = document
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .filter(|p| p.props.list.is_some())
        .map(|p| p.plain_text())
        .collect();
    assert_eq!(lists, ["First item", "Second item"]);
    let labels: Vec<String> = letters_office::numbering::list_labels(&document)
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(labels.len(), 2, "{labels:?}");
    assert!(document
        .blocks
        .iter()
        .any(|b| matches!(b, Block::Table(t) if t.rows.len() == 2)));
    let user_notes: Vec<&Note> = document
        .notes
        .iter()
        .filter(|n| n.special.is_none())
        .collect();
    assert_eq!(user_notes.len(), 1);
    assert_eq!(
        user_notes[0].blocks[0]
            .paragraph()
            .unwrap()
            .plain_text()
            .trim(),
        "The footnote text."
    );
    assert_eq!(document.media.len(), 1);
    assert_eq!(document.media[0].bytes, png());
    assert_eq!(document.comments.len(), 1);
    let json = serde_json::to_string(&document.blocks).unwrap();
    assert!(json.contains("\"insertion\""), "tracked insertion missing");
    assert_eq!(document.metadata.title.as_deref(), Some("Made in Word"));

    // our save of word's file opens in word, unrepaired, with the same content
    let original_report = word_report(&file);
    let resaved = dir.join("resaved.docx");
    std::fs::write(&resaved, write_docx(&document).unwrap()).unwrap();
    let report = word_report(&resaved);
    eprintln!("word report of our save: {report:#}");
    assert_eq!(report["repaired"], Value::Bool(false));
    assert_eq!(paragraphs(&report), paragraphs(&original_report));
    for key in [
        "footnotes",
        "comments",
        "revisions",
        "pictures",
        "tables",
        "sections",
    ] {
        assert_eq!(report[key], original_report[key], "{key} differs");
    }
}
