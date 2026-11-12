//! the product owner's case: adding a sentence to a corporate letter
//! without changing its fonts, spacing or colours. the page has a logo
//! made of paths, a justified paragraph in an embedded subset of Arial
//! set in a spot colour, and a footer repeated on the next page. the new
//! sentence has characters the subset lacks.
//!
//! the tests print what they measure (run with --nocapture to see it).

mod support;

use std::collections::BTreeMap;

use letters_edit::{
    extend_font, page_objects, reflow_paragraph, EditConsent, PageObjects, ReflowOptions,
    TextAlignment, TextLine,
};
use support::subset;
use support::{load, render, save, Image, PageSetup, PdfBuilder};

const BODY: &str = "Thank you for your order of last week. We have reviewed the details you sent \
and can confirm that every item is in stock and ready to ship. Our team will contact you by \
phone to agree on a delivery date that suits you, and we will send the invoice once the goods \
have left our warehouse. Please keep this letter for your records.";
const ADDED: &str = " From 1 July, the café opens at 7 am — ask Quentin for the €5 voucher.";
const FOOTER: &str = "Example Corporation Ltd, registered office: High Street, Leeds";
const SIZE: f32 = 11.0;
const LEADING: f32 = 15.0;
const WIDTH: f32 = 468.0;

fn installed(name: &str) -> Option<Vec<u8>> {
    let path = std::path::Path::new("C:/Windows/Fonts").join(name);
    std::fs::read(path).ok()
}

/// the font as a composite font with Identity codes, the way word
/// processors embed TrueType: the subset program, widths and /ToUnicode.
fn composite_font(pdf: &mut PdfBuilder, program: &[u8], text: &str, name: &str) -> usize {
    let glyphs = subset::glyphs_for(program, text);
    let file = pdf.stream(&format!("/Length1 {}", program.len()), program);
    let descriptor = pdf.add(format!(
        "<< /Type /FontDescriptor /FontName /{name} /Flags 32 /FontBBox [-665 -325 2000 1006] \
         /ItalicAngle 0 /Ascent 905 /Descent -212 /CapHeight 716 /StemV 80 /FontFile2 {file} 0 R >>"
    ));
    let widths: String = glyphs
        .values()
        .collect::<std::collections::BTreeSet<_>>()
        .iter()
        .map(|glyph| format!("{glyph} [{}] ", subset::advance(program, **glyph)))
        .collect();
    let descendant = pdf.add(format!(
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) \
         /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /W [{widths}] /CIDToGIDMap /Identity >>"
    ));
    let entries: String = glyphs
        .iter()
        .map(|(c, glyph)| format!("<{glyph:04X}> <{:04X}>\n", *c as u32))
        .collect();
    let map = format!(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapName /Adobe-Identity-UCS def \
         1 begincodespacerange <0000> <FFFF> endcodespacerange {} beginbfchar\n{entries}endbfchar endcmap \
         CMapName currentdict /CMap defineresource pop end end",
        glyphs.len()
    );
    let to_unicode = pdf.stream("", map.as_bytes());
    pdf.add(format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H \
         /DescendantFonts [{descendant} 0 R] /ToUnicode {to_unicode} 0 R >>"
    ))
}

fn hex(glyphs: &BTreeMap<char, u16>, text: &str) -> String {
    let codes: String = text
        .chars()
        .map(|c| format!("{:04X}", glyphs[&c]))
        .collect();
    format!("<{codes}>")
}

/// greedy lines of the paragraph at the given width.
fn lines_of(font: &[u8], text: &str) -> Vec<String> {
    let glyphs = subset::glyphs_for(font, text);
    let width = |piece: &str| {
        piece
            .chars()
            .map(|c| subset::advance(font, glyphs[&c]) * SIZE / 1000.0)
            .sum::<f32>()
    };
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if current.is_empty() || width(&candidate) <= WIDTH {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_owned();
        }
    }
    lines.push(current);
    lines
}

