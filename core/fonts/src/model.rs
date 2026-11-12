//! the shapes every part of the font library shares: where a font comes
//! from, what kind of font it is, what its licence allows, and the facts
//! read from one face.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// where a font comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// installed on this computer; used where it is, never copied
    System,
    /// a file the person added, copied into Letters' own folder
    Custom,
    /// shipped with Letters (open licences only)
    Bundled,
    /// downloaded from the online library into Letters' own folder
    Library,
    /// in the online library and not downloaded yet
    Downloadable,
}

impl Source {
    /// true when the font can be used right now, without a download.
    pub fn is_on_this_computer(self) -> bool {
        self != Source::Downloadable
    }
}

/// what a font looks like, in the words a student would use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Serif,
    SansSerif,
    Monospace,
    Display,
    Handwriting,
    /// symbols and pictures instead of letters
    Symbol,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Serif => "Serif",
            Category::SansSerif => "Sans Serif",
            Category::Monospace => "Monospaced",
            Category::Display => "Display",
            Category::Handwriting => "Handwriting",
            Category::Symbol => "Symbols",
        }
    }

    /// the words someone might type to mean this category.
    pub fn words(self) -> &'static [&'static str] {
        match self {
            Category::Serif => &["serif", "serifs"],
            Category::SansSerif => &["sans", "sans-serif", "sansserif"],
            Category::Monospace => &["mono", "monospace", "monospaced", "code", "typewriter"],
            Category::Display => &["display", "decorative", "headline"],
            Category::Handwriting => &["handwriting", "handwritten", "script", "hand", "cursive"],
            Category::Symbol => &["symbol", "symbols", "dingbats", "icons"],
        }
    }

    pub const ALL: [Category; 6] = [
        Category::Serif,
        Category::SansSerif,
        Category::Monospace,
        Category::Display,
        Category::Handwriting,
        Category::Symbol,
    ];
}

/// what the font's licence lets a document do with it, from the OS/2
/// table's fsType bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmbeddingPermission {
    /// may be embedded and even installed by whoever receives the document
    Installable,
    /// may be embedded and the text edited
    Editable,
    /// may be embedded for reading and printing only
    PreviewAndPrint,
    /// may not be embedded at all
    Restricted,
}

/// the embedding rules of one face, with the sentence the ui shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Embedding {
    pub permission: EmbeddingPermission,
    /// false when the licence says the whole font must be embedded
    pub subsetting: bool,
    /// true when only the font's bitmaps (not its outlines) may be embedded
    pub bitmap_only: bool,
}

impl Embedding {
    /// what an open font allows: everything.
    pub const OPEN: Embedding = Embedding {
        permission: EmbeddingPermission::Installable,
        subsetting: true,
        bitmap_only: false,
    };

    /// reads the OS/2 fsType bits. the lowest four bits are one choice
    /// (the most permissive set bit wins when a font sets several, as the
    /// OpenType spec says), bits 8 and 9 are extra rules.
    pub fn from_fs_type(fs_type: u16) -> Embedding {
        let permission = match fs_type & 0x000f {
            0 => EmbeddingPermission::Installable,
            bits if bits & 0x8 != 0 => EmbeddingPermission::Editable,
            bits if bits & 0x4 != 0 => EmbeddingPermission::PreviewAndPrint,
            _ => EmbeddingPermission::Restricted,
        };
        Embedding {
            permission,
            subsetting: fs_type & 0x0100 == 0,
            bitmap_only: fs_type & 0x0200 != 0,
        }
    }

    /// true when the outlines may go into a pdf or a document at all.
    pub fn allows_embedding(&self) -> bool {
        self.permission != EmbeddingPermission::Restricted && !self.bitmap_only
    }

    /// one plain sentence for the person (or two when subsetting is barred).
    pub fn sentence(&self) -> String {
        let main = if !self.allows_embedding() {
            "This font’s license doesn’t allow embedding; others will see a similar font unless they have it."
        } else if self.permission == EmbeddingPermission::PreviewAndPrint {
            "This font can be embedded in PDFs and documents for reading and printing, but others may not be able to edit text in it."
        } else {
            "This font can be embedded in PDFs and documents."
        };
        if self.allows_embedding() && !self.subsetting {
            format!("{main} The whole font is embedded, which makes files larger.")
        } else {
            main.to_string()
        }
    }
}

