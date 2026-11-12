//! making missing glyphs out of glyphs the font already has.
//!
//! - accented letters: the base letter plus the mark, the mark taken
//!   from another accented letter the font has ("è" lends its grave to
//!   "ù"), from the font's own spacing accent ("´"), or from a fitted
//!   donor's accented letters, already changed to the target's weight.
//!   the mark keeps the gap above (or below) its letter and its sideways
//!   offset from the letter's centre that it had where it came from.
//! - dotless "ı" and "ȷ" from "i" and "j" with the dot taken off, as
//!   bases for "í", "ĵ" and the like.
//! - ligatures from their letters side by side.
//! - superscript digits from the digits, smaller, raised, and a little
//!   heavier so they don't look faint (as type designers do).

use super::marks::{decompose, letters_with, ligature_parts, superscript_base, Mark};
use crate::matching::glyphs::{GlyphData, GlyphSet};
use crate::matching::offset::offset_shape;
use crate::matching::shape::{Bounds, Shape};
use crate::matching::style::StyleFit;

/// a glyph made from others, and what it was made from.
#[derive(Clone, Debug, PartialEq)]
pub struct Composed {
    pub glyph: GlyphData,
    /// the characters it was built from, base first
    pub built_from: Vec<char>,
    pub sentence: String,
}

/// a mark taken out of a glyph, with where it sat on its letter.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkShape {
    pub shape: Shape,
    /// mark centre minus letter centre, em
    pub dx: f64,
    /// above marks: mark bottom minus letter top; below marks: letter
    /// bottom minus mark top (both usually positive), em
    pub gap: f64,
    /// whether the letter it came from was a capital
    pub on_capital: bool,
    /// the character it came from
    pub from: char,
}

/// how far above the letter's top (or below its bottom) a contour must
/// start to count as the mark rather than part of the letter, em.
const MARK_CLEARANCE: f64 = 0.004;

/// the letter's box, for placing marks: the base glyph's ink.
fn ink(glyph: &GlyphData) -> Option<Bounds> {
    glyph.shape.bounds()
}

/// a base letter from the set, making dotless "ı" and "ȷ" from "i" and
/// "j" when the font has no dotless ones.
pub fn base_glyph(set: &GlyphSet, base: char, style: &StyleFit) -> Option<(GlyphData, char)> {
    if let Some(glyph) = set.get(base) {
        return Some((glyph.clone(), base));
    }
    let dotted = match base {
        'ı' => 'i',
        'ȷ' => 'j',
        _ => return None,
    };
    let glyph = set.get(dotted)?;
    let keep_below = style.x_height * 1.08;
    let contours: Vec<_> = glyph
        .shape
        .contours
        .iter()
        .filter(|contour| contour.bounds().is_some_and(|b| b.y_min < keep_below))
        .cloned()
        .collect();
    // the dot must really have been a separate contour
    if contours.len() == glyph.shape.contours.len() || contours.is_empty() {
        return None;
    }
    Some((
        GlyphData {
            shape: Shape { contours },
            advance: glyph.advance,
        },
        dotted,
    ))
}

/// finds `mark` in the set: in an accented letter (split off its
/// letter), or as a spacing accent.
pub fn find_mark(
    set: &GlyphSet,
    mark: Mark,
    style: &StyleFit,
    prefer_capital: bool,
) -> Option<MarkShape> {
    let mut found: Vec<MarkShape> = letters_with(mark)
        .filter(|(accented, _)| set.has(*accented))
        .filter_map(|(accented, base)| split_mark(set, accented, base, mark, style))
        .collect();
    found.sort_by_key(|m| m.on_capital != prefer_capital);
    if let Some(best) = found.into_iter().next() {
        return Some(best);
    }
    mark.spacing_chars()
        .iter()
        .chain(std::iter::once(&mark.combining_char()))
        .find_map(|c| spacing_mark(set, *c, mark, style))
}

