//! the preview must say exactly what redaction will do: the same counts,
//! the same notes about things removed whole, and the same warnings about
//! places that still show the removed text.

mod support;

use letters_redact::lopdf::Document;
use letters_redact::{
    encrypt, find_text_areas, open, preview, preview_with_options, redact, save_clean, FindOptions,
    LockOptions, PageRect, PermissionOptions, RedactOptions, RedactionArea, RedactionReport,
    TextQuery,
};
use support::*;

const SECRET: &str = "SECRETWORD";

/// the secret on page 1 and in every place a warning is about: the
/// title, a bookmark, a comment and the text of page 2. also an image and
/// a form field used on both pages, an xfa copy, an attachment, an image
/// that can't be decoded and one whose data is broken.
fn warnings_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let first = pdf.reserve();
    let second = pdf.reserve();
    let pixels: Vec<u8> = (0..300).map(|i| (i % 251) as u8).collect();
    let shared = pdf.add_flate_stream(
        "/Type /XObject /Subtype /Image /Width 10 /Height 10 /ColorSpace /DeviceRGB /BitsPerComponent 8",
        &pixels,
    );
    let jpx = pdf.add_stream(
        "/Type /XObject /Subtype /Image /Width 10 /Height 10 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /JPXDecode",
        b"not jpeg 2000",
    );
    // only decoding it shows that this jpeg is cmyk, which is removed whole
    let mut cmyk = Vec::new();
    jpeg_encoder::Encoder::new(&mut cmyk, 90)
        .encode(&[30u8; 10 * 10 * 4], 10, 10, jpeg_encoder::ColorType::Cmyk)
        .unwrap();
    let broken = pdf.add_stream(
        "/Type /XObject /Subtype /Image /Width 10 /Height 10 /ColorSpace /DeviceCMYK /BitsPerComponent 8 /Filter /DCTDecode",
        &cmyk,
    );
    let field = pdf.reserve();
    let widget_one = pdf.add(format!(
        "<< /Type /Annot /Subtype /Widget /Parent {field} 0 R /Rect [72 300 200 320] /P {first} 0 R >>"
    ));
    let widget_two = pdf.add(format!(
        "<< /Type /Annot /Subtype /Widget /Parent {field} 0 R /Rect [72 300 200 320] /P {second} 0 R >>"
    ));
    pdf.set(
        field,
        format!("<< /FT /Tx /T (id) /V (12345) /Kids [{widget_one} 0 R {widget_two} 0 R] >>"),
    );
    let comment = pdf.add(format!(
        "<< /Type /Annot /Subtype /Text /Rect [500 50 520 70] /Contents (about {SECRET}) >>"
    ));
    let bookmark = pdf.reserve();
    let outlines = pdf.add(format!(
        "<< /Type /Outlines /First {bookmark} 0 R /Last {bookmark} 0 R /Count 1 >>"
    ));
    pdf.set(
        bookmark,
        format!("<< /Title (Chapter {SECRET}) /Parent {outlines} 0 R /Dest [{first} 0 R /Fit] >>"),
    );
    let attached = pdf.add_stream("/Type /EmbeddedFile", b"an attached file");
    let filespec = pdf.add(format!(
        "<< /Type /Filespec /F (a.txt) /EF << /F {attached} 0 R >> >>"
    ));
    let info = pdf.add(format!("<< /Title (All about {SECRET}) >>"));
    let resources = format!(
        "/Font << /F1 {font} 0 R >> /XObject << /Shared {shared} 0 R /Jpx {jpx} 0 R /Broken {broken} 0 R >>"
    );
    let first_content = pdf.add_stream(
        "",
        format!(
            "BT /F1 12 Tf 72 700 Td ({SECRET}) Tj ET\n\
             q 100 0 0 100 72 400 cm /Shared Do Q\n\
             q 100 0 0 100 300 400 cm /Jpx Do Q\n\
             q 100 0 0 100 300 200 cm /Broken Do Q"
        )
        .as_bytes(),
    );
    let second_content = pdf.add_stream(
        "",
        format!(
            "BT /F1 12 Tf 72 700 Td ({SECRET} again) Tj ET\nq 100 0 0 100 72 400 cm /Shared Do Q"
        )
        .as_bytes(),
    );
    pdf.set(first, format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] /Resources << {resources} >> /Contents {first_content} 0 R /Annots [{widget_one} 0 R] >>"
    ));
    pdf.set(second, format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] /Resources << {resources} >> /Contents {second_content} 0 R /Annots [{widget_two} 0 R {comment} 0 R] >>"
    ));
    pdf.set(
        tree,
        format!("<< /Type /Pages /Count 2 /Kids [{first} 0 R {second} 0 R] >>"),
    );
    pdf.set(catalog, format!(
        "<< /Type /Catalog /Pages {tree} 0 R /Outlines {outlines} 0 R /AcroForm << /Fields [{field} 0 R] /XFA (xfa data) >> \
         /Names << /EmbeddedFiles << /Names [(a.txt) {filespec} 0 R] >> >> >>"
    ));
    pdf.finish(catalog, &format!("/Info {info} 0 R"))
}

