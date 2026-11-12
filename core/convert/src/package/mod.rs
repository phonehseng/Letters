//! what xlsx and pptx files have in common: a zip of xml parts, written
//! to a temporary file and moved into place only when complete.

mod zip;

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

pub(crate) use zip::ZipWriter;

use crate::error::write_failed;
use crate::ConvertError;

/// text made safe inside xml, attribute values included. characters xml
/// can't hold at all (most control characters) are left out.
pub(crate) fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

pub(crate) const XML_HEADER: &str =
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

/// a package being written next to its final place.
pub(crate) struct PackageFile {
    pub zip: ZipWriter<BufWriter<File>>,
    partial: PathBuf,
    target: PathBuf,
}

impl PackageFile {
    pub fn create(target: &Path) -> Result<Self, ConvertError> {
        let partial = target.with_extension("letters-partial");
        let file = File::create(&partial).map_err(write_failed)?;
        Ok(Self {
            zip: ZipWriter::new(BufWriter::new(file)),
            partial,
            target: target.to_path_buf(),
        })
    }

    /// finishes the zip and moves it over the target in one step.
    pub fn finish(self) -> Result<(), ConvertError> {
        let partial = self.partial.clone();
        let result = self
            .zip
            .finish()
            .and_then(|writer| {
                writer
                    .into_inner()
                    .map_err(|e| write_failed(e.into_error()))
            })
            .and_then(|file| file.sync_all().map_err(write_failed))
            .and_then(|_| std::fs::rename(&partial, &self.target).map_err(write_failed));
        if result.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        result
    }

    /// gives up: the half written file is deleted.
    pub fn abandon(self) {
        let partial = self.partial.clone();
        drop(self.zip);
        let _ = std::fs::remove_file(partial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_special_and_control_characters_are_handled() {
        assert_eq!(xml_escape("a<b & \"c\"\u{1}"), "a&lt;b &amp; &quot;c&quot;");
    }
}
