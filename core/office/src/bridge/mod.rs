//! asking installed applications for what letters cannot do itself:
//! microsoft word's own page layout, an exact print-ready pdf, and
//! conversions to and from the old .doc format. libreoffice, when it is
//! there, is the fallback for the pdf and for .doc.
//!
//! word is driven through com by a small powershell script (windows
//! only). it always runs a private, hidden word and stops only that one.
//! libreoffice runs headless with a profile folder of its own, so it never
//! touches the person's settings, and is always waited for.

use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

mod batch;

pub use batch::{
    libreoffice_convert_files, word_batch, WordJob, WordJobKind, WORD_FORMAT_DOCX, WORD_FORMAT_ODT,
};

const SESSION_SCRIPT: &str = include_str!("word_session.ps1");
const BRIDGE_SCRIPT: &str = include_str!("word_bridge.ps1");

/// why a bridge call did not work.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BridgeError {
    #[error("Word isn’t installed on this computer, so its exact layout isn’t available.")]
    WordNotInstalled,
    #[error("Neither Word nor LibreOffice is installed, so this file can’t be converted.")]
    NoConverter,
    #[error("Word or LibreOffice couldn’t finish the job. ({0})")]
    Failed(String),
}

/// which application did the work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    Word,
    LibreOffice,
}

