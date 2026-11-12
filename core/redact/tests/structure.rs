//! text alternatives: /ActualText and /Alt can spell out removed content
//! a second time, in the content stream and in the structure tree.

mod support;

use letters_redact::lopdf::Document;
use letters_redact::{
    find_text_areas, redact, save_clean, FindOptions, RedactOptions, RedactionArea, TextQuery,
};
use support::*;

const SECRET: &str = "SECRETWORD";

#[test]
fn text_alternatives_of_removed_content_are_removed() {
    let dir = temp_dir("structure");
    let worker = start_worker();
    let mut pdf = Pdf::new();
    let font = add_helvetica(&mut pdf);
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let page = pdf.reserve();
    let root = pdf.reserve();
    let span = pdf.add(format!("<< /Type /StructElem /S /Span /P {root} 0 R /Pg {page} 0 R /K 0 /Alt (the word {SECRET}) /ActualText ({SECRET}) >>"));
    let kept = pdf.add(format!(
        "<< /Type /StructElem /S /P /P {root} 0 R /Pg {page} 0 R /K 1 /Alt (kept description) >>"
    ));
    pdf.set(
        root,
        format!("<< /Type /StructTreeRoot /K [{span} 0 R {kept} 0 R] >>"),
    );
    let content = pdf.add_stream(
        "",
        format!(
            "/Span << /MCID 0 /ActualText ({SECRET}) >> BDC BT /F1 12 Tf 72 700 Td ({SECRET}) Tj ET EMC\n\
             /P << /MCID 1 >> BDC BT /F1 12 Tf 72 650 Td (other text) Tj ET EMC"
        )
        .as_bytes(),
    );
    pdf.set(page, format!(
        "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] /Contents {content} 0 R /StructParents 0 /Resources << /Font << /F1 {font} 0 R >> >> >>"
    ));
    pdf.set(
        tree,
        format!("<< /Type /Pages /Count 1 /Kids [{page} 0 R] >>"),
    );
    pdf.set(catalog, format!("<< /Type /Catalog /Pages {tree} 0 R /StructTreeRoot {root} 0 R /MarkInfo << /Marked true >> >>"));
    let mut doc = Document::load_mem(&pdf.finish(catalog, "")).unwrap();

    let query = TextQuery::Phrase {
        text: SECRET.into(),
        match_case: true,
    };
    let areas: Vec<RedactionArea> = find_text_areas(&doc, &query, &FindOptions::default())
        .unwrap()
        .into_iter()
        .map(|found| RedactionArea {
            page: found.page,
            rects: found.rects,
        })
        .collect();
    redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    let saved = save_clean(&mut doc).unwrap();
    assert_secret_gone(&worker, &dir, &saved, SECRET);
    assert!(
        saved.windows(16).any(|w| w == b"kept description"),
        "the other element keeps its description"
    );
}
