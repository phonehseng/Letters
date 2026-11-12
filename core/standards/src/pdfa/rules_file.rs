//! pdf/a rules about the file's bytes themselves: the header and its
//! binary comment, the end of the file, cross-reference streams in part 1,
//! and the line breaks around stream data. all are fixed by saving with
//! `save_pdfa`.

use super::level::PdfaLevel;
use crate::report::{Findings, Fix};

/// a binary comment needs at least four bytes of 128 or more, so tools
/// treat the file as binary (19005-1 6.1.2, 19005-2 6.1.2).
const BINARY_BYTES_NEEDED: usize = 4;

pub(super) fn check_file(bytes: &[u8], level: PdfaLevel, findings: &mut Findings) {
    check_header(bytes, level, findings);
    check_end(bytes, level, findings);
    check_stream_keywords(bytes, level, findings);
    if level.is_part_one() {
        check_part_one_structure(bytes, findings);
    }
}

fn check_header(bytes: &[u8], level: PdfaLevel, findings: &mut Findings) {
    let clause = level.clause("6.1.2", "6.1.2");
    let well_formed = bytes.len() > 8
        && bytes.starts_with(b"%PDF-")
        && bytes[5].is_ascii_digit()
        && bytes[6] == b'.'
        && bytes[7].is_ascii_digit();
    if !well_formed {
        findings.add(
            "file.header",
            &clause,
            "The file doesn’t start with a %PDF-n.n header.".into(),
            Fix::Automatic,
        );
        return;
    }
    let second_line = bytes[8..]
        .iter()
        .skip_while(|byte| **byte == b'\r' || **byte == b'\n')
        .take_while(|byte| **byte != b'\r' && **byte != b'\n')
        .copied()
        .collect::<Vec<u8>>();
    let binary = second_line.starts_with(b"%")
        && second_line.iter().filter(|byte| **byte >= 128).count() >= BINARY_BYTES_NEEDED;
    if !binary {
        findings.add(
            "file.binaryComment",
            &clause,
            "The header isn’t followed by a comment of binary characters, which marks the file as binary.".into(),
            Fix::Automatic,
        );
    }
}

/// nothing but one line break may follow the last %%EOF (19005-1 6.1.3,
/// 19005-2 6.1.3).
fn check_end(bytes: &[u8], level: PdfaLevel, findings: &mut Findings) {
    let Some(at) = bytes.windows(5).rposition(|window| window == b"%%EOF") else {
        findings.add(
            "file.eof",
            &level.clause("6.1.3", "6.1.3"),
            "The file has no %%EOF marker at its end.".into(),
            Fix::Automatic,
        );
        return;
    };
    let rest = &bytes[at + 5..];
    let allowed = matches!(rest, [] | [b'\n'] | [b'\r'] | [b'\r', b'\n']);
    if !allowed {
        findings.add(
            "file.eof",
            &level.clause("6.1.3", "6.1.3"),
            "There is data after the file’s last %%EOF marker.".into(),
            Fix::Automatic,
        );
    }
}

/// the stream keyword is followed by cr lf or lf, never a lone cr
/// (19005-1 6.1.7, 19005-2 6.1.7.1).
fn check_stream_keywords(bytes: &[u8], level: PdfaLevel, findings: &mut Findings) {
    let mut from = 0;
    while let Some(found) = find(bytes, from, b"stream") {
        from = found + 6;
        let after_keyword = bytes.get(found + 6..found + 8).unwrap_or(&[]);
        // a stream keyword follows its dictionary's >>; anything else (the
        // word inside a name such as /octet-stream, or endstream) isn't one
        if !follows_dictionary(bytes, found) {
            continue;
        }
        let good = after_keyword.starts_with(b"\r\n") || after_keyword.starts_with(b"\n");
        if !good {
            findings.add(
                "file.streamKeyword",
                &level.clause("6.1.7", "6.1.7.1"),
                "A stream’s data doesn’t start on a new line after the stream keyword.".into(),
                Fix::Automatic,
            );
            return;
        }
    }
}

fn follows_dictionary(bytes: &[u8], keyword: usize) -> bool {
    let before = &bytes[..keyword];
    let trimmed = before.iter().rposition(|byte| !byte.is_ascii_whitespace());
    trimmed.is_some_and(|last| last >= 1 && &before[last - 1..=last] == b">>")
}

/// part 1 is pdf 1.4, which has no cross-reference or object streams.
fn check_part_one_structure(bytes: &[u8], findings: &mut Findings) {
    let uses_new_structure = find(bytes, 0, b"/Type /XRef").is_some()
        || find(bytes, 0, b"/Type/XRef").is_some()
        || find(bytes, 0, b"/Type /ObjStm").is_some()
        || find(bytes, 0, b"/Type/ObjStm").is_some();
    if uses_new_structure {
        findings.add(
            "file.compressedStructure",
            "ISO 19005-1 6.1.4",
            "The file uses cross-reference or object streams, which PDF/A-1 (based on PDF 1.4) doesn’t allow.".into(),
            Fix::Automatic,
        );
    }
}

fn find(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| from + position)
}
