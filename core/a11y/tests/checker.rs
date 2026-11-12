//! each checker rule on a hand-built document: a passing case and a
//! failing case.

mod support;

use letters_a11y::{FixKind, RuleStatus, Severity};
use lopdf::{Dictionary, Document, Object};
use support::*;

/// a tagged document with the given page content and tags, plus good
/// metadata, so only the rule under test can fail.
fn tagged(content: &str, top: Vec<Kid>) -> Document {
    let mut builder = Builder::new();
    builder.page(content);
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    add_tags(&mut doc, &pages, top);
    add_metadata(&mut doc, "en-US", "Test");
    doc
}

fn heading_and(content: &str) -> String {
    format!(
        "{}{content}",
        marked("H1", 0, &text("F1", 24.0, 72.0, 80.0, "Title"))
    )
}

fn status(doc: &Document, rule: &str) -> RuleStatus {
    check(doc)
        .rules
        .iter()
        .find(|r| r.id == rule)
        .unwrap()
        .status
}

// ---- the whole document ---------------------------------------------------------------

#[test]
fn a_good_document_passes_every_machine_check() {
    let report = check(&good_document());
    assert_eq!(problems(&report), set(&[]), "{:#?}", report.findings);
    assert_eq!(report.score.percent, 100);
    assert_eq!(report.score.failed, 0);
    // the checks by hand are still listed, because a person must do them
    assert!(report.has("check-reading-order-logical"));
    assert!(report
        .findings
        .iter()
        .all(|f| f.severity == Severity::CheckByHand));
}

#[test]
fn the_score_is_honest_about_what_was_not_checked() {
    let report = check(&good_document());
    // without a renderer, contrast can't be measured and says so
    let contrast = report
        .rules
        .iter()
        .find(|r| r.id == "low-contrast")
        .unwrap();
    assert_eq!(contrast.status, RuleStatus::NotChecked);
    assert!(contrast.reason.as_deref().unwrap().contains("picture"));
    assert_eq!(report.score.not_checked, 1);
    assert!(report.score.statement.contains("could not be checked"));
}

#[test]
fn an_untagged_document_gets_one_finding_for_all_its_tags() {
    let mut builder = Builder::new();
    builder.page(&format!(
        "{}{}",
        text("F1", 12.0, 72.0, 100.0, "Hello"),
        picture(72.0, 200.0, 100.0, 100.0)
    ));
    let mut doc = builder.build();
    add_metadata(&mut doc, "en", "Hello");
    let report = check(&doc);
    assert_eq!(problems(&report), set(&["not-tagged"]));
    let finding = report.with_id("not-tagged").next().unwrap();
    assert_eq!(finding.fix, Some(FixKind::AddTags));
    assert_eq!(finding.severity, Severity::MustFix);
    for rule in [
        "missing-alt-text",
        "image-not-tagged",
        "untagged-content",
        "reading-order-mismatch",
    ] {
        assert_eq!(status(&doc, rule), RuleStatus::NotChecked, "{rule}");
    }
}

#[test]
fn a_tree_without_the_tagged_flag_is_reported() {
    let mut doc = good_document();
    let catalog = doc.catalog_mut().unwrap();
    let mut info = Dictionary::new();
    info.set("Marked", Object::Boolean(false));
    catalog.set("MarkInfo", Object::Dictionary(info));
    assert_eq!(problem_ids(&doc), set(&["tagged-flag-missing"]));
}

#[test]
fn an_encrypted_document_asks_to_be_unlocked() {
    let mut doc = good_document();
    let mut encrypt = Dictionary::new();
    encrypt.set("Filter", name("Standard"));
    let encrypt = doc.add_object(encrypt);
    doc.trailer.set("Encrypt", Object::Reference(encrypt));
    let error = letters_a11y::check(&doc, None).unwrap_err();
    assert_eq!(error, letters_a11y::A11yError::Locked);
    assert!(error.to_string().contains("Unlock it first"));
}

// ---- language ---------------------------------------------------------------------------