/// one variation axis of a variable font, such as weight from 100 to 900.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Axis {
    /// the four letter tag: "wght", "wdth", "opsz", "ital", "slnt"...
    pub tag: String,
    pub min: f32,
    pub default: f32,
    pub max: f32,
}

/// how the glyphs are drawn inside the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outlines {
    /// quadratic outlines in a glyf table (most .ttf files)
    TrueType,
    /// cubic outlines in a CFF or CFF2 table (most .otf files)
    Cff,
    /// pictures only (emoji and some old fonts)
    Bitmap,
}

/// measurements used to find fonts that look alike or take up the same
/// room. lengths are thousandths of the em, as pdf widths are.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurements {
    pub units_per_em: u16,
    pub ascender: i16,
    pub descender: i16,
    pub x_height: i16,
    pub cap_height: i16,
    /// the ten panose numbers (all zero when the font has none)
    pub panose: [u8; 10],
    /// the advance of each printable ascii character (space to tilde), 0
    /// when the font lacks it. same widths mean the same line breaks
    pub ascii_widths: Vec<u16>,
    /// thick strokes divided by thin ones, measured on the letter "o"
    pub contrast: f32,
    /// the width of the vertical stroke of "o", thousandths of the em
    pub stem: f32,
}

/// everything Letters knows about one face of a font.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceFacts {
    pub family: String,
    /// "Regular", "Bold Italic", "Condensed Light"...
    pub style_name: String,
    pub full_name: String,
    pub postscript_name: String,
    /// 100 (thin) to 900 (black); 400 is regular, 700 bold
    pub weight: u16,
    /// 1 (ultra condensed) to 9 (ultra expanded); 5 is normal
    pub width: u16,
    pub italic: bool,
    pub oblique: bool,
    pub monospace: bool,
    pub axes: Vec<Axis>,
    /// writing systems and languages the font can draw (ids from coverage.rs)
    pub writing: Vec<String>,
    /// every character the font maps, as sorted, merged ranges of code
    /// points, so "can it draw this?" needs no second look at the file
    pub coverage: Vec<(u32, u32)>,
    pub category: Category,
    pub embedding: Embedding,
    pub version: String,
    pub glyph_count: u16,
    pub outlines: Outlines,
    pub measurements: Measurements,
}

/// one face on this computer (any source but downloadable), as the index
/// keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceRecord {
    /// stable while the file is unchanged: source, path and index
    pub key: String,
    pub source: Source,
    pub path: PathBuf,
    /// which font in a collection (.ttc); 0 for a single font
    pub index: u32,
    pub facts: FaceFacts,
    /// for library and bundled fonts: the licence named by the licence file beside them
    pub license: Option<String>,
}

impl FaceRecord {
    pub fn make_key(source: Source, path: &std::path::Path, index: u32) -> String {
        format!("{source:?}:{}#{index}", path.display()).to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_type_bits_become_permissions() {
        assert_eq!(
            Embedding::from_fs_type(0).permission,
            EmbeddingPermission::Installable
        );
        assert_eq!(
            Embedding::from_fs_type(0x2).permission,
            EmbeddingPermission::Restricted
        );
        assert_eq!(
            Embedding::from_fs_type(0x4).permission,
            EmbeddingPermission::PreviewAndPrint
        );
        assert_eq!(
            Embedding::from_fs_type(0x8).permission,
            EmbeddingPermission::Editable
        );
        // several bits set: the most permissive wins
        assert_eq!(
            Embedding::from_fs_type(0xc).permission,
            EmbeddingPermission::Editable
        );
        assert!(!Embedding::from_fs_type(0x0100).subsetting);
        assert!(Embedding::from_fs_type(0x0200).bitmap_only);
    }

    #[test]
    fn embedding_sentences_are_plain() {
        assert_eq!(
            Embedding::from_fs_type(0).sentence(),
            "This font can be embedded in PDFs and documents."
        );
        assert_eq!(
            Embedding::from_fs_type(0x2).sentence(),
            "This font’s license doesn’t allow embedding; others will see a similar font unless they have it."
        );
        assert!(Embedding::from_fs_type(0x4)
            .sentence()
            .contains("reading and printing"));
        assert!(Embedding::from_fs_type(0x0108)
            .sentence()
            .ends_with("which makes files larger."));
        // bitmap only means no outlines may travel, which is no embedding for us
        assert!(!Embedding::from_fs_type(0x0200).allows_embedding());
    }
}
