//! finding fonts installed on this computer.
//!
//! when an original font can't draw a character, the same font may be
//! installed on the computer. only each font file's name table is read
//! (the files can be tens of megabytes), and the list is made once per
//! run of the app.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// one font installed on the computer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InstalledFont {
    pub path: PathBuf,
    /// which font in a collection (.ttc); 0 for a single font
    pub index: u32,
    pub postscript_name: String,
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

/// the folders operating systems install fonts into.
fn font_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    if let Some(windows) = std::env::var_os("WINDIR") {
        folders.push(PathBuf::from(windows).join("Fonts"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        folders.push(
            PathBuf::from(local)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"),
        );
    }
    for folder in [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/local/share/fonts",
    ] {
        folders.push(PathBuf::from(folder));
    }
    if let Some(home) = home {
        folders.push(home.join("Library").join("Fonts"));
        folders.push(home.join(".fonts"));
        folders.push(home.join(".local").join("share").join("fonts"));
    }
    folders
}

/// every installed font, read once.
pub(crate) fn installed_fonts() -> &'static [InstalledFont] {
    static FONTS: OnceLock<Vec<InstalledFont>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut fonts = Vec::new();
        for folder in font_folders() {
            collect(&folder, 0, &mut fonts);
        }
        fonts
    })
}

fn collect(folder: &Path, depth: usize, fonts: &mut Vec<InstalledFont>) {
    // linux keeps fonts in nested folders; a few levels is plenty
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, depth + 1, fonts);
            continue;
        }
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "ttf" | "otf" | "ttc") {
            fonts.extend(read_font_names(&path).unwrap_or_default());
        }
    }
}

/// the installed font that is the same as a pdf font name such as
/// "ArialMT", "Arial-BoldMT" or "Arial,Bold".
pub(crate) fn find(pdf_name: &str) -> Option<&'static InstalledFont> {
    let fonts = installed_fonts();
    let squash = |text: &str| {
        text.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase()
    };
    let wanted = squash(pdf_name);
    if let Some(font) = fonts
        .iter()
        .find(|font| squash(&font.postscript_name) == wanted)
    {
        return Some(font);
    }
    // acrobat names TrueType styles "Family,Bold" or "Family,BoldItalic"
    let (family, style) = pdf_name.split_once(',')?;
    let style = style.to_ascii_lowercase();
    let (bold, italic) = (style.contains("bold"), style.contains("italic"));
    fonts.iter().find(|font| {
        squash(&font.family) == squash(family) && font.bold == bold && font.italic == italic
    })
}

fn read_bytes(file: &mut File, offset: u64, length: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// reads the names of every font in one file.
fn read_font_names(path: &Path) -> Option<Vec<InstalledFont>> {
    let mut file = File::open(path).ok()?;
    let header = read_bytes(&mut file, 0, 12)?;
    let starts: Vec<u32> = if header.starts_with(b"ttcf") {
        let count = u32_at(&header, 8)?.min(64);
        let offsets = read_bytes(&mut file, 12, count as usize * 4)?;
        (0..count as usize)
            .filter_map(|index| u32_at(&offsets, index * 4))
            .collect()
    } else {
        vec![0]
    };
    let mut fonts = Vec::new();
    for (index, start) in starts.into_iter().enumerate() {
        if let Some(font) = read_one_font(&mut file, path, index as u32, u64::from(start)) {
            fonts.push(font);
        }
    }
    Some(fonts)
}

fn read_one_font(file: &mut File, path: &Path, index: u32, start: u64) -> Option<InstalledFont> {
    let header = read_bytes(file, start, 12)?;
    let tables = usize::from(u16_at(&header, 4)?);
    let directory = read_bytes(file, start + 12, tables * 16)?;
    let record = (0..tables).find(|record| &directory[record * 16..record * 16 + 4] == b"name")?;
    let offset = u32_at(&directory, record * 16 + 8)?;
    let length = u32_at(&directory, record * 16 + 12)?.min(1 << 20);
    let table = read_bytes(file, u64::from(offset), length as usize)?;
    let name = |id: u16| name_from_table(&table, id);
    let subfamily = name(2).unwrap_or_default().to_ascii_lowercase();
    Some(InstalledFont {
        path: path.to_path_buf(),
        index,
        postscript_name: name(6)?,
        family: name(16).or_else(|| name(1)).unwrap_or_default(),
        bold: subfamily.contains("bold"),
        italic: subfamily.contains("italic") || subfamily.contains("oblique"),
    })
}

/// one name from a name table, preferring the windows unicode records.
fn name_from_table(table: &[u8], wanted: u16) -> Option<String> {
    let count = usize::from(u16_at(table, 2)?);
    let strings = usize::from(u16_at(table, 4)?);
    let mut mac = None;
    for record in 0..count {
        let at = 6 + record * 12;
        let (platform, encoding, id) = (
            u16_at(table, at)?,
            u16_at(table, at + 2)?,
            u16_at(table, at + 6)?,
        );
        if id != wanted {
            continue;
        }
        let length = usize::from(u16_at(table, at + 8)?);
        let offset = strings + usize::from(u16_at(table, at + 10)?);
        let bytes = table.get(offset..offset + length)?;
        match (platform, encoding) {
            (3, 1) | (0, _) => {
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes(*pair))
                    .collect();
                return Some(String::from_utf16_lossy(&units));
            }
            (1, 0) => mac = Some(bytes.iter().map(|byte| char::from(*byte)).collect()),
            _ => {}
        }
    }
    mac
}