#[test]
fn a_missing_language_is_reported() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().remove(b"Lang");
    assert_eq!(problem_ids(&doc), set(&["missing-language"]));
}

#[test]
fn an_invalid_language_is_reported() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().set("Lang", string("english"));
    assert_eq!(problem_ids(&doc), set(&["invalid-language"]));
}

#[test]
fn greek_text_in_an_english_document_needs_its_language() {
    let content = heading_and(&marked("P", 1, &text("F4", 12.0, 72.0, 140.0, "ABGDE")));
    let doc = tagged(&content, vec![leaf("H1", 0, &[0]), leaf("P", 0, &[1])]);
    assert_eq!(problem_ids(&doc), set(&["language-change-unmarked"]));

    let marked_greek = tagged(
        &content,
        vec![
            leaf("H1", 0, &[0]),
            with(leaf("P", 0, &[1]), "Lang", string("el")),
        ],
    );
    assert_eq!(problem_ids(&marked_greek), set(&[]));
}

// ---- title -----------------------------------------------------------------------------

#[test]
fn a_missing_title_is_reported_with_a_suggestion() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().remove(b"Metadata");
    doc.trailer.remove(b"Info");
    let report = check(&doc);
    assert_eq!(problems(&report), set(&["missing-title"]));
    let fix = report.with_id("missing-title").next().unwrap().fix.clone();
    assert_eq!(
        fix,
        Some(FixKind::SetTitle {
            suggested: Some("Rivers".into())
        })
    );
}

#[test]
fn a_title_only_in_the_old_information_is_not_enough() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().remove(b"Metadata");
    let report = check(&doc);
    assert_eq!(problems(&report), set(&["missing-title"]));
    assert!(report
        .with_id("missing-title")
        .next()
        .unwrap()
        .details
        .contains("only in the old"));
}

#[test]
fn a_title_that_is_not_displayed_is_reported() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().remove(b"ViewerPreferences");
    assert_eq!(problem_ids(&doc), set(&["title-not-displayed"]));
}

// ---- pictures ---------------------------------------------------------------------------

fn figure_document(figure: Kid) -> Document {
    let content = heading_and(&marked("Figure", 1, &picture(72.0, 200.0, 200.0, 150.0)));
    tagged(&content, vec![leaf("H1", 0, &[0]), figure])
}

#[test]
fn a_figure_with_alt_text_passes() {
    let doc = figure_document(with(
        leaf("Figure", 0, &[1]),
        "Alt",
        string("A red, green and blue square"),
    ));
    assert_eq!(problem_ids(&doc), set(&[]));
}

#[test]
fn a_figure_without_alt_text_is_reported_where_it_is() {
    let doc = figure_document(leaf("Figure", 0, &[1]));
    let report = check(&doc);
    assert_eq!(problems(&report), set(&["missing-alt-text"]));
    let finding = report.with_id("missing-alt-text").next().unwrap();
    assert_eq!(finding.location.page, Some(0));
    let rect = finding.location.rect.unwrap();
    assert!(
        (rect.x - 72.0).abs() < 0.5 && (rect.y - 200.0).abs() < 0.5,
        "{rect:?}"
    );
    assert!((rect.width - 200.0).abs() < 0.5 && (rect.height - 150.0).abs() < 0.5);
    assert_eq!(
        finding.location.structure_path.as_deref(),
        Some("Document › Figure")
    );
}

#[test]
fn a_file_name_as_alt_text_is_reported() {
    let doc = figure_document(with(leaf("Figure", 0, &[1]), "Alt", string("IMG_2041.jpg")));
    assert_eq!(problem_ids(&doc), set(&["alt-text-is-file-name"]));
}

#[test]
fn an_untagged_picture_is_reported_and_an_artifact_is_not() {
    let untagged = tagged(
        &heading_and(&picture(72.0, 200.0, 200.0, 150.0)),
        vec![leaf("H1", 0, &[0])],
    );
    let report = check(&untagged);
    assert_eq!(problems(&report), set(&["image-not-tagged"]));
    assert!(matches!(
        report.with_id("image-not-tagged").next().unwrap().fix,
        Some(FixKind::SetAltText { .. })
    ));

    let decoration = tagged(
        &heading_and(&artifact(&picture(72.0, 200.0, 200.0, 150.0))),
        vec![leaf("H1", 0, &[0])],
    );
    assert_eq!(problem_ids(&decoration), set(&[]));
}

