//! "Check This PDF", optimising without changing the look, and the pdf/x
//! print checks, cross-checked with qpdf where it applies.

mod support;

use letters_standards::{check_pdfx, optimize, preflight, OptimizeOptions, PdfxLevel, PdfxOptions};
use lopdf::{dictionary, Document, Object, Stream};
use support::{
    assert_qpdf_clean, boxes, differing_pixels, helvetica_text, one_page, qpdf_problems, render,
    save,
};

fn rules(findings: &[letters_standards::Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| finding.rule.clone())
        .collect()
}

#[test]
fn a_sound_file_has_no_structural_findings() {
    let bytes = save(&mut boxes());
    let report = preflight(&bytes).unwrap();
    assert!(report.structure.is_empty(), "{:?}", report.structure);
    assert_eq!(report.pages.len(), 1);
    assert_eq!(report.pages[0].media, Some([0.0, 0.0, 612.0, 792.0]));
    assert!(report
        .size
        .iter()
        .any(|line| line.part == "page content" && line.bytes > 0));
    assert!(report.summary.starts_with("The file is sound"));
}

#[test]
fn broken_cross_references_are_found_and_qpdf_agrees() {
    let mut bytes = save(&mut boxes());
    // move the first in-use entry of the table by one byte
    let table = bytes
        .windows(5)
        .rposition(|window| window == b"\nxref")
        .unwrap();
    let entry = table
        + bytes[table..]
            .windows(3)
            .position(|window| window == b" n ")
            .unwrap()
        - 16;
    let digit = entry + 9;
    bytes[digit] = if bytes[digit] == b'9' {
        b'8'
    } else {
        bytes[digit] + 1
    };
    let report = preflight(&bytes).unwrap();
    assert!(
        rules(&report.structure).contains(&"structure.xrefOffset".to_owned()),
        "{:?}",
        report.structure
    );
    if let Some(problems) = qpdf_problems(&bytes, "broken-xref") {
        assert!(
            !problems.is_empty(),
            "qpdf should notice the broken table too"
        );
    }
}

#[test]
fn references_to_missing_objects_are_found() {
    let mut doc = boxes();
    let page = doc.get_pages()[&1];
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("Thumb", Object::Reference((999, 0)));
    let report = preflight(&save(&mut doc)).unwrap();
    assert!(rules(&report.structure).contains(&"structure.danglingReference".to_owned()));
}

fn image(width: i64, height: i64, rgb: Vec<u8>) -> Stream {
    Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => width, "Height" => height, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 },
        rgb,
    )
}

#[test]
fn fonts_resolutions_and_viewer_differences_are_listed() {
    let mut doc = helvetica_text();
    let picture = doc.add_object(image(10, 10, vec![200; 300]));
    let page = doc.get_pages()[&1];
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 72 72 Td (x) Tj ET q 500 0 0 500 50 50 cm /Im1 Do Q".to_vec(),
    ));
    let page_dict = doc.get_dictionary_mut(page).unwrap();
    page_dict.set("Contents", Object::Reference(content));
    let mut resources = page_dict
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap()
        .clone();
    resources.set(
        "XObject",
        dictionary! { "Im1" => Object::Reference(picture) },
    );
    resources.set(
        "ExtGState",
        dictionary! { "OP" => dictionary! { "OP" => true } },
    );
    page_dict.set("Resources", resources);
    let report = preflight(&save(&mut doc)).unwrap();
    assert_eq!(report.fonts[0].name, "Helvetica");
    assert!(!report.fonts[0].embedded);
    let placed = &report.images[0];
    assert!(
        (placed.pixels_per_inch - 1.44).abs() < 0.01,
        "{}",
        placed.pixels_per_inch
    );
    let found = rules(&report.portability);
    for expected in [
        "portability.fontNotEmbedded",
        "portability.lowResolution",
        "portability.overprint",
    ] {
        assert!(
            found.contains(&expected.to_owned()),
            "{expected} in {found:?}"
        );
    }
}

/// a page with the same picture stored twice, a sharp picture drawn small,
/// and uncompressed content.
fn wasteful() -> Document {
    let mut doc = one_page("", lopdf::Dictionary::new());
    let flat = |colour: [u8; 3], size: usize| colour.repeat(size * size);
    let first = doc.add_object(image(40, 40, flat([20, 120, 200], 40)));
    let copy = doc.add_object(image(40, 40, flat([20, 120, 200], 40)));
    let sharp = doc.add_object(image(800, 800, flat([200, 60, 60], 800)));
    let content = "q 100 0 0 100 72 600 cm /A Do Q q 100 0 0 100 300 600 cm /B Do Q q 96 0 0 96 72 300 cm /C Do Q";
    let content = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page = doc.get_pages()[&1];
    let page_dict = doc.get_dictionary_mut(page).unwrap();
    page_dict.set("Contents", Object::Reference(content));
    page_dict.set(
        "Resources",
        dictionary! { "XObject" => dictionary! { "A" => Object::Reference(first), "B" => Object::Reference(copy), "C" => Object::Reference(sharp) } },
    );
    doc.add_object(Stream::new(dictionary! {}, b"nobody uses this".to_vec()));
    doc
}

