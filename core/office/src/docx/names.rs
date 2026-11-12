//! the fixed strings of the docx format: relationship kinds, content
//! types and the namespace declarations a written part needs.

/// relationship kinds are urls; only the last segment matters (it is the
/// same in the transitional and strict flavours of the format).
pub fn relationship_kind(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or(url)
}

pub const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
pub const REL_CORE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";

pub const KIND_DOCUMENT: &str = "officeDocument";
pub const KIND_STYLES: &str = "styles";
pub const KIND_NUMBERING: &str = "numbering";
pub const KIND_FOOTNOTES: &str = "footnotes";
pub const KIND_ENDNOTES: &str = "endnotes";
pub const KIND_COMMENTS: &str = "comments";
pub const KIND_HEADER: &str = "header";
pub const KIND_FOOTER: &str = "footer";
pub const KIND_HYPERLINK: &str = "hyperlink";
pub const KIND_IMAGE: &str = "image";
pub const KIND_SETTINGS: &str = "settings";
pub const KIND_THEME: &str = "theme";
pub const KIND_CORE: &str = "core-properties";
pub const KIND_APP: &str = "extended-properties";

/// relationship kinds whose targets the writer produces itself. any
/// other relationship is kept as it was.
pub const REWRITTEN_KINDS: &[&str] = &[
    KIND_STYLES,
    KIND_NUMBERING,
    KIND_FOOTNOTES,
    KIND_ENDNOTES,
    KIND_COMMENTS,
    KIND_HEADER,
    KIND_FOOTER,
    KIND_HYPERLINK,
    KIND_IMAGE,
];

pub const CT_MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
pub const CT_STYLES: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
pub const CT_NUMBERING: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";
pub const CT_FOOTNOTES: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml";
pub const CT_ENDNOTES: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml";
pub const CT_COMMENTS: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml";
pub const CT_HEADER: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
pub const CT_FOOTER: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
pub const CT_SETTINGS: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
pub const CT_CORE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
pub const CT_APP: &str = "application/vnd.openxmlformats-officedocument.extended-properties+xml";
pub const CT_RELS: &str = "application/vnd.openxmlformats-package.relationships+xml";

/// the namespaces every story part (document, header, notes...) declares
/// when it has no original root to copy. word reads parts that declare
/// these without complaint.
pub const STORY_NAMESPACES: &[(&str, &str)] = &[
    (
        "xmlns:w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ),
    (
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ),
    (
        "xmlns:wp",
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    ),
    (
        "xmlns:a",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    ),
    (
        "xmlns:pic",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    ),
    (
        "xmlns:mc",
        "http://schemas.openxmlformats.org/markup-compatibility/2006",
    ),
    // groups are written for word 2010 and later inside a compatibility
    // wrapper that names this prefix
    (
        "xmlns:wpg",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup",
    ),
    (
        "xmlns:w14",
        "http://schemas.microsoft.com/office/word/2010/wordml",
    ),
    (
        "xmlns:wps",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
    ),
    ("xmlns:v", "urn:schemas-microsoft-com:vml"),
    ("xmlns:o", "urn:schemas-microsoft-com:office:office"),
    ("xmlns:w10", "urn:schemas-microsoft-com:office:word"),
];