// ---- headings -------------------------------------------------------------------------------

fn headings(levels: &[u8]) -> Document {
    let mut content = String::new();
    let mut tags = Vec::new();
    for (index, level) in levels.iter().enumerate() {
        let role = format!("H{level}");
        content.push_str(&marked(
            &role,
            index as i64,
            &text("F1", 14.0, 72.0, 80.0 + index as f32 * 30.0, "Part"),
        ));
        tags.push(leaf(&role, 0, &[index as i64]));
    }
    tagged(&content, tags)
}

#[test]
fn heading_levels_in_order_pass() {
    assert_eq!(problem_ids(&headings(&[1, 2, 3, 2, 1])), set(&[]));
}

#[test]
fn a_first_heading_that_is_not_h1_is_reported() {
    assert_eq!(
        problem_ids(&headings(&[2, 3])),
        set(&["first-heading-not-h1"])
    );
}

#[test]
fn a_skipped_heading_level_is_reported() {
    let report = check(&headings(&[1, 3]));
    assert_eq!(problems(&report), set(&["heading-level-skipped"]));
    assert_eq!(
        report.with_id("heading-level-skipped").next().unwrap().fix,
        Some(FixKind::FixHeadingLevels)
    );
}

#[test]
fn a_long_document_without_headings_is_reported() {
    let mut builder = Builder::new();
    builder.page(&marked("P", 0, &text("F1", 12.0, 72.0, 100.0, "One")));
    builder.page(&marked("P", 0, &text("F1", 12.0, 72.0, 100.0, "Two")));
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    add_tags(
        &mut doc,
        &pages,
        vec![leaf("P", 0, &[0]), leaf("P", 1, &[0])],
    );
    add_metadata(&mut doc, "en", "Two pages");
    assert_eq!(problem_ids(&doc), set(&["no-headings"]));
}

// ---- reading order ---------------------------------------------------------------------------

/// a title across the page, then two columns of three paragraphs each.
/// mcid 0 is the title; 1 to 3 the left column top down; 4 to 6 the right.
fn two_columns(tag_order: &[i64]) -> Document {
    let mut content = marked("H1", 0, &text("F1", 24.0, 72.0, 80.0, "Two columns"));
    for column in 0..2 {
        for row in 0..3 {
            let mcid = 1 + column * 3 + row;
            let x = 72.0 + column as f32 * 250.0;
            let y = 140.0 + row as f32 * 60.0;
            content.push_str(&marked(
                "P",
                mcid,
                &text("F1", 12.0, x, y, "Some words here"),
            ));
        }
    }
    let tags = tag_order
        .iter()
        .map(|&mcid| leaf(if mcid == 0 { "H1" } else { "P" }, 0, &[mcid]))
        .collect();
    tagged(&content, tags)
}

#[test]
fn tags_in_column_order_pass_on_a_two_column_page() {
    assert_eq!(problem_ids(&two_columns(&[0, 1, 2, 3, 4, 5, 6])), set(&[]));
}

#[test]
fn tags_read_across_the_columns_are_reported() {
    // row by row across both columns, the classic mistake
    let report = check(&two_columns(&[0, 1, 4, 2, 5, 3, 6]));
    assert_eq!(problems(&report), set(&["reading-order-mismatch"]));
    let Some(FixKind::SetReadingOrder { page, proposal }) = report
        .with_id("reading-order-mismatch")
        .next()
        .unwrap()
        .fix
        .clone()
    else {
        panic!("a reading order proposal was expected");
    };
    assert_eq!(page, 0);
    assert_eq!(proposal.len(), 7);
}

// ---- tables -----------------------------------------------------------------------------------