/// the mark of an accented letter: its contours clear of the letter.
fn split_mark(
    set: &GlyphSet,
    accented: char,
    base: char,
    mark: Mark,
    style: &StyleFit,
) -> Option<MarkShape> {
    let glyph = set.get(accented)?;
    let capital = base.is_uppercase();
    let letter_box = match base_glyph(set, base, style) {
        Some((letter, _)) => ink(&letter)?,
        None => {
            // without the plain letter, its height stands in
            let mut b = ink(glyph)?;
            if mark.is_below() {
                b.y_min = 0.0;
            } else {
                b.y_max = if capital {
                    style.cap_height
                } else {
                    style.x_height
                };
            }
            b
        }
    };
    let (mark_contours, letter_contours): (Vec<_>, Vec<_>) =
        glyph.shape.contours.iter().cloned().partition(|contour| {
            contour.bounds().is_some_and(|b| {
                if mark.is_below() {
                    b.y_max < letter_box.y_min - MARK_CLEARANCE
                } else {
                    b.y_min > letter_box.y_max + MARK_CLEARANCE
                }
            })
        });
    if mark_contours.is_empty() || letter_contours.is_empty() {
        return None;
    }
    let shape = Shape {
        contours: mark_contours,
    };
    let mark_box = shape.bounds()?;
    let letter_ink = Shape {
        contours: letter_contours,
    }
    .bounds()?;
    Some(MarkShape {
        dx: mark_box.centre_x() - letter_ink.centre_x(),
        gap: if mark.is_below() {
            letter_ink.y_min - mark_box.y_max
        } else {
            mark_box.y_min - letter_ink.y_max
        },
        shape,
        on_capital: capital,
        from: accented,
    })
}

/// a spacing accent ("´") used as the mark: fonts draw these at the
/// height of lowercase accents, centred in their advance.
fn spacing_mark(set: &GlyphSet, c: char, mark: Mark, style: &StyleFit) -> Option<MarkShape> {
    let glyph = set.get(c)?;
    let bounds = ink(glyph)?;
    let looks_like_a_mark = if mark.is_below() {
        bounds.y_max < 0.1
    } else {
        bounds.y_min > style.x_height * 0.9
    };
    if !looks_like_a_mark {
        return None;
    }
    Some(MarkShape {
        dx: bounds.centre_x() - glyph.advance / 2.0,
        gap: if mark.is_below() {
            -bounds.y_max
        } else {
            bounds.y_min - style.x_height
        },
        shape: glyph.shape.clone(),
        on_capital: false,
        from: c,
    })
}

/// puts a mark on a letter: same gap and offset as where it came from.
/// a lowercase mark on a capital sits a little closer and flatter, as
/// fonts draw capital accents.
fn place_mark(
    letter: &GlyphData,
    mark: &MarkShape,
    below: bool,
    capital: bool,
    style: &StyleFit,
) -> Option<GlyphData> {
    let letter_box = ink(letter)?;
    let mut shape = mark.shape.clone();
    let mut gap = mark.gap;
    if capital && !mark.on_capital && !below {
        let bounds = shape.bounds()?;
        shape = shape.scaled(1.0, 0.85, 0.0, bounds.y_min * 0.15);
        gap *= 0.7;
    }
    let bounds = shape.bounds()?;
    let target_x = letter_box.centre_x() + mark.dx;
    let target_y = if below {
        letter_box.y_min - gap - bounds.y_max
    } else {
        letter_box.y_max + gap - bounds.y_min
    };
    let dy = target_y;
    // in a slanted font a mark moved up must also move right
    let lean = style.slant_degrees.to_radians().tan() * dy;
    let dx = target_x - bounds.centre_x() + lean;
    Some(GlyphData {
        shape: letter.shape.merged(&shape.translated(dx, dy)),
        advance: letter.advance,
    })
}

