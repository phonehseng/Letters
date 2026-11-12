//! inserting, extracting, merging and splitting: everything that moves
//! pages between documents.

mod support;

use letters_pages::{
    extract, insert_blank, insert_from, merge, split, ExtractParams, InsertBlankParams,
    InsertFromParams, MergeParams, PageSpan, PagesError, SplitParams,
};
use lopdf::{Dictionary, Document, Object, Stream};
use support::*;

#[test]
fn blank_pages_go_where_asked_with_the_size_asked() {
    let mut doc = tagged(2);
    let params = InsertBlankParams {
        at_index: 1,
        width_pt: 300.0,
        height_pt: 400.0,
        count: 2,
    };
    insert_blank(&mut doc, &params).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(widths(&doc), [100.0, 300.0, 300.0, 101.0]);
    let blank = page_ids(&doc)[1];
    assert_eq!(
        page_box(&doc, blank, b"MediaBox"),
        Some([0.0, 0.0, 300.0, 400.0])
    );
    assert!(doc.get_page_content(blank).is_empty());
}

#[test]
fn blank_pages_check_their_settings() {
    let mut doc = tagged(2);
    let before = fingerprint(&doc);
    let params = |at_index, width_pt, count| InsertBlankParams {
        at_index,
        width_pt,
        height_pt: 400.0,
        count,
    };
    assert_eq!(
        insert_blank(&mut doc, &params(3, 300.0, 1)),
        Err(PagesError::NoSuchPosition)
    );
    assert_eq!(
        insert_blank(&mut doc, &params(0, 1.0, 1)),
        Err(PagesError::PageSizeOutOfRange)
    );
    assert_eq!(
        insert_blank(&mut doc, &params(0, f32::NAN, 1)),
        Err(PagesError::PageSizeOutOfRange)
    );
    assert_eq!(
        insert_blank(&mut doc, &params(0, 300.0, 0)),
        Err(PagesError::BlankPageCountOutOfRange)
    );
    assert_eq!(fingerprint(&doc), before);
}

/// a document whose pages inherit their size, turn and resources, with a
/// big unused picture and an annotation, to check what an import carries.
fn donor() -> Document {
    let mut doc = build(&[
        PageSpec::new(250.0, 150.0).drawing("BT /F1 30 Tf 10 50 Td (Donor A) Tj ET"),
        PageSpec::new(260.0, 150.0).drawing("BT /F1 30 Tf 10 50 Td (Donor B) Tj ET"),
    ]);
    let pages = page_ids(&doc);
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    // move the first page's resources and turn onto the root, to be inherited
    let first = doc.get_dictionary_mut(pages[0]).unwrap();
    let resources = first.remove(b"Resources").unwrap();
    let root_dict = doc.get_dictionary_mut(root).unwrap();
    root_dict.set("Resources", resources);
    root_dict.set("Rotate", Object::Integer(90));
    // an object nothing on page 1 uses: it must not be copied with it
    let unused = doc.add_object(Stream::new(Dictionary::new(), vec![b'x'; 50_000]));
    doc.get_dictionary_mut(pages[1])
        .unwrap()
        .set("PieceInfo", Object::Reference(unused));
    // a note on page 1
    let mut note = Dictionary::new();
    note.set("Type", Object::Name(b"Annot".to_vec()));
    note.set("Subtype", Object::Name(b"Text".to_vec()));
    note.set("Rect", rect(0.0, 0.0, 20.0, 20.0));
    note.set("Contents", Object::string_literal("a note"));
    let note = doc.add_object(note);
    add_annotation(&mut doc, pages[0], note);
    // a link from page 1 to page 2, which won't come along
    add_link(&mut doc, 0, 1);
    doc
}

