//! the order child elements must come in.
//!
//! wordprocessingml is strict about order: `w:jc` before `w:ind` in a
//! paragraph's properties is an error word answers with "unreadable
//! content". properties the model writes and properties kept as xml are
//! mixed, so each is given its rank in the schema's sequence and the lot
//! is sorted before writing.

use crate::xml::{local_name, prefix_of};

pub const PARAGRAPH: &[&str] = &[
    "pStyle",
    "keepNext",
    "keepLines",
    "pageBreakBefore",
    "framePr",
    "widowControl",
    "numPr",
    "suppressLineNumbers",
    "pBdr",
    "shd",
    "tabs",
    "suppressAutoHyphens",
    "kinsoku",
    "wordWrap",
    "overflowPunct",
    "topLinePunct",
    "autoSpaceDE",
    "autoSpaceDN",
    "bidi",
    "adjustRightInd",
    "snapToGrid",
    "spacing",
    "ind",
    "contextualSpacing",
    "mirrorIndents",
    "suppressOverlap",
    "jc",
    "textDirection",
    "textAlignment",
    "textboxTightWrap",
    "outlineLvl",
    "divId",
    "cnfStyle",
    "rPr",
    "sectPr",
    "pPrChange",
];

pub const RUN: &[&str] = &[
    "ins",
    "del",
    "moveFrom",
    "moveTo",
    "rStyle",
    "rFonts",
    "b",
    "bCs",
    "i",
    "iCs",
    "caps",
    "smallCaps",
    "strike",
    "dstrike",
    "outline",
    "shadow",
    "emboss",
    "imprint",
    "noProof",
    "snapToGrid",
    "vanish",
    "webHidden",
    "color",
    "spacing",
    "w",
    "kern",
    "position",
    "sz",
    "szCs",
    "highlight",
    "u",
    "effect",
    "bdr",
    "shd",
    "fitText",
    "vertAlign",
    "rtl",
    "cs",
    "em",
    "lang",
    "eastAsianLayout",
    "specVanish",
    "oMath",
    "rPrChange",
];

pub const SECTION: &[&str] = &[
    "headerReference",
    "footerReference",
    "footnotePr",
    "endnotePr",
    "type",
    "pgSz",
    "pgMar",
    "paperSrc",
    "pgBorders",
    "lnNumType",
    "pgNumType",
    "cols",
    "formProt",
    "vAlign",
    "noEndnote",
    "titlePg",
    "textDirection",
    "bidi",
    "rtlGutter",
    "docGrid",
    "printerSettings",
    "sectPrChange",
];

pub const TABLE: &[&str] = &[
    "tblStyle",
    "tblpPr",
    "tblOverlap",
    "bidiVisual",
    "tblStyleRowBandSize",
    "tblStyleColBandSize",
    "tblW",
    "jc",
    "tblCellSpacing",
    "tblInd",
    "tblBorders",
    "shd",
    "tblLayout",
    "tblCellMar",
    "tblLook",
    "tblCaption",
    "tblDescription",
    "tblPrChange",
];

pub const ROW: &[&str] = &[
    "cnfStyle",
    "divId",
    "gridBefore",
    "gridAfter",
    "wBefore",
    "wAfter",
    "cantSplit",
    "trHeight",
    "tblHeader",
    "tblCellSpacing",
    "jc",
    "hidden",
    "ins",
    "del",
    "trPrChange",
];

pub const CELL: &[&str] = &[
    "cnfStyle",
    "tcW",
    "gridSpan",
    "hMerge",
    "vMerge",
    "tcBorders",
    "shd",
    "noWrap",
    "tcMar",
    "textDirection",
    "tcFitText",
    "vAlign",
    "hideMark",
    "headers",
    "cellIns",
    "cellDel",
    "cellMerge",
    "tcPrChange",
];

pub const STYLE: &[&str] = &[
    "name",
    "aliases",
    "basedOn",
    "next",
    "link",
    "autoRedefine",
    "hidden",
    "uiPriority",
    "semiHidden",
    "unhideWhenUsed",
    "qFormat",
    "locked",
    "personal",
    "personalCompose",
    "personalReply",
    "rsid",
    "pPr",
    "rPr",
    "tblPr",
    "trPr",
    "tcPr",
    "tblStylePr",
];

pub const LIST_DEFINITION: &[&str] = &[
    "nsid",
    "multiLevelType",
    "tmpl",
    "name",
    "styleLink",
    "numStyleLink",
    "lvl",
];

pub const LIST_LEVEL: &[&str] = &[
    "start",
    "numFmt",
    "lvlRestart",
    "pStyle",
    "isLgl",
    "suff",
    "lvlText",
    "lvlPicBulletId",
    "legacy",
    "lvlJc",
    "pPr",
    "rPr",
];

