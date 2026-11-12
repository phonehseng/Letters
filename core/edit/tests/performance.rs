//! editing one page of a 300-page document reads that page (and, for
//! footer detection, its two neighbours) and rewrites only that page.

mod support;

use std::time::{Duration, Instant};

use letters_edit::{move_paths, page_objects, reflow_paragraph, EditConsent, ReflowOptions};
use lopdf::Document;
use support::{load, PageSetup, PdfBuilder};

fn page_content(number: usize) -> String {
    format!(
        "BT /F1 10 Tf 14 TL 72 700 Td (Page {number} begins with a paragraph of) Tj T* (ordinary text set in two lines.) Tj ET
BT /F1 10 Tf 14 TL 72 640 Td (A second paragraph follows it.) Tj ET
0 0.5 0 rg 72 560 128 40 re f
BT /F1 9 Tf 290 40 Td (Page {number}) Tj ET"
    )
}

fn book(pages: usize) -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    for number in 1..=pages {
        pdf.page(
            &PageSetup::letter(),
            &[&page_content(number)],
            "/Font << /F1 3 0 R >>",
        );
    }
    pdf.finish()
}

/// every object page `page` uses directly, written out.
fn page_bytes(doc: &Document, page: usize) -> String {
    let id = doc.get_pages()[&(page as u32 + 1)];
    let mut ids = vec![id];
    ids.extend(doc.get_page_contents(id));
    ids.iter()
        .map(|id| format!("{:?}", doc.get_object(*id).unwrap()))
        .collect()
}

const LONGER: &str =
    "This page begins with a paragraph of ordinary text set in two lines, and now a third line.";

fn time_reflow(doc: &mut Document, page: usize) -> Duration {
    let id = page_objects(doc, page).unwrap().text_blocks[0].id.clone();
    let started = Instant::now();
    let result = reflow_paragraph(doc, page, &id, LONGER, &ReflowOptions::default()).unwrap();
    let elapsed = started.elapsed();
    assert!(result.applied);
    assert_eq!(result.summary.pages_changed, [page]);
    elapsed
}

#[test]
fn editing_one_page_of_three_hundred_costs_about_as_much_as_of_three() {
    let mut small = load(&book(3));
    let mut large = load(&book(300));
    let untouched: Vec<String> = [0, 148, 150, 299]
        .iter()
        .map(|page| page_bytes(&large, *page))
        .collect();
    // warm up both so the first measurement isn't paying for setup
    time_reflow(&mut load(&book(3)), 1);
    let small_time = time_reflow(&mut small, 1);
    let large_time = time_reflow(&mut large, 149);
    println!("reflow on page 2 of 3: {small_time:?}; on page 150 of 300: {large_time:?}");
    let after: Vec<String> = [0, 148, 150, 299]
        .iter()
        .map(|page| page_bytes(&large, *page))
        .collect();
    assert_eq!(after, untouched, "no other page's objects changed");
    // finding the page in the tree is the only part that grows with the
    // document; a generous bound keeps this steady on a busy machine
    assert!(
        large_time < small_time * 10 + Duration::from_millis(150),
        "{large_time:?} against {small_time:?}"
    );

    let path = page_objects(&large, 200).unwrap().paths[0].id.clone();
    let started = Instant::now();
    move_paths(&mut large, 200, &[path], 5.0, 5.0, &EditConsent::default()).unwrap();
    println!("moving a path on page 201 of 300: {:?}", started.elapsed());
}
