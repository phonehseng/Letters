//! writing the model as a pptx package.
//!
//! the package read is the starting point: every part the model does not
//! own is copied byte for byte, parts whose model did not change are
//! copied too, and only edited parts are rebuilt (from their original
//! xml where there is one). a deck that came from odp or from nothing
//! gets fresh parts (see `fresh`).

mod canvas;
mod deck;
mod fresh;
mod names;
mod package;
mod parts;
mod slides;

use std::collections::HashMap;

use crate::error::SlidesError;
use crate::model::{fingerprint_of, Presentation};

pub use names::PartNames;
pub use package::OutPackage;

/// the deck-level settings that live in presentation.xml, each with its
/// own fingerprint so only the changed ones are rewritten.
pub fn deck_pieces(presentation: &Presentation) -> Vec<(String, String)> {
    let piece = |name: &str, fingerprint: String| (name.to_string(), fingerprint);
    vec![
        piece("size", fingerprint_of(&presentation.slide_size)),
        piece("notesSize", fingerprint_of(&presentation.notes_size)),
        piece(
            "defaultTextStyle",
            fingerprint_of(&presentation.default_text_style),
        ),
        piece("sections", fingerprint_of(&presentation.sections)),
        piece(
            "firstSlide",
            fingerprint_of(&presentation.first_slide_number),
        ),
    ]
}

/// true when a deck-level piece still has the fingerprint it was read with.
pub(crate) fn deck_piece_unchanged(presentation: &Presentation, name: &str) -> bool {
    let now = deck_pieces(presentation);
    let current = now.iter().find(|(key, _)| key == name);
    let read = presentation
        .carry
        .deck_pieces
        .iter()
        .find(|(key, _)| key == name);
    current.is_some() && current == read
}

/// the whole deck as pptx bytes.
pub fn write_pptx(presentation: &Presentation) -> Result<Vec<u8>, SlidesError> {
    let mut package = OutPackage::from_carry(presentation);
    if !package.has_presentation() {
        fresh::add_package_skeleton(&mut package);
    }
    let names = PartNames::assign(presentation, &package);
    let media_parts = place_media(presentation, &mut package);
    parts::write_themes(presentation, &names, &mut package);
    parts::write_masters(presentation, &names, &media_parts, &mut package);
    parts::write_layouts(presentation, &names, &media_parts, &mut package);
    parts::write_slides(presentation, &names, &media_parts, &mut package);
    deck::write_presentation(presentation, &names, &mut package)?;
    if presentation.carry.format == Some(crate::model::SourceFormat::Odp) {
        add_shadow(presentation, &names, &mut package)?;
    }
    deck::write_content_types(&mut package);
    deck::drop_unreachable(&mut package);
    package.zip()
}

/// puts every media item in the package and says where each one went.
/// items read from a file go back to the part they came from.
fn place_media(presentation: &Presentation, package: &mut OutPackage) -> HashMap<String, String> {
    let mut placed = HashMap::new();
    for item in &presentation.media {
        // odp media paths ("Pictures/a.png") are not pptx part names
        let from_pptx = presentation.carry.format == Some(crate::model::SourceFormat::Pptx);
        let part = match &item.source_path {
            Some(path) if from_pptx => path.clone(),
            _ => package.free_name("ppt/media/", &media_file_name(&item.id)),
        };
        package.set(&part, item.bytes.clone());
        package
            .content_types
            .ensure_default(part.rsplit('.').next().unwrap_or("bin"), &item.content_type);
        placed.insert(item.id.clone(), part);
    }
    placed
}

/// a media id as a file name: odp ids are paths ("Pictures/a.png").
fn media_file_name(id: &str) -> String {
    id.rsplit('/').next().unwrap_or(id).to_string()
}

/// the odp this deck came from, with a fingerprint of every slide as
/// written, so letters reads this pptx back as the exact original (see
/// `crate::shadow`).
fn add_shadow(
    presentation: &Presentation,
    names: &PartNames,
    package: &mut OutPackage,
) -> Result<(), SlidesError> {
    use crate::shadow::{fingerprints, PPTX_SHADOW, PPTX_SLIDES, SHADOW_RELATIONSHIP};
    let (source, _) = crate::odp::write_odp(presentation)?;
    let slides: Vec<String> = names
        .slides
        .iter()
        .filter_map(|part| package.text(part))
        .collect();
    package.set(PPTX_SHADOW, source);
    package.set_text(
        PPTX_SLIDES,
        &fingerprints(slides.iter().map(String::as_str)),
    );
    package
        .content_types
        .ensure_default("odp", crate::odp::write::MIMETYPE);
    package.content_types.ensure_default("txt", "text/plain");
    let mut root = package.rels_of("");
    for (target, kind) in [
        (PPTX_SHADOW, SHADOW_RELATIONSHIP.to_string()),
        (PPTX_SLIDES, format!("{SHADOW_RELATIONSHIP}-slides")),
    ] {
        root.items.retain(|rel| rel.kind != kind);
        let id = root.free_id();
        root.items.push(super::rels::Rel {
            id,
            kind,
            target: target.to_string(),
            external: false,
        });
    }
    package.set_rels("", &root);
    Ok(())
}
