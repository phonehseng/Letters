//! facts read from real fonts (the system's when present, small OFL
//! fixtures always), woff and woff2 decoding, previews, embedding and
//! look-alikes.

mod support;

use std::collections::BTreeSet;

use letters_fonts::container::{read_limited, to_plain_font};
use letters_fonts::facts::read_facts;
use letters_fonts::scan::read_file_faces;
use letters_fonts::similar::{metric_compatible, similar_to};
use letters_fonts::{
    embedding_data, preview_bitmap, preview_svg, Catalogue, Category, EmbedKind,
    EmbeddingPermission, FaceRecord, FontIndex, Source,
};
use support::{fixture, system_font, vendor_fonts};

fn plain(path: &std::path::Path) -> Vec<u8> {
    to_plain_font(read_limited(path, 64 << 20).unwrap(), "test").unwrap()
}

#[test]
fn the_fixture_font_has_the_facts_it_should() {
    let facts = read_facts(&plain(&fixture("AtkinsonHyperlegible-Regular.ttf")), 0).unwrap();
    assert_eq!(facts.family, "Atkinson Hyperlegible");
    assert_eq!(facts.style_name, "Regular");
    assert_eq!(facts.weight, 400);
    assert!(!facts.italic && !facts.monospace);
    assert_eq!(facts.category, Category::SansSerif);
    assert_eq!(facts.embedding.permission, EmbeddingPermission::Installable);
    assert_eq!(
        facts.embedding.sentence(),
        "This font can be embedded in PDFs and documents."
    );
    assert!(facts.writing.contains(&"latin".to_string()));
    assert!(facts.writing.contains(&"french".to_string()));
    assert!(!facts.writing.contains(&"greek".to_string()));
    assert!(facts.glyph_count > 200);
    assert!(facts.measurements.x_height > 400 && facts.measurements.x_height < 650);
    assert!(facts.measurements.contrast > 0.8 && facts.measurements.contrast < 2.0);
    let bold = read_facts(&plain(&fixture("AtkinsonHyperlegible-Bold.ttf")), 0).unwrap();
    assert_eq!(bold.weight, 700);
    assert!(bold.measurements.stem > facts.measurements.stem);
}

#[test]
fn woff_and_woff2_decode_to_the_same_font() {
    let ttf = read_facts(&plain(&fixture("AtkinsonHyperlegible-Regular.ttf")), 0).unwrap();
    for name in ["atkinson-latin-400.woff", "atkinson-latin-400.woff2"] {
        let decoded = plain(&fixture(name));
        let facts = read_facts(&decoded, 0).unwrap_or_else(|| panic!("{name} decodes"));
        assert_eq!(facts.family, ttf.family, "{name}");
        assert_eq!(facts.weight, 400);
        // same outlines: the same text draws the same amount of ink
        let a = preview_bitmap(
            &plain(&fixture("AtkinsonHyperlegible-Regular.ttf")),
            0,
            "Hamburgefonstiv",
            48.0,
            &[],
        )
        .unwrap();
        let b = preview_bitmap(&decoded, 0, "Hamburgefonstiv", 48.0, &[]).unwrap();
        let difference = (a.ink() - b.ink()).abs() / a.ink();
        assert!(difference < 0.01, "{name}: ink differs by {difference}");
        assert_eq!(a.width, b.width);
    }
}

#[test]
fn previews_have_ink_where_letters_are() {
    let data = plain(&fixture("AtkinsonHyperlegible-Regular.ttf"));
    let bitmap = preview_bitmap(&data, 0, "H  H", 64.0, &[]).unwrap();
    assert!(bitmap.ink() > 100.0);
    // the left stem of the first H is dark half way down; the gap between
    // the letters (two spaces) is empty
    let baseline_row = bitmap.height * 6 / 10;
    let first_stem = (0..bitmap.width / 4)
        .map(|x| bitmap.at(x, baseline_row))
        .max()
        .unwrap();
    assert!(first_stem > 200, "stem {first_stem}");
    let gap = (bitmap.width * 45 / 100..bitmap.width * 55 / 100)
        .map(|x| bitmap.at(x, baseline_row))
        .max()
        .unwrap();
    assert_eq!(gap, 0);
    let svg = preview_svg(&data, 0, "Letters 中", 32.0, &[]).unwrap();
    assert!(svg.path.starts_with('M'));
    assert!(svg.width > 50.0 && svg.height > 20.0);
    assert_eq!(svg.missing, vec!['中']);
}

#[test]
fn embedding_subsets_keep_glyph_numbers() {
    let data = plain(&fixture("AtkinsonHyperlegible-Regular.ttf"));
    let embed = embedding_data(&data, 0, "Hello").unwrap();
    assert_eq!(embed.kind, EmbedKind::TrueTypeSubset);
    assert!(embed.bytes.len() < data.len() / 2);
    let original = ttf_parser::Face::parse(&data, 0).unwrap();
    let subset = ttf_parser::Face::parse(&embed.bytes, 0).unwrap();
    assert_eq!(subset.number_of_glyphs(), original.number_of_glyphs());
    let h = original.glyph_index('H').unwrap();
    assert_eq!(subset.glyph_index('H'), Some(h));
    assert!(subset.glyph_bounding_box(h).is_some());
    let z = original.glyph_index('z').unwrap();
    assert!(
        subset.glyph_bounding_box(z).is_none(),
        "unused glyphs are emptied"
    );
    assert_eq!(
        embed.glyphs,
        BTreeSet::from([
            0,
            h.0,
            original.glyph_index('e').unwrap().0,
            original.glyph_index('l').unwrap().0,
            original.glyph_index('o').unwrap().0
        ])
    );
}

