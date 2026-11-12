//! the comparison report: a pdf that opens, reads, renders and is tagged,
//! and the same content as text and html.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use letters_compare::*;
use lopdf::{Document, Object};
use support::*;

fn heading_page(word: &str, heading_size: f32) -> PageSpec {
    let mut items = vec![text(72.0, 80.0, heading_size, "Introduction")];
    items.extend(lines(
        72.0,
        110.0,
        11.0,
        15.0,
        &[
            &format!("The quick brown {word} jumps over the lazy dog"),
            "and the rest of this page stays as it was.",
        ],
    ));
    page(items)
}

/// three page pairs: a changed word, nothing, and a bigger heading.
fn compared() -> (TestDoc, TestDoc, ComparisonResult) {
    // tests run at the same time, so each call writes to its own folder
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let folder = format!("report-{}", CALLS.fetch_add(1, Ordering::Relaxed));
    let old = pdf(vec![
        heading_page("fox", 14.0),
        numbered_page(5, 4),
        heading_page("owl", 14.0),
    ]);
    let new = pdf(vec![
        heading_page("cat", 14.0),
        numbered_page(5, 4),
        heading_page("owl", 16.0),
    ]);
    let (old, new) = write_pair(&folder, &old, &new);
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    (old, new, result)
}

fn options() -> ReportOptions {
    ReportOptions {
        old_name: "essay-draft.pdf".to_owned(),
        new_name: "essay-final.pdf".to_owned(),
        old_date: Some("3 November 2026".to_owned()),
        new_date: Some("10 November 2026".to_owned()),
        made_on: Some("11 November 2026".to_owned()),
        ..ReportOptions::default()
    }
}

fn all_text(document: &Document) -> String {
    let pages: Vec<u32> = document.get_pages().keys().copied().collect();
    document.extract_text(&pages).unwrap()
}

/// the words of the text, in order, so assertions don't depend on how the
/// text extractor breaks lines.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

fn contains_in_order(haystack: &[String], needle: &str) -> bool {
    let needle = words(needle);
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_slice())
}

#[test]
fn the_report_pdf_opens_and_says_what_changed() {
    let (old, new, result) = compared();
    assert_eq!(result.summary.text, 1);
    assert_eq!(result.summary.formatting, 1);
    let bytes = report_pdf(&result, &old.source(), &new.source(), &options()).unwrap();
    let document = Document::load_mem(&bytes).unwrap();
    // a title page, then one page per changed pair (each has thumbnails)
    assert_eq!(document.get_pages().len(), 3);

    let text = words(&all_text(&document));
    for expected in [
        "Comparison report",
        "Old version: essay-draft.pdf (3 November 2026)",
        "New version: essay-final.pdf (10 November 2026)",
        "What changed Changes",
        "Text 1",
        "Formatting 1",
        "Layout 0",
        "Total 2",
        "2 of 3 page pairs have changes.",
        "No changes on new page 2.",
        "Page 1",
        "Changed “fox” to “cat”.",
        "Font size changed from 14 to 16 in “Introduction”.",
        "Old page 1",
        "New page 3",
        "Page 1 of 3",
    ] {
        assert!(
            contains_in_order(&text, expected),
            "{expected:?} not in {text:?}"
        );
    }
}

#[test]
fn the_report_pdf_renders_with_thumbnails() {
    let (old, new, result) = compared();
    let bytes = report_pdf(&result, &old.source(), &new.source(), &options()).unwrap();
    let dir = temp_dir("report-render");
    let report = TestDoc::write_bytes(&start_worker(), &dir, "report.pdf", &bytes);
    let source = report.source();
    assert_eq!(source.page_count(), 3);
    // the second page holds the first pair's thumbnails: the old page's
    // heading is drawn in its top left corner, so there is dark ink there
    let picture = source.render(1, 1.0).unwrap();
    let thumbnail_top = 54.0 + 30.0;
    let mut dark = 0;
    for y in thumbnail_top as u32..thumbnail_top as u32 + 60 {
        for x in 54..294 {
            if picture.rgb_on_white(x, y)[0] < 128 {
                dark += 1;
            }
        }
    }
    assert!(
        dark > 20,
        "the thumbnail should show the page's text ({dark} dark pixels)"
    );
    // and a red outline marks the changed word on the old page's thumbnail
    let reddish = (0..picture.height)
        .flat_map(|y| (0..300).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let [r, g, b] = picture.rgb_on_white(x, y);
            r > 180 && g < 90 && b < 90
        })
        .count();
    assert!(reddish > 10, "{reddish}");
    // pdfium can find the report's own text
    assert!(!report
        .worker
        .search(report.id, "Changed “fox”")
        .unwrap()
        .matches
        .is_empty());
}

