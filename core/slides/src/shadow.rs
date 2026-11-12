//! lossless back and forth between pptx and odp.
//!
//! a deck converted to the other format carries an exact copy of itself
//! in its original format as an extra package part ("the shadow"), with a
//! fingerprint of every slide as it was written. when letters reads the
//! converted file back and every slide is still exactly as written, the
//! shadow is used instead, so pptx -> odp -> pptx (and the reverse) gives
//! the original deck. once any application changed a slide, the
//! fingerprints no longer match and the file is read as it now is.
//! other applications ignore the extra part (opendocument and open xml
//! both allow parts a consumer does not know).

use crate::model::fingerprint_of;

/// the shadow of a deck converted from pptx, inside an odp package
pub const ODP_SHADOW: &str = "Letters/source.pptx";
/// the shadow of a deck converted from odp, inside a pptx package
pub const PPTX_SHADOW: &str = "letters/source.odp";
/// one fingerprint per slide, one per line, next to the shadow
pub const ODP_PAGES: &str = "Letters/pages.txt";
pub const PPTX_SLIDES: &str = "letters/slides.txt";
/// the relationship type that points a pptx package at its shadow
pub const SHADOW_RELATIONSHIP: &str = "http://schemas.letters.app/2026/relationships/source";

/// the fingerprint list of slides written as `texts`.
pub fn fingerprints<'a>(texts: impl Iterator<Item = &'a str>) -> String {
    texts
        .map(|text| fingerprint_of(&text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// true when the slides read now are exactly the ones written.
pub fn unchanged<'a>(recorded: &str, texts: impl Iterator<Item = &'a str>) -> bool {
    let now = fingerprints(texts);
    !recorded.trim().is_empty() && recorded.trim() == now.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_notice_any_change() {
        let written = fingerprints(["<a/>", "<b/>"].into_iter());
        assert!(unchanged(&written, ["<a/>", "<b/>"].into_iter()));
        assert!(!unchanged(&written, ["<a/>", "<b x='1'/>"].into_iter()));
        assert!(!unchanged(&written, ["<a/>"].into_iter()));
        assert!(!unchanged("", std::iter::empty()));
    }
}
