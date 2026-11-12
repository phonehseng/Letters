//! round trips of real decks made by powerpoint and by libreoffice
//! (tests/decks): saving without edits keeps every part byte for byte,
//! and editing one word leaves every other object's xml unchanged.

use std::path::{Path, PathBuf};

use letters_slides::model::{Object, TextRun};
use letters_slides::package::Package;
use letters_slides::{read_pptx, write_pptx};

fn decks(extension: &str) -> Vec<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/decks");
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
        .expect("tests/decks exists")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == extension))
        .collect();
    found.sort();
    found
}

fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut parts: Vec<(String, Vec<u8>)> = Package::read(bytes)
        .unwrap()
        .parts
        .into_iter()
        .map(|part| (part.name, part.bytes))
        .collect();
    parts.sort();
    parts
}

#[test]
fn saving_without_edits_keeps_every_part_byte_for_byte() {
    for path in decks("pptx") {
        let original = std::fs::read(&path).unwrap();
        let deck = read_pptx(&original).unwrap();
        let written = write_pptx(&deck).unwrap();
        let before = parts(&original);
        let after = parts(&written);
        let names =
            |list: &[(String, Vec<u8>)]| list.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(&before),
            names(&after),
            "{}: the same parts",
            path.display()
        );
        for ((name, a), (_, b)) in before.iter().zip(&after) {
            assert!(a == b, "{}: {name} changed", path.display());
        }
    }
}

/// the first text run of the deck, as (slide, object index).
fn first_text_object(objects: &[Object]) -> Option<usize> {
    objects.iter().position(|object| {
        object.text().is_some_and(|text| {
            text.paragraphs
                .iter()
                .any(|p| p.runs.iter().any(|r| matches!(r, TextRun::Text { .. })))
        })
    })
}

fn retype_first_word(object: &mut Object) {
    let text = object.text_mut().unwrap();
    for paragraph in &mut text.paragraphs {
        for run in &mut paragraph.runs {
            if let TextRun::Text { text, .. } = run {
                *text = format!("Changed {text}");
                return;
            }
        }
    }
}

#[test]
fn editing_one_word_leaves_every_other_object_byte_identical() {
    for path in decks("pptx") {
        let original = std::fs::read(&path).unwrap();
        let mut deck = read_pptx(&original).unwrap();
        let Some((slide_index, object_index)) = deck
            .slides
            .iter()
            .enumerate()
            .find_map(|(index, slide)| first_text_object(&slide.objects).map(|o| (index, o)))
        else {
            continue;
        };
        retype_first_word(&mut deck.slides[slide_index].objects[object_index]);
        let written = write_pptx(&deck).unwrap();
        let reread = read_pptx(&written).unwrap();
        let before = &read_pptx(&original).unwrap().slides[slide_index];
        let after = &reread.slides[slide_index];
        assert_eq!(
            before.objects.len(),
            after.objects.len(),
            "{}",
            path.display()
        );
        for (index, (a, b)) in before.objects.iter().zip(&after.objects).enumerate() {
            let (a_xml, b_xml) = (a.source().map(|s| &s.xml), b.source().map(|s| &s.xml));
            if index == object_index {
                assert_ne!(
                    a_xml,
                    b_xml,
                    "{}: the edited object changed",
                    path.display()
                );
                assert!(after.objects[index]
                    .text()
                    .unwrap()
                    .plain_text()
                    .starts_with("Changed "));
            } else {
                assert_eq!(
                    a_xml,
                    b_xml,
                    "{}: object {index} kept its xml",
                    path.display()
                );
            }
        }
        // every other slide is untouched too
        for (index, slide) in reread
            .slides
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != slide_index)
        {
            let original_slide = &deck.slides[index];
            assert_eq!(
                slide.source.as_ref().map(|s| &s.xml),
                original_slide.source.as_ref().map(|s| &s.xml),
                "{}: slide {index}",
                path.display()
            );
        }
    }
}

#[test]
fn an_edited_object_keeps_its_geometry_and_properties_xml() {
    let path = decks("pptx")
        .into_iter()
        .find(|p| p.ends_with("pp_shapes.pptx"))
        .unwrap();
    let mut deck = read_pptx(&std::fs::read(&path).unwrap()).unwrap();
    let index = first_text_object(&deck.slides[0].objects).unwrap();
    let before = deck.slides[0].objects[index].source().unwrap().xml.clone();
    retype_first_word(&mut deck.slides[0].objects[index]);
    let reread = read_pptx(&write_pptx(&deck).unwrap()).unwrap();
    let after = reread.slides[0].objects[index]
        .source()
        .unwrap()
        .xml
        .clone();
    let properties =
        |xml: &str| xml[xml.find("<p:spPr").unwrap()..xml.find("</p:spPr>").unwrap()].to_string();
    assert_eq!(properties(&before), properties(&after));
}

#[test]
fn moving_a_shape_rewrites_only_its_position() {
    let path = decks("pptx")
        .into_iter()
        .find(|p| p.ends_with("pp_shapes.pptx"))
        .unwrap();
    let mut deck = read_pptx(&std::fs::read(&path).unwrap()).unwrap();
    let before = deck.slides[0].objects[0].source().unwrap().xml.clone();
    if let Some(Some(transform)) = deck.slides[0].objects[0].transform_mut() {
        transform.x += 12_700;
    }
    let reread = read_pptx(&write_pptx(&deck).unwrap()).unwrap();
    let after = reread.slides[0].objects[0].source().unwrap().xml.clone();
    let without_offset = |xml: &str| {
        let start = xml.find("<a:off").unwrap();
        let end = start + xml[start..].find("/>").unwrap();
        format!("{}{}", &xml[..start], &xml[end..])
    };
    assert_ne!(before, after);
    assert_eq!(without_offset(&before), without_offset(&after));
}

#[test]
fn broken_files_give_errors_not_panics() {
    let good = std::fs::read(&decks("pptx")[0]).unwrap();
    let mut inputs: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"hello".to_vec(),
        vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1],
    ];
    for cut in [10, 100, 1000, good.len() / 2, good.len() - 10] {
        inputs.push(good[..cut].to_vec());
    }
    // flipping bytes inside the compressed data
    for step in [97usize, 389, 1021] {
        let mut damaged = good.clone();
        for index in (step..damaged.len()).step_by(step * 7) {
            damaged[index] ^= 0x5A;
        }
        inputs.push(damaged);
    }
    for input in inputs {
        let result = std::panic::catch_unwind(|| read_pptx(&input).map(|deck| write_pptx(&deck)));
        assert!(result.is_ok(), "reading must not panic");
    }
}
