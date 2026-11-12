//! pdf to html, in the two ways the export dialog offers:
//! - exact match (`fixed`): each page is a box of the page's size, its
//!   text real, selectable spans placed where the pdf draws them, over a
//!   picture of everything that isn't text
//! - easy to edit (`semantic`): headings, paragraphs, lists, tables,
//!   figures and links as plain, accessible html that reflows
//!
//! either can be one self-contained file (pictures and fonts inside it
//! as data addresses) or a page with a "<name>_files" folder beside it.

mod fixed;
mod fonts;
mod semantic;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use letters_ocr::{CancelFlag, Progress};
use serde::{Deserialize, Serialize};

use crate::error::write_failed;
use crate::naming::{create_unique_file, safe_base_name};
use crate::render::PageRenderer;
use crate::source::{PageSelection, PdfSource};
use crate::ConvertError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HtmlMode {
    /// every page looks like the pdf; text is placed exactly
    #[default]
    ExactMatch,
    /// clean html that flows: headings, paragraphs, lists, tables
    EasyToEdit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HtmlExportOptions {
    pub mode: HtmlMode,
    pub pages: PageSelection,
    /// one file with pictures and fonts inside, instead of a folder beside it
    pub self_contained: bool,
    /// exact match: draw what isn't text as a picture behind the text
    pub background: bool,
    /// exact match: the resolution of that picture
    pub background_dpi: f32,
    /// exact match: use the pdf's own fonts where they allow it
    pub embed_fonts: bool,
    /// easy to edit: rejoin words split at line ends
    pub dehyphenate: bool,
}

impl Default for HtmlExportOptions {
    fn default() -> Self {
        Self {
            mode: HtmlMode::ExactMatch,
            pages: PageSelection::All,
            self_contained: true,
            background: true,
            background_dpi: 150.0,
            embed_fonts: true,
            dehyphenate: true,
        }
    }
}

/// what an html export made.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HtmlReport {
    /// the page itself, then any pictures and fonts beside it
    pub files: Vec<PathBuf>,
    /// fonts from the pdf the page uses
    pub fonts_used: Vec<String>,
    /// fonts that were left out, and why, in a sentence each
    pub font_notes: Vec<String>,
    /// pages drawn as a background picture
    pub pages_with_background: Vec<usize>,
}

/// writes the chosen pages as html to `path` (replacing it: the save
/// panel already asked). pictures and fonts go inside the file, or into
/// a "<name>_files" folder beside it, where nothing is ever replaced.
pub fn pdf_to_html(
    source: &PdfSource,
    renderer: &mut dyn PageRenderer,
    options: &HtmlExportOptions,
    path: &Path,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<HtmlReport, ConvertError> {
    if options.mode == HtmlMode::ExactMatch && !(30.0..=600.0).contains(&options.background_dpi) {
        return Err(ConvertError::InvalidSetting(
            "Choose a background resolution between 30 and 600 dpi.".into(),
        ));
    }
    let partial = path.with_extension("html.letters-partial");
    let mut assets = Assets::new(path, options.self_contained);
    let mut report = HtmlReport::default();
    let result = (|| {
        let file = File::create(&partial).map_err(write_failed)?;
        let mut out = BufWriter::new(file);
        match options.mode {
            HtmlMode::ExactMatch => fixed::write(
                source,
                renderer,
                options,
                &mut out,
                &mut assets,
                &mut report,
                progress,
                cancel,
            )?,
            HtmlMode::EasyToEdit => {
                semantic::write(source, options, &mut out, &mut assets, progress, cancel)?
            }
        }
        out.flush().map_err(write_failed)?;
        drop(out);
        std::fs::rename(&partial, path).map_err(write_failed)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        assets.remove_all();
        return Err(error);
    }
    report.files.push(path.to_path_buf());
    report.files.extend(assets.written);
    Ok(report)
}

/// pictures and fonts: inside the page as data addresses, or as files in
/// a folder beside it.
pub(crate) struct Assets {
    self_contained: bool,
    folder: PathBuf,
    folder_name: String,
    written: Vec<PathBuf>,
}

impl Assets {
    fn new(html: &Path, self_contained: bool) -> Self {
        let stem = html
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "page".into());
        let folder_name = format!("{}_files", safe_base_name(&stem));
        let folder = html.parent().unwrap_or(Path::new(".")).join(&folder_name);
        Self {
            self_contained,
            folder,
            folder_name,
            written: Vec::new(),
        }
    }

    /// stores one picture or font and returns the address to use for it.
    pub fn add(&mut self, name: &str, mime: &str, bytes: &[u8]) -> Result<String, ConvertError> {
        if self.self_contained {
            return Ok(format!("data:{mime};base64,{}", base64(bytes)));
        }
        std::fs::create_dir_all(&self.folder).map_err(write_failed)?;
        let (mut file, path) = create_unique_file(&self.folder, name)?;
        file.write_all(bytes).map_err(write_failed)?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.written.push(path);
        Ok(format!(
            "{}/{}",
            url_part(&self.folder_name),
            url_part(&file_name)
        ))
    }

    fn remove_all(&mut self) {
        for file in self.written.drain(..) {
            let _ = std::fs::remove_file(file);
        }
        let _ = std::fs::remove_dir(&self.folder);
    }
}

/// a file or folder name made safe inside an address: anything but
/// letters, digits and - _ . is written as %xx.
fn url_part(name: &str) -> String {
    name.bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// text made safe inside html, attribute values included.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// standard base64, as data addresses need.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].iter().enumerate() {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// a number for css, without needless digits ("12.5", "72").
pub(crate) fn number(value: f32) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == rounded.trunc() {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

pub(crate) fn write_str(out: &mut impl Write, text: &str) -> Result<(), ConvertError> {
    out.write_all(text.as_bytes()).map_err(write_failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(escape("a < b & \"c\""), "a &lt; b &amp; &quot;c&quot;");
        assert_eq!(url_part("My notes.png"), "My%20notes.png");
        assert_eq!(number(12.0), "12");
        assert_eq!(number(12.345), "12.35");
    }
}
