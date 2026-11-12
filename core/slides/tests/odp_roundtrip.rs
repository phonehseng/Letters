//! opendocument round trips: libreoffice's own decks read and written
//! without edits keep every part, one retyped word changes only that
//! object, and pptx decks converted to odp and read back keep their
//! slides, text and positions to the emu.

use std::path::{Path, PathBuf};

use letters_slides::model::{Object, TextRun};
use letters_slides::package::Package;
use letters_slides::{read_odp, read_pptx, write_odp};

fn decks(extension: &str) -> Vec<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/decks");
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == extension))
        .collect();
    found.sort();
    found
}

fn without_shadow(bytes: &[u8]) -> Vec<u8> {
    let mut parts = Package::read(bytes).unwrap().parts;
    parts.retain(|p| !p.name.starts_with("Letters/"));
    letters_slides::package::write_zip(&parts, &["mimetype"]).unwrap()
}

fn part(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    Package::read(bytes)
        .unwrap()
        .parts
        .into_iter()
        .find(|p| p.name == name)
        .map(|p| p.bytes)
}

#[test]
fn saving_odp_without_edits_keeps_every_part() {
    for path in decks("odp") {
        let original = std::fs::read(&path).unwrap();
        let deck = read_odp(&original).unwrap();
        assert!(!deck.slides.is_empty(), "{}", path.display());
        let (written, _) = write_odp(&deck).unwrap();
        for entry in Package::read(&original).unwrap().parts {
            if entry.name == "META-INF/manifest.xml" {
                continue;
            }
            assert_eq!(
                part(&written, &entry.name),
                Some(entry.bytes),
                "{}: {}",
                path.display(),
                entry.name
            );
        }
    }
}

fn retype_first_word(objects: &mut [Object]) -> Option<usize> {
    for (index, object) in objects.iter_mut().enumerate() {
        let Some(text) = object.text_mut() else {
            continue;
        };
        for run in text.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
            if let TextRun::Text { text, .. } = run {
                *text = format!("Changed {text}");
                return Some(index);
            }
        }
    }
    None
}

#[test]
fn retyping_one_word_in_odp_changes_only_that_object() {
    for path in decks("odp") {
        let original = std::fs::read(&path).unwrap();
        let mut deck = read_odp(&original).unwrap();
        let Some(edited) = retype_first_word(&mut deck.slides[0].objects) else {
            continue;
        };
        let (written, _) = write_odp(&deck).unwrap();
        let reread = read_odp(&written).unwrap();
        let before = read_odp(&original).unwrap();
        let (a, b) = (&before.slides[0].objects, &reread.slides[0].objects);
        assert_eq!(a.len(), b.len(), "{}", path.display());
        for (index, (x, y)) in a.iter().zip(b).enumerate() {
            let (x, y) = (x.source().map(|s| &s.xml), y.source().map(|s| &s.xml));
            if index == edited {
                assert_ne!(x, y);
                assert!(
                    b[index]
                        .text()
                        .unwrap()
                        .plain_text()
                        .starts_with("Changed "),
                    "{}",
                    path.display()
                );
            } else {
                assert_eq!(x, y, "{}: object {index}", path.display());
            }
        }
        for index in 1..before.slides.len() {
            assert_eq!(
                before.slides[index].source.as_ref().map(|s| &s.xml),
                reread.slides[index].source.as_ref().map(|s| &s.xml)
            );
        }
    }
}

/// each object's text with its box (x, y, width, height), on one slide.
type SlideFacts = Vec<(String, Option<(i64, i64, i64, i64)>)>;

/// the text and box of every object with text, slide by slide.
fn facts(deck: &letters_slides::Presentation) -> Vec<SlideFacts> {
    deck.slides
        .iter()
        .map(|slide| {
            slide
                .objects
                .iter()
                .filter_map(|o| {
                    let text = o.text()?.plain_text();
                    Some((text, o.transform().map(|t| (t.x, t.y, t.width, t.height))))
                })
                .collect()
        })
        .collect()
}

#[test]
fn pptx_decks_survive_conversion_to_odp_with_text_and_exact_positions() {
    for path in decks("pptx")
        .into_iter()
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("pp_"))
    {
        let deck = read_pptx(&std::fs::read(&path).unwrap()).unwrap();
        let (odp, _) = write_odp(&deck).unwrap();
        // read the odp content itself, not the shadow copy of the pptx
        let back = read_odp(&without_shadow(&odp)).unwrap();
        assert_eq!(deck.slides.len(), back.slides.len(), "{}", path.display());
        let (before, after) = (facts(&deck), facts(&back));
        for (slide, (a, b)) in before.iter().zip(&after).enumerate() {
            let texts = |list: &SlideFacts| list.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
            assert_eq!(texts(a), texts(b), "{} slide {slide}", path.display());
            for ((_, box_a), (_, box_b)) in a.iter().zip(b) {
                if let (Some(box_a), Some(box_b)) = (box_a, box_b) {
                    assert_eq!(box_a, box_b, "{} slide {slide}", path.display());
                }
            }
        }
    }
}