#[test]
fn system_fonts_have_the_facts_people_expect() {
    let Some(arial) = system_font("arial.ttf").or_else(|| system_font("Arial.ttf")) else {
        eprintln!("skipped: Arial is not installed on this computer");
        return;
    };
    let faces = read_file_faces(&arial, 128 << 20);
    let (_, facts) = &faces[0];
    assert_eq!(facts.family, "Arial");
    assert_eq!(facts.weight, 400);
    assert_eq!(facts.category, Category::SansSerif);
    for writing in ["latin", "greek", "cyrillic", "hebrew", "vietnamese"] {
        assert!(
            facts.writing.contains(&writing.to_string()),
            "Arial draws {writing}"
        );
    }
    assert!(facts.embedding.allows_embedding());

    if let Some(times) = system_font("times.ttf") {
        let (_, facts) = &read_file_faces(&times, 128 << 20)[0];
        assert_eq!(facts.family, "Times New Roman");
        assert_eq!(facts.category, Category::Serif);
        // Times has more stroke contrast than Arial
        assert!(facts.measurements.contrast > faces[0].1.measurements.contrast);
    }
    if let Some(courier) = system_font("cour.ttf") {
        let (_, facts) = &read_file_faces(&courier, 128 << 20)[0];
        assert!(facts.monospace);
        assert_eq!(facts.category, Category::Monospace);
    }
    match system_font("calibri.ttf") {
        Some(calibri) => {
            let (_, facts) = &read_file_faces(&calibri, 128 << 20)[0];
            assert_eq!(facts.family, "Calibri");
            // calibri allows editable embedding (fsType 8)
            assert_eq!(facts.embedding.permission, EmbeddingPermission::Editable);
        }
        None => eprintln!("skipped: Calibri is not installed on this computer"),
    }
}

/// face records for a font file, as the scan would make them.
fn records(path: &std::path::Path, source: Source) -> Vec<FaceRecord> {
    read_file_faces(path, 128 << 20)
        .into_iter()
        .map(|(index, facts)| FaceRecord {
            key: FaceRecord::make_key(source, path, index),
            source,
            path: path.to_path_buf(),
            index,
            facts,
            license: None,
        })
        .collect()
}

#[test]
fn similar_to_finds_the_metric_twins() {
    let Some(vendor) = vendor_fonts() else {
        eprintln!("skipped: run tools/fetch-fonts first for the bundled fonts");
        return;
    };
    let mut all = Vec::new();
    for (system, bundled) in [
        ("arial.ttf", "arimo/Arimo[wght].ttf"),
        ("calibri.ttf", "carlito/Carlito-Regular.ttf"),
        ("times.ttf", "tinos/Tinos-Regular.ttf"),
        ("cambria.ttc", "caladea/Caladea-Regular.ttf"),
    ] {
        all.extend(records(&vendor.join(bundled), Source::Bundled));
        match system_font(system) {
            Some(path) => all.extend(records(&path, Source::System)),
            None => eprintln!("skipped part: {system} is not installed"),
        }
    }
    for extra in [
        "inter/Inter[opsz,wght].ttf",
        "sourceserif4/SourceSerif4[opsz,wght].ttf",
        "notosans/NotoSans[wdth,wght].ttf",
    ] {
        all.extend(records(&vendor.join(extra), Source::Bundled));
    }
    let index = FontIndex::build(all, Catalogue::shipped());
    // caladea is the closest look-alike of cambria but its widths differ
    // (see substitute.rs), so it must come first without being called
    // metric compatible
    for (system, twin, same_widths) in [
        ("Arial", "Arimo", true),
        ("Calibri", "Carlito", true),
        ("Times New Roman", "Tinos", true),
        ("Cambria", "Caladea", false),
    ] {
        if index
            .family(system)
            .is_none_or(|family| family.faces.is_empty())
        {
            continue;
        }
        // a sibling from the same collection (Cambria Math) may come first
        let found: Vec<_> = similar_to(&index, system, 6)
            .into_iter()
            .filter(|similar| !similar.family.starts_with(system))
            .collect();
        assert_eq!(found[0].family, twin, "{system}: {found:?}");
        assert_eq!(
            found[0].metric_compatible, same_widths,
            "{system} and {twin}"
        );
        let a = &index
            .family(system)
            .unwrap()
            .regular()
            .unwrap()
            .facts
            .measurements;
        let b = &index
            .family(twin)
            .unwrap()
            .regular()
            .unwrap()
            .facts
            .measurements;
        assert_eq!(metric_compatible(a, b), same_widths);
        // and a font that only looks alike is not called compatible
        assert!(found.iter().skip(1).all(|other| !other.metric_compatible));
    }
}

#[test]
fn bundled_fonts_match_the_online_catalogue() {
    let Some(vendor) = vendor_fonts() else {
        eprintln!("skipped: run tools/fetch-fonts first for the bundled fonts");
        return;
    };
    let catalogue = Catalogue::shipped();
    let mut checked = 0;
    for entry in std::fs::read_dir(&vendor).unwrap().flatten() {
        let folder = entry.path();
        let dir = format!("ofl/{}", entry.file_name().to_string_lossy());
        let family = catalogue
            .families
            .iter()
            .find(|family| family.dir == dir)
            .expect("bundled family is in the catalogue");
        assert_eq!(family.license, "OFL");
        assert!(folder.join("OFL.txt").is_file(), "{dir} keeps its licence");
        for file in std::fs::read_dir(&folder).unwrap().flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if let Some(listed) = family.files.iter().find(|listed| listed.filename == name) {
                let hash = letters_fonts::download::git_blob_hash(&file.path()).unwrap();
                assert_eq!(hash, listed.sha1, "{name}");
                checked += 1;
            }
        }
    }
    assert!(checked >= 30, "{checked} files checked");
}
