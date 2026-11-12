//! choosing the font new text is written in, keeping the original when
//! it can draw every character.
//!
//! the chain, for characters the original font lacks:
//! 1. the same font installed on this computer, if its licence allows
//!    embedding: the needed glyphs are copied into the file;
//! 2. otherwise the standard font that looks most like it (Helvetica,
//!    Times or Courier, with matching bold and italic).
//!
//! every fallback is reported in plain words, so the ui can tell the
//! person what happened.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::draft::Draft;
use crate::error::EditError;
use crate::fonts::embed::{self, WriteFont};
use crate::fonts::pdf_font::PdfFont;
use crate::fonts::system;
use crate::fonts::truetype::{Embedding, FontProgram};

/// how a character the original font lacks was drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FallbackKind {
    /// the same font, installed on this computer, was copied into the file
    InstalledFont,
    /// a similar standard font (Helvetica, Times or Courier) was used
    StandardFont,
}

/// a font change the person should be told about.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontNotice {
    pub kind: FallbackKind,
    pub original_font: String,
    pub used_font: String,
    /// the characters the original font couldn't draw
    pub characters: String,
    /// the sentence to show, e.g. "The original font can’t draw “é”, so
    /// Helvetica was used for the new text."
    pub message: String,
}

/// the fonts one piece of new text is written in: for each character,
/// which of `fonts` draws it.
pub(crate) struct FontChoice {
    pub fonts: Vec<WriteFont>,
    pub by_char: BTreeMap<char, usize>,
    pub notices: Vec<FontNotice>,
    /// characters a composite original font can draw but its /ToUnicode
    /// doesn't list yet: glyph number to text
    pub to_unicode_additions: BTreeMap<u16, String>,
}

/// what may happen when the original font can't draw a character.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FallbackRules {
    pub installed_fonts: bool,
    pub standard_fonts: bool,
}

fn quoted(characters: &[char]) -> String {
    characters.iter().collect()
}

/// picks fonts for `text`, starting from the original font.
pub(crate) fn choose(
    draft: &mut Draft,
    original: &PdfFont,
    text: &str,
    rules: FallbackRules,
) -> Result<FontChoice, EditError> {
    let needed: BTreeSet<char> = text.chars().filter(|c| *c != '\n').collect();
    // a font Letters can't write to (unreadable codes, no object of its
    // own) draws nothing new: every character goes to the fallback
    let first = match original.object {
        Some(object) if original.writable() => WriteFont::from_original(original, object, &needed),
        _ => WriteFont::nothing(&original.name),
    };
    let mut choice = FontChoice {
        by_char: needed
            .iter()
            .filter(|c| first.can_draw(**c))
            .map(|c| (*c, 0))
            .collect(),
        fonts: vec![first],
        notices: Vec::new(),
        to_unicode_additions: BTreeMap::new(),
    };
    for character in &needed {
        if choice.by_char.contains_key(character)
            && original.missing_from_to_unicode.contains(character)
        {
            if let Some(code) = choice.fonts[0].codes.get(character) {
                choice.to_unicode_additions.insert(
                    u16::from_be_bytes([code.code[0], code.code[1]]),
                    character.to_string(),
                );
            }
        }
    }
    let missing: Vec<char> = needed
        .iter()
        .filter(|c| !choice.by_char.contains_key(c))
        .copied()
        .collect();
    if missing.is_empty() {
        return Ok(choice);
    }
    // a space the font lacks can be written as a plain move instead
    let missing: Vec<char> = missing.into_iter().filter(|c| *c != ' ').collect();
    if missing.is_empty() {
        return Ok(choice);
    }
    let mut refused_installed: Option<String> = None;
    if rules.installed_fonts {
        match installed_copy(draft, original, &missing) {
            Ok(Some(font)) => {
                let index = choice.fonts.len();
                choice.notices.push(FontNotice {
                    kind: FallbackKind::InstalledFont,
                    original_font: original.name.clone(),
                    used_font: font.display_name.clone(),
                    characters: quoted(&missing),
                    message: format!(
                        "The font in the file can’t draw “{}”, so those letters were copied from {} on this computer.",
                        quoted(&missing),
                        font.display_name
                    ),
                });
                choice.fonts.push(font);
                for character in &missing {
                    choice.by_char.insert(*character, index);
                }
                return Ok(choice);
            }
            Ok(None) => {}
            Err(EditError::EmbeddingNotAllowed(name)) => refused_installed = Some(name),
            Err(_) => {}
        }
    }
    if !rules.standard_fonts {
        return Err(EditError::UnsupportedCharacters(quoted(&missing)));
    }
    let not_standard = embed::not_in_standard_fonts(&quoted(&missing));
    if !not_standard.is_empty() {
        return Err(EditError::UnsupportedCharacters(quoted(&not_standard)));
    }
    let standard = original.closest_standard_font();
    let font = embed::standard_font(draft, standard);
    let index = choice.fonts.len();
    let message = match &refused_installed {
        Some(installed) => format!(
            "The original font can’t draw “{}”, and the copy of {installed} on this computer doesn’t allow being copied into a PDF, so {} was used for the new text.",
            quoted(&missing),
            font.display_name
        ),
        None => format!("The original font can’t draw “{}”, so {} was used for the new text.", quoted(&missing), font.display_name),
    };
    choice.notices.push(FontNotice {
        kind: FallbackKind::StandardFont,
        original_font: original.name.clone(),
        used_font: font.display_name.clone(),
        characters: quoted(&missing),
        message,
    });
    choice.fonts.push(font);
    for character in &missing {
        choice.by_char.insert(*character, index);
    }
    Ok(choice)
}

/// the same font from the computer's font folders, with the missing
/// glyphs copied into the file; none if it isn't installed or lacks them.
fn installed_copy(
    draft: &mut Draft,
    original: &PdfFont,
    missing: &[char],
) -> Result<Option<WriteFont>, EditError> {
    let Some(installed) = system::find(&original.name) else {
        return Ok(None);
    };
    let Ok(data) = std::fs::read(&installed.path) else {
        return Ok(None);
    };
    let Some(program) = FontProgram::parse(&data, installed.index) else {
        return Ok(None);
    };
    if program.embedding() == Embedding::Forbidden {
        return Err(EditError::EmbeddingNotAllowed(installed.family.clone()));
    }
    if missing.iter().any(|c| program.glyph_for_char(*c).is_none()) {
        return Ok(None);
    }
    let characters: BTreeSet<char> = missing.iter().copied().collect();
    embed::truetype_font(draft, &program, &characters, &installed.family).map(Some)
}
