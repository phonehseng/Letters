//! font completion end to end: a subset of a real font (Atkinson
//! Hyperlegible, in tests/fixtures under the OFL) is given new letters by
//! each strategy, and the completed font is checked as a file.

use std::collections::BTreeMap;

use letters_fonts::complete::{
    complete_font, complete_font_with, identify, plan_generation, CompleteOptions, FakeGenerator,
    Generation, GlyphGenerator, Strategy, SubsetFont,
};
use letters_fonts::matching::distance::{shape_distance, Samples};
use letters_fonts::matching::donors::{DonorFile, DonorPool};
use letters_fonts::matching::glyphs::read_glyphs;
use letters_fonts::matching::style::measure_style;
use letters_fonts::model::Source;
use ttf_parser::{Face, GlyphId};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn regular() -> Vec<u8> {
    fixture("AtkinsonHyperlegible-Regular.ttf")
}

fn bold() -> Vec<u8> {
    fixture("AtkinsonHyperlegible-Bold.ttf")
}

/// the subset a pdf would embed for `text`: other glyphs emptied,
/// numbers kept, only the text's characters mapped.
fn subset_of(bytes: &[u8], text: &str) -> SubsetFont {
    let face = Face::parse(bytes, 0).unwrap();
    let map: BTreeMap<char, u16> = text
        .chars()
        .filter_map(|c| Some((c, face.glyph_index(c)?.0)))
        .collect();
    let keep = map.values().copied().collect();
    let subset = letters_fonts::subset::subset_truetype(bytes, 0, &keep).unwrap();
    SubsetFont::from_font_bytes(&subset, 0, Some(&map))
        .unwrap()
        .with_name("QWERTY+AtkinsonHyperlegible-Regular")
}

fn pool(fonts: &[(&str, Vec<u8>)]) -> DonorPool {
    DonorPool {
        donors: fonts
            .iter()
            .map(|(key, bytes)| {
                DonorFile::from_bytes(Source::Bundled, key, bytes.clone(), 0).unwrap()
            })
            .collect(),
    }
}

const FIRST: &str = "The annual report shows revenue rose in the third quarter.";
const SECOND: &str = "Jack mixed big jazz kits by Friday";

#[test]
fn the_same_font_is_identified_and_lends_its_glyphs_unchanged() {
    let subset = subset_of(&regular(), FIRST);
    let donors = pool(&[("bold", bold()), ("regular", regular())]);
    let candidates = identify(&subset, &donors, &CompleteOptions::default());
    assert!(candidates[0].same_design);
    assert_eq!(candidates[0].key, "regular");

    let completed = complete_font(&subset, SECOND, &donors, &CompleteOptions::default()).unwrap();
    assert!(completed
        .glyphs
        .iter()
        .all(|g| g.strategy == Strategy::ExactDonor));
    assert_eq!(
        completed.report.sentences[0],
        "Found the same font that comes with Letters: Atkinson Hyperlegible."
    );
    // the new glyphs are the true ones
    let truth = read_glyphs(&regular(), 0, &[], SECOND.chars()).unwrap();
    for (c, glyph) in &completed.new_glyphs {
        let distance = shape_distance(
            &Samples::of(&glyph.shape),
            &Samples::of(&truth.glyphs[c].shape),
        );
        assert!(distance.max < 1e-9, "{c}");
        assert_eq!(glyph.advance, truth.glyphs[c].advance);
    }
}

