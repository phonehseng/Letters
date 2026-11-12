//! the index from the outside: adding documents, searching them the way
//! a person would, and surviving crashes, deletions and changes.

mod common;

use common::{add, document, print};
use letters_search::{DocKind, Filters, Index, SearchRequest, SearchResults};

fn search(index: &Index, query: &str) -> SearchResults {
    index
        .search(&SearchRequest {
            query: query.into(),
            ..SearchRequest::default()
        })
        .unwrap()
}

fn titles(results: &SearchResults) -> Vec<String> {
    results.documents.iter().map(|d| d.title.clone()).collect()
}

fn library() -> (tempfile::TempDir, Index) {
    let folder = tempfile::tempdir().unwrap();
    let mut index = Index::open(folder.path()).unwrap();
    add(
        &mut index,
        "/library/biology.pdf",
        "Biology Notes",
        &[
            (
                "Cells",
                "The cell membrane controls what enters and leaves the cell.",
            ),
            ("Cells", "Mitochondria are where respiration happens."),
            (
                "Plants",
                "Photosynthesis in plants turns light into chemical energy.",
            ),
        ],
    );
    add(
        &mut index,
        "/library/history.pdf",
        "Modern History",
        &[
            ("Revolutions", "The French Revolution began in 1789."),
            (
                "Revolutions",
                "A membrane of secrecy surrounded the cell of plotters.",
            ),
        ],
    );
    add(
        &mut index,
        "/library/essays/running.docx",
        "Running Essay",
        &[(
            "Training",
            "Runners who ran every day connected better with their running club.",
        )],
    );
    (folder, index)
}

#[test]
fn plain_words_must_all_be_there() {
    let (_folder, index) = library();
    let results = search(&index, "cell membrane ");
    assert_eq!(titles(&results), vec!["Biology Notes", "Modern History"]);
    // the biology page has the words side by side: it ranks first
    assert_eq!(results.documents[0].passages[0].page, 0);
    assert!(search(&index, "cell zebra ").documents.is_empty());
}

#[test]
fn english_word_forms_match_each_other() {
    let (_folder, index) = library();
    // "connect" finds "connected", "runs" finds "running" and "runners"
    assert_eq!(titles(&search(&index, "connect ")), vec!["Running Essay"]);
    assert_eq!(titles(&search(&index, "runs ")), vec!["Running Essay"]);
    // accents and case do not matter
    assert_eq!(
        titles(&search(&index, "PHOTOSYNTHÉSIS ")),
        vec!["Biology Notes"]
    );
}

#[test]
fn quoted_phrases_need_the_words_in_order() {
    let (_folder, index) = library();
    assert_eq!(
        titles(&search(&index, "\"cell membrane\"")),
        vec!["Biology Notes"]
    );
    assert!(search(&index, "\"membrane cell\"").documents.is_empty());
}

#[test]
fn the_last_word_is_a_prefix_while_typing() {
    let (_folder, index) = library();
    assert_eq!(titles(&search(&index, "photosyn")), vec!["Biology Notes"]);
    assert_eq!(titles(&search(&index, "mitoch")), vec!["Biology Notes"]);
    // with a space after it, it is a whole word again
    assert!(
        search(&index, "mitoch ").documents.is_empty() || {
            // a typo correction may still find it
            !search(&index, "mitoch ").corrections.is_empty()
        }
    );
}

#[test]
fn single_word_typos_are_forgiven_and_reported() {
    let (_folder, index) = library();
    let results = search(&index, "photosynthsis ");
    assert_eq!(titles(&results), vec!["Biology Notes"]);
    assert_eq!(results.corrections[0].used, "photosynthesis");
    // short words get no tolerance
    assert!(search(&index, "cel ").documents.is_empty());
}

#[test]
fn minus_excludes_and_or_widens() {
    let (_folder, index) = library();
    assert_eq!(
        titles(&search(&index, "membrane -revolution")),
        vec!["Biology Notes"]
    );
    assert_eq!(
        titles(&search(&index, "membrane -\"cell membrane\"")),
        vec!["Modern History"]
    );
    let either = search(&index, "mitochondria OR revolution");
    assert_eq!(either.documents.len(), 2);
}

#[test]
fn heading_and_title_matches_rank_higher() {
    let folder = tempfile::tempdir().unwrap();
    let mut index = Index::open(folder.path()).unwrap();
    add(
        &mut index,
        "a.pdf",
        "Lecture",
        &[("Introduction", "We mention enzymes once.")],
    );
    add(
        &mut index,
        "b.pdf",
        "Lecture Two",
        &[("Enzymes", "We mention enzymes once.")],
    );
    let results = search(&index, "enzymes ");
    assert_eq!(results.documents[0].path, "b.pdf");
    // title or heading only leaves out text matches
    let headings = index
        .search(&SearchRequest {
            query: "enzymes".into(),
            filters: Filters {
                titles_only: true,
                ..Filters::default()
            },
            ..SearchRequest::default()
        })
        .unwrap();
    assert_eq!(titles(&headings), vec!["Lecture Two"]);
}

