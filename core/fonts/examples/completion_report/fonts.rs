//! the test fonts: loading them, making the subset a pdf would embed, and
//! choosing which may lend glyphs in a scenario.

use super::*;

pub(crate) fn load(path: &Path, source: Source) -> Option<Font> {
    let bytes = std::fs::read(path).ok()?;
    let face = ttf_parser::Face::parse(&bytes, 0).ok()?;
    let facts = letters_fonts::facts::facts_of(&face)?;
    Some(Font {
        file: path.to_path_buf(),
        family: facts.family,
        style: facts.style_name,
        source,
        postscript: facts.postscript_name,
        bytes,
    })
}

pub(crate) fn donor(font: &Font) -> Option<DonorFile> {
    let key = format!("{:?}:{}", font.source, font.file.display()).to_lowercase();
    DonorFile::from_bytes(font.source, &key, font.bytes.clone(), 0)
}

/// every test font: the bundled set and the usual system families.
pub(crate) fn all_fonts() -> Vec<Font> {
    let mut fonts = Vec::new();
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/fonts");
    if let Ok(folders) = std::fs::read_dir(&vendor) {
        for folder in folders.flatten() {
            if let Ok(files) = std::fs::read_dir(folder.path()) {
                for file in files.flatten() {
                    let path = file.path();
                    if path.extension().is_some_and(|e| e == "ttf") {
                        fonts.extend(load(&path, Source::Bundled));
                    }
                }
            }
        }
    }
    let windows = std::env::var_os("WINDIR")
        .map(|w| PathBuf::from(w).join("Fonts"))
        .unwrap_or_else(|| PathBuf::from("C:/Windows/Fonts"));
    let wanted = [
        "arial",
        "arialbd",
        "ariali",
        "arialbi",
        "ARIALN",
        "times",
        "timesbd",
        "timesi",
        "timesbi",
        "calibri",
        "calibrib",
        "calibril",
        "calibrii",
        "georgia",
        "georgiab",
        "verdana",
        "verdanab",
        "segoeui",
        "segoeuib",
        "segoeuil",
        "segoeuisl",
        "seguisb",
        "tahoma",
        "tahomabd",
        "cambriab",
        "cambriai",
        "constan",
        "corbel",
        "candara",
        "cour",
        "consola",
    ];
    for name in wanted {
        let path = windows.join(format!("{name}.ttf"));
        fonts.extend(load(&path, Source::System));
    }
    fonts.sort_by(|a, b| (&a.family, &a.style).cmp(&(&b.family, &b.style)));
    fonts
}

/// the subset a pdf would embed: the glyphs of `text` kept, every other
/// glyph emptied, glyph numbers unchanged, and the pdf's name for it.
pub(crate) fn make_subset(font: &Font, text: &str) -> SubsetFont {
    let face = ttf_parser::Face::parse(&font.bytes, 0).expect("test fonts parse");
    let mut map = BTreeMap::new();
    let mut keep = std::collections::BTreeSet::new();
    for c in text.chars() {
        if let Some(glyph) = face.glyph_index(c) {
            map.insert(c, glyph.0);
            keep.insert(glyph.0);
        }
    }
    let bytes = letters_fonts::subset::subset_truetype(&font.bytes, 0, &keep).expect("subset");
    SubsetFont::from_font_bytes(&bytes, 0, Some(&map))
        .expect("subset reads")
        .with_name(&format!("ABCDEF+{}", font.postscript))
}

pub(crate) fn label(font: &Font) -> String {
    if font.style.eq_ignore_ascii_case("regular") {
        font.family.clone()
    } else {
        format!("{} {}", font.family, font.style)
    }
}

pub(crate) fn find<'a>(fonts: &'a [Font], family: &str, style: &str) -> Option<&'a Font> {
    fonts
        .iter()
        .find(|f| f.family.eq_ignore_ascii_case(family) && f.style.eq_ignore_ascii_case(style))
}

pub(crate) fn pool_of(fonts: &[Font], keep: impl Fn(&Font) -> bool) -> DonorPool {
    DonorPool {
        donors: fonts.iter().filter(|f| keep(f)).filter_map(donor).collect(),
    }
}
