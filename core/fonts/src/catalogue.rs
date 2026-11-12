//! the online library: every family of the google/fonts repository at
//! one pinned commit, with its styles, files, sizes and hashes. the
//! catalogue ships inside the app (data/catalogue.z, made by
//! examples/build_catalogue.rs), so searching it needs no network; only
//! downloading a family's files does (download.rs).

use std::sync::OnceLock;

use serde::Serialize;

use crate::coverage::writing_of_subsets;
use crate::model::{Axis, Category};

/// the compressed catalogue, about 260 kB.
const SHIPPED: &[u8] = include_bytes!("../data/catalogue.z");

/// the most the catalogue may grow to when unpacked.
const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;

/// one file of a family: a font or its licence.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueFile {
    pub filename: String,
    /// "normal" or "italic" (empty for a licence file)
    pub style: String,
    pub weight: u16,
    /// the git blob hash of the file at the pinned commit
    pub sha1: String,
    pub size: u64,
    pub postscript_name: String,
    pub full_name: String,
}

/// one family of the online library.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueFamily {
    /// the folder in the repository: "ofl/inter"
    pub dir: String,
    pub name: String,
    pub designer: String,
    /// "OFL", "APACHE2" or "UFL"
    pub license: String,
    pub categories: Vec<Category>,
    pub date_added: String,
    pub subsets: Vec<String>,
    pub axes: Vec<Axis>,
    /// the script the family is mainly for ("Arab"), empty for latin
    pub primary_script: String,
    pub license_file: Option<CatalogueFile>,
    pub files: Vec<CatalogueFile>,
}

impl CatalogueFamily {
    pub fn category(&self) -> Category {
        self.categories
            .first()
            .copied()
            .unwrap_or(Category::SansSerif)
    }

    /// what downloading the whole family puts on disk, licence included.
    pub fn download_bytes(&self) -> u64 {
        self.files
            .iter()
            .chain(&self.license_file)
            .map(|file| file.size)
            .sum()
    }

    pub fn writing(&self) -> Vec<String> {
        writing_of_subsets(&self.subsets)
    }

    /// the file closest to regular upright, which a preview fetches alone.
    pub fn regular_file(&self) -> Option<&CatalogueFile> {
        self.files
            .iter()
            .min_by_key(|file| (file.style != "normal", (i32::from(file.weight) - 400).abs()))
    }
}

/// the whole online library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalogue {
    /// the google/fonts commit every file is pinned to
    pub commit: String,
    pub families: Vec<CatalogueFamily>,
}

impl Catalogue {
    /// the catalogue shipped with the app, read once.
    pub fn shipped() -> &'static Catalogue {
        static CATALOGUE: OnceLock<Catalogue> = OnceLock::new();
        CATALOGUE.get_or_init(|| Catalogue::unpack(SHIPPED).unwrap_or_default())
    }

    /// the catalogue in its compressed form, or None when it is damaged.
    pub fn unpack(packed: &[u8]) -> Option<Catalogue> {
        let text =
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(packed, MAX_TEXT_BYTES).ok()?;
        Catalogue::parse(&String::from_utf8(text).ok()?)
    }

    /// reads the line format examples/build_catalogue.rs writes.
    pub fn parse(text: &str) -> Option<Catalogue> {
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next()?.split('\t').collect();
        if header.first() != Some(&"letters-font-catalogue") || header.get(1) != Some(&"1") {
            return None;
        }
        let mut catalogue = Catalogue {
            commit: header.get(2)?.to_string(),
            families: Vec::new(),
        };
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.first() {
                Some(&"F") => catalogue.families.push(family(&fields)?),
                Some(&"L") => {
                    catalogue.families.last_mut()?.license_file = Some(file(&fields, true)?)
                }
                Some(&"S") => catalogue
                    .families
                    .last_mut()?
                    .files
                    .push(file(&fields, false)?),
                _ => {}
            }
        }
        Some(catalogue)
    }

    pub fn family(&self, name: &str) -> Option<&CatalogueFamily> {
        let wanted = crate::index::family_key(name);
        self.families
            .iter()
            .find(|family| crate::index::family_key(&family.name) == wanted)
    }
}

