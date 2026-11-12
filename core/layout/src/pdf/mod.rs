//! writing a layout result as a pdf: real text in embedded subset fonts,
//! pictures, and vector rules for borders, underlines and leaders. this is
//! what "export as pdf" and printing use, with no word or libreoffice.
//!
//! positions are written exactly as the result has them (four decimals,
//! a ten-thousandth of a point): our pdf has no device grid of its own.
//! each run is one text object; its glyphs are placed with a TJ array whose
//! adjustments put every glyph at its own x.

pub mod fonts;
pub mod images;

use std::collections::HashMap;
use std::fmt::Write;

use lopdf::{Dictionary, Object, ObjectId, Stream};
use thiserror::Error;

use crate::result::{GlyphRun, LayoutResult, Page, Rect};
use fonts::{add_font, FontUse};

#[derive(Debug, Error)]
pub enum PdfWriteError {
    #[error("The PDF could not be written: {0}")]
    Write(String),
}

/// a picture's bytes and content type, by media id.
pub type MediaLookup<'a> = dyn Fn(&str) -> Option<(&'a [u8], &'a str)> + 'a;

/// writes the result as a pdf, taking pictures from the document's media.
pub fn write_pdf(
    result: &LayoutResult,
    document: &letters_office::model::Document,
) -> Result<Vec<u8>, PdfWriteError> {
    let lookup = |id: &str| {
        document
            .media_item(id)
            .map(|item| (item.bytes.as_slice(), item.content_type.as_str()))
    };
    write_pdf_with(result, &lookup)
}

/// writes the result as a pdf with pictures from `media`.
pub fn write_pdf_with(
    result: &LayoutResult,
    media: &MediaLookup,
) -> Result<Vec<u8>, PdfWriteError> {
    let mut document = lopdf::Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let fonts = embed_fonts(&mut document, result);
    let mut image_ids: HashMap<String, Option<ObjectId>> = HashMap::new();
    let mut kids = Vec::new();
    for page in &result.pages {
        let page_id = write_page(&mut document, page, pages_id, &fonts, &mut image_ids, media);
        kids.push(Object::Reference(page_id));
    }
    let mut pages = Dictionary::new();
    pages.set("Type", name("Pages"));
    pages.set("Count", Object::Integer(kids.len() as i64));
    pages.set("Kids", Object::Array(kids));
    document.objects.insert(pages_id, Object::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.set("Type", name("Catalog"));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = document.add_object(catalog);
    document.trailer.set("Root", Object::Reference(catalog_id));
    document.compress();
    let mut out = Vec::new();
    document
        .save_to(&mut out)
        .map_err(|error| PdfWriteError::Write(error.to_string()))?;
    Ok(out)
}

fn name(text: &str) -> Object {
    Object::Name(text.as_bytes().to_vec())
}

/// an embedded font and the advance of each glyph it uses (1/1000 em).
struct Embedded {
    id: ObjectId,
    widths: HashMap<u16, f64>,
}

/// one embedded font object per face used anywhere in the result.
fn embed_fonts(document: &mut lopdf::Document, result: &LayoutResult) -> HashMap<u32, Embedded> {
    let mut uses: HashMap<u32, FontUse> = HashMap::new();
    for page in &result.pages {
        for line in &page.lines {
            for run in &line.runs {
                let entry = uses.entry(run.font).or_default();
                let chars: Vec<char> = run.text.chars().collect();
                for (index, glyph) in run.glyphs.iter().enumerate() {
                    let start = run.clusters[index] as usize;
                    let end = run
                        .clusters
                        .get(index + 1)
                        .map_or(chars.len(), |next| *next as usize);
                    let text: String = chars
                        .get(start..end.max(start))
                        .map(|c| c.iter().collect())
                        .unwrap_or_default();
                    entry.glyphs.entry(*glyph).or_insert(text);
                }
            }
        }
    }
    let mut ids = HashMap::new();
    for (font, used) in uses {
        let Some(face) = result.faces.get(font as usize) else {
            continue;
        };
        let tag = subset_tag(font);
        let scale = 1000.0 / face.metrics.units_per_em.max(1.0);
        // whole thousandths, as /W holds them: readers such as pdfium keep
        // widths as integers, so the tj adjustments use the same numbers
        let widths = used
            .glyphs
            .keys()
            .map(|glyph| {
                (
                    *glyph,
                    (f64::from(face.advance_of_glyph(*glyph)) * scale).round(),
                )
            })
            .collect();
        let id = add_font(document, face, &used, &tag);
        ids.insert(font, Embedded { id, widths });
    }
    ids
}

/// six capital letters naming a subset (pdf 1.7, 9.6.4)
fn subset_tag(font: u32) -> String {
    let mut value = font as u64 * 7_919 + 12_345;
    (0..6)
        .map(|_| {
            let letter = char::from(b'A' + (value % 26) as u8);
            value /= 26;
            letter
        })
        .collect()
}

fn write_page(
    document: &mut lopdf::Document,
    page: &Page,
    parent: ObjectId,
    fonts: &HashMap<u32, Embedded>,
    images: &mut HashMap<String, Option<ObjectId>>,
    media: &MediaLookup,
) -> ObjectId {
    let mut content = String::new();
    let height = page.height;
    // rules and shading first, so text and pictures sit on top of them
    for frame in &page.frames {
        draw_frame(&mut content, frame, height);
    }
    for rect in &page.rects {
        draw_rect(&mut content, rect, height);
    }
    let mut xobjects = Dictionary::new();
    for (index, image) in page.images.iter().enumerate() {
        let id = images.entry(image.media_id.clone()).or_insert_with(|| {
            media(&image.media_id)
                .and_then(|(bytes, kind)| images::add_image(document, bytes, kind))
        });
        let Some(id) = id else {
            continue;
        };
        let key = format!("Im{index}");
        xobjects.set(key.as_str(), Object::Reference(*id));
        let _ = writeln!(
            content,
            "q {:.4} 0 0 {:.4} {:.4} {:.4} cm /{key} Do Q",
            image.width,
            image.height,
            image.x,
            height - image.y - image.height
        );
    }
    let mut used_fonts = Dictionary::new();
    for line in &page.lines {
        for run in &line.runs {
            if let Some(embedded) = fonts.get(&run.font) {
                let key = format!("F{}", run.font);
                used_fonts.set(key.as_str(), Object::Reference(embedded.id));
                draw_run(&mut content, run, height, &embedded.widths);
            }
        }
    }
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(used_fonts));
    resources.set("XObject", Object::Dictionary(xobjects));
    let content_id = document.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
    let mut dict = Dictionary::new();
    dict.set("Type", name("Page"));
    dict.set("Parent", Object::Reference(parent));
    dict.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(page.width as f32),
            Object::Real(page.height as f32),
        ]),
    );
    dict.set("Resources", Object::Dictionary(resources));
    dict.set("Contents", Object::Reference(content_id));
    document.add_object(dict)
}

