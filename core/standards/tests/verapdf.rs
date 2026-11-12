//! runs veraPDF, the reference pdf/a validator, on the files the
//! conversion makes. veraPDF is a development tool here (gplv3+ / mplv2+),
//! never bundled with or linked into the app, so these tests are ignored
//! by default: run them with `cargo test -p letters-standards --test
//! verapdf -- --ignored`. veraPDF is found through LETTERS_VERAPDF (the
//! path to verapdf.bat or verapdf) or the per-user install folder.

mod support;

use std::path::PathBuf;
use std::process::Command;

use letters_standards::{
    convert_to_pdfa, save_pdfa, ConversionHelpers, ConversionOptions, PdfaLevel,
};
use lopdf::{dictionary, Object, Stream};
use support::{atkinson, boxes, helvetica_text, save, temp_dir, OneFont, PdfiumRenderer};

fn verapdf() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("LETTERS_VERAPDF") {
        return Some(PathBuf::from(path));
    }
    let local = std::env::var_os("LOCALAPPDATA")?;
    let installed = PathBuf::from(local)
        .join("Programs")
        .join("verapdf")
        .join("verapdf.bat");
    installed.exists().then_some(installed)
}

fn flavour(level: PdfaLevel) -> &'static str {
    match level {
        PdfaLevel::A1b => "1b",
        PdfaLevel::A2b => "2b",
        PdfaLevel::A2u => "2u",
        PdfaLevel::A3b => "3b",
    }
}

/// veraPDF's verdict and the rules it lists as failed.
fn validate(bytes: &[u8], level: PdfaLevel, name: &str) -> (bool, String) {
    let tool = verapdf().expect("veraPDF isn’t installed; set LETTERS_VERAPDF");
    let path = temp_dir(&format!("verapdf-{name}")).join(format!("{name}.pdf"));
    std::fs::write(&path, bytes).unwrap();
    let output = Command::new(tool)
        .args(["--flavour", flavour(level), "--format", "text", "-v"])
        .arg(&path)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    (text.contains("PASS"), text)
}

fn converted(mut doc: lopdf::Document, level: PdfaLevel) -> Vec<u8> {
    let font = OneFont::new(atkinson(), "Atkinson Hyperlegible");
    let renderer = PdfiumRenderer {
        original: save(&mut doc.clone()),
    };
    let helpers = ConversionHelpers {
        fonts: Some(&font),
        renderer: Some(&renderer),
    };
    let report = convert_to_pdfa(&mut doc, level, &ConversionOptions::default(), &helpers).unwrap();
    assert!(
        report.compliant,
        "{level:?}: our checker: {:#?}",
        report.blockers
    );
    save_pdfa(&mut doc).unwrap()
}

fn soft_masked_picture() -> lopdf::Document {
    let mut doc = boxes();
    let mask = doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
        vec![255, 0],
    ));
    let image = doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "SMask" => Object::Reference(mask) },
        vec![0, 128, 0, 0, 128, 0],
    ));
    let page = doc.get_pages()[&1];
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"q 200 0 0 100 100 300 cm /Im1 Do Q".to_vec(),
    ));
    let page_dict = doc.get_dictionary_mut(page).unwrap();
    page_dict.set(
        "Resources",
        dictionary! { "XObject" => dictionary! { "Im1" => Object::Reference(image) } },
    );
    page_dict.set("Contents", Object::Reference(content));
    doc
}

/// makes one of the test documents.
type MakeDocument = fn() -> lopdf::Document;

#[test]
#[ignore = "needs veraPDF, a development-only tool"]
fn verapdf_accepts_our_conversions_at_every_level() {
    let mut failures = Vec::new();
    let documents: Vec<(&str, MakeDocument)> = vec![
        ("boxes", boxes),
        ("embedded-font", helvetica_text),
        ("soft-mask", soft_masked_picture),
    ];
    for level in [
        PdfaLevel::A1b,
        PdfaLevel::A2b,
        PdfaLevel::A2u,
        PdfaLevel::A3b,
    ] {
        for (name, make) in &documents {
            let bytes = converted(make(), level);
            let label = format!("{name}-{}", flavour(level));
            let (passed, text) = validate(&bytes, level, &label);
            println!("veraPDF {label}: {}", if passed { "PASS" } else { "FAIL" });
            if !passed {
                failures.push(format!("{label}:\n{text}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "veraPDF rejected:\n{}",
        failures.join("\n\n")
    );
}

/// a square comment without an appearance, a link with a script, and an
/// attachment: all things the conversion has to repair.
fn repairs_needed() -> lopdf::Document {
    let mut doc = boxes();
    let square = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Square", "F" => 4,
        "Rect" => vec![100.into(), 100.into(), 200.into(), 160.into()],
        "C" => vec![0.into(), 0.5.into(), 0.into()],
    });
    let link = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![0.into(), 0.into(), 50.into(), 50.into()],
        "A" => dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert(1)") },
    });
    let page = doc.get_pages()[&1];
    doc.get_dictionary_mut(page).unwrap().set(
        "Annots",
        vec![Object::Reference(square), Object::Reference(link)],
    );
    let file = doc.add_object(Stream::new(
        dictionary! { "Type" => "EmbeddedFile" },
        b"notes".to_vec(),
    ));
    let spec = doc.add_object(dictionary! { "Type" => "Filespec", "F" => Object::string_literal("notes.txt"), "EF" => dictionary! { "F" => Object::Reference(file) } });
    let catalog = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let names = dictionary! { "EmbeddedFiles" => dictionary! { "Names" => vec![Object::string_literal("notes.txt"), Object::Reference(spec)] } };
    doc.get_dictionary_mut(catalog).unwrap().set("Names", names);
    doc
}

#[test]
#[ignore = "needs veraPDF, a development-only tool"]
fn verapdf_accepts_repaired_documents_and_rejects_unconverted_ones() {
    let (passed, _) = validate(&save(&mut helvetica_text()), PdfaLevel::A2b, "unconverted");
    assert!(!passed, "veraPDF must reject a file that wasn't converted");
    for level in [
        PdfaLevel::A1b,
        PdfaLevel::A2b,
        PdfaLevel::A2u,
        PdfaLevel::A3b,
    ] {
        let bytes = converted(repairs_needed(), level);
        let label = format!("repaired-{}", flavour(level));
        let (passed, text) = validate(&bytes, level, &label);
        println!("veraPDF {label}: {}", if passed { "PASS" } else { "FAIL" });
        assert!(passed, "{label}:\n{text}");
    }
}
