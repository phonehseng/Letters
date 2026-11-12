//! speed and size on a generated library.
//!
//! the full measurement (5,000 documents) runs only when asked for, in a
//! release build, because it takes about a minute:
//!
//! ```text
//! cargo test --release -p letters-search --test large_library -- --ignored --nocapture
//! ```
//!
//! a smaller library (300 documents) runs with the other tests and checks
//! that searching stays fast enough to run on every key press.

mod common;

use std::time::Instant;

use common::{print, Random, Vocabulary};
use letters_search::{DocKind, ExtractedDocument, Index, Passage, SearchRequest};

const PLANTED: &str = "the quick brown fox studies photosynthesis beside a lazy river";
const PLANTED_DOCUMENT: usize = 123;
const PLANTED_PAGE: u32 = 7;

struct Measured {
    documents: usize,
    passages: usize,
    words: usize,
    index_seconds: f64,
    bytes_on_disk: u64,
    bytes_in_memory: u64,
    dictionary: usize,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

fn generated_document(
    number: usize,
    vocabulary: &Vocabulary,
    random: &mut Random,
) -> ExtractedDocument {
    // most student files are short; a few are long books
    let pages = if number % 250 == 0 {
        300
    } else if number == PLANTED_DOCUMENT {
        12
    } else {
        1 + random.below(24)
    };
    let passages = (0..pages as u32)
        .map(|page| {
            let length = 220 + random.below(180);
            let mut text = vocabulary.sentence(random, length);
            if number == PLANTED_DOCUMENT && page == PLANTED_PAGE {
                text.push_str(". ");
                text.push_str(PLANTED);
            }
            Passage {
                number: page,
                page,
                heading: format!("Section {}", page / 5 + 1),
                text,
            }
        })
        .collect();
    ExtractedDocument {
        title: format!("Document {number} {}", vocabulary.word(random)),
        pages: pages as u32,
        passages,
        notes: Vec::new(),
    }
}

fn build_and_measure(documents: usize) -> Measured {
    let folder = tempfile::tempdir().unwrap();
    let mut random = Random::new(7);
    let vocabulary = Vocabulary::new(60_000, &mut random);
    let mut index = Index::open(folder.path()).unwrap();
    let mut words = 0usize;
    let started = Instant::now();
    for number in 0..documents {
        let extracted = generated_document(number, &vocabulary, &mut random);
        words += extracted
            .passages
            .iter()
            .map(|p| p.text.split_whitespace().count())
            .sum::<usize>();
        let path = format!("C:/library/folder{}/document{number}.pdf", number % 40);
        index
            .add_document(&path, DocKind::Pdf, print(number as u64 + 1), extracted)
            .unwrap();
        // the indexing job checkpoints every 200 documents
        if number % 200 == 199 {
            index.commit().unwrap();
        }
    }
    index.commit().unwrap();
    let index_seconds = started.elapsed().as_secs_f64();
    let stats = index.stats();

    // the planted phrase is found on its page
    let found = index
        .search(&SearchRequest {
            query: "\"brown fox studies photosynthesis\"".into(),
            ..SearchRequest::default()
        })
        .unwrap();
    assert_eq!(found.documents.len(), 1);
    assert!(found.documents[0]
        .path
        .ends_with(&format!("document{PLANTED_DOCUMENT}.pdf")));
    assert_eq!(found.documents[0].passages[0].page, PLANTED_PAGE);

    let queries = typed_queries(&vocabulary, &mut random);
    let mut times = Vec::new();
    for query in &queries {
        let started = Instant::now();
        index
            .search(&SearchRequest {
                query: query.clone(),
                ..SearchRequest::default()
            })
            .unwrap();
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    let at = |share: f64| times[((times.len() - 1) as f64 * share) as usize];
    Measured {
        documents,
        passages: stats.passages,
        words,
        index_seconds,
        bytes_on_disk: stats.bytes_on_disk,
        bytes_in_memory: stats.bytes_in_memory,
        dictionary: stats.words,
        p50_ms: at(0.5),
        p95_ms: at(0.95),
        max_ms: at(1.0),
    }
}

/// what a person types: every prefix of a few words as they type them,
/// whole common and rare words, phrases and typos.
fn typed_queries(vocabulary: &Vocabulary, random: &mut Random) -> Vec<String> {
    let mut queries = Vec::new();
    for _ in 0..40 {
        let first = vocabulary.word(random).to_string();
        let second = vocabulary.word(random).to_string();
        let full = format!("{first} {second}");
        for end in 2..=full.len() {
            if full.is_char_boundary(end) {
                queries.push(full[..end].to_string());
            }
        }
        queries.push(format!("\"{full}\""));
        queries.push(format!("{first} -{second}"));
    }
    for typo in ["photosynthsis ", "chapetr ", "studnets ", "histroy "] {
        queries.push(typo.to_string());
    }
    queries
}

fn report(m: &Measured) {
    println!(
        "library: {} documents, {} passages, {:.1} million words",
        m.documents,
        m.passages,
        m.words as f64 / 1e6
    );
    println!(
        "indexing: {:.1} s ({:.0} documents a second, {:.2} million words a second)",
        m.index_seconds,
        m.documents as f64 / m.index_seconds,
        m.words as f64 / 1e6 / m.index_seconds
    );
    println!(
        "index on disk: {:.1} MB, word lists in memory: about {:.1} MB, {} distinct words",
        m.bytes_on_disk as f64 / 1e6,
        m.bytes_in_memory as f64 / 1e6,
        m.dictionary
    );
    println!(
        "query latency: p50 {:.2} ms, p95 {:.2} ms, slowest {:.2} ms",
        m.p50_ms, m.p95_ms, m.max_ms
    );
}

#[test]
fn a_small_library_searches_fast_enough_to_type() {
    let measured = build_and_measure(300);
    report(&measured);
    // the median, because a debug build's slowest queries swing with the
    // machine's load (189 to 340 ms p95 seen while other builds ran).
    // the release measurement below is the real gate
    assert!(measured.p50_ms < 150.0, "p50 {} ms", measured.p50_ms);
}

#[test]
#[ignore = "takes about a minute; run with --release --ignored to measure"]
fn five_thousand_documents() {
    let measured = build_and_measure(5000);
    report(&measured);
    assert!(measured.p95_ms < 100.0, "p95 {} ms", measured.p95_ms);
}
