//! checks against the applications themselves (ignored by default: they
//! need powerpoint and libreoffice on this computer).
//!
//! a deck made by powerpoint, changed by one word and saved by letters,
//! must open in powerpoint without a repair prompt and show the same
//! shapes in the same places with the same text (only the word changed).
//! a deck saved by libreoffice must do the same in libreoffice.
//!
//! cargo test -p letters-slides --test applications -- --ignored --nocapture --test-threads=1

use std::path::{Path, PathBuf};
use std::process::Command;

use letters_slides::model::{Object, TextRun};
use letters_slides::xml;
use letters_slides::{read_pptx, write_pptx};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn out() -> PathBuf {
    root().join("tests/fidelity/out/applications")
}

fn decks(prefix: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(root().join("tests/decks"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "pptx"))
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(prefix))
        .collect();
    found.sort();
    found
}

/// reads a deck, puts "Changed " before its first run of text, writes it.
fn edit_one_word(path: &Path) -> PathBuf {
    let mut deck = read_pptx(&std::fs::read(path).unwrap()).unwrap();
    'slides: for slide in &mut deck.slides {
        for object in &mut slide.objects {
            if let Object::Shape(shape) = object {
                for paragraph in shape.text.iter_mut().flat_map(|t| t.paragraphs.iter_mut()) {
                    for run in &mut paragraph.runs {
                        if let TextRun::Text { text, .. } = run {
                            *text = format!("Changed {text}");
                            break 'slides;
                        }
                    }
                }
            }
        }
    }
    let _ = std::fs::create_dir_all(out());
    let target = out().join(path.file_name().unwrap());
    std::fs::write(&target, write_pptx(&deck).unwrap()).unwrap();
    target
}

#[derive(Debug, PartialEq)]
struct ShapeFacts {
    slide: i64,
    name: String,
    left: i64,
    top: i64,
    width: i64,
    height: i64,
    text: String,
}

/// whether powerpoint opened a deck, its error, and its shapes.
type DeckFacts = (bool, String, Vec<ShapeFacts>);

/// opens decks in powerpoint (tests/fidelity/powerpoint_check.ps1) and
/// returns, per deck, whether it opened and its shapes.
fn powerpoint_facts(decks: &[PathBuf]) -> Vec<DeckFacts> {
    let script = root().join("tests/fidelity/powerpoint_check.ps1");
    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .args(decks)
        .output()
        .expect("powershell runs");
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter(|l| l.starts_with('{'))
        .map(parse_facts)
        .collect()
}

