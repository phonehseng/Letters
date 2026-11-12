//! writing a converted scan: the cleaned background or the kept figures,
//! then the text, one positioned run per word so each word covers the
//! word it replaces.

use std::collections::BTreeMap;

use image::RgbImage;
use lopdf::{Object, StringFormat};

use crate::draft::Draft;
use crate::error::EditError;
use crate::fonts::embed::{self, WriteFont};
use crate::geometry::{Matrix, Point, Rect};
use crate::image_data;
use crate::layout::PageModel;
use crate::moving::matrix_numbers;
use crate::pdf_values::format_number;
use crate::rewrite::Plan;
use crate::writer;

use super::pixels::Scan;
use super::{baseline_start, clean, face_of, ocr_rect, OcrLine, ScanLine, ScanMode};

/// words are stretched or squeezed at most this far to fit their boxes;
/// beyond it the box (or the word) is wrong and fitting would distort.
const MIN_SCALE: f32 = 0.5;
const MAX_SCALE: f32 = 2.0;

/// writes the page in the chosen mode; returns the figures kept.
pub(super) fn write_page(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    scan: &Scan,
    lines: &[OcrLine],
    converted: &[ScanLine],
    mode: ScanMode,
) -> Result<Vec<Rect>, EditError> {
    let record = &model.run.images[scan.image];
    let figures = match mode {
        ScanMode::TextOverCleanedImage => {
            let painted: Vec<(Rect, [u8; 3])> = lines
                .iter()
                .zip(converted)
                .flat_map(|(line, look)| {
                    let paper = super::style::line_pixels(&scan.pixels, scan.pixel_box(&look.rect))
                        .paper_color;
                    line.words
                        .iter()
                        .map(move |word| (ocr_rect(&word.rect), paper))
                })
                .collect();
            let mut cleaned = scan.pixels.clone();
            clean::paint_out(scan, &mut cleaned, &painted);
            let name = add_picture(draft, model, plan, &cleaned, Some(record.source))?;
            let drawing = format!("{} Do", writer::name(&name));
            plan.op(record.source, record.op).replace = Some(drawing.into_bytes());
            Vec::new()
        }
        ScanMode::TextOnly => {
            plan.op(record.source, record.op).replace = Some(Vec::new());
            let words: Vec<Rect> = lines
                .iter()
                .flat_map(|line| line.words.iter().map(|word| ocr_rect(&word.rect)))
                .collect();
            keep_figures(draft, model, plan, scan, &words)?
        }
    };
    let text = text_content(draft, model, plan, lines, converted);
    plan.append(text.as_bytes());
    Ok(figures)
}

/// stores pixels as a picture and names it in a stream's resources.
fn add_picture(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    pixels: &RgbImage,
    source: Option<usize>,
) -> Result<Vec<u8>, EditError> {
    let mut png = Vec::new();
    pixels
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|_| EditError::UnsupportedImage("The cleaned scan couldn’t be written.".into()))?;
    let added = image_data::add_image_object(draft, &png)?;
    Ok(plan.resource_name(draft.doc, &model.run, source, "XObject", added.object))
}

/// cuts each figure out of the scan and draws it where it was.
fn keep_figures(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    scan: &Scan,
    words: &[Rect],
) -> Result<Vec<Rect>, EditError> {
    let mut kept = Vec::new();
    for (left, top, right, bottom) in clean::figures(scan, words) {
        let cut = image::imageops::crop_imm(&scan.pixels, left, top, right - left, bottom - top)
            .to_image();
        let name = add_picture(draft, model, plan, &cut, None)?;
        let place = scan.page_rect(left, top, right, bottom);
        let corners = place
            .corners()
            .map(|corner| model.run.space.user_point(corner));
        let matrix = Matrix::unit_square_to(corners[0], corners[1], corners[2]);
        plan.append(
            format!(
                "q {} cm {} Do Q",
                matrix_numbers(&matrix),
                writer::name(&name)
            )
            .as_bytes(),
        );
        kept.push(place);
    }
    Ok(kept)
}

/// the text of every line, each word placed on its baseline and scaled
/// across to its box.
fn text_content(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    lines: &[OcrLine],
    converted: &[ScanLine],
) -> String {
    let mut fonts: BTreeMap<String, (WriteFont, Vec<u8>)> = BTreeMap::new();
    let space = &model.run.space;
    let right = space.user_vector(Point::new(1.0, 0.0));
    let up = space.user_vector(Point::new(0.0, -1.0));
    let mut out = vec!["q BT".to_owned()];
    for (line, look) in lines.iter().zip(converted) {
        let face = face_of(look);
        let (font, name) = fonts
            .entry(look.font.clone())
            .or_insert_with(|| {
                let font = embed::standard_font(draft, face);
                let name = plan.resource_name(draft.doc, &model.run, None, "Font", font.object);
                (font, name)
            })
            .clone();
        out.push(look.color.fill_operator());
        out.push(format!(
            "{} {} Tf",
            writer::name(&name),
            format_number(look.size)
        ));
        for word in &line.words {
            let natural = natural_width(&font, &word.text, look.size);
            let scale = (word.rect.width / natural.max(1e-3)).clamp(MIN_SCALE, MAX_SCALE);
            let origin = space.user_point(Point::new(word.rect.x, baseline_start(word).y));
            let matrix = [right.x, right.y, up.x, up.y, origin.x, origin.y]
                .map(format_number)
                .join(" ");
            out.push(format!("{} Tz {matrix} Tm", format_number(scale * 100.0)));
            let codes: Vec<u8> = word
                .text
                .chars()
                .filter_map(|c| font.codes.get(&c))
                .flat_map(|code| code.code.clone())
                .collect();
            let shown =
                writer::operation("Tj", &[Object::String(codes, StringFormat::Hexadecimal)]);
            out.push(String::from_utf8_lossy(&shown).into_owned());
        }
    }
    out.push("ET Q".to_owned());
    out.join("\n")
}

/// a word's width in its font at a size, unscaled.
fn natural_width(font: &WriteFont, text: &str, size: f32) -> f32 {
    text.chars()
        .filter_map(|c| font.codes.get(&c))
        .map(|code| code.width * size / 1000.0)
        .sum()
}