/// a 2 by 2 table: mcids 1 and 2 the first row, 3 and 4 the second.
fn table(first_row: &str, header_scope: Option<&str>, second_row_cells: usize) -> Document {
    let mut content = heading_and("");
    let mut mcid = 1;
    let mut rows = Vec::new();
    for (row, role) in [first_row, "TD"].iter().enumerate() {
        let cells = if row == 0 { 2 } else { second_row_cells };
        let mut kids = Vec::new();
        for column in 0..cells {
            content.push_str(&marked(
                role,
                mcid,
                &text(
                    "F1",
                    12.0,
                    72.0 + column as f32 * 150.0,
                    140.0 + row as f32 * 30.0,
                    "Cell",
                ),
            ));
            let mut cell = leaf(role, 0, &[mcid]);
            if let (0, Some(scope_name)) = (row, header_scope) {
                cell = with(cell, "A", scope(scope_name));
            }
            kids.push(cell);
            mcid += 1;
        }
        rows.push(tag("TR", kids));
    }
    tagged(&content, vec![leaf("H1", 0, &[0]), tag("Table", rows)])
}

#[test]
fn a_table_with_scoped_headers_passes() {
    assert_eq!(problem_ids(&table("TH", Some("Column"), 2)), set(&[]));
}

#[test]
fn a_table_without_headers_is_reported() {
    let report = check(&table("TD", None, 2));
    assert_eq!(problems(&report), set(&["table-missing-headers"]));
    assert!(matches!(
        report.with_id("table-missing-headers").next().unwrap().fix,
        Some(FixKind::SetTableHeaders {
            suggested_header_rows: 1,
            ..
        })
    ));
}

#[test]
fn headers_without_a_scope_are_reported() {
    assert_eq!(
        problem_ids(&table("TH", None, 2)),
        set(&["table-header-missing-scope"])
    );
}

#[test]
fn rows_of_different_lengths_are_reported() {
    assert_eq!(
        problem_ids(&table("TH", Some("Column"), 1)),
        set(&["table-irregular-rows"])
    );
}

#[test]
fn a_cell_outside_a_row_is_reported() {
    let content = heading_and(&marked("TD", 1, &text("F1", 12.0, 72.0, 140.0, "Loose")));
    let doc = tagged(
        &content,
        vec![leaf("H1", 0, &[0]), tag("Table", vec![leaf("TD", 0, &[1])])],
    );
    assert_eq!(problem_ids(&doc), set(&["table-structure-invalid"]));
}

// ---- lists ----------------------------------------------------------------------------------------

fn list_content() -> String {
    heading_and(&format!(
        "{}{}",
        marked("P", 1, &text("F1", 12.0, 72.0, 140.0, "- Apples")),
        marked("P", 2, &text("F1", 12.0, 72.0, 160.0, "- Pears"))
    ))
}

#[test]
fn a_well_formed_list_passes() {
    let item = |mcid| tag("LI", vec![leaf("LBody", 0, &[mcid])]);
    let doc = tagged(
        &list_content(),
        vec![leaf("H1", 0, &[0]), tag("L", vec![item(1), item(2)])],
    );
    assert_eq!(problem_ids(&doc), set(&[]));
}

#[test]
fn a_list_holding_paragraphs_is_reported() {
    let doc = tagged(
        &list_content(),
        vec![
            leaf("H1", 0, &[0]),
            tag("L", vec![leaf("P", 0, &[1]), leaf("P", 0, &[2])]),
        ],
    );
    assert_eq!(problem_ids(&doc), set(&["list-structure-invalid"]));
}

#[test]
fn bulleted_paragraphs_are_reported_as_a_hidden_list() {
    let doc = tagged(
        &list_content(),
        vec![leaf("H1", 0, &[0]), leaf("P", 0, &[1]), leaf("P", 0, &[2])],
    );
    assert_eq!(problem_ids(&doc), set(&["list-not-tagged-as-list"]));
}

// ---- links, notes and fields -------------------------------------------------------------------------