fn parse_facts(line: &str) -> DeckFacts {
    let value: serde_json::Value = serde_json::from_str(line).unwrap();
    let shapes = value["shapes"]
        .as_array()
        .map(|shapes| {
            shapes
                .iter()
                .map(|s| ShapeFacts {
                    slide: s["slide"].as_i64().unwrap_or(0),
                    name: s["name"].as_str().unwrap_or("").to_string(),
                    // powerpoint reports points as floats; tenths are plenty
                    left: (s["left"].as_f64().unwrap_or(0.0) * 10.0).round() as i64,
                    top: (s["top"].as_f64().unwrap_or(0.0) * 10.0).round() as i64,
                    width: (s["width"].as_f64().unwrap_or(0.0) * 10.0).round() as i64,
                    height: (s["height"].as_f64().unwrap_or(0.0) * 10.0).round() as i64,
                    text: s["text"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    let error = value["error"].as_str().unwrap_or("").to_string();
    (value["opened"].as_bool().unwrap_or(false), error, shapes)
}

/// only the first text that "Changed " was put before may differ.
fn compare(name: &str, before: &[ShapeFacts], after: &[ShapeFacts]) {
    assert_eq!(before.len(), after.len(), "{name}: shape count");
    let mut changed = 0;
    for (a, b) in before.iter().zip(after) {
        assert_eq!(
            (a.slide, &a.name, a.left, a.top, a.width, a.height),
            (b.slide, &b.name, b.left, b.top, b.width, b.height),
            "{name}: {}",
            a.name
        );
        if a.text != b.text {
            changed += 1;
            assert_eq!(
                b.text,
                format!("Changed {}", a.text),
                "{name}: only the word was added"
            );
        }
    }
    assert!(changed <= 1, "{name}: {changed} texts changed");
}

#[test]
#[ignore = "needs Microsoft PowerPoint on windows"]
fn powerpoint_opens_our_edited_powerpoint_decks_unchanged_but_for_the_word() {
    let originals = decks("pp_");
    let edited: Vec<PathBuf> = originals.iter().map(|p| edit_one_word(p)).collect();
    let all: Vec<PathBuf> = originals.iter().chain(&edited).cloned().collect();
    let facts = powerpoint_facts(&all);
    assert_eq!(facts.len(), all.len(), "powerpoint answered for every deck");
    let (before, after) = facts.split_at(originals.len());
    for (index, path) in originals.iter().enumerate() {
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(
            after[index].0,
            "{name}: powerpoint could not open our file: {}",
            after[index].1
        );
        compare(&name, &before[index].2, &after[index].2);
        println!(
            "{name}: opened, {} shapes, positions and text as expected",
            after[index].2.len()
        );
    }
}

/// libreoffice's command line program: LETTERS_SOFFICE, else the usual
/// install folders on each system (the same places as
/// letters_office::bridge::libreoffice_path, which this crate doesn't use)
fn soffice() -> std::path::PathBuf {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(path) = std::env::var("LETTERS_SOFFICE") {
        candidates.push(path.into());
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates
            .push(std::path::Path::new(&local).join(r"Programs\LibreOffice\program\soffice.com"));
    }
    candidates.push(r"C:\Program Files\LibreOffice\program\soffice.com".into());
    candidates.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into());
    candidates.push("/usr/bin/soffice".into());
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .expect("libreoffice was not found; set LETTERS_SOFFICE to its soffice program")
}

/// converts decks to odp with a private-profile headless libreoffice,
/// waiting for it to finish.
fn libreoffice_to_odp(decks: &[PathBuf], folder: &Path) {
    let profile = out().join("lo_profile");
    let profile_url = format!("file:///{}", profile.to_string_lossy().replace('\\', "/"));
    let status = Command::new(soffice())
        .arg(format!("-env:UserInstallation={profile_url}"))
        .args([
            "--headless",
            "--norestore",
            "--convert-to",
            "odp",
            "--outdir",
        ])
        .arg(folder)
        .args(decks)
        .status()
        .expect("libreoffice runs");
    assert!(status.success(), "libreoffice converted");
}

/// x, y, width, height and text of a frame or shape, as written.
type OdpShape = (String, String, String, String, String);

/// per page: every frame or shape.
fn odp_facts(path: &Path) -> Vec<Vec<OdpShape>> {
    let package = letters_slides::package::Package::read(&std::fs::read(path).unwrap()).unwrap();
    let content = String::from_utf8_lossy(package.bytes("content.xml").unwrap()).to_string();
    let document = xml::parse(&content).unwrap();
    let mut pages = Vec::new();
    document.root.walk(&mut |element| {
        if element.name == "draw:page" {
            let shapes = element
                .elements()
                .map(|shape| {
                    let get = |name: &str| shape.attr(name).unwrap_or("").to_string();
                    (
                        get("svg:x"),
                        get("svg:y"),
                        get("svg:width"),
                        get("svg:height"),
                        shape.deep_text(),
                    )
                })
                .collect();
            pages.push(shapes);
        }
    });
    pages
}

#[test]
#[ignore = "needs LibreOffice"]
fn libreoffice_opens_our_edited_libreoffice_decks_unchanged_but_for_the_word() {
    let originals = decks("lo_");
    let edited: Vec<PathBuf> = originals.iter().map(|p| edit_one_word(p)).collect();
    let before_folder = out().join("lo_before");
    let after_folder = out().join("lo_after");
    libreoffice_to_odp(&originals, &before_folder);
    libreoffice_to_odp(&edited, &after_folder);
    for path in &originals {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let before = odp_facts(&before_folder.join(format!("{stem}.odp")));
        let after = odp_facts(&after_folder.join(format!("{stem}.odp")));
        assert_eq!(before.len(), after.len(), "{stem}: pages");
        let mut changed = 0;
        for (a, b) in before.iter().zip(&after) {
            assert_eq!(a.len(), b.len(), "{stem}: shapes on a page");
            for (x, y) in a.iter().zip(b) {
                assert_eq!(
                    (&x.0, &x.1, &x.2, &x.3),
                    (&y.0, &y.1, &y.2, &y.3),
                    "{stem}: position"
                );
                if x.4 != y.4 {
                    changed += 1;
                    assert!(y.4.contains("Changed "), "{stem}: only the word changed");
                }
            }
        }
        assert!(changed <= 1, "{stem}: {changed} texts changed");
        println!(
            "{stem}: libreoffice opened it, {} pages, positions and text as expected",
            after.len()
        );
    }
}

#[test]
#[ignore = "needs Microsoft PowerPoint on windows"]
fn powerpoint_opens_decks_converted_from_odp() {
    let mut written = Vec::new();
    let folder = out().join("from_odp");
    let _ = std::fs::create_dir_all(&folder);
    let mut odp_decks: Vec<PathBuf> = std::fs::read_dir(root().join("tests/decks"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "odp"))
        .collect();
    odp_decks.sort();
    for path in odp_decks {
        let deck = letters_slides::read_odp(&std::fs::read(&path).unwrap()).unwrap();
        let target = folder.join(format!(
            "{}.pptx",
            path.file_stem().unwrap().to_string_lossy()
        ));
        std::fs::write(&target, write_pptx(&deck).unwrap()).unwrap();
        written.push(target);
    }
    let facts = powerpoint_facts(&written);
    let mut failures = Vec::new();
    for (path, (opened, error, shapes)) in written.iter().zip(&facts) {
        println!(
            "{}: opened {opened}, {} shapes {error}",
            path.display(),
            shapes.len()
        );
        if !opened {
            failures.push(path.clone());
        }
    }
    assert!(
        failures.is_empty(),
        "powerpoint could not open {failures:?}"
    );
}
#[test]
#[ignore = "needs Microsoft PowerPoint on windows"]
fn powerpoint_opens_a_new_deck_made_from_nothing() {
    let mut deck = letters_slides::blank::blank_presentation();
    let mut slide =
        letters_slides::blank::new_slide(&deck, "slide-1", letters_slides::blank::CONTENT_LAYOUT);
    if let Some(text) = slide.objects[0].text_mut() {
        *text = letters_slides::model::TextBody::from_text("A new deck");
    }
    deck.slides.push(slide);
    let _ = std::fs::create_dir_all(out());
    let path = out().join("new_deck.pptx");
    std::fs::write(&path, write_pptx(&deck).unwrap()).unwrap();
    let facts = powerpoint_facts(std::slice::from_ref(&path));
    println!("{facts:?}");
    assert!(facts[0].0, "powerpoint opened the new deck: {}", facts[0].1);
}
