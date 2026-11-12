//! removing hidden information, and the whole job together: a file with
//! the secret hidden in every place we know of, redacted and scrubbed,
//! has no trace of it left.

mod support;

use letters_redact::lopdf::Document;
use letters_redact::{
    find_text_areas, redact, save_clean, scrub, scrub_preview, FindOptions, RedactOptions,
    RedactionArea, ScrubItem, ScrubOptions, TextQuery,
};
use support::*;

const SECRET: &str = "SECRETWORD";

/// a file with the secret in the page text and in every hidden place:
/// document info, xmp metadata, two kinds of attachment, scripts, a
/// hidden layer (text and an image), private data, a thumbnail, a
/// comment, a form field, a link and an object nothing uses.
fn everything_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let helvetica = add_helvetica(&mut pdf);
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let page = pdf.reserve();

    let info = pdf.add(format!(
        "<< /Title (Notes about {SECRET}) /Author (Jane {SECRET}) /Producer (Some App) >>"
    ));
    let xmp = format!(
        "<?xpacket begin=\"\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:xmpMM=\"http://ns.adobe.com/xap/1.0/mm/\" xmpMM:DocumentID=\"uuid:1234\"><dc:title>{SECRET}</dc:title></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>"
    );
    let metadata = pdf.add_stream("/Type /Metadata /Subtype /XML", xmp.as_bytes());
    let attached = pdf.add_flate_stream(
        "/Type /EmbeddedFile",
        format!("my {SECRET} essay").as_bytes(),
    );
    let filespec = pdf.add(format!(
        "<< /Type /Filespec /F (essay.txt) /UF (essay.txt) /EF << /F {attached} 0 R >> >>"
    ));
    let note_file = pdf.add_stream("/Type /EmbeddedFile", format!("note {SECRET}").as_bytes());
    let note_spec = pdf.add(format!(
        "<< /Type /Filespec /F (note.txt) /EF << /F {note_file} 0 R >> >>"
    ));
    let script = pdf.add(format!(
        "<< /S /JavaScript /JS (app.alert\\('{SECRET}'\\)) >>"
    ));
    let names_script = pdf.add(format!("<< /S /JavaScript /JS (var x = '{SECRET}';) >>"));
    let layer = pdf.add("<< /Type /OCG /Name (Hidden notes) >>");
    let hidden_image = pdf.add_stream(
        &format!("/Type /XObject /Subtype /Image /Width 4 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /OC {layer} 0 R"),
        &SECRET.as_bytes()[..4],
    );
    let thumb = pdf.add_stream(
        "/Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8",
        SECRET.as_bytes(),
    );
    let comment = pdf.add(format!(
        "<< /Type /Annot /Subtype /Text /Rect [500 700 520 720] /Contents (remember {SECRET}) >>"
    ));
    let attachment_annot = pdf.add(format!(
        "<< /Type /Annot /Subtype /FileAttachment /Rect [500 650 520 670] /FS {note_spec} 0 R >>"
    ));
    let field_ap = pdf.add_stream(
        &format!("/Type /XObject /Subtype /Form /BBox [0 0 100 20] /Resources << /Font << /F1 {helvetica} 0 R >> >>"),
        format!("/Tx BMC BT /F1 10 Tf 2 5 Td ({SECRET}) Tj ET EMC").as_bytes(),
    );
    let field = pdf.add(format!(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V ({SECRET}) /Rect [300 100 400 120] /AP << /N {field_ap} 0 R >> /AA << /K << /S /JavaScript /JS ({SECRET}) >> >> >>"
    ));
    let link = pdf.add(format!("<< /Type /Annot /Subtype /Link /Rect [300 50 400 70] /A << /S /URI /URI (https://example.com/{SECRET}) >> >>"));
    // an object nothing points at
    pdf.add(format!("<< /Leftover ({SECRET}) >>"));

    let content = pdf.add_stream(
        "",
        format!(
            "BT /F1 12 Tf 72 700 Td (visible {SECRET} line) Tj ET\n\
             /OC /L1 BDC BT /F1 12 Tf 72 400 Td (hidden layer {SECRET}) Tj ET EMC\n\
             q 40 0 0 10 300 300 cm /HiddenImage Do Q"
        )
        .as_bytes(),
    );
    pdf.set(page, format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] /Contents {content} 0 R /Thumb {thumb} 0 R \
         /PieceInfo << /SomeApp << /Private ({SECRET}) /LastModified (D:2024) >> >> \
         /Resources << /Font << /F1 {helvetica} 0 R >> /Properties << /L1 {layer} 0 R >> /XObject << /HiddenImage {hidden_image} 0 R >> >> \
         /Annots [{comment} 0 R {attachment_annot} 0 R {field} 0 R {link} 0 R] >>"
    ));
    pdf.set(
        tree,
        format!("<< /Type /Pages /Count 1 /Kids [{page} 0 R] >>"),
    );
    pdf.set(catalog, format!(
        "<< /Type /Catalog /Pages {tree} 0 R /Metadata {metadata} 0 R /OpenAction {script} 0 R \
         /Names << /EmbeddedFiles << /Names [(essay.txt) {filespec} 0 R] >> /JavaScript << /Names [(init) {names_script} 0 R] >> >> \
         /OCProperties << /OCGs [{layer} 0 R] /D << /OFF [{layer} 0 R] /Order [{layer} 0 R] >> >> \
         /AcroForm << /Fields [{field} 0 R] >> /PieceInfo << /App << /Data ({SECRET}) >> >> >>"
    ));
    pdf.finish(catalog, &format!("/Info {info} 0 R /ID [<00112233445566778899AABBCCDDEEFF> <00112233445566778899AABBCCDDEEFF>]"))
}

