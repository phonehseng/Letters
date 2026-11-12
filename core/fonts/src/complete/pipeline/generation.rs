//! the optional generated glyphs inside a completion: asked for only for
//! letters the fitted donor made or nobody could, kept only when better.

use super::*;

/// asks the generator for characters the fitted donor made (or nobody
/// could), and keeps a generated glyph only when it fits the style
/// better than what is there.
pub(crate) fn try_generation(
    generation: &Generation,
    subset: &SubsetFont,
    style: &StyleFit,
    near: &[NearLender],
    made: &mut BTreeMap<char, Made>,
    notices: &mut Vec<String>,
) {
    let improvable: Vec<char> = made
        .iter()
        .filter(|(_, m)| {
            matches!(
                m.strategy,
                Strategy::FittedDonor | Strategy::LookAlike | Strategy::Missing
            )
        })
        .map(|(c, _)| *c)
        .filter(|c| generation.plan.disclosure().wanted.contains(c))
        .collect();
    if improvable.is_empty() {
        return;
    }
    let reply = match run_generation(generation, &improvable) {
        Ok(reply) => reply,
        Err(error) => {
            notices.push(error.to_string());
            return;
        }
    };
    let advances = rough_advances(&improvable, subset, style, near, made);
    let expected = |c: char| advances.get(&c).copied().unwrap_or(style.average_advance);
    let (accepted, refused) = read_reply(&reply, &improvable, &expected);
    let mut kept = Vec::new();
    for generated in accepted {
        let c = generated.character;
        let known_advance = subset.pdf_widths.get(&c).copied();
        let new_score = style_score(c, &generated.glyph, style, known_advance);
        let current_score = made
            .get(&c)
            .and_then(|m| m.glyph.as_ref())
            .map(|glyph| style_score(c, glyph, style, known_advance))
            .unwrap_or(GENERATED_WITHOUT_RIVAL);
        if new_score < current_score {
            made.insert(c, generated_made(c, generated.glyph));
            kept.push(c);
        }
    }
    if !refused.is_empty() {
        notices.push(format!(
            "{} generated letter(s) were refused because they failed the checks.",
            refused.len()
        ));
    }
    if kept.is_empty() {
        notices.push(
            "Generated letters weren’t used: the adjusted near font matched better.".to_string(),
        );
    }
}

/// a generated glyph for a character nothing else could draw is kept only
/// when its style score is under this (em): within about two stem widths'
/// worth of error in heights and stem together.
pub(crate) const GENERATED_WITHOUT_RIVAL: f64 = 0.05;

/// the advance each letter should roughly have, for checking a reply:
/// the pdf's width, else what the near font made, else the font's average.
pub(crate) fn rough_advances(
    chars: &[char],
    subset: &SubsetFont,
    style: &StyleFit,
    near: &[NearLender],
    made: &BTreeMap<char, Made>,
) -> BTreeMap<char, f64> {
    chars
        .iter()
        .map(|c| {
            let from_pdf = subset.pdf_widths.get(c).copied();
            let from_made = || made.get(c)?.glyph.as_ref().map(|g| g.advance);
            let from_near = || near.first()?.make(*c, None).map(|g| g.advance);
            let advance = from_pdf
                .or_else(from_made)
                .or_else(from_near)
                .unwrap_or(style.average_advance);
            (*c, advance)
        })
        .collect()
}

pub(crate) fn generated_made(c: char, glyph: GlyphData) -> Made {
    Made {
        glyph: Some(glyph),
        strategy: Strategy::Generated,
        donor: None,
        built_from: Vec::new(),
        similarity: None,
        sentence: format!("“{c}” was drawn by the AI tools provider and checked against the font’s measured style."),
    }
}
