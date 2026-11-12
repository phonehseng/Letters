//! pdf/a: a failing and a passing case for each rule, conversion of
//! documents with each kind of violation followed by a clean check, the
//! look kept where nothing was turned into a picture, and qpdf finding no
//! structural problem in any output.

mod support;

use letters_standards::{
    check_pdfa, check_pdfa_file, convert_to_pdfa, save_pdfa, ConversionHelpers, ConversionOptions,
    Fix, PdfaLevel, PdfaReport,
};
use lopdf::{dictionary, Document, Object, Stream};
use support::{
    assert_qpdf_clean, atkinson, boxes, differing_pixels, helvetica_text, one_page, render,
    restricted_font, save, OneFont, PdfiumRenderer,
};

const LEVELS: [PdfaLevel; 4] = [
    PdfaLevel::A1b,
    PdfaLevel::A2b,
    PdfaLevel::A2u,
    PdfaLevel::A3b,
];

fn convert(doc: &mut Document, level: PdfaLevel) -> letters_standards::ConversionReport {
    let font = OneFont::new(atkinson(), "Atkinson Hyperlegible");
    let original = save(&mut doc.clone());
    let renderer = PdfiumRenderer { original };
    let helpers = ConversionHelpers {
        fonts: Some(&font),
        renderer: Some(&renderer),
    };
    convert_to_pdfa(doc, level, &ConversionOptions::default(), &helpers).unwrap()
}

/// a document already converted to the level, saved and loaded again.
fn clean(level: PdfaLevel) -> Document {
    let mut doc = boxes();
    let report = convert(&mut doc, level);
    assert!(report.compliant, "{:#?}", report.blockers);
    Document::load_mem(&save_pdfa(&mut doc).unwrap()).unwrap()
}

fn rules(report: &PdfaReport) -> Vec<String> {
    report
        .findings
        .iter()
        .map(|finding| finding.rule.clone())
        .collect()
}

fn catalog_id(doc: &Document) -> lopdf::ObjectId {
    doc.trailer.get(b"Root").unwrap().as_reference().unwrap()
}

fn first_page(doc: &Document) -> lopdf::ObjectId {
    doc.get_pages()[&1]
}

fn add_annotation(doc: &mut Document, annotation: lopdf::Dictionary) {
    let id = doc.add_object(annotation);
    let page = first_page(doc);
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("Annots", Object::Array(vec![Object::Reference(id)]));
}

fn page_resources(doc: &mut Document) -> &mut lopdf::Dictionary {
    let page = first_page(doc);
    doc.get_dictionary_mut(page)
        .unwrap()
        .get_mut(b"Resources")
        .unwrap()
        .as_dict_mut()
        .unwrap()
}

fn set_content(doc: &mut Document, content: &str) {
    let id = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page = first_page(doc);
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("Contents", Object::Reference(id));
}