#[test]
fn imported_pages_bring_what_they_need_and_nothing_else() {
    let mut doc = tagged(2);
    let other = donor();
    let size_before = save(&mut doc.clone()).len();
    insert_from(
        &mut doc,
        &other,
        &InsertFromParams {
            at_index: 1,
            other_pages: vec![0],
        },
    )
    .unwrap();
    let mut doc = reload(&mut doc);
    assert_eq!(widths(&doc), [100.0, 250.0, 101.0]);
    let imported = page_ids(&doc)[1];
    let dict = doc.get_dictionary(imported).unwrap();
    // inherited settings are now written on the page itself
    assert_eq!(dict.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
    assert!(dict.has(b"Resources"));
    assert!(content_text(&doc, imported).contains("(Donor A)"));
    // the note came along; the link to a page that didn't was dropped
    let annots = dict.get(b"Annots").unwrap().as_array().unwrap();
    assert_eq!(annots.len(), 1);
    let note = doc
        .get_dictionary(annots[0].as_reference().unwrap())
        .unwrap();
    assert_eq!(note.get(b"Contents").unwrap().as_str().unwrap(), b"a note");
    // the unused 50 kB picture and the other page stayed behind
    let grown = save(&mut doc).len() - size_before;
    assert!(grown < 5_000, "the import added {grown} bytes");
    assert_no_dangling_references(&doc);
    // and both documents' pages still draw, so no object ids clashed
    let original = render(&doc, 0, "import-original");
    assert!(
        original.image.inked(0, 120, 80, 30) > 0,
        "page 0 still shows its text"
    );
    let copied = render(&doc, 1, "import-copied");
    assert_eq!(
        (copied.width_pt, copied.height_pt),
        (150.0, 250.0),
        "the turn came along"
    );
    assert!(copied.image.ink_bounds().is_some());
}

#[test]
fn importing_checks_both_documents() {
    let mut doc = tagged(2);
    let other = tagged(1);
    let before = fingerprint(&doc);
    let params = |at_index, other_pages: Vec<u32>| InsertFromParams {
        at_index,
        other_pages,
    };
    assert_eq!(
        insert_from(&mut doc, &other, &params(0, vec![1])),
        Err(PagesError::NoSuchPage(2))
    );
    assert_eq!(
        insert_from(&mut doc, &other, &params(5, vec![0])),
        Err(PagesError::NoSuchPosition)
    );
    assert_eq!(
        insert_from(&mut doc, &other, &params(0, vec![])),
        Err(PagesError::NoPagesChosen)
    );
    assert_eq!(fingerprint(&doc), before);
}

#[test]
fn imported_form_fields_join_the_form_under_a_free_name() {
    let mut doc = tagged(1);
    add_text_field(&mut doc, "name", &[0]);
    let mut other = tagged(2);
    add_text_field(&mut other, "name", &[0, 1]);
    insert_from(
        &mut doc,
        &other,
        &InsertFromParams {
            at_index: 1,
            other_pages: vec![1],
        },
    )
    .unwrap();
    let doc = reload(&mut doc);
    // the copied field only has the box on the copied page
    assert_eq!(
        fields(&doc),
        [("name".to_owned(), 1), ("name_2".to_owned(), 1)]
    );
    assert_no_dangling_references(&doc);
}

#[test]
fn extract_makes_a_small_new_document_with_its_bookmarks() {
    let mut doc = tagged(5);
    add_outline(
        &mut doc,
        &[
            bookmark("One", 0, vec![]),
            bookmark("Two", 1, vec![bookmark("Two a", 3, vec![])]),
            bookmark("Five", 4, vec![]),
        ],
    );
    add_link(&mut doc, 3, 1);
    add_link(&mut doc, 3, 0);
    let before = fingerprint(&doc);
    let mut part = extract(&doc, &ExtractParams { pages: vec![3, 1] }).unwrap();
    assert_eq!(fingerprint(&doc), before, "the original is untouched");
    let part = reload(&mut part);
    assert_eq!(order(&part), [3, 1]);
    assert_eq!(
        read_outline(&part),
        [
            (0, "Two".to_owned(), Some(1)),
            (1, "Two a".to_owned(), Some(0))
        ]
    );
    assert_eq!(link_targets(&part, 0), [Some(1)]);
    // only two pages' worth of objects: catalog, page list, two pages,
    // two drawings, one font, one link, three bookmark objects (lopdf
    // also keeps the cross-reference stream it read the file with)
    let objects = part
        .objects
        .values()
        .filter(|object| object.type_name().ok() != Some(b"XRef"))
        .count();
    assert_eq!(objects, 11, "{:?}", part.objects);
    assert_no_dangling_references(&part);
}

#[test]
fn merge_keeps_each_documents_bookmarks_under_its_name() {
    let mut first = tagged(2);
    add_outline(&mut first, &[bookmark("Intro", 1, vec![])]);
    let mut second = build(&[PageSpec::new(110.0, 150.0), PageSpec::new(111.0, 150.0)]);
    add_outline(
        &mut second,
        &[bookmark("Results", 0, vec![bookmark("Table", 1, vec![])])],
    );
    let names = vec!["Essay.pdf".to_owned(), "Appendix – data.pdf".to_owned()];
    let mut merged = merge(&[first, second], &MergeParams { names }).unwrap();
    let merged = reload(&mut merged);
    assert_eq!(order(&merged), [0, 1, 10, 11]);
    let outline = read_outline(&merged);
    let shape: Vec<(usize, Option<usize>)> = outline
        .iter()
        .map(|(depth, _, page)| (*depth, *page))
        .collect();
    assert_eq!(
        shape,
        [
            (0, Some(0)),
            (1, Some(1)),
            (0, Some(2)),
            (1, Some(2)),
            (2, Some(3))
        ]
    );
    assert_eq!(outline[0].1, "Essay.pdf");
    assert_eq!(outline[3].1, "Results");
    // a name outside plain ascii is stored so readers show it correctly
    assert_eq!(outline[2].1, "Appendix – data.pdf");
    assert_no_dangling_references(&merged);
}

#[test]
fn merge_checks_its_settings() {
    let names = |count: usize| MergeParams {
        names: vec!["a".to_owned(); count],
    };
    assert!(matches!(
        merge(&[], &names(0)),
        Err(PagesError::NothingToMerge)
    ));
    assert!(matches!(
        merge(&[tagged(1)], &names(2)),
        Err(PagesError::MergeNamesMissing)
    ));
}

fn suffixes(parts: &[letters_pages::SplitPart]) -> Vec<&str> {
    parts.iter().map(|part| part.name_suffix.as_str()).collect()
}

#[test]
fn split_by_ranges_then_merge_gives_back_the_original() {
    let doc = tagged(7);
    let ranges = vec![
        PageSpan { start: 0, end: 2 },
        PageSpan { start: 3, end: 3 },
        PageSpan { start: 4, end: 6 },
    ];
    let parts = split(&doc, &SplitParams::ByRanges { ranges }).unwrap();
    assert_eq!(suffixes(&parts), ["pages 1-3", "page 4", "pages 5-7"]);
    let mut reloaded: Vec<Document> = parts
        .into_iter()
        .map(|mut part| reload(&mut part.document))
        .collect();
    assert_eq!(order(&reloaded[1]), [3]);
    let names = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
    let mut merged = merge(&reloaded, &MergeParams { names }).unwrap();
    let merged = reload(&mut merged);
    assert_eq!(widths(&merged), widths(&doc));
    let heights: Vec<f32> = page_ids(&merged)
        .into_iter()
        .map(|page| page_box(&merged, page, b"MediaBox").unwrap()[3])
        .collect();
    assert_eq!(heights, [150.0; 7]);
    reloaded.clear();
}

#[test]
fn split_ranges_are_checked_first() {
    let doc = tagged(3);
    let by = |start, end| SplitParams::ByRanges {
        ranges: vec![PageSpan { start: 0, end: 0 }, PageSpan { start, end }],
    };
    assert!(matches!(
        split(&doc, &by(2, 1)),
        Err(PagesError::BackwardsRange(3, 2))
    ));
    assert!(matches!(
        split(&doc, &by(1, 3)),
        Err(PagesError::NoSuchPage(4))
    ));
    assert!(matches!(
        split(&doc, &SplitParams::ByRanges { ranges: vec![] }),
        Err(PagesError::NoPagesChosen)
    ));
}

#[test]
fn split_every_n_pages() {
    let doc = tagged(7);
    let parts = split(&doc, &SplitParams::EveryNPages { pages_per_part: 3 }).unwrap();
    assert_eq!(suffixes(&parts), ["pages 1-3", "pages 4-6", "page 7"]);
    let counts: Vec<u32> = parts.iter().map(|part| part.page_count).collect();
    assert_eq!(counts, [3, 3, 1]);
    assert!(matches!(
        split(&doc, &SplitParams::EveryNPages { pages_per_part: 0 }),
        Err(PagesError::PartSizeZero)
    ));
}

#[test]
fn split_by_bookmarks_makes_one_file_per_chapter() {
    let mut doc = tagged(6);
    add_outline(
        &mut doc,
        &[
            // listed out of page order on purpose
            bookmark("Chapter 2: Methods", 4, vec![]),
            bookmark("Chapter 1", 1, vec![bookmark("1.1", 2, vec![])]),
        ],
    );
    let parts = split(&doc, &SplitParams::ByBookmarks).unwrap();
    assert_eq!(
        suffixes(&parts),
        ["start", "Chapter 1", "Chapter 2 Methods"]
    );
    let pages: Vec<Vec<usize>> = parts
        .into_iter()
        .map(|mut part| order(&reload(&mut part.document)))
        .collect();
    assert_eq!(pages, [vec![0], vec![1, 2, 3], vec![4, 5]]);
    assert!(matches!(
        split(&tagged(2), &SplitParams::ByBookmarks),
        Err(PagesError::NoBookmarks)
    ));
}

#[test]
fn split_by_size_fills_each_file_up_to_the_limit() {
    // each page draws its own 2 kB of content, so sizes grow page by page
    let pages: Vec<PageSpec> = (0..9)
        .map(|index| {
            let filler = format!("% {}\n", "x".repeat(2000));
            PageSpec::new(tag_width(index), 150.0).drawing(&filler)
        })
        .collect();
    let mut doc = build(&pages);
    // page 6 is far bigger than the limit
    let big = page_ids(&doc)[6];
    let huge = format!("% {}\n", "y".repeat(30_000));
    let content = doc.add_object(Stream::new(Dictionary::new(), huge.into_bytes()));
    doc.get_dictionary_mut(big)
        .unwrap()
        .set("Contents", Object::Reference(content));

    let limit = 7_000;
    let parts = split(&doc, &SplitParams::ByMaxSize { max_bytes: limit }).unwrap();
    let mut seen = Vec::new();
    for mut part in parts {
        let size = save(&mut part.document).len() as u64;
        let pages = order(&part.document);
        if part.over_size_limit {
            assert_eq!(pages, [6], "only the huge page is over the limit");
            assert!(size > limit);
        } else {
            assert!(size <= limit, "part {pages:?} is {size} bytes");
            // greedy: one more page would not have fitted
            let next = pages.last().unwrap() + 1;
            if next < 9 && next != 6 {
                let ranges = vec![PageSpan {
                    start: pages[0] as u32,
                    end: next as u32,
                }];
                let mut longer = split(&doc, &SplitParams::ByRanges { ranges }).unwrap();
                assert!(save(&mut longer[0].document).len() as u64 > limit);
            }
        }
        seen.extend(pages);
    }
    assert_eq!(seen, (0..9).collect::<Vec<_>>());
    assert!(matches!(
        split(&doc, &SplitParams::ByMaxSize { max_bytes: 0 }),
        Err(PagesError::SizeLimitZero)
    ));
}
