//! a gsub table for the fitted font: per feature one chaining lookup whose
//! rules replace a piece's typed glyphs by the piece's composite glyph.
//!
//! all rules of a feature sit in one chaining lookup so they are applied in
//! one left-to-right pass: when piece k is reached, piece k - 1 has already
//! become its own glyph, which is what piece k's backtrack names. the
//! substitutions themselves are nested lookups the chaining rules call
//! (ligature substitution, or single substitution for a one-glyph piece).
//! every lookup is wrapped in extension lookups (type 7) so the table can
//! grow past the 64 kB that 16-bit offsets reach (opentype gsub chapter).

use std::collections::BTreeSet;

/// one substitution with its context, in glyph ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// glyphs right before the input, in text order
    pub backtrack: Vec<u16>,
    pub input: Vec<u16>,
    /// glyphs right after the input, in text order
    pub lookahead: Vec<u16>,
    /// what the input becomes
    pub glyph: u16,
}

/// the rules a feature (liga, ss01 ... ss20) applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeatureRules {
    pub tag: [u8; 4],
    pub rules: Vec<Rule>,
}

const SINGLE: u16 = 1;
const LIGATURE: u16 = 4;
const CHAIN: u16 = 6;
const EXTENSION: u16 = 7;

/// a lookup before layout: its real type and its subtables.
struct Lookup {
    kind: u16,
    subtables: Vec<Vec<u8>>,
}

fn u16s(out: &mut Vec<u8>, values: &[u16]) {
    for value in values {
        out.extend_from_slice(&value.to_be_bytes());
    }
}

/// coverage format 1 of sorted glyph ids.
fn coverage(glyphs: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(&mut out, &[1, glyphs.len() as u16]);
    u16s(&mut out, glyphs);
    out
}

/// the substitutions a chaining lookup calls, grouped so that one lookup
/// never holds two rules starting with the same glyph (a ligature lookup
/// takes the longest ligature at a position, which could be another rule's)
/// and never mixes single and ligature substitutions (types 1 and 4).
fn nested_groups(rules: &[Rule]) -> Vec<Vec<usize>> {
    let mut groups: Vec<(bool, BTreeSet<u16>, Vec<usize>)> = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        let single = rule.input.len() == 1;
        let first = rule.input[0];
        match groups
            .iter_mut()
            .find(|(kind, firsts, _)| *kind == single && !firsts.contains(&first))
        {
            Some((_, firsts, members)) => {
                firsts.insert(first);
                members.push(index);
            }
            None => groups.push((single, BTreeSet::from([first]), vec![index])),
        }
    }
    groups.into_iter().map(|(_, _, members)| members).collect()
}

/// single substitution format 2.
fn single_subtable(rules: &[&Rule]) -> Vec<u8> {
    let mut sorted: Vec<&&Rule> = rules.iter().collect();
    sorted.sort_by_key(|r| r.input[0]);
    let firsts: Vec<u16> = sorted.iter().map(|r| r.input[0]).collect();
    let substitutes: Vec<u16> = sorted.iter().map(|r| r.glyph).collect();
    let mut out = Vec::new();
    let coverage_offset = 6 + 2 * substitutes.len();
    u16s(
        &mut out,
        &[2, coverage_offset as u16, substitutes.len() as u16],
    );
    u16s(&mut out, &substitutes);
    out.extend(coverage(&firsts));
    out
}

/// ligature substitution format 1, one ligature per first glyph.
fn ligature_subtable(rules: &[&Rule]) -> Vec<u8> {
    let mut sorted: Vec<&&Rule> = rules.iter().collect();
    sorted.sort_by_key(|r| r.input[0]);
    let firsts: Vec<u16> = sorted.iter().map(|r| r.input[0]).collect();
    // header, then the ligature sets, then the coverage
    let header = 6 + 2 * sorted.len();
    let mut sets: Vec<u8> = Vec::new();
    let mut set_offsets = Vec::new();
    for rule in &sorted {
        set_offsets.push((header + sets.len()) as u16);
        // a set of one ligature: count, offset 4, then the ligature
        u16s(&mut sets, &[1, 4, rule.glyph, rule.input.len() as u16]);
        u16s(&mut sets, &rule.input[1..]);
    }
    let mut out = Vec::new();
    u16s(
        &mut out,
        &[1, (header + sets.len()) as u16, sorted.len() as u16],
    );
    u16s(&mut out, &set_offsets);
    out.extend(sets);
    out.extend(coverage(&firsts));
    out
}

