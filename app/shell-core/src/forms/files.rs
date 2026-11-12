//! reading a file the person chose (a signature picture, a certificate)
//! and writing a new certificate file where the save panel said.

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::session::file_name;

/// reads a chosen file whole, refusing one bigger than `max_bytes`.
/// pictures and certificates are small; a huge file is almost certainly
/// the wrong one, and reading it would only make the app slow.
pub fn read_chosen_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    let name = file_name(path);
    let meta = fs::metadata(path)
        .map_err(|error| format!("“{name}” can’t be read. Choose it again. ({error})"))?;
    if meta.len() > max_bytes {
        let megabytes = max_bytes / (1024 * 1024);
        return Err(format!(
            "“{name}” is too big. Choose a file smaller than {megabytes} MB."
        ));
    }
    fs::read(path).map_err(|error| format!("“{name}” can’t be read. Choose it again. ({error})"))
}

/// a picture as a `data:` address, the one kind of address the window's
/// security rules let a picture load from besides the app itself.
pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64(bytes))
}

/// the mime type of a picture, from its file name.
pub fn picture_mime(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

/// standard base64 with padding (rfc 4648). small enough to write here
/// rather than add a library for one use.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            // one character per six bits; the missing bytes of the last chunk become "="
            if index <= chunk.len() {
                out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// writes `bytes` to a path the save panel confirmed, in one step: a
/// temporary file beside it first, then a rename, so a crash never
/// leaves half a file. the panel already asked about replacing.
pub fn write_chosen_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = file_name(path);
    let temporary = path.with_file_name(format!(".{name}.letters-saving"));
    let written = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    written.map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("“{name}” couldn’t be saved. Check that you can save in that folder, then try again. ({error})")
    })
}