/// true when microsoft word is registered for automation.
pub fn word_available() -> bool {
    if !cfg!(windows) {
        return false;
    }
    Command::new("reg")
        .args(["query", r"HKCR\Word.Application\CurVer"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// where libreoffice's command line program is: `LETTERS_SOFFICE`, the
/// usual install folders, or a copy unpacked into the user's programs.
pub fn libreoffice_path() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(path) = std::env::var("LETTERS_SOFFICE") {
        candidates.push(path.into());
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(Path::new(&local).join(r"Programs\LibreOffice\program\soffice.com"));
    }
    candidates.push(r"C:\Program Files\LibreOffice\program\soffice.com".into());
    candidates.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into());
    candidates.push("/usr/bin/soffice".into());
    candidates.into_iter().find(|path| path.is_file())
}

/// a folder for one bridge job, removed when dropped.
struct Workspace {
    path: PathBuf,
}

impl Workspace {
    fn new() -> std::io::Result<Workspace> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let unique = format!("letters-bridge-{}-{count}-{nanos}", std::process::id());
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path)?;
        Ok(Workspace { path })
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn failed(error: impl std::fmt::Display) -> BridgeError {
    BridgeError::Failed(error.to_string())
}

/// runs the word script in `workspace` and waits for it.
fn run_word(workspace: &Workspace, arguments: &[&str]) -> Result<(), BridgeError> {
    if !word_available() {
        return Err(BridgeError::WordNotInstalled);
    }
    std::fs::write(workspace.path.join("word_session.ps1"), SESSION_SCRIPT).map_err(failed)?;
    let script = workspace.path.join("word_bridge.ps1");
    std::fs::write(&script, BRIDGE_SCRIPT).map_err(failed)?;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .args(arguments)
        .output()
        .map_err(failed)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(BridgeError::Failed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

/// runs libreoffice headless to convert `input` into `format` in the
/// workspace, waiting at most two minutes, and returns the new file.
fn run_libreoffice(
    workspace: &Workspace,
    input: &Path,
    format: &str,
    extension: &str,
) -> Result<PathBuf, BridgeError> {
    let out_dir = workspace.path.join("out");
    std::fs::create_dir_all(&out_dir).map_err(failed)?;
    run_libreoffice_files(
        &[input.to_path_buf()],
        format,
        &out_dir,
        Duration::from_secs(120),
    )?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let produced = out_dir.join(format!("{stem}.{extension}"));
    if produced.is_file() {
        Ok(produced)
    } else {
        Err(BridgeError::Failed(format!(
            "LibreOffice did not write a .{extension} file"
        )))
    }
}

/// runs one headless libreoffice that converts every file in `inputs`
/// into `out_dir`, waiting at most `limit`.
fn run_libreoffice_files(
    inputs: &[PathBuf],
    format: &str,
    out_dir: &Path,
    limit: Duration,
) -> Result<(), BridgeError> {
    let program = libreoffice_path().ok_or(BridgeError::NoConverter)?;
    // one private profile, kept between calls: making a fresh one costs
    // libreoffice about twenty seconds. a profile serves one libreoffice at
    // a time, so calls take turns
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let profile = std::env::temp_dir().join(format!(
        "letters-libreoffice-profile-{}",
        std::process::id()
    ));
    let profile_url = format!(
        "file:///{}",
        profile
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );
    let mut child = Command::new(&program)
        .arg("--headless")
        .arg("--norestore")
        .arg("--nolockcheck")
        .arg(format!("-env:UserInstallation={profile_url}"))
        .args(["--convert-to", format, "--outdir"])
        .arg(out_dir)
        .args(inputs)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(failed)?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(failed)? {
            if !status.success() {
                return Err(BridgeError::Failed(format!(
                    "LibreOffice ended with {status}"
                )));
            }
            return Ok(());
        }
        if started.elapsed() > limit {
            // end only the process tree this call started
            if cfg!(windows) {
                let _ = Command::new("taskkill")
                    .args(["/T", "/F", "/PID", &child.id().to_string()])
                    .output();
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(BridgeError::Failed(format!(
                "LibreOffice took longer than {} seconds",
                limit.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// a stable fingerprint of a document's bytes (fnv-1a, 64 bit), used to
/// name cached layouts.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = Fnv(0xcbf2_9ce4_8422_2325);
    hasher.write(bytes);
    hasher.finish()
}

struct Fnv(u64);

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// one line as word laid it out. positions are twips from the page's
/// top-left corner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutLine {
    pub top: i64,
    pub left: i64,
    pub height: i64,
    pub width: i64,
    /// word's story: 1 is the main text, others are headers, notes, boxes
    pub story: i64,
    /// the line's paragraph in word's order (see `word_paragraph_map`);
    /// -1 outside the main text
    pub paragraph: i64,
    /// character positions in word's main text
    pub start: i64,
    pub end: i64,
    /// the first characters of the line, to check the match
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPage {
    pub number: i64,
    pub width: i64,
    pub height: i64,
    #[serde(default, deserialize_with = "one_or_many")]
    pub lines: Vec<LayoutLine>,
}

/// where word put a floating object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutShape {
    pub name: String,
    pub page: i64,
    pub left: i64,
    pub top: i64,
    pub width: i64,
    pub height: i64,
    /// word's codes: 0 margin, 1 page, 2 column, 3 character (horizontal);
    /// 0 margin, 1 page, 2 paragraph, 3 line (vertical)
    pub relative_horizontal: i64,
    pub relative_vertical: i64,
    #[serde(default)]
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordLayout {
    #[serde(deserialize_with = "one_or_many")]
    pub pages: Vec<LayoutPage>,
    #[serde(default, deserialize_with = "one_or_many")]
    pub shapes: Vec<LayoutShape>,
    /// word's own page count statistic
    pub word_pages: i64,
    /// how long word took, without starting it
    pub milliseconds: i64,
    /// the document fingerprint the layout belongs to
    #[serde(default)]
    pub fingerprint: String,
}

/// powershell writes a list of one as a lone object; accept both.
fn one_or_many<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|item| serde_json::from_value(item).map_err(serde::de::Error::custom))
            .collect(),
        single => Ok(vec![
            serde_json::from_value(single).map_err(serde::de::Error::custom)?
        ]),
    }
}

/// word's own layout of a docx: every page, every line and every floating
/// object. with `cache`, a layout already worked out for the same bytes
/// is read from there instead of asking word again.
pub fn word_layout(docx: &[u8], cache: Option<&Path>) -> Result<WordLayout, BridgeError> {
    let key = format!("{:016x}", fingerprint(docx));
    let cached = cache.map(|folder| folder.join(format!("word-layout-{key}.json")));
    if let Some(path) = &cached {
        if let Some(layout) = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
        {
            return Ok(layout);
        }
    }
    let workspace = Workspace::new().map_err(failed)?;
    let input = workspace.path.join("document.docx");
    let output = workspace.path.join("layout.json");
    std::fs::write(&input, docx).map_err(failed)?;
    run_word(
        &workspace,
        &[
            "-Mode",
            "layout",
            "-Path",
            &input.to_string_lossy(),
            "-Out",
            &output.to_string_lossy(),
        ],
    )?;
    let text = std::fs::read_to_string(&output).map_err(failed)?;
    let mut layout: WordLayout =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(failed)?;
    layout.fingerprint = key;
    if let Some(path) = cached {
        if let Ok(json) = serde_json::to_string(&layout) {
            let _ = std::fs::write(path, json);
        }
    }
    Ok(layout)
}

/// a print-ready pdf and how it was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintOutput {
    pub pdf: Vec<u8>,
    pub engine: Engine,
    /// true when word made it, so it matches word page for page
    pub exact: bool,
    /// a plain sentence when the pdf is not exact
    pub note: Option<String>,
}

/// a pdf of the document for printing: through word when it is there
/// (exact), else through libreoffice (close, not exact).
pub fn print_pdf(docx: &[u8]) -> Result<PrintOutput, BridgeError> {
    let workspace = Workspace::new().map_err(failed)?;
    let input = workspace.path.join("document.docx");
    std::fs::write(&input, docx).map_err(failed)?;
    let output = workspace.path.join("document.pdf");
    let through_word = run_word(
        &workspace,
        &[
            "-Mode",
            "pdf",
            "-Path",
            &input.to_string_lossy(),
            "-Out",
            &output.to_string_lossy(),
        ],
    );
    match through_word {
        Ok(()) => Ok(PrintOutput {
            pdf: std::fs::read(&output).map_err(failed)?,
            engine: Engine::Word,
            exact: true,
            note: None,
        }),
        Err(word_error) => {
            let produced =
                run_libreoffice(&workspace, &input, "pdf", "pdf").map_err(|error| {
                    match (&word_error, error) {
                        (BridgeError::WordNotInstalled, BridgeError::NoConverter) => {
                            BridgeError::NoConverter
                        }
                        (_, other) => other,
                    }
                })?;
            Ok(PrintOutput {
                pdf: std::fs::read(produced).map_err(failed)?,
                engine: Engine::LibreOffice,
                exact: false,
                note: Some(format!(
                    "Printed through LibreOffice because Word wasn’t available ({word_error}). Line and page breaks can differ slightly from Word."
                )),
            })
        }
    }
}

/// word's save formats used here.
const WORD_DOCX: &str = "16";
const WORD_DOC: &str = "0";

/// converts between .doc and .docx (`to_doc` picks the direction) through
/// word, or libreoffice when word is missing.
pub fn convert_word_format(bytes: &[u8], to_doc: bool) -> Result<(Vec<u8>, Engine), BridgeError> {
    let workspace = Workspace::new().map_err(failed)?;
    let (from, to, code) = if to_doc {
        ("docx", "doc", WORD_DOC)
    } else {
        ("doc", "docx", WORD_DOCX)
    };
    let input = workspace.path.join(format!("document.{from}"));
    let output = workspace.path.join(format!("converted.{to}"));
    std::fs::write(&input, bytes).map_err(failed)?;
    match run_word(
        &workspace,
        &[
            "-Mode",
            "convert",
            "-Path",
            &input.to_string_lossy(),
            "-Out",
            &output.to_string_lossy(),
            "-Format",
            code,
        ],
    ) {
        Ok(()) => Ok((std::fs::read(&output).map_err(failed)?, Engine::Word)),
        Err(_) => {
            let filter = if to_doc {
                "doc:MS Word 97"
            } else {
                "docx:MS Word 2007 XML"
            };
            let produced = run_libreoffice(&workspace, &input, filter, to)?;
            Ok((
                std::fs::read(produced).map_err(failed)?,
                Engine::LibreOffice,
            ))
        }
    }
}

/// converts with libreoffice into `format` (its --convert-to name, such
/// as "pdf", "odt" or "docx") and returns the bytes. tests use it to
/// check that libreoffice accepts our files.
pub fn libreoffice_convert(
    bytes: &[u8],
    extension: &str,
    format: &str,
    out_extension: &str,
) -> Result<Vec<u8>, BridgeError> {
    let workspace = Workspace::new().map_err(failed)?;
    let input = workspace.path.join(format!("document.{extension}"));
    std::fs::write(&input, bytes).map_err(failed)?;
    let produced = run_libreoffice(&workspace, &input, format, out_extension)?;
    std::fs::read(produced).map_err(failed)
}

/// where a paragraph of the model sits in word's list of paragraphs, so
/// a `LayoutLine::paragraph` can be traced back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordParagraph {
    /// the top-level block
    pub block: usize,
    /// inside a table: row, cell and block within the cell
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<(usize, usize, usize)>,
}

/// the model's paragraphs in word's order. word also counts the mark
/// that ends each table row; those entries are `None`. text boxes are not
/// in the main text and are left out.
pub fn word_paragraph_map(document: &crate::model::Document) -> Vec<Option<WordParagraph>> {
    use crate::model::Block;
    let mut map = Vec::new();
    for (index, block) in document.blocks.iter().enumerate() {
        match block {
            Block::Paragraph(_)
            | Block::Heading(_)
            | Block::PageBreak
            | Block::Preserved { .. } => map.push(Some(WordParagraph {
                block: index,
                cell: None,
            })),
            Block::Table(table) => {
                for (row_index, row) in table.rows.iter().enumerate() {
                    for (cell_index, cell) in row.cells.iter().enumerate() {
                        let count = cell.blocks.len().max(1);
                        for inner in 0..count {
                            map.push(Some(WordParagraph {
                                block: index,
                                cell: Some((row_index, cell_index, inner)),
                            }));
                        }
                    }
                    map.push(None);
                }
            }
            Block::SectionBreak => {}
            Block::TextBox(_) | Block::Shape(_) | Block::Group(_) => {
                // a drawing sits in an anchor paragraph of its own, unless
                // it shares one with the drawing before it or with the
                // paragraph of text after it
                let own = block
                    .object_anchor()
                    .is_none_or(|anchor| anchor.with == crate::model::AnchorWith::Own);
                if own {
                    map.push(Some(WordParagraph {
                        block: index,
                        cell: None,
                    }));
                }
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_are_stable() {
        assert_eq!(fingerprint(b""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(fingerprint(b"a"), fingerprint(b"b"));
    }

    #[test]
    fn layout_json_with_single_items() {
        let json = r#"{"pages":{"number":1,"width":12240,"height":15840,"lines":{"top":1440,"left":1440,"height":276,"width":100,"story":1,"paragraph":0,"start":0,"end":5,"text":"Hello"}},"shapes":null,"wordPages":1,"milliseconds":5}"#;
        let layout: WordLayout = serde_json::from_str(json).unwrap();
        assert_eq!(layout.pages[0].lines[0].text, "Hello");
        assert!(layout.shapes.is_empty());
    }
}