/// the paragraph justified with TJ numbers after each space, the way
/// composite fonts are justified (word spacing doesn't apply to them).
fn justified_paragraph(font: &[u8]) -> String {
    let glyphs = subset::glyphs_for(font, BODY);
    let width = |piece: &str| {
        piece
            .chars()
            .map(|c| subset::advance(font, glyphs[&c]) * SIZE / 1000.0)
            .sum::<f32>()
    };
    let lines = lines_of(font, BODY);
    let mut out = format!("BT /CS0 cs 1 scn /F1 {SIZE} Tf {LEADING} TL 72 640 Td\n");
    for (number, line) in lines.iter().enumerate() {
        let spaces = line.matches(' ').count();
        let last = number + 1 == lines.len();
        let extra = if last || spaces == 0 {
            0.0
        } else {
            (WIDTH - width(line)) / spaces as f32
        };
        let adjust = -extra * 1000.0 / SIZE;
        let words: Vec<String> = line.split(' ').map(|word| hex(&glyphs, word)).collect();
        let joined = words.join(&format!(" {} {adjust:.3} ", hex(&glyphs, " ")));
        out.push_str(&format!("[{joined}] TJ T*\n"));
    }
    out.push_str("ET\n");
    out
}

/// the letterhead: a logo of four paths in one q block, in the spot colour.
const LOGO: &str = "q /CS0 cs 1 scn \
    90 758 m 99.94 758 108 749.94 108 740 c 108 730.06 99.94 722 90 722 c 80.06 722 72 730.06 72 740 c 72 749.94 80.06 758 90 758 c f \
    116 726 50 28 re f \
    0.5 g 172 726 m 196 754 l 220 726 l h f \
    /CS0 cs 0.6 scn 116 720 104 3 re f Q\n";

fn corporate_pdf(font: &[u8]) -> Vec<u8> {
    let text = format!("{BODY}{FOOTER}");
    let program = subset::subset(font, &text);
    let mut pdf = PdfBuilder::new();
    let function =
        pdf.add("<< /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [1 0.6 0 0.2] /N 1 >>");
    let spot = pdf.add(format!(
        "[/Separation /CorporateBlue /DeviceCMYK {function} 0 R]"
    ));
    let arial = composite_font(&mut pdf, &program, &text, "ABCDEF+ArialMT");
    let glyphs = subset::glyphs_for(&program, &text);
    let footer = format!("BT 0 g /F1 8 Tf 72 40 Td {} Tj ET\n", hex(&glyphs, FOOTER));
    let page_one = format!("{LOGO}{}{footer}", justified_paragraph(&program));
    let page_two = format!("{LOGO}{footer}");
    let resources = format!("/Font << /F1 {arial} 0 R >> /ColorSpace << /CS0 {spot} 0 R >>");
    pdf.page(&PageSetup::letter(), &[&page_one], &resources);
    pdf.page(&PageSetup::letter(), &[&page_two], &resources);
    pdf.finish()
}

fn body(objects: &PageObjects) -> &letters_edit::TextBlock {
    objects
        .text_blocks
        .iter()
        .find(|block| block.text.starts_with("Thank you"))
        .unwrap()
}

/// how differently two renders draw a band of rows: the share of inked
/// pixels (in either) whose colour differs by more than a quarter.
fn ink_difference(edited: &Image, reference: &Image, top: f32, bottom: f32) -> (usize, usize) {
    let inked = |pixel: [u8; 3]| pixel.iter().any(|channel| *channel < 200);
    let (mut differing, mut ink) = (0, 0);
    for y in top as u32..bottom as u32 {
        for x in 60..560u32 {
            let a = edited.pixel(x as f32, y as f32);
            let b = reference.pixel(x as f32, y as f32);
            if inked(a) || inked(b) {
                ink += 1;
                if a.iter()
                    .zip(b)
                    .any(|(p, q)| (i32::from(*p) - i32::from(q)).abs() > 64)
                {
                    differing += 1;
                }
            }
        }
    }
    (differing, ink)
}

/// the lines of the paragraph that hold the added sentence.
fn added_lines(objects: &PageObjects) -> Vec<TextLine> {
    body(objects)
        .lines
        .iter()
        .filter(|line| {
            line.text.contains("café")
                || line.text.contains("Quentin")
                || line.text.contains("voucher")
        })
        .cloned()
        .collect()
}

