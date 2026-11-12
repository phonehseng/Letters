//! the words of the report: the neutral family name, the identified
//! font, the sentences for the person and the licence notes.

use super::*;

/// "Letters Completed 3f2a91c0": a name that says nothing of the font
/// it came from, different for each document font and set of letters.
pub(crate) fn neutral_family(subset: &SubsetFont, new_glyphs: &[(char, GlyphData)]) -> String {
    let letters: String = new_glyphs.iter().map(|(c, _)| *c).collect();
    let key = cache_key(&[&subset.outline_hash(), &letters]);
    format!("Letters Completed {}", &key[..8])
}

pub(crate) fn identified(
    candidates: &[Candidate],
    exact: Option<&ExactLender>,
) -> Option<Identified> {
    let best = exact.map(|e| &e.candidate).or_else(|| candidates.first())?;
    Some(Identified {
        family: best.family.clone(),
        style: best.style.clone(),
        source: best.source,
        same_design: best.same_design,
        similarity: best.similarity,
    })
}

/// the sentences for the person, most important first.
pub(crate) fn sentences(
    glyphs: &[GlyphProvenance],
    exact: Option<&ExactLender>,
    near: &[NearLender],
    candidates: &[Candidate],
) -> Vec<String> {
    let mut lines = Vec::new();
    let of = |strategy: Strategy| -> Vec<char> {
        glyphs
            .iter()
            .filter(|g| g.strategy == strategy)
            .map(|g| g.character)
            .collect()
    };
    if let Some(exact) = exact {
        let label = family_and_style(&exact.lender.file.family, &exact.lender.file.style);
        lines.push(format!(
            "Found the same font {}: {label}.",
            source_words(exact.lender.file.source)
        ));
        let copied = of(Strategy::ExactDonor);
        if !copied.is_empty() {
            lines.push(format!(
                "The new letters ({}) come from {label}, unchanged.",
                char_list(&copied)
            ));
        }
    } else if let Some(same) = candidates.iter().find(|c| c.same_design) {
        lines.push(format!(
            "Found the same font {}: {}, but its license doesn’t allow copying its letters.",
            source_words(same.source),
            family_and_style(&same.family, &same.style)
        ));
    }
    for lender in near {
        let from_it: Vec<char> = glyphs
            .iter()
            .filter(|g| {
                g.strategy == Strategy::FittedDonor
                    && g.donor.as_ref().is_some_and(|d| {
                        d.family == lender.lender.file.family && d.style == lender.lender.file.style
                    })
            })
            .map(|g| g.character)
            .collect();
        if !from_it.is_empty() {
            lines.push(format!(
                "The new letters ({}) come from {} {}, adjusted to match ({} alike).",
                char_list(&from_it),
                family_and_style(&lender.lender.file.family, &lender.lender.file.style),
                source_words(lender.lender.file.source),
                percent(lender.fitted.quality.similarity)
            ));
        }
    }
    for glyph in glyphs.iter().filter(|g| g.strategy == Strategy::Composed) {
        lines.push(glyph.sentence.clone());
    }
    let generated = of(Strategy::Generated);
    if !generated.is_empty() {
        lines.push(format!(
            "{} were drawn by the AI tools provider and checked against the font’s style.",
            char_list(&generated)
        ));
    }
    let mut look_alike_families: Vec<String> = Vec::new();
    for glyph in glyphs.iter().filter(|g| g.strategy == Strategy::LookAlike) {
        if let Some(donor) = &glyph.donor {
            let label = family_and_style(&donor.family, &donor.style);
            if !look_alike_families.contains(&label) {
                look_alike_families.push(label);
            }
        }
    }
    for label in look_alike_families {
        lines.push(format!(
            "New text uses {label} because nothing closer was found."
        ));
    }
    let missing = of(Strategy::Missing);
    if !missing.is_empty() {
        lines.push(format!(
            "Nothing on this computer can draw {}.",
            char_list(&missing)
        ));
    }
    lines
}

/// the licence of every font that lent glyphs, once each.
pub(crate) fn note_licences(glyphs: &[GlyphProvenance], report: &mut MatchReport) {
    for donor in glyphs.iter().filter_map(|g| g.donor.as_ref()) {
        let line = format!(
            "{}: {}",
            family_and_style(&donor.family, &donor.style),
            donor.permission_sentence
        );
        if !report.permission_notes.contains(&line) {
            report.permission_notes.push(line);
        }
    }
}