/// chaining context format 3: one coverage per context glyph.
fn chain_subtable(rule: &Rule, nested: u16) -> Vec<u8> {
    // backtrack coverages go nearest glyph first
    let backtrack: Vec<u16> = rule.backtrack.iter().rev().copied().collect();
    let all: Vec<u16> = backtrack
        .iter()
        .chain(&rule.input)
        .chain(&rule.lookahead)
        .copied()
        .collect();
    let header = 2
        + 2
        + 2 * backtrack.len()
        + 2
        + 2 * rule.input.len()
        + 2
        + 2 * rule.lookahead.len()
        + 2
        + 4;
    let offsets: Vec<u16> = (0..all.len()).map(|i| (header + 6 * i) as u16).collect();
    let (back, rest) = offsets.split_at(backtrack.len());
    let (input, ahead) = rest.split_at(rule.input.len());
    let mut out = Vec::new();
    u16s(&mut out, &[3, back.len() as u16]);
    u16s(&mut out, back);
    u16s(&mut out, &[input.len() as u16]);
    u16s(&mut out, input);
    u16s(&mut out, &[ahead.len() as u16]);
    u16s(&mut out, ahead);
    u16s(&mut out, &[1, 0, nested]);
    for glyph in all {
        out.extend(coverage(&[glyph]));
    }
    out
}

/// most specific rules first: longer backtrack, then longer lookahead, then longer input.
fn ordered(rules: &[Rule]) -> Vec<&Rule> {
    let mut sorted: Vec<&Rule> = rules.iter().collect();
    sorted.sort_by_key(|r| {
        (
            std::cmp::Reverse(r.backtrack.len()),
            std::cmp::Reverse(r.lookahead.len()),
            std::cmp::Reverse(r.input.len()),
        )
    });
    sorted
}

/// every lookup of every feature, and the chaining lookup's index per feature.
fn lookups(features: &[FeatureRules]) -> (Vec<Lookup>, Vec<u16>) {
    let mut lookups = Vec::new();
    let mut chain_indexes = Vec::new();
    for feature in features {
        let groups = nested_groups(&feature.rules);
        let mut group_of = vec![0u16; feature.rules.len()];
        for members in &groups {
            let rules: Vec<&Rule> = members.iter().map(|&i| &feature.rules[i]).collect();
            for &member in members {
                group_of[member] = lookups.len() as u16;
            }
            let single = rules[0].input.len() == 1;
            let subtable = if single {
                single_subtable(&rules)
            } else {
                ligature_subtable(&rules)
            };
            let kind = if single { SINGLE } else { LIGATURE };
            lookups.push(Lookup {
                kind,
                subtables: vec![subtable],
            });
        }
        let index_of = |rule: &Rule| {
            let position = feature
                .rules
                .iter()
                .position(|r| std::ptr::eq(r, rule))
                .expect("rule of this feature");
            group_of[position]
        };
        let subtables = ordered(&feature.rules)
            .into_iter()
            .map(|rule| chain_subtable(rule, index_of(rule)))
            .collect();
        chain_indexes.push(lookups.len() as u16);
        lookups.push(Lookup {
            kind: CHAIN,
            subtables,
        });
    }
    (lookups, chain_indexes)
}

/// script list: dflt and latn, both with every feature in their default language system.
fn script_list(feature_count: usize) -> Vec<u8> {
    let lang_sys = {
        let mut out = Vec::new();
        u16s(&mut out, &[0, 0xFFFF, feature_count as u16]);
        u16s(&mut out, &(0..feature_count as u16).collect::<Vec<_>>());
        out
    };
    let script = {
        let mut out = Vec::new();
        u16s(&mut out, &[4, 0]);
        out.extend(&lang_sys);
        out
    };
    let mut out = Vec::new();
    u16s(&mut out, &[2]);
    let first = 2 + 2 * 6;
    for (number, tag) in [b"DFLT", b"latn"].iter().enumerate() {
        out.extend_from_slice(*tag);
        u16s(&mut out, &[(first + number * script.len()) as u16]);
    }
    out.extend(&script);
    out.extend(&script);
    out
}

