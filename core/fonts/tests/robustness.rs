//! fonts are untrusted input: truncated, flipped and random bytes must be
//! refused with a sentence, never a panic.

mod support;

use letters_fonts::container::to_plain_font;
use letters_fonts::custom::validate;
use letters_fonts::facts::read_facts;
use letters_fonts::{embedding_data, preview_svg};
use support::{fixture, temporary};

/// a small, repeatable stream of pseudo-random numbers (no extra crate).
struct Numbers(u64);

impl Numbers {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// every way the engine reads a font, on one set of bytes.
fn read_everything(bytes: &[u8]) {
    if let Ok(plain) = to_plain_font(bytes.to_vec(), "fuzz") {
        for index in 0..letters_fonts::sfnt::face_count(&plain).min(4) {
            let _ = read_facts(&plain, index);
            let _ = preview_svg(&plain, index, "Hamburg ĐỒNG ₫", 24.0, &[]);
            let _ = embedding_data(&plain, index, "Hamburg");
        }
    }
}

#[test]
fn truncated_fonts_never_panic() {
    for name in [
        "AtkinsonHyperlegible-Regular.ttf",
        "atkinson-latin-400.woff",
        "atkinson-latin-400.woff2",
    ] {
        let bytes = std::fs::read(fixture(name)).unwrap();
        let step = (bytes.len() / 150).max(1);
        for length in (0..bytes.len()).step_by(step) {
            read_everything(&bytes[..length]);
        }
    }
}

#[test]
fn flipped_bytes_never_panic() {
    let mut numbers = Numbers(7);
    for name in [
        "AtkinsonHyperlegible-Regular.ttf",
        "atkinson-latin-400.woff",
        "atkinson-latin-400.woff2",
    ] {
        let original = std::fs::read(fixture(name)).unwrap();
        for _ in 0..120 {
            let mut bytes = original.clone();
            for _ in 0..1 + numbers.next() % 8 {
                let at = (numbers.next() as usize) % bytes.len();
                bytes[at] = numbers.next() as u8;
            }
            read_everything(&bytes);
        }
    }
}

#[test]
fn random_bytes_with_font_headers_never_panic() {
    let mut numbers = Numbers(11);
    for header in [&[0u8, 1, 0, 0][..], b"OTTO", b"ttcf", b"wOFF", b"wOF2"] {
        for _ in 0..80 {
            let length = 4 + (numbers.next() % 2000) as usize;
            let mut bytes: Vec<u8> = (0..length).map(|_| numbers.next() as u8).collect();
            bytes[..4].copy_from_slice(header);
            read_everything(&bytes);
        }
    }
}

#[test]
fn broken_files_are_explained() {
    let folder = temporary("broken");
    let original = std::fs::read(fixture("AtkinsonHyperlegible-Regular.ttf")).unwrap();
    let cut = folder.join("Cut.ttf");
    std::fs::write(&cut, &original[..900]).unwrap();
    let error = validate(&cut).unwrap_err();
    assert_eq!(
        error.to_string(),
        "“Cut.ttf” is damaged and can’t be used. Download the font again from where you got it."
    );
    let renamed = folder.join("photo.ttf");
    std::fs::write(&renamed, b"\x89PNG\r\n\x1a\nnot a font").unwrap();
    assert!(validate(&renamed)
        .unwrap_err()
        .to_string()
        .starts_with("“photo.ttf” isn’t a font file"));
    let woff2 = std::fs::read(fixture("atkinson-latin-400.woff2")).unwrap();
    let cut_woff2 = folder.join("cut.woff2");
    std::fs::write(&cut_woff2, &woff2[..woff2.len() / 2]).unwrap();
    assert!(validate(&cut_woff2)
        .unwrap_err()
        .to_string()
        .contains("is damaged"));
    std::fs::remove_dir_all(&folder).ok();
}