/// builds an accented letter from its base and a mark. `sources` are
/// searched for the mark in order (the document's font first, then a
/// fitted donor's changed glyphs); the base comes from `letters`.
pub fn compose_accented(
    c: char,
    letters: &GlyphSet,
    sources: &[(&GlyphSet, &str)],
    style: &StyleFit,
) -> Option<Composed> {
    let (base, mark) = decompose(c)?;
    let (letter, letter_char) = base_glyph(letters, base, style)?;
    let capital = base.is_uppercase();
    for (source, source_name) in sources {
        let Some(found) = find_mark(source, mark, style, capital) else {
            continue;
        };
        let glyph = place_mark(&letter, &found, mark.is_below(), capital, style)?;
        let from_donor = !source_name.is_empty();
        let sentence = if from_donor {
            format!(
                "“{c}” was built from “{letter_char}” and the {} of {source_name}, adjusted to match.",
                mark.name()
            )
        } else {
            format!(
                "“{c}” was built from “{letter_char}” and the {} of “{}”.",
                mark.name(),
                found.from
            )
        };
        return Some(Composed {
            glyph,
            built_from: vec![letter_char, found.from],
            sentence,
        });
    }
    None
}

/// builds a ligature from its letters side by side.
pub fn compose_ligature(c: char, letters: &GlyphSet) -> Option<Composed> {
    let parts = ligature_parts(c)?;
    let mut shape = Shape::default();
    let mut x = 0.0;
    let mut built_from = Vec::new();
    for part in parts.chars() {
        let glyph = letters.get(part)?;
        shape = shape.merged(&glyph.shape.translated(x, 0.0));
        x += glyph.advance;
        built_from.push(part);
    }
    let names: Vec<String> = built_from.iter().map(|p| format!("“{p}”")).collect();
    Some(Composed {
        glyph: GlyphData { shape, advance: x },
        sentence: format!("“{c}” was built from {} side by side.", names.join(" and ")),
        built_from,
    })
}

/// the size of a superscript digit against the normal one.
const SUPERSCRIPT_SCALE: f64 = 0.62;
/// how heavy its stems are against the normal stems, after scaling.
const SUPERSCRIPT_WEIGHT: f64 = 0.78;