/// the secret on page 1, the left half of each image, and the widget.
fn areas(doc: &Document) -> Vec<RedactionArea> {
    let query = TextQuery::Phrase {
        text: SECRET.into(),
        match_case: true,
    };
    let mut rects: Vec<PageRect> = find_text_areas(doc, &query, &FindOptions::default())
        .unwrap()
        .into_iter()
        .filter(|found| found.page == 0)
        .flat_map(|found| found.rects)
        .collect();
    rects.push(PageRect::new(72.0, 292.0, 50.0, 100.0));
    rects.push(PageRect::new(300.0, 292.0, 50.0, 100.0));
    rects.push(PageRect::new(300.0, 492.0, 50.0, 100.0));
    rects.push(PageRect::new(80.0, 474.0, 20.0, 15.0));
    vec![RedactionArea { page: 0, rects }]
}

/// the report a real redaction of a copy gives.
fn real_report(
    doc: &Document,
    areas: &[RedactionArea],
    options: &RedactOptions,
) -> RedactionReport {
    let mut copy = doc.clone();
    redact(&mut copy, areas, options).unwrap()
}

#[test]
fn the_preview_report_is_the_report_redaction_gives() {
    let doc = Document::load_mem(&warnings_pdf()).unwrap();
    let areas = areas(&doc);
    let before = doc.objects.clone();
    let preview = preview(&doc, &areas).unwrap();
    assert_eq!(doc.objects, before, "the preview changed the document");
    let report = real_report(&doc, &areas, &RedactOptions::default());
    assert_eq!(preview.report, report);

    // and every kind of warning is really there
    let warnings = &preview.report.warnings;
    let has = |text: &str| warnings.iter().any(|warning| warning.contains(text));
    assert!(has("document’s Title"), "{warnings:?}");
    assert!(has("in a bookmark"), "{warnings:?}");
    assert!(has("on page 2"), "{warnings:?}");
    assert!(has("comment or form field on page 2"), "{warnings:?}");
    assert!(
        has("image you redacted is also used on page 2"),
        "{warnings:?}"
    );
    assert!(has("also shown somewhere else"), "{warnings:?}");
    assert!(has("XFA"), "{warnings:?}");
    assert!(has("attached files"), "{warnings:?}");

    // the notes about images removed whole, including the one only
    // decoding finds out about
    let wholesale = &preview.report.pages[0].removed_wholesale;
    assert!(
        wholesale.iter().any(|note| note.contains("JPEG 2000")),
        "{wholesale:?}"
    );
    assert!(
        wholesale.iter().any(|note| note.contains("CMYK JPEG")),
        "{wholesale:?}"
    );
}

#[test]
fn the_preview_follows_the_options() {
    let doc = Document::load_mem(&warnings_pdf()).unwrap();
    let areas = areas(&doc);
    let options = RedactOptions {
        clip_crossing_paths: true,
        overlay_text: Some("Redacted".into()),
        ..Default::default()
    };
    let preview = preview_with_options(&doc, &areas, &options).unwrap();
    assert_eq!(preview.report, real_report(&doc, &areas, &options));
}

#[test]
fn the_preview_warns_about_permissions_that_will_not_be_kept() {
    let mut doc = Document::load_mem(&warnings_pdf()).unwrap();
    let lock = LockOptions {
        owner_password: "owner".into(),
        permissions: PermissionOptions {
            copying: false,
            ..Default::default()
        },
        ..Default::default()
    };
    encrypt(&mut doc, &lock).unwrap();
    let doc = open(&save_clean(&mut doc).unwrap()).unwrap();
    let areas = areas(&doc);
    let preview = preview(&doc, &areas).unwrap();
    assert!(preview
        .report
        .warnings
        .iter()
        .any(|w| w.contains("permission settings")));
    assert_eq!(
        preview.report,
        real_report(&doc, &areas, &RedactOptions::default())
    );
}
