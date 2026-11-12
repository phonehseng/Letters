//! vector paths, annotations, links and form fields under a box.

mod support;

use letters_redact::lopdf::Document;
use letters_redact::{preview, redact, save_clean, PageRect, RedactOptions, RedactionArea};
use support::*;

const SECRET: &str = "SECRETVALUE";

/// a box around (100..200, 280..340) in page space, given in visible
/// coordinates (y measured down from the top of a 792 point page).
fn signature_box() -> Vec<RedactionArea> {
    vec![RedactionArea {
        page: 0,
        rects: vec![PageRect::new(100.0, 452.0, 100.0, 60.0)],
    }]
}

/// a hand drawn looking path whose odd coordinates are the "secret",
/// a filled background bar crossing the box, and a line far away.
fn paths_pdf() -> Vec<u8> {
    let content = "0.8 g 50 300 500 20 re f\n\
                   0 G 2 w 105 300 m 110.375 320.125 l 120.5 300.25 l 130.625 320.875 l 140.75 301.5 l 230.5 315 l S\n\
                   1 w 50 100 m 333.333 100 l S";
    let mut pdf = Pdf::new();
    let (catalog, _) = add_pages(&mut pdf, vec![PageSpec::new("", content)], "");
    pdf.finish(catalog, "")
}

#[test]
fn a_path_crossing_the_box_is_removed_whole_and_a_bar_is_cut() {
    let dir = temp_dir("paths");
    let worker = start_worker();
    let original = paths_pdf();
    let mut doc = Document::load_mem(&original).unwrap();
    let report = redact(&mut doc, &signature_box(), &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].paths_removed, 1);
    assert_eq!(report.pages[0].paths_cut, 1);
    let saved = save_clean(&mut doc).unwrap();
    for coordinate in ["110.375", "120.5", "130.625", "140.75", "230.5"] {
        assert_not_in_bytes(&saved, coordinate);
    }
    // the far away line is still there
    let inflated = String::from_utf8_lossy(&saved).into_owned()
        + &Document::load_mem(&saved)
            .unwrap()
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .filter_map(|s| s.decompressed_content().ok())
            .map(|c| String::from_utf8_lossy(&c).into_owned())
            .collect::<String>();
    assert!(inflated.contains("333.333"));

    let before = worker
        .open(&write(&dir, "before.pdf", &original), None)
        .unwrap();
    let after = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    let old = render_page(&worker, before.id, 0, 1.0, (612.0, 792.0));
    let new = render_page(&worker, after.id, 0, 1.0, (612.0, 792.0));
    // the bar outside the box is untouched, inside it is black
    for x in (55..95).chain(205..545) {
        assert_eq!(
            new.pixel(x, 482),
            old.pixel(x, 482),
            "bar pixel {x} changed"
        );
    }
    for y in 455..510 {
        for x in 103..197 {
            assert_eq!(new.pixel(x, y), [0, 0, 0]);
        }
    }
}

#[test]
fn with_clipping_chosen_the_path_stays_but_paints_nothing_in_the_box() {
    let dir = temp_dir("paths-clipped");
    let worker = start_worker();
    let mut doc = Document::load_mem(&paths_pdf()).unwrap();
    let options = RedactOptions {
        clip_crossing_paths: true,
        fill: letters_redact::Rgb { r: 255, g: 0, b: 0 },
        ..Default::default()
    };
    let report = redact(&mut doc, &signature_box(), &options).unwrap();
    assert_eq!(report.pages[0].paths_clipped, 1);
    let saved = save_clean(&mut doc).unwrap();
    let after = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    let new = render_page(&worker, after.id, 0, 1.0, (612.0, 792.0));
    for y in 455..510 {
        for x in 103..197 {
            assert_eq!(new.pixel(x, y), [255, 0, 0]);
        }
    }
}

/// a comment with a popup, a link, and a filled in text field inside the
/// box; another comment outside it.
fn annotations_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let helvetica = add_helvetica(&mut pdf);
    let page = pdf.reserve();
    let comment = pdf.reserve();
    let popup = pdf.add(format!(
        "<< /Type /Annot /Subtype /Popup /Rect [300 300 400 400] /Parent {comment} 0 R >>"
    ));
    pdf.set(comment, format!("<< /Type /Annot /Subtype /Text /Rect [110 290 130 310] /Contents ({SECRET} note) /Popup {popup} 0 R /T (Reviewer) >>"));
    let link = pdf.add("<< /Type /Annot /Subtype /Link /Rect [150 300 190 315] /A << /S /URI /URI (https://example.com/SECRETVALUE) >> >>");
    let appearance = pdf.add_stream(
        &format!("/Type /XObject /Subtype /Form /BBox [0 0 80 20] /Resources << /Font << /F1 {helvetica} 0 R >> >>"),
        format!("/Tx BMC BT /F1 10 Tf 2 5 Td ({SECRET}) Tj ET EMC").as_bytes(),
    );
    let field = pdf.add(format!(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (StudentId) /V ({SECRET}) /DV ({SECRET}) /Rect [110 320 190 335] /P {page} 0 R /AP << /N {appearance} 0 R >> >>"
    ));
    let other =
        pdf.add("<< /Type /Annot /Subtype /Text /Rect [400 600 420 620] /Contents (KEEPNOTE) >>");
    let tree = pdf.reserve();
    let catalog = pdf.add(format!("<< /Type /Catalog /Pages {tree} 0 R /AcroForm << /Fields [{field} 0 R] /DA (/Helv 0 Tf 0 g) >> >>"));
    let content = pdf.add_stream("", b"");
    pdf.set(page, format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] /Resources << >> /Contents {content} 0 R /Annots [{comment} 0 R {popup} 0 R {link} 0 R {field} 0 R {other} 0 R] >>"
    ));
    pdf.set(
        tree,
        format!("<< /Type /Pages /Count 1 /Kids [{page} 0 R] >>"),
    );
    pdf.finish(catalog, "")
}

#[test]
fn annotations_links_and_field_values_under_the_box_are_removed() {
    let dir = temp_dir("annotations");
    let worker = start_worker();
    let mut doc = Document::load_mem(&annotations_pdf()).unwrap();
    let preview = preview(&doc, &signature_box()).unwrap();
    assert_eq!(preview.areas[0].form_values, vec![SECRET.to_owned()]);
    assert_eq!(
        preview.areas[0].annotations.len(),
        3,
        "{:?}",
        preview.areas[0].annotations
    );
    let report = redact(&mut doc, &signature_box(), &RedactOptions::default()).unwrap();
    assert_eq!(
        report.pages[0].annotations_removed, 4,
        "comment, popup, link and field"
    );
    assert_eq!(report.pages[0].form_values_removed, 1);
    let saved = save_clean(&mut doc).unwrap();
    assert_secret_gone(&worker, &dir, &saved, SECRET);
    assert!(
        saved.windows(8).any(|w| w == b"KEEPNOTE"),
        "the comment outside the box stays"
    );
    let doc = Document::load_mem(&saved).unwrap();
    let fields = doc
        .catalog()
        .unwrap()
        .get(b"AcroForm")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Fields")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(fields.is_empty(), "the field went with its only widget");
}
