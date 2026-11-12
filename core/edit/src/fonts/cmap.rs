//! character maps (CMaps): /ToUnicode, which says what text each code
//! is, and embedded /Encoding maps of composite fonts, which turn codes
//! into glyph ids (CIDs).

use std::collections::BTreeMap;

use lopdf::Object;

use crate::lexer;

/// a parsed CMap. codes are kept with their length in bytes, since <41>
/// and <0041> are different codes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CMap {
    /// the byte lengths codes may have, from the codespace ranges
    pub code_lengths: Vec<usize>,
    /// code to text, from bfchar and bfrange
    pub to_text: BTreeMap<(usize, u32), String>,
    /// code to glyph id, from cidchar and cidrange
    pub to_cid: BTreeMap<(usize, u32), u32>,
}

fn code_of(object: &Object) -> Option<(usize, u32)> {
    let bytes = object.as_str().ok()?;
    if bytes.is_empty() || bytes.len() > 4 {
        return None;
    }
    Some((
        bytes.len(),
        bytes
            .iter()
            .fold(0u32, |value, byte| value << 8 | u32::from(*byte)),
    ))
}

/// text written as UTF-16BE in a CMap destination.
fn text_of(object: &Object) -> Option<String> {
    let bytes = object.as_str().ok()?;
    let (units, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = units.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
    Some(String::from_utf16_lossy(&units))
}

/// the next text after `text` in a bfrange: its last character moves on by one.
fn next_text(text: &str, step: u32) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    if let Some(last) = chars.last_mut() {
        *last = char::from_u32(*last as u32 + step).unwrap_or(*last);
    }
    chars.into_iter().collect()
}

/// a range has no more than this many codes; larger ones are damaged.
const MAX_RANGE: u32 = 65_536;

pub(crate) fn parse(content: &[u8]) -> CMap {
    let mut cmap = CMap::default();
    for operation in lexer::parse(content) {
        let operands = &operation.operands;
        match operation.operator.as_str() {
            "endcodespacerange" => {
                for pair in operands.as_chunks::<2>().0 {
                    if let Some((length, _)) = code_of(&pair[0]) {
                        if !cmap.code_lengths.contains(&length) {
                            cmap.code_lengths.push(length);
                        }
                    }
                }
            }
            "endbfchar" => {
                for pair in operands.as_chunks::<2>().0 {
                    if let (Some(code), Some(text)) = (code_of(&pair[0]), text_of(&pair[1])) {
                        cmap.to_text.insert(code, text);
                    }
                }
            }
            "endbfrange" => {
                for triple in operands.as_chunks::<3>().0 {
                    add_bf_range(&mut cmap, triple);
                }
            }
            "endcidchar" => {
                for pair in operands.as_chunks::<2>().0 {
                    if let (Some(code), Ok(cid)) = (code_of(&pair[0]), pair[1].as_i64()) {
                        cmap.to_cid.insert(code, cid as u32);
                    }
                }
            }
            "endcidrange" => {
                for triple in operands.as_chunks::<3>().0 {
                    let (Some((length, low)), Some((_, high)), Ok(cid)) =
                        (code_of(&triple[0]), code_of(&triple[1]), triple[2].as_i64())
                    else {
                        continue;
                    };
                    for code in low..=high.min(low.saturating_add(MAX_RANGE)) {
                        cmap.to_cid
                            .insert((length, code), cid as u32 + (code - low));
                    }
                }
            }
            _ => {}
        }
    }
    cmap.code_lengths.sort_unstable();
    cmap
}

fn add_bf_range(cmap: &mut CMap, triple: &[Object]) {
    let (Some((length, low)), Some((_, high))) = (code_of(&triple[0]), code_of(&triple[1])) else {
        return;
    };
    let high = high.min(low.saturating_add(MAX_RANGE));
    match &triple[2] {
        Object::Array(targets) => {
            for (code, target) in (low..=high).zip(targets) {
                if let Some(text) = text_of(target) {
                    cmap.to_text.insert((length, code), text);
                }
            }
        }
        start => {
            if let Some(first) = text_of(start) {
                for code in low..=high {
                    cmap.to_text
                        .insert((length, code), next_text(&first, code - low));
                }
            }
        }
    }
}

/// writes a ToUnicode CMap for two-byte codes, as readers expect for
/// fonts Letters embeds. copying text out of the pdf depends on it.
pub(crate) fn write_two_byte_to_unicode(entries: &BTreeMap<u16, String>) -> Vec<u8> {
    let mut text = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(&u16, &String)> = entries.iter().collect();
    // a cmap section may hold at most 100 entries
    for chunk in entries.chunks(100) {
        text.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (code, value) in chunk {
            let utf16: String = value
                .encode_utf16()
                .map(|unit| format!("{unit:04X}"))
                .collect();
            text.push_str(&format!("<{code:04X}> <{utf16}>\n"));
        }
        text.push_str("endbfchar\n");
    }
    text.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    text.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_chars_and_ranges() {
        let cmap = parse(
            b"1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
              2 beginbfchar <0003> <0020> <0024> <00E9> endbfchar\n\
              2 beginbfrange <0010> <0012> <0041> <0020> <0021> [<0066006C> <0078>] endbfrange",
        );
        assert_eq!(cmap.code_lengths, [2]);
        assert_eq!(cmap.to_text[&(2, 0x24)], "é");
        assert_eq!(cmap.to_text[&(2, 0x12)], "C");
        assert_eq!(cmap.to_text[&(2, 0x20)], "fl");
        assert_eq!(cmap.to_text[&(2, 0x21)], "x");
    }

    #[test]
    fn written_maps_read_back() {
        let entries = BTreeMap::from([(5u16, "é".to_string()), (9u16, "ﬁ".to_string())]);
        let cmap = parse(&write_two_byte_to_unicode(&entries));
        assert_eq!(cmap.to_text[&(2, 5)], "é");
        assert_eq!(cmap.to_text[&(2, 9)], "ﬁ");
    }
}
