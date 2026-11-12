//! the font library as the app uses it: one object that keeps the index
//! up to date while system fonts are scanned in the background, adds and
//! removes the person's fonts, downloads library families, remembers
//! favourites and recent fonts, and answers searches and previews.
//!
//! it owns no window and no thread pool: the scan and each download run
//! on their own thread, and the ui asks for `status()` to follow them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::catalogue::Catalogue;
use crate::container::{display_name, read_limited, to_plain_font, MAX_SYSTEM_FONT_BYTES};
use crate::coverage::writing;
use crate::custom::{self, AddReport};
use crate::download::{family_folder, DownloadProgress, Downloader, LIBRARY_HOST};
use crate::error::FontError;
use crate::index::{family_key, Family, FontIndex, Style};
use crate::model::{Axis, Category, FaceRecord, Source};
use crate::preview::{preview_svg, Preview};
use crate::scan::{find_font_files, scan, ScanCache, ScanProgress};
use crate::search::{search, Personal, SearchQuery, SearchResults};
use crate::similar::{similar_to, Similar};
use crate::subset::{embedding_data, EmbedData};
use crate::substitute::{missing_fonts, resolve, MissingFont, Resolution};

/// where the library keeps things.
#[derive(Clone, Debug)]
pub struct LibraryPaths {
    /// the app's data folder for fonts: added fonts, downloads, settings
    pub data: PathBuf,
    /// the app's cache folder for fonts: the scan cache and previews
    pub cache: PathBuf,
    /// the fonts shipped with the app, when they were found
    pub bundled: Option<PathBuf>,
    /// the folders to scan for system fonts
    pub system: Vec<PathBuf>,
}

impl LibraryPaths {
    pub fn custom(&self) -> PathBuf {
        self.data.join("added")
    }
    pub fn library(&self) -> PathBuf {
        self.data.join("library")
    }
    fn settings(&self) -> PathBuf {
        self.data.join("settings.json")
    }
    fn scan_cache(&self) -> PathBuf {
        self.cache.join("system-fonts.json")
    }
    fn previews(&self) -> PathBuf {
        self.cache.join("previews")
    }
}

/// what the library remembers for the person.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub personal: Personal,
    /// the one-time notice that downloading contacts github was seen
    pub download_notice_accepted: bool,
}

/// how many recent fonts are remembered.
const RECENT_COUNT: usize = 12;

/// one family as the font list shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilySummary {
    pub name: String,
    pub category: Category,
    pub sources: Vec<Source>,
    pub on_this_computer: bool,
    /// true when the web view must be given the font's bytes to draw it
    /// (it isn't installed on the system)
    pub needs_font_face: bool,
    /// the face that stands for the family, when it is on this computer
    pub regular_face: Option<String>,
    pub style_count: usize,
    pub has_bold: bool,
    pub has_italic: bool,
    pub variable: bool,
    pub monospace: bool,
    pub embeddable: bool,
    pub favourite: bool,
    pub download_bytes: Option<u64>,
}

/// everything the detail pane shows about a family.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyDetails {
    pub summary: FamilySummary,
    pub styles: Vec<Style>,
    /// writing system names: "Latin (English)", "Vietnamese"...
    pub writing: Vec<String>,
    pub embedding_sentence: String,
    pub license: Option<String>,
    /// one sentence on where the font comes from
    pub source_sentence: String,
    pub designer: Option<String>,
    pub version: Option<String>,
    pub glyph_count: Option<u16>,
    pub axes: Vec<Axis>,
    pub files: Vec<PathBuf>,
    /// fonts with the same widths or a similar look
    pub similar: Vec<Similar>,
    /// true when this family's preview file has been fetched
    pub preview_ready: bool,
}

/// one download, as the ui follows it.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadState {
    pub family: String,
    pub progress: DownloadProgress,
    pub finished: bool,
    pub error: Option<String>,
    #[serde(skip)]
    cancel: Arc<AtomicBool>,
}

/// the state the ui polls while something is under way.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStatus {
    /// goes up whenever the families change
    pub revision: u64,
    pub scan: ScanProgress,
    pub family_count: usize,
    pub downloads: Vec<DownloadState>,
    pub download_notice_accepted: bool,
    /// the one host downloads contact
    pub download_host: &'static str,
}

#[derive(Default)]
struct State {
    system: Vec<FaceRecord>,
    custom: Vec<FaceRecord>,
    bundled: Vec<FaceRecord>,
    library: Vec<FaceRecord>,
    index: Arc<FontIndex>,
    revision: u64,
    scan: ScanProgress,
    settings: Settings,
    downloads: HashMap<String, DownloadState>,
}