#[test]
fn recent_documents_win_a_tie() {
    let folder = tempfile::tempdir().unwrap();
    let mut index = Index::open(folder.path()).unwrap();
    let day = 86_400_000u64;
    let now = 1000 * day;
    for (path, age) in [("old.pdf", 900), ("new.pdf", 1)] {
        index
            .add_document(
                path,
                DocKind::Pdf,
                print(now - age * day),
                document(path, &[("", "same words here")]),
            )
            .unwrap();
    }
    let results = index
        .search(&SearchRequest {
            query: "same words".into(),
            now,
            ..SearchRequest::default()
        })
        .unwrap();
    assert_eq!(results.documents[0].path, "new.pdf");
}

#[test]
fn filters_narrow_by_kind_folder_date_and_document() {
    let (_folder, mut index) = library();
    index
        .add_document(
            "/library/notes.md",
            DocKind::Text,
            print(5),
            document("notes", &[("", "cell notes")]),
        )
        .unwrap();
    let run = |filters: Filters| {
        let request = SearchRequest {
            query: "cell".into(),
            filters,
            ..SearchRequest::default()
        };
        titles(&index.search(&request).unwrap())
    };
    assert_eq!(
        run(Filters {
            kinds: vec![DocKind::Text],
            ..Filters::default()
        }),
        vec!["notes"]
    );
    assert_eq!(
        run(Filters {
            modified_after: Some(3),
            ..Filters::default()
        }),
        vec!["notes"]
    );
    assert_eq!(
        run(Filters {
            document: Some("/library/history.pdf".into()),
            ..Filters::default()
        }),
        vec!["Modern History"]
    );
    let in_essays = index
        .search(&SearchRequest {
            query: "running".into(),
            filters: Filters {
                folder: Some("/library/essays".into()),
                ..Filters::default()
            },
            ..SearchRequest::default()
        })
        .unwrap();
    assert_eq!(titles(&in_essays), vec!["Running Essay"]);
    // a folder whose name merely starts the same is not inside
    assert!(run(Filters {
        folder: Some("/lib".into()),
        ..Filters::default()
    })
    .is_empty());
}

#[test]
fn snippets_show_the_words_highlighted() {
    let (_folder, index) = library();
    let results = search(&index, "respiration");
    let passage = &results.documents[0].passages[0];
    assert_eq!(passage.heading, "Cells");
    let [start, end] = passage.highlights[0];
    let units: Vec<u16> = passage.snippet.encode_utf16().collect();
    assert_eq!(
        String::from_utf16(&units[start as usize..end as usize]).unwrap(),
        "respiration"
    );
}

