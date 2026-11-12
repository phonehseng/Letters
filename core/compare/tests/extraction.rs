//! reading letters with lopdf: positions agree with pdfium's own, and the
//! harder parts of pdf text (spacing operators, two-byte fonts with a
//! /ToUnicode map, forms drawn inside a page) are understood.

mod support;

use letters_compare::*;
use support::*;

fn word<'a>(words: &'a [PageWord], text: &str) -> &'a PageWord {
    words
        .iter()
        .find(|word| word.text == text)
        .unwrap_or_else(|| {
            panic!(
                "{text:?} not among {:?}",
                words.iter().map(|w| &w.text).collect::<Vec<_>>()
            )
        })
}

#[test]
fn word_positions_agree_with_pdfium() {
    let spec = pdf(vec![page(vec![
        text(72.0, 100.0, 12.0, "The quick brown fox"),
        text(300.0, 400.0, 20.0, "Large Words").face(Face::HelveticaBold),
        text(90.0, 600.0, 9.0, "small serif type").face(Face::Times),
    ])]);
    let dir = temp_dir("positions");
    let doc = TestDoc::write(&start_worker(), &dir, "positions.pdf", &spec);
    let words = read_words(&doc.source(), 0).unwrap();
    for text in [
        "The", "quick", "brown", "fox", "Large", "Words", "small", "serif", "type",
    ] {
        let ours = word(&words, text).rects[0];
        let theirs = doc.word_rect(0, text);
        assert!(
            lands_on(&[ours], theirs, 1.5),
            "{text}: ours {ours:?}, pdfium {theirs:?}"
        );
    }
}

#[test]
fn spacing_operators_move_letters_as_pdfium_does() {
    // character spacing, horizontal scaling, a kerning gap in TJ, and a
    // text matrix that scales the font: all change where letters land
    let raw = "BT /F1 12 Tf 3 Tc 150 Tz 80 600 Td [(Wide) -2000 (gap)] TJ ET \
               BT /F1 1 Tf 2 0 0 2 80 500 Tm 0 Tc 100 Tz 10 TL (scaled) Tj T* (next) Tj ET";
    let spec = pdf(vec![page(vec![Item::Raw(raw.to_owned())])]);
    let dir = temp_dir("spacing");
    let doc = TestDoc::write(&start_worker(), &dir, "spacing.pdf", &spec);
    let source = doc.source();
    let words = read_words(&source, 0).unwrap();
    for text in ["Wide", "gap"] {
        let ours = word(&words, text).rects[0];
        let theirs = doc.word_rect(0, text);
        // character spacing adds space after every letter, which pdfium
        // leaves out of its box, so the right edge may sit a little further
        assert!(
            (ours.x - theirs.x).abs() < 2.0,
            "{text}: ours {ours:?}, pdfium {theirs:?}"
        );
        assert!(
            ours.right() >= theirs.right() - 2.0,
            "{text}: ours {ours:?}, pdfium {theirs:?}"
        );
    }
    // text scaled by the text matrix is reported at the size it is drawn at
    let glyphs = source.glyphs(0).unwrap();
    let scaled = glyphs.iter().find(|glyph| glyph.ch == 's').unwrap();
    assert!((scaled.font_size - 2.0).abs() < 0.01, "{scaled:?}");
    // T* moves down by the leading times the matrix: 10 * 2 = 20 points
    let next = word(&words, "next").rects[0];
    let scaled_rect = word(&words, "scaled").rects[0];
    assert!(
        (next.y - scaled_rect.y - 20.0).abs() < 0.5,
        "{next:?} {scaled_rect:?}"
    );
}