impl State {
    fn rebuild(&mut self, catalogue: &'static Catalogue) {
        let records = self
            .system
            .iter()
            .chain(&self.custom)
            .chain(&self.bundled)
            .chain(&self.library)
            .cloned()
            .collect();
        self.index = Arc::new(FontIndex::build(records, catalogue));
        self.revision += 1;
    }
}

pub struct FontLibrary {
    paths: LibraryPaths,
    catalogue: &'static Catalogue,
    downloader: Downloader,
    state: Mutex<State>,
    /// set when the app is closing, so a running scan stops
    closing: AtomicBool,
}

impl FontLibrary {
    pub fn new(
        paths: LibraryPaths,
        catalogue: &'static Catalogue,
        downloader: Downloader,
    ) -> Arc<FontLibrary> {
        let settings = std::fs::read(paths.settings())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let mut state = State {
            settings,
            ..State::default()
        };
        state.rebuild(catalogue);
        Arc::new(FontLibrary {
            paths,
            catalogue,
            downloader,
            state: Mutex::new(state),
            closing: AtomicBool::new(false),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn paths(&self) -> &LibraryPaths {
        &self.paths
    }

    /// reads Letters' own fonts at once (there are few), then scans the
    /// system's on a background thread, filling the index as it goes.
    pub fn start_scan(self: &Arc<Self>) -> std::thread::JoinHandle<()> {
        let library = self.clone();
        std::thread::spawn(move || library.scan_everything())
    }

    /// the whole scan, on the calling thread.
    pub fn scan_everything(&self) {
        let custom = self.scan_folder(&self.paths.custom(), Source::Custom);
        let library = self.scan_folder(&self.paths.library(), Source::Library);
        let bundled = self
            .paths
            .bundled
            .as_ref()
            .map(|folder| self.scan_folder(folder, Source::Bundled))
            .unwrap_or_default();
        {
            let mut state = self.state();
            state.custom = custom;
            state.library = library;
            state.bundled = bundled;
            state.rebuild(self.catalogue);
        }
        let cache_path = self.paths.scan_cache();
        let mut cache = ScanCache::load(&cache_path);
        let files = find_font_files(&self.paths.system);
        let mut system = Vec::new();
        scan(&files, Source::System, &mut cache, |batch, progress| {
            system.extend(batch);
            let mut state = self.state();
            state.system = system.clone();
            state.scan = progress;
            state.rebuild(self.catalogue);
            !self.closing.load(Ordering::Relaxed)
        });
        cache.keep_only(&files);
        cache.save(&cache_path).ok();
    }

    /// Letters' own folders are small and change rarely; they are read
    /// without the cache, so a font added or removed shows at once.
    fn scan_folder(&self, folder: &Path, source: Source) -> Vec<FaceRecord> {
        let mut records = Vec::new();
        let mut no_cache = ScanCache::default();
        scan(
            &find_font_files(&[folder.to_path_buf()]),
            source,
            &mut no_cache,
            |batch, _| {
                records.extend(batch);
                true
            },
        );
        records
    }

    /// stops a running scan at the next file.
    pub fn close(&self) {
        self.closing.store(true, Ordering::Relaxed);
        for download in self.state().downloads.values() {
            download.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn index(&self) -> Arc<FontIndex> {
        self.state().index.clone()
    }

    pub fn status(&self) -> LibraryStatus {
        let state = self.state();
        let mut downloads: Vec<DownloadState> = state.downloads.values().cloned().collect();
        downloads.sort_by(|a, b| a.family.cmp(&b.family));
        LibraryStatus {
            revision: state.revision,
            scan: state.scan,
            family_count: state.index.families.len(),
            downloads,
            download_notice_accepted: state.settings.download_notice_accepted,
            download_host: LIBRARY_HOST,
        }
    }

    fn personal(&self) -> Personal {
        self.state().settings.personal.clone()
    }

    /// every family, in index order (the order search results refer to),
    /// with the revision they belong to.
    pub fn families(&self) -> (u64, Vec<FamilySummary>) {
        let (revision, index) = {
            let state = self.state();
            (state.revision, state.index.clone())
        };
        let personal = self.personal();
        (
            revision,
            index
                .families
                .iter()
                .map(|family| summary(family, &personal))
                .collect(),
        )
    }

    pub fn search(&self, query: &SearchQuery) -> (u64, SearchResults) {
        let (revision, index) = {
            let state = self.state();
            (state.revision, state.index.clone())
        };
        (revision, search(&index, query, &self.personal()))
    }

    pub fn details(&self, name: &str) -> Result<FamilyDetails, FontError> {
        let index = self.index();
        let family = index.family(name).ok_or(FontError::UnknownFont)?;
        Ok(details(
            family,
            &index,
            &self.personal(),
            self.preview_file(family).is_some(),
        ))
    }

    // ---- the person's lists ----

    fn save_settings(&self, settings: &Settings) {
        if let Ok(bytes) = serde_json::to_vec_pretty(settings) {
            std::fs::create_dir_all(&self.paths.data).ok();
            let temporary = self.paths.settings().with_extension("saving");
            if std::fs::write(&temporary, bytes).is_ok() {
                std::fs::rename(&temporary, self.paths.settings()).ok();
            }
        }
    }

    pub fn set_favourite(&self, name: &str, favourite: bool) -> Personal {
        let settings = {
            let mut state = self.state();
            let list = &mut state.settings.personal.favourites;
            list.retain(|entry| family_key(entry) != family_key(name));
            if favourite {
                list.push(name.to_string());
                list.sort_by_key(|entry| family_key(entry));
            }
            state.settings.clone()
        };
        self.save_settings(&settings);
        settings.personal
    }

    /// remembers that a font was just used, for the "Recent" group.
    pub fn note_used(&self, name: &str) -> Personal {
        let settings = {
            let mut state = self.state();
            let list = &mut state.settings.personal.recent;
            list.retain(|entry| family_key(entry) != family_key(name));
            list.insert(0, name.to_string());
            list.truncate(RECENT_COUNT);
            state.settings.clone()
        };
        self.save_settings(&settings);
        settings.personal
    }

    pub fn personal_lists(&self) -> Personal {
        self.personal()
    }

    pub fn accept_download_notice(&self) {
        let settings = {
            let mut state = self.state();
            state.settings.download_notice_accepted = true;
            state.settings.clone()
        };
        self.save_settings(&settings);
    }

    // ---- added fonts ----

    pub fn add_fonts(&self, paths: &[PathBuf]) -> AddReport {
        let report = custom::add_fonts(paths, &self.paths.custom());
        if !report.added.is_empty() {
            let mut state = self.state();
            state.custom.extend(report.added.iter().cloned());
            state.rebuild(self.catalogue);
        }
        report
    }

    /// removes every added face of a family, or downloaded files of a
    /// library family. system and bundled fonts can't be removed.
    pub fn remove_family(&self, name: &str) -> Result<(), FontError> {
        let index = self.index();
        let family = index.family(name).ok_or(FontError::UnknownFont)?;
        let removable: Vec<&FaceRecord> = family
            .faces
            .iter()
            .filter(|face| matches!(face.source, Source::Custom | Source::Library))
            .collect();
        if removable.is_empty() {
            return Err(FontError::NotRemovable);
        }
        for face in &removable {
            match face.source {
                Source::Custom => custom::remove_font(&face.path, &self.paths.custom())?,
                _ => custom::remove_font(&face.path, &self.paths.library())?,
            }
        }
        if let Some(entry) = family.catalogue {
            let folder = family_folder(&self.paths.library(), entry);
            if folder.starts_with(self.paths.library()) && folder.exists() {
                std::fs::remove_dir_all(&folder).ok();
            }
        }
        let mut state = self.state();
        let gone: Vec<String> = removable.iter().map(|face| face.key.clone()).collect();
        state.custom.retain(|face| !gone.contains(&face.key));
        state.library.retain(|face| !gone.contains(&face.key));
        state.downloads.remove(&family.name);
        state.rebuild(self.catalogue);
        Ok(())
    }

    // ---- font bytes, previews and embedding ----

    fn plain_bytes(path: &Path) -> Result<Vec<u8>, FontError> {
        to_plain_font(
            read_limited(path, MAX_SYSTEM_FONT_BYTES)?,
            &display_name(path),
        )
    }

    /// one face as a single plain font, for the web view's FontFace.
    pub fn face_bytes(&self, key: &str) -> Result<Vec<u8>, FontError> {
        let index = self.index();
        let face = index.face(key).ok_or(FontError::UnknownFont)?;
        let plain = Self::plain_bytes(&face.path)?;
        crate::sfnt::single_font(&plain, face.index)
            .ok_or_else(|| FontError::Damaged(display_name(&face.path)))
    }

    /// the fetched preview file of a family not on this computer.
    fn preview_file(&self, family: &Family) -> Option<PathBuf> {
        let entry = family.catalogue?;
        let file = entry.regular_file()?;
        let path = family_folder(&self.paths.previews(), entry).join(&file.filename);
        path.is_file().then_some(path)
    }

    /// where the fetched preview file of a library family is, once
    /// `fetch_preview` has fetched it.
    pub fn preview_font_file(&self, name: &str) -> Option<PathBuf> {
        let index = self.index();
        self.preview_file(index.family(name)?)
    }

    /// downloads only the regular file of a library family, so it can be
    /// previewed before choosing to download it all.
    pub fn fetch_preview(&self, name: &str) -> Result<(), FontError> {
        let index = self.index();
        let family = index.family(name).ok_or(FontError::UnknownFont)?;
        let entry = family
            .catalogue
            .ok_or_else(|| FontError::NotInCatalogue(name.to_string()))?;
        let file = entry
            .regular_file()
            .ok_or_else(|| FontError::NotInCatalogue(name.to_string()))?;
        let folder = family_folder(&self.paths.previews(), entry);
        std::fs::create_dir_all(&folder)
            .map_err(|error| FontError::CannotSave(error.to_string()))?;
        let cancel = AtomicBool::new(false);
        self.downloader.fetch_file(
            &self.catalogue.commit,
            &entry.dir,
            file,
            &folder.join(&file.filename),
            &mut |_| {},
            &cancel,
        )?;
        let mut state = self.state();
        state.revision += 1;
        Ok(())
    }

    /// sample text in a family (its regular face, or the fetched preview
    /// file), drawn as an svg path.
    pub fn preview(
        &self,
        name: &str,
        text: &str,
        size: f32,
        weight: Option<u16>,
        italic: bool,
    ) -> Result<Preview, FontError> {
        let index = self.index();
        let family = index.family(name).ok_or(FontError::UnknownFont)?;
        let (path, face_index) = match index.closest_face(name, weight.unwrap_or(400), italic) {
            Some(face) => (face.path.clone(), face.index),
            None => (self.preview_file(family).ok_or(FontError::UnknownFont)?, 0),
        };
        let plain = Self::plain_bytes(&path)?;
        let variations = weight
            .map(|weight| vec![("wght".to_string(), f32::from(weight))])
            .unwrap_or_default();
        preview_svg(&plain, face_index, text, size, &variations)
            .ok_or_else(|| FontError::Damaged(display_name(&path)))
    }

    /// the font program to embed for `text` in a family's closest face.
    pub fn embedding(
        &self,
        name: &str,
        weight: u16,
        italic: bool,
        text: &str,
    ) -> Result<EmbedData, FontError> {
        let index = self.index();
        let face = index
            .closest_face(name, weight, italic)
            .ok_or(FontError::UnknownFont)?;
        embedding_data(&Self::plain_bytes(&face.path)?, face.index, text)
    }

    pub fn resolve(&self, name: &str, weight: u16, italic: bool) -> Resolution {
        resolve(&self.index(), name, weight, italic)
    }

    pub fn missing_fonts(&self, names: &[String]) -> Vec<MissingFont> {
        missing_fonts(&self.index(), names)
    }

    pub fn similar(&self, name: &str, limit: usize) -> Vec<Similar> {
        similar_to(&self.index(), name, limit)
    }

    // ---- downloads ----

    /// starts downloading a library family on its own thread. asking again
    /// while it runs does nothing; after a failure or cancel it continues.
    pub fn start_download(self: &Arc<Self>, name: &str) -> Result<(), FontError> {
        let index = self.index();
        let family = index.family(name).ok_or(FontError::UnknownFont)?;
        let entry = family
            .catalogue
            .ok_or_else(|| FontError::NotInCatalogue(name.to_string()))?;
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut state = self.state();
            if state
                .downloads
                .get(&entry.name)
                .is_some_and(|download| !download.finished && download.error.is_none())
            {
                return Ok(());
            }
            state.downloads.insert(
                entry.name.clone(),
                DownloadState {
                    family: entry.name.clone(),
                    progress: DownloadProgress {
                        done: 0,
                        total: entry.download_bytes(),
                    },
                    cancel: cancel.clone(),
                    ..DownloadState::default()
                },
            );
        }
        let library = self.clone();
        std::thread::spawn(move || library.run_download(entry, &cancel));
        Ok(())
    }

    fn run_download(&self, entry: &'static crate::catalogue::CatalogueFamily, cancel: &AtomicBool) {
        let mut report = |progress: DownloadProgress| {
            if let Some(download) = self.state().downloads.get_mut(&entry.name) {
                download.progress = progress;
            }
        };
        let result = self.downloader.download_family(
            &self.catalogue.commit,
            entry,
            &self.paths.library(),
            &mut report,
            cancel,
        );
        let new_faces = result
            .as_ref()
            .ok()
            .map(|folder| self.scan_folder(folder, Source::Library));
        let mut state = self.state();
        if let Some(faces) = new_faces {
            let folder = family_folder(&self.paths.library(), entry);
            state.library.retain(|face| !face.path.starts_with(&folder));
            state.library.extend(faces);
            state.rebuild(self.catalogue);
        }
        if let Some(download) = state.downloads.get_mut(&entry.name) {
            match result {
                Ok(_) => download.finished = true,
                Err(error) => download.error = Some(error.to_string()),
            }
        }
    }

    pub fn cancel_download(&self, name: &str) {
        let key = family_key(name);
        if let Some(download) = self
            .state()
            .downloads
            .values()
            .find(|download| family_key(&download.family) == key)
        {
            download.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// the font files a finished scan found, for tests and diagnostics.
    pub fn face_count(&self) -> usize {
        self.index()
            .families
            .iter()
            .map(|family| family.faces.len())
            .sum()
    }
}

pub fn summary(family: &Family, personal: &Personal) -> FamilySummary {
    let sources = family.sources();
    FamilySummary {
        name: family.name.clone(),
        category: family.category(),
        on_this_computer: family.is_on_this_computer(),
        needs_font_face: family.is_on_this_computer() && !sources.contains(&Source::System),
        regular_face: family.regular().map(|face| face.key.clone()),
        style_count: family.style_count(),
        has_bold: family.has_bold(),
        has_italic: family.has_italic(),
        variable: family.variable(),
        monospace: family.monospace(),
        embeddable: family.embedding().allows_embedding(),
        favourite: personal.is_favourite(&family.name),
        download_bytes: family.download_bytes(),
        sources,
    }
}

/// "2.4 MB" for a number of bytes.
pub fn size_words(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes.div_ceil(1024).max(1))
    }
}

fn source_sentence(family: &Family) -> String {
    let sources = family.sources();
    let first = family.regular();
    match sources.first() {
        Some(Source::System) => match first.and_then(|face| face.path.parent()) {
            Some(folder) => format!("Installed on this computer, in {}.", folder.display()),
            None => "Installed on this computer.".to_string(),
        },
        Some(Source::Custom) => "Added by you. Letters keeps its own copy, so the original file can be moved or deleted.".to_string(),
        Some(Source::Bundled) => "Comes with Letters.".to_string(),
        Some(Source::Library) => "Downloaded from the Letters library (Google Fonts).".to_string(),
        Some(Source::Downloadable) | None => format!(
            "In the Letters library (Google Fonts). Downloading it takes {} of space.",
            size_words(family.download_bytes().unwrap_or(0))
        ),
    }
}

fn details(
    family: &Family,
    index: &FontIndex,
    personal: &Personal,
    preview_ready: bool,
) -> FamilyDetails {
    let regular = family.regular();
    FamilyDetails {
        summary: summary(family, personal),
        styles: family.styles(),
        writing: family
            .writing()
            .iter()
            .filter_map(|id| writing(id))
            .map(|writing| writing.name.to_string())
            .collect(),
        embedding_sentence: family.embedding().sentence(),
        license: family.license(),
        source_sentence: source_sentence(family),
        designer: family
            .catalogue
            .map(|entry| entry.designer.clone())
            .filter(|designer| !designer.is_empty()),
        version: regular
            .map(|face| face.facts.version.clone())
            .filter(|v| !v.is_empty()),
        glyph_count: regular.map(|face| face.facts.glyph_count),
        axes: regular
            .map(|face| face.facts.axes.clone())
            .or_else(|| family.catalogue.map(|entry| entry.axes.clone()))
            .unwrap_or_default(),
        files: {
            let mut files: Vec<PathBuf> =
                family.faces.iter().map(|face| face.path.clone()).collect();
            files.dedup();
            files
        },
        similar: similar_to(index, &family.name, 6),
        preview_ready,
    }
}
