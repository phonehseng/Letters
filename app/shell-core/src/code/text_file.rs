//! reading and writing a plain text file for the code editor.
//!
//! the editor works on text with "\n" line breaks only. this file remembers
//! what the file on disk had (a byte order mark, crlf or lf, utf-16) so that
//! saving writes the same kind of file back, and only the edits change.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::session::file_name;
use crate::writer::save::write_atomically;

/// files up to this size open with every feature: highlighting and checks
/// while typing. measured in november 2026 by parsing generated files with
/// @lezer/javascript in node: a whole 2 mb file parses in 0.5 s, a 20 mb
/// one in 3.7 s, and every check reads the whole text again, so above this
/// the grammar and the checks are left out
pub const FULL_FEATURES_LIMIT_BYTES: u64 = 2 * 1024 * 1024;

/// files up to this size can still be edited (no highlighting, no checks
/// while typing). above it they open read-only: sending the text to the
/// window and back on every save would take seconds
pub const EDITABLE_LIMIT_BYTES: u64 = 32 * 1024 * 1024;

/// the largest file the code screen opens at all, read-only
pub const READ_ONLY_LIMIT_BYTES: u64 = 128 * 1024 * 1024;

/// how many bytes are looked at to decide whether a file is text. git and
/// most editors use the first 8000 bytes for the same question
const BINARY_SNIFF_BYTES: usize = 8000;

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16_LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16_BE_BOM: &[u8] = &[0xFE, 0xFF];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

/// what the file on disk looked like, so a save writes the same kind back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFormat {
    pub encoding: Encoding,
    pub byte_order_mark: bool,
    /// the most common line ending; a file with mixed endings saves with this one
    pub line_ending: LineEnding,
    pub mixed_line_endings: bool,
    /// false when some bytes were not valid utf-8 and were shown as "�".
    /// saving such a file replaces those bytes, so the ui asks first
    pub valid: bool,
}

impl Default for TextFormat {
    fn default() -> Self {
        TextFormat {
            encoding: Encoding::Utf8,
            byte_order_mark: false,
            line_ending: native_line_ending(),
            mixed_line_endings: false,
            valid: true,
        }
    }
}

/// what a new, untitled file uses: the system's own line ending.
pub fn native_line_ending() -> LineEnding {
    if cfg!(windows) {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    }
}

/// a text file as the editor receives it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedText {
    pub path: String,
    pub name: String,
    /// with "\n" line breaks only
    pub text: String,
    pub format: TextFormat,
    pub bytes: u64,
    /// true above EDITABLE_LIMIT_BYTES, or when the file can't be written
    pub read_only: bool,
    /// true up to FULL_FEATURES_LIMIT_BYTES
    pub full_features: bool,
}

/// true when the start of the file holds a zero byte and no utf-16 mark:
/// text files never contain one, pictures and programs nearly always do.
pub fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.starts_with(UTF16_LE_BOM) || bytes.starts_with(UTF16_BE_BOM) {
        return false;
    }
    let sniffed = &bytes[..bytes.len().min(BINARY_SNIFF_BYTES)];
    sniffed.contains(&0)
}

/// the text of `bytes` with "\n" line breaks, and what the file looked like.
pub fn decode(bytes: &[u8]) -> (String, TextFormat) {
    let (encoding, byte_order_mark, body) = split_byte_order_mark(bytes);
    let (raw, valid) = match encoding {
        Encoding::Utf8 => decode_utf8(body),
        Encoding::Utf16Le => decode_utf16(body, u16::from_le_bytes),
        Encoding::Utf16Be => decode_utf16(body, u16::from_be_bytes),
    };
    let (line_ending, mixed_line_endings) = count_line_endings(&raw);
    let format = TextFormat {
        encoding,
        byte_order_mark,
        line_ending,
        mixed_line_endings,
        valid,
    };
    (normalize_line_endings(&raw), format)
}

fn split_byte_order_mark(bytes: &[u8]) -> (Encoding, bool, &[u8]) {
    if let Some(body) = bytes.strip_prefix(UTF8_BOM) {
        return (Encoding::Utf8, true, body);
    }
    if let Some(body) = bytes.strip_prefix(UTF16_LE_BOM) {
        return (Encoding::Utf16Le, true, body);
    }
    if let Some(body) = bytes.strip_prefix(UTF16_BE_BOM) {
        return (Encoding::Utf16Be, true, body);
    }
    (Encoding::Utf8, false, bytes)
}

