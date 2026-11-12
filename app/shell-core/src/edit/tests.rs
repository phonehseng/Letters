//! the edit helpers, and letters-edit run through an edit session the way
//! the app runs it.

use std::path::{Path, PathBuf};

use letters_edit::{page_objects, replace_text, EditError, TextEditOptions};
use lopdf::{dictionary, Document, Object, Stream};

use super::block_matching;
use super::fit::{balance_lines, replace_fitting, shrink_share, SMALLEST_SHARE};
use super::fonts::{display_font, object_id, FontFormat};
use super::pictures::{check_picture, picture_kind, PictureKind};
use crate::session::{ApplyError, EditSession};
use crate::test_pdf::temp_dir;

/// a letter page with a two line paragraph in 12 point helvetica.
fn text_document() -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let content = b"BT /F1 12 Tf 72 700 Td (The quick brown fox jumps) Tj 0 -14.4 Td (over the lazy dog.) Tj ET".to_vec();
    let content_id = doc.add_object(Stream::new(dictionary! {}, content));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc
}

fn show(path: &Path) -> Result<PathBuf, String> {
    Ok(path.to_path_buf())
}

#[test]
fn words_are_shared_out_over_the_lines() {
    assert_eq!(
        balance_lines("one two three four", 2),
        vec!["one two", "three four"]
    );
    // fewer words than lines: one each, and no invented words
    assert_eq!(balance_lines("alone", 3), vec!["alone"]);
    assert_eq!(balance_lines("", 2), vec![""]);
    // a long word first still leaves something for the last line
    let lines = balance_lines("extraordinarily a b c", 2);
    assert_eq!(lines.len(), 2);
    assert!(!lines[1].is_empty(), "{lines:?}");
    // every word survives in order
    let text = "the cat sat on the mat and looked at the hat";
    assert_eq!(balance_lines(text, 3).join(" "), text);
}

#[test]
fn text_shrinks_only_as_far_as_it_must() {
    assert_eq!(shrink_share(200.0, 150.0), 1.0);
    let share = shrink_share(200.0, 250.0);
    assert!(share < 0.8 && share > 0.75, "{share}");
    assert_eq!(shrink_share(100.0, 1000.0), SMALLEST_SHARE);
}

#[test]
fn a_block_is_found_again_after_an_edit() {
    let mut doc = text_document();
    let before = page_objects(&doc, 0).unwrap();
    assert_eq!(before.text_blocks.len(), 1);
    let block = before.text_blocks[0].clone();
    let line = &block.lines[0];
    let result = replace_text(
        &mut doc,
        0,
        &line.id,
        "A slow red fox",
        &TextEditOptions::default(),
    )
    .unwrap();
    let found = block_matching(&result.objects, &block.rect).expect("the block is still there");
    assert_ne!(found.id, block.id, "ids change with every edit");
    assert!(found.text.starts_with("A slow red fox"), "{}", found.text);
}

#[test]
fn shrink_to_fit_keeps_the_paragraph_its_lines() {
    let mut doc = text_document();
    let block = page_objects(&doc, 0).unwrap().text_blocks[0].clone();
    let long = "The quick brown fox jumps over the lazy dog and then runs all the way home again before dinner.";

    // as a plain replace, the paragraph needs more lines
    let mut grown = doc.clone();
    let plain = replace_text(&mut grown, 0, &block.id, long, &TextEditOptions::default()).unwrap();
    assert!(plain.overflow.is_some(), "the long text should overflow");

    let fitted =
        replace_fitting(&mut doc, 0, &block.id, long, &TextEditOptions::default()).unwrap();
    assert!(fitted.overflow.is_none());
    let after = block_matching(&fitted.objects, &block.rect).unwrap();
    assert_eq!(after.lines.len(), 2, "{:?}", after.text);
    // smaller text, every word kept, and no line wider than before
    let size = after.lines[0].runs[0].size;
    assert!((12.0 * SMALLEST_SHARE..12.0).contains(&size), "{size}");
    assert_eq!(
        after.text.split_whitespace().collect::<Vec<_>>().join(" "),
        long
    );
    for line in &after.lines {
        assert!(
            line.rect.right() <= block.rect.right() + 1.0,
            "{:?}",
            line.rect
        );
    }
}

#[test]
fn a_failed_fit_leaves_the_document_alone() {
    let mut doc = text_document();
    let before = page_objects(&doc, 0).unwrap();
    let error = replace_fitting(
        &mut doc,
        0,
        "no-such-block",
        "text",
        &TextEditOptions::default(),
    );
    assert_eq!(error.unwrap_err(), EditError::UnknownObject);
    assert_eq!(page_objects(&doc, 0).unwrap(), before);
}