/// one run as one text object. the first glyph is placed by the text
/// matrix; each later one by a TJ adjustment that moves the pen from where
/// the previous glyph's width left it to the glyph's own x.
fn draw_run(content: &mut String, run: &GlyphRun, page_height: f64, widths: &HashMap<u16, f64>) {
    if run.glyphs.is_empty() {
        return;
    }
    let color = run.color.unwrap_or([0, 0, 0]);
    let _ = write!(
        content,
        "BT {:.4} {:.4} {:.4} rg /F{} {:.4} Tf ",
        f64::from(color[0]) / 255.0,
        f64::from(color[1]) / 255.0,
        f64::from(color[2]) / 255.0,
        run.font,
        run.size
    );
    if run.embolden > 0.0 {
        // synthesised bold: fill and stroke the outline
        let _ = write!(
            content,
            "2 Tr {:.4} w {:.4} {:.4} {:.4} RG ",
            run.embolden,
            f64::from(color[0]) / 255.0,
            f64::from(color[1]) / 255.0,
            f64::from(color[2]) / 255.0
        );
    }
    let skew = run.skew.to_radians().tan();
    let _ = write!(
        content,
        "1 0 {skew:.4} 1 {:.4} {:.4} Tm [",
        run.x[0],
        page_height - run.y
    );
    for (index, glyph) in run.glyphs.iter().enumerate() {
        if index > 0 {
            let previous = run.glyphs[index - 1];
            let advance = widths.get(&previous).copied().unwrap_or(0.0) / 1000.0 * run.size;
            let expected = run.x[index - 1] + advance;
            let adjustment = (expected - run.x[index]) * 1000.0 / run.size;
            if adjustment.abs() > 1e-4 {
                let _ = write!(content, "{adjustment:.4}");
            }
        }
        let _ = write!(content, "<{glyph:04X}>");
    }
    let _ = writeln!(content, "] TJ ET");
}

fn draw_rect(content: &mut String, rect: &Rect, page_height: f64) {
    let [r, g, b] = rect.color.map(|c| f64::from(c) / 255.0);
    let bottom = page_height - rect.y - rect.height;
    if rect.dotted {
        // a dotted rule: round dots of the rule's thickness along its middle
        let thickness = rect.height.min(rect.width).max(0.3);
        let horizontal = rect.width >= rect.height;
        let (x1, y1, x2, y2) = if horizontal {
            (
                rect.x,
                bottom + rect.height / 2.0,
                rect.x + rect.width,
                bottom + rect.height / 2.0,
            )
        } else {
            (
                rect.x + rect.width / 2.0,
                bottom,
                rect.x + rect.width / 2.0,
                bottom + rect.height,
            )
        };
        let _ = writeln!(
            content,
            "q {r:.4} {g:.4} {b:.4} RG {thickness:.4} w 1 J [0 {:.4}] 0 d {x1:.4} {y1:.4} m {x2:.4} {y2:.4} l S Q",
            thickness * 2.5
        );
        return;
    }
    let _ = writeln!(
        content,
        "q {r:.4} {g:.4} {b:.4} rg {:.4} {bottom:.4} {:.4} {:.4} re f Q",
        rect.x, rect.width, rect.height
    );
}

fn draw_frame(content: &mut String, frame: &crate::result::Frame, page_height: f64) {
    let bottom = page_height - frame.y - frame.height;
    if let Some([r, g, b]) = frame.fill {
        let _ = writeln!(
            content,
            "q {:.4} {:.4} {:.4} rg {:.4} {bottom:.4} {:.4} {:.4} re f Q",
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
            frame.x,
            frame.width,
            frame.height
        );
    }
    if let Some(([r, g, b], width)) = frame.border {
        let _ = writeln!(
            content,
            "q {:.4} {:.4} {:.4} RG {width:.4} w {:.4} {bottom:.4} {:.4} {:.4} re S Q",
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
            frame.x,
            frame.width,
            frame.height
        );
    }
}