/// boxes for the secret on the visible line only (the hidden layer's copy
/// is left for scrubbing to remove).
fn visible_secret(doc: &Document) -> Vec<RedactionArea> {
    let query = TextQuery::Phrase {
        text: SECRET.into(),
        match_case: true,
    };
    find_text_areas(doc, &query, &FindOptions::default())
        .unwrap()
        .into_iter()
        .filter(|found| found.rects[0].y < 200.0)
        .map(|found| RedactionArea {
            page: found.page,
            rects: found.rects,
        })
        .collect()
}

#[test]
fn redacting_and_scrubbing_leaves_no_trace_of_the_secret() {
    let dir = temp_dir("everything");
    let worker = start_worker();
    let mut doc = Document::load_mem(&everything_pdf()).unwrap();
    let areas = visible_secret(&doc);
    assert_eq!(areas.len(), 1);
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    assert!(
        report.warnings.iter().any(|w| w.contains("Title")),
        "the title is reported: {:?}",
        report.warnings
    );
    assert!(
        report.warnings.iter().any(|w| w.contains("attached")),
        "{:?}",
        report.warnings
    );
    let scrubbed = scrub(
        &mut doc,
        &ScrubOptions {
            items: ScrubItem::ALL.to_vec(),
        },
    )
    .unwrap();
    // redaction already took the metadata, thumbnail and private data; the
    // rest is scrubbing's job
    for item in [
        ScrubItem::HiddenLayers,
        ScrubItem::Attachments,
        ScrubItem::JavaScript,
        ScrubItem::Comments,
        ScrubItem::FormValues,
        ScrubItem::ExternalLinks,
    ] {
        let found = scrubbed
            .findings
            .iter()
            .find(|finding| finding.item == item)
            .unwrap();
        assert!(found.count > 0, "{item:?} found nothing");
    }
    let saved = save_clean(&mut doc).unwrap();
    assert_secret_gone(&worker, &dir, &saved, SECRET);
    let info = worker
        .open(&write(&dir, "final.pdf", &saved), None)
        .unwrap();
    assert_eq!(worker.search(info.id, "visible").unwrap().matches.len(), 1);
    assert!(!saved.windows(9).any(|w| w == b"uuid:1234"));
    assert!(
        !saved
            .windows(32)
            .any(|w| w == b"00112233445566778899AABBCCDDEEFF"),
        "the file id was replaced"
    );
}

#[test]
fn the_preview_lists_every_item_and_changes_nothing() {
    let doc = Document::load_mem(&everything_pdf()).unwrap();
    let before = doc.objects.clone();
    let preview = scrub_preview(&doc).unwrap();
    assert_eq!(doc.objects, before);
    let item = |kind: ScrubItem| {
        preview
            .findings
            .iter()
            .find(|finding| finding.item == kind)
            .unwrap()
    };
    assert!(item(ScrubItem::DocumentInfo)
        .details
        .iter()
        .any(|d| d.starts_with("Author:")));
    assert!(item(ScrubItem::Attachments)
        .details
        .contains(&"essay.txt".to_owned()));
    assert_eq!(
        item(ScrubItem::HiddenLayers).details,
        vec!["Hidden notes".to_owned()]
    );
    assert!(item(ScrubItem::JavaScript).count >= 3);
    assert_eq!(item(ScrubItem::Comments).count, 1);
    assert!(item(ScrubItem::FormValues)
        .details
        .iter()
        .any(|d| d.contains(SECRET)));
    assert_eq!(item(ScrubItem::Thumbnails).count, 1);
    assert_eq!(item(ScrubItem::ExternalLinks).count, 1);
    assert!(item(ScrubItem::UnusedObjects).count >= 1);
}

#[test]
fn each_item_can_be_chosen_alone() {
    let mut doc = Document::load_mem(&everything_pdf()).unwrap();
    let report = scrub(
        &mut doc,
        &ScrubOptions {
            items: vec![ScrubItem::DocumentInfo],
        },
    )
    .unwrap();
    assert_eq!(report.findings.len(), 1);
    assert!(!doc.trailer.has(b"Info"));
    // everything else is still there
    assert!(doc.catalog().unwrap().has(b"Metadata"));
    assert!(doc.catalog().unwrap().has(b"OpenAction"));
}

#[test]
fn a_hidden_layer_loses_its_content_not_just_its_name() {
    let mut doc = Document::load_mem(&everything_pdf()).unwrap();
    scrub(
        &mut doc,
        &ScrubOptions {
            items: vec![ScrubItem::HiddenLayers],
        },
    )
    .unwrap();
    let saved = save_clean(&mut doc).unwrap();
    assert_not_in_bytes(&saved, &format!("hidden layer {SECRET}"));
    assert!(!saved.windows(13).any(|w| w == b"OCProperties"));
    let doc = Document::load_mem(&saved).unwrap();
    let query = TextQuery::Phrase {
        text: "visible".into(),
        match_case: true,
    };
    assert_eq!(
        find_text_areas(&doc, &query, &FindOptions::default())
            .unwrap()
            .len(),
        1
    );
}
