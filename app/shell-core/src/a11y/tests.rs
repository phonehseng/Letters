//! the accessibility fixes run through an edit session the way the app
//! runs them, the contrast renderer over the real pdf engine, and the
//! reading order blocks.

use letters_a11y::{auto_reading_order, check, Rect, RegionRenderer, Report};
use lopdf::Document;

use super::*;
use crate::compare::test_files::{pdf_bytes, start_worker, write, Page};
use crate::session::EditSession;
use crate::test_pdf::temp_dir;

/// an untagged essay: a big title, a section heading and body text.
fn essay() -> Vec<Page> {
    vec![Page::default()
        .text(72.0, 90.0, 24.0, "Rivers of Europe")
        .text(
            72.0,
            130.0,
            11.0,
            "The Rhine starts in the Alps and runs north to the sea.",
        )
        .text(
            72.0,
            146.0,
            11.0,
            "Many towns grew up along its banks over the centuries.",
        )
        .text(72.0, 190.0, 16.0, "The Danube")
        .text(
            72.0,
            220.0,
            11.0,
            "The Danube flows east through ten countries to the Black Sea.",
        )
        .text(
            72.0,
            236.0,
            11.0,
            "It is the second longest river in Europe after the Volga.",
        )]
}

fn session_for(name: &str, pages: &[Page]) -> (EditSession, std::path::PathBuf) {
    let dir = temp_dir(name);
    let file = write(&dir, "essay.pdf", &pdf_bytes(pages));
    let session = EditSession::start(&file, &dir.join("sessions")).unwrap();
    (session, dir)
}

/// runs one fix as the shell does: inside one edit of the session.
fn fix(session: &mut EditSession, request: FixRequest) -> FixOutcome {
    let mut outcome = None;
    session
        .apply(
            |doc| {
                outcome = Some(apply_fix(doc, &request, None)?);
                Ok::<(), letters_a11y::A11yError>(())
            },
            |_| Ok(()),
        )
        .unwrap_or_else(|error| panic!("the fix {request:?} failed: {error}"));
    outcome.unwrap()
}

fn report(session: &EditSession) -> Report {
    check(&session.read_document().unwrap(), None).unwrap()
}

#[test]
fn requests_read_the_shapes_the_window_sends() {
    let language: FixRequest =
        serde_json::from_str(r#"{"kind":"setLanguage","language":"en-GB"}"#).unwrap();
    let alt: FixRequest = serde_json::from_str(
        r#"{"kind":"setAltText","target":{"kind":"image","image":{"page":0,"opIndex":3}},"text":"A map"}"#,
    )
    .unwrap();
    let table: FixRequest = serde_json::from_str(
        r#"{"kind":"setTableHeaders","table":{"number":5,"generation":0},"headerRows":1,"headerColumns":0}"#,
    )
    .unwrap();
    let tags: FixRequest = serde_json::from_str(r#"{"kind":"addTags","language":"en"}"#).unwrap();

    assert_eq!(
        language,
        FixRequest::SetLanguage {
            language: "en-GB".into()
        }
    );
    assert!(matches!(alt, FixRequest::SetAltText { text, .. } if text == "A map"));
    assert!(matches!(
        table,
        FixRequest::SetTableHeaders {
            header_rows: 1,
            header_columns: 0,
            ..
        }
    ));
    assert!(matches!(
        tags,
        FixRequest::AddTags {
            replace_existing: false,
            ..
        }
    ));
    assert!(FixRequest::MarkPdfua.needs_renderer());
}

#[test]
fn each_fix_is_one_undo_step_and_its_finding_goes() {
    let (mut session, _dir) = session_for("a11y-fixes", &essay());
    let before = report(&session);
    assert!(
        before.has("not-tagged") && before.has("missing-language") && before.has("missing-title")
    );

    let tagged = fix(
        &mut session,
        FixRequest::AddTags {
            language: None,
            replace_existing: false,
        },
    );
    assert!(tagged.counts.unwrap().headings >= 2, "{tagged:?}");
    assert!(!tagged.changes.is_empty());
    fix(
        &mut session,
        FixRequest::SetLanguage {
            language: "en-GB".into(),
        },
    );
    fix(
        &mut session,
        FixRequest::SetTitle {
            title: "Rivers of Europe".into(),
        },
    );

    let after = report(&session);
    assert!(!after.has("not-tagged"));
    assert!(!after.has("missing-language"));
    assert!(!after.has("missing-title"));

    // one undo takes back exactly the title
    session.undo(|_| Ok::<(), String>(())).unwrap();
    let undone = report(&session);
    assert!(undone.has("missing-title"));
    assert!(!undone.has("missing-language"));
}

#[test]
fn an_edited_heading_ladder_is_one_step_and_skips_are_fixed() {
    let (mut session, _dir) = session_for("a11y-headings", &essay());
    fix(
        &mut session,
        FixRequest::AddTags {
            language: Some("en".into()),
            replace_existing: false,
        },
    );
    let doc = session.read_document().unwrap();
    let second = heading_elements(&doc)[1];

    // the person makes the second heading an H3, which skips a level
    fix(
        &mut session,
        FixRequest::SetHeadingLevels {
            levels: vec![HeadingLevel {
                element: second,
                level: 3,
            }],
        },
    );
    assert!(report(&session).has("heading-level-skipped"));

    fix(&mut session, FixRequest::FixHeadingLevels);
    assert!(!report(&session).has("heading-level-skipped"));
}

/// the headings of a tagged document, in the order they are proposed.
fn heading_elements(doc: &Document) -> Vec<letters_a11y::ObjectRef> {
    let is_heading = |name: &[u8]| name.len() == 2 && name[0] == b'H' && name[1].is_ascii_digit();
    let mut headings: Vec<lopdf::ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, object)| {
            let name = object
                .as_dict()
                .ok()
                .and_then(|dict| dict.get(b"S").ok())
                .and_then(|s| s.as_name().ok());
            name.is_some_and(is_heading)
        })
        .map(|(id, _)| *id)
        .collect();
    headings.sort();
    headings
        .into_iter()
        .map(letters_a11y::ObjectRef::from)
        .collect()
}

