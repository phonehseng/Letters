//! what the comparison needs from a pdf font: letters, widths and style.
//!
//! a string in a content stream is a list of codes. the font says how to
//! split the bytes into codes, which letters each code means, and how far
//! each code moves the pen. those three answers are all that is needed to
//! know which letter is where on the page.

use std::sync::Arc;

use lopdf::{Dictionary, Document, Object};

use super::cmap::ToUnicode;
use super::page_frame::number;
use crate::standard_fonts::{standard_font_for, StandardFont};

/// pdf font descriptor flags, from the pdf specification
const FLAG_ITALIC: i64 = 1 << 6;
const FLAG_FORCE_BOLD: i64 = 1 << 18;

/// the most entries read from a width list, so a damaged file can't use
/// unbounded memory.
const MAX_WIDTH_ENTRIES: usize = 65_536;

/// a font, ready to turn codes into placed letters.
pub(crate) struct Font {
    /// the font's name, without a subset prefix such as "ABCDEF+"
    pub name: Arc<str>,
    pub bold: bool,
    pub italic: bool,
    /// how far the letters reach above and below the baseline, in thousandths of the size
    pub ascent: f32,
    pub descent: f32,
    two_byte: bool,
    widths: Widths,
    /// turns a width from the font's units into a fraction of the font size
    width_scale: f32,
    to_unicode: Option<ToUnicode>,
    /// for one-byte fonts: the letters each of the 256 codes means
    simple_letters: Option<Vec<String>>,
}

enum Widths {
    Simple {
        first_char: u32,
        widths: Vec<f32>,
        missing: f32,
        standard: Option<&'static StandardFont>,
    },
    Composite {
        default: f32,
        widths: std::collections::HashMap<u32, f32>,
    },
}

impl Font {
    /// reads a font dictionary. it never fails: anything missing gets a
    /// sensible default, because a slightly wrong width is far better than
    /// losing the page's text.
    pub fn load(document: &Document, font: &Dictionary) -> Font {
        let subtype = name_of(document, font, b"Subtype").unwrap_or_default();
        let name = base_name(document, font, &subtype);
        let two_byte = subtype == "Type0";
        let descendant = two_byte.then(|| descendant_font(document, font)).flatten();
        let descriptor = descendant
            .and_then(|descendant| dictionary_at(document, descendant, b"FontDescriptor"))
            .or_else(|| dictionary_at(document, font, b"FontDescriptor"));
        let standard = standard_font_for(&name);
        let (bold, italic) = style(&name, descriptor, document);
        let (ascent, descent) = vertical_metrics(document, descriptor, standard, &subtype);
        let widths = match descendant {
            Some(descendant) => composite_widths(document, descendant),
            None => simple_widths(document, font, descriptor, standard),
        };
        let to_unicode = stream_at(document, font, b"ToUnicode")
            .and_then(|stream| stream.get_plain_content().ok())
            .map(|program| ToUnicode::parse(&program));
        let simple_letters = (!two_byte).then(|| simple_letters(document, font));
        Font {
            name: Arc::from(name.as_str()),
            bold,
            italic,
            ascent,
            descent,
            two_byte,
            widths,
            width_scale: width_scale(document, font, &subtype),
            to_unicode,
            simple_letters,
        }
    }

    /// splits a string's bytes into (code, byte length) pairs.
    pub fn codes(&self, bytes: &[u8]) -> Vec<(u32, u8)> {
        let default_length = if self.two_byte { 2 } else { 1 };
        match &self.to_unicode {
            Some(map) if self.two_byte && map.has_code_spaces() => {
                map.split_codes(bytes, default_length)
            }
            _ if self.two_byte => ToUnicode::default().split_codes(bytes, 2),
            _ => bytes.iter().map(|&byte| (byte as u32, 1)).collect(),
        }
    }

    /// the letters one code stands for. may be several (a ligature such as
    /// "fi") or none (a code the file never explains).
    pub fn text(&self, code: u32, length: u8) -> String {
        if let Some(text) = self
            .to_unicode
            .as_ref()
            .and_then(|map| map.text_for(code, length))
        {
            return text;
        }
        if let Some(letters) = &self.simple_letters {
            return letters.get(code as usize).cloned().unwrap_or_default();
        }
        // a two-byte font without a map: the code is often the unicode value
        char::from_u32(code)
            .filter(|ch| !ch.is_control())
            .map(String::from)
            .unwrap_or_default()
    }

