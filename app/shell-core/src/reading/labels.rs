//! a pdf's page labels: the numbers printed on its pages.
//!
//! books often number their front matter "i, ii, iii" and start "1" at
//! the first chapter, so the 13th sheet of paper is page 1. pdf stores
//! this as ranges (from page 12: decimal numbers starting at 1), which is
//! what Go To uses to find "page xii" or "page 1" the way the reader
//! means it.

use lopdf::{Dictionary, Document, Object};

/// one label per page, or empty when the pdf has no labels.
pub fn page_labels(document: &Document) -> Vec<String> {
    let page_count = document.get_pages().len();
    let Some(tree) = label_tree(document) else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    collect_ranges(document, tree, &mut ranges, 0);
    ranges.sort_by_key(|(start, _)| *start);
    if ranges.is_empty() {
        return Vec::new();
    }
    (0..page_count)
        .map(|page| {
            ranges
                .iter()
                .rev()
                .find(|(start, _)| *start <= page)
                .map_or_else(
                    || (page + 1).to_string(),
                    |(start, range)| range.label(page - start),
                )
        })
        .collect()
}

fn label_tree(document: &Document) -> Option<&Dictionary> {
    let catalog = document.catalog().ok()?;
    let tree = catalog.get(b"PageLabels").ok()?;
    resolve(document, tree)?.as_dict().ok()
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        other => Some(other),
    }
}

/// a range's numbering: prefix, style and the number of its first page.
struct LabelRange {
    prefix: String,
    style: Option<u8>,
    first: usize,
}

impl LabelRange {
    fn from_dict(document: &Document, dict: &Dictionary) -> LabelRange {
        let style = dict
            .get(b"S")
            .ok()
            .and_then(|s| s.as_name().ok())
            .and_then(|name| name.first().copied());
        let prefix = dict
            .get(b"P")
            .ok()
            .and_then(|p| resolve(document, p))
            .map(text_of)
            .unwrap_or_default();
        let first = dict
            .get(b"St")
            .ok()
            .and_then(|s| s.as_i64().ok())
            .filter(|n| *n >= 1)
            .unwrap_or(1) as usize;
        LabelRange {
            prefix,
            style,
            first,
        }
    }

    fn label(&self, offset: usize) -> String {
        let number = self.first + offset;
        let numeral = match self.style {
            Some(b'D') => number.to_string(),
            Some(b'R') => roman(number).to_uppercase(),
            Some(b'r') => roman(number),
            Some(b'A') => letters(number).to_uppercase(),
            Some(b'a') => letters(number),
            _ => String::new(),
        };
        format!("{}{numeral}", self.prefix)
    }
}

/// the number tree's leaves, followed through its kids (with a depth
/// limit so a damaged file cannot loop).
fn collect_ranges(
    document: &Document,
    node: &Dictionary,
    out: &mut Vec<(usize, LabelRange)>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    if let Some(Ok(nums)) = node
        .get(b"Nums")
        .ok()
        .and_then(|n| resolve(document, n))
        .map(|n| n.as_array())
    {
        for pair in nums.chunks(2) {
            let [start, range] = pair else { continue };
            let Ok(start) = start.as_i64() else { continue };
            let Some(Ok(range)) = resolve(document, range).map(|r| r.as_dict()) else {
                continue;
            };
            if start >= 0 {
                out.push((start as usize, LabelRange::from_dict(document, range)));
            }
        }
    }
    if let Some(Ok(kids)) = node
        .get(b"Kids")
        .ok()
        .and_then(|k| resolve(document, k))
        .map(|k| k.as_array())
    {
        for kid in kids {
            if let Some(Ok(kid)) = resolve(document, kid).map(|k| k.as_dict()) {
                collect_ranges(document, kid, out, depth + 1);
            }
        }
    }
}

/// a pdf text string: utf-16 with a byte order mark, or single bytes.
fn text_of(object: &Object) -> String {
    let Object::String(bytes, _) = object else {
        return String::new();
    };
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    bytes.iter().map(|b| char::from(*b)).collect()
}

pub fn roman(mut number: usize) -> String {
    const NUMERALS: [(usize, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (value, numeral) in NUMERALS {
        while number >= value {
            out.push_str(numeral);
            number -= value;
        }
    }
    out
}

/// a, b, ... z, aa, bb, ... as pdf defines letter numbering.
fn letters(number: usize) -> String {
    if number == 0 {
        return String::new();
    }
    let letter = (b'a' + ((number - 1) % 26) as u8) as char;
    std::iter::repeat_n(letter, (number - 1) / 26 + 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn numerals() {
        assert_eq!(roman(4), "iv");
        assert_eq!(roman(12), "xii");
        assert_eq!(roman(1994), "mcmxciv");
        assert_eq!(letters(1), "a");
        assert_eq!(letters(27), "aa");
    }

    #[test]
    fn labels_from_a_pdf_with_front_matter_and_an_appendix() {
        let folder = crate::test_pdf::temp_dir("page-labels");
        let path = folder.join("book.pdf");
        crate::test_pdf::write_pdf(&path, 8);
        let mut document = Document::load(&path).unwrap();
        let labels = dictionary! {
            "Nums" => vec![
                0.into(), Object::Dictionary(dictionary! { "S" => "r" }),
                3.into(), Object::Dictionary(dictionary! { "S" => "D" }),
                6.into(), Object::Dictionary(dictionary! {
                    "S" => "D",
                    "P" => Object::string_literal("A-"),
                    "St" => 1,
                }),
            ],
        };
        let labels_id = document.add_object(labels);
        document
            .catalog_mut()
            .unwrap()
            .set("PageLabels", Object::Reference(labels_id));
        assert_eq!(
            page_labels(&document),
            vec!["i", "ii", "iii", "1", "2", "3", "A-1", "A-2"]
        );
        let plain = Document::load(&path).unwrap();
        assert!(page_labels(&plain).is_empty());
    }
}