fn split_list(text: &str) -> Vec<String> {
    text.split(',')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn family(fields: &[&str]) -> Option<CatalogueFamily> {
    let field = |index: usize| fields.get(index).copied().unwrap_or("");
    Some(CatalogueFamily {
        dir: fields.get(1)?.to_string(),
        name: fields.get(2)?.to_string(),
        designer: field(3).to_string(),
        license: field(4).to_string(),
        categories: field(5).split(',').filter_map(category).collect(),
        date_added: field(6).to_string(),
        subsets: split_list(field(7)),
        axes: field(8).split(',').filter_map(axis).collect(),
        primary_script: field(9).to_string(),
        license_file: None,
        files: Vec::new(),
    })
}

fn category(name: &str) -> Option<Category> {
    match name {
        "SANS_SERIF" => Some(Category::SansSerif),
        "SERIF" => Some(Category::Serif),
        "DISPLAY" => Some(Category::Display),
        "HANDWRITING" => Some(Category::Handwriting),
        "MONOSPACE" => Some(Category::Monospace),
        _ => None,
    }
}

fn axis(text: &str) -> Option<Axis> {
    let mut parts = text.split(':');
    let tag = parts.next()?.to_string();
    let min: f32 = parts.next()?.parse().ok()?;
    let max: f32 = parts.next()?.parse().ok()?;
    // the catalogue doesn't record defaults; regular weight is the usual one
    let default = if tag == "wght" {
        400f32.clamp(min, max)
    } else {
        min
    };
    Some(Axis {
        tag,
        min,
        default,
        max,
    })
}

fn file(fields: &[&str], license: bool) -> Option<CatalogueFile> {
    let field = |index: usize| fields.get(index).copied().unwrap_or("");
    if license {
        return Some(CatalogueFile {
            filename: fields.get(1)?.to_string(),
            style: String::new(),
            weight: 0,
            sha1: fields.get(2)?.to_string(),
            size: fields.get(3)?.parse().ok()?,
            postscript_name: String::new(),
            full_name: String::new(),
        });
    }
    Some(CatalogueFile {
        filename: fields.get(1)?.to_string(),
        style: field(2).to_string(),
        weight: field(3).parse().unwrap_or(400),
        sha1: fields.get(4)?.to_string(),
        size: fields.get(5)?.parse().ok()?,
        postscript_name: field(6).to_string(),
        full_name: field(7).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_catalogue_is_large_and_pinned() {
        let catalogue = Catalogue::shipped();
        assert_eq!(catalogue.commit.len(), 40);
        assert!(
            catalogue.families.len() > 1500,
            "{} families",
            catalogue.families.len()
        );
        let inter = catalogue.family("Inter").expect("Inter is in the library");
        assert_eq!(inter.license, "OFL");
        assert_eq!(inter.category(), Category::SansSerif);
        assert!(inter
            .axes
            .iter()
            .any(|axis| axis.tag == "wght" && axis.max == 900.0));
        assert!(inter.writing().contains(&"vietnamese".to_string()));
        assert!(inter
            .license_file
            .as_ref()
            .is_some_and(|file| file.filename == "OFL.txt"));
        let carlito = catalogue.family("carlito").unwrap();
        assert_eq!(carlito.files.len(), 4);
        assert_eq!(
            carlito.regular_file().unwrap().filename,
            "Carlito-Regular.ttf"
        );
        assert!(carlito.download_bytes() > 2_000_000);
        // every family has a licence the app may redistribute
        assert!(catalogue
            .families
            .iter()
            .all(|family| matches!(family.license.as_str(), "OFL" | "APACHE2" | "UFL")));
    }

    #[test]
    fn damaged_catalogues_are_refused() {
        assert!(Catalogue::parse("something else").is_none());
        assert!(Catalogue::unpack(b"not zlib").is_none());
        let small = Catalogue::parse("letters-font-catalogue\t1\tabc\nF\tofl/x\tX\t\tOFL\tSERIF\nS\tX.ttf\tnormal\t400\tff\t12\tX\tX Regular").unwrap();
        assert_eq!(small.families[0].files[0].size, 12);
        assert_eq!(small.families[0].category(), Category::Serif);
        // a line before any family is ignored rather than trusted
        assert!(Catalogue::parse("letters-font-catalogue\t1\tabc\nS\tX.ttf").is_none());
    }
}