/// one rule: how to break it, the finding it gives, and the level.
struct Case {
    name: &'static str,
    level: PdfaLevel,
    rule: &'static str,
    breaks: fn(&mut Document),
    /// conversion makes it compliant again
    fixable: bool,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "no metadata",
            level: PdfaLevel::A2b,
            rule: "metadata.missing",
            fixable: true,
            breaks: |doc| {
                let catalog = catalog_id(doc);
                doc.get_dictionary_mut(catalog).unwrap().remove(b"Metadata");
            },
        },
        Case {
            name: "wrong identification",
            level: PdfaLevel::A2u,
            rule: "metadata.identification",
            fixable: true,
            breaks: |doc| {
                let packet = "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\" pdfaid:part=\"2\" pdfaid:conformance=\"B\"/></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>";
                let id = doc.add_object(Stream::new(
                    dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
                    packet.as_bytes().to_vec(),
                ));
                let catalog = catalog_id(doc);
                doc.get_dictionary_mut(catalog)
                    .unwrap()
                    .set("Metadata", Object::Reference(id));
            },
        },
        Case {
            name: "info disagrees",
            level: PdfaLevel::A2b,
            rule: "metadata.infoMismatch",
            fixable: true,
            breaks: |doc| {
                let info = doc.trailer.get(b"Info").unwrap().as_reference().unwrap();
                doc.get_dictionary_mut(info)
                    .unwrap()
                    .set("Title", Object::string_literal("Another title"));
            },
        },
        Case {
            name: "no file id",
            level: PdfaLevel::A1b,
            rule: "document.fileId",
            fixable: true,
            breaks: |doc| {
                doc.trailer.remove(b"ID");
            },
        },
        Case {
            name: "xfa and need appearances",
            level: PdfaLevel::A2b,
            rule: "forms.xfa",
            fixable: true,
            breaks: |doc| {
                let form = doc.add_object(dictionary! { "Fields" => vec![], "XFA" => Object::string_literal("<xdp/>"), "NeedAppearances" => true });
                let catalog = catalog_id(doc);
                doc.get_dictionary_mut(catalog)
                    .unwrap()
                    .set("AcroForm", Object::Reference(form));
            },
        },
        Case {
            name: "a script action",
            level: PdfaLevel::A2b,
            rule: "actions.forbidden",
            fixable: true,
            breaks: |doc| {
                let action = dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert(1)") };
                add_annotation(
                    doc,
                    dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![0.into(), 0.into(), 50.into(), 50.into()], "F" => 4, "A" => action },
                );
            },
        },
        Case {
            name: "event actions",
            level: PdfaLevel::A1b,
            rule: "actions.additional",
            fixable: true,
            breaks: |doc| {
                let catalog = catalog_id(doc);
                let aa = dictionary! { "WC" => dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("1") } };
                doc.get_dictionary_mut(catalog).unwrap().set("AA", aa);
            },
        },
        Case {
            name: "document scripts",
            level: PdfaLevel::A3b,
            rule: "actions.javascript",
            fixable: true,
            breaks: |doc| {
                let catalog = catalog_id(doc);
                doc.get_dictionary_mut(catalog).unwrap().set(
                    "Names",
                    dictionary! { "JavaScript" => dictionary! { "Names" => vec![] } },
                );
            },
        },
        Case {
            name: "lzw",
            level: PdfaLevel::A2b,
            rule: "streams.lzw",
            fixable: true,
            breaks: |doc| {
                // the lzw example of iso 32000-1 7.4.4.2: "-----A---B"
                let data = vec![0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01];
                let stream = Stream::new(dictionary! { "Filter" => "LZWDecode" }, data);
                let id = doc.add_object(stream);
                page_resources(doc).set(
                    "Properties",
                    dictionary! { "Unused" => Object::Reference(id) },
                );
            },
        },
        Case {
            name: "external stream",
            level: PdfaLevel::A2b,
            rule: "streams.external",
            fixable: false,
            breaks: |doc| {
                let id = doc.add_object(Stream::new(
                    dictionary! { "F" => Object::string_literal("data.bin") },
                    vec![],
                ));
                page_resources(doc).set(
                    "Properties",
                    dictionary! { "Outside" => Object::Reference(id) },
                );
            },
        },
        Case {
            name: "postscript",
            level: PdfaLevel::A1b,
            rule: "xobjects.postscript",
            fixable: true,
            breaks: |doc| {
                let form = Stream::new(
                    dictionary! { "Type" => "XObject", "Subtype" => "Form", "Subtype2" => "PS", "BBox" => vec![0.into(), 0.into(), 1.into(), 1.into()] },
                    vec![],
                );
                let id = doc.add_object(form);
                page_resources(doc).set("XObject", dictionary! { "P1" => Object::Reference(id) });
            },
        },
        Case {
            name: "font not embedded",
            level: PdfaLevel::A2u,
            rule: "fonts.notEmbedded",
            fixable: true,
            breaks: |doc| {
                let font = dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" };
                page_resources(doc).set("Font", dictionary! { "F1" => font });
                set_content(doc, "BT /F1 12 Tf 72 72 Td (Hi) Tj ET");
            },
        },
        Case {
            name: "rgb without intent",
            level: PdfaLevel::A2b,
            rule: "color.deviceRgb",
            fixable: true,
            breaks: |doc| {
                let catalog = catalog_id(doc);
                doc.get_dictionary_mut(catalog)
                    .unwrap()
                    .remove(b"OutputIntents");
            },
        },
        Case {
            name: "cmyk with an rgb intent",
            level: PdfaLevel::A2b,
            rule: "color.deviceCmyk",
            fixable: false,
            breaks: |doc| {
                set_content(doc, "0 1 1 0 k 72 72 100 100 re f");
            },
        },
        Case {
            name: "transparency in part 1",
            level: PdfaLevel::A1b,
            rule: "transparency.state",
            fixable: true,
            breaks: |doc| {
                page_resources(doc).set(
                    "ExtGState",
                    dictionary! { "Half" => dictionary! { "ca" => 0.5 } },
                );
                set_content(doc, "BT ET /Half gs 1 0 0 rg 72 600 200 100 re f");
            },
        },
        Case {
            name: "image smoothing",
            level: PdfaLevel::A2b,
            rule: "images.interpolate",
            fixable: true,
            breaks: |doc| {
                let image = Stream::new(
                    dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Interpolate" => true },
                    vec![255, 0, 0],
                );
                let id = doc.add_object(image);
                page_resources(doc).set("XObject", dictionary! { "Im1" => Object::Reference(id) });
                set_content(doc, "q 100 0 0 100 72 72 cm /Im1 Do Q");
            },
        },
        Case {
            name: "hidden annotation",
            level: PdfaLevel::A2b,
            rule: "annotations.flags",
            fixable: true,
            breaks: |doc| {
                add_annotation(
                    doc,
                    dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![0.into(), 0.into(), 50.into(), 50.into()], "F" => 2 },
                );
            },
        },
        Case {
            name: "movie annotation",
            level: PdfaLevel::A2b,
            rule: "annotations.type",
            fixable: true,
            breaks: |doc| {
                add_annotation(
                    doc,
                    dictionary! { "Type" => "Annot", "Subtype" => "Movie", "Rect" => vec![0.into(), 0.into(), 50.into(), 50.into()], "F" => 4 },
                );
            },
        },
        Case {
            name: "annotation without appearance",
            level: PdfaLevel::A2b,
            rule: "annotations.appearance",
            fixable: true,
            breaks: |doc| {
                add_annotation(
                    doc,
                    dictionary! { "Type" => "Annot", "Subtype" => "Square", "Rect" => vec![100.into(), 100.into(), 200.into(), 160.into()], "F" => 4, "C" => vec![0.into(), 0.5.into(), 0.into()] },
                );
            },
        },
        Case {
            name: "attachment in part 1",
            level: PdfaLevel::A1b,
            rule: "files.partOne",
            fixable: true,
            breaks: attach,
        },
        Case {
            name: "undescribed attachment in part 3",
            level: PdfaLevel::A3b,
            rule: "files.description",
            fixable: true,
            breaks: attach,
        },
        Case {
            name: "unnamed layer configuration",
            level: PdfaLevel::A2b,
            rule: "layers.configuration",
            fixable: true,
            breaks: |doc| {
                let group = doc.add_object(
                    dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Notes") },
                );
                let catalog = catalog_id(doc);
                doc.get_dictionary_mut(catalog).unwrap().set("OCProperties", dictionary! { "OCGs" => vec![Object::Reference(group)], "D" => dictionary! { "AS" => vec![] } });
            },
        },
        Case {
            name: "a very long name",
            level: PdfaLevel::A2b,
            rule: "limits.value",
            fixable: false,
            breaks: |doc| {
                page_resources(doc).set("Properties", dictionary! { "A".repeat(130) => 1 });
            },
        },
        Case {
            name: "a tiny page",
            level: PdfaLevel::A2b,
            rule: "pages.size",
            fixable: false,
            breaks: |doc| {
                let page = first_page(doc);
                doc.get_dictionary_mut(page)
                    .unwrap()
                    .set("MediaBox", vec![0.into(), 0.into(), 2.into(), 2.into()]);
            },
        },
    ]
}