fn link(contents: Option<&str>) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", name("Annot"));
    dict.set("Subtype", name("Link"));
    // over the words of the paragraph, 140 points from the top
    dict.set(
        "Rect",
        numbers(&[70.0, HEIGHT - 145.0, 300.0, HEIGHT - 128.0]),
    );
    let mut action = Dictionary::new();
    action.set("S", name("URI"));
    action.set("URI", string("https://example.com/rivers"));
    dict.set("A", Object::Dictionary(action));
    if let Some(contents) = contents {
        dict.set("Contents", string(contents));
    }
    dict
}

/// a document whose paragraph is a link; `tag_it` puts a Link tag around it.
fn link_document(words: &str, contents: Option<&str>, tag_it: bool, tabs: bool) -> Document {
    let mut builder = Builder::new();
    builder.page(&heading_and(&marked(
        "P",
        1,
        &text("F1", 12.0, 72.0, 140.0, words),
    )));
    let annotation = builder.annotation(0, link(contents));
    if tabs {
        builder
            .doc
            .get_dictionary_mut(builder.pages[0])
            .unwrap()
            .set("Tabs", name("S"));
    }
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    let paragraph = if tag_it {
        tag(
            "P",
            vec![
                Kid::Content(0, 1),
                tag("Link", vec![Kid::Object(0, annotation)]),
            ],
        )
    } else {
        leaf("P", 0, &[1])
    };
    add_tags(&mut doc, &pages, vec![leaf("H1", 0, &[0]), paragraph]);
    add_metadata(&mut doc, "en", "Links");
    doc
}

#[test]
fn a_tagged_described_link_passes() {
    let doc = link_document(
        "River data for 2024",
        Some("River data for 2024"),
        true,
        true,
    );
    assert_eq!(problem_ids(&doc), set(&[]));
}

#[test]
fn an_untagged_link_is_reported() {
    let doc = link_document("River data for 2024", Some("River data"), false, true);
    assert_eq!(problem_ids(&doc), set(&["link-not-tagged"]));
}

#[test]
fn a_link_without_a_description_is_reported_with_a_suggestion() {
    let report = check(&link_document("River data for 2024", None, true, true));
    assert_eq!(problems(&report), set(&["link-missing-description"]));
    let Some(FixKind::AddLinkText { suggested, .. }) = report
        .with_id("link-missing-description")
        .next()
        .unwrap()
        .fix
        .clone()
    else {
        panic!("a link text fix was expected");
    };
    assert_eq!(suggested.as_deref(), Some("River data for 2024"));
}

#[test]
fn vague_link_text_is_reported() {
    let doc = link_document("Click here", Some("Click here"), true, true);
    assert_eq!(problem_ids(&doc), set(&["link-text-not-descriptive"]));
}

#[test]
fn a_page_with_links_needs_tab_order_from_the_tags() {
    let doc = link_document("River data for 2024", Some("River data"), true, false);
    assert_eq!(problem_ids(&doc), set(&["tab-order-not-structure"]));
}

#[test]
fn an_untagged_note_without_text_is_reported() {
    let mut builder = Builder::new();
    builder.page(&heading_and(""));
    let mut note = Dictionary::new();
    note.set("Type", name("Annot"));
    note.set("Subtype", name("Square"));
    note.set("Rect", numbers(&[100.0, 100.0, 200.0, 200.0]));
    builder.annotation(0, note);
    builder
        .doc
        .get_dictionary_mut(builder.pages[0])
        .unwrap()
        .set("Tabs", name("S"));
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    add_tags(&mut doc, &pages, vec![leaf("H1", 0, &[0])]);
    add_metadata(&mut doc, "en", "Notes");
    assert_eq!(
        problem_ids(&doc),
        set(&["annotation-not-tagged", "annotation-missing-contents"])
    );
}

