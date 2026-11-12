//! the signature dictionary's two placeholders and filling them in.
//!
//! a signature covers the whole file except its own /Contents. so the
//! dictionary is written with a fixed-width /ByteRange and a /Contents of
//! zeros, the file is finished around it, and only then are the real byte
//! range and the signature (or time stamp token) written into that room.

use lopdf::Object;

use crate::error::{FormError, FormResult};
use crate::objects::text_string;

/// "0 0000000000 0000000000 0000000000": four numbers of up to ten digits,
/// enough for files up to 9,999,999,999 bytes.
const BYTE_RANGE_WIDTH: usize = 1 + 3 * 11;

/// the start of a signature dictionary: `kind_entries` are its /Type,
/// /Filter and /SubFilter, then the placeholders, then `text_entries`
/// (/Name, /M, /Reason...) and `extra` (already written pdf objects, such
/// as /Reference for certification).
pub(super) fn signature_body(
    kind_entries: &str,
    reserved: usize,
    text_entries: &[(&str, String)],
    extra: &[u8],
) -> FormResult<Vec<u8>> {
    let mut body = format!("<< {kind_entries}").into_bytes();
    let zeros = "0".repeat(10);
    body.extend_from_slice(format!(" /ByteRange [0 {zeros} {zeros} {zeros}]").as_bytes());
    body.extend_from_slice(b" /Contents <");
    body.extend(std::iter::repeat_n(b'0', reserved * 2));
    body.extend_from_slice(b">");
    for (key, value) in text_entries {
        if value.trim().is_empty() {
            continue;
        }
        body.extend_from_slice(format!(" /{key} ").as_bytes());
        lopdf::Writer::write_object(&mut body, &text_string(value))
            .map_err(|error| FormError::SigningFailed(error.to_string()))?;
    }
    body.extend_from_slice(extra);
    body.extend_from_slice(b" /Prop_Build << /App << /Name /Letters >> >> >>");
    Ok(body)
}

/// writes a pdf object after a space, for `extra` entries.
pub(super) fn entry(key: &str, value: &Object) -> FormResult<Vec<u8>> {
    let mut out = format!(" /{key} ").into_bytes();
    lopdf::Writer::write_object(&mut out, value)
        .map_err(|error| FormError::SigningFailed(error.to_string()))?;
    Ok(out)
}

/// fills in /ByteRange, then asks `make_contents` for the signature over
/// the two covered parts of the file and puts it into /Contents.
/// `start` is where the signature dictionary's object begins.
pub(super) fn fill_placeholders(
    mut bytes: Vec<u8>,
    start: usize,
    reserved: usize,
    make_contents: impl FnOnce([&[u8]; 2]) -> FormResult<Vec<u8>>,
) -> FormResult<Vec<u8>> {
    let missing = || FormError::SigningFailed("the signature placeholder went missing".into());
    let range_at =
        find(&bytes, start, b"/ByteRange [").ok_or_else(missing)? + b"/ByteRange [".len();
    let contents_open =
        find(&bytes, start, b"/Contents <").ok_or_else(missing)? + b"/Contents ".len();
    let contents_close = contents_open + 1 + reserved * 2;
    if bytes.get(contents_close) != Some(&b'>') {
        return Err(missing());
    }
    let after_contents = contents_close + 1;
    let tail_length = bytes.len() - after_contents;
    let range = format!("0 {contents_open} {after_contents} {tail_length}");
    if range.len() > BYTE_RANGE_WIDTH {
        return Err(FormError::SigningFailed(
            "the file is too large to sign".into(),
        ));
    }
    let padded = format!("{range:<BYTE_RANGE_WIDTH$}");
    bytes[range_at..range_at + BYTE_RANGE_WIDTH].copy_from_slice(padded.as_bytes());

    let contents = make_contents([&bytes[..contents_open], &bytes[after_contents..]])?;
    let hex: Vec<u8> = contents
        .iter()
        .flat_map(|byte| format!("{byte:02X}").into_bytes())
        .collect();
    if hex.len() > reserved * 2 {
        return Err(FormError::SigningFailed(
            "the signature is bigger than the room kept for it".into(),
        ));
    }
    bytes[contents_open + 1..contents_open + 1 + hex.len()].copy_from_slice(&hex);
    Ok(bytes)
}

fn find(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| from + position)
}