/// feature list, sorted by tag as the format asks.
fn feature_list(features: &[FeatureRules], chain_indexes: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(&mut out, &[features.len() as u16]);
    let first = 2 + 6 * features.len();
    for (number, feature) in features.iter().enumerate() {
        out.extend_from_slice(&feature.tag);
        u16s(&mut out, &[(first + 6 * number) as u16]);
    }
    for &lookup in chain_indexes {
        u16s(&mut out, &[0, 1, lookup]);
    }
    out
}

/// the gsub table; features must be given sorted by tag.
pub fn gsub_table(features: &[FeatureRules]) -> Result<Vec<u8>, String> {
    let (lookups, chain_indexes) = lookups(features);
    let scripts = script_list(features.len());
    let feature_bytes = feature_list(features, &chain_indexes);
    let lookup_list_at = 10 + scripts.len() + feature_bytes.len();
    // each lookup: type 7, flags, count, offsets; then its 8-byte extension records
    let lookup_sizes: Vec<usize> = lookups
        .iter()
        .map(|l| 6 + 2 * l.subtables.len() + 8 * l.subtables.len())
        .collect();
    let list_size = 2 + 2 * lookups.len();
    let lookups_size: usize = lookup_sizes.iter().sum();
    if list_size + lookups_size > usize::from(u16::MAX) {
        return Err("the substitution rules don't fit one gsub table".into());
    }
    let mut out = Vec::new();
    u16s(
        &mut out,
        &[1, 0, 10, (10 + scripts.len()) as u16, lookup_list_at as u16],
    );
    out.extend(&scripts);
    out.extend(&feature_bytes);
    u16s(&mut out, &[lookups.len() as u16]);
    let mut at = list_size;
    for size in &lookup_sizes {
        u16s(&mut out, &[at as u16]);
        at += size;
    }
    let data_start = lookup_list_at + list_size + lookups_size;
    let mut data: Vec<u8> = Vec::new();
    for lookup in &lookups {
        let table_start = out.len();
        let count = lookup.subtables.len();
        u16s(&mut out, &[EXTENSION, 0, count as u16]);
        let records_at = 6 + 2 * count;
        u16s(
            &mut out,
            &(0..count)
                .map(|i| (records_at + 8 * i) as u16)
                .collect::<Vec<_>>(),
        );
        for subtable in &lookup.subtables {
            let record_at = out.len();
            let target = data_start + data.len();
            u16s(&mut out, &[1, lookup.kind]);
            out.extend_from_slice(&((target - record_at) as u32).to_be_bytes());
            data.extend(subtable);
        }
        debug_assert_eq!(out.len() - table_start, 6 + 10 * count);
    }
    out.extend(data);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_with_the_same_first_glyph_go_to_different_lookups() {
        let rule = |input: Vec<u16>| Rule {
            backtrack: Vec::new(),
            input,
            lookahead: Vec::new(),
            glyph: 9,
        };
        let groups = nested_groups(&[
            rule(vec![1, 2]),
            rule(vec![1, 3]),
            rule(vec![2, 3]),
            rule(vec![4]),
        ]);
        assert_eq!(groups, [vec![0, 2], vec![1], vec![3]]);
    }

    #[test]
    fn the_most_specific_rule_comes_first() {
        let rule = |backtrack: usize, lookahead: usize, input: usize| Rule {
            backtrack: vec![1; backtrack],
            input: vec![2; input],
            lookahead: vec![3; lookahead],
            glyph: 9,
        };
        let rules = [rule(0, 0, 3), rule(1, 0, 2), rule(0, 2, 2)];
        let order: Vec<(usize, usize)> = ordered(&rules)
            .iter()
            .map(|r| (r.backtrack.len(), r.lookahead.len()))
            .collect();
        assert_eq!(order, [(1, 0), (0, 2), (0, 0)]);
    }

    use crate::fonts::composite::{write_truetype_with, Component, CompositeGlyph};
    use crate::fonts::outline::ContourPoint;
    use crate::fonts::truetype::{FontGlyph, FontSpec};
    use std::collections::BTreeMap;

    const LETTERS: &str = "abcdx";

    fn square(width: i32) -> Vec<Vec<ContourPoint>> {
        let p = |x, y| ContourPoint {
            x,
            y,
            on_curve: true,
        };
        vec![vec![
            p(10, 0),
            p(10, 500),
            p(width - 10, 500),
            p(width - 10, 0),
        ]]
    }

    /// glyph ids: 0 notdef, 1 space, 2.. the letters; then the composites.
    fn tiny_font(composites: &[CompositeGlyph], features: &[FeatureRules]) -> Vec<u8> {
        let mut glyphs = vec![
            FontGlyph {
                contours: square(500),
                advance: 500,
            },
            FontGlyph {
                contours: Vec::new(),
                advance: 250,
            },
        ];
        let mut cmap = BTreeMap::from([(0x20, 1u16)]);
        for (index, char) in LETTERS.chars().enumerate() {
            let width = 400 + 50 * index as i32;
            cmap.insert(char as u32, glyphs.len() as u16);
            glyphs.push(FontGlyph {
                contours: square(width),
                advance: width as u16,
            });
        }
        let spec = FontSpec {
            family: "Tiny Test",
            units_per_em: 1000,
            glyphs: &glyphs,
            cmap: &cmap,
            ascent: 800,
            descent: -200,
            fs_type: 0,
        };
        write_truetype_with(
            &spec,
            composites,
            vec![(*b"GSUB", gsub_table(features).unwrap())],
        )
    }

    fn glyph_of(char: char) -> u16 {
        if char == ' ' {
            1
        } else {
            2 + LETTERS.find(char).unwrap() as u16
        }
    }

    /// a composite for `text` at natural offsets, glyph id `id`.
    fn piece(text: &str) -> CompositeGlyph {
        let mut pen = 0;
        let mut components = Vec::new();
        for char in text.chars() {
            let glyph = glyph_of(char);
            if char != ' ' {
                components.push(Component {
                    glyph,
                    dx: pen,
                    dy: 0,
                });
            }
            pen += if char == ' ' {
                250
            } else {
                400 + 50 * (glyph as i32 - 2)
            };
        }
        CompositeGlyph {
            components,
            advance: pen as u16,
        }
    }

    fn rule(text: &str, glyph: u16, backtrack: &[u16], lookahead: &str) -> Rule {
        Rule {
            backtrack: backtrack.to_vec(),
            input: text.chars().map(glyph_of).collect(),
            lookahead: lookahead.chars().map(glyph_of).collect(),
            glyph,
        }
    }

    /// glyph ids rustybuzz produces with only the given features on.
    fn shaped(font: &[u8], text: &str, on: &[&[u8; 4]]) -> Vec<u16> {
        let face = rustybuzz::Face::from_slice(font, 0).unwrap();
        let mut features = vec![rustybuzz::Feature::new(
            rustybuzz::ttf_parser::Tag::from_bytes(b"liga"),
            0,
            ..,
        )];
        for tag in on {
            features.push(rustybuzz::Feature::new(
                rustybuzz::ttf_parser::Tag::from_bytes(tag),
                1,
                ..,
            ));
        }
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let glyphs = rustybuzz::shape(&face, &features, buffer);
        glyphs
            .glyph_infos()
            .iter()
            .map(|i| i.glyph_id as u16)
            .collect()
    }

    // glyph ids of the composites: after the 7 simple glyphs
    const P0: u16 = 7;
    const P1: u16 = 8;
    const P2: u16 = 9;
    const P3: u16 = 10;

    #[test]
    fn pieces_chain_on_the_glyph_before_them() {
        let composites = [piece("ab "), piece("cd ")];
        let rules = vec![rule("ab ", P0, &[], "c"), rule("cd ", P1, &[P0], "")];
        let font = tiny_font(
            &composites,
            &[FeatureRules {
                tag: *b"liga",
                rules,
            }],
        );
        assert_eq!(shaped(&font, "ab cd ", &[b"liga"]), [P0, P1]);
        // without the feature nothing is replaced
        assert_eq!(shaped(&font, "ab cd ", &[]).len(), 6);
    }

    #[test]
    fn an_edited_word_drops_only_its_piece_and_the_ones_after_it() {
        let composites = [piece("ab "), piece("cd "), piece("xa")];
        let rules = vec![
            rule("ab ", P0, &[], "c"),
            rule("cd ", P1, &[P0], ""),
            rule("xa", P2, &[P1], ""),
        ];
        let font = tiny_font(
            &composites,
            &[FeatureRules {
                tag: *b"liga",
                rules,
            }],
        );
        assert_eq!(shaped(&font, "ab cd xa", &[b"liga"]), [P0, P1, P2]);
        let edited = shaped(&font, "ab cc xa", &[b"liga"]);
        assert_eq!(edited[0], P0);
        assert!(!edited.contains(&P1) && !edited.contains(&P2));
    }

    #[test]
    fn after_an_edit_a_later_copy_of_the_first_word_keeps_its_own_glyphs() {
        // "ab cd ab xx": the run's first piece "ab " and a later "ab " after "cd "
        let composites = [piece("ab "), piece("cd "), piece("ab "), piece("xx")];
        let rules = vec![
            rule("ab ", P0, &[], "c"),
            rule("cd ", P1, &[P0], ""),
            rule("ab ", P2, &[P1], ""),
            rule("xx", P3, &[P2], ""),
        ];
        let font = tiny_font(
            &composites,
            &[FeatureRules {
                tag: *b"liga",
                rules,
            }],
        );
        assert_eq!(shaped(&font, "ab cd ab xx", &[b"liga"]), [P0, P1, P2, P3]);
        // "cd" edited to "cc": the later "ab " must not take the first piece's glyph
        let edited = shaped(&font, "ab cc ab xx", &[b"liga"]);
        assert_eq!(edited[0], P0);
        assert_eq!(edited.iter().filter(|&&g| g == P0).count(), 1, "{edited:?}");
    }

    #[test]
    fn the_same_word_in_another_slot_gets_its_own_glyph() {
        let composites = [piece("ab"), piece("ab")];
        let features = [
            FeatureRules {
                tag: *b"liga",
                rules: vec![rule("ab", P0, &[], "")],
            },
            FeatureRules {
                tag: *b"ss01",
                rules: vec![rule("ab", P1, &[], "")],
            },
        ];
        let font = tiny_font(&composites, &features);
        assert_eq!(shaped(&font, "ab", &[b"liga"]), [P0]);
        assert_eq!(shaped(&font, "ab", &[b"ss01"]), [P1]);
    }

    #[test]
    fn a_one_glyph_piece_uses_a_single_substitution() {
        let composites = [piece("a")];
        let font = tiny_font(
            &composites,
            &[FeatureRules {
                tag: *b"liga",
                rules: vec![rule("a", P0, &[], "")],
            }],
        );
        assert_eq!(shaped(&font, "a", &[b"liga"]), [P0]);
    }

    #[test]
    fn composites_keep_their_exact_offsets_and_advance() {
        let composites = [CompositeGlyph {
            components: vec![
                Component {
                    glyph: 2,
                    dx: 3,
                    dy: -7,
                },
                Component {
                    glyph: 3,
                    dx: 4123,
                    dy: 11,
                },
            ],
            advance: 1234,
        }];
        let font = tiny_font(
            &composites,
            &[FeatureRules {
                tag: *b"liga",
                rules: vec![rule("ab", P0, &[], "")],
            }],
        );
        let face = ttf_parser::Face::parse(&font, 0).unwrap();
        let id = ttf_parser::GlyphId(P0);
        assert_eq!(face.glyph_hor_advance(id), Some(1234));
        let mut recorder = crate::fonts::outline::Recorder::default();
        let bounds = face.outline_glyph(id, &mut recorder).unwrap();
        // a's square starts at x 10, b's (450 wide) ends at 440: 4123 + 440
        assert_eq!((bounds.x_min, bounds.x_max, bounds.y_min), (13, 4563, -7));
    }
}