#[test]
fn broken_odp_files_give_errors_not_panics() {
    let good = std::fs::read(&decks("odp")[0]).unwrap();
    let mut inputs = vec![Vec::new(), b"PK\x03\x04".to_vec()];
    for cut in [30, 300, good.len() / 3, good.len() - 5] {
        inputs.push(good[..cut].to_vec());
    }
    for step in [101usize, 997] {
        let mut damaged = good.clone();
        for index in (step..damaged.len()).step_by(step * 5) {
            damaged[index] ^= 0x33;
        }
        inputs.push(damaged);
    }
    for input in inputs {
        let result = std::panic::catch_unwind(|| read_odp(&input).map(|deck| write_odp(&deck)));
        assert!(result.is_ok(), "reading must not panic");
    }
}

#[test]
fn pptx_to_odp_and_back_through_letters_is_the_original_deck() {
    for path in decks("pptx")
        .into_iter()
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("pp_"))
    {
        let original = std::fs::read(&path).unwrap();
        let deck = read_pptx(&original).unwrap();
        let (odp, _) = write_odp(&deck).unwrap();
        let back = letters_slides::write_pptx(&read_odp(&odp).unwrap()).unwrap();
        let slides = |bytes: &[u8]| {
            read_pptx(bytes)
                .unwrap()
                .slides
                .iter()
                .map(|s| s.source.as_ref().unwrap().xml.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(slides(&original), slides(&back), "{}", path.display());
    }
}

#[test]
fn odp_to_pptx_and_back_through_letters_is_the_original_deck() {
    for path in decks("odp") {
        let original = std::fs::read(&path).unwrap();
        let pptx = letters_slides::write_pptx(&read_odp(&original).unwrap()).unwrap();
        let (back, _) = write_odp(&read_pptx(&pptx).unwrap()).unwrap();
        assert_eq!(
            part(&original, "content.xml"),
            part(&back, "content.xml"),
            "{}",
            path.display()
        );
        assert_eq!(
            part(&original, "styles.xml"),
            part(&back, "styles.xml"),
            "{}",
            path.display()
        );
    }
}

#[test]
fn an_odp_edited_after_conversion_is_read_as_it_now_is() {
    let path = decks("pptx")
        .into_iter()
        .find(|p| p.ends_with("pp_title_bullets.pptx"))
        .unwrap();
    let deck = read_pptx(&std::fs::read(&path).unwrap()).unwrap();
    let (odp, _) = write_odp(&deck).unwrap();
    // another application changes a word inside the first page
    let mut parts = Package::read(&odp).unwrap().parts;
    for entry in &mut parts {
        if entry.name == "content.xml" {
            let text = String::from_utf8(entry.bytes.clone()).unwrap().replacen(
                "Photosynthesis",
                "Respiration",
                1,
            );
            entry.bytes = text.into_bytes();
        }
    }
    let edited = letters_slides::package::write_zip(&parts, &["mimetype"]).unwrap();
    let reread = read_odp(&edited).unwrap();
    assert!(reread.slides[0].objects.iter().any(|o| o
        .text()
        .is_some_and(|t| t.plain_text().contains("Respiration"))));
}

/// libreoffice's title-slide master keeps a subtitle placeholder that a
/// pptx master can't have; it becomes a hidden text box, so its prompt
/// isn't drawn over each title slide's own subtitle as libreoffice never does.
#[test]
fn a_masters_extra_placeholder_is_kept_but_not_drawn() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/decks/lo_theme_facet.odp");
    let deck = read_odp(&std::fs::read(path).unwrap()).unwrap();
    let prompts: Vec<&Object> = deck
        .masters
        .iter()
        .flat_map(|master| &master.objects)
        .filter(|object| {
            object
                .text()
                .is_some_and(|t| t.plain_text().contains("subtitle"))
        })
        .collect();
    assert!(!prompts.is_empty(), "the subtitle frame is kept");
    for prompt in prompts {
        let Object::Shape(shape) = prompt else {
            panic!("a text box")
        };
        assert!(shape.hidden && shape.placeholder.is_none());
    }
}
