//! the fonts that may lend glyphs: every face on this computer that the
//! index knows (system, added, bundled, downloaded), or any files a
//! caller hands over. each donor remembers its licence (fsType), and a
//! donor whose licence forbids copying glyphs out of it is never used.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::container::{display_name, read_limited, to_plain_font, MAX_SYSTEM_FONT_BYTES};
use crate::error::FontError;
use crate::facts::{facts_of, ASCII_SAMPLE};
use crate::index::{family_key, FontIndex};
use crate::model::{
    Axis, Category, Embedding, EmbeddingPermission, FaceRecord, Measurements, Outlines, Source,
};

/// one font that may lend glyphs.
#[derive(Clone, Debug)]
pub struct DonorFile {
    pub family: String,
    pub style: String,
    pub postscript_name: String,
    pub source: Source,
    /// stable name for caches and reports (the index's face key)
    pub key: String,
    pub path: Option<PathBuf>,
    pub index: u32,
    pub embedding: Embedding,
    pub measurements: Option<Measurements>,
    pub category: Category,
    pub axes: Vec<Axis>,
    pub weight: u16,
    pub italic: bool,
    /// the characters it maps, as ranges of code points
    pub coverage: Vec<(u32, u32)>,
    /// the plain font bytes, read on first use
    bytes: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
}

impl DonorFile {
    /// a donor for a face the index found.
    pub fn from_record(record: &FaceRecord) -> DonorFile {
        let facts = &record.facts;
        DonorFile {
            family: facts.family.clone(),
            style: facts.style_name.clone(),
            postscript_name: facts.postscript_name.clone(),
            source: record.source,
            key: record.key.clone(),
            path: Some(record.path.clone()),
            index: record.index,
            embedding: facts.embedding.clone(),
            measurements: Some(facts.measurements.clone()),
            category: facts.category,
            axes: facts.axes.clone(),
            weight: facts.weight,
            italic: facts.italic || facts.oblique,
            coverage: facts.coverage.clone(),
            bytes: Arc::new(Mutex::new(None)),
        }
    }

    /// whether the font maps the character at all.
    pub fn can_draw(&self, c: char) -> bool {
        crate::coverage::ranges_contain(&self.coverage, c)
    }

    /// a donor from font bytes already in memory (tests, the harness, a
    /// font fetched for a preview). None when the bytes aren't a font.
    pub fn from_bytes(source: Source, key: &str, bytes: Vec<u8>, index: u32) -> Option<DonorFile> {
        let plain = to_plain_font(bytes, key).ok()?;
        let face = ttf_parser::Face::parse(&plain, index).ok()?;
        let facts = facts_of(&face)?;
        if facts.outlines == Outlines::Bitmap {
            return None;
        }
        Some(DonorFile {
            family: facts.family.clone(),
            style: facts.style_name.clone(),
            postscript_name: facts.postscript_name.clone(),
            source,
            key: key.to_string(),
            path: None,
            index,
            embedding: facts.embedding.clone(),
            measurements: Some(facts.measurements.clone()),
            category: facts.category,
            axes: facts.axes.clone(),
            weight: facts.weight,
            italic: facts.italic || facts.oblique,
            coverage: facts.coverage.clone(),
            bytes: Arc::new(Mutex::new(Some(Arc::new(plain)))),
        })
    }

    /// "Arial Bold" for messages.
    pub fn label(&self) -> String {
        if self.style.eq_ignore_ascii_case("regular") || self.style.is_empty() {
            self.family.clone()
        } else {
            format!("{} {}", self.family, self.style)
        }
    }

    /// the plain font bytes, read from disk (with the size limit) the
    /// first time they are needed.
    pub fn bytes(&self) -> Result<Arc<Vec<u8>>, FontError> {
        let mut slot = self
            .bytes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(bytes) = slot.as_ref() {
            return Ok(bytes.clone());
        }
        let path = self.path.as_ref().ok_or(FontError::UnknownFont)?;
        let plain = to_plain_font(
            read_limited(path, MAX_SYSTEM_FONT_BYTES)?,
            &display_name(path),
        )?;
        let shared = Arc::new(plain);
        *slot = Some(shared.clone());
        Ok(shared)
    }

