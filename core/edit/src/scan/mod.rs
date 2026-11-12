//! turning a scanned page into real, visible, editable text.
//!
//! the words come from the caller's recognition (letters-ocr's `OcrPage`):
//! the text is exactly what was read, never changed here. the scan's
//! pixels decide how each line looks: its ink colour and the standard
//! face closest to its letters (see `style`), and every word is fitted to
//! its box with horizontal scaling, so the new text covers the old. two
//! modes: the text over the scan with the old letters painted out, or the
//! text alone with figures kept as pictures. the app keeps the original
//! scan in its undo history.
//!
//! | module | job |
//! |---|---|
//! | `pixels` | the scan's pixels and the map between points and pixels |
//! | `style` | measuring a line's look and choosing a face |
//! | `clean` | painting out old letters, finding figures |
//! | `write` | the text and pictures written to the page |

mod clean;
mod pixels;
mod style;
mod write;

use std::collections::BTreeMap;

use letters_ocr::{OcrPage, OcrWord};
use lopdf::Document;
use serde::{Deserialize, Serialize};

use crate::colors::Color;
use crate::edit_run::{run_edit, Outcome};
use crate::error::EditError;
use crate::fonts::standard::StandardFont;
use crate::geometry::{Point, Rect};
use crate::model::{PageObjects, TextAlignment};
use crate::summary::{ChangesSummary, EditConsent};

pub use style::ScanStyle;

/// what becomes of the scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanMode {
    /// keep the scan as the background, with the old letters painted out
    /// in the paper colour around them, and draw the text on top
    #[default]
    TextOverCleanedImage,
    /// drop the scan; keep only the regions no word covers (figures,
    /// signatures, stamps) as pictures
    TextOnly,
}

/// choices for a conversion.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanOptions {
    pub mode: ScanMode,
    /// lines read with less confidence than this (0 to 100) are marked
    pub low_confidence: f32,
    pub consent: EditConsent,
}

/// recognition engines call a word under about 70% doubtful; tesseract's
/// own word confidences fall below that mostly for misread words.
const DEFAULT_LOW_CONFIDENCE: f32 = 70.0;

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            mode: ScanMode::default(),
            low_confidence: DEFAULT_LOW_CONFIDENCE,
            consent: EditConsent::default(),
        }
    }
}

/// one converted line, for the ui to show and check.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanLine {
    pub text: String,
    pub rect: Rect,
    /// the mean word confidence, 0 to 100
    pub confidence: f32,
    /// below `lowConfidence`: worth checking against the scan
    pub low_confidence: bool,
    /// the standard face chosen, e.g. "Times-Bold"
    pub font: String,
    pub size: f32,
    pub color: Color,
    pub style: ScanStyle,
    pub paragraph: usize,
}

/// one converted paragraph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanParagraph {
    pub lines: Vec<usize>,
    pub alignment: TextAlignment,
}

/// what a conversion did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanConversion {
    pub objects: PageObjects,
    pub lines: Vec<ScanLine>,
    pub paragraphs: Vec<ScanParagraph>,
    /// pictures kept from the scan (text-only mode), on screen
    pub figures: Vec<Rect>,
    pub summary: ChangesSummary,
}

fn ocr_rect(rect: &letters_ocr::Rect) -> Rect {
    Rect::new(rect.x, rect.y, rect.width, rect.height)
}

/// a recognised line: its words, in order.
pub(crate) struct OcrLine<'a> {
    pub words: Vec<&'a OcrWord>,
}

impl OcrLine<'_> {
    pub fn rect(&self) -> Rect {
        self.words
            .iter()
            .skip(1)
            .fold(ocr_rect(&self.words[0].rect), |all, word| {
                all.union(&ocr_rect(&word.rect))
            })
    }

    pub fn text(&self) -> String {
        self.words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn confidence(&self) -> f32 {
        self.words.iter().map(|word| word.confidence).sum::<f32>() / self.words.len() as f32
    }

    /// the type size: the middle of the words' sizes
    pub fn size(&self) -> f32 {
        let mut sizes: Vec<f32> = self.words.iter().map(|word| word.font_size).collect();
        sizes.sort_by(f32::total_cmp);
        sizes[sizes.len() / 2]
    }
}

fn lines_of(ocr: &OcrPage) -> Vec<OcrLine<'_>> {
    let mut lines: BTreeMap<usize, Vec<&OcrWord>> = BTreeMap::new();
    for word in ocr.words.iter().filter(|word| !word.text.trim().is_empty()) {
        lines.entry(word.line).or_default().push(word);
    }
    lines.into_values().map(|words| OcrLine { words }).collect()
}

