//! the scan cache, downloads against a local server (never the
//! internet), and the library end to end: scanning, adding and removing
//! fonts, favourites, search, missing fonts.

mod support;

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use letters_fonts::download::git_blob_hash;
use letters_fonts::scan::{find_font_files, scan, ScanCache};
use letters_fonts::{
    Catalogue, CatalogueFamily, CatalogueFile, Downloader, FontError, FontLibrary, LibraryPaths,
    SearchQuery, Source,
};
use support::{fixture, serve, temporary, vendor_fonts, Seen};

#[test]
fn the_cache_is_used_until_a_file_changes() {
    let folder = temporary("cache");
    let font = folder.join("Atkinson.ttf");
    std::fs::copy(fixture("AtkinsonHyperlegible-Regular.ttf"), &font).unwrap();
    let cache_path = folder.join("cache.json");
    let files = find_font_files(std::slice::from_ref(&folder));
    assert_eq!(files.len(), 1);

    let mut cache = ScanCache::load(&cache_path);
    let mut names = Vec::new();
    scan(&files, Source::System, &mut cache, |batch, _| {
        names.extend(batch.into_iter().map(|face| face.facts.family));
        true
    });
    assert_eq!(names, vec!["Atkinson Hyperlegible"]);
    cache.save(&cache_path).unwrap();

    // a second start reads the cache, not the file: prove it by changing
    // what the cache says
    let mut cache = ScanCache::load(&cache_path);
    cache.files.get_mut(&font).unwrap().faces[0].1.family = "From The Cache".into();
    let mut names = Vec::new();
    scan(&files, Source::System, &mut cache, |batch, _| {
        names.extend(batch.into_iter().map(|face| face.facts.family));
        true
    });
    assert_eq!(names, vec!["From The Cache"]);

    // a changed file (another size and time) is read again
    std::fs::copy(fixture("AtkinsonHyperlegible-Bold.ttf"), &font).unwrap();
    let files = find_font_files(std::slice::from_ref(&folder));
    let mut weights = Vec::new();
    scan(&files, Source::System, &mut cache, |batch, _| {
        weights.extend(batch.into_iter().map(|face| face.facts.weight));
        true
    });
    assert_eq!(weights, vec![700]);

    // a damaged cache file is ignored rather than trusted
    std::fs::write(&cache_path, b"{ not json").unwrap();
    assert!(ScanCache::load(&cache_path).files.is_empty());
    std::fs::remove_dir_all(&folder).ok();
}

/// a one-file family for download tests, pointing at bytes we control.
fn family_of(bytes: &[u8], folder: &std::path::Path) -> CatalogueFamily {
    let file = folder.join("hash-me");
    std::fs::write(&file, bytes).unwrap();
    CatalogueFamily {
        dir: "ofl/testfamily".into(),
        name: "Test Family".into(),
        designer: String::new(),
        license: "OFL".into(),
        categories: Vec::new(),
        date_added: String::new(),
        subsets: vec!["latin".into()],
        axes: Vec::new(),
        primary_script: String::new(),
        license_file: None,
        files: vec![CatalogueFile {
            filename: "Test-Regular.ttf".into(),
            style: "normal".into(),
            weight: 400,
            sha1: git_blob_hash(&file).unwrap(),
            size: bytes.len() as u64,
            postscript_name: "Test-Regular".into(),
            full_name: "Test Regular".into(),
        }],
    }
}

