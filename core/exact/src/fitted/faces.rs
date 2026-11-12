//! the fitted fonts: per grid font, one composite glyph per piece and the
//! substitution rules that pick it, spread over slots (a font copy and a
//! feature the docx run switches on).
//!
//! in one slot no two runs may start with the same piece text: a run's
//! first piece has no glyph before it to tell occurrences apart. later
//! pieces backtrack on the glyph of the piece before them. word applies
//! liga with `w14:ligatures standard` and ss01 to ss20 each with its own
//! `w14:stylisticSets` entry, so one font copy gives 21 slots
//! (fitted-font probe c1); a copy under another family name gives 21 more.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::pieces::{Piece, Run};
use crate::fonts::composite::{write_truetype_with, Component, CompositeGlyph};
use crate::fonts::grid::GridFont;
use crate::fonts::gsub::{gsub_table, FeatureRules, Rule};
use crate::fonts::truetype::FontSpec;
use crate::units::{round_half_even, PT_PER_PIXEL};
use crate::word::sizes::{drawn_size, GRID_UNITS_PER_PIXEL};

/// the slot features, in the order they are handed out.
pub const FEATURES: [&[u8; 4]; 21] = [
    b"liga", b"ss01", b"ss02", b"ss03", b"ss04", b"ss05", b"ss06", b"ss07", b"ss08", b"ss09",
    b"ss10", b"ss11", b"ss12", b"ss13", b"ss14", b"ss15", b"ss16", b"ss17", b"ss18", b"ss19",
    b"ss20",
];

/// the family name of a font copy.
pub fn face_family(base: &str, face: usize) -> String {
    if face == 0 {
        base.to_string()
    } else {
        format!("{base} f{face}")
    }
}

/// gives every run with pieces a slot; returns how many font copies are needed.
pub fn assign_slots(runs: &mut [&mut Run]) -> usize {
    let mut taken: HashMap<(usize, usize), BTreeSet<String>> = HashMap::new();
    let mut faces = 1;
    for run in runs.iter_mut() {
        let Some(first) = run.pieces.first().map(Piece::text) else {
            continue;
        };
        let mut slot = 0;
        loop {
            let key = (slot / FEATURES.len(), slot % FEATURES.len());
            if taken.entry(key).or_default().insert(first.clone()) {
                (run.face, run.feature) = key;
                faces = faces.max(run.face + 1);
                break;
            }
            slot += 1;
        }
    }
    faces
}

/// where a piece's glyph starts if word puts it exactly at its source.
pub fn expected_origin(piece: &Piece, line_x: f64, line_y: f64) -> (f64, f64) {
    (line_x + piece.items[0].along, line_y)
}

/// each inked glyph's offset from the piece's origin in font units: from
/// where word drew the piece when it was measured, else from where it is
/// expected, at the size word draws (recipe 4: whole pixels per em).
pub fn component_offsets(
    piece: &Piece,
    grid: &GridFont,
    line_x: f64,
    line_y: f64,
) -> Vec<(i32, i32)> {
    let (origin, size) = match piece.drawn {
        Some(drawn) => (drawn.origin, drawn.size),
        None => (
            expected_origin(piece, line_x, line_y),
            drawn_size(grid.stored_size),
        ),
    };
    let units = grid.units_per_em as f64 / size;
    piece
        .items
        .iter()
        .filter(|i| i.inked())
        .filter_map(|i| i.source)
        .map(|(x, y, _)| {
            (
                round_half_even((x - origin.0) * units) as i32,
                round_half_even((y - origin.1) * units) as i32,
            )
        })
        .collect()
}

/// a piece's layout advance: to the next piece's source start, in whole
/// pixels, so word's layout stays on its own 1/600 in grid and the next
/// piece starts where the source has it, to the pixel. the last piece of a
/// line takes its chars' own advances.
pub fn piece_advance(piece: &Piece, next_along: Option<f64>, grid: &GridFont) -> u16 {
    let units = match next_along {
        Some(next) => {
            let pixels = round_half_even((next - piece.items[0].along) / PT_PER_PIXEL) as i64;
            pixels * GRID_UNITS_PER_PIXEL
        }
        None => piece
            .items
            .iter()
            .map(|i| grid.pixels(i.char.chars().next().unwrap_or(' ')) * GRID_UNITS_PER_PIXEL)
            .sum(),
    };
    units.clamp(0, i64::from(u16::MAX)) as u16
}

/// per run of a font copy, per piece, its composite's glyph id (none: no rule).
pub type RunIds = Vec<Vec<Option<u16>>>;

/// one run's pieces as composites and rules, for one font copy.
pub struct RunGlyphs<'a> {
    pub run: &'a Run,
    pub line_x: f64,
    pub line_y: f64,
    /// where the next run of the same line starts, if any
    pub next_run_along: Option<f64>,
}

