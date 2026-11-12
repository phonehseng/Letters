//! tests for generated glyphs: disclosure, consent, reply checks and scoring,
//! all against the fake generator.

use super::*;
use crate::matching::shape::tests::square;
use crate::matching::style::measure_style;

fn sample_set() -> GlyphSet {
    let mut set = GlyphSet::default();
    for (c, top) in [('n', 0.5), ('o', 0.5), ('H', 0.7), ('l', 0.72), ('x', 0.5)] {
        set.glyphs.insert(
            c,
            GlyphData {
                shape: Shape {
                    contours: vec![square(0.05, 0.0, 0.13, top)],
                },
                advance: 0.5,
            },
        );
    }
    set
}

#[test]
fn the_disclosure_says_what_goes_and_that_no_text_does() {
    let set = sample_set();
    let style = measure_style(&set);
    let fake = FakeGenerator::new(true, vec![]);
    let plan = plan_generation(&set, &style, &['q', 'j', 'k', 'j', ' '], &fake.describe());
    let disclosure = plan.disclosure();
    assert_eq!(disclosure.wanted, vec!['j', 'k', 'q']);
    assert_eq!(
        disclosure.sentence,
        "This will send the outlines of 5 letters of this document’s font and the characters wanted (“j”, “k” and “q”) to glyphs.example.org. No text of the document is sent."
    );
    let body = plan.request().body();
    assert!(body.contains("\"examples\"") && body.contains("M50 0L50 500"));
}

#[test]
fn sending_needs_the_matching_yes() {
    let set = sample_set();
    let style = measure_style(&set);
    let fake = FakeGenerator::new(
        true,
        vec!["{\"glyphs\":[]}".into(), "{\"glyphs\":[]}".into()],
    );
    let plan = plan_generation(&set, &style, &['k'], &fake.describe());
    let other = plan_generation(&set, &style, &['k'], &fake.describe());
    let without = Generation {
        generator: &fake,
        plan: plan.clone(),
        acknowledgement: None,
    };
    assert_eq!(
        run_generation(&without, &['k']),
        Err(GenerationError::ConsentRequired)
    );
    let wrong = Generation {
        generator: &fake,
        plan: plan.clone(),
        acknowledgement: Some(other.disclosure().acknowledge()),
    };
    assert_eq!(
        run_generation(&wrong, &['k']),
        Err(GenerationError::ConsentMismatch)
    );
    let right = Generation {
        generator: &fake,
        plan: plan.clone(),
        acknowledgement: Some(plan.disclosure().acknowledge()),
    };
    assert!(run_generation(&right, &['k', 'z']).is_ok());
    // only what was disclosed is asked for
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].contains("\"wanted\":[\"k\"]"));
    // a provider that changed since the plan is refused
    let moved = FakeGenerator {
        info: GeneratorInfo {
            host: "other.example.org".into(),
            ..fake.describe()
        },
        ..FakeGenerator::new(true, vec![])
    };
    let changed = Generation {
        generator: &moved,
        plan: plan.clone(),
        acknowledgement: Some(plan.disclosure().acknowledge()),
    };
    assert_eq!(
        run_generation(&changed, &['k']),
        Err(GenerationError::ProviderChanged)
    );
}

#[test]
fn local_generators_need_no_yes() {
    let set = sample_set();
    let style = measure_style(&set);
    let fake = FakeGenerator::new(false, vec!["{\"glyphs\":[]}".into()]);
    let plan = plan_generation(&set, &style, &['k'], &fake.describe());
    assert_eq!(plan.disclosure().sentence, "This stays on this computer.");
    let generation = Generation {
        generator: &fake,
        plan,
        acknowledgement: None,
    };
    assert!(run_generation(&generation, &['k']).is_ok());
}

#[test]
fn replies_are_checked_strictly() {
    let expected = |_c: char| 0.5;
    let good = r#"{"glyphs":[{"char":"k","advance":500,"path":"M50 0L50 700L130 700L130 0Z"}]}"#;
    let (accepted, refused) = read_reply(good, &['k'], &expected);
    assert_eq!(accepted.len(), 1, "{refused:?}");
    let cases = [
        (
            r#"{"glyphs":[{"char":"k","advance":500,"path":"M50 0L50 700L130 700L130 0"}]}"#,
            Rejection::OpenContour,
        ),
        (
            r#"{"glyphs":[{"char":"k","advance":500,"path":"M50 0 A 5 5 0 0 1 50 700Z"}]}"#,
            Rejection::BadPath("only M, L, Q, C and Z are allowed".into()),
        ),
        (
            r#"{"glyphs":[{"char":"k","advance":500,"path":"M50 0L50 9000L130 700L130 0Z"}]}"#,
            Rejection::OutOfBounds,
        ),
        // a bow tie crosses itself
        (
            r#"{"glyphs":[{"char":"k","advance":500,"path":"M0 0L400 600L400 0L0 600Z"}]}"#,
            Rejection::Crossing,
        ),
        (
            r#"{"glyphs":[{"char":"k","advance":5000,"path":"M50 0L50 700L130 700L130 0Z"}]}"#,
            Rejection::BadAdvance,
        ),
        (
            r#"{"glyphs":[{"char":"z","advance":500,"path":"M50 0L50 700L130 700L130 0Z"}]}"#,
            Rejection::NotAsked,
        ),
        (
            r#"{"glyphs":[{"char":"k","advance":500,"path":"M50 0L50 0.001L50.001 0Z"}]}"#,
            Rejection::TinyContour,
        ),
    ];
    for (reply, why) in cases {
        let (accepted, refused) = read_reply(reply, &['k'], &expected);
        assert!(accepted.is_empty(), "{reply}");
        assert_eq!(refused[0].1, why, "{reply}");
    }
    let (accepted, refused) = read_reply("sorry, I can't", &['k'], &expected);
    assert!(accepted.is_empty() && refused[0].1 == Rejection::Unreadable);
    // json inside a code block is still read
    let wrapped = format!("Here you go:\n```json\n{good}\n```");
    assert_eq!(read_reply(&wrapped, &['k'], &expected).0.len(), 1);
}

#[test]
fn off_style_glyphs_score_worse() {
    let set = sample_set();
    let style = measure_style(&set);
    let on_style = GlyphData {
        shape: Shape {
            contours: vec![square(0.05, 0.0, 0.13, 0.5)],
        },
        advance: 0.5,
    };
    let too_tall = GlyphData {
        shape: Shape {
            contours: vec![square(0.05, 0.0, 0.13, 0.65)],
        },
        advance: 0.5,
    };
    let too_heavy = GlyphData {
        shape: Shape {
            contours: vec![square(0.05, 0.0, 0.2, 0.5)],
        },
        advance: 0.5,
    };
    let good = style_score('n', &on_style, &style, Some(0.5));
    assert!(good < 1e-9, "{good}");
    assert!(style_score('n', &too_tall, &style, Some(0.5)) > good + 0.1);
    assert!(style_score('n', &too_heavy, &style, Some(0.5)) > good + 0.05);
}