/// the font of resource /F1 on the first page.
fn first_page_font(doc: &lopdf::Document) -> lopdf::ObjectId {
    let page = doc.get_pages()[&1];
    let resources = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap();
    resources
        .get(b"Font")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"F1")
        .unwrap()
        .as_reference()
        .unwrap()
}

/// cid to text from a /ToUnicode map with one entry per line.
fn to_unicode_entries(doc: &lopdf::Document, font: lopdf::ObjectId) -> Vec<(u16, char)> {
    let map_id = doc
        .get_dictionary(font)
        .unwrap()
        .get(b"ToUnicode")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = doc.get_object(map_id).unwrap().as_stream().unwrap();
    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line
                .split(['<', '>', ' '])
                .filter(|part| !part.is_empty())
                .collect();
            let [cid, text] = parts[..] else { return None };
            let cid = u16::from_str_radix(cid, 16).ok()?;
            let character = char::from_u32(u32::from_str_radix(text, 16).ok()?)?;
            Some((cid, character))
        })
        .collect()
}

/// the edited file with its font program swapped for the full true font.
/// the subset kept the true font's glyph numbers, so every cid below the
/// true font's glyph count already names the right glyph; every cid the
/// extension added (past that count) is sent through a CIDToGIDMap to the
/// true font's own glyph for its character. the content stays the same,
/// so pixels can only differ where the added glyphs differ from the true
/// ones.
fn swapped_to_true_font(saved: &[u8], true_font: &[u8]) -> Vec<u8> {
    let mut doc = load(saved);
    let face = ttf_parser::Face::parse(true_font, 0).unwrap();
    let count = face.number_of_glyphs();
    let font = first_page_font(&doc);
    let mut cid_to_gid: Vec<u16> = (0..count).collect();
    for (cid, character) in to_unicode_entries(&doc, font) {
        if cid >= count {
            cid_to_gid.resize(cid_to_gid.len().max(usize::from(cid) + 1), 0);
            cid_to_gid[usize::from(cid)] = face.glyph_index(character).map_or(0, |glyph| glyph.0);
        }
    }
    let descendants = doc
        .get_dictionary(font)
        .unwrap()
        .get(b"DescendantFonts")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    let descendant = descendants[0].as_reference().unwrap();
    let descriptor = doc
        .get_dictionary(descendant)
        .unwrap()
        .get(b"FontDescriptor")
        .unwrap()
        .as_reference()
        .unwrap();
    let bytes: Vec<u8> = cid_to_gid
        .iter()
        .flat_map(|gid| gid.to_be_bytes())
        .collect();
    let map_id = doc.add_object(lopdf::Stream::new(lopdf::Dictionary::new(), bytes));
    let mut file = lopdf::Dictionary::new();
    file.set("Length1", true_font.len() as i64);
    let file_id = doc.add_object(lopdf::Stream::new(file, true_font.to_vec()));
    doc.get_dictionary_mut(descendant)
        .unwrap()
        .set("CIDToGIDMap", lopdf::Object::Reference(map_id));
    doc.get_dictionary_mut(descriptor)
        .unwrap()
        .set("FontFile2", lopdf::Object::Reference(file_id));
    save(&mut doc)
}

