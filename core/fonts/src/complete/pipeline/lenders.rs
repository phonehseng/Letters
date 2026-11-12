//! the fonts that lend glyphs to a completion: the same design when one
//! is found and allowed, the closest designs fitted, and the same
//! design's kerning.

use super::*;

/// a font that lends glyphs, with the glyphs read from it.
pub(crate) struct Lender<'a> {
    pub(crate) file: &'a DonorFile,
    pub(crate) glyphs: GlyphSet,
    pub(crate) note: DonorNote,
}

/// the same design, found on this computer.
pub(crate) struct ExactLender<'a> {
    pub(crate) lender: Lender<'a>,
    pub(crate) candidate: Candidate,
}

/// a near design, fitted.
pub(crate) struct NearLender<'a> {
    pub(crate) lender: Lender<'a>,
    pub(crate) fitted: FittedDonor,
}

impl NearLender<'_> {
    pub(crate) fn make(&self, c: char, fixed_advance: Option<f64>) -> Option<GlyphData> {
        let glyph = self.lender.glyphs.get(c)?;
        Some(self.fitted.make(c, glyph, fixed_advance))
    }

    /// the lender's glyphs changed to the target's style, for taking
    /// marks from.
    pub(crate) fn changed_set(&self) -> GlyphSet {
        GlyphSet {
            glyphs: self
                .lender
                .glyphs
                .glyphs
                .iter()
                .map(|(c, glyph)| (*c, self.fitted.make(*c, glyph, None)))
                .collect(),
            metrics: self.lender.glyphs.metrics.clone(),
        }
    }
}

/// the characters to read from a lender: the needed ones, and for
/// buildable ones the letters and marks they could be built from.
pub(crate) fn lender_chars(needed: &[char]) -> Vec<char> {
    let mut chars: BTreeSet<char> = needed.iter().copied().collect();
    for c in needed {
        if let Some((base, mark)) = decompose(*c) {
            chars.insert(base);
            chars.extend(mark.spacing_chars());
            chars.insert(mark.combining_char());
            for (accented, its_base) in letters_with(mark).take(12) {
                chars.insert(accented);
                chars.insert(its_base);
            }
        }
        if let Some(parts) = ligature_parts(*c) {
            chars.extend(parts.chars());
        }
        if let Some(base) = superscript_base(*c) {
            chars.insert(base);
        }
    }
    chars.into_iter().collect()
}

pub(crate) fn donor_note(file: &DonorFile) -> DonorNote {
    DonorNote {
        family: file.family.clone(),
        style: file.style.clone(),
        source: file.source,
        permission: file.embedding.permission,
        permission_sentence: file.embedding.sentence(),
    }
}

pub(crate) fn find_file<'a>(pool: &'a DonorPool, key: &str) -> Option<&'a DonorFile> {
    pool.donors.iter().find(|donor| donor.key == key)
}

/// the same design, when one was found and its licence lets glyphs be
/// copied out of it.
pub(crate) fn choose_exact<'a>(
    candidates: &[Candidate],
    pool: &'a DonorPool,
    wanted: &[char],
    report: &mut MatchReport,
) -> Option<ExactLender<'a>> {
    for candidate in candidates.iter().filter(|c| c.same_design) {
        let Some(file) = find_file(pool, &candidate.key) else {
            continue;
        };
        if let Err(why) = file.permission() {
            report.permission_notes.push(why);
            continue;
        }
        let bytes = file.bytes().ok()?;
        let glyphs = read_glyphs(&bytes, file.index, &candidate.axes, wanted.iter().copied())?;
        return Some(ExactLender {
            lender: Lender {
                file,
                glyphs,
                note: donor_note(file),
            },
            candidate: candidate.clone(),
        });
    }
    None
}

/// fits the closest designs (other than the same one), best first.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fit_near<'a>(
    subset: &SubsetFont,
    style: &StyleFit,
    candidates: &[Candidate],
    pool: &'a DonorPool,
    wanted: &[char],
    exact: Option<&ExactLender>,
    options: &CompleteOptions,
    cache: Option<&CompletionCache>,
    report: &mut MatchReport,
) -> Vec<NearLender<'a>> {
    let mut ranked: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| exact.is_none_or(|exact| exact.candidate.key != c.key))
        .collect();
    ranked.sort_by(|a, b| a.shape_distance.total_cmp(&b.shape_distance));
    let mut near = Vec::new();
    for candidate in ranked {
        if near.len() >= options.fit_limit {
            break;
        }
        let Some(file) = find_file(pool, &candidate.key) else {
            continue;
        };
        if let Err(why) = file.permission() {
            if !report.permission_notes.contains(&why) {
                report.permission_notes.push(why);
            }
            continue;
        }
        let axes_key = format!("{:?}", candidate.axes);
        let fit_key = cache_key(&[&subset.outline_hash(), &file.key, &axes_key]);
        let fitted = match cache.and_then(|cache| cache.load_fit(&fit_key)) {
            Some(fitted) => Some(fitted),
            None => {
                let fitted = fit_donor(&subset.glyphs, style, file, &candidate.axes);
                if let (Some(cache), Some(fitted)) = (cache, &fitted) {
                    cache.store_fit(&fit_key, fitted);
                }
                fitted
            }
        };
        let Some(fitted) = fitted else { continue };
        let Ok(bytes) = file.bytes() else { continue };
        let Some(glyphs) = read_glyphs(&bytes, file.index, &candidate.axes, wanted.iter().copied())
        else {
            continue;
        };
        near.push(NearLender {
            lender: Lender {
                file,
                glyphs,
                note: donor_note(file),
            },
            fitted,
        });
    }
    near.sort_by(|a, b| a.fitted.quality.mean.total_cmp(&b.fitted.quality.mean));
    if let Some(best) = near.first() {
        report.fit = Some(FitSummary::of(&best.fitted));
    }
    near
}

/// the same design's kerning for every pair with a new glyph from it.
pub(crate) fn exact_kerning(
    exact: &ExactLender,
    subset: &SubsetFont,
    made: &BTreeMap<char, Made>,
) -> Vec<KernPair> {
    let Ok(bytes) = exact.lender.file.bytes() else {
        return Vec::new();
    };
    let Ok(mut face) = Face::parse(&bytes, exact.lender.file.index) else {
        return Vec::new();
    };
    for (tag, value) in &exact.candidate.axes {
        if let Ok(tag) = <[u8; 4]>::try_from(tag.as_bytes()) {
            face.set_variation(Tag::from_bytes(&tag), *value);
        }
    }
    let units = f64::from(face.units_per_em().max(16));
    let copied: Vec<char> = made
        .iter()
        .filter(|(_, m)| m.strategy == Strategy::ExactDonor)
        .map(|(c, _)| *c)
        .collect();
    let all: Vec<char> = subset
        .glyphs
        .glyphs
        .keys()
        .copied()
        .chain(made.keys().copied())
        .collect();
    let mut pairs = Vec::new();
    for new in &copied {
        for other in &all {
            for (left, right) in [(*new, *other), (*other, *new)] {
                let (Some(l), Some(r)) = (face.glyph_index(left), face.glyph_index(right)) else {
                    continue;
                };
                let value = pair_kerning(&face, l, r);
                if value != 0 && !pairs.iter().any(|(a, b, _)| *a == left && *b == right) {
                    pairs.push((left, right, f64::from(value) / units));
                }
            }
        }
    }
    pairs
}