    /// how far a code moves the pen, as a fraction of the font size.
    pub fn advance(&self, code: u32, text: &str) -> f32 {
        let width = match &self.widths {
            Widths::Simple {
                first_char,
                widths,
                missing,
                standard,
            } => code
                .checked_sub(*first_char)
                .and_then(|index| widths.get(index as usize).copied())
                .or_else(|| {
                    let first = text.chars().next()?;
                    standard.map(|font| font.char_width(first))
                })
                .unwrap_or(*missing),
            Widths::Composite { default, widths } => widths.get(&code).copied().unwrap_or(*default),
        };
        width * self.width_scale
    }

    /// pdf adds word spacing only to the one-byte code 32.
    pub fn is_word_space(&self, code: u32, length: u8) -> bool {
        code == 32 && length == 1
    }
}

fn name_of(document: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    let (_, object) = document.dereference(dict.get(key).ok()?).ok()?;
    object
        .as_name()
        .ok()
        .map(|name| String::from_utf8_lossy(name).into_owned())
}

fn dictionary_at<'a>(
    document: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Dictionary> {
    let (_, object) = document.dereference(dict.get(key).ok()?).ok()?;
    object.as_dict().ok()
}

fn stream_at<'a>(
    document: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a lopdf::Stream> {
    let (_, object) = document.dereference(dict.get(key).ok()?).ok()?;
    object.as_stream().ok()
}

fn array_at<'a>(
    document: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Vec<Object>> {
    let (_, object) = document.dereference(dict.get(key).ok()?).ok()?;
    object.as_array().ok()
}

fn number_at(document: &Document, dict: &Dictionary, key: &[u8]) -> Option<f32> {
    let (_, object) = document.dereference(dict.get(key).ok()?).ok()?;
    number(object)
}

/// the font's name, minus the six-letter subset tag some files add.
fn base_name(document: &Document, font: &Dictionary, subtype: &str) -> String {
    let name = name_of(document, font, b"BaseFont").unwrap_or_else(|| {
        if subtype == "Type3" {
            "Type3".to_owned()
        } else {
            "Unknown".to_owned()
        }
    });
    strip_subset_tag(&name).to_owned()
}

pub(crate) fn strip_subset_tag(name: &str) -> &str {
    match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.chars().all(|ch| ch.is_ascii_uppercase()) => {
            rest
        }
        _ => name,
    }
}

fn descendant_font<'a>(document: &'a Document, font: &'a Dictionary) -> Option<&'a Dictionary> {
    let first = array_at(document, font, b"DescendantFonts")?.first()?;
    let (_, object) = document.dereference(first).ok()?;
    object.as_dict().ok()
}

/// bold and italic, from the name first and the descriptor second, since
/// names are what people see in a font menu.
fn style(name: &str, descriptor: Option<&Dictionary>, document: &Document) -> (bool, bool) {
    let lower = name.to_ascii_lowercase();
    let mut bold = ["bold", "black", "heavy", "semibold", "demi"]
        .iter()
        .any(|word| lower.contains(word));
    let mut italic = lower.contains("italic") || lower.contains("oblique");
    if let Some(descriptor) = descriptor {
        let flags = descriptor
            .get(b"Flags")
            .ok()
            .and_then(|flags| flags.as_i64().ok())
            .unwrap_or(0);
        bold |= flags & FLAG_FORCE_BOLD != 0;
        bold |= number_at(document, descriptor, b"FontWeight").unwrap_or(0.0) >= 600.0;
        italic |= flags & FLAG_ITALIC != 0;
        italic |= number_at(document, descriptor, b"ItalicAngle").unwrap_or(0.0) != 0.0;
    }
    (bold, italic)
}

fn vertical_metrics(
    document: &Document,
    descriptor: Option<&Dictionary>,
    standard: Option<&StandardFont>,
    subtype: &str,
) -> (f32, f32) {
    let from_descriptor = descriptor.and_then(|descriptor| {
        let ascent = number_at(document, descriptor, b"Ascent")?;
        let descent = number_at(document, descriptor, b"Descent").unwrap_or(-200.0);
        // some files write zero, which would make every letter flat
        (ascent > 100.0).then_some((ascent, descent.min(0.0)))
    });
    from_descriptor
        .or_else(|| standard.map(|font| (font.ascent, font.descent)))
        .unwrap_or(if subtype == "Type3" {
            (800.0, -200.0)
        } else {
            (750.0, -250.0)
        })
}