/// pixels of the added lines that differ between the edited page and the
/// same page drawn with the true font.
fn measure_swapped(label: &str, objects: &PageObjects, saved: &[u8], true_font: &[u8]) -> f32 {
    let edited = render(saved, 0);
    let reference = render(&swapped_to_true_font(saved, true_font), 0);
    let lines = added_lines(objects);
    let (mut differing, mut ink) = (0, 0);
    for line in &lines {
        let (d, i) = ink_difference(
            &edited,
            &reference,
            line.rect.y - 1.0,
            line.rect.bottom() + 1.0,
        );
        differing += d;
        ink += i;
    }
    let share = differing as f32 / ink.max(1) as f32;
    println!(
        "{label}: characters with the most pixels differing at all: {}",
        worst_characters(&lines, &edited, &reference)
    );
    println!(
        "{label}: {differing} of {ink} inked pixels of the added lines differ from the true font ({:.2}%)",
        share * 100.0
    );
    share
}
fn worst_characters(lines: &[TextLine], edited: &Image, reference: &Image) -> String {
    let mut counts: Vec<(usize, char)> = Vec::new();
    for line in lines {
        for run in &line.runs {
            for (character, place) in run.text.chars().zip(&run.char_boxes) {
                let region = (
                    place.x as u32,
                    place.y as u32,
                    (place.right() + 1.0) as u32,
                    (place.bottom() + 1.0) as u32,
                );
                counts.push((edited.differences_in(reference, region), character));
            }
        }
    }
    counts.sort_by_key(|(count, _)| std::cmp::Reverse(*count));
    counts
        .iter()
        .take(6)
        .map(|(count, character)| format!("{character}:{count}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// the paragraph stays justified, in its one font object and colour, and
/// the logo is untouched.
fn assert_nothing_else_changed(
    before: &PageObjects,
    after_objects: &PageObjects,
    font_before: &str,
) {
    let paragraph = body(before);
    let after = body(after_objects);
    assert_eq!(
        after.alignment,
        TextAlignment::Justified,
        "it stays justified"
    );
    for line in &after.lines {
        for run in &line.runs {
            assert_eq!(
                run.font.id, font_before,
                "every run is in the original font object"
            );
            assert_eq!(run.color, paragraph.lines[0].runs[0].color);
        }
    }
    // the logo, the footer and the other page are untouched
    let shapes = |objects: &PageObjects| -> Vec<_> {
        objects
            .paths
            .iter()
            .map(|path| (path.rect, path.fill.clone()))
            .collect()
    };
    assert_eq!(shapes(after_objects), shapes(before));
}

#[test]
fn a_sentence_with_missing_characters_is_added_in_the_same_font_and_colour() {
    let Some(arial) = installed("arial.ttf") else {
        println!("skipped: Arial isn't installed here");
        return;
    };
    let bytes = corporate_pdf(&arial);
    let mut doc = load(&bytes);
    let before = page_objects(&doc, 0).unwrap();
    let paragraph = body(&before).clone();
    assert_eq!(paragraph.alignment, TextAlignment::Justified);
    let missing: String = ADDED
        .chars()
        .filter(|c| *c != ' ' && !paragraph.lines[0].runs[0].font.editable_chars.contains(*c))
        .collect();
    assert!(
        missing.contains('é') && missing.contains('€') && missing.contains('Q'),
        "{missing}"
    );
    let font_before = paragraph.lines[0].runs[0].font.id.clone();
    let text = format!("{}{ADDED}", paragraph.text.replace('\n', " "));
    let result =
        reflow_paragraph(&mut doc, 0, &paragraph.id, &text, &ReflowOptions::default()).unwrap();
    assert!(result.applied, "{:?}", result.overflow);
    assert!(
        result.notices.is_empty(),
        "no fallback fonts: {:?}",
        result.notices
    );
    let extended = &result.summary.fonts_extended;
    assert_eq!(extended.len(), 1);
    println!(
        "extended {} with “{}” from {}",
        extended[0].font, extended[0].characters, extended[0].glyph_source
    );
    assert_nothing_else_changed(&before, &result.objects, &font_before);
    let saved = save(&mut doc);
    let content = support::page_content(&load(&saved), 0);
    assert!(!content.contains(" rg"), "no colour was converted to rgb");
    assert!(
        !support::find_text(&saved, 0, "café opens at 7 am").is_empty(),
        "the new text is searchable"
    );
    let share = measure_swapped(
        "same font (installed Arial as donor)",
        &result.objects,
        &saved,
        &arial,
    );
    assert!(
        share < 0.02,
        "the added glyphs should be the true font's: {share}"
    );
}

#[test]
fn a_look_alike_donor_is_measured_against_the_true_font() {
    let (Some(arial), Some(look_alike)) = (installed("arial.ttf"), installed("tahoma.ttf")) else {
        println!("skipped: Arial or Tahoma isn't installed here");
        return;
    };
    let mut doc = load(&corporate_pdf(&arial));
    let before = page_objects(&doc, 0).unwrap();
    let paragraph = body(&before).clone();
    let font_id = paragraph.lines[0].runs[0].font.id.clone();
    let result = extend_font(
        &mut doc,
        0,
        &font_id,
        ADDED,
        &look_alike,
        &EditConsent::default(),
    )
    .unwrap();
    assert_eq!(result.summary.fonts_extended.len(), 1);
    let extension = &result.summary.fonts_extended[0];
    println!("{}", extension.sentence);
    assert_ne!(extension.match_method, "same font", "Tahoma isn't Arial");
    let text = format!("{}{ADDED}", paragraph.text.replace('\n', " "));
    let id = body(&result.objects).id.clone();
    let result = reflow_paragraph(&mut doc, 0, &id, &text, &ReflowOptions::default()).unwrap();
    assert!(result.applied && result.notices.is_empty());
    assert!(
        result.summary.fonts_extended.is_empty(),
        "the font already had them"
    );
    let saved = save(&mut doc);
    measure_swapped(
        "near font (Tahoma fitted to Arial by letters-fonts)",
        &result.objects,
        &saved,
        &arial,
    );
}

#[test]
fn without_a_donor_missing_characters_fall_back_to_a_standard_font_and_are_reported() {
    let Some(arial) = installed("arial.ttf") else {
        println!("skipped: Arial isn't installed here");
        return;
    };
    let mut doc = load(&corporate_pdf(&arial));
    let paragraph = body(&page_objects(&doc, 0).unwrap()).clone();
    let options = ReflowOptions {
        allow_installed_fonts: false,
        ..ReflowOptions::default()
    };
    let text = format!("{}{ADDED}", paragraph.text.replace('\n', " "));
    let result = reflow_paragraph(&mut doc, 0, &paragraph.id, &text, &options);
    match result {
        Ok(result) => {
            assert!(!result.notices.is_empty(), "the fallback is reported");
            let helvetica_runs = body(&result.objects)
                .lines
                .iter()
                .flat_map(|line| &line.runs)
                .filter(|run| run.font.name == "Helvetica")
                .count();
            println!("fallback: {helvetica_runs} runs of the paragraph are now in Helvetica");
            assert!(helvetica_runs > 0);
        }
        // every added character is in WinAnsi, so a standard font can draw it
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn a_matched_text_box_takes_the_paragraphs_font_size_and_spot_colour() {
    let Some(arial) = installed("arial.ttf") else {
        println!("skipped: Arial isn't installed here");
        return;
    };
    let mut doc = load(&corporate_pdf(&arial));
    let before = body(&page_objects(&doc, 0).unwrap()).clone();
    let new_text = letters_edit::MatchedText {
        position: letters_edit::Point::new(72.0, before.rect.bottom() + 20.0),
        text: "Yours sincerely, Quentin".into(),
        width: None,
        like_block: None,
    };
    let options = letters_edit::TextEditOptions::default();
    let result =
        letters_edit::add_text_matching(&mut doc, 0, &new_text, &options, &EditConsent::default())
            .unwrap();
    let added = result
        .objects
        .text_blocks
        .iter()
        .find(|block| block.text.starts_with("Yours"))
        .expect("the new text is on the page");
    let original_run = &before.lines[0].runs[0];
    for run in added.lines.iter().flat_map(|line| &line.runs) {
        assert_eq!(run.font.id, original_run.font.id);
        assert!((run.size - original_run.size).abs() < 0.01);
        assert_eq!(run.color, original_run.color);
    }
    assert!(
        (added.rect.x - 72.0).abs() < 1.0 && (added.rect.y - new_text.position.y).abs() < 1.0,
        "{:?}",
        added.rect
    );
    let content = support::page_content(&load(&save(&mut doc)), 0);
    assert!(
        content.contains("/CS0 cs 1 scn"),
        "the spot colour is reused exactly"
    );
    println!("{}", result.summary.notes.join(" "));
}