/// builds a superscript digit: scaled, made heavier to make up for the
/// scaling, and raised so its top meets the capital height.
pub fn compose_superscript(c: char, letters: &GlyphSet, style: &StyleFit) -> Option<Composed> {
    let base = superscript_base(c)?;
    let glyph = letters.get(base)?;
    let small = glyph
        .shape
        .scaled(SUPERSCRIPT_SCALE, SUPERSCRIPT_SCALE, 0.0, 0.0);
    // scaled stems are SUPERSCRIPT_SCALE × stem; bring them up to
    // SUPERSCRIPT_WEIGHT × stem by moving each side half the difference
    let weight = (SUPERSCRIPT_WEIGHT - SUPERSCRIPT_SCALE) * style.stem / 2.0;
    let heavier = offset_shape(&small.with_truetype_direction(), weight);
    let bounds = heavier.bounds()?;
    let raised = heavier.translated(weight, style.cap_height - bounds.y_max);
    Some(Composed {
        glyph: GlyphData {
            shape: raised,
            advance: glyph.advance * SUPERSCRIPT_SCALE + 2.0 * weight,
        },
        built_from: vec![base],
        sentence: format!("“{c}” was built from “{base}”, made smaller and raised."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::shape::tests::square;
    use crate::matching::shape::Contour;
    use crate::matching::style::measure_style;

    fn glyph(contours: Vec<Contour>, advance: f64) -> GlyphData {
        GlyphData {
            shape: Shape { contours },
            advance,
        }
    }

    /// box letters: "e" and "a" up to the x-height, "è" with a grave box
    /// above, "i" with a dot, "E" up to the capital height.
    fn letters() -> GlyphSet {
        let mut set = GlyphSet::default();
        set.glyphs
            .insert('e', glyph(vec![square(0.05, 0.0, 0.45, 0.5)], 0.5));
        set.glyphs
            .insert('a', glyph(vec![square(0.04, 0.0, 0.44, 0.5)], 0.48));
        set.glyphs
            .insert('u', glyph(vec![square(0.06, 0.0, 0.46, 0.5)], 0.52));
        set.glyphs.insert(
            'è',
            glyph(
                vec![square(0.05, 0.0, 0.45, 0.5), square(0.15, 0.58, 0.29, 0.7)],
                0.5,
            ),
        );
        set.glyphs.insert(
            'i',
            glyph(
                vec![square(0.06, 0.0, 0.14, 0.5), square(0.06, 0.6, 0.14, 0.68)],
                0.2,
            ),
        );
        set.glyphs
            .insert('E', glyph(vec![square(0.08, 0.0, 0.5, 0.7)], 0.56));
        set.glyphs
            .insert('x', glyph(vec![square(0.05, 0.0, 0.45, 0.5)], 0.5));
        set.glyphs
            .insert('l', glyph(vec![square(0.06, 0.0, 0.14, 0.72)], 0.2));
        set.glyphs
            .insert('2', glyph(vec![square(0.05, 0.0, 0.45, 0.7)], 0.5));
        set
    }

    #[test]
    fn a_grave_moves_from_e_to_u() {
        let set = letters();
        let style = measure_style(&set);
        let made = compose_accented('ù', &set, &[(&set, "")], &style).unwrap();
        assert_eq!(made.built_from, vec!['u', 'è']);
        assert_eq!(
            made.sentence,
            "“ù” was built from “u” and the grave accent of “è”."
        );
        let mark = &made.glyph.shape.contours[1];
        let bounds = mark.bounds().unwrap();
        // same gap (0.08) above the letter, same offset left of centre
        assert!((bounds.y_min - 0.58).abs() < 1e-9, "{bounds:?}");
        let u_centre = 0.26;
        let source_offset = 0.22 - 0.25;
        assert!((bounds.centre_x() - (u_centre + source_offset)).abs() < 1e-9);
        assert_eq!(made.glyph.advance, 0.52);
    }

    #[test]
    fn capitals_get_flatter_lower_marks() {
        let set = letters();
        let style = measure_style(&set);
        let made = compose_accented('È', &set, &[(&set, "")], &style).unwrap();
        let mark = made.glyph.shape.contours[1].bounds().unwrap();
        assert!((mark.y_min - (0.7 + 0.08 * 0.7)).abs() < 1e-9, "{mark:?}");
        assert!((mark.height() - 0.12 * 0.85).abs() < 1e-9);
    }

    #[test]
    fn dotless_i_takes_the_accent() {
        let set = letters();
        let style = measure_style(&set);
        let made = compose_accented('ì', &set, &[(&set, "")], &style).unwrap();
        // the dot is gone and the grave sits on the stem
        assert_eq!(made.glyph.shape.contours.len(), 2);
        assert_eq!(made.built_from[0], 'i');
        assert!(
            compose_accented('ñ', &set, &[(&set, "")], &style).is_none(),
            "no tilde anywhere"
        );
    }

    #[test]
    fn ligatures_and_superscripts() {
        let mut set = letters();
        set.glyphs
            .insert('f', glyph(vec![square(0.05, 0.0, 0.25, 0.72)], 0.3));
        let fi = compose_ligature('ﬁ', &set).unwrap();
        assert!((fi.glyph.advance - 0.5).abs() < 1e-12);
        assert_eq!(fi.built_from, vec!['f', 'i']);
        let style = measure_style(&set);
        let two = compose_superscript('²', &set, &style).unwrap();
        let bounds = two.glyph.shape.bounds().unwrap();
        assert!((bounds.y_max - style.cap_height).abs() < 1e-6);
        assert!(bounds.height() < 0.5);
    }
}
