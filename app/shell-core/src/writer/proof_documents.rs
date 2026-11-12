//! the documents the writer's end-to-end checks open: one paragraph to
//! split into text blocks, a picture in the line, a 16-page essay for
//! word's line breaks and a 30-page document with 20 pictures for frame
//! times. run with LETTERS_PROOF_DIR set:
//! `cargo test -p letters-shell-core write_proof_documents -- --ignored`

use std::path::Path;

use letters_office::model::{
    Block, Document, Heading, Image, Inline, MediaItem, Paragraph, Placement,
};

/// the words the made-up sentences are built from: plain, wrap-friendly english
const WORDS: [&str; 25] = [
    "students", "write", "essays", "about", "history", "science", "and", "art", "while",
    "teachers", "read", "every", "page", "with", "care", "because", "good", "writing", "takes",
    "time", "to", "grow", "into", "clear", "ideas",
];

/// one made-up sentence; the same seed always gives the same sentence, so
/// the documents (and word's layout of them) never change between runs
fn sentence(seed: usize, length: usize) -> String {
    let words: Vec<&str> = (0..length)
        .map(|i| WORDS[(seed * 7 + i * 3 + i * i) % WORDS.len()])
        .collect();
    let text = words.join(" ");
    let mut characters = text.chars();
    let first = characters
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    format!("{first}{}.", characters.as_str())
}

/// a paragraph of `sentences` sentences of nine to fifteen words
fn paragraph(seed: usize, sentences: usize) -> Block {
    let text: Vec<String> = (0..sentences)
        .map(|i| sentence(seed + i, 9 + (seed + i) % 7))
        .collect();
    Block::text(&text.join(" "))
}

fn heading(text: &str) -> Block {
    Block::Heading(Heading {
        level: 1,
        paragraph: Paragraph {
            style_id: Some("Heading1".into()),
            content: vec![Inline::text(text)],
            ..Default::default()
        },
    })
}

fn picture(width: i32, height: i32) -> Inline {
    Inline::Image(Image {
        media_id: "card.png".into(),
        width,
        height,
        description: Some("A coloured test card".into()),
        placement: Placement::Inline,
        ..Default::default()
    })
}

fn with_card(mut document: Document) -> Document {
    let bytes = picture_png(300, 180);
    document.media.push(MediaItem {
        id: "card.png".into(),
        content_type: "image/png".into(),
        source_path: None,
        bytes,
    });
    document
}

/// word's own defaults since 2016 (Calibri 11, 8 points after, lines at
/// 1.08), so the documents look like ones people make in word
fn with_word_defaults(document: &Document) -> Document {
    let mut value = serde_json::to_value(document).unwrap();
    value["styles"]["defaults"] = serde_json::json!({
        "run": { "font": { "name": "Calibri" }, "size": 22 },
        "paragraph": { "spacing": { "after": 160, "line": { "rule": "auto", "value": 259 } } }
    });
    serde_json::from_value(value).unwrap()
}

fn save(folder: &Path, name: &str, blocks: Vec<Block>, document: Document) {
    let mut document = document;
    document.blocks = blocks;
    let bytes = letters_office::write_docx(&with_word_defaults(&document)).unwrap();
    std::fs::write(folder.join(name), bytes).unwrap();
}

fn text_blocks() -> Vec<Block> {
    vec![
        heading("Text blocks"),
        Block::text("Split me here into two blocks please."),
        paragraph(1, 4),
        paragraph(2, 5),
        paragraph(3, 4),
    ]
}

fn pictures() -> Vec<Block> {
    let line = Paragraph {
        content: vec![
            Inline::text("The picture "),
            picture(2700, 1620),
            Inline::text(" sits in this line."),
        ],
        ..Default::default()
    };
    let mut blocks = vec![heading("Pictures"), paragraph(4, 3), Block::Paragraph(line)];
    blocks.extend((5..12).map(|seed| paragraph(seed, 5)));
    blocks
}

/// 150 blocks with word's defaults come to 16 pages in word
fn essay() -> Vec<Block> {
    (0..150)
        .map(|index| {
            if index % 12 == 0 {
                heading(&format!("Part {}", index / 12 + 1))
            } else {
                paragraph(index, 4 + index % 3)
            }
        })
        .collect()
}

/// 320 blocks come to 30 pages; a picture every 16th block, 20 in all
fn heavy() -> Vec<Block> {
    (0..320)
        .map(|index| {
            if index % 16 == 8 && index / 16 < 20 {
                Block::Paragraph(Paragraph {
                    content: vec![picture(2400, 1440)],
                    ..Default::default()
                })
            } else {
                paragraph(index, 4)
            }
        })
        .collect()
}

/// a small colourful png made here, so the checks need no picture file
fn picture_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgb);
    let mut writer = encoder.write_header().unwrap();
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|i| {
            let (x, y) = (i % width, i / width);
            let checker = if (x / 30 + y / 30) % 2 == 0 { 200 } else { 60 };
            [(x * 255 / width) as u8, (y * 255 / height) as u8, checker]
        })
        .collect();
    writer.write_image_data(&pixels).unwrap();
    writer.finish().unwrap();
    bytes
}

#[test]
#[ignore = "writes proof documents into LETTERS_PROOF_DIR"]
fn write_proof_documents() {
    let folder = std::path::PathBuf::from(std::env::var("LETTERS_PROOF_DIR").unwrap());
    std::fs::create_dir_all(&folder).unwrap();
    save(&folder, "textblocks.docx", text_blocks(), Document::new());
    save(
        &folder,
        "pictures.docx",
        pictures(),
        with_card(Document::new()),
    );
    save(&folder, "essay16.docx", essay(), Document::new());
    save(&folder, "heavy30.docx", heavy(), with_card(Document::new()));
}

#[test]
fn sentences_are_the_same_every_time() {
    assert_eq!(sentence(3, 4), sentence(3, 4));
    assert!(sentence(0, 9).ends_with('.'));
    assert!(sentence(0, 9).chars().next().unwrap().is_uppercase());
}