fn attach(doc: &mut Document) {
    let file = doc.add_object(Stream::new(
        dictionary! { "Type" => "EmbeddedFile" },
        b"notes".to_vec(),
    ));
    let spec = doc.add_object(dictionary! { "Type" => "Filespec", "F" => Object::string_literal("notes.txt"), "EF" => dictionary! { "F" => Object::Reference(file) } });
    let catalog = catalog_id(doc);
    let names = dictionary! { "EmbeddedFiles" => dictionary! { "Names" => vec![Object::string_literal("notes.txt"), Object::Reference(spec)] } };
    doc.get_dictionary_mut(catalog).unwrap().set("Names", names);
}

#[test]
fn every_rule_has_a_failing_and_a_passing_case() {
    for case in cases() {
        let passing = clean(case.level);
        assert!(
            !rules(&check_pdfa(&passing, case.level)).contains(&case.rule.to_owned()),
            "{}: the clean document already breaks {}",
            case.name,
            case.rule
        );
        let mut failing = passing.clone();
        (case.breaks)(&mut failing);
        let report = check_pdfa(&failing, case.level);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.rule == case.rule);
        let finding = finding.unwrap_or_else(|| {
            panic!(
                "{}: {} wasn’t found in {:?}",
                case.name,
                case.rule,
                rules(&report)
            )
        });
        assert!(
            finding.clause.starts_with("ISO 19005-"),
            "{}",
            finding.clause
        );
        assert!(!finding.message.is_empty());
    }
}

