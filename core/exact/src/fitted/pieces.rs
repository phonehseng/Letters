//! which chars become one pre-composed glyph: lines, runs, pieces.
//!
//! a run is a stretch of one line with the same grid font, colour and
//! visibility (round one's chars, so the text is exactly round one's). a
//! piece is what one substitution rule turns into one glyph: a word with
//! the spaces after it, cut shorter where a glyph would reach too far.

use crate::hybrid::PlannedLine;

/// a piece's glyph must stay within this many font units of its origin:
/// glyf coordinates are signed 16-bit, and the margin leaves room for the
/// baked residual (fitted-font report, "fitted_pieces").
pub const PIECE_REACH_UNITS: f64 = 30000.0;
/// and within this many ems: word's pdf export failed on any glyph whose
/// ink reached 32.7 em from its origin; 32 worked, so 31.5 leaves room for
/// the last glyph's width (fitted-font probe c2).
pub const PIECE_REACH_EMS: f64 = 31.5;

/// one char of a line as round one writes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub char: String,
    pub family: String,
    pub half_points: i64,
    pub color: String,
    pub hidden: bool,
    /// the source glyph's pen origin and advance (none for a virtual space)
    pub source: Option<(f64, f64, f64)>,
    /// points along the baseline from the line's first glyph
    pub along: f64,
}

impl Item {
    /// drawn with ink: a component of its piece's composite. spaces are
    /// left out (word's export failed on composites holding several, c3).
    pub fn inked(&self) -> bool {
        self.source.is_some() && !self.char.trim().is_empty() && !self.hidden
    }
}

/// where word drew a piece's glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub origin: (f64, f64),
    pub size: f64,
}

/// what one substitution rule turns into one glyph.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub items: Vec<Item>,
    /// where word drew it in the measuring export
    pub drawn: Option<Drawn>,
}

impl Piece {
    pub fn text(&self) -> String {
        self.items.iter().map(|i| i.char.as_str()).collect()
    }
}

/// a same-style stretch of a line: one slot (font copy and feature).
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub items: Vec<Item>,
    pub pieces: Vec<Piece>,
    pub face: usize,
    /// index into `faces::FEATURES`
    pub feature: usize,
    /// per piece, its composite's glyph id in its font copy (none: no rule)
    pub ids: Vec<Option<u16>>,
}

impl Run {
    pub fn text(&self) -> String {
        self.items.iter().map(|i| i.char.as_str()).collect()
    }

    fn style(&self) -> (&str, i64, &str, bool) {
        let first = &self.items[0];
        (&first.family, first.half_points, &first.color, first.hidden)
    }
}

/// round one's chars of a line with their source positions.
pub fn line_items(planned: &PlannedLine) -> Vec<Item> {
    let line = &planned.line;
    let mut items: Vec<Item> = Vec::new();
    for (run, source) in planned.runs.iter().zip(&planned.plan.sources) {
        let (position, along) = match source {
            Some(index) => {
                let glyph = &line.glyphs[*index];
                (
                    Some((glyph.x, glyph.y, glyph.advance)),
                    line.along(glyph.x, glyph.y),
                )
            }
            // a virtual space sits at the previous glyph's pen plus its advance (recipe 5)
            None => {
                let along = items
                    .last()
                    .map(|previous| previous.along + previous.source.map_or(0.0, |s| s.2))
                    .unwrap_or(0.0);
                (None, along)
            }
        };
        items.push(Item {
            char: run.char.clone(),
            family: run.family.clone(),
            half_points: run.half_points,
            color: run.color.clone(),
            hidden: run.hidden,
            source: position,
            along,
        });
    }
    items
}

/// a char outside the basic plane: a rule holding one never applied in
/// word, while rules just before and after it did (fitted-font probe c4),
/// so it is a run of its own.
pub fn outside_basic_plane(text: &str) -> bool {
    text.chars().any(|c| c as u32 > 0xFFFF)
}

/// same-style runs, also cut before and after every char outside the basic plane.
pub fn split_runs(items: Vec<Item>) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for item in items {
        let style = (
            item.family.as_str(),
            item.half_points,
            item.color.as_str(),
            item.hidden,
        );
        let breaks = outside_basic_plane(&item.char)
            || runs.last().is_some_and(|r| {
                outside_basic_plane(&r.items.last().expect("runs have items").char)
            });
        match runs.last_mut() {
            Some(run) if run.style() == style && !breaks => run.items.push(item),
            _ => runs.push(Run {
                items: vec![item],
                pieces: Vec::new(),
                face: 0,
                feature: 0,
                ids: Vec::new(),
            }),
        }
    }
    runs
}