#[test]
fn search_this_document_lists_every_page_in_order() {
    let folder = tempfile::tempdir().unwrap();
    let mut index = Index::open(folder.path()).unwrap();
    let pages: Vec<(&str, String)> = (0..30)
        .map(|i| {
            (
                "",
                format!("page {i} mentions osmosis {}", "again ".repeat(i % 5)),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = pages.iter().map(|(h, t)| (*h, t.as_str())).collect();
    add(&mut index, "long.pdf", "Long", &borrowed);
    let results = index
        .search(&SearchRequest {
            query: "osmosis".into(),
            filters: Filters {
                document: Some("long.pdf".into()),
                ..Filters::default()
            },
            passages_per_document: 1000,
            ..SearchRequest::default()
        })
        .unwrap();
    let found: Vec<u32> = results.documents[0]
        .passages
        .iter()
        .map(|p| p.page)
        .collect();
    assert_eq!(found, (0..30).collect::<Vec<u32>>());
    assert_eq!(results.total_passages, 30);
}

#[test]
fn changed_and_deleted_files_are_handled() {
    let (_folder, mut index) = library();
    assert!(!index.needs_indexing("/library/biology.pdf", &print(1)));
    assert!(index.needs_indexing("/library/biology.pdf", &print(2)));
    assert!(index.needs_indexing("/library/new.pdf", &print(1)));
    // on windows a path is the same document whatever its case and slashes
    if cfg!(windows) {
        assert!(!index.needs_indexing("\\LIBRARY\\Biology.pdf", &print(1)));
    }
    // re-indexing replaces the old text
    add(
        &mut index,
        "/library/biology.pdf",
        "Biology Notes",
        &[("", "Only ribosomes now.")],
    );
    assert!(search(&index, "mitochondria ").documents.is_empty());
    assert_eq!(titles(&search(&index, "ribosomes")), vec!["Biology Notes"]);
    // deleted files drop out
    let gone = index.retain(|doc| !doc.path.contains("history"));
    assert_eq!(gone, vec!["/library/history.pdf".to_string()]);
    assert!(search(&index, "revolution").documents.is_empty());
    assert_eq!(index.stats().documents, 2);
}

#[test]
fn a_checkpoint_survives_reopening_and_a_crash_loses_only_the_rest() {
    let folder = tempfile::tempdir().unwrap();
    {
        let mut index = Index::open(folder.path()).unwrap();
        add(&mut index, "kept.pdf", "Kept", &[("", "committed words")]);
        index.commit().unwrap();
        add(&mut index, "lost.pdf", "Lost", &[("", "uncommitted words")]);
        // dropped without a commit: as if the app had crashed here
    }
    let mut index = Index::open(folder.path()).unwrap();
    assert!(!index.was_recovered());
    assert_eq!(titles(&search(&index, "committed")), vec!["Kept"]);
    // (a typo match on "committed" may find Kept, but never Lost)
    assert!(!titles(&search(&index, "uncommitted")).contains(&"Lost".to_string()));
    // the lost file is simply indexed again, and the index works on
    assert!(index.needs_indexing("lost.pdf", &print(1)));
    add(&mut index, "lost.pdf", "Lost", &[("", "uncommitted words")]);
    index.commit().unwrap();
    let index = Index::open(folder.path()).unwrap();
    assert_eq!(index.stats().documents, 2);
    assert_eq!(titles(&search(&index, "uncommitted")), vec!["Lost"]);
}

#[test]
fn a_damaged_index_starts_again_instead_of_failing() {
    let folder = tempfile::tempdir().unwrap();
    {
        let mut index = Index::open(folder.path()).unwrap();
        add(&mut index, "a.pdf", "A", &[("", "words")]);
        index.commit().unwrap();
    }
    std::fs::write(folder.path().join("index.bin"), b"not an index").unwrap();
    let index = Index::open(folder.path()).unwrap();
    assert!(index.was_recovered());
    assert_eq!(index.stats().documents, 0);
}

#[test]
fn compaction_drops_dead_passages_and_keeps_results() {
    let folder = tempfile::tempdir().unwrap();
    let mut index = Index::open(folder.path()).unwrap();
    let text = "filler words for compaction ".repeat(10);
    let pages: Vec<(&str, &str)> = (0..600).map(|_| ("", text.as_str())).collect();
    add(&mut index, "big.pdf", "Big", &pages);
    add(
        &mut index,
        "small.pdf",
        "Small",
        &[("", "the one survivor")],
    );
    index.commit().unwrap();
    let before = index.stats().bytes_on_disk;
    index.remove_document("big.pdf");
    // more than a quarter dead: the commit compacts
    index.commit().unwrap();
    let after = index.stats();
    assert!(
        after.bytes_on_disk < before / 4,
        "{} -> {}",
        before,
        after.bytes_on_disk
    );
    assert_eq!(after.passages, 1);
    assert_eq!(titles(&search(&index, "survivor")), vec!["Small"]);
    drop(index);
    let index = Index::open(folder.path()).unwrap();
    assert_eq!(titles(&search(&index, "survivor")), vec!["Small"]);
    assert!(search(&index, "filler").documents.is_empty());
}

#[test]
fn locked_files_are_found_by_name_only() {
    let folder = tempfile::tempdir().unwrap();
    let mut index = Index::open(folder.path()).unwrap();
    let mut locked = document("Tax Return 2025", &[]);
    locked.notes = vec![letters_search::DocNote::Locked];
    index
        .add_document("tax.pdf", DocKind::Pdf, print(1), locked)
        .unwrap();
    let results = search(&index, "tax return");
    assert_eq!(titles(&results), vec!["Tax Return 2025"]);
    assert_eq!(
        results.documents[0].notes,
        vec![letters_search::DocNote::Locked]
    );
}

#[test]
fn a_moved_file_keeps_its_entry_under_the_new_path() {
    let (_folder, mut index) = library();
    assert!(index.rename_document("/library/biology.pdf", "/courses/Biology 101.pdf"));
    let results = search(&index, "respiration");
    assert_eq!(results.documents[0].path, "/courses/Biology 101.pdf");
    // a title of its own (not the file name) stays as it was
    assert_eq!(results.documents[0].title, "Biology Notes");
    assert!(!index.needs_indexing("/courses/Biology 101.pdf", &print(1)));
    assert!(!index.rename_document("/library/biology.pdf", "x.pdf"));
}

#[test]
fn clearing_deletes_the_index() {
    let (folder, mut index) = library();
    index.commit().unwrap();
    index.clear().unwrap();
    assert_eq!(index.stats().documents, 0);
    assert!(!folder.path().join("index.bin").exists());
}
