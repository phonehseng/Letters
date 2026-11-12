//! the table of editor features and how exactly each format keeps them:
//! the single source the interface reads to decide what strict word mode
//! offers.

use serde::Serialize;

/// how well a format keeps an editor feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Support {
    /// exact in word files and in odt files
    Both,
    /// exact in word files only
    WordExact,
    /// exact in odt files only
    OdtExact,
    /// kept in both, but not exactly
    Approximate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub key: &'static str,
    pub label: &'static str,
    pub support: Support,
    /// what "approximate" or "exact in one format" means here
    pub note: &'static str,
}

impl Feature {
    /// offered in strict word mode.
    pub fn word_exact(&self) -> bool {
        matches!(self.support, Support::Both | Support::WordExact)
    }
}

/// every editor feature and how each format keeps it: the single source
/// of truth for strict word mode.
pub fn feature_matrix() -> Vec<Feature> {
    use Support::*;
    let feature = |key, label, support, note| Feature {
        key,
        label,
        support,
        note,
    };
    vec![
        feature("bold", "Bold, italic, underline, strike", Both, ""),
        feature("superscript", "Superscript and subscript", Both, ""),
        feature(
            "fontFamily",
            "Font and size",
            Both,
            "fonts must be installed or embedded to look the same",
        ),
        feature("color", "Text colour", Both, ""),
        feature(
            "highlight",
            "Highlighter",
            WordExact,
            "word has 16 highlighter colours; odt any colour",
        ),
        feature(
            "highlightAnyColour",
            "Highlight in any colour",
            OdtExact,
            "word uses the nearest of its 16 colours",
        ),
        feature("smallCaps", "Small capitals and all capitals", Both, ""),
        feature("alignment", "Alignment and justification", Both, ""),
        feature("indents", "Indents, first-line and hanging", Both, ""),
        feature("spacing", "Paragraph and line spacing", Both, ""),
        feature("tabs", "Tab stops with leaders", Both, ""),
        feature("headings", "Headings 1 to 6 and the outline", Both, ""),
        feature("styles", "Named styles with inheritance", Both, ""),
        feature(
            "lists",
            "Bulleted, numbered and multi-level lists",
            Both,
            "",
        ),
        feature(
            "listFormats",
            "Unusual list numbering (circled, full-width...)",
            Approximate,
            "each format has its own set",
        ),
        feature(
            "tables",
            "Tables with merged cells, borders and shading",
            Both,
            "",
        ),
        feature("footnotes", "Footnotes and endnotes", Both, ""),
        feature("comments", "Comments on a range", Both, ""),
        feature("trackChanges", "Tracked insertions and deletions", Both, ""),
        feature(
            "trackFormatting",
            "Tracked formatting changes",
            WordExact,
            "kept as is in word files, lost in odt",
        ),
        feature(
            "headersFooters",
            "Headers and footers, first page, odd and even",
            Both,
            "",
        ),
        feature(
            "sections",
            "Sections with their own page setup",
            Both,
            "odt starts each on a new page",
        ),
        feature(
            "continuousSections",
            "Section changes in the middle of a page",
            Both,
            "odt carries on with a column section while the page size stays the same",
        ),
        feature("columns", "Text columns", Both, ""),
        feature("pageNumbers", "Page number and page count fields", Both, ""),
        feature(
            "tableOfContents",
            "Table of contents",
            WordExact,
            "odt keeps its own as is; a new one is word only",
        ),
        feature("inlinePictures", "Pictures in the line", Both, ""),
        feature(
            "floatingPictures",
            "Floating pictures with wrapping",
            Both,
            "",
        ),
        feature(
            "pictureCrop",
            "Cropped pictures",
            WordExact,
            "odt keeps the whole picture",
        ),
        feature(
            "pictureRotation",
            "Rotated pictures and text boxes",
            WordExact,
            "odt shows them unrotated",
        ),
        feature("textBoxes", "Text boxes", Both, ""),
        feature(
            "textBoxAutoHeight",
            "Text boxes that grow with their text",
            Both,
            "",
        ),
        feature("bookmarks", "Bookmarks and links", Both, ""),
        feature(
            "inlinePageBreak",
            "A page break inside a line",
            WordExact,
            "odt breaks the line instead",
        ),
        feature(
            "equations",
            "Equations",
            Approximate,
            "kept in the format they came from; text only in the other",
        ),
        feature(
            "contentControls",
            "Content controls and forms",
            WordExact,
            "kept in word files; text only in odt",
        ),
        feature(
            "shapes",
            "Shapes: rectangles, ovals, arrows, callouts, stars",
            Both,
            "",
        ),
        feature(
            "unusualShapes",
            "Outlines opendocument has no name for",
            WordExact,
            "odt files use libreoffice's own name for word's outline",
        ),
        feature(
            "freeformPaths",
            "Free outlines drawn point by point",
            Both,
            "",
        ),
        feature(
            "shapeLines",
            "Lines with dashes, caps and joins",
            Approximate,
            "word's short dashes use the nearest odt dash",
        ),
        feature(
            "arrowheads",
            "Arrowheads",
            Approximate,
            "odt sizes them from the line's width",
        ),
        feature("gradients", "Two-colour gradient fills", Both, ""),
        feature(
            "multiColourGradients",
            "Gradients with more than two colours",
            WordExact,
            "libreoffice shows all colours, other odt programs two",
        ),
        feature("pictureFill", "A picture as a shape's fill", Both, ""),
        feature("transparency", "See-through fills", Both, ""),
        feature(
            "groups",
            "Groups of shapes, boxes and pictures",
            Both,
            "odt groups pass their turn to each member",
        ),
        feature(
            "connectors",
            "Connector lines",
            Approximate,
            "kept as lines; they no longer follow the objects they join",
        ),
        feature(
            "floatingTables",
            "Tables floating on the page",
            Both,
            "odt floats the table in a frame",
        ),
        feature("exactRows", "Rows of an exact height", Both, ""),
        feature("cellMargins", "Space inside cells, per cell", Both, ""),
        feature("cellTextDirection", "Text turned in a cell", Both, ""),
        feature(
            "linkedTextBoxes",
            "One story through linked text boxes",
            Both,
            "",
        ),
        feature(
            "unequalColumns",
            "Columns of different widths, with a line between",
            Approximate,
            "odt columns always fill the width between the margins",
        ),
        feature(
            "columnBreaks",
            "Column breaks",
            Approximate,
            "odt breaks only at the start of a paragraph",
        ),
        feature("canvasPages", "Pages of positioned objects only", Both, ""),
    ]
}