/// glyph ids of a piece's chars; none when the font lacks one.
fn input_glyphs(piece: &Piece, cmap: &BTreeMap<u32, u16>) -> Option<Vec<u16>> {
    piece
        .items
        .iter()
        .map(|i| {
            i.char
                .chars()
                .next()
                .and_then(|c| cmap.get(&(c as u32)).copied())
        })
        .collect()
}

/// writes one font copy of a grid font with its runs' composites and rules.
/// returns the font and, per run and piece, the composite's glyph id.
pub fn build_face(
    grid: &GridFont,
    face: usize,
    runs: &[RunGlyphs],
) -> Result<(Vec<u8>, RunIds), String> {
    let mut composites: Vec<CompositeGlyph> = Vec::new();
    let mut rules: BTreeMap<usize, Vec<Rule>> = BTreeMap::new();
    let mut ids = Vec::new();
    let first_id = grid.glyphs.len();
    for entry in runs {
        let run = entry.run;
        let mut run_ids = Vec::new();
        let mut previous: Option<u16> = None;
        for (index, piece) in run.pieces.iter().enumerate() {
            let Some(input) = input_glyphs(piece, &grid.cmap) else {
                run_ids.push(None);
                previous = None;
                continue;
            };
            let id = (first_id + composites.len()) as u16;
            let next_along = run
                .pieces
                .get(index + 1)
                .map(|p| p.items[0].along)
                .or(entry.next_run_along);
            let components = component_offsets(piece, grid, entry.line_x, entry.line_y);
            let glyphs = piece.items.iter().filter(|i| i.inked());
            composites.push(CompositeGlyph {
                components: glyphs
                    .zip(components)
                    .map(|(item, (dx, dy))| Component {
                        glyph: grid.cmap[&(item.char.chars().next().unwrap_or(' ') as u32)],
                        dx,
                        dy,
                    })
                    .collect(),
                advance: piece_advance(piece, next_along, grid),
            });
            rules.entry(run.feature).or_default().push(Rule {
                backtrack: previous.into_iter().collect(),
                input,
                lookahead: first_piece_lookahead(run, index, &grid.cmap),
                glyph: id,
            });
            previous = Some(id);
            run_ids.push(Some(id));
        }
        ids.push(run_ids);
    }
    let features: Vec<FeatureRules> = rules
        .into_iter()
        .map(|(feature, rules)| FeatureRules {
            tag: *FEATURES[feature],
            rules,
        })
        .collect();
    let family = face_family(&grid.family, face);
    let spec = FontSpec {
        family: &family,
        units_per_em: grid.units_per_em as u16,
        glyphs: &grid.glyphs,
        cmap: &grid.cmap,
        ascent: grid.ascent,
        descent: grid.descent,
        fs_type: grid.fs_type,
    };
    let gsub = gsub_table(&features)?;
    Ok((
        write_truetype_with(&spec, &composites, vec![(*b"GSUB", gsub)]),
        ids,
    ))
}

/// a run's first piece also asks for the first char of the piece after it:
/// without context its rule could match a later copy of the same word once
/// an edit has broken the chain before that copy (the fitted-font report's
/// known flaw; `gsub::tests::after_an_edit_a_later_copy_...`).
fn first_piece_lookahead(run: &Run, index: usize, cmap: &BTreeMap<u32, u16>) -> Vec<u16> {
    if index != 0 {
        return Vec::new();
    }
    run.pieces
        .get(1)
        .and_then(|next| next.items[0].char.chars().next())
        .and_then(|c| cmap.get(&(c as u32)).copied())
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::pieces::tests::item;
    use super::*;

    fn run_starting_with(text: &str) -> Run {
        Run {
            items: vec![item(text, 0.0)],
            pieces: vec![Piece {
                items: vec![item(text, 0.0)],
                drawn: None,
            }],
            face: 0,
            feature: 0,
            ids: Vec::new(),
        }
    }

    #[test]
    fn runs_with_the_same_first_piece_get_different_slots() {
        let mut runs: Vec<Run> = (0..23).map(|_| run_starting_with("a")).collect();
        let mut references: Vec<&mut Run> = runs.iter_mut().collect();
        let faces = assign_slots(&mut references);
        let slots: BTreeSet<(usize, usize)> = runs.iter().map(|r| (r.face, r.feature)).collect();
        assert_eq!(slots.len(), 23);
        assert_eq!(faces, 2);
        assert_eq!((runs[21].face, runs[21].feature), (1, 0));
    }

    #[test]
    fn different_first_pieces_share_a_slot() {
        let mut runs = [run_starting_with("a"), run_starting_with("b")];
        let mut references: Vec<&mut Run> = runs.iter_mut().collect();
        assert_eq!(assign_slots(&mut references), 1);
        assert_eq!((runs[1].face, runs[1].feature), (0, 0));
    }
}