fn decode_utf8(body: &[u8]) -> (String, bool) {
    match std::str::from_utf8(body) {
        Ok(text) => (text.to_owned(), true),
        Err(_) => (String::from_utf8_lossy(body).into_owned(), false),
    }
}

fn decode_utf16(body: &[u8], unit: fn([u8; 2]) -> u16) -> (String, bool) {
    let units: Vec<u16> = body
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| unit(*pair))
        .collect();
    let has_odd_byte = body.len() % 2 == 1;
    match String::from_utf16(&units) {
        Ok(text) => (text, !has_odd_byte),
        Err(_) => (String::from_utf16_lossy(&units), false),
    }
}

/// the most common ending, and whether more than one kind appears.
fn count_line_endings(text: &str) -> (LineEnding, bool) {
    let bytes = text.as_bytes();
    let (mut lf, mut crlf, mut cr) = (0usize, 0usize, 0usize);
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                crlf += 1;
                index += 1;
            }
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
        index += 1;
    }
    let kinds_present = [lf, crlf, cr].iter().filter(|count| **count > 0).count();
    let most_common = if lf == 0 && crlf == 0 && cr == 0 {
        native_line_ending()
    } else if crlf >= lf && crlf >= cr {
        LineEnding::CrLf
    } else if lf >= cr {
        LineEnding::Lf
    } else {
        LineEnding::Cr
    };
    (most_common, kinds_present > 1)
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// the bytes to write for `text` (with "\n" breaks) in `format`.
pub fn encode(text: &str, format: &TextFormat) -> Vec<u8> {
    let with_endings = match format.line_ending {
        LineEnding::Lf => text.to_owned(),
        LineEnding::CrLf => text.replace('\n', "\r\n"),
        LineEnding::Cr => text.replace('\n', "\r"),
    };
    let mut bytes = Vec::with_capacity(with_endings.len() + 3);
    match format.encoding {
        Encoding::Utf8 => {
            if format.byte_order_mark {
                bytes.extend_from_slice(UTF8_BOM);
            }
            bytes.extend_from_slice(with_endings.as_bytes());
        }
        Encoding::Utf16Le => {
            bytes.extend_from_slice(UTF16_LE_BOM);
            with_endings
                .encode_utf16()
                .for_each(|unit| bytes.extend_from_slice(&unit.to_le_bytes()));
        }
        Encoding::Utf16Be => {
            bytes.extend_from_slice(UTF16_BE_BOM);
            with_endings
                .encode_utf16()
                .for_each(|unit| bytes.extend_from_slice(&unit.to_be_bytes()));
        }
    }
    bytes
}

/// reads a file for the editor. the error is a sentence for the person.
pub fn read_text_file(path: &Path) -> Result<OpenedText, String> {
    let name = file_name(path);
    let size = fs::metadata(path)
        .map_err(|error| format!("“{name}” couldn’t be opened. ({error})"))?
        .len();
    if size > READ_ONLY_LIMIT_BYTES {
        return Err(format!(
            "“{name}” is {} MB, too large for the code editor (up to {} MB). Open it in Visual Studio Code or another editor made for large files.",
            size / (1024 * 1024),
            READ_ONLY_LIMIT_BYTES / (1024 * 1024)
        ));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("“{name}” couldn’t be opened. ({error})"))?;
    if looks_binary(&bytes) {
        return Err(not_text_message(&name));
    }
    let (text, format) = decode(&bytes);
    let cannot_write = fs::metadata(path)
        .map(|meta| meta.permissions().readonly())
        .unwrap_or(false);
    Ok(OpenedText {
        path: path.to_string_lossy().into_owned(),
        name,
        text,
        format,
        bytes: size,
        read_only: size > EDITABLE_LIMIT_BYTES || cannot_write,
        full_features: size <= FULL_FEATURES_LIMIT_BYTES,
    })
}

/// the sentence for a file that isn't text (a picture, a program, a zip).
pub fn not_text_message(name: &str) -> String {
    format!("Letters can’t open “{name}”. It isn’t a text file, and Letters opens PDFs, Word and OpenDocument documents, and text or code files.")
}

