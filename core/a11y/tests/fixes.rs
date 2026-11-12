//! each fix: the finding it is for goes away, nothing else changes in the
//! report, the file still loads, and pages that were rewritten still draw
//! exactly the same pixels.

mod support;

use letters_a11y::{A11yError, ContentRef, FixKind, ObjectRef, TableHeaders, Target};
use lopdf::{Dictionary, Document, Object};
use support::*;

/// applies a fix and checks that exactly `fixed` disappeared from the
/// problems, and that the document loads again after saving.
fn assert_fixes(doc: &mut Document, fixed: &[&str], fix: impl FnOnce(&mut Document)) {
    let before = problem_ids(doc);
    for id in fixed {
        assert!(
            before.contains(*id),
            "{id} should be found before the fix: {before:?}"
        );
    }
    fix(doc);
    let reloaded = reload(doc);
    let after = problem_ids(&reloaded);
    let mut expected = before.clone();
    for id in fixed {
        expected.remove(*id);
    }
    assert_eq!(
        after, expected,
        "the fix should remove {fixed:?} and nothing else"
    );
}

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

fn element_of(report: &letters_a11y::Report, id: &str) -> ObjectRef {
    report.with_id(id).next().unwrap().location.element.unwrap()
}

// ---- document settings -------------------------------------------------------------------

#[test]
fn set_language_fixes_a_missing_language() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().remove(b"Lang");
    assert_fixes(&mut doc, &["missing-language"], |doc| {
        letters_a11y::set_language(doc, "en-GB").unwrap();
    });
}

#[test]
fn set_language_refuses_a_code_screen_readers_cannot_use() {
    let mut doc = good_document();
    let error = letters_a11y::set_language(&mut doc, "English").unwrap_err();
    assert_eq!(error, A11yError::InvalidLanguage("English".into()));
}

#[test]
fn set_title_fixes_a_missing_title_and_shows_it() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().remove(b"Metadata");
    doc.catalog_mut().unwrap().remove(b"ViewerPreferences");
    doc.trailer.remove(b"Info");
    assert_fixes(&mut doc, &["missing-title", "title-not-displayed"], |doc| {
        let result = letters_a11y::set_title(doc, "Rivers of Europe").unwrap();
        assert!(result.changes[0].contains("Rivers of Europe"));
    });
}

#[test]
fn set_title_keeps_unicode_titles() {
    let mut doc = good_document();
    letters_a11y::set_title(&mut doc, "Flüsse – Ελλάδα").unwrap();
    let info = doc.trailer.get(b"Info").unwrap().as_reference().unwrap();
    let title = doc.get_dictionary(info).unwrap().get(b"Title").unwrap();
    assert_eq!(lopdf::decode_text_string(title).unwrap(), "Flüsse – Ελλάδα");
}

#[test]
fn show_title_fixes_the_display_setting() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().remove(b"ViewerPreferences");
    assert_fixes(&mut doc, &["title-not-displayed"], |doc| {
        letters_a11y::show_title(doc).unwrap();
    });
}

#[test]
fn mark_as_tagged_sets_the_flag() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().remove(b"MarkInfo");
    assert_fixes(&mut doc, &["tagged-flag-missing"], |doc| {
        letters_a11y::mark_as_tagged(doc).unwrap();
    });
}

// ---- pictures ----------------------------------------------------------------------------------

fn figure_without_alt() -> Document {
    let content = heading_and(&marked("Figure", 1, &picture(72.0, 200.0, 200.0, 150.0)));
    tagged(&content, vec![leaf("H1", 0, &[0]), leaf("Figure", 0, &[1])])
}

fn untagged_picture() -> Document {
    tagged(
        &heading_and(&picture(72.0, 200.0, 200.0, 150.0)),
        vec![leaf("H1", 0, &[0])],
    )
}

#[test]
fn alt_text_on_a_figure_fixes_it() {
    let mut doc = figure_without_alt();
    let figure = element_of(&check(&doc), "missing-alt-text");
    assert_fixes(&mut doc, &["missing-alt-text"], |doc| {
        letters_a11y::set_alt_text(
            doc,
            Target::Element { element: figure },
            "Three coloured squares",
        )
        .unwrap();
    });
    assert!(check(&doc).has("check-alt-text-meaningful"));
}

#[test]
fn alt_text_for_an_untagged_picture_tags_it_as_a_figure() {
    let mut doc = untagged_picture();
    let original = doc.clone();
    let Some(FixKind::SetAltText { target }) = check(&doc)
        .with_id("image-not-tagged")
        .next()
        .unwrap()
        .fix
        .clone()
    else {
        panic!("an alt text fix was expected");
    };
    assert_fixes(&mut doc, &["image-not-tagged"], |doc| {
        letters_a11y::set_alt_text(doc, target, "Three coloured squares").unwrap();
    });
    assert_same_pixels(&original, &doc, "alt-untagged-picture");
}

