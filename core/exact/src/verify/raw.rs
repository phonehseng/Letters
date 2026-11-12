//! the raw text operators word writes into its pdfs, kept as printed.
//!
//! the writer model is fitted on the exact digits word prints, so this
//! reader keeps each text object's `Tm` as written (read as f64 from its
//! digits) and the codes it shows. only what word writes is handled: one
//! `Tm` per text object, `TJ` arrays, one-byte and identity-h fonts.

use std::collections::HashMap;

use lopdf::Document;

use crate::pdf::lexer::{parse, Operand};
use crate::pdf::objects::{get, get_dict, get_name, resolve};
use crate::pdf::PdfFile;

/// one text object as word wrote it.
#[derive(Clone, Debug, PartialEq)]
pub struct TextObject {
    pub page: usize,
    pub tm: [f64; 6],
    pub codes: Vec<u32>,
}

impl TextObject {
    pub fn x(&self) -> f64 {
        self.tm[4]
    }

    pub fn y(&self) -> f64 {
        self.tm[5]
    }
}

fn two_byte_fonts(
    document: &Document,
    resources: Option<&lopdf::Dictionary>,
) -> HashMap<Vec<u8>, bool> {
    let mut fonts = HashMap::new();
    let Some(dict) = resources.and_then(|r| get_dict(document, r, b"Font")) else {
        return fonts;
    };
    for (name, value) in dict.iter() {
        if let lopdf::Object::Dictionary(font) = resolve(document, value) {
            let two = matches!(get_name(document, font, b"Subtype"), Some(b"Type0"));
            fonts.insert(name.clone(), two);
        }
    }
    fonts
}

/// every text object of every page.
pub fn read_text_objects(file: &PdfFile) -> Vec<TextObject> {
    let document = &file.document;
    let mut objects = Vec::new();
    for (page_index, page_id) in file.page_ids().into_iter().enumerate() {
        let Ok(page) = document.get_dictionary(page_id) else {
            continue;
        };
        let resources = match get(document, page, b"Resources") {
            Some(lopdf::Object::Dictionary(dict)) => Some(dict),
            _ => None,
        };
        let fonts = two_byte_fonts(document, resources);
        let content = document.get_page_content(page_id);
        objects.extend(page_text_objects(page_index, &content, &fonts));
    }
    objects
}

/// the text objects of one page's content stream.
fn page_text_objects(
    page_index: usize,
    content: &[u8],
    fonts: &HashMap<Vec<u8>, bool>,
) -> Vec<TextObject> {
    let mut objects = Vec::new();
    let mut two_bytes = false;
    let mut current: Option<TextObject> = None;
    for operation in parse(content) {
        let numbers: Vec<f64> = operation
            .operands
            .iter()
            .filter_map(Operand::number)
            .collect();
        match operation.operator.as_str() {
            "Tf" => {
                if let Some(name) = operation.operands.first().and_then(Operand::name) {
                    two_bytes = fonts.get(name).copied().unwrap_or(false);
                }
            }
            "Tm" if numbers.len() == 6 => {
                objects.extend(current.take());
                current = Some(TextObject {
                    page: page_index,
                    tm: [
                        numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
                    ],
                    codes: Vec::new(),
                });
            }
            "Tj" | "TJ" => {
                if let Some(object) = current.as_mut() {
                    add_codes(object, operation.operands.first(), two_bytes);
                }
            }
            "ET" => objects.extend(current.take()),
            _ => {}
        }
    }
    objects.extend(current.take());
    objects
}

/// the char codes a `Tj` string or `TJ` array shows.
fn add_codes(object: &mut TextObject, operand: Option<&Operand>, two_bytes: bool) {
    let strings: Vec<&[u8]> = match operand {
        Some(Operand::String(bytes)) => vec![bytes.as_slice()],
        Some(Operand::Array(items)) => items.iter().filter_map(Operand::string).collect(),
        _ => Vec::new(),
    };
    for bytes in strings {
        if two_bytes {
            object.codes.extend(
                bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| u32::from(p[0]) << 8 | u32::from(p[1])),
            );
        } else {
            object.codes.extend(bytes.iter().map(|&b| u32::from(b)));
        }
    }
}
