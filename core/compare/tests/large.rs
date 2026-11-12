//! a 500 page document compared with an edited copy: how long it takes
//! and how much memory it needs.
//!
//! the memory figure counts every allocation the rust code makes (through a
//! counting allocator below); pdfium's own memory, used only for drawing
//! the few pages that differ, is not included.

mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use letters_compare::*;
use support::*;

/// counts how many bytes are allocated now, and the most there have been.
struct CountingAllocator;

static IN_USE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call is passed straight to the system allocator; the
// counting around it touches only the two atomic counters.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let now = IN_USE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        IN_USE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const PAGES: usize = 500;
const LINES_PER_PAGE: usize = 30;

/// the engine's tile cache is kept small: its memory is the engine's, not
/// the comparison's, and would otherwise hide what the comparison uses.
const TILE_CACHE: usize = 1024 * 1024;

/// the edited copy: three words changed, a page added and a page removed.
fn edited_pages() -> Vec<PageSpec> {
    let mut pages: Vec<PageSpec> = (0..PAGES)
        .map(|seed| numbered_page(seed, LINES_PER_PAGE))
        .collect();
    for page_number in [50, 150, 250] {
        if let Item::Text(line) = &mut pages[page_number].items[10] {
            line.text = line
                .text
                .replacen(&format!("w{page_number}x10y3"), "edited", 1);
        }
    }
    pages.remove(400);
    pages.insert(300, numbered_page(9_999, LINES_PER_PAGE));
    pages
}

#[test]
fn five_hundred_pages_with_scattered_edits() {
    let old_spec = pdf((0..PAGES)
        .map(|seed| numbered_page(seed, LINES_PER_PAGE))
        .collect());
    let new_spec = pdf(edited_pages());
    let (old, new) = write_pair_with_cache("large", &old_spec, &new_spec, TILE_CACHE);
    let (old_source, new_source) = (old.source(), new.source());

    let before = IN_USE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let started = Instant::now();
    let result = compare(
        &old_source,
        &new_source,
        &CompareOptions::default(),
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    let seconds = started.elapsed().as_secs_f32();
    let peak_extra = PEAK.load(Ordering::Relaxed) - before;
    let source_bytes = before;
    println!(
        "500 vs 500 pages: {seconds:.2} s, peak working memory {:.1} MB above the {:.1} MB the two open documents and the test already held",
        peak_extra as f32 / 1_048_576.0,
        source_bytes as f32 / 1_048_576.0
    );

    assert_eq!(result.summary.text, 3, "{:#?}", result.summary);
    assert_eq!(result.summary.page, 2, "one page added and one removed");
    assert_eq!(result.summary.total, 5);
    assert_eq!(result.page_pairs.len(), PAGES + 1);
    let edited: Vec<&Change> = result
        .page_pairs
        .iter()
        .flat_map(|pair| pair.changes.iter())
        .filter(|change| change.category == Category::Text)
        .collect();
    for change in edited {
        assert_eq!(change.new_text.as_deref(), Some("edited"));
    }
    // a generous bound for an unoptimised test build on a laptop; the
    // release build is several times faster
    assert!(seconds < 120.0, "{seconds} s");
    // pages are compared one pair at a time, so memory doesn't grow with them
    assert!(peak_extra < 100 * 1_048_576, "{peak_extra} bytes");
}

/// the same pages written differently (as when a file is saved again by
/// another program): no fingerprint matches, so every page pair is drawn
/// and compared as pictures too. this is the slow case.
#[test]
fn every_page_drawn_when_the_files_were_written_differently() {
    const DRAWN_PAGES: usize = 100;
    let old_spec = pdf((0..DRAWN_PAGES)
        .map(|seed| numbered_page(seed, LINES_PER_PAGE))
        .collect());
    let mut new_spec = old_spec.clone();
    new_spec.variant = true;
    let (old, new) = write_pair_with_cache("large-drawn", &old_spec, &new_spec, TILE_CACHE);
    let (old_source, new_source) = (old.source(), new.source());

    let before = IN_USE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let started = Instant::now();
    let result = compare_documents(&old_source, &new_source).unwrap();
    let seconds = started.elapsed().as_secs_f32();
    let peak_extra = PEAK.load(Ordering::Relaxed) - before;
    println!(
        "{DRAWN_PAGES} vs {DRAWN_PAGES} pages, every page drawn: {seconds:.2} s, peak working memory {:.1} MB",
        peak_extra as f32 / 1_048_576.0
    );
    assert_eq!(result.summary.total, 0, "{:#?}", result.summary);
    assert!(peak_extra < 100 * 1_048_576, "{peak_extra} bytes");
}