#[test]
fn empty_alt_text_is_refused() {
    let mut doc = figure_without_alt();
    let figure = element_of(&check(&doc), "missing-alt-text");
    let error = letters_a11y::set_alt_text(&mut doc, Target::Element { element: figure }, "  ")
        .unwrap_err();
    assert!(matches!(error, A11yError::InvalidInput(_)));
}

#[test]
fn marking_a_figure_as_decoration_removes_it_from_the_tags() {
    let mut doc = figure_without_alt();
    let original = doc.clone();
    let figure = element_of(&check(&doc), "missing-alt-text");
    assert_fixes(&mut doc, &["missing-alt-text"], |doc| {
        letters_a11y::mark_as_artifact(doc, Target::Element { element: figure }).unwrap();
    });
    assert!(!doc.objects.contains_key(&figure.id()));
    assert_same_pixels(&original, &doc, "artifact-figure");
}

#[test]
fn marking_an_untagged_picture_as_decoration() {
    let mut doc = untagged_picture();
    let original = doc.clone();
    let image = ContentRef {
        page: 0,
        op_index: content_index(&doc, "Do"),
    };
    assert_fixes(&mut doc, &["image-not-tagged"], |doc| {
        letters_a11y::mark_as_artifact(doc, Target::Image { image }).unwrap();
    });
    assert_same_pixels(&original, &doc, "artifact-picture");
}

/// the index of the first operator with this name on the first page.
fn content_index(doc: &Document, operator: &str) -> usize {
    let page = doc.get_pages()[&1];
    let content = lopdf::content::Content::decode(&doc.get_page_content(page)).unwrap();
    content
        .operations
        .iter()
        .position(|op| op.operator == operator)
        .unwrap()
}

// ---- headings -----------------------------------------------------------------------------------

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
fn the_heading_ladder_keeps_relative_order() {
    let doc = headings(&[1, 3, 3, 2, 4]);
    let changes = letters_a11y::propose_heading_levels(&doc).unwrap();
    let moves: Vec<(u8, u8)> = changes.iter().map(|c| (c.from, c.to)).collect();
    // 1 3 3 2 4 becomes 1 2 2 2 3
    assert_eq!(moves, vec![(3, 2), (3, 2), (4, 3)]);
}

#[test]
fn fixing_heading_levels_removes_skips() {
    let mut doc = headings(&[1, 3]);
    assert_fixes(&mut doc, &["heading-level-skipped"], |doc| {
        letters_a11y::fix_heading_levels(doc).unwrap();
    });
}

#[test]
fn fixing_heading_levels_makes_the_first_one_h1() {
    let mut doc = headings(&[2, 3]);
    assert_fixes(&mut doc, &["first-heading-not-h1"], |doc| {
        letters_a11y::fix_heading_levels(doc).unwrap();
    });
}

#[test]
fn setting_one_heading_level() {
    let mut doc = headings(&[1, 3]);
    let skipped = element_of(&check(&doc), "heading-level-skipped");
    assert_fixes(&mut doc, &["heading-level-skipped"], |doc| {
        letters_a11y::set_heading_level(doc, skipped, 2).unwrap();
    });
    assert!(letters_a11y::set_heading_level(&mut doc, skipped, 7).is_err());
}

#[test]
fn bookmarks_are_made_from_headings() {
    let mut builder = Builder::new();
    let mut tags = Vec::new();
    for page in 0..10 {
        let content = format!(
            "{}{}",
            marked(
                "H1",
                0,
                &text("F1", 18.0, 72.0, 80.0, &format!("Chapter {page}"))
            ),
            marked("H2", 1, &text("F1", 14.0, 72.0, 120.0, "Section"))
        );
        builder.page(&content);
        tags.push(leaf("H1", page, &[0]));
        tags.push(leaf("H2", page, &[1]));
    }
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    add_tags(&mut doc, &pages, tags);
    add_metadata(&mut doc, "en", "Long");
    assert_fixes(&mut doc, &["no-bookmarks"], |doc| {
        let result = letters_a11y::add_bookmarks_from_headings(doc).unwrap();
        assert!(result.changes[0].contains("20 bookmarks"));
    });
    // a second time is refused rather than doubling them
    assert!(letters_a11y::add_bookmarks_from_headings(&mut doc).is_err());
}

// ---- tables ----------------------------------------------------------------------------------------

