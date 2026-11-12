//! a 500 page document, to show the operations stay quick on long files.

mod support;

use std::time::{Duration, Instant};

use letters_pages::*;
use support::*;

/// generous, so a slow test machine doesn't fail it, but far below what
/// an approach that rescans the whole file per page would take.
const BUDGET: Duration = Duration::from_secs(10);

#[test]
fn five_hundred_pages_stay_fast() {
    let mut doc = tagged(500);
    add_outline(
        &mut doc,
        &(0..50)
            .map(|chapter| bookmark(&format!("Chapter {chapter}"), chapter * 10, vec![]))
            .collect::<Vec<_>>(),
    );
    for page in 0..499 {
        add_link(&mut doc, page, page + 1);
    }
    let started = Instant::now();
    let mut timings = Vec::new();
    let mut time = |label: &str, started: &mut Instant| {
        timings.push(format!("{label}: {:?}", started.elapsed()));
        *started = Instant::now();
    };
    let mut step = Instant::now();

    let reversed: Vec<u32> = (0..500).rev().collect();
    reorder(
        &mut doc,
        &ReorderParams {
            new_order: reversed,
        },
    )
    .unwrap();
    time("reorder", &mut step);
    let all: Vec<u32> = (0..500).collect();
    rotate(
        &mut doc,
        &RotateParams {
            pages: all.clone(),
            quarter_turns: 1,
        },
    )
    .unwrap();
    time("rotate", &mut step);
    add_page_numbers(&mut doc, &PageNumberParams::default()).unwrap();
    time("page numbers", &mut step);
    let every_other: Vec<u32> = (0..500).step_by(2).collect();
    delete(&mut doc, &DeleteParams { pages: every_other }).unwrap();
    time("delete 250", &mut step);
    remove_stamps(
        &mut doc,
        &RemoveStampsParams {
            kind: StampKind::PageNumbers,
            pages: None,
        },
    )
    .unwrap();
    time("remove numbers", &mut step);
    let first_hundred: Vec<u32> = (0..100).collect();
    let part = extract(
        &doc,
        &ExtractParams {
            pages: first_hundred,
        },
    )
    .unwrap();
    time("extract 100", &mut step);
    let parts = split(&doc, &SplitParams::EveryNPages { pages_per_part: 10 }).unwrap();
    time("split into 25", &mut step);
    let documents: Vec<_> = parts.into_iter().map(|part| part.document).collect();
    let names = (0..documents.len()).map(|n| format!("part {n}")).collect();
    let mut merged = merge(&documents, &MergeParams { names }).unwrap();
    time("merge 25", &mut step);
    let merged = reload(&mut merged);
    time("save and load", &mut step);

    let elapsed = started.elapsed();
    println!("{}", timings.join("\n"));
    assert!(
        elapsed < BUDGET,
        "took {elapsed:?}:\n{}",
        timings.join("\n")
    );
    assert_eq!(part.get_pages().len(), 100);
    assert_eq!(merged.get_pages().len(), 250);
    // after reversing, position 0 holds page 499; deleting the even
    // positions leaves pages 498, 496, ... 0
    let expected: Vec<usize> = (0..250).map(|n| 498 - 2 * n).collect();
    assert_eq!(order(&merged), expected);
    assert_no_dangling_references(&merged);
}
