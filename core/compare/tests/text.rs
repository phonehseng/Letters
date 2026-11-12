//! text changes: changed words, added sentences, moved paragraphs, reading
//! order, and the differences that should not count.

mod support;

use letters_compare::*;
use support::*;

const BODY: [&str; 4] = [
    "The quick brown fox jumps over the lazy dog near",
    "the river bank while the farmer watches from the",
    "old wooden fence and the sun sets slowly behind",
    "the hills at the end of a long summer day here.",
];

fn body_page(texts: &[&str]) -> PageSpec {
    page(lines(72.0, 100.0, 12.0, 16.0, texts))
}

fn all_changes(result: &ComparisonResult) -> Vec<&Change> {
    result
        .page_pairs
        .iter()
        .flat_map(|pair| pair.changes.iter())
        .collect()
}

#[test]
fn one_changed_word_is_one_replacement_on_the_right_words() {
    let mut changed = BODY;
    changed[0] = "The quick brown cat jumps over the lazy dog near";
    let (old, new) = write_pair(
        "one-word",
        &pdf(vec![body_page(&BODY)]),
        &pdf(vec![body_page(&changed)]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();

    let changes = all_changes(&result);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    let change = changes[0];
    assert_eq!(change.kind, ChangeKind::Replaced);
    assert_eq!(change.category, Category::Text);
    assert_eq!(change.old_text.as_deref(), Some("fox"));
    assert_eq!(change.new_text.as_deref(), Some("cat"));
    assert_eq!(change.detail, "Changed “fox” to “cat”");
    // the rectangles sit where pdfium itself finds the words
    assert!(
        lands_on(&change.left_rects, old.word_rect(0, "fox"), 3.0),
        "{:?} vs {:?}",
        change.left_rects,
        old.word_rect(0, "fox")
    );
    assert!(
        lands_on(&change.right_rects, new.word_rect(0, "cat"), 3.0),
        "{:?} vs {:?}",
        change.right_rects,
        new.word_rect(0, "cat")
    );
    assert_eq!(result.summary.text, 1);
    assert_eq!(result.summary.total, 1);
}

#[test]
fn an_inserted_sentence_is_one_text_change_even_though_later_text_reflows() {
    let old = [
        lines(72.0, 100.0, 12.0, 16.0, &BODY),
        lines(
            72.0,
            180.0,
            12.0,
            16.0,
            &[
                "A second paragraph stays the same in both",
                "versions of the document.",
            ],
        ),
    ]
    .concat();
    // "It was warm." goes in after the first sentence's first line; the rest
    // of the paragraph moves along and needs one more line
    let new = [
        lines(
            72.0,
            100.0,
            12.0,
            16.0,
            &[
                "The quick brown fox jumps over the lazy dog near",
                "the river bank. It was warm. While the farmer",
                "watches from the old wooden fence and the sun",
                "sets slowly behind the hills at the end of a long",
                "summer day here.",
            ],
        ),
        lines(
            72.0,
            196.0,
            12.0,
            16.0,
            &[
                "A second paragraph stays the same in both",
                "versions of the document.",
            ],
        ),
    ]
    .concat();
    let (old, new) = write_pair(
        "sentence-inserted",
        &pdf(vec![page(old)]),
        &pdf(vec![page(new)]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let changes = all_changes(&result);
    // "bank while" became "bank. It was warm. While": one rewritten passage, nothing else
    assert!(
        changes
            .iter()
            .all(|change| change.category == Category::Text),
        "{changes:#?}"
    );
    assert_eq!(changes.len(), 1, "{changes:#?}");
    assert!(
        changes[0]
            .new_text
            .as_deref()
            .unwrap()
            .contains("It was warm."),
        "{changes:#?}"
    );
}

#[test]
fn a_pure_insertion_is_reported_as_added() {
    let old = body_page(&["One two three four five six.", "Seven eight nine ten."]);
    let new = body_page(&[
        "One two three four five six.",
        "Brand new words here.",
        "Seven eight nine ten.",
    ]);
    let (old, new) = write_pair("pure-insertion", &pdf(vec![old]), &pdf(vec![new]));
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let changes = all_changes(&result);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    assert_eq!(changes[0].kind, ChangeKind::Inserted);
    assert_eq!(
        changes[0].new_text.as_deref(),
        Some("Brand new words here.")
    );
    assert_eq!(changes[0].detail, "Added “Brand new words here.”");
    assert!(covers(&changes[0].right_rects, new.word_rect(0, "Brand")));
    // the old side gets a thin mark where the words would go
    assert_eq!(changes[0].left_rects.len(), 1);
    assert_eq!(changes[0].left_rects[0].width, 0.0);
}

#[test]
fn a_moved_paragraph_is_one_move_not_a_removal_and_an_addition() {
    let first = [
        "Alpha paragraph speaks of mountains and",
        "rivers in the far north.",
    ];
    let second = [
        "Beta paragraph tells of cities, trains and",
        "the people living there.",
    ];
    let third = [
        "Gamma paragraph closes the page with a",
        "short note on the weather.",
    ];
    let old = [
        lines(72.0, 100.0, 12.0, 16.0, &first),
        lines(72.0, 160.0, 12.0, 16.0, &second),
        lines(72.0, 220.0, 12.0, 16.0, &third),
    ]
    .concat();
    let new = [
        lines(72.0, 100.0, 12.0, 16.0, &second),
        lines(72.0, 160.0, 12.0, 16.0, &first),
        lines(72.0, 220.0, 12.0, 16.0, &third),
    ]
    .concat();
    let (old, new) = write_pair(
        "paragraph-moved",
        &pdf(vec![page(old)]),
        &pdf(vec![page(new)]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let changes = all_changes(&result);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    let moved = changes[0];
    assert_eq!(moved.kind, ChangeKind::Moved);
    assert_eq!(moved.category, Category::Text);
    assert!(moved.detail.starts_with("Moved “"), "{}", moved.detail);
    assert!(
        moved.detail.ends_with("to a different place on the page"),
        "{}",
        moved.detail
    );
    // whichever paragraph is called the moved one, both sides point at it
    let word = moved
        .old_text
        .as_deref()
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_owned();
    assert!(
        covers(&moved.left_rects, old.word_rect(0, &word)),
        "{moved:#?}"
    );
    assert!(
        covers(&moved.right_rects, new.word_rect(0, &word)),
        "{moved:#?}"
    );
    assert_eq!(result.summary.total, 1);
}

#[test]
fn a_paragraph_moved_away_from_where_new_text_arrived_is_still_a_move() {
    // the first paragraph goes to the end of the page, and a new sentence
    // takes its place: comparing words in order sees "first replaced by the
    // new sentence" at the top and "first added" at the bottom
    let first = [
        "Alpha paragraph speaks of mountains and",
        "rivers in the far north.",
    ];
    let second = [
        "Beta paragraph tells of cities, trains and",
        "the people living there.",
    ];
    let third = [
        "Gamma paragraph closes the page with a",
        "short note on the weather.",
    ];
    let arrived = ["Delta is a sentence written for this version only."];
    let old = [
        lines(72.0, 100.0, 12.0, 16.0, &first),
        lines(72.0, 160.0, 12.0, 16.0, &second),
        lines(72.0, 220.0, 12.0, 16.0, &third),
    ]
    .concat();
    let new = [
        lines(72.0, 100.0, 12.0, 16.0, &arrived),
        lines(72.0, 160.0, 12.0, 16.0, &second),
        lines(72.0, 220.0, 12.0, 16.0, &third),
        lines(72.0, 280.0, 12.0, 16.0, &first),
    ]
    .concat();
    let (old, new) = write_pair(
        "moved-from-replaced",
        &pdf(vec![page(old)]),
        &pdf(vec![page(new)]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let changes = all_changes(&result);
    assert_eq!(changes.len(), 2, "{changes:#?}");
    let moved = changes
        .iter()
        .find(|c| c.kind == ChangeKind::Moved)
        .unwrap_or_else(|| panic!("no move in {changes:#?}"));
    assert!(
        moved.old_text.as_deref().unwrap().starts_with("Alpha"),
        "{moved:#?}"
    );
    assert!(
        covers(&moved.left_rects, old.word_rect(0, "Alpha")),
        "{moved:#?}"
    );
    assert!(
        covers(&moved.right_rects, new.word_rect(0, "Alpha")),
        "{moved:#?}"
    );
    let added = changes
        .iter()
        .find(|c| c.kind == ChangeKind::Inserted)
        .unwrap_or_else(|| panic!("no insertion in {changes:#?}"));
    assert!(
        added.new_text.as_deref().unwrap().starts_with("Delta"),
        "{added:#?}"
    );
    assert!(added.old_text.is_none(), "{added:#?}");
}

#[test]
fn an_edited_paragraph_is_not_a_move_to_where_it_already_is() {
    let old = lines(
        72.0,
        100.0,
        12.0,
        16.0,
        &["The cell wall keeps the plant cell firm and shaped."],
    );
    let new = lines(
        72.0,
        100.0,
        12.0,
        16.0,
        &["The cell wall keeps the plant cell firm and round."],
    );
    let (old, new) = write_pair(
        "edited-not-moved",
        &pdf(vec![page(old)]),
        &pdf(vec![page(new)]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let changes = all_changes(&result);
    assert!(
        changes.iter().all(|c| c.kind != ChangeKind::Moved),
        "{changes:#?}"
    );
}

#[test]
fn a_paragraph_moved_to_another_page_names_both_pages() {
    let keep_one = lines(
        72.0,
        100.0,
        12.0,
        16.0,
        &[
            "Page one keeps this opening paragraph",
            "about the history of the town.",
        ],
    );
    let traveller = lines(
        72.0,
        160.0,
        12.0,
        16.0,
        &[
            "This travelling paragraph describes the old",
            "harbour and its fishing boats.",
        ],
    );
    let keep_two = lines(
        72.0,
        100.0,
        12.0,
        16.0,
        &[
            "Page two starts with its own paragraph",
            "about the market square.",
        ],
    );
    // the paragraph leaves page one and lands under page two's own paragraph,
    // at the same height it had before
    let old = pdf(vec![
        page([keep_one.clone(), traveller.clone()].concat()),
        page(keep_two.clone()),
    ]);
    let new = pdf(vec![page(keep_one), page([keep_two, traveller].concat())]);
    let (old, new) = write_pair("moved-across-pages", &old, &new);
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let changes = all_changes(&result);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    let moved = changes[0];
    assert_eq!(moved.kind, ChangeKind::Moved);
    assert_eq!((moved.left_page, moved.right_page), (Some(0), Some(1)));
    assert!(
        moved.detail.ends_with("from old page 1 to new page 2"),
        "{}",
        moved.detail
    );
    assert!(covers(&moved.left_rects, old.word_rect(0, "travelling")));
    assert!(covers(&moved.right_rects, new.word_rect(1, "travelling")));
}

#[test]
fn identical_documents_have_no_changes_even_when_their_bytes_differ() {
    let spec = pdf(vec![body_page(&BODY), numbered_page(2, 10)]);
    let mut variant = spec.clone();
    variant.variant = true;
    let (old, new) = write_pair("identical", &spec, &variant);
    let old_bytes = std::fs::read(&old.path).unwrap();
    let new_bytes = std::fs::read(&new.path).unwrap();
    assert_ne!(old_bytes, new_bytes, "the files should differ in bytes");

    let result = compare_documents(&old.source(), &new.source()).unwrap();
    assert_eq!(result.summary.total, 0, "{:#?}", all_changes(&result));
    assert_eq!(result.page_pairs.len(), 2);
    for pair in &result.page_pairs {
        assert_eq!(pair.similarity, 1.0);
        assert!(pair.scroll_map.len() > 2);
    }
}

#[test]
fn completely_different_documents_are_rewritten_page_by_page() {
    let old = pdf(vec![numbered_page(1, 5), numbered_page(2, 5)]);
    let new = pdf(vec![numbered_page(3, 5), numbered_page(4, 5)]);
    let (old, new) = write_pair("different", &old, &new);
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    assert_eq!(result.page_pairs.len(), 2);
    for (index, pair) in result.page_pairs.iter().enumerate() {
        assert_eq!(pair.left_page, Some(index as u32));
        assert_eq!(pair.right_page, Some(index as u32));
        assert_eq!(pair.changes.len(), 1, "{:#?}", pair.changes);
        assert_eq!(pair.changes[0].kind, ChangeKind::Replaced);
    }
    assert_eq!(result.summary.page, 0);
    assert_eq!(result.summary.text, 2);
}

#[test]
fn two_columns_are_read_column_by_column() {
    let mut items = vec![text(200.0, 70.0, 16.0, "Two Column Title")];
    items.extend(lines(
        72.0,
        110.0,
        11.0,
        14.0,
        &["left one", "left two", "left three"],
    ));
    items.extend(lines(
        330.0,
        110.0,
        11.0,
        14.0,
        &["right one", "right two", "right three"],
    ));
    let mut changed = items.clone();
    changed[6] = text(330.0, 138.0, 11.0, "right four");
    let (old, new) = write_pair(
        "columns",
        &pdf(vec![page(items)]),
        &pdf(vec![page(changed)]),
    );

    let words: Vec<String> = read_words(&old.source(), 0)
        .unwrap()
        .into_iter()
        .map(|word| word.text)
        .collect();
    assert_eq!(
        words,
        [
            "Two", "Column", "Title", "left", "one", "left", "two", "left", "three", "right",
            "one", "right", "two", "right", "three"
        ]
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let changes = all_changes(&result);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    assert_eq!(changes[0].old_text.as_deref(), Some("three"));
    assert_eq!(changes[0].new_text.as_deref(), Some("four"));
    assert!(lands_on(
        &changes[0].right_rects,
        new.word_rect(0, "four"),
        3.0
    ));
}

#[test]
fn spacing_and_hyphenation_differences_are_ignored_by_default() {
    let old = body_page(&[
        "We made a careful compar-",
        "ison of the data base records.",
    ]);
    let new = body_page(&["We made a careful comparison of", "the database records."]);
    let (old, new) = write_pair("spacing", &pdf(vec![old]), &pdf(vec![new]));
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    assert_eq!(result.summary.total, 0, "{:#?}", all_changes(&result));
    assert_eq!(result.summary.ignored, 1, "“data base” and “database”");

    // asked for, the same difference is reported in plain words
    let options = CompareOptions {
        ignore_spacing: false,
        ..CompareOptions::default()
    };
    let result = compare(
        &old.source(),
        &new.source(),
        &options,
        &mut |_| {},
        &Default::default(),
    )
    .unwrap();
    let changes = all_changes(&result);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    assert_eq!(
        changes[0].detail,
        "Spacing or hyphenation changed in “database”"
    );
}

#[test]
fn the_result_is_camel_case_json() {
    let mut changed = BODY;
    changed[1] = "the river bank while the farmer looks from the";
    let (old, new) = write_pair(
        "json",
        &pdf(vec![body_page(&BODY)]),
        &pdf(vec![body_page(&changed)]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let json = serde_json::to_value(&result).unwrap();
    let change = &json["pagePairs"][0]["changes"][0];
    assert_eq!(change["kind"], "replaced");
    assert_eq!(change["category"], "text");
    assert_eq!(change["oldText"], "watches");
    assert!(change["leftRects"][0]["width"].as_f64().unwrap() > 0.0);
    assert!(json["pagePairs"][0]["scrollMap"].as_array().unwrap().len() > 2);
    assert_eq!(json["summary"]["total"], 1);
}