/// every character must be one a standard face can draw: the text is
/// what recognition read and is never changed to fit.
fn check_characters(lines: &[OcrLine]) -> Result<(), EditError> {
    let all: String = lines.iter().map(OcrLine::text).collect();
    let missing = crate::fonts::embed::not_in_standard_fonts(&all);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(EditError::UnsupportedCharacters(
            missing.into_iter().collect(),
        ))
    }
}

/// turns the scan on `page` into visible text. see `ScanMode`.
pub fn convert_scan_to_text(
    doc: &mut Document,
    page: usize,
    ocr: &OcrPage,
    options: &ScanOptions,
) -> Result<ScanConversion, EditError> {
    let lines = lines_of(ocr);
    if lines.is_empty() {
        return Err(EditError::InvalidValue(
            "Recognition found no words on this page.".into(),
        ));
    }
    check_characters(&lines)?;
    let mut converted = Vec::new();
    let mut figures = Vec::new();
    let result = run_edit(doc, page, &options.consent, |model, draft, plan| {
        let scan = pixels::find_scan(draft.doc, model)?;
        converted = describe_lines(&scan, &lines, options);
        figures = write::write_page(draft, model, plan, &scan, &lines, &converted, options.mode)?;
        Ok(Outcome::default())
    })?;
    let paragraphs = paragraphs_of(&converted);
    Ok(ScanConversion {
        objects: result.objects,
        lines: converted,
        paragraphs,
        figures,
        summary: result.summary,
    })
}

/// each line's look, from the scan's pixels.
fn describe_lines(scan: &pixels::Scan, lines: &[OcrLine], options: &ScanOptions) -> Vec<ScanLine> {
    lines
        .iter()
        .map(|line| {
            let rect = line.rect();
            let measured = style::line_pixels(&scan.pixels, scan.pixel_box(&rect));
            let size = line.size();
            let size_pixels = size * scan.scale().1;
            let letter_widths: Vec<f32> = line
                .words
                .iter()
                .map(|word| word.rect.width / word.text.chars().count().max(1) as f32)
                .collect();
            let look = style::measure(&measured, size_pixels, &letter_widths);
            let face = style::face_for(&look);
            let [r, g, b] = measured.ink_color;
            ScanLine {
                text: line.text(),
                rect,
                confidence: line.confidence(),
                low_confidence: line.confidence() < options.low_confidence,
                font: face.base_font_name().to_owned(),
                size,
                color: Color::new(r, g, b),
                style: look,
                paragraph: line.words[0].paragraph,
            }
        })
        .collect()
}

/// the face a converted line is written in.
pub(crate) fn face_of(line: &ScanLine) -> StandardFont {
    StandardFont::from_name(&line.font).unwrap_or_default()
}

/// lines grouped by paragraph, each with the alignment its edges show.
fn paragraphs_of(lines: &[ScanLine]) -> Vec<ScanParagraph> {
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, line) in lines.iter().enumerate() {
        groups.entry(line.paragraph).or_default().push(index);
    }
    groups
        .into_values()
        .map(|members| ScanParagraph {
            alignment: alignment_of(
                &members
                    .iter()
                    .map(|index| lines[*index].rect)
                    .collect::<Vec<_>>(),
            ),
            lines: members,
        })
        .collect()
}

/// edges within this share of the size (here in points) line up.
const EDGE_TOLERANCE: f32 = 3.0;

fn alignment_of(rects: &[Rect]) -> TextAlignment {
    if rects.len() < 2 {
        return TextAlignment::Left;
    }
    let lined_up = |values: Vec<f32>| {
        values
            .iter()
            .all(|value| (value - values[0]).abs() < EDGE_TOLERANCE)
    };
    let lefts = lined_up(rects.iter().map(|rect| rect.x).collect());
    let rights_but_last = lined_up(
        rects[..rects.len() - 1]
            .iter()
            .map(|rect| rect.right())
            .collect(),
    );
    let rights = lined_up(rects.iter().map(|rect| rect.right()).collect());
    let centres = lined_up(rects.iter().map(|rect| rect.x + rect.width / 2.0).collect());
    match (lefts, rights_but_last, rights, centres) {
        (true, true, _, _) if rects.len() > 2 => TextAlignment::Justified,
        (true, _, _, _) => TextAlignment::Left,
        (_, _, true, _) => TextAlignment::Right,
        (_, _, _, true) => TextAlignment::Center,
        _ => TextAlignment::Left,
    }
}

/// a recognised word's baseline start, on the page.
pub(crate) fn baseline_start(word: &OcrWord) -> Point {
    Point::new(word.baseline.start.x, word.baseline.start.y)
}