#[test]
fn downloads_check_hashes_resume_and_cancel() {
    let folder = temporary("download");
    let font = std::fs::read(fixture("AtkinsonHyperlegible-Regular.ttf")).unwrap();
    let family = family_of(&font, &folder);
    let path = "/abc123/ofl/testfamily/Test-Regular.ttf".to_string();

    // a good download
    let seen = Arc::new(Mutex::new(Seen::default()));
    let base = serve(HashMap::from([(path.clone(), font.clone())]), seen.clone());
    let downloader = Downloader::new(&base);
    let library = folder.join("library");
    let mut last = None;
    let target = downloader
        .download_family(
            "abc123",
            &family,
            &library,
            &mut |progress| last = Some(progress),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(target.join("Test-Regular.ttf")).unwrap(),
        font
    );
    assert_eq!(last.unwrap().done, font.len() as u64);
    assert_eq!(seen.lock().unwrap().paths, vec![path.clone()]);

    // a second call finds the file already there and asks for nothing
    downloader
        .download_family(
            "abc123",
            &family,
            &library,
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(seen.lock().unwrap().paths.len(), 1);

    // resuming: half a file is left from before; only the rest is asked for
    let resume_library = folder.join("resume");
    let resume_folder = resume_library.join("ofl-testfamily");
    std::fs::create_dir_all(&resume_folder).unwrap();
    std::fs::write(
        resume_folder.join("Test-Regular.ttf.part"),
        &font[..font.len() / 2],
    )
    .unwrap();
    downloader
        .download_family(
            "abc123",
            &family,
            &resume_library,
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(resume_folder.join("Test-Regular.ttf")).unwrap(),
        font
    );
    assert_eq!(
        seen.lock().unwrap().ranges.last().unwrap().as_deref(),
        Some(format!("bytes={}-", font.len() / 2).as_str())
    );

    // a server sending other bytes: refused, and nothing is kept
    let mut tampered = font.clone();
    tampered[1000] ^= 0xff;
    let bad_base = serve(
        HashMap::from([(path.clone(), tampered)]),
        Arc::new(Mutex::new(Seen::default())),
    );
    let bad_library = folder.join("bad");
    let error = Downloader::new(&bad_base)
        .download_family(
            "abc123",
            &family,
            &bad_library,
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert_eq!(error, FontError::HashMismatch("Test-Regular.ttf".into()));
    assert!(!bad_library
        .join("ofl-testfamily")
        .join("Test-Regular.ttf")
        .exists());
    assert!(!bad_library
        .join("ofl-testfamily")
        .join("Test-Regular.ttf.part")
        .exists());

    // cancelling keeps the part, so the next try continues
    let cancel = AtomicBool::new(true);
    let cancel_library = folder.join("cancel");
    let error = downloader
        .download_family("abc123", &family, &cancel_library, &mut |_| {}, &cancel)
        .unwrap_err();
    assert_eq!(error, FontError::Cancelled);
    assert!(cancel_library
        .join("ofl-testfamily")
        .join("Test-Regular.ttf.part")
        .exists());

    // a missing file is a failed download with a sentence
    let missing = family_of(b"other", &folder);
    let error = downloader
        .download_family(
            "nope",
            &missing,
            &folder.join("missing"),
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(error.to_string().starts_with("The download didn’t finish."));
    std::fs::remove_dir_all(&folder).ok();
}

fn library_in(folder: &std::path::Path, bundled: Option<std::path::PathBuf>) -> Arc<FontLibrary> {
    FontLibrary::new(
        LibraryPaths {
            data: folder.join("data"),
            cache: folder.join("cache"),
            bundled,
            system: Vec::new(),
        },
        Catalogue::shipped(),
        // nothing in these tests may reach the internet
        Downloader::new("http://127.0.0.1:9"),
    )
}

#[test]
fn added_fonts_appear_and_can_be_removed() {
    let folder = temporary("library");
    let library = library_in(&folder, None);
    library.scan_everything();
    let before = library.status().revision;

    let report = library.add_fonts(&[fixture("atkinson-latin-400.woff2"), fixture("OFL.txt")]);
    assert_eq!(report.added.len(), 1);
    assert_eq!(report.rejected.len(), 1, "the licence text is not a font");
    assert!(library.status().revision > before);
    // the woff2 was decoded and kept as a plain font in Letters' folder
    let kept = &report.added[0].path;
    assert!(kept.starts_with(folder.join("data").join("added")));
    assert_eq!(kept.extension().unwrap(), "ttf");

    let details = library.details("Atkinson Hyperlegible").unwrap();
    assert_eq!(details.summary.sources, vec![Source::Custom]);
    assert!(details.summary.needs_font_face);
    assert!(details.source_sentence.starts_with("Added by you."));
    assert_eq!(
        details.embedding_sentence,
        "This font can be embedded in PDFs and documents."
    );
    let bytes = library
        .face_bytes(details.summary.regular_face.as_ref().unwrap())
        .unwrap();
    assert!(ttf_parser::Face::parse(&bytes, 0).is_ok());
    let preview = library
        .preview("Atkinson Hyperlegible", "Abc", 40.0, None, false)
        .unwrap();
    assert!(!preview.path.is_empty());

    // adding the same font again copies nothing
    let again = library.add_fonts(&[fixture("atkinson-latin-400.woff2")]);
    assert!(again.added.is_empty());
    assert_eq!(again.already_added.len(), 1);

    let (_, results) = library.search(&SearchQuery {
        text: "atkinsn".into(),
        places: vec![letters_fonts::Place::AddedByMe],
        ..SearchQuery::default()
    });
    assert_eq!(results.families.len(), 1);

    library.remove_family("Atkinson Hyperlegible").unwrap();
    assert!(!kept.exists());
    let details = library.details("Atkinson Hyperlegible").unwrap();
    assert_eq!(
        details.summary.sources,
        vec![Source::Downloadable],
        "it is still in the online library"
    );
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn favourites_and_recents_are_kept() {
    let folder = temporary("personal");
    let library = library_in(&folder, None);
    library.set_favourite("Inter", true);
    library.note_used("Lobster");
    library.note_used("Inter");
    library.accept_download_notice();
    let reopened = library_in(&folder, None);
    let personal = reopened.personal_lists();
    assert_eq!(personal.favourites, vec!["Inter"]);
    assert_eq!(personal.recent, vec!["Inter", "Lobster"]);
    assert!(reopened.status().download_notice_accepted);
    reopened.set_favourite("inter", false);
    assert!(reopened.personal_lists().favourites.is_empty());
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn bundled_fonts_cover_the_missing_microsoft_fonts() {
    let Some(vendor) = vendor_fonts() else {
        eprintln!("skipped: run tools/fetch-fonts first for the bundled fonts");
        return;
    };
    let folder = temporary("bundled");
    let library = library_in(&folder, Some(vendor));
    library.scan_everything();
    assert!(library.face_count() >= 30);
    let missing = library.missing_fonts(&[
        "Calibri".into(),
        "Cambria".into(),
        "Georgia".into(),
        "Inter".into(),
    ]);
    let by_name = |name: &str| missing.iter().find(|font| font.name == name).unwrap();
    assert!(by_name("Calibri").substitution.available);
    assert_eq!(by_name("Calibri").substitution.used, "Carlito");
    assert!(by_name("Calibri").substitution.metric_compatible);
    assert_eq!(by_name("Cambria").substitution.used, "Caladea");
    // gelasio isn't bundled: it is offered for download
    assert!(by_name("Georgia").substitution.downloadable);
    assert!(
        missing.iter().all(|font| font.name != "Inter"),
        "Inter is bundled"
    );
    let resolution = library.resolve("Calibri", 700, true);
    let face = resolution.face.unwrap();
    assert_eq!(face.facts.family, "Carlito");
    assert_eq!(face.facts.weight, 700);
    assert!(face.facts.italic);
    let details = library.details("Carlito").unwrap();
    assert_eq!(
        details.license.as_deref(),
        Some("SIL Open Font License 1.1")
    );
    assert_eq!(details.source_sentence, "Comes with Letters.");
    std::fs::remove_dir_all(&folder).ok();
}
