//! finding fonts installed on this computer.
//!
//! font folders can hold hundreds of files and gigabytes, so only each
//! file's table directory, name table and os/2 table are read, once per
//! run. the faces themselves are loaded when the layout first needs them.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// one font face found in a folder.
#[derive(Clone, Debug, PartialEq)]
pub struct InstalledFace {
    pub path: PathBuf,
    /// which face in a collection (.ttc); 0 for a single font
    pub index: u32,
    /// every family name the face answers to: the legacy family (name id
    /// 1, what word shows and documents store, "Calibri Light") and the
    /// typographic family (name id 16, "Calibri")
    pub families: Vec<String>,
    pub bold: bool,
    pub italic: bool,
    pub weight: u16,
}

/// office's download folder for its cloud fonts (aptos and others): word
/// finds fonts there, libreoffice does not.
pub fn office_cloud_folder() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|local| {
        PathBuf::from(local)
            .join("Microsoft")
            .join("FontCache")
            .join("4")
            .join("CloudFonts")
    })
}

/// the folders operating systems (and office's cloud fonts) keep fonts in.
pub fn font_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(windows) = std::env::var_os("WINDIR") {
        folders.push(PathBuf::from(windows).join("Fonts"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        folders.push(local.join("Microsoft").join("Windows").join("Fonts"));
        // office downloads its newer fonts (aptos) here and word finds them
        folders.push(
            local
                .join("Microsoft")
                .join("FontCache")
                .join("4")
                .join("CloudFonts"),
        );
    }
    for folder in [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
    ] {
        folders.push(PathBuf::from(folder));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        folders.push(home.join("Library").join("Fonts"));
        folders.push(home.join(".local").join("share").join("fonts"));
    }
    folders
}

/// every installed face, scanned once per run.
pub fn installed_faces() -> &'static [InstalledFace] {
    static FACES: OnceLock<Vec<InstalledFace>> = OnceLock::new();
    FACES.get_or_init(|| {
        let mut faces = Vec::new();
        for folder in font_folders() {
            collect(&folder, 0, &mut faces);
        }
        faces
    })
}

fn collect(folder: &Path, depth: usize, faces: &mut Vec<InstalledFace>) {
    // linux and the office cache nest folders; a few levels is plenty
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, depth + 1, faces);
            continue;
        }
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "ttf" | "otf" | "ttc") {
            faces.extend(read_faces(&path).unwrap_or_default());
        }
    }
}

fn read_bytes(file: &mut File, offset: u64, length: usize) -> Option<Vec<u8>> {
    // a damaged directory could ask for absurd lengths
    if length > 16 << 20 {
        return None;
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

pub(crate) fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

pub(crate) fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn read_faces(path: &Path) -> Option<Vec<InstalledFace>> {
    let mut file = File::open(path).ok()?;
    let header = read_bytes(&mut file, 0, 12)?;
    let starts: Vec<u32> = if header.starts_with(b"ttcf") {
        let count = u32_at(&header, 8)?.min(64);
        let offsets = read_bytes(&mut file, 12, 4 * count as usize)?;
        (0..count as usize)
            .filter_map(|i| u32_at(&offsets, 4 * i))
            .collect()
    } else {
        vec![0]
    };
    let faces = starts
        .iter()
        .enumerate()
        .filter_map(|(index, start)| read_face(&mut file, path, index as u32, u64::from(*start)))
        .collect();
    Some(faces)
}

fn read_face(file: &mut File, path: &Path, index: u32, start: u64) -> Option<InstalledFace> {
    let directory = read_bytes(file, start, 12)?;
    let count = usize::from(u16_at(&directory, 4)?).min(64);
    let records = read_bytes(file, start + 12, 16 * count)?;
    let mut name = None;
    let mut os2 = None;
    for record in 0..count {
        let at = 16 * record;
        let tag = &records[at..at + 4];
        let offset = u64::from(u32_at(&records, at + 8)?);
        let length = u32_at(&records, at + 12)? as usize;
        match tag {
            b"name" => name = read_bytes(file, offset, length),
            b"OS/2" => os2 = read_bytes(file, offset, length),
            _ => {}
        }
    }
    let name = name?;
    let mut families = Vec::new();
    for id in [1u16, 16, 21] {
        if let Some(family) = name_string(&name, id) {
            if !families.contains(&family) {
                families.push(family);
            }
        }
    }
    if families.is_empty() {
        return None;
    }
    let (weight, selection) = match &os2 {
        Some(table) => (
            u16_at(table, 4).unwrap_or(400),
            u16_at(table, 62).unwrap_or(0),
        ),
        None => (400, 0),
    };
    Some(InstalledFace {
        path: path.to_path_buf(),
        index,
        families,
        // fsSelection bit 5 is bold, bit 0 italic (opentype os/2 spec)
        bold: selection & 0x20 != 0 || weight >= 600,
        italic: selection & 0x01 != 0,
        weight,
    })
}

/// an english (or any windows unicode) entry of the name table.
pub(crate) fn name_string(table: &[u8], wanted: u16) -> Option<String> {
    let count = usize::from(u16_at(table, 2)?);
    let strings = usize::from(u16_at(table, 4)?);
    let mut fallback = None;
    for record in 0..count {
        let at = 6 + 12 * record;
        let platform = u16_at(table, at)?;
        let language = u16_at(table, at + 4)?;
        let id = u16_at(table, at + 6)?;
        if id != wanted {
            continue;
        }
        let length = usize::from(u16_at(table, at + 8)?);
        let offset = strings + usize::from(u16_at(table, at + 10)?);
        let bytes = table.get(offset..offset + length)?;
        let text = match platform {
            0 | 3 => {
                let (pairs, _) = bytes.as_chunks::<2>();
                let units: Vec<u16> = pairs.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
                String::from_utf16_lossy(&units)
            }
            1 => bytes.iter().map(|&b| char::from(b)).collect(),
            _ => continue,
        };
        // us english first, as word matches on it
        if platform == 3 && language == 0x0409 {
            return Some(text);
        }
        fallback.get_or_insert(text);
    }
    fallback
}