#[test]
fn the_completed_font_keeps_old_glyphs_and_draws_new_ones() {
    let subset = subset_of(&regular(), FIRST);
    let donors = pool(&[("regular", regular())]);
    let completed = complete_font(&subset, SECOND, &donors, &CompleteOptions::default()).unwrap();
    let before = Face::parse(subset.truetype.as_deref().unwrap(), 0).unwrap();
    let after = Face::parse(&completed.font, 0).unwrap();
    for (c, glyph) in &subset.glyph_ids {
        assert_eq!(completed.char_to_glyph[c], *glyph, "“{c}” kept its number");
        let old = letters_fonts::matching::glyphs::glyph_shape(&before, GlyphId(*glyph));
        let new = letters_fonts::matching::glyphs::glyph_shape(&after, GlyphId(*glyph));
        assert_eq!(old, new);
    }
    for c in "Jkxbzy".chars() {
        let glyph = after.glyph_index(c).expect("the new letter is mapped");
        assert!(
            glyph.0 >= before.number_of_glyphs(),
            "new glyphs come after the old ones"
        );
        assert!(after.glyph_bounding_box(glyph).is_some());
    }
    let family = letters_fonts::facts::name(&after, ttf_parser::name_id::FAMILY).unwrap();
    assert!(family.starts_with("Letters Completed "), "{family}");
    assert!(!family.contains("Atkinson"));
}

#[test]
fn another_weight_is_fitted_and_reports_how_alike_it_is() {
    let subset = subset_of(&regular(), FIRST);
    let donors = pool(&[("bold", bold())]);
    let completed = complete_font(&subset, SECOND, &donors, &CompleteOptions::default()).unwrap();
    assert!(completed
        .glyphs
        .iter()
        .all(|g| g.strategy == Strategy::FittedDonor));
    let fit = completed.report.fit.as_ref().unwrap();
    assert!(fit.similarity > fit.unchanged_similarity, "{fit:?}");
    assert!(fit.transform.weight < 0.0, "bold is thinned: {fit:?}");
    assert!(completed.report.sentences[0].contains("adjusted to match"));
    // the fitted glyphs are much closer to the truth than bold ones are
    let truth = read_glyphs(&regular(), 0, &[], SECOND.chars()).unwrap();
    let raw = read_glyphs(&bold(), 0, &[], SECOND.chars()).unwrap();
    let mean = |set: &BTreeMap<char, letters_fonts::matching::GlyphData>| {
        let total: f64 = set
            .iter()
            .map(|(c, g)| {
                shape_distance(&Samples::of(&g.shape), &Samples::of(&truth.glyphs[c].shape)).mean
            })
            .sum();
        total / set.len() as f64
    };
    let raw_new: BTreeMap<_, _> = raw
        .glyphs
        .into_iter()
        .filter(|(c, _)| completed.new_glyphs.contains_key(c))
        .collect();
    assert!(mean(&completed.new_glyphs) < mean(&raw_new) * 0.6);
}

#[test]
fn a_font_whose_licence_forbids_embedding_lends_nothing() {
    let mut restricted = regular();
    // fsType is the u16 at offset 8 of OS/2; 2 means "restricted"
    let directory = letters_fonts::sfnt::table_directory(&restricted, 0).unwrap();
    let os2 = directory[b"OS/2"].start;
    restricted[os2 + 8..os2 + 10].copy_from_slice(&2u16.to_be_bytes());
    let subset = subset_of(&regular(), FIRST);
    let completed = complete_font(
        &subset,
        SECOND,
        &pool(&[("restricted", restricted)]),
        &CompleteOptions::default(),
    )
    .unwrap();
    assert!(completed
        .glyphs
        .iter()
        .all(|g| g.strategy == Strategy::Missing));
    assert!(completed
        .report
        .permission_notes
        .iter()
        .any(|note| note.contains("doesn’t allow embedding")));
    assert!(completed
        .report
        .sentences
        .iter()
        .any(|s| s.contains("license doesn’t allow copying")));
}