#[test]
fn table_headers_get_header_cells_with_a_scope() {
    let mut content = heading_and("");
    let mut rows = Vec::new();
    let mut mcid = 1;
    for row in 0..3 {
        let mut cells = Vec::new();
        for column in 0..2 {
            content.push_str(&marked(
                "TD",
                mcid,
                &text(
                    "F1",
                    12.0,
                    72.0 + column as f32 * 150.0,
                    140.0 + row as f32 * 30.0,
                    "Cell",
                ),
            ));
            cells.push(leaf("TD", 0, &[mcid]));
            mcid += 1;
        }
        rows.push(tag("TR", cells));
    }
    let mut doc = tagged(&content, vec![leaf("H1", 0, &[0]), tag("Table", rows)]);
    let Some(FixKind::SetTableHeaders { table, .. }) = check(&doc)
        .with_id("table-missing-headers")
        .next()
        .unwrap()
        .fix
        .clone()
    else {
        panic!("a table headers fix was expected");
    };
    assert_fixes(&mut doc, &["table-missing-headers"], |doc| {
        letters_a11y::set_table_headers(
            doc,
            table,
            TableHeaders {
                header_rows: 1,
                header_columns: 1,
            },
        )
        .unwrap();
    });
    let error = letters_a11y::set_table_headers(
        &mut doc,
        table,
        TableHeaders {
            header_rows: 9,
            header_columns: 0,
        },
    );
    assert!(error.is_err());
}

// ---- reading order ----------------------------------------------------------------------------------

#[test]
fn the_proposed_reading_order_fixes_a_two_column_page() {
    let mut content = marked("H1", 0, &text("F1", 24.0, 72.0, 80.0, "Two columns"));
    let mut tags = vec![leaf("H1", 0, &[0])];
    for row in 0..3 {
        for column in 0..2 {
            let mcid = 1 + column * 3 + row;
            content.push_str(&marked(
                "P",
                mcid,
                &text(
                    "F1",
                    12.0,
                    72.0 + column as f32 * 250.0,
                    140.0 + row as f32 * 60.0,
                    "Words",
                ),
            ));
        }
    }
    // tagged row by row: 1 4 2 5 3 6
    for row in 0..3 {
        for column in 0..2 {
            tags.push(leaf("P", 0, &[1 + column * 3 + row]));
        }
    }
    let mut doc = tagged(&content, tags);
    let original = doc.clone();
    let proposal = letters_a11y::auto_reading_order(&doc, 0).unwrap();
    assert_eq!(proposal.len(), 7);
    assert_fixes(&mut doc, &["reading-order-mismatch"], |doc| {
        letters_a11y::set_reading_order(doc, 0, &proposal).unwrap();
    });
    assert_same_pixels(&original, &doc, "reading-order");
}

#[test]
fn a_reading_order_listing_a_tag_twice_is_refused() {
    let doc = headings(&[1, 2]);
    let order = letters_a11y::auto_reading_order(&doc, 0).unwrap();
    let mut doubled = order.clone();
    doubled.push(order[0]);
    let mut doc = doc;
    assert!(letters_a11y::set_reading_order(&mut doc, 0, &doubled).is_err());
}

// ---- links, notes and fields ----------------------------------------------------------------------------