    /// whether glyphs may be copied out of this font, and if not, why.
    pub fn permission(&self) -> Result<(), String> {
        donor_permission(&self.embedding, &self.label())
    }
}

/// glyphs may be copied out of a font only when its licence allows it to
/// be embedded with editing, and allows subsetting (copying some glyphs
/// is subsetting). print-and-preview and restricted fonts are refused.
pub fn donor_permission(embedding: &Embedding, label: &str) -> Result<(), String> {
    let allowed = matches!(
        embedding.permission,
        EmbeddingPermission::Installable | EmbeddingPermission::Editable
    ) && embedding.subsetting
        && !embedding.bitmap_only;
    if allowed {
        Ok(())
    } else if !embedding.allows_embedding() {
        Err(format!(
            "{label} wasn’t used: its license doesn’t allow embedding."
        ))
    } else if !embedding.subsetting {
        Err(format!(
            "{label} wasn’t used: its license allows only the whole font to be embedded, not some of its letters."
        ))
    } else {
        Err(format!(
            "{label} wasn’t used: its license allows embedding for reading and printing only, not for editing."
        ))
    }
}

/// the fonts that may lend glyphs.
#[derive(Clone, Debug, Default)]
pub struct DonorPool {
    pub donors: Vec<DonorFile>,
}

impl DonorPool {
    /// every face on this computer that the index knows, with outlines.
    pub fn from_index(index: &FontIndex) -> DonorPool {
        let donors = index
            .families
            .iter()
            .flat_map(|family| &family.faces)
            .filter(|face| face.facts.outlines != Outlines::Bitmap)
            .map(DonorFile::from_record)
            .collect();
        DonorPool { donors }
    }

    pub fn push(&mut self, donor: DonorFile) {
        self.donors.push(donor);
    }

    /// the pool without the families named (for "the font isn't here").
    pub fn without_families(&self, names: &[&str]) -> DonorPool {
        let keys: Vec<String> = names.iter().map(|name| family_key(name)).collect();
        DonorPool {
            donors: self
                .donors
                .iter()
                .filter(|donor| !keys.contains(&family_key(&donor.family)))
                .cloned()
                .collect(),
        }
    }

    /// the pool with only the families named.
    pub fn only_families(&self, names: &[&str]) -> DonorPool {
        let keys: Vec<String> = names.iter().map(|name| family_key(name)).collect();
        DonorPool {
            donors: self
                .donors
                .iter()
                .filter(|donor| keys.contains(&family_key(&donor.family)))
                .cloned()
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.donors.is_empty()
    }
}

/// what is quickly known about the font being matched, to choose which
/// donors are worth reading glyph by glyph.
#[derive(Clone, Debug, Default)]
pub struct TargetProfile {
    /// advances of printable ascii characters present, thousandths of the em
    pub ascii_widths: Vec<(char, f64)>,
    pub x_height: f64,
    pub stem_of_o: Option<f64>,
    pub italic: bool,
    pub serif: Option<bool>,
    /// the family the document names, without any subset tag
    pub name_hint: Option<String>,
}

/// "ABCDEF+Arial-BoldMT" or "Arial,Bold" to the family words "arial".
pub fn family_from_font_name(name: &str) -> String {
    let without_tag = match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.chars().all(|c| c.is_ascii_uppercase()) => rest,
        _ => name,
    };
    let base = without_tag
        .split([',', '-'])
        .next()
        .unwrap_or(without_tag)
        .trim();
    // producers add "MT" and "PS" (and "PSMT") to postscript names
    let mut base = base;
    while let Some(shorter) = base
        .strip_suffix("MT")
        .or_else(|| base.strip_suffix("PS"))
        .filter(|shorter| !shorter.is_empty())
    {
        base = shorter;
    }
    // "TimesNewRoman" to "Times New Roman"
    let mut spaced = String::new();
    for (index, c) in base.chars().enumerate() {
        let previous = base.chars().nth(index.wrapping_sub(1));
        if index > 0 && c.is_uppercase() && previous.is_some_and(|p| p.is_lowercase()) {
            spaced.push(' ');
        }
        spaced.push(c);
    }
    spaced.trim().to_string()
}

