//! a small xml reader and writer made for office files, copied from
//! letters-office (see docs/worklogs/core-slides.md: kept here so this crate builds while
//! that one changes; both should move into a shared crate) with the
//! presentation namespaces added.
//!
//! the reader remembers where each element sat in the original text,
//! which is what makes byte-for-byte preservation possible. prefixes are
//! normalised: elements are named `p:...`, `a:...`, `draw:...` whatever
//! prefix the file used.

mod parse;
mod write;

pub use parse::{decode_text, parse, Element, Node, XmlDocument};
pub use write::{escape_attribute, escape_text, XmlWriter};

/// namespaces office files use, with the prefix the tree names them by.
pub const NAMESPACES: &[(&str, &str)] = &[
    (
        "p",
        "http://schemas.openxmlformats.org/presentationml/2006/main",
    ),
    ("p", "http://purl.oclc.org/ooxml/presentationml/main"),
    ("a", "http://purl.oclc.org/ooxml/drawingml/main"),
    (
        "p14",
        "http://schemas.microsoft.com/office/powerpoint/2010/main",
    ),
    (
        "presentation",
        "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0",
    ),
    (
        "smil",
        "urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0",
    ),
    (
        "anim",
        "urn:oasis:names:tc:opendocument:xmlns:animation:1.0",
    ),
    ("dr3d", "urn:oasis:names:tc:opendocument:xmlns:dr3d:1.0"),
    (
        "w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ),
    ("w", "http://purl.oclc.org/ooxml/wordprocessingml/main"),
    (
        "r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ),
    (
        "r",
        "http://purl.oclc.org/ooxml/officeDocument/relationships",
    ),
    (
        "wp",
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    ),
    ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
    (
        "pic",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    ),
    (
        "mc",
        "http://schemas.openxmlformats.org/markup-compatibility/2006",
    ),
    (
        "w14",
        "http://schemas.microsoft.com/office/word/2010/wordml",
    ),
    (
        "wps",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
    ),
    (
        "wp14",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing",
    ),
    ("v", "urn:schemas-microsoft-com:vml"),
    ("o", "urn:schemas-microsoft-com:office:office"),
    ("w10", "urn:schemas-microsoft-com:office:word"),
    (
        "pr",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ),
    (
        "ct",
        "http://schemas.openxmlformats.org/package/2006/content-types",
    ),
    (
        "cp",
        "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
    ),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("xsi", "http://www.w3.org/2001/XMLSchema-instance"),
    ("xml", "http://www.w3.org/XML/1998/namespace"),
    ("office", "urn:oasis:names:tc:opendocument:xmlns:office:1.0"),
    ("style", "urn:oasis:names:tc:opendocument:xmlns:style:1.0"),
    ("text", "urn:oasis:names:tc:opendocument:xmlns:text:1.0"),
    ("table", "urn:oasis:names:tc:opendocument:xmlns:table:1.0"),
    ("draw", "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"),
    (
        "fo",
        "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0",
    ),
    (
        "svg",
        "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0",
    ),
    ("meta", "urn:oasis:names:tc:opendocument:xmlns:meta:1.0"),
    (
        "number",
        "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0",
    ),
    (
        "manifest",
        "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0",
    ),
    ("xlink", "http://www.w3.org/1999/xlink"),
    (
        "loext",
        "urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0",
    ),
];

/// the uri for one of the prefixes above, for writers declaring them.
pub fn namespace_uri(prefix: &str) -> &'static str {
    NAMESPACES
        .iter()
        .find(|(known, _)| *known == prefix)
        .map(|(_, uri)| *uri)
        .unwrap_or("")
}

fn canonical_prefix(uri: &str) -> Option<&'static str> {
    NAMESPACES
        .iter()
        .find(|(_, known)| *known == uri)
        .map(|(prefix, _)| *prefix)
}

/// the part of a qualified name after the colon.
pub fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// the part of a qualified name before the colon, if any.
pub fn prefix_of(name: &str) -> Option<&str> {
    name.split_once(':').map(|(prefix, _)| prefix)
}
