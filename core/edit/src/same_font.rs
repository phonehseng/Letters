//! choosing fonts for new text so it stays in the font of the text
//! around it: when the original embedded font lacks a character, the font
//! itself is extended with a look-alike glyph (see fonts/extend.rs) before
//! round one's fallback chain (installed copy, then a standard font) is
//! tried.

use std::collections::BTreeSet;

use lopdf::Document;

use crate::draft::Draft;
use crate::edit_run::{run_edit, Outcome};
use crate::error::EditError;
use crate::fonts::donor::{FontFileSource, GlyphSource};
use crate::fonts::embed::{self, WriteFont};
use crate::fonts::extend;
use crate::fonts::pdf_font::PdfFont;
use crate::layout::PageModel;
use crate::summary::{ContentEditResult, EditConsent, FontExtension};
use crate::text_edit::{choose_fonts, TextEditOptions};
use crate::text_fonts::FontChoice;

/// the characters of `text` the font can't draw yet. a space it lacks is
/// left out: new text writes one as a plain move.
fn missing_chars(font: &PdfFont, text: &str) -> BTreeSet<char> {
    text.chars()
        .filter(|c| *c != '\n' && *c != ' ')
        .filter(|c| font.encode(&c.to_string()).is_err())
        .collect()
}

/// fonts for `text` like the glyph `like`, extending its font when it
/// lacks characters and a source of glyphs is at hand. returns the choice
/// and, when the font was extended, what was added.
pub(crate) fn choose_same_font(
    model: &PageModel,
    draft: &mut Draft,
    like: usize,
    text: &str,
    options: &TextEditOptions,
    donor: Option<&dyn GlyphSource>,
) -> Result<(FontChoice, Option<FontExtension>), EditError> {
    let record = &model.run.glyphs[like];
    let font = record.font.map(|index| &model.run.fonts[index]);
    let (Some(font), Some(object)) = (font, font.and_then(|font| font.object)) else {
        return Ok((choose_fonts(model, draft, like, text, None, options)?, None));
    };
    let missing = missing_chars(font, text);
    if missing.is_empty() || !font.writable() {
        return Ok((choose_fonts(model, draft, like, text, None, options)?, None));
    }
    let installed = options
        .allow_installed_fonts
        .then(|| FontFileSource::installed(&font.name))
        .flatten();
    let source: Option<&dyn GlyphSource> =
        donor.or(installed.as_ref().map(|source| source as &dyn GlyphSource));
    let extension = match source {
        Some(source) => extend::extend_font(draft, object, &font.name, &missing, source)?,
        None => None,
    };
    let Some(extension) = extension else {
        return Ok((choose_fonts(model, draft, like, text, None, options)?, None));
    };
    let needed: BTreeSet<char> = text.chars().filter(|c| *c != '\n').collect();
    let mut original = WriteFont::from_original(font, object, &needed);
    original.codes.extend(extension.codes);
    add_unlisted_to_unicode(draft, font, object, &original, &needed);
    let choice = FontChoice {
        by_char: original.codes.keys().map(|c| (*c, 0)).collect(),
        fonts: vec![original],
        notices: Vec::new(),
        to_unicode_additions: Default::default(),
    };
    Ok((choice, Some(extension.record)))
}

/// characters a composite font could already draw but its /ToUnicode
/// didn't list: listed now, as round one does, so copied text is right.
fn add_unlisted_to_unicode(
    draft: &mut Draft,
    font: &PdfFont,
    object: lopdf::ObjectId,
    written: &WriteFont,
    needed: &BTreeSet<char>,
) {
    let additions: std::collections::BTreeMap<u16, String> = needed
        .iter()
        .filter(|c| font.missing_from_to_unicode.contains(c))
        .filter_map(|c| {
            let code = &written.codes.get(c)?.code;
            Some((u16::from_be_bytes([code[0], *code.get(1)?]), c.to_string()))
        })
        .collect();
    if !additions.is_empty() {
        embed::extend_to_unicode(draft, object, &additions);
    }
}

/// adds glyphs for `characters` to a font on the page (by its `FontInfo`
/// id), drawn from `donor_font` (a TrueType file: a look-alike chosen by
/// the app). text already in the font is drawn exactly as before; new
/// text with these characters can then use the same font.
pub fn extend_font(
    doc: &mut Document,
    page: usize,
    font_id: &str,
    characters: &str,
    donor_font: &[u8],
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    let donor =
        FontFileSource::look_alike(donor_font.to_vec(), "the chosen look-alike font".into())
            .ok_or_else(|| {
                EditError::UnsupportedFont(
                    "It isn’t a TrueType font Letters may copy glyphs from.".into(),
                )
            })?;
    run_edit(doc, page, consent, |model, draft, _| {
        let index = (0..model.run.fonts.len())
            .find(|index| model.font_info(Some(*index)).id == font_id)
            .ok_or(EditError::UnknownObject)?;
        let font = &model.run.fonts[index];
        let object = font.object.ok_or(EditError::UnknownObject)?;
        let wanted = missing_chars(font, characters);
        let mut outcome = Outcome::default();
        if wanted.is_empty() {
            return Ok(outcome);
        }
        let extension = extend::extend_font(draft, object, &font.name, &wanted, &donor)?.ok_or_else(|| {
            EditError::UnsupportedFont(
                "Only embedded TrueType fonts can gain glyphs, and the chosen font must have every character.".into(),
            )
        })?;
        outcome.summary.fonts_extended.push(extension.record);
        Ok(outcome)
    })
}