/// each word with the spaces after it; spaces at the start go with the first word.
pub fn words(items: &[Item]) -> Vec<Vec<Item>> {
    let mut words: Vec<Vec<Item>> = Vec::new();
    for item in items {
        let starts_word = item.char != " "
            && words
                .last()
                .is_some_and(|w| w.last().is_some_and(|i| i.char == " "));
        if words.is_empty() || starts_word {
            words.push(vec![item.clone()]);
        } else {
            words
                .last_mut()
                .expect("a word was started")
                .push(item.clone());
        }
    }
    words
}

/// how far a group's last ink reaches from its first glyph, in font units
/// of a grid font with `units_per_em` drawn at `drawn_size`.
fn reach_units(items: &[Item], units_per_em: f64, drawn_size: f64) -> f64 {
    let sources: Vec<(f64, f64, f64)> = items.iter().filter_map(|i| i.source).collect();
    let (Some(first), Some(last)) = (sources.first(), sources.last()) else {
        return 0.0;
    };
    let distance = (last.0 - first.0).hypot(last.1 - first.1) + last.2;
    distance * units_per_em / drawn_size
}

/// cuts a word so no piece reaches past the limits above.
pub fn cut_by_reach(items: Vec<Item>, units_per_em: f64, drawn_size: f64) -> Vec<Vec<Item>> {
    let limit = PIECE_REACH_UNITS.min(PIECE_REACH_EMS * units_per_em);
    let mut pieces: Vec<Vec<Item>> = vec![Vec::new()];
    for item in items {
        let current = pieces.last_mut().expect("there is a piece");
        let mut longer = current.clone();
        longer.push(item.clone());
        if !current.is_empty() && reach_units(&longer, units_per_em, drawn_size) > limit {
            pieces.push(vec![item]);
        } else {
            current.push(item);
        }
    }
    pieces
}

/// the pieces of a run; none for hidden text (word doesn't draw it) or a
/// run of a char outside the basic plane.
pub fn make_pieces(run: &mut Run, units_per_em: f64, drawn_size: f64) {
    let first = &run.items[0];
    if first.hidden || outside_basic_plane(&run.text()) {
        return;
    }
    for word in words(&run.items) {
        for chunk in cut_by_reach(word, units_per_em, drawn_size) {
            run.pieces.push(Piece {
                items: chunk,
                drawn: None,
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn item(char: &str, along: f64) -> Item {
        Item {
            char: char.into(),
            family: "Fam".into(),
            half_points: 20,
            color: "000000".into(),
            hidden: false,
            source: (char != " ").then_some((along, 0.0, 5.0)),
            along,
        }
    }

    #[test]
    fn words_keep_their_following_spaces() {
        let items: Vec<Item> = "ab  cd e"
            .chars()
            .enumerate()
            .map(|(i, c)| item(&c.to_string(), 5.0 * i as f64))
            .collect();
        let texts: Vec<String> = words(&items)
            .iter()
            .map(|w| w.iter().map(|i| i.char.as_str()).collect())
            .collect();
        assert_eq!(texts, ["ab  ", "cd ", "e"]);
    }

    #[test]
    fn chars_outside_the_basic_plane_are_runs_of_their_own() {
        let items = vec![
            item("a", 0.0),
            item("\u{F0000}", 5.0),
            item("b", 10.0),
            item("c", 15.0),
        ];
        let texts: Vec<String> = split_runs(items).iter().map(Run::text).collect();
        assert_eq!(texts, ["a", "\u{F0000}", "bc"]);
    }

    #[test]
    fn long_words_are_cut_before_they_reach_too_far() {
        // 10 pt glyphs 5 pt apart, 1000 units per em at 10 pt: 500 units a glyph
        let items: Vec<Item> = (0..100).map(|i| item("a", 5.0 * i as f64)).collect();
        let pieces = cut_by_reach(items, 1000.0, 10.0);
        assert!(pieces.len() > 1);
        for piece in &pieces {
            assert!(reach_units(piece, 1000.0, 10.0) <= PIECE_REACH_EMS * 1000.0);
        }
    }
}
