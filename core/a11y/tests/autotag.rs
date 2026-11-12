//! building tags for untagged documents from their layout.

mod support;

use std::collections::HashMap;

use letters_a11y::{auto_tag, A11yError, AutoTagOptions, Confidence};
use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};
use support::*;

/// one structure element as read back from the file.
#[derive(Debug)]
struct Element {
    role: String,
    mcids: Vec<i64>,
    kids: Vec<Element>,
}

/// the elements under the Document element, read back with lopdf.
fn read_tags(doc: &Document) -> Vec<Element> {
    let catalog = doc.catalog().unwrap();
    let root = doc
        .get_dictionary(
            catalog
                .get(b"StructTreeRoot")
                .unwrap()
                .as_reference()
                .unwrap(),
        )
        .unwrap();
    let top = read_kids(doc, root.get(b"K").unwrap());
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].role, "Document");
    top.into_iter().next().unwrap().kids
}

fn read_kids(doc: &Document, kids: &Object) -> Vec<Element> {
    let list = match kids {
        Object::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    list.iter()
        .filter_map(|kid| {
            let id = kid.as_reference().ok()?;
            let dict = doc.get_dictionary(id).ok()?;
            let role = String::from_utf8_lossy(dict.get(b"S").ok()?.as_name().ok()?).into_owned();
            let own = dict.get(b"K").cloned().unwrap_or(Object::Array(Vec::new()));
            let own_list = match &own {
                Object::Array(items) => items.clone(),
                other => vec![other.clone()],
            };
            let mcids = own_list.iter().filter_map(|k| k.as_i64().ok()).collect();
            Some(Element {
                role,
                mcids,
                kids: read_kids(doc, &own),
            })
        })
        .collect()
}

fn roles(elements: &[Element]) -> Vec<&str> {
    elements.iter().map(|e| e.role.as_str()).collect()
}

/// what each mcid on a page holds: the text shown inside it, or "<image>".
fn mcid_contents(doc: &Document, page: ObjectId) -> HashMap<i64, String> {
    let content = Content::decode(&doc.get_page_content(page)).unwrap();
    let mut found = HashMap::new();
    let mut stack: Vec<Option<i64>> = Vec::new();
    for op in &content.operations {
        match op.operator.as_str() {
            "BDC" => stack.push(match op.operands.get(1) {
                Some(Object::Dictionary(d)) => d.get(b"MCID").ok().and_then(|m| m.as_i64().ok()),
                _ => None,
            }),
            "BMC" => stack.push(None),
            "EMC" => {
                stack.pop();
            }
            "Tj" | "Do" => {
                if let Some(Some(mcid)) = stack.last() {
                    let piece = match op.operator.as_str() {
                        "Tj" => {
                            String::from_utf8_lossy(op.operands[0].as_str().unwrap()).into_owned()
                        }
                        _ => "<image>".to_owned(),
                    };
                    let entry: &mut String = found.entry(*mcid).or_default();
                    if !entry.is_empty() {
                        entry.push(' ');
                    }
                    entry.push_str(&piece);
                }
            }
            _ => {}
        }
    }
    found
}

/// text shown outside any tagged sequence and outside artifacts.
fn untagged_text(doc: &Document, page: ObjectId) -> Vec<String> {
    let content = Content::decode(&doc.get_page_content(page)).unwrap();
    let mut depth = 0;
    let mut loose = Vec::new();
    for op in &content.operations {
        match op.operator.as_str() {
            "BDC" | "BMC" => depth += 1,
            "EMC" => depth -= 1,
            "Tj" if depth == 0 => {
                loose.push(String::from_utf8_lossy(op.operands[0].as_str().unwrap()).into_owned())
            }
            _ => {}
        }
    }
    loose
}

/// a simple page: a title, two paragraphs of two lines, a list whose
/// bullets are drawn on their own, a picture and a page number.
fn simple_page() -> String {
    let mut content = text("F1", 24.0, 72.0, 90.0, "Rivers");
    content.push_str(&text("F1", 12.0, 72.0, 130.0, "Rivers carry water"));
    content.push_str(&text("F1", 12.0, 72.0, 145.0, "to the sea."));
    content.push_str(&text("F1", 12.0, 72.0, 175.0, "Some rivers freeze"));
    content.push_str(&text("F1", 12.0, 72.0, 190.0, "in winter."));
    for (index, item) in ["Rhine", "Danube"].iter().enumerate() {
        let y = 220.0 + index as f32 * 18.0;
        content.push_str(&text("F1", 12.0, 72.0, y, "-"));
        content.push_str(&text("F1", 12.0, 90.0, y, item));
    }
    content.push_str(&picture(72.0, 300.0, 200.0, 150.0));
    content.push_str(&text("F1", 10.0, 300.0, 770.0, "1"));
    content
}

fn untagged(pages: &[String]) -> Document {
    let mut builder = Builder::new();
    for page in pages {
        builder.page(page);
    }
    let mut doc = builder.build();
    add_metadata(&mut doc, "en", "Rivers");
    doc
}

#[test]
fn a_simple_page_gets_the_expected_tags() {
    let mut doc = untagged(&[simple_page()]);
    let result = auto_tag(&mut doc, &AutoTagOptions::default()).unwrap();
    let tags = read_tags(&doc);
    assert_eq!(
        roles(&tags),
        vec!["H1", "P", "P", "L", "Figure"],
        "{tags:#?}"
    );
    let list = &tags[3];
    assert_eq!(roles(&list.kids), vec!["LI", "LI"]);
    assert_eq!(roles(&list.kids[0].kids), vec!["Lbl", "LBody"]);
    assert_eq!(result.counts.headings, 1);
    assert_eq!(result.counts.paragraphs, 2);
    assert_eq!(result.counts.lists, 1);
    assert_eq!(result.counts.figures, 1);

    // the mcids of each tag hold exactly the content it should
    let page = doc.get_pages()[&1];
    let contents = mcid_contents(&doc, page);
    let text_of = |element: &Element| -> String {
        element
            .mcids
            .iter()
            .map(|m| contents[m].clone())
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(text_of(&tags[0]), "Rivers");
    assert_eq!(text_of(&tags[1]), "Rivers carry water to the sea.");
    assert_eq!(text_of(&tags[2]), "Some rivers freeze in winter.");
    assert_eq!(text_of(&list.kids[0].kids[0]), "-");
    assert_eq!(text_of(&list.kids[1].kids[1]), "Danube");
    assert_eq!(text_of(&tags[4]), "<image>");
    // the page number is decoration, not loose content
    assert!(untagged_text(&doc, page).is_empty());
}

#[test]
fn auto_tag_leaves_only_the_alt_text_question() {
    let mut doc = untagged(&[simple_page()]);
    let original = doc.clone();
    assert!(problem_ids(&doc).contains("not-tagged"));
    let result = auto_tag(&mut doc, &AutoTagOptions::default()).unwrap();
    assert_eq!(problem_ids(&reload(&doc)), set(&["missing-alt-text"]));
    let figure = result
        .decisions
        .iter()
        .find(|d| d.decision.contains("figure"))
        .unwrap();
    assert_eq!(figure.confidence, Confidence::Medium);
    assert!(figure.element.is_some());
    assert!(result.changes.iter().any(|c| c.contains("best guess")));
    assert_same_pixels(&original, &doc, "auto-tag-simple");
}

#[test]
fn repeating_headers_and_page_numbers_become_artifacts() {
    let page = |number: usize| {
        format!(
            "{}{}{}",
            text("F1", 9.0, 72.0, 30.0, "Essay on rivers"),
            text("F1", 12.0, 72.0, 120.0, "Body text of the page."),
            text("F1", 9.0, 300.0, 770.0, &format!("Page {number}"))
        )
    };
    let mut doc = untagged(&[page(1), page(2)]);
    auto_tag(&mut doc, &AutoTagOptions::default()).unwrap();
    let tags = read_tags(&doc);
    assert_eq!(
        roles(&tags),
        vec!["P", "P"],
        "only the body text is content: {tags:#?}"
    );
    for page in doc.get_pages().into_values() {
        assert!(untagged_text(&doc, page).is_empty());
    }
    let content = String::from_utf8_lossy(&doc.get_page_content(doc.get_pages()[&1])).into_owned();
    assert!(
        content.contains("/Pagination"),
        "headers and footers are pagination artifacts"
    );
}

#[test]
fn a_ruled_grid_becomes_a_table_with_a_header_row() {
    let mut content = text("F1", 18.0, 72.0, 80.0, "Results");
    // a 3 by 2 grid from 120 to 210 points down the page
    for row in 0..=3 {
        let y = HEIGHT - (120.0 + row as f32 * 30.0);
        content.push_str(&format!("0 G 1 w 72 {y} m 372 {y} l S\n"));
    }
    for x in [72.0, 222.0, 372.0] {
        content.push_str(&format!(
            "0 G 1 w {x} {} m {x} {} l S\n",
            HEIGHT - 120.0,
            HEIGHT - 210.0
        ));
    }
    for (row, cells) in [["River", "Length"], ["Rhine", "1230"], ["Danube", "2850"]]
        .iter()
        .enumerate()
    {
        for (column, cell) in cells.iter().enumerate() {
            content.push_str(&text(
                "F1",
                11.0,
                80.0 + column as f32 * 150.0,
                140.0 + row as f32 * 30.0,
                cell,
            ));
        }
    }
    let mut doc = untagged(&[content]);
    let original = doc.clone();
    let result = auto_tag(&mut doc, &AutoTagOptions::default()).unwrap();
    let tags = read_tags(&doc);
    assert_eq!(roles(&tags), vec!["H1", "Table"], "{tags:#?}");
    let rows = &tags[1].kids;
    assert_eq!(roles(rows), vec!["TR", "TR", "TR"]);
    assert_eq!(roles(&rows[0].kids), vec!["TH", "TH"]);
    assert_eq!(roles(&rows[2].kids), vec!["TD", "TD"]);
    let contents = mcid_contents(&doc, doc.get_pages()[&1]);
    assert_eq!(contents[&rows[2].kids[1].mcids[0]], "2850");
    let table = result
        .decisions
        .iter()
        .find(|d| d.decision.contains("table"))
        .unwrap();
    assert_eq!(table.confidence, Confidence::Low);
    // the headers have a scope, so nothing about the table is left to fix
    assert_eq!(problem_ids(&reload(&doc)), set(&[]));
    assert_same_pixels(&original, &doc, "auto-tag-table");
}

#[test]
fn two_columns_are_tagged_one_column_at_a_time() {
    let mut content = text("F1", 24.0, 72.0, 80.0, "Two columns");
    for row in 0..3 {
        for (column, words) in ["Left", "Right"].iter().enumerate() {
            content.push_str(&text(
                "F1",
                12.0,
                72.0 + column as f32 * 260.0,
                140.0 + row as f32 * 40.0,
                &format!("{words} {row}"),
            ));
        }
    }
    let mut doc = untagged(&[content]);
    auto_tag(&mut doc, &AutoTagOptions::default()).unwrap();
    let contents = mcid_contents(&doc, doc.get_pages()[&1]);
    let order: Vec<String> = read_tags(&doc)
        .iter()
        .map(|e| {
            e.mcids
                .iter()
                .map(|m| contents[m].clone())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    assert_eq!(
        order,
        vec![
            "Two columns",
            "Left 0",
            "Left 1",
            "Left 2",
            "Right 0",
            "Right 1",
            "Right 2"
        ]
    );
    assert_eq!(problem_ids(&reload(&doc)), set(&[]));
}

#[test]
fn bold_short_lines_become_headings_and_levels_follow_size() {
    let mut content = text("F1", 20.0, 72.0, 80.0, "Big title");
    content.push_str(&text("F3", 12.0, 72.0, 130.0, "Bold section"));
    content.push_str(&text(
        "F2",
        12.0,
        72.0,
        160.0,
        "Plain body text that runs on for a while.",
    ));
    content.push_str(&text(
        "F2",
        12.0,
        72.0,
        175.0,
        "More plain body text in the same paragraph.",
    ));
    let mut doc = untagged(&[content]);
    auto_tag(&mut doc, &AutoTagOptions::default()).unwrap();
    assert_eq!(roles(&read_tags(&doc)), vec!["H1", "H2", "P"]);
}

#[test]
fn a_tagged_document_is_only_retagged_when_asked() {
    let mut doc = good_document();
    assert_eq!(
        auto_tag(&mut doc, &AutoTagOptions::default()).unwrap_err(),
        A11yError::AlreadyTagged
    );
    let options = AutoTagOptions {
        replace_existing: true,
        language: Some("en-GB".into()),
    };
    auto_tag(&mut doc, &options).unwrap();
    assert_eq!(roles(&read_tags(&doc)), vec!["H1", "P"]);
    assert_eq!(problem_ids(&reload(&doc)), set(&[]));
    let lang = doc
        .catalog()
        .unwrap()
        .get(b"Lang")
        .unwrap()
        .as_str()
        .unwrap()
        .to_vec();
    assert_eq!(lang, b"en-GB");
}

#[test]
fn links_are_tagged_with_the_text_they_sit_on() {
    let mut builder = Builder::new();
    builder.page(&format!(
        "{}{}",
        text("F1", 24.0, 72.0, 80.0, "Links"),
        text("F1", 12.0, 72.0, 140.0, "River data for 2024")
    ));
    let mut link = lopdf::Dictionary::new();
    link.set("Type", name("Annot"));
    link.set("Subtype", name("Link"));
    link.set(
        "Rect",
        numbers(&[70.0, HEIGHT - 145.0, 300.0, HEIGHT - 128.0]),
    );
    link.set("Contents", string("River data for 2024"));
    builder.annotation(0, link);
    let mut doc = builder.build();
    add_metadata(&mut doc, "en", "Links");
    auto_tag(&mut doc, &AutoTagOptions::default()).unwrap();
    let tags = read_tags(&doc);
    assert_eq!(roles(&tags[1].kids), vec!["Link"]);
    assert_eq!(problem_ids(&reload(&doc)), set(&[]));
}