pub const SETTINGS: &[&str] = &[
    "writeProtection",
    "view",
    "zoom",
    "removePersonalInformation",
    "removeDateAndTime",
    "doNotDisplayPageBoundaries",
    "displayBackgroundShape",
    "printPostScriptOverText",
    "printFractionalCharacterWidth",
    "printFormsData",
    "embedTrueTypeFonts",
    "embedSystemFonts",
    "saveSubsetFonts",
    "saveFormsData",
    "mirrorMargins",
    "alignBordersAndEdges",
    "bordersDoNotSurroundHeader",
    "bordersDoNotSurroundFooter",
    "gutterAtTop",
    "hideSpellingErrors",
    "hideGrammaticalErrors",
    "activeWritingStyle",
    "proofState",
    "formsDesign",
    "attachedTemplate",
    "linkStyles",
    "stylePaneFormatFilter",
    "stylePaneSortMethod",
    "documentType",
    "mailMerge",
    "revisionView",
    "trackRevisions",
    "doNotTrackMoves",
    "doNotTrackFormatting",
    "documentProtection",
    "autoFormatOverride",
    "styleLockTheme",
    "styleLockQFSet",
    "defaultTabStop",
    "autoHyphenation",
    "consecutiveHyphenLimit",
    "hyphenationZone",
    "doNotHyphenateCaps",
    "showEnvelope",
    "summaryLength",
    "clickAndTypeStyle",
    "defaultTableStyle",
    "evenAndOddHeaders",
    "bookFoldRevPrinting",
    "bookFoldPrinting",
    "bookFoldPrintingSheets",
    "drawingGridHorizontalSpacing",
    "drawingGridVerticalSpacing",
    "displayHorizontalDrawingGridEvery",
    "displayVerticalDrawingGridEvery",
    "doNotUseMarginsForDrawingGridOrigin",
    "drawingGridHorizontalOrigin",
    "drawingGridVerticalOrigin",
    "doNotShadeFormData",
    "noPunctuationKerning",
    "characterSpacingControl",
    "printTwoOnOne",
    "strictFirstAndLastChars",
    "noLineBreaksAfter",
    "noLineBreaksBefore",
    "savePreviewPicture",
    "doNotValidateAgainstSchema",
    "saveInvalidXml",
    "ignoreMixedContent",
    "alwaysShowPlaceholderText",
    "doNotDemarcateInvalidXml",
    "saveXmlDataOnly",
    "useXSLTWhenSaving",
    "saveThroughXslt",
    "showXMLTags",
    "alwaysMergeEmptyNamespace",
    "updateFields",
    "hdrShapeDefaults",
    "footnotePr",
    "endnotePr",
    "compat",
    "docVars",
    "rsids",
    "mathPr",
    "attachedSchema",
    "themeFontLang",
    "clrSchemeMapping",
    "doNotIncludeSubdocsInStats",
    "doNotAutoCompressPictures",
    "forceUpgrade",
    "captions",
    "readModeInkLockDown",
    "smartTagType",
    "schemaLibrary",
    "shapeDefaults",
    "doNotEmbedSmartTags",
    "decimalSymbol",
    "listSeparator",
];

/// child elements collected in any order and written in schema order.
pub struct Ordered<'t> {
    order: &'t [&'t str],
    items: Vec<(usize, String)>,
    last_rank: usize,
}

impl<'t> Ordered<'t> {
    pub fn new(order: &'t [&'t str]) -> Ordered<'t> {
        Ordered {
            order,
            items: Vec::new(),
            last_rank: 0,
        }
    }

    /// the position of a `w:` element in the sequence. elements from other
    /// namespaces (word's extensions, which files mark as ignorable) and
    /// unknown names stay next to whatever came before them.
    fn rank(&self, qualified: &str) -> Option<usize> {
        if prefix_of(qualified) != Some("w") {
            return None;
        }
        let local = local_name(qualified);
        self.order.iter().position(|name| *name == local)
    }

    /// an element the writer produced, by name.
    pub fn push(&mut self, qualified: &str, xml: String) {
        let rank = self.rank(qualified).unwrap_or(self.last_rank);
        self.last_rank = rank;
        self.items.push((rank, xml));
    }

    /// an element kept as original xml; its name is read from the xml.
    pub fn push_raw(&mut self, xml: &str) {
        let name = first_tag_name(xml);
        self.push(name, xml.to_string());
    }

    pub fn finish(mut self) -> String {
        // a stable sort keeps elements of equal rank in the order given
        self.items.sort_by_key(|(rank, _)| *rank);
        self.items.into_iter().map(|(_, xml)| xml).collect()
    }
}

/// the name of the first tag in a piece of xml ("w:jc" for `<w:jc .../>`).
pub fn first_tag_name(xml: &str) -> &str {
    let start = xml.find('<').map(|index| index + 1).unwrap_or(0);
    let rest = &xml[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(rest.len());
    &rest[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_into_schema_order_and_keeps_unknowns_in_place() {
        let mut ordered = Ordered::new(PARAGRAPH);
        ordered.push("w:jc", "<w:jc/>".into());
        ordered.push("w:pStyle", "<w:pStyle/>".into());
        ordered.push_raw("<w:ind w:leftChars=\"2\"/>");
        assert_eq!(
            ordered.finish(),
            "<w:pStyle/><w:ind w:leftChars=\"2\"/><w:jc/>"
        );
    }

    #[test]
    fn tag_names() {
        assert_eq!(first_tag_name("<w:jc w:val=\"x\"/>"), "w:jc");
        assert_eq!(first_tag_name("<w14:glow>"), "w14:glow");
    }
}