#[test]
fn optimising_makes_files_smaller_without_changing_their_look() {
    let mut doc = wasteful();
    let before = save(&mut doc.clone());
    let options = OptimizeOptions {
        max_pixels_per_inch: Some(150.0),
        object_streams: true,
    };
    let report = optimize(&mut doc, &options).unwrap();
    assert!(
        report.bytes_after < report.bytes_before / 4,
        "{} -> {}",
        report.bytes_before,
        report.bytes_after
    );
    for measure in &report.measures {
        assert!(measure.bytes_saved >= 0, "{measure:?}");
    }
    let saved = |name: &str| {
        report
            .measures
            .iter()
            .find(|m| m.name.starts_with(name))
            .unwrap()
            .bytes_saved
    };
    assert!(saved("Identical streams") > 0);
    assert!(saved("Uncompressed streams") > 0);
    assert!(saved("Unused objects") > 0);
    assert!(report
        .not_done
        .iter()
        .any(|line| line.contains("linearised")));
    let optimised = Document::load_mem(&report.bytes).unwrap();
    let sharp = optimised
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .filter(|stream| {
            stream
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|s| s.as_name().ok())
                == Some(b"Image")
        })
        .map(|stream| stream.dict.get(b"Width").unwrap().as_i64().unwrap())
        .max()
        .unwrap();
    assert_eq!(sharp, 200, "the 600 ppi picture is now 150 ppi");
    assert_qpdf_clean(&report.bytes, "optimised");
    let a = render(&before, 0, 1.0, "optimise-before");
    let b = render(&report.bytes, 0, 1.0, "optimise-after");
    assert!(differing_pixels(&a, &b) <= 20, "the look changed");
}

/// a page ready for a print shop: cmyk only, trim box, trapping stated,
/// version named, and a pdf/x output intent.
fn print_ready() -> Document {
    let mut doc = one_page("0 0 0 1 k 72 72 200 200 re f", lopdf::Dictionary::new());
    let page = doc.get_pages()[&1];
    let page_dict = doc.get_dictionary_mut(page).unwrap();
    page_dict.set("TrimBox", vec![9.into(), 9.into(), 603.into(), 783.into()]);
    page_dict.set("BleedBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
    let info = doc.add_object(dictionary! { "Trapped" => "False", "GTS_PDFXVersion" => Object::string_literal("PDF/X-1a:2003") });
    doc.trailer.set("Info", Object::Reference(info));
    let intent = doc.add_object(dictionary! { "Type" => "OutputIntent", "S" => "GTS_PDFX", "OutputConditionIdentifier" => Object::string_literal("CGATS TR 001") });
    let catalog = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    doc.get_dictionary_mut(catalog)
        .unwrap()
        .set("OutputIntents", vec![Object::Reference(intent)]);
    doc
}

#[test]
fn pdfx_checks_pass_a_print_ready_file_and_name_each_problem() {
    let report = check_pdfx(&print_ready(), PdfxLevel::X1a, &PdfxOptions::default());
    assert!(report.passes, "{:#?}", report.findings);
    let report = check_pdfx(&boxes(), PdfxLevel::X1a, &PdfxOptions::default());
    let found = rules(&report.findings);
    for expected in [
        "pdfx.trapped",
        "pdfx.version",
        "pdfx.outputIntent",
        "pdfx.trimBox",
        "pdfx.rgb",
    ] {
        assert!(
            found.contains(&expected.to_owned()),
            "{expected} in {found:?}"
        );
    }
    let mut transparent = print_ready();
    let page = transparent.get_pages()[&1];
    transparent
        .get_dictionary_mut(page)
        .unwrap()
        .set("Group", dictionary! { "S" => "Transparency" });
    let found = rules(&check_pdfx(&transparent, PdfxLevel::X1a, &PdfxOptions::default()).findings);
    assert!(found.contains(&"pdfx.transparency".to_owned()));
    let found = rules(&check_pdfx(&transparent, PdfxLevel::X4, &PdfxOptions::default()).findings);
    assert!(
        !found.contains(&"pdfx.transparency".to_owned()),
        "x-4 allows transparency"
    );
    assert!(
        found.contains(&"pdfx.outputProfile".to_owned()),
        "x-4 needs a profile"
    );
}

#[test]
fn an_encrypted_document_is_refused_rather_than_optimised_empty() {
    let mut doc = wasteful();
    let encrypt = doc.add_object(dictionary! { "Filter" => "Standard", "V" => 4, "R" => 4 });
    doc.trailer.set("Encrypt", Object::Reference(encrypt));
    let refused = optimize(&mut doc, &OptimizeOptions::default());
    assert_eq!(
        refused.unwrap_err(),
        letters_standards::StandardsError::Locked
    );
}