#[test]
fn each_kind_of_violation_converts_to_a_clean_check() {
    for case in cases() {
        let mut doc = clean(case.level);
        (case.breaks)(&mut doc);
        let report = convert(&mut doc, case.level);
        let bytes = save_pdfa(&mut doc).unwrap();
        let after = check_pdfa_file(&bytes, case.level).unwrap();
        if case.fixable {
            assert!(report.compliant, "{}: {:#?}", case.name, report.blockers);
            assert!(after.compliant, "{}: {:#?}", case.name, after.findings);
            assert!(
                !report.changes.is_empty()
                    || report.output_intent_added
                    || !report.fonts.is_empty(),
                "{}: nothing was reported as changed",
                case.name
            );
        } else {
            assert!(!report.compliant, "{}", case.name);
            assert!(
                report
                    .blockers
                    .iter()
                    .any(|blocker| blocker.rule == case.rule && blocker.fix == Fix::NotPossible),
                "{}: {:#?}",
                case.name,
                report.blockers
            );
            assert!(
                report.summary.contains("can’t be made"),
                "{}",
                report.summary
            );
        }
        assert_qpdf_clean(&bytes, case.name);
    }
}

#[test]
fn plain_documents_convert_to_every_level_without_changing_their_look() {
    for level in LEVELS {
        let mut doc = boxes();
        let before = save(&mut doc.clone());
        let report = convert(&mut doc, level);
        assert!(report.compliant, "{level:?}: {:#?}", report.blockers);
        assert!(report.output_intent_added);
        let bytes = save_pdfa(&mut doc).unwrap();
        let checked = check_pdfa_file(&bytes, level).unwrap();
        assert!(checked.compliant, "{level:?}: {:#?}", checked.findings);
        assert!(bytes.starts_with(if level == PdfaLevel::A1b {
            b"%PDF-1.4"
        } else {
            b"%PDF-1.7"
        }));
        assert_qpdf_clean(&bytes, &format!("{level:?}"));
        let a = render(&before, 0, 1.0, "look-before");
        let b = render(&bytes, 0, 1.0, "look-after");
        assert!(
            differing_pixels(&a, &b) <= 20,
            "{level:?}: the look changed"
        );
    }
}

#[test]
fn fonts_are_embedded_with_their_own_widths_and_unicode() {
    let mut doc = helvetica_text();
    let font = OneFont::new(atkinson(), "Atkinson Hyperlegible");
    let helpers = ConversionHelpers {
        fonts: Some(&font),
        renderer: None,
    };
    let report = convert_to_pdfa(
        &mut doc,
        PdfaLevel::A2u,
        &ConversionOptions::default(),
        &helpers,
    )
    .unwrap();
    assert!(report.compliant, "{:#?}", report.blockers);
    let outcome = &report.fonts[0];
    assert_eq!(
        outcome.embedded_as.as_deref(),
        Some("Atkinson Hyperlegible")
    );
    assert!(
        outcome.widths_differ,
        "a substitute with other widths is reported"
    );
    let request = &font.requests.borrow()[0];
    assert_eq!(request.name, "Helvetica");
    assert!(
        !request.metric_compatible.is_empty(),
        "letters-fonts names a metric-compatible substitute"
    );
    let bytes = save_pdfa(&mut doc).unwrap();
    assert!(check_pdfa_file(&bytes, PdfaLevel::A2u).unwrap().compliant);
    assert_qpdf_clean(&bytes, "embedded font");
    let rendered = render(&bytes, 0, 1.0, "embedded-font");
    let dark = rendered
        .rgb
        .chunks(3)
        .filter(|pixel| pixel.iter().all(|c| *c < 100))
        .count();
    assert!(dark > 50, "the text is drawn with the embedded font");
}

#[test]
fn fonts_whose_licence_forbids_embedding_are_blockers() {
    let mut doc = helvetica_text();
    let font = OneFont::new(restricted_font(), "Restricted Sans");
    let helpers = ConversionHelpers {
        fonts: Some(&font),
        renderer: None,
    };
    let report = convert_to_pdfa(
        &mut doc,
        PdfaLevel::A2b,
        &ConversionOptions::default(),
        &helpers,
    )
    .unwrap();
    assert!(!report.compliant);
    let problem = report.fonts[0].problem.as_deref().unwrap();
    assert!(
        problem.contains("licence") && problem.contains("doesn’t allow embedding"),
        "{problem}"
    );
    assert!(report
        .blockers
        .iter()
        .any(|blocker| blocker.rule == "fonts.notEmbedded"));
}