#[test]
fn the_report_pdf_is_tagged_titled_and_bookmarked() {
    let (old, new, result) = compared();
    let bytes = report_pdf(&result, &old.source(), &new.source(), &options()).unwrap();
    let document = Document::load_mem(&bytes).unwrap();
    let catalog = document.catalog().unwrap();
    assert_eq!(catalog.get(b"Lang").unwrap().as_str().unwrap(), b"en");
    let marked = catalog.get(b"MarkInfo").unwrap().as_dict().unwrap();
    assert!(marked.get(b"Marked").unwrap().as_bool().unwrap());
    let preferences = catalog
        .get(b"ViewerPreferences")
        .unwrap()
        .as_dict()
        .unwrap();
    assert!(preferences
        .get(b"DisplayDocTitle")
        .unwrap()
        .as_bool()
        .unwrap());

    let info_id = document
        .trailer
        .get(b"Info")
        .unwrap()
        .as_reference()
        .unwrap();
    let info = document.get_dictionary(info_id).unwrap();
    assert_eq!(
        lopdf::decode_text_string(info.get(b"Title").unwrap()).unwrap(),
        "Comparison report"
    );

    // bookmarks: the title, the summary and both changed pairs
    let outline_id = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
    let outline = document.get_dictionary(outline_id).unwrap();
    assert_eq!(outline.get(b"Count").unwrap().as_i64().unwrap(), 4);

    // the structure tree holds headings, a table, lists and pictures with text alternatives
    let roles: Vec<String> = document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| dict.get(b"Type").and_then(Object::as_name).ok() == Some(b"StructElem"))
        .map(|dict| {
            String::from_utf8_lossy(dict.get(b"S").unwrap().as_name().unwrap()).into_owned()
        })
        .collect();
    for role in [
        "Document", "H1", "H2", "H3", "P", "Table", "TR", "TH", "TD", "L", "LI", "LBody", "Figure",
        "Caption",
    ] {
        assert!(
            roles.iter().any(|found| found == role),
            "no {role} in {roles:?}"
        );
    }
    let figures_with_alt = document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| dict.get(b"S").and_then(Object::as_name).ok() == Some(b"Figure"))
        .filter(|dict| dict.get(b"Alt").is_ok())
        .count();
    assert_eq!(figures_with_alt, 4, "two thumbnails for each of two pairs");
    // every page says which structure it belongs to
    for (_, page_id) in document.get_pages() {
        let page = document.get_dictionary(page_id).unwrap();
        assert!(page.get(b"StructParents").is_ok());
    }
}

#[test]
fn a_report_without_renderers_still_lists_every_change() {
    let (old, new, _) = compared();
    let old_source = LopdfSource::open(&old.path).unwrap();
    let new_source = LopdfSource::open(&new.path).unwrap();
    let result = compare_documents(&old_source, &new_source).unwrap();
    let bytes = report_pdf(&result, &old_source, &new_source, &ReportOptions::default()).unwrap();
    let document = Document::load_mem(&bytes).unwrap();
    let text = words(&all_text(&document));
    assert!(
        contains_in_order(&text, "Changed “fox” to “cat”."),
        "{text:?}"
    );
    assert!(
        contains_in_order(&text, "Pictures and drawings weren’t compared, because"),
        "{text:?}"
    );
}

#[test]
fn a_report_of_identical_documents_says_so() {
    let spec = pdf(vec![numbered_page(1, 4)]);
    let (old, new) = write_pair("report-same", &spec, &spec);
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let bytes = report_pdf(
        &result,
        &old.source(),
        &new.source(),
        &ReportOptions::default(),
    )
    .unwrap();
    let document = Document::load_mem(&bytes).unwrap();
    assert_eq!(document.get_pages().len(), 1);
    let text = words(&all_text(&document));
    assert!(
        contains_in_order(
            &text,
            "The two versions are the same: no changes were found."
        ),
        "{text:?}"
    );
}

#[test]
fn text_and_html_reports_say_the_same_things() {
    let (_, _, result) = compared();
    let text = report_text(&result, &options());
    assert!(text.contains("Old version: essay-draft.pdf (3 November 2026)"));
    assert!(text.contains("  Total: 2"));
    assert!(text.contains("    - Changed “fox” to “cat”."));
    assert!(text.contains("    - Font size changed from 14 to 16 in “Introduction”."));
    let html = report_html(&result, &options());
    assert!(html.contains("<title>Comparison report</title>"));
    assert!(html.contains("<th scope=\"row\">Total</th><td>2</td>"));
    assert!(html.contains("Changed “fox” to “cat”.</li>"));
}