fn link_document(words: &str, contents: Option<&str>, tag_it: bool) -> Document {
    let mut builder = Builder::new();
    builder.page(&heading_and(&marked(
        "P",
        1,
        &text("F1", 12.0, 72.0, 140.0, words),
    )));
    let mut link = Dictionary::new();
    link.set("Type", name("Annot"));
    link.set("Subtype", name("Link"));
    link.set(
        "Rect",
        numbers(&[70.0, HEIGHT - 145.0, 300.0, HEIGHT - 128.0]),
    );
    if let Some(contents) = contents {
        link.set("Contents", string(contents));
    }
    let annotation = builder.annotation(0, link);
    builder
        .doc
        .get_dictionary_mut(builder.pages[0])
        .unwrap()
        .set("Tabs", name("S"));
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

fn annotation_of(doc: &Document, id: &str) -> ObjectRef {
    check(doc)
        .with_id(id)
        .next()
        .unwrap()
        .location
        .annotation
        .unwrap()
}

#[test]
fn link_text_fixes_a_missing_description() {
    let mut doc = link_document("River data for 2024", None, true);
    let link = annotation_of(&doc, "link-missing-description");
    assert_fixes(&mut doc, &["link-missing-description"], |doc| {
        letters_a11y::add_link_text(doc, link, "River data for 2024").unwrap();
    });
}

#[test]
fn link_text_fixes_vague_words() {
    let mut doc = link_document("Click here", Some("Click here"), true);
    let link = annotation_of(&doc, "link-text-not-descriptive");
    assert_fixes(&mut doc, &["link-text-not-descriptive"], |doc| {
        letters_a11y::add_link_text(doc, link, "River data for 2024").unwrap();
    });
}

#[test]
fn untagged_links_and_fields_get_tags() {
    let mut doc = link_document("River data for 2024", Some("River data"), false);
    assert_fixes(&mut doc, &["link-not-tagged"], |doc| {
        let result = letters_a11y::tag_annotations(doc).unwrap();
        assert!(
            result.changes[0].contains("1 links"),
            "{:?}",
            result.changes
        );
    });
}

#[test]
fn tab_order_is_set_on_pages_with_links() {
    let mut doc = link_document("River data for 2024", Some("River data"), true);
    doc.get_dictionary_mut(doc.get_pages()[&1])
        .unwrap()
        .remove(b"Tabs");
    assert_fixes(&mut doc, &["tab-order-not-structure"], |doc| {
        letters_a11y::set_tab_order(doc).unwrap();
    });
}

fn note_document() -> Document {
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
    doc
}

#[test]
fn notes_get_a_description_and_a_tag() {
    let mut doc = note_document();
    let note = annotation_of(&doc, "annotation-missing-contents");
    assert_fixes(&mut doc, &["annotation-missing-contents"], |doc| {
        letters_a11y::set_annotation_contents(doc, note, "Box around the key figure").unwrap();
    });
    assert_fixes(&mut doc, &["annotation-not-tagged"], |doc| {
        letters_a11y::tag_annotations(doc).unwrap();
    });
}

fn field_document() -> Document {
    let mut builder = Builder::new();
    builder.page(&heading_and(""));
    let mut widget = Dictionary::new();
    widget.set("Type", name("Annot"));
    widget.set("Subtype", name("Widget"));
    widget.set("FT", name("Tx"));
    widget.set("T", string("first_name"));
    widget.set("Rect", numbers(&[72.0, 600.0, 272.0, 620.0]));
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
    add_tags(&mut doc, &pages, vec![leaf("H1", 0, &[0])]);
    add_metadata(&mut doc, "en", "Form");
    doc
}

#[test]
fn fields_get_a_tooltip_and_a_tag() {
    let mut doc = field_document();
    let Some(FixKind::SetFieldTooltip { field, suggested }) = check(&doc)
        .with_id("field-missing-tooltip")
        .next()
        .unwrap()
        .fix
        .clone()
    else {
        panic!("a tooltip fix was expected");
    };
    assert_fixes(&mut doc, &["field-missing-tooltip"], |doc| {
        letters_a11y::set_field_tooltip(doc, field, &suggested.unwrap()).unwrap();
    });
    assert_fixes(&mut doc, &["field-not-tagged"], |doc| {
        letters_a11y::tag_annotations(doc).unwrap();
    });
}

// ---- content ----------------------------------------------------------------------------------------------

#[test]
fn untagged_content_gets_tagged_without_changing_the_page() {
    let content = heading_and(&format!(
        "{}{}{}",
        text("F1", 12.0, 72.0, 140.0, "Forgotten words"),
        frame(60.0, 60.0, 480.0, 650.0),
        picture(72.0, 300.0, 100.0, 80.0)
    ));
    let mut doc = tagged(&content, vec![leaf("H1", 0, &[0])]);
    let original = doc.clone();
    assert_eq!(
        problem_ids(&doc),
        set(&["untagged-content", "image-not-tagged"])
    );
    letters_a11y::tag_untagged_content(&mut doc, 0).unwrap();
    // the picture is now a figure, which needs alt text: the next question
    // the wizard asks. nothing else is left.
    assert_eq!(problem_ids(&reload(&doc)), set(&["missing-alt-text"]));
    assert_same_pixels(&original, &doc, "tag-untagged");
}

// ---- pdf/ua -------------------------------------------------------------------------------------------------

#[test]
fn pdfua_is_claimed_only_when_the_machine_checks_pass() {
    let mut bad = figure_without_alt();
    assert_eq!(
        letters_a11y::mark_pdfua(&mut bad, None).unwrap_err(),
        A11yError::NotReadyForPdfUa
    );

    let mut good = good_document();
    letters_a11y::mark_pdfua(&mut good, None).unwrap();
    let catalog = good.catalog().unwrap();
    let metadata = good
        .get_object(catalog.get(b"Metadata").unwrap().as_reference().unwrap())
        .unwrap();
    let packet = String::from_utf8(metadata.as_stream().unwrap().content.clone()).unwrap();
    assert!(packet.contains("<pdfuaid:part>1</pdfuaid:part>"));
    assert!(packet.contains("Rivers"), "the title stays in the metadata");
    assert_eq!(problem_ids(&reload(&good)), set(&[]));
}

#[test]
fn fixes_refuse_an_encrypted_document() {
    let mut doc = good_document();
    let encrypt = doc.add_object(Dictionary::new());
    doc.trailer.set("Encrypt", Object::Reference(encrypt));
    assert_eq!(
        letters_a11y::set_language(&mut doc, "en").unwrap_err(),
        A11yError::Locked
    );
    assert_eq!(
        letters_a11y::auto_tag(&mut doc, &Default::default()).unwrap_err(),
        A11yError::Locked
    );
}