/// a document with one text field, tagged or not, with a tooltip or not.
pub fn field_document(tooltip: Option<&str>, tag_it: bool) -> Document {
    let mut builder = Builder::new();
    builder.page(&heading_and(""));
    let mut widget = Dictionary::new();
    widget.set("Type", name("Annot"));
    widget.set("Subtype", name("Widget"));
    widget.set("FT", name("Tx"));
    widget.set("T", string("first_name"));
    widget.set("Rect", numbers(&[72.0, 600.0, 272.0, 620.0]));
    if let Some(tooltip) = tooltip {
        widget.set("TU", string(tooltip));
    }
    let field = builder.annotation(0, widget);
    builder
        .doc
        .get_dictionary_mut(builder.pages[0])
        .unwrap()
        .set("Tabs", name("S"));
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    let mut form = Dictionary::new();
    form.set("Fields", Object::Array(vec![Object::Reference(field)]));
    doc.catalog_mut()
        .unwrap()
        .set("AcroForm", Object::Dictionary(form));
    let mut top = vec![leaf("H1", 0, &[0])];
    if tag_it {
        top.push(tag("Form", vec![Kid::Object(0, field)]));
    }
    add_tags(&mut doc, &pages, top);
    add_metadata(&mut doc, "en", "Form");
    doc
}

#[test]
fn a_tagged_field_with_a_tooltip_passes() {
    assert_eq!(
        problem_ids(&field_document(Some("First name"), true)),
        set(&[])
    );
}

#[test]
fn a_field_without_a_tooltip_or_tag_is_reported() {
    let report = check(&field_document(None, false));
    assert_eq!(
        problems(&report),
        set(&["field-missing-tooltip", "field-not-tagged"])
    );
    let Some(FixKind::SetFieldTooltip { suggested, .. }) = report
        .with_id("field-missing-tooltip")
        .next()
        .unwrap()
        .fix
        .clone()
    else {
        panic!("a tooltip fix was expected");
    };
    assert_eq!(suggested.as_deref(), Some("First name"));
}

// ---- content and fonts --------------------------------------------------------------------------------

#[test]
fn untagged_text_in_a_tagged_document_is_reported() {
    let content = heading_and(&text("F1", 12.0, 72.0, 140.0, "Forgotten"));
    let report = check(&tagged(&content, vec![leaf("H1", 0, &[0])]));
    assert_eq!(problems(&report), set(&["untagged-content"]));
    let finding = report.with_id("untagged-content").next().unwrap();
    assert_eq!(finding.fix, Some(FixKind::TagUntaggedContent { page: 0 }));
    assert!(finding.details.contains("1 piece of text"));
}

#[test]
fn decoration_marked_as_artifact_passes() {
    let content = heading_and(&artifact(&frame(50.0, 50.0, 500.0, 700.0)));
    assert_eq!(
        problem_ids(&tagged(&content, vec![leaf("H1", 0, &[0])])),
        set(&[])
    );
}

#[test]
fn a_font_that_is_not_embedded_is_reported() {
    let content = heading_and(&marked("P", 1, &text("F2", 12.0, 72.0, 140.0, "Helvetica")));
    let report = check(&tagged(
        &content,
        vec![leaf("H1", 0, &[0]), leaf("P", 0, &[1])],
    ));
    assert_eq!(problems(&report), set(&["font-not-embedded"]));
    assert!(report
        .with_id("font-not-embedded")
        .next()
        .unwrap()
        .details
        .contains("Helvetica"));
}

#[test]
fn invisible_text_need_not_be_embedded() {
    let invisible = "BT 3 Tr /F2 12 Tf 72 600 Td (scan text) Tj ET\n";
    let content = heading_and(&marked("P", 1, invisible));
    assert_eq!(
        problem_ids(&tagged(
            &content,
            vec![leaf("H1", 0, &[0]), leaf("P", 0, &[1])]
        )),
        set(&[])
    );
}

#[test]
fn text_that_cannot_become_characters_is_reported() {
    let content = heading_and(&marked("P", 1, &text("F5", 12.0, 72.0, 140.0, "abc")));
    let found = problem_ids(&tagged(
        &content,
        vec![leaf("H1", 0, &[0]), leaf("P", 0, &[1])],
    ));
    assert!(found.contains("text-not-unicode"), "{found:?}");
}

// ---- the document as a whole ------------------------------------------------------------------------

