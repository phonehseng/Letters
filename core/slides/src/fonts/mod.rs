//! fonts: finding them, loading them, and reporting substitutions. copied
//! from letters-layout's font module (see WORKLOG) so slides and documents
//! substitute fonts by the same rules.
//!
//! the renderer asks a `FontSource` for faces by family and style; the
//! font library crate (core/fonts) can plug in later by implementing it.

pub mod face;
pub mod system;

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use face::{FaceMetrics, FontOrigin, LoadedFace};

/// a face's number in a render; `FontLibrary::fonts` lists them.
pub type FontId = u32;

/// where a face can be loaded from.
#[derive(Clone, Debug)]
pub enum FaceLocation {
    File {
        path: std::path::PathBuf,
        index: u32,
        /// for a variable font: the weight of the instance to lay out
        weight: Option<f32>,
    },
    Bytes {
        bytes: Arc<Vec<u8>>,
        index: u32,
        origin: FontOrigin,
    },
}

/// what the engine needs from whoever knows the fonts.
pub trait FontSource: Send + Sync {
    /// a face of this family with this style, if there is one. a face of
    /// the family in another style is better than none: the library then
    /// marks the style as synthesised.
    fn find(&self, family: &str, bold: bool, italic: bool) -> Option<FaceLocation>;

    /// families that draw like `family` with the same advance widths
    /// (carlito for calibri...), best first.
    fn metric_compatible(&self, family: &str) -> Vec<String> {
        metric_compatible_substitutes(family)
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    /// families that only look like `family` (different widths), tried
    /// after the metric-compatible ones.
    fn similar(&self, family: &str) -> Vec<String> {
        similar_substitutes(family)
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    /// families to try for a character the chosen face cannot draw.
    fn fallback_families(&self, ch: char) -> Vec<String> {
        default_fallbacks(ch)
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    /// the families tried when nothing else is found.
    fn last_resort(&self) -> Vec<String> {
        LAST_RESORT.iter().map(|name| name.to_string()).collect()
    }
}

/// the installed fonts of this computer.
#[derive(Clone, Copy, Debug)]
pub struct SystemFonts {
    /// look in office's cloud font folder too (word does, libreoffice
    /// does not: it set the corpus' aptos probe in times new roman)
    pub office_cloud_fonts: bool,
}

impl Default for SystemFonts {
    fn default() -> Self {
        SystemFonts {
            office_cloud_fonts: true,
        }
    }
}

impl FontSource for SystemFonts {
    fn find(&self, family: &str, bold: bool, italic: bool) -> Option<FaceLocation> {
        let wanted = family.trim().to_lowercase();
        let cloud = system::office_cloud_folder();
        let faces: Vec<&system::InstalledFace> = system::installed_faces()
            .iter()
            .filter(|face| {
                face.families
                    .iter()
                    .any(|name| name.to_lowercase() == wanted)
            })
            .filter(|face| {
                self.office_cloud_fonts
                    || cloud
                        .as_ref()
                        .is_none_or(|folder| !face.path.starts_with(folder))
            })
            .collect();
        let best = faces
            .iter()
            .min_by_key(|face| style_distance(face, bold, italic))?;
        Some(FaceLocation::File {
            path: best.path.clone(),
            index: best.index,
            weight: None,
        })
    }
}

/// how far a face's style is from the wanted one; 0 is exact.
fn style_distance(face: &system::InstalledFace, bold: bool, italic: bool) -> u32 {
    let mut distance = 0;
    if face.bold != bold {
        distance += 10;
    }
    if face.italic != italic {
        distance += 20;
    }
    // among regular faces prefer weight 400 over light or semibold ones
    distance + u32::from(face.weight.abs_diff(if bold { 700 } else { 400 }) / 100)
}

/// faces given directly, tried before another source (document-embedded
/// fonts, or fonts a test provides).
pub struct ProvidedFonts {
    pub faces: Vec<ProvidedFace>,
    pub then: Arc<dyn FontSource>,
}

#[derive(Clone, Debug)]
pub struct ProvidedFace {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub bytes: Arc<Vec<u8>>,
    pub index: u32,
    pub origin: FontOrigin,
}

impl FontSource for ProvidedFonts {
    fn find(&self, family: &str, bold: bool, italic: bool) -> Option<FaceLocation> {
        let wanted = family.to_lowercase();
        let of_family = || {
            self.faces
                .iter()
                .filter(|f| f.family.to_lowercase() == wanted)
        };
        let location = |face: &ProvidedFace| FaceLocation::Bytes {
            bytes: face.bytes.clone(),
            index: face.index,
            origin: face.origin.clone(),
        };
        if let Some(face) = of_family().find(|f| f.bold == bold && f.italic == italic) {
            return Some(location(face));
        }
        // the computer may have the missing style; else synthesise it from
        // the embedded face, as word does
        self.then
            .find(family, bold, italic)
            .or_else(|| of_family().next().map(location))
    }

    fn metric_compatible(&self, family: &str) -> Vec<String> {
        self.then.metric_compatible(family)
    }

    fn similar(&self, family: &str) -> Vec<String> {
        self.then.similar(family)
    }

    fn fallback_families(&self, ch: char) -> Vec<String> {
        self.then.fallback_families(ch)
    }

    fn last_resort(&self) -> Vec<String> {
        self.then.last_resort()
    }
}

/// metric-compatible open fonts for the common office fonts: same advance
/// widths, so a document breaks its lines the same way with either.
pub fn metric_compatible_substitutes(family: &str) -> &'static [&'static str] {
    match family.to_lowercase().as_str() {
        "calibri" => &["Carlito"],
        "arial" | "helvetica" => &["Liberation Sans", "Arimo", "Arial"],
        "times new roman" | "times" => &["Liberation Serif", "Tinos", "Times New Roman"],
        "courier new" | "courier" => &["Liberation Mono", "Cousine", "Courier New"],
        "georgia" => &["Gelasio"],
        "liberation serif" | "tinos" => &["Times New Roman"],
        "liberation sans" | "arimo" => &["Arial"],
        "carlito" => &["Calibri"],
        _ => &[],
    }
}

/// open fonts that look like a common office font without its widths.
/// caladea was made for cambria, but the font library measured the
/// version it ships 2.5 % of the em off on average (core/fonts readme),
/// so lines break differently with it.
pub fn similar_substitutes(family: &str) -> &'static [&'static str] {
    match family.to_lowercase().as_str() {
        "cambria" => &["Caladea"],
        "calibri light" => &["Carlito"],
        "aptos" => &["Inter"],
        _ => &[],
    }
}

/// families tried, in order, for a character the run's font lacks.
pub fn default_fallbacks(ch: char) -> &'static [&'static str] {
    let code = ch as u32;
    match code {
        0x0590..=0x06ff | 0x0750..=0x077f => &["Times New Roman", "Arial", "Segoe UI"],
        0x0900..=0x0dff => &["Nirmala UI", "Mangal"],
        0x3000..=0x9fff | 0xff00..=0xffef => {
            &["Yu Gothic", "MS Gothic", "Microsoft YaHei", "SimSun"]
        }
        0xac00..=0xd7af => &["Malgun Gothic"],
        0x2200..=0x22ff | 0x1d400..=0x1d7ff => &["Cambria Math", "Segoe UI Symbol"],
        0x2190..=0x2bff | 0x1f300..=0x1faff => {
            &["Segoe UI Symbol", "Segoe UI Emoji", "Cambria Math"]
        }
        _ => &[
            "Segoe UI",
            "Arial",
            "Times New Roman",
            "Segoe UI Symbol",
            "Cambria Math",
        ],
    }
}