#[test]
fn two_byte_fonts_are_read_through_their_unicode_map() {
    let to_unicode = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
                      1 begincodespacerange <0000> <FFFF> endcodespacerange \
                      2 beginbfchar <0011> <0048> <0022> <0069> endbfchar \
                      endcmap end end";
    let objects = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_owned()),
        (2, "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_owned()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_owned(),
        ),
        (4, stream_body("", "BT /F1 20 Tf 100 700 Td <00110022> Tj ET")),
        (
            5,
            "<< /Type /Font /Subtype /Type0 /BaseFont /ABCDEF+Mystery /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 7 0 R >>".to_owned(),
        ),
        (
            6,
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /ABCDEF+Mystery /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /DW 1000 /W [17 [700] 34 [300]] /FontDescriptor 8 0 R >>".to_owned(),
        ),
        (7, stream_body("", to_unicode)),
        (
            8,
            "<< /Type /FontDescriptor /FontName /ABCDEF+Mystery /Flags 4 /Ascent 800 /Descent -200 /FontWeight 700 /ItalicAngle 0 /StemV 80 /FontBBox [0 -200 1000 800] >>".to_owned(),
        ),
    ];
    let source = LopdfSource::from_bytes(&raw_pdf(objects)).unwrap();
    let words = read_words(&source, 0).unwrap();
    assert_eq!(words.len(), 1);
    assert_eq!(words[0].text, "Hi");
    // H is 700/1000 of 20 points wide and i is 300/1000: 20 points in all
    let rect = words[0].rects[0];
    assert!(
        (rect.x - 100.0).abs() < 0.01 && (rect.width - 20.0).abs() < 0.01,
        "{rect:?}"
    );
    // from the ascent (800) to the descent (-200) at 20 points
    assert!(
        (rect.y - (792.0 - 700.0 - 16.0)).abs() < 0.01 && (rect.height - 20.0).abs() < 0.01,
        "{rect:?}"
    );
    let glyph = &source.glyphs(0).unwrap()[0];
    assert_eq!(&*glyph.font_name, "Mystery");
    assert!(glyph.bold, "a font weight of 700 is bold");
}

#[test]
fn text_inside_a_form_is_found_where_it_is_drawn() {
    let objects = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R >>".to_owned()),
        (2, "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_owned()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /XObject << /Fm0 5 0 R >> >> >>".to_owned(),
        ),
        (4, stream_body("", "q 1 0 0 1 100 200 cm /Fm0 Do Q BT /F1 12 Tf 72 72 Td (outside) Tj ET")),
        (
            5,
            stream_body(
                "/Type /XObject /Subtype /Form /BBox [0 0 300 100] /Matrix [1 0 0 1 10 0] /Resources << /Font << /F1 6 0 R >> >>",
                "BT /F1 12 Tf 0 0 Td (Inside) Tj ET",
            ),
        ),
        (6, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_owned()),
    ];
    let dir = temp_dir("form");
    let doc = TestDoc::write_bytes(&start_worker(), &dir, "form.pdf", &raw_pdf(objects));
    let words = read_words(&doc.source(), 0).unwrap();
    // "outside" uses a font the page doesn't have, so it can't be placed
    assert_eq!(words.len(), 1, "{words:?}");
    let inside = words[0].rects[0];
    assert!(
        lands_on(&[inside], doc.word_rect(0, "Inside"), 1.5),
        "{inside:?} vs {:?}",
        doc.word_rect(0, "Inside")
    );
    assert!((inside.x - 110.0).abs() < 0.01);
}

#[test]
fn text_outside_the_crop_box_is_not_on_the_page() {
    let mut cropped = page(vec![
        text(72.0, 100.0, 12.0, "Visible words"),
        text(72.0, 780.0, 12.0, "Hidden words"),
    ]);
    cropped.crop = Some([0.0, 36.0, 612.0, 792.0]);
    let source = LopdfSource::from_bytes(&pdf_bytes(&pdf(vec![cropped]))).unwrap();
    let words: Vec<String> = read_words(&source, 0)
        .unwrap()
        .into_iter()
        .map(|word| word.text)
        .collect();
    assert_eq!(words, ["Visible", "words"]);
}
