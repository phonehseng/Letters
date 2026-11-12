//! features the engine finds but does not lay out yet. a document with
//! them still lays out (what can be laid out is), and each one found is
//! listed with where it is, so the interface can say "this page may not
//! look exactly like word".

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnsupportedKind {
    /// right-to-left paragraphs are detected and laid out left to right
    RightToLeftText,
    /// east asian line breaking rules and vertical text
    EastAsianLayout,
    /// shapes, charts, smartart, equations and other drawings kept as xml
    Drawing,
    Equation,
    /// a content control or other structured block kept as xml
    PreservedBlock,
    /// run content kept as xml that is not a symbol
    PreservedRun,
    /// a table of contents is shown with its last stored text
    TableOfContentsField,
    /// floating pictures wrap as their box (square), not their outline
    TightWrap,
    /// text box contents are drawn but text does not wrap around them
    TextBoxWrap,
    /// a picture rotated or cropped is drawn as its unrotated box
    ImageTransform,
    /// endnotes are listed at the end without their own page rules
    Endnotes,
    /// a table nested more than this engine's depth limit
    DeepNesting,
}

impl UnsupportedKind {
    /// what a preserved inline element is, judged by its name.
    pub fn of_preserved_inline(xml: &str) -> Option<UnsupportedKind> {
        let name = first_name(xml);
        Some(match name {
            n if n.starts_with("m:") => UnsupportedKind::Equation,
            "w:sdt" | "w:customXml" | "w:smartTag" | "w:fldSimple" => UnsupportedKind::PreservedRun,
            "w:proofErr" | "w:permStart" | "w:permEnd" => return None,
            _ => UnsupportedKind::PreservedRun,
        })
    }

    /// what preserved run content is.
    pub fn of_preserved_run(xml: &str) -> Option<UnsupportedKind> {
        if xml.contains("<w:drawing")
            || xml.contains("<w:pict")
            || xml.contains("<mc:AlternateContent")
        {
            return Some(UnsupportedKind::Drawing);
        }
        if xml.contains("<m:") {
            return Some(UnsupportedKind::Equation);
        }
        // empty run content such as w:lastRenderedPageBreak needs nothing
        if xml.contains("lastRenderedPageBreak") || xml.trim().is_empty() {
            return None;
        }
        Some(UnsupportedKind::PreservedRun)
    }
}

fn first_name(xml: &str) -> &str {
    let start = xml.find('<').map_or(0, |i| i + 1);
    let rest = &xml[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(rest.len());
    &rest[..end]
}

/// one unsupported feature found in the document.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unsupported {
    pub kind: UnsupportedKind,
    /// a readable place: "body block 12", "header h1 block 0", "note 3"
    pub location: String,
}

impl Unsupported {
    pub fn new(kind: UnsupportedKind, location: &str) -> Unsupported {
        Unsupported {
            kind,
            location: location.to_string(),
        }
    }
}

/// keeps one report per kind and location.
pub fn deduplicate(list: &mut Vec<Unsupported>) {
    let mut seen = std::collections::HashSet::new();
    list.retain(|item| seen.insert(item.clone()));
}