fn simple_widths(
    document: &Document,
    font: &Dictionary,
    descriptor: Option<&Dictionary>,
    standard: Option<&'static StandardFont>,
) -> Widths {
    let widths: Vec<f32> = array_at(document, font, b"Widths")
        .map(|list| {
            list.iter()
                .take(MAX_WIDTH_ENTRIES)
                .map(|item| {
                    document
                        .dereference(item)
                        .ok()
                        .and_then(|(_, item)| number(item))
                        .unwrap_or(0.0)
                })
                .collect()
        })
        .unwrap_or_default();
    let first_char = number_at(document, font, b"FirstChar")
        .unwrap_or(0.0)
        .max(0.0) as u32;
    let missing = descriptor
        .and_then(|descriptor| number_at(document, descriptor, b"MissingWidth"))
        .filter(|width| *width > 0.0)
        .unwrap_or(if standard.is_some() { 556.0 } else { 500.0 });
    Widths::Simple {
        first_char,
        widths,
        missing,
        // the standard tables only stand in when the file lists no widths
        standard: if array_at(document, font, b"Widths").is_none() {
            standard
        } else {
            None
        },
    }
}

/// reads a composite font's /W list: "c [w1 w2 ...]" gives widths from c on,
/// "first last w" gives one width to a whole run of codes.
fn composite_widths(document: &Document, descendant: &Dictionary) -> Widths {
    let default = number_at(document, descendant, b"DW").unwrap_or(1000.0);
    let mut widths = std::collections::HashMap::new();
    let items: Vec<&Object> = array_at(document, descendant, b"W")
        .map(|list| {
            list.iter()
                .filter_map(|item| document.dereference(item).ok().map(|(_, item)| item))
                .collect()
        })
        .unwrap_or_default();
    let mut position = 0;
    while position + 1 < items.len() && widths.len() < MAX_WIDTH_ENTRIES {
        let Some(first) = number(items[position]) else {
            break;
        };
        let first = first.max(0.0) as u32;
        if let Ok(list) = items[position + 1].as_array() {
            for (offset, width) in list.iter().enumerate() {
                if let Some(width) = number(width) {
                    widths.insert(first + offset as u32, width);
                }
            }
            position += 2;
        } else if let (Some(last), Some(width)) = (
            number(items[position + 1]),
            items.get(position + 2).and_then(|item| number(item)),
        ) {
            let last = (last.max(0.0) as u32).min(first + MAX_WIDTH_ENTRIES as u32);
            for code in first..=last {
                widths.insert(code, width);
            }
            position += 3;
        } else {
            break;
        }
    }
    Widths::Composite { default, widths }
}

/// type 3 fonts measure in their own units, given by /FontMatrix; all
/// others measure in thousandths of the font size.
fn width_scale(document: &Document, font: &Dictionary, subtype: &str) -> f32 {
    if subtype != "Type3" {
        return 0.001;
    }
    array_at(document, font, b"FontMatrix")
        .and_then(|matrix| matrix.first())
        .and_then(number)
        .filter(|scale| *scale > 0.0)
        .unwrap_or(0.001)
}

/// the letters of all 256 codes of a one-byte font, from its encoding.
fn simple_letters(document: &Document, font: &Dictionary) -> Vec<String> {
    let encoding = font.get_font_encoding(document).ok();
    (0..=255u8)
        .map(|byte| {
            encoding
                .as_ref()
                .and_then(|encoding| encoding.bytes_to_string(&[byte]).ok())
                .filter(|text| !text.is_empty())
                // an encoding lopdf doesn't know: latin-1 is the usual guess
                .unwrap_or_else(|| (byte as char).to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subset_tags_are_removed() {
        assert_eq!(strip_subset_tag("ABCDEF+Arial-BoldMT"), "Arial-BoldMT");
        assert_eq!(strip_subset_tag("Times-Roman"), "Times-Roman");
        assert_eq!(strip_subset_tag("abc+Thing"), "abc+Thing");
    }

    #[test]
    fn style_comes_from_the_name() {
        let document = Document::with_version("1.7");
        assert_eq!(
            style("Helvetica-BoldOblique", None, &document),
            (true, true)
        );
        assert_eq!(style("Times-Roman", None, &document), (false, false));
    }
}