/// a substitution the layout had to make, for the report and the ui.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontSubstitution {
    pub requested: String,
    pub bold: bool,
    pub italic: bool,
    /// the family actually used
    pub used: String,
    pub reason: SubstitutionReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstitutionReason {
    /// the family is not installed; a metric-compatible one stands in
    MetricCompatible,
    /// the family is not installed; a look-alike with other widths stands in
    Similar,
    /// the family is not installed; a general fallback stands in
    NotInstalled,
    /// the family is there but not in this style; it is synthesised
    StyleSynthesised,
    /// single characters the face cannot draw come from another font
    MissingCharacters,
}

/// one resolved face as layout uses it: the loaded face and how it should
/// be drawn (bold or italic made up when the family lacks that style).
#[derive(Clone, Debug)]
pub struct ResolvedFont {
    pub face: Arc<LoadedFace>,
    pub synthetic_bold: bool,
    pub synthetic_italic: bool,
}

/// the faces one layout uses, numbered, with what had to be substituted.
pub struct FontLibrary {
    source: Arc<dyn FontSource>,
    fonts: Vec<ResolvedFont>,
    by_request: HashMap<(String, bool, bool), FontId>,
    by_location: HashMap<(String, u32), Arc<LoadedFace>>,
    fallback_for_char: HashMap<(FontId, char), Option<FontId>>,
    pub substitutions: Vec<FontSubstitution>,
}

/// the last resort when nothing else is found: the usual default faces.
pub(crate) const LAST_RESORT: &[&str] = &[
    "Times New Roman",
    "Liberation Serif",
    "Arial",
    "DejaVu Serif",
    "Helvetica",
];

impl FontLibrary {
    pub fn new(source: Arc<dyn FontSource>) -> FontLibrary {
        FontLibrary {
            source,
            fonts: Vec::new(),
            by_request: HashMap::new(),
            by_location: HashMap::new(),
            fallback_for_char: HashMap::new(),
            substitutions: Vec::new(),
        }
    }

    pub fn system() -> FontLibrary {
        FontLibrary::new(Arc::new(SystemFonts::default()))
    }

    pub fn font(&self, id: FontId) -> &ResolvedFont {
        &self.fonts[id as usize]
    }

    pub fn fonts(&self) -> &[ResolvedFont] {
        &self.fonts
    }