fn long_document(bookmarks: bool) -> Document {
    let mut builder = Builder::new();
    let mut tags = Vec::new();
    for page in 0..10 {
        builder.page(&marked("H1", 0, &text("F1", 18.0, 72.0, 80.0, "Chapter")));
        tags.push(leaf("H1", page, &[0]));
    }
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    add_tags(&mut doc, &pages, tags);
    add_metadata(&mut doc, "en", "Long");
    if bookmarks {
        let root = doc.new_object_id();
        let mut entry = Dictionary::new();
        entry.set("Title", string("Chapter"));
        entry.set("Parent", Object::Reference(root));
        entry.set(
            "Dest",
            Object::Array(vec![Object::Reference(pages[0]), name("Fit")]),
        );
        let entry = doc.add_object(entry);
        let mut outlines = Dictionary::new();
        outlines.set("First", Object::Reference(entry));
        outlines.set("Last", Object::Reference(entry));
        outlines.set("Count", Object::Integer(1));
        doc.objects.insert(root, Object::Dictionary(outlines));
        doc.catalog_mut()
            .unwrap()
            .set("Outlines", Object::Reference(root));
    }
    doc
}

#[test]
fn a_long_document_needs_bookmarks() {
    let report = check(&long_document(false));
    assert_eq!(problems(&report), set(&["no-bookmarks"]));
    assert_eq!(
        report.with_id("no-bookmarks").next().unwrap().fix,
        Some(FixKind::AddBookmarksFromHeadings)
    );
    assert_eq!(problem_ids(&long_document(true)), set(&[]));
}

#[test]
fn page_labels_that_skip_the_first_page_are_reported() {
    let mut doc = good_document();
    let mut style = Dictionary::new();
    style.set("S", name("D"));
    let mut labels = Dictionary::new();
    labels.set(
        "Nums",
        Object::Array(vec![Object::Integer(3), Object::Dictionary(style)]),
    );
    doc.catalog_mut()
        .unwrap()
        .set("PageLabels", Object::Dictionary(labels));
    assert_eq!(problem_ids(&doc), set(&["page-labels-invalid"]));
}

#[test]
fn scripts_are_listed_for_a_check_by_hand() {
    let mut doc = good_document();
    let mut action = Dictionary::new();
    action.set("S", name("JavaScript"));
    action.set("JS", string("app.alert('hi')"));
    doc.catalog_mut()
        .unwrap()
        .set("OpenAction", Object::Dictionary(action));
    let report = check(&doc);
    assert_eq!(problems(&report), set(&[]));
    let finding = report.with_id("scripted-or-moving-content").next().unwrap();
    assert_eq!(finding.severity, Severity::CheckByHand);
}

#[test]
fn a_scanned_page_points_to_text_recognition() {
    let mut builder = Builder::new();
    builder.page(&picture(0.0, 0.0, WIDTH, HEIGHT));
    let mut doc = builder.build();
    add_metadata(&mut doc, "en", "Scan");
    let report = check(&doc);
    let finding = report.with_id("image-only-page").next().unwrap();
    assert_eq!(finding.fix, Some(FixKind::RecogniseText { pages: vec![0] }));
    assert!(finding.details.starts_with("Page 1"));
}

#[test]
fn every_finding_explains_itself_in_plain_sentences() {
    let report = check(&figure_document(leaf("Figure", 0, &[1])));
    for finding in &report.findings {
        assert!(!finding.title.is_empty());
        assert!(finding.explanation.ends_with('.'));
        assert!(finding.why_it_matters.ends_with('.'));
        assert!(!finding.fix_description.is_empty());
        assert!(!finding.pdfua.is_empty());
    }
}

#[test]
fn the_report_crosses_to_the_ui_as_camel_case_json() {
    let report = check(&figure_document(leaf("Figure", 0, &[1])));
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"whyItMatters\""));
    assert!(json.contains("\"where\":{\"page\":0"));
    assert!(json.contains("\"severity\":\"mustFix\""));
    assert!(json.contains("\"kind\":\"setAltText\""));
}
