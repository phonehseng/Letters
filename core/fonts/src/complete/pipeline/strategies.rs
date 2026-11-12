//! one character by the first strategy that works: the same design,
//! built from the font's own glyphs, the fitted near design, the closest
//! font as it is.

use super::*;

pub(crate) fn is_buildable(c: char) -> bool {
    decompose(c).is_some() || ligature_parts(c).is_some() || superscript_base(c).is_some()
}

/// the document font's glyphs plus those made so far.
pub(crate) fn letters_so_far(subset: &SubsetFont, made: &BTreeMap<char, Made>) -> GlyphSet {
    let mut letters = subset.glyphs.clone();
    for (c, m) in made {
        if let Some(glyph) = &m.glyph {
            letters.glyphs.entry(*c).or_insert_with(|| glyph.clone());
        }
    }
    letters
}

/// one character, by the first strategy that works.
#[allow(clippy::too_many_arguments)]
pub(crate) fn make_glyph(
    c: char,
    subset: &SubsetFont,
    style: &StyleFit,
    letters: &GlyphSet,
    exact: Option<&ExactLender>,
    near: &[NearLender],
    pool: &DonorPool,
    options: &CompleteOptions,
) -> Made {
    let fixed_advance = subset.pdf_widths.get(&c).copied();
    if let Some(made) = exact.and_then(|exact| from_exact(c, exact, fixed_advance)) {
        return made;
    }
    let accent = decompose(c).is_some();
    let ligature = ligature_parts(c).is_some();
    if (accent && options.prefer_composed_accents) || ligature {
        if let Some(made) = build(c, letters, style, exact, near, fixed_advance) {
            return made;
        }
    }
    if let Some(made) = from_near(c, near, fixed_advance) {
        return made;
    }
    if accent || superscript_base(c).is_some() {
        if let Some(made) = build(c, letters, style, exact, near, fixed_advance) {
            return made;
        }
    }
    if options.allow_look_alike {
        if let Some(made) = look_alike(c, subset, near, pool, fixed_advance) {
            return made;
        }
    }
    Made::missing(c)
}

pub(crate) fn with_advance(mut glyph: GlyphData, fixed_advance: Option<f64>) -> GlyphData {
    if let Some(advance) = fixed_advance {
        // keep the ink where it was relative to the middle of the advance
        let shift = (advance - glyph.advance) / 2.0;
        glyph.shape = glyph.shape.translated(shift, 0.0);
        glyph.advance = advance;
    }
    glyph
}

pub(crate) fn from_exact(c: char, exact: &ExactLender, fixed_advance: Option<f64>) -> Option<Made> {
    let glyph = exact.lender.glyphs.get(c)?.clone();
    let label = family_and_style(&exact.lender.file.family, &exact.lender.file.style);
    Some(Made {
        glyph: Some(with_advance(glyph, fixed_advance)),
        strategy: Strategy::ExactDonor,
        donor: Some(exact.lender.note.clone()),
        built_from: Vec::new(),
        similarity: Some(exact.candidate.similarity),
        sentence: format!(
            "“{c}” comes from {label} {}, unchanged.",
            source_words(exact.lender.file.source)
        ),
    })
}

pub(crate) fn from_near(c: char, near: &[NearLender], fixed_advance: Option<f64>) -> Option<Made> {
    for lender in near {
        let Some(glyph) = lender.make(c, fixed_advance) else {
            continue;
        };
        let label = family_and_style(&lender.lender.file.family, &lender.lender.file.style);
        return Some(Made {
            glyph: Some(glyph),
            strategy: Strategy::FittedDonor,
            donor: Some(lender.lender.note.clone()),
            built_from: Vec::new(),
            similarity: Some(lender.fitted.quality.similarity),
            sentence: format!(
                "“{c}” comes from {label} {}, adjusted to match ({} alike).",
                source_words(lender.lender.file.source),
                percent(lender.fitted.quality.similarity)
            ),
        });
    }
    None
}

/// builds an accented letter, ligature or superscript. marks come from
/// the document's font first, then the same design, then the best fit.
pub(crate) fn build(
    c: char,
    letters: &GlyphSet,
    style: &StyleFit,
    exact: Option<&ExactLender>,
    near: &[NearLender],
    fixed_advance: Option<f64>,
) -> Option<Made> {
    let composed: Composed = if decompose(c).is_some() {
        let changed = near.first().map(NearLender::changed_set);
        let exact_name =
            exact.map(|e| family_and_style(&e.lender.file.family, &e.lender.file.style));
        let near_name = near
            .first()
            .map(|n| family_and_style(&n.lender.file.family, &n.lender.file.style));
        let mut sources: Vec<(&GlyphSet, &str)> = vec![(letters, "")];
        if let (Some(exact), Some(name)) = (exact, exact_name.as_deref()) {
            sources.push((&exact.lender.glyphs, name));
        }
        if let (Some(changed), Some(name)) = (changed.as_ref(), near_name.as_deref()) {
            sources.push((changed, name));
        }
        compose_accented(c, letters, &sources, style)?
    } else if ligature_parts(c).is_some() {
        compose_ligature(c, letters)?
    } else {
        compose_superscript(c, letters, style)?
    };
    Some(Made {
        glyph: Some(with_advance(composed.glyph, fixed_advance)),
        strategy: Strategy::Composed,
        donor: None,
        built_from: composed.built_from,
        similarity: None,
        sentence: composed.sentence,
    })
}

/// the closest font that has the character, used as it is.
pub(crate) fn look_alike(
    c: char,
    subset: &SubsetFont,
    near: &[NearLender],
    pool: &DonorPool,
    fixed_advance: Option<f64>,
) -> Option<Made> {
    let profile = subset.profile();
    let mut files: Vec<(f64, &DonorFile)> = near
        .iter()
        .map(|n| (-1.0, n.lender.file))
        .chain(
            pool.donors
                .iter()
                .filter(|donor| donor.can_draw(c))
                .map(|donor| (quick_distance(&profile, donor), donor)),
        )
        .filter(|(_, donor)| donor.permission().is_ok())
        .collect();
    files.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, file) in files.into_iter().take(12) {
        let Ok(bytes) = file.bytes() else { continue };
        let Some(set) = read_glyphs(&bytes, file.index, &[], [c]) else {
            continue;
        };
        let Some(glyph) = set.get(c).cloned() else {
            continue;
        };
        let label = family_and_style(&file.family, &file.style);
        return Some(Made {
            glyph: Some(with_advance(glyph, fixed_advance)),
            strategy: Strategy::LookAlike,
            donor: Some(donor_note(file)),
            built_from: Vec::new(),
            similarity: None,
            sentence: format!("“{c}” uses {label} because nothing closer was found."),
        });
    }
    None
}
