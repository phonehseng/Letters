//! small text pdfs for the export and ai tests, written with lopdf in the
//! standard fonts, so the convert engine's analysis has real text to read.

use lopdf::{dictionary, Document, Object, Stream};

/// one line of text: font ("F1" regular, "F2" bold), size, x, y from the
/// bottom of the page, and the words.
pub struct Line<'a> {
    pub font: &'a str,
    pub size: f32,
    pub x: f32,
    pub y: f32,
    pub text: &'a str,
}

/// a us letter pdf, one page per entry of `pages`, each with its lines
/// and any extra drawing (`re S` boxes for tables).
pub fn text_pdf(pages: &[(Vec<Line>, &str)]) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let regular = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });
    let bold = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding",
    });
    let resources = document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => regular, "F2" => bold },
    });
    let mut kids = Vec::new();
    for (lines, drawing) in pages {
        let mut content = drawing.as_bytes().to_vec();
        for line in lines {
            content.extend(
                format!(
                    "\nBT /{} {} Tf {} {} Td (",
                    line.font, line.size, line.x, line.y
                )
                .bytes(),
            );
            content.extend(win_ansi(line.text));
            content.extend(b") Tj ET");
        }
        let stream = document.add_object(Stream::new(dictionary! {}, content));
        let page = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => stream,
            "Resources" => resources,
        });
        kids.push(Object::Reference(page));
    }
    let count = kids.len() as i64;
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

/// the text as the standard fonts' windows encoding, escaped for a pdf
/// string. only ascii and the bullet are needed here.
fn win_ansi(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for ch in text.chars() {
        match ch {
            '\u{2022}' => bytes.push(0x95),
            '(' | ')' | '\\' => bytes.extend([b'\\', ch as u8]),
            ch if ch.is_ascii() => bytes.push(ch as u8),
            _ => bytes.push(b'?'),
        }
    }
    bytes
}

/// a one page pdf with a single line of 12 point text.
pub fn one_line_pdf(text: &str) -> Vec<u8> {
    text_pdf(&[(
        vec![Line {
            font: "F1",
            size: 12.0,
            x: 72.0,
            y: 700.0,
            text,
        }],
        "",
    )])
}

/// one line of text, as `Line` with its fields in order.
fn line(font: &'static str, size: f32, x: f32, y: f32, text: &'static str) -> Line<'static> {
    Line {
        font,
        size,
        x,
        y,
        text,
    }
}

/// a two page study guide: a title, paragraphs, a bulleted list and a
/// ruled table, the shapes "easy to edit" has to rebuild.
pub fn study_guide_pdf() -> Vec<u8> {
    text_pdf(&[
        (study_guide_first_page(), ""),
        (study_guide_second_page(), ENERGY_TABLE_RULES),
    ])
}

/// a title, two lines of body text, a heading, three bullets and a last
/// paragraph. "F2" is the bold font.
fn study_guide_first_page() -> Vec<Line<'static>> {
    vec![
        line("F2", 22.0, 72.0, 700.0, "Study Guide"),
        line(
            "F1",
            11.0,
            72.0,
            660.0,
            "Photosynthesis turns light into chemical energy in plants.",
        ),
        line(
            "F1",
            11.0,
            72.0,
            646.0,
            "It happens in the chloroplasts of leaf cells every day.",
        ),
        line("F2", 15.0, 72.0, 610.0, "What You Need"),
        line("F1", 11.0, 72.0, 585.0, "\u{2022}  Light from the sun"),
        line("F1", 11.0, 72.0, 569.0, "\u{2022}  Water from the roots"),
        line(
            "F1",
            11.0,
            72.0,
            553.0,
            "\u{2022}  Carbon dioxide from the air",
        ),
        line(
            "F1",
            11.0,
            72.0,
            520.0,
            "Respiration is the reverse process that releases the energy again.",
        ),
    ]
}

/// the lines of a 3 by 2 ruled table, rows 20 points tall from y 400 up to 460.
const ENERGY_TABLE_RULES: &str =
    "0 0 0 RG 1 w 72 400 300 60 re S 72 420 m 372 420 l S 72 440 m 372 440 l S 222 400 m 222 460 l S";

/// a heading and the text inside the ruled table's cells.
fn study_guide_second_page() -> Vec<Line<'static>> {
    vec![
        line("F2", 15.0, 72.0, 700.0, "Energy Table"),
        line("F2", 11.0, 80.0, 446.0, "Stage"),
        line("F2", 11.0, 230.0, 446.0, "Energy"),
        line("F1", 11.0, 80.0, 426.0, "Light reactions"),
        line("F1", 11.0, 230.0, 426.0, "Captured"),
        line("F1", 11.0, 80.0, 406.0, "Calvin cycle"),
        line("F1", 11.0, 230.0, 406.0, "Stored"),
    ]
}