#[test]
fn accents_are_built_from_the_subset_when_nothing_else_is_there() {
    let subset = subset_of(&regular(), "Le café crème, à côté. Ou");
    let completed = complete_font(
        &subset,
        "Où êtes",
        &DonorPool::default(),
        &CompleteOptions::default(),
    )
    .unwrap();
    assert_eq!(completed.strategy_of('ù'), Some(Strategy::Composed));
    assert_eq!(completed.strategy_of('ê'), Some(Strategy::Composed));
    let u_grave = completed
        .glyphs
        .iter()
        .find(|g| g.character == 'ù')
        .unwrap();
    assert_eq!(u_grave.built_from[0], 'u');
    assert!(u_grave.sentence.contains("grave accent"));
    // the built letter is close to the font's own "ù"
    let truth = read_glyphs(&regular(), 0, &[], ['ù']).unwrap();
    let distance = shape_distance(
        &Samples::of(&completed.new_glyphs[&'ù'].shape),
        &Samples::of(&truth.glyphs[&'ù'].shape),
    );
    assert!(distance.mean < 0.005, "{distance:?}");
    // a letter with no donor and no parts is missing, and said so
    assert_eq!(completed.strategy_of('s'), Some(Strategy::Missing));
    assert!(completed
        .report
        .sentences
        .iter()
        .any(|s| s == "Nothing on this computer can draw “s”."));
}

#[test]
fn a_second_completion_comes_from_the_cache() {
    let folder =
        std::env::temp_dir().join(format!("letters-completion-cache-{}", std::process::id()));
    let options = CompleteOptions {
        cache_folder: Some(folder.clone()),
        ..CompleteOptions::default()
    };
    let subset = subset_of(&regular(), FIRST);
    let donors = pool(&[("bold", bold())]);
    let first = complete_font(&subset, SECOND, &donors, &options).unwrap();
    let second = complete_font(&subset, SECOND, &donors, &options).unwrap();
    assert!(!first.from_cache && second.from_cache);
    assert_eq!(first.font, second.font);
    assert_eq!(first.glyphs, second.glyphs);
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn generated_glyphs_need_a_yes_and_must_beat_the_fit() {
    let subset = subset_of(&regular(), FIRST);
    let donors = pool(&[("bold", bold())]);
    let style = measure_style(&subset.glyphs);
    let wanted: Vec<char> = "Jkx".chars().collect();
    // a reply drawing the true letters (the best a provider could do)
    let truth = read_glyphs(&regular(), 0, &[], wanted.iter().copied()).unwrap();
    let fake = FakeGenerator::new(true, vec![FakeGenerator::reply_for(&truth.glyphs)]);
    let plan = plan_generation(&subset.glyphs, &style, &wanted, &fake.describe());
    let sentence = plan.disclosure().sentence.clone();
    assert!(sentence.contains("No text of the document is sent."));

    let without_yes = Generation {
        generator: &fake,
        plan: plan.clone(),
        acknowledgement: None,
    };
    let refused = complete_font_with(
        &subset,
        "Jkx",
        &donors,
        &CompleteOptions::default(),
        Some(&without_yes),
    )
    .unwrap();
    assert!(refused
        .glyphs
        .iter()
        .all(|g| g.strategy == Strategy::FittedDonor));
    assert!(refused.notices.iter().any(|n| n.contains("needs your OK")));
    assert!(fake.seen.lock().unwrap().is_empty(), "nothing was sent");

    let with_yes = Generation {
        generator: &fake,
        acknowledgement: Some(plan.disclosure().acknowledge()),
        plan,
    };
    let accepted = complete_font_with(
        &subset,
        "Jkx",
        &donors,
        &CompleteOptions::default(),
        Some(&with_yes),
    )
    .unwrap();
    assert_eq!(fake.seen.lock().unwrap().len(), 1);
    assert!(
        accepted
            .glyphs
            .iter()
            .any(|g| g.strategy == Strategy::Generated),
        "{:?}",
        accepted.report.strategies
    );
    // the body sent holds outlines and the wanted letters, not the text
    let body = &fake.seen.lock().unwrap()[0];
    assert!(!body.contains("annual report"));
}

#[test]
fn nothing_needed_returns_the_document_font_as_it_was() {
    let subset = subset_of(&regular(), FIRST);
    let completed = complete_font(
        &subset,
        "the rose",
        &DonorPool::default(),
        &CompleteOptions::default(),
    )
    .unwrap();
    assert!(completed.glyphs.is_empty());
    assert_eq!(completed.font, subset.truetype.unwrap());
}