/// writes `text` to `path` in `format`, in one step (see writer/save.rs).
pub fn save_text_file(path: &Path, text: &str, format: &TextFormat) -> Result<(), String> {
    let name = file_name(path);
    if let Ok(meta) = fs::metadata(path) {
        if meta.is_dir() {
            return Err(format!("“{name}” is a folder. Choose another name."));
        }
        if meta.permissions().readonly() {
            return Err(format!(
                "“{name}” is read-only, so it can’t be replaced. Use Save As to save a copy."
            ));
        }
    }
    write_atomically(&encode(text, format), path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf8_format(line_ending: LineEnding, byte_order_mark: bool) -> TextFormat {
        TextFormat {
            encoding: Encoding::Utf8,
            byte_order_mark,
            line_ending,
            mixed_line_endings: false,
            valid: true,
        }
    }

    #[test]
    fn crlf_files_reach_the_editor_with_plain_line_breaks_and_save_back_as_crlf() {
        let (text, format) = decode(b"int a;\r\nint b;\r\n");
        assert_eq!(text, "int a;\nint b;\n");
        assert_eq!(format.line_ending, LineEnding::CrLf);
        assert!(!format.mixed_line_endings);
        assert_eq!(encode(&text, &format), b"int a;\r\nint b;\r\n");
    }

    #[test]
    fn a_byte_order_mark_is_kept_through_a_round_trip() {
        let original = b"\xEF\xBB\xBF<p>hi</p>\n";
        let (text, format) = decode(original);
        assert_eq!(text, "<p>hi</p>\n");
        assert!(format.byte_order_mark);
        assert_eq!(encode(&text, &format), original);
    }

    #[test]
    fn lf_files_without_a_mark_stay_that_way() {
        let (text, format) = decode(b"a\nb");
        assert_eq!(format, utf8_format(LineEnding::Lf, false));
        assert_eq!(encode(&text, &format), b"a\nb");
    }

    #[test]
    fn mixed_endings_are_noticed_and_the_most_common_one_wins() {
        let (text, format) = decode(b"a\r\nb\r\nc\nd");
        assert_eq!(text, "a\nb\nc\nd");
        assert_eq!(format.line_ending, LineEnding::CrLf);
        assert!(format.mixed_line_endings);
    }

    #[test]
    fn invalid_utf8_is_shown_with_replacement_marks_and_flagged() {
        let (text, format) = decode(b"caf\xE9\n");
        assert!(text.contains('\u{FFFD}'));
        assert!(!format.valid);
    }

    #[test]
    fn utf16_files_are_read_and_written_as_utf16() {
        let original: Vec<u8> = [0xFF, 0xFE, b'h', 0, b'i', 0, b'\r', 0, b'\n', 0].to_vec();
        let (text, format) = decode(&original);
        assert_eq!(text, "hi\n");
        assert_eq!(format.encoding, Encoding::Utf16Le);
        assert_eq!(encode(&text, &format), original);
    }

    #[test]
    fn zero_bytes_mean_a_file_is_not_text() {
        assert!(looks_binary(b"\x89PNG\r\n\x1a\n\0\0\0"));
        assert!(!looks_binary(b"plain text"));
    }

    #[test]
    fn saving_and_reading_a_file_keeps_its_format() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("main.c");
        fs::write(&path, b"\xEF\xBB\xBFint main(void)\r\n{\r\n}\r\n").unwrap();
        let opened = read_text_file(&path).unwrap();
        assert_eq!(opened.text, "int main(void)\n{\n}\n");
        assert!(opened.full_features);
        assert!(!opened.read_only);
        let edited = opened.text.replace("{\n}", "{\n  return 0;\n}");
        save_text_file(&path, &edited, &opened.format).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            b"\xEF\xBB\xBFint main(void)\r\n{\r\n  return 0;\r\n}\r\n"
        );
    }

    #[test]
    fn a_binary_file_is_refused_with_a_sentence() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("picture.bin");
        fs::write(&path, [0u8, 1, 2, 3]).unwrap();
        let error = read_text_file(&path).unwrap_err();
        assert!(error.contains("isn’t a text file"), "{error}");
    }
}