#[test]
fn font_ids_name_objects() {
    assert_eq!(object_id("font:12-0"), Some((12, 0)));
    assert_eq!(object_id("font:#3"), None);
    assert_eq!(object_id("font:none"), None);
}

#[test]
fn a_standard_font_has_no_file_to_hand_over() {
    let doc = text_document();
    let font = &page_objects(&doc, 0).unwrap().text_blocks[0].lines[0].runs[0].font;
    assert!(!font.embedded);
    assert_eq!(display_font(&doc, &font.id, "fox"), None);
}

#[test]
fn an_embedded_truetype_font_is_handed_over_when_it_draws_the_text() {
    let Ok(arial) = std::fs::read(r"C:\Windows\Fonts\arial.ttf") else {
        eprintln!("skipped: arial is not installed here");
        return;
    };
    // the page's font object becomes an embedded TrueType font
    let mut full = text_document();
    let font_object = {
        let objects = page_objects(&full, 0).unwrap();
        object_id(&objects.text_blocks[0].lines[0].runs[0].font.id).unwrap()
    };
    let file = full.add_object(Stream::new(dictionary! {}, arial.clone()));
    let descriptor = full.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "ArialMT",
        "Flags" => 32,
        "FontBBox" => vec![0.into(), 0.into(), 1000.into(), 1000.into()],
        "ItalicAngle" => 0,
        "Ascent" => 905,
        "Descent" => -212,
        "CapHeight" => 716,
        "StemV" => 80,
        "FontFile2" => file,
    });
    let font = dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => "ArialMT",
        "Encoding" => "WinAnsiEncoding",
        "FontDescriptor" => descriptor,
    };
    full.objects.insert(font_object, Object::Dictionary(font));
    let objects = page_objects(&full, 0).unwrap();
    let font = &objects.text_blocks[0].lines[0].runs[0].font;
    let handed = display_font(&full, &font.id, "The quick fox é").expect("arial draws these");
    assert_eq!(handed.format, FontFormat::TrueType);
    assert_eq!(handed.bytes, arial);
    // a character arial lacks: the editor uses an installed font instead
    assert_eq!(display_font(&full, &font.id, "\u{4e00}"), None);
}

#[test]
fn only_png_and_jpeg_go_onto_pages() {
    assert_eq!(
        picture_kind(b"\x89PNG\r\n\x1a\nrest"),
        Some(PictureKind::Png)
    );
    assert_eq!(
        picture_kind(&[0xff, 0xd8, 0xff, 0xe0]),
        Some(PictureKind::Jpeg)
    );
    assert_eq!(picture_kind(b"II*\0"), None);
    let refusal = check_picture(b"II*\0tiff", "scan.tif").unwrap_err();
    assert!(refusal.contains("New from Images"), "{refusal}");
}

#[test]
fn a_stale_id_refuses_without_an_undo_step() {
    let dir = temp_dir("edit-stale-id");
    let original = dir.join("essay.pdf");
    text_document().save(&original).unwrap();
    let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
    let line = {
        let doc = session.read_document().unwrap();
        page_objects(&doc, 0).unwrap().text_blocks[0].lines[0].clone()
    };
    // the first edit works and makes the old ids stale
    session
        .apply(
            |doc| replace_text(doc, 0, &line.id, "Hello", &TextEditOptions::default()).map(|_| ()),
            show,
        )
        .unwrap();
    let revision = session.revision();
    let again = session.apply(
        |doc| replace_text(doc, 0, &line.id, "Again", &TextEditOptions::default()).map(|_| ()),
        show,
    );
    assert!(matches!(
        again,
        Err(ApplyError::Edit(EditError::UnknownObject))
    ));
    assert_eq!(session.revision(), revision, "nothing changed");
    assert!(session.can_undo());
}

#[test]
fn the_donor_pool_is_kept_until_the_font_library_changes() {
    let cache = super::donors::DonorCache::new(None);
    let first = std::sync::Arc::new(letters_fonts::FontIndex::default());
    assert!(
        !cache.is_current(&first),
        "nothing is made before it is asked for"
    );
    cache.donors_for(first.clone());
    assert!(cache.is_current(&first));
    // a scan that finished, or a font added, makes a new index
    let rescanned = std::sync::Arc::new(letters_fonts::FontIndex::default());
    cache.donors_for(rescanned.clone());
    assert!(cache.is_current(&rescanned));
    assert!(!cache.is_current(&first));
}