    /// the font for a family and style, loading it the first time.
    /// `None` only when no font at all can be found on the computer.
    pub fn resolve(&mut self, family: &str, bold: bool, italic: bool) -> Option<FontId> {
        let key = (family.to_lowercase(), bold, italic);
        if let Some(id) = self.by_request.get(&key) {
            return Some(*id);
        }
        let id = self.resolve_uncached(family, bold, italic)?;
        self.by_request.insert(key, id);
        Some(id)
    }

    fn resolve_uncached(&mut self, family: &str, bold: bool, italic: bool) -> Option<FontId> {
        if let Some(face) = self.load_family(family, bold, italic) {
            return Some(self.add(face, family, bold, italic, None));
        }
        let compatible = self.source.metric_compatible(family);
        for candidate in &compatible {
            if let Some(face) = self.load_family(candidate, bold, italic) {
                let reason = Some(SubstitutionReason::MetricCompatible);
                return Some(self.add(face, family, bold, italic, reason));
            }
        }
        for candidate in &self.source.similar(family) {
            if let Some(face) = self.load_family(candidate, bold, italic) {
                let reason = Some(SubstitutionReason::Similar);
                return Some(self.add(face, family, bold, italic, reason));
            }
        }
        for candidate in &self.source.last_resort() {
            if let Some(face) = self.load_family(candidate, bold, italic) {
                let reason = Some(SubstitutionReason::NotInstalled);
                return Some(self.add(face, family, bold, italic, reason));
            }
        }
        let any = system::installed_faces().first()?;
        let face = self.load(&FaceLocation::File {
            path: any.path.clone(),
            index: any.index,
            weight: None,
        })?;
        Some(self.add(
            face,
            family,
            bold,
            italic,
            Some(SubstitutionReason::NotInstalled),
        ))
    }

    fn load_family(&mut self, family: &str, bold: bool, italic: bool) -> Option<Arc<LoadedFace>> {
        let location = self.source.find(family, bold, italic)?;
        self.load(&location)
    }

    fn load(&mut self, location: &FaceLocation) -> Option<Arc<LoadedFace>> {
        let key = match location {
            FaceLocation::File {
                path,
                index,
                weight,
            } => (
                format!("{}@{}", path.to_string_lossy(), weight.unwrap_or(0.0)),
                *index,
            ),
            FaceLocation::Bytes { bytes, index, .. } => {
                (format!("bytes:{:p}", Arc::as_ptr(bytes)), *index)
            }
        };
        if let Some(face) = self.by_location.get(&key) {
            return Some(face.clone());
        }
        let face = match location {
            FaceLocation::File {
                path,
                index,
                weight,
            } => {
                let bytes = Arc::new(std::fs::read(path).ok()?);
                let origin = FontOrigin::System { path: path.clone() };
                match weight {
                    Some(weight) => LoadedFace::load_instance(bytes, *index, origin, *weight)?,
                    None => LoadedFace::load(bytes, *index, origin)?,
                }
            }
            FaceLocation::Bytes {
                bytes,
                index,
                origin,
            } => LoadedFace::load(bytes.clone(), *index, origin.clone())?,
        };
        let face = Arc::new(face);
        self.by_location.insert(key, face.clone());
        Some(face)
    }

    fn add(
        &mut self,
        face: Arc<LoadedFace>,
        requested: &str,
        bold: bool,
        italic: bool,
        reason: Option<SubstitutionReason>,
    ) -> FontId {
        let synthetic_bold = bold && !face.bold;
        let synthetic_italic = italic && !face.italic;
        let reason = reason
            .or((synthetic_bold || synthetic_italic)
                .then_some(SubstitutionReason::StyleSynthesised));
        if let Some(reason) = reason {
            self.substitutions.push(FontSubstitution {
                requested: requested.to_string(),
                bold,
                italic,
                used: face.family.clone(),
                reason,
            });
        }
        // the same face with the same synthesis is one font in the result
        if let Some(existing) = self.fonts.iter().position(|font| {
            Arc::ptr_eq(&font.face, &face)
                && font.synthetic_bold == synthetic_bold
                && font.synthetic_italic == synthetic_italic
        }) {
            return existing as FontId;
        }
        self.fonts.push(ResolvedFont {
            face,
            synthetic_bold,
            synthetic_italic,
        });
        (self.fonts.len() - 1) as FontId
    }

    /// a font that can draw `ch` when `font` cannot, if any. word and
    /// libreoffice both borrow single characters from other fonts.
    pub fn fallback(&mut self, font: FontId, ch: char) -> Option<FontId> {
        if let Some(found) = self.fallback_for_char.get(&(font, ch)) {
            return *found;
        }
        let (bold, italic) = {
            let resolved = self.font(font);
            (resolved.face.bold, resolved.face.italic)
        };
        let mut found = None;
        for family in self.source.fallback_families(ch) {
            if let Some(face) = self.load_family(&family, bold, italic) {
                if face.has_char(ch) {
                    let requested = self.font(font).face.family.clone();
                    found = Some(self.add(
                        face,
                        &requested,
                        bold,
                        italic,
                        Some(SubstitutionReason::MissingCharacters),
                    ));
                    break;
                }
            }
        }
        self.fallback_for_char.insert((font, ch), found);
        found
    }
}