/// how unlike the target a donor looks from its stored facts alone:
/// widths, x-height, stroke and slant. smaller is closer.
pub fn quick_distance(target: &TargetProfile, donor: &DonorFile) -> f64 {
    let Some(measurements) = &donor.measurements else {
        return 50.0;
    };
    let mut width_total = 0.0;
    let mut width_count = 0usize;
    for (c, width) in &target.ascii_widths {
        let Some(at) = ASCII_SAMPLE.clone().position(|sample| sample == *c) else {
            continue;
        };
        let theirs = f64::from(measurements.ascii_widths.get(at).copied().unwrap_or(0));
        if theirs > 0.0 {
            width_total += (width - theirs).abs();
            width_count += 1;
        }
    }
    let width = if width_count == 0 {
        8.0
    } else {
        width_total / width_count as f64 / 15.0
    };
    let x_height = (target.x_height * 1000.0 - f64::from(measurements.x_height)).abs() / 30.0;
    let stem = target
        .stem_of_o
        .map(|stem| (stem * 1000.0 - f64::from(measurements.stem)).abs() / 20.0)
        .unwrap_or(0.0);
    let slant = if target.italic != donor.italic {
        3.0
    } else {
        0.0
    };
    let serif = match target.serif {
        Some(true) if donor.category == Category::SansSerif => 2.0,
        Some(false) if donor.category == Category::Serif => 2.0,
        _ => 0.0,
    };
    let unusable = if matches!(donor.category, Category::Symbol) {
        20.0
    } else {
        0.0
    };
    width + x_height + stem + slant + serif + unusable
}

/// true when the donor's family is the one the document names, or its
/// known metric twin.
pub fn named_by_hint(hint: Option<&str>, donor: &DonorFile) -> bool {
    let Some(hint) = hint else { return false };
    let hint_key = family_key(hint).replace(' ', "");
    let donor_key = family_key(&donor.family).replace(' ', "");
    let postscript = family_key(&family_from_font_name(&donor.postscript_name)).replace(' ', "");
    hint_key == donor_key || hint_key == postscript
}

/// the donors worth comparing glyph by glyph: those the document names
/// first, then the closest by their facts, at most `limit`.
pub fn shortlist<'a>(
    target: &TargetProfile,
    pool: &'a DonorPool,
    limit: usize,
) -> Vec<&'a DonorFile> {
    let hint = target.name_hint.as_deref();
    let twin = hint.and_then(crate::substitute::substitute_for);
    let mut ranked: Vec<(f64, &DonorFile)> = pool
        .donors
        .iter()
        .map(|donor| {
            let mut score = quick_distance(target, donor);
            if named_by_hint(hint, donor) {
                score -= 100.0;
            } else if twin.is_some_and(|twin| family_key(twin.with) == family_key(&donor.family)) {
                score -= 4.0;
            }
            (score, donor)
        })
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.key.cmp(&b.1.key)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, donor)| donor)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_font_names_become_families() {
        assert_eq!(family_from_font_name("ABCDEF+Arial-BoldMT"), "Arial");
        assert_eq!(
            family_from_font_name("TimesNewRomanPSMT"),
            "Times New Roman"
        );
        assert_eq!(family_from_font_name("Calibri,Bold"), "Calibri");
        assert_eq!(family_from_font_name("abc+Odd"), "abc+Odd");
    }

    #[test]
    fn only_editable_licences_lend_glyphs() {
        assert!(donor_permission(&Embedding::from_fs_type(0), "A").is_ok());
        assert!(donor_permission(&Embedding::from_fs_type(0x8), "A").is_ok());
        let restricted = donor_permission(&Embedding::from_fs_type(0x2), "Corp Sans").unwrap_err();
        assert_eq!(
            restricted,
            "Corp Sans wasn’t used: its license doesn’t allow embedding."
        );
        assert!(donor_permission(&Embedding::from_fs_type(0x4), "A")
            .unwrap_err()
            .contains("reading and printing"));
        assert!(donor_permission(&Embedding::from_fs_type(0x0108), "A")
            .unwrap_err()
            .contains("whole font"));
    }
}