#[test]
fn part_one_transparency_becomes_pictures_and_says_so() {
    // a picture with a transparent background, drawn on the page
    let mut doc = boxes();
    let mask = Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
        vec![255, 0],
    );
    let mask = doc.add_object(mask);
    let image = Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "SMask" => Object::Reference(mask) },
        vec![0, 128, 0, 0, 128, 0],
    );
    let image = doc.add_object(image);
    page_resources(&mut doc).set("XObject", dictionary! { "Im1" => Object::Reference(image) });
    set_content(
        &mut doc,
        "1 0 0 rg 72 600 200 100 re f q 200 0 0 100 100 300 cm /Im1 Do Q",
    );
    let before = save(&mut doc.clone());
    let report = convert(&mut doc, PdfaLevel::A1b);
    assert!(report.compliant, "{:#?}", report.blockers);
    assert_eq!(report.rasterised.len(), 1);
    assert!(!report.rasterised[0].whole_page);
    assert_eq!(report.rasterised[0].area, [100.0, 300.0, 300.0, 400.0]);
    assert!(report
        .changes
        .iter()
        .any(|change| change.contains("turned into a picture")));
    let bytes = save_pdfa(&mut doc).unwrap();
    assert!(check_pdfa_file(&bytes, PdfaLevel::A1b).unwrap().compliant);
    assert_qpdf_clean(&bytes, "rasterised area");
    let a = render(&before, 0, 1.0, "smask-before");
    let b = render(&bytes, 0, 1.0, "smask-after");
    assert!(
        differing_pixels(&a, &b) <= 200,
        "the area looks as before ({} pixels differ)",
        differing_pixels(&a, &b)
    );

    // opacity on the page turns the page into a picture, text kept searchable
    let mut doc = helvetica_text();
    page_resources(&mut doc).set(
        "ExtGState",
        dictionary! { "Half" => dictionary! { "ca" => 0.5 } },
    );
    set_content(
        &mut doc,
        "BT /F1 24 Tf 72 700 Td (Still searchable) Tj ET /Half gs 0 0 1 rg 72 600 200 100 re f",
    );
    let report = convert(&mut doc, PdfaLevel::A1b);
    assert!(report.rasterised[0].whole_page);
    assert!(report.compliant, "{:#?}", report.blockers);
    let bytes = save_pdfa(&mut doc).unwrap();
    let text = Document::load_mem(&bytes)
        .unwrap()
        .extract_text(&[1])
        .unwrap_or_default();
    assert!(text.contains("Still searchable"), "{text}");
}

#[test]
fn encrypted_documents_are_refused() {
    let mut doc = boxes();
    doc.trailer
        .set("Encrypt", dictionary! { "Filter" => "Standard" });
    let error = convert_to_pdfa(
        &mut doc,
        PdfaLevel::A2b,
        &ConversionOptions::default(),
        &ConversionHelpers::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("Unlock it first"));
    assert!(rules(&check_pdfa(&doc, PdfaLevel::A2b)).contains(&"document.encrypted".to_owned()));
}

#[test]
fn the_file_rules_look_at_the_bytes() {
    let mut doc = clean(PdfaLevel::A1b);
    let bytes = save_pdfa(&mut doc).unwrap();
    assert!(check_pdfa_file(&bytes, PdfaLevel::A1b).unwrap().compliant);
    let mut junk = bytes.clone();
    junk.extend_from_slice(b"garbage after the end");
    assert!(
        rules(&check_pdfa_file(&junk, PdfaLevel::A1b).unwrap()).contains(&"file.eof".to_owned())
    );
    let mut modern = Vec::new();
    doc.clone().save_modern(&mut modern).unwrap();
    let report = check_pdfa_file(&modern, PdfaLevel::A1b).unwrap();
    assert!(
        rules(&report).contains(&"file.compressedStructure".to_owned()),
        "{:?}",
        rules(&report)
    );
    let report = check_pdfa_file(&modern, PdfaLevel::A2b).unwrap();
    assert!(!rules(&report).contains(&"file.compressedStructure".to_owned()));
    let plain = one_page("", lopdf::Dictionary::new());
    let mut plain = plain;
    let mut ascii = Vec::new();
    plain
        .save_with_options(
            &mut ascii,
            lopdf::SaveOptions {
                ascii: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(rules(&check_pdfa_file(&ascii, PdfaLevel::A2b).unwrap())
        .contains(&"file.binaryComment".to_owned()));
}