#[test]
fn contrast_is_measured_from_the_engines_pixels() {
    let dir = temp_dir("a11y-contrast");
    let worker = start_worker();
    let pale = [0.67, 0.67, 0.67];
    let page = Page::default()
        .coloured_text(
            72.0,
            100.0,
            12.0,
            "This grey sentence is too pale to read easily.",
            pale,
        )
        .text(72.0, 130.0, 12.0, "This black sentence is easy to read.");
    let path = write(&dir, "pale.pdf", &pdf_bytes(&[page]));
    let info = worker.open(&path, None).unwrap();
    let mut regions = PageRegions::new(&worker, info.id, info.page_sizes.clone());
    let doc = Document::load(&path).unwrap();

    let report = check(&doc, Some(&mut regions)).unwrap();

    let finding = report
        .with_id("low-contrast")
        .next()
        .expect("the pale text is found");
    let measured = finding.contrast.unwrap();
    assert!(measured.ratio < 3.0, "{measured:?}");
    assert!(measured.ratio > 2.0, "{measured:?}");
    assert_eq!(
        report.with_id("low-contrast").count(),
        1,
        "black text passes"
    );
    assert_eq!(
        regions.pages_drawn, 1,
        "the page is drawn once for every piece of text"
    );
    worker.close(info.id);
}

#[test]
fn regions_are_cut_from_the_page_and_kept_on_it() {
    let dir = temp_dir("a11y-regions");
    let worker = start_worker();
    let page = Page::default().filled([100.0, 100.0, 50.0, 50.0], [0.0, 0.0, 1.0]);
    let path = write(&dir, "box.pdf", &pdf_bytes(&[page]));
    let info = worker.open(&path, None).unwrap();
    let mut regions = PageRegions::new(&worker, info.id, info.page_sizes.clone());

    let inside = regions
        .render_region(0, Rect::new(110.0, 110.0, 10.0, 10.0), 2.0)
        .unwrap();
    let edge = regions
        .render_region(0, Rect::new(600.0, 780.0, 50.0, 50.0), 2.0)
        .unwrap();
    let png = region_png(
        &mut regions,
        0,
        Rect::new(100.0, 100.0, 50.0, 50.0),
        4.0,
        1.0,
    )
    .unwrap();

    assert_eq!((inside.width, inside.height), (20, 20));
    assert_eq!(inside.pixel(5, 5), Some([0, 0, 255]));
    assert_eq!(
        (edge.width, edge.height),
        (24, 24),
        "cut at the page's edge"
    );
    assert!(png.starts_with(b"\x89PNG"));
    assert_eq!(
        pixel_area(Rect::new(700.0, 10.0, 5.0, 5.0), 1.0, (612, 792)),
        None
    );
    worker.close(info.id);
}

#[test]
fn reading_blocks_follow_the_proposed_order_with_their_places_and_words() {
    let (mut session, _dir) = session_for("a11y-blocks", &essay());
    fix(
        &mut session,
        FixRequest::AddTags {
            language: None,
            replace_existing: false,
        },
    );
    let doc = session.read_document().unwrap();
    let order = auto_reading_order(&doc, 0).unwrap();

    let blocks = reading_blocks(&doc, 0, &order);

    assert_eq!(blocks.len(), order.len());
    assert!(blocks.len() >= 4, "{blocks:?}");
    assert!(blocks[0].text.starts_with("Rivers of Europe"), "{blocks:?}");
    assert!(blocks[0].role.starts_with('H'));
    let tops: Vec<f32> = blocks.iter().map(|block| block.rect.unwrap().y).collect();
    assert!(
        tops.windows(2).all(|pair| pair[0] < pair[1]),
        "top to bottom: {tops:?}"
    );
    // the title's box sits around its baseline at 90 points from the top
    let title = blocks[0].rect.unwrap();
    assert!(title.y < 90.0 && title.bottom() > 85.0, "{title:?}");
}
