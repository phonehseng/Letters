//! probe exports that measure what the model needs for a page size or a
//! line height word wasn't measured on (recipe 9).
//!
//! every probe is a document of 12 pt frames holding the letter "n" in a
//! probe font, exported through word; the printed `Tm` of each frame's
//! glyph tells the writer's unit value for that frame's pixel.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::model::{is_landscape, Axis, WordModel};
use crate::bridge::WordBridge;
use crate::docx::xml::{frame_paragraph, marked_section_properties, page_break_paragraph};
use crate::docx::{EmbeddedFont, Package};
use crate::fonts::embed::font_key_from;
use crate::fonts::outline::ContourPoint;
use crate::fonts::truetype::{write_truetype, FontGlyph, FontSpec};
use crate::pdf::PdfFile;
use crate::plan::glyphs::CharRun;
use crate::verify::raw::{read_text_objects, TextObject};
use crate::word::writer_model::{
    decode_y, page_pixels, twips_for_pixel, twips_pixel, units_candidates, StepTable, TWIPS_PERIOD,
    UNITS_PER_PIXEL,
};
use crate::{CancelFlag, ExactError};

const PROBE_FAMILY: &str = "Letters Exact Probe";
const PROBE_HALF_POINTS: i64 = 24;
const PROBE_LINE_TWIPS: i64 = 240;
/// a 12 pt exact line puts its baseline 80 px below the frame top, at every twip (e10)
const PROBE_BASELINE_OFFSET: i64 = 80;
/// probe frames: wide enough for one glyph, narrow enough to reach the page edge
const FRAME_TWIPS: i64 = 200;
const COARSE_STEP_PX: i64 = 5;
const PROBE_CODE: u32 = b'n' as u32;
/// the first baseline guess for an exact line (e2, e8)
const BASELINE_SHARE: f64 = 0.804;

/// the probe font: a box for "n", half an em wide.
fn probe_font() -> EmbeddedFont {
    let point = |x, y| ContourPoint {
        x,
        y,
        on_curve: true,
    };
    let glyphs = vec![
        FontGlyph {
            contours: Vec::new(),
            advance: 500,
        },
        FontGlyph {
            contours: Vec::new(),
            advance: 250,
        },
        FontGlyph {
            contours: vec![vec![
                point(50, 0),
                point(50, 450),
                point(450, 450),
                point(450, 0),
            ]],
            advance: 500,
        },
    ];
    let cmap = BTreeMap::from([(0x20, 1), (PROBE_CODE, 2)]);
    let bytes = write_truetype(&FontSpec {
        family: PROBE_FAMILY,
        units_per_em: 1000,
        glyphs: &glyphs,
        cmap: &cmap,
        ascent: 800,
        descent: -200,
        fs_type: 0,
    });
    EmbeddedFont {
        family: PROBE_FAMILY.into(),
        bytes,
        font_key: font_key_from(0x9E0B),
    }
}

fn probe_frame(x_twips: i64, y_twips: i64, line_twips: i64) -> String {
    let run = CharRun {
        char: "n".into(),
        family: PROBE_FAMILY.into(),
        half_points: PROBE_HALF_POINTS,
        color: "000000".into(),
        spacing_twips: 0,
        hidden: false,
    };
    frame_paragraph(&[run], x_twips, y_twips, FRAME_TWIPS, line_twips)
}

fn write_probe(
    parts: Vec<String>,
    width_pt: f64,
    height_pt: f64,
    landscape: bool,
    path: &Path,
) -> Result<(), ExactError> {
    let mut package = Package {
        body: parts,
        final_section: marked_section_properties(width_pt, height_pt, landscape),
        ..Package::default()
    };
    package.fonts.push(probe_font());
    let bytes = package.to_bytes().map_err(ExactError::WordFailed)?;
    std::fs::write(path, bytes).map_err(crate::error::write_failed)
}

fn probe_objects(pdf: &Path) -> Result<Vec<TextObject>, ExactError> {
    let file = PdfFile::open(pdf)?;
    Ok(read_text_objects(&file)
        .into_iter()
        .filter(|o| o.codes.first() == Some(&PROBE_CODE))
        .collect())
}

/// runs probe documents through word; returns the pdf paths in order.
struct Prober<'a> {
    bridge: &'a WordBridge,
    cancel: &'a CancelFlag,
    dir: PathBuf,
    seconds: f64,
    /// mark probe sections landscape or not; `None`: as the converter does, by size
    landscape: Option<bool>,
}

impl Prober<'_> {
    fn write(
        &self,
        parts: Vec<String>,
        width_pt: f64,
        height_pt: f64,
        path: &Path,
    ) -> Result<(), ExactError> {
        let landscape = self.landscape.unwrap_or(is_landscape(width_pt, height_pt));
        write_probe(parts, width_pt, height_pt, landscape, path)
    }

    fn export(&mut self, documents: &[(PathBuf, PathBuf)]) -> Result<(), ExactError> {
        let run = self.bridge.export(documents, self.cancel)?;
        self.seconds += run.seconds;
        for result in run.results {
            result.map_err(|why| {
                ExactError::WordFailed(format!("A measuring export failed. ({why})"))
            })?;
        }
        Ok(())
    }

    fn paths(&self, name: &str) -> (PathBuf, PathBuf) {
        (
            self.dir.join(format!("{name}.docx")),
            self.dir.join(format!("{name}.word.pdf")),
        )
    }

    /// printed writer units of a frame at each layout pixel, one frame per row.
    fn x_units_at(
        &mut self,
        pixels: &[i64],
        width_pt: f64,
        height_pt: f64,
        name: &str,
    ) -> Result<BTreeMap<i64, i64>, ExactError> {
        let height_twips = crate::units::pt_to_twips(height_pt);
        let rows = ((height_twips - 1200) / PROBE_LINE_TWIPS).max(1) as usize;
        let mut parts = Vec::new();
        for (index, &pixel) in pixels.iter().enumerate() {
            if index > 0 && index % rows == 0 {
                parts.push(page_break_paragraph());
            }
            parts.push(probe_frame(
                twips_for_pixel(pixel),
                600 + (index % rows) as i64 * PROBE_LINE_TWIPS,
                PROBE_LINE_TWIPS,
            ));
        }
        let (docx, pdf) = self.paths(name);
        self.write(parts, width_pt, height_pt, &docx)?;
        self.export(&[(docx, pdf.clone())])?;
        let objects = probe_objects(&pdf)?;
        if objects.len() != pixels.len() {
            return Err(ExactError::WordFailed(format!(
                "The x measuring export showed {} glyphs for {} frames.",
                objects.len(),
                pixels.len()
            )));
        }
        Ok(pixels
            .iter()
            .zip(&objects)
            .map(|(&pixel, object)| {
                (
                    pixel,
                    nearest_units(&units_candidates(object.x()), UNITS_PER_PIXEL * pixel),
                )
            })
            .collect())
    }

    /// printed writer units from the top of a 12 pt exact line's baseline at each pixel.
    fn y_units_at(
        &mut self,
        pixels: &[i64],
        width_pt: f64,
        height_pt: f64,
        name: &str,
    ) -> Result<BTreeMap<i64, i64>, ExactError> {
        let columns = ((crate::units::pt_to_twips(width_pt) - 400) / 700).max(1) as usize;
        let height_px = page_pixels(height_pt);
        // consecutive pixels go to consecutive columns; a column's next frame is at least 120 px lower
        let mut pages: Vec<Vec<(i64, i64, i64)>> = vec![Vec::new()];
        let mut last_in_column: BTreeMap<usize, i64> = BTreeMap::new();
        let mut sorted = pixels.to_vec();
        sorted.sort_unstable();
        for pixel in sorted {
            let page = pages.last_mut().expect("there is always a page");
            let mut column = page.len() % columns;
            if last_in_column
                .get(&column)
                .is_some_and(|&last| pixel - last < 120)
            {
                pages.push(Vec::new());
                last_in_column.clear();
                column = 0;
            }
            let page = pages.last_mut().expect("there is always a page");
            page.push((
                pixel,
                400 + column as i64 * 700,
                twips_for_pixel(pixel - PROBE_BASELINE_OFFSET),
            ));
            last_in_column.insert(column, pixel);
        }
        let mut parts = Vec::new();
        for (number, frames) in pages.iter().enumerate() {
            if number > 0 {
                parts.push(page_break_paragraph());
            }
            parts.extend(
                frames
                    .iter()
                    .map(|&(_, x, y)| probe_frame(x, y, PROBE_LINE_TWIPS)),
            );
        }
        let (docx, pdf) = self.paths(name);
        self.write(parts, width_pt, height_pt, &docx)?;
        self.export(&[(docx, pdf.clone())])?;
        let objects = probe_objects(&pdf)?;
        let mut units = BTreeMap::new();
        let page_units = UNITS_PER_PIXEL * height_px;
        for (number, frames) in pages.iter().enumerate() {
            for &(pixel, x, _) in frames {
                let candidates: Vec<&TextObject> = objects
                    .iter()
                    .filter(|o| o.page == number && (o.x() - x as f64 / 20.0).abs() < 2.0)
                    .collect();
                let Some(object) = candidates.into_iter().min_by(|a, b| {
                    let distance = |o: &TextObject| {
                        ((page_units as f64 * 0.024 - o.y()) / 0.12 - pixel as f64).abs()
                    };
                    distance(a).total_cmp(&distance(b))
                }) else {
                    return Err(ExactError::WordFailed(
                        "The y measuring export lost a frame.".into(),
                    ));
                };
                let from_top: Vec<i64> = units_candidates(object.y())
                    .into_iter()
                    .map(|u| page_units - u)
                    .collect();
                units.insert(pixel, nearest_units(&from_top, UNITS_PER_PIXEL * pixel));
            }
        }
        Ok(units)
    }
}

fn nearest_units(candidates: &[i64], wanted: i64) -> i64 {
    candidates
        .iter()
        .copied()
        .min_by_key(|u| (u - wanted).abs())
        .unwrap_or(wanted)
}

/// the steps (first pixels with k = 1, 2, ...) from measured k values.
fn steps_from(k: &BTreeMap<i64, i64>) -> Vec<i64> {
    let mut steps = Vec::new();
    let mut previous = 0;
    for (&pixel, &value) in k {
        while value > previous {
            steps.push(pixel);
            previous += 1;
        }
    }
    steps
}

/// the writer's step pixels along one axis of a page size: a coarse pass
/// every 5 px, then every pixel between two coarse pixels whose k differ.
fn measure_steps(
    prober: &mut Prober,
    axis: Axis,
    width_pt: f64,
    height_pt: f64,
) -> Result<Vec<i64>, ExactError> {
    let along = page_pixels(if axis == Axis::X { width_pt } else { height_pt });
    let (margin, first) = if axis == Axis::X {
        (100, 0)
    } else {
        (60, PROBE_BASELINE_OFFSET + 1)
    };
    let coarse: Vec<i64> = (first..along - margin)
        .step_by(COARSE_STEP_PX as usize)
        .collect();
    let tag = format!("steps_{}{along}", if axis == Axis::X { "x" } else { "y" });
    let probe = |prober: &mut Prober, pixels: &[i64], name: &str| match axis {
        Axis::X => prober.x_units_at(pixels, width_pt, height_pt, name),
        Axis::Y => prober.y_units_at(pixels, width_pt, height_pt, name),
    };
    let units = probe(prober, &coarse, &format!("{tag}_coarse"))?;
    let mut k: BTreeMap<i64, i64> = units
        .iter()
        .map(|(&p, &u)| (p, u - UNITS_PER_PIXEL * p))
        .collect();
    let fine: Vec<i64> = coarse
        .windows(2)
        .filter(|pair| k[&pair[0]] != k[&pair[1]])
        .flat_map(|pair| pair[0] + 1..pair[1])
        .collect();
    if !fine.is_empty() {
        let more = probe(prober, &fine, &format!("{tag}_fine"))?;
        k.extend(more.iter().map(|(&p, &u)| (p, u - UNITS_PER_PIXEL * p)));
    }
    Ok(steps_from(&k))
}

/// baseline offsets (pixels below `twips_pixel(t)`) for t mod 60 = 0..59
/// at one exact line height, measured on a letter page.
fn measure_baseline(
    prober: &mut Prober,
    line_twips: i64,
    letter_y: &StepTable,
) -> Result<Vec<i64>, ExactError> {
    let (docx, pdf) = write_baseline_probe(prober, line_twips)?;
    prober.export(&[(docx, pdf.clone())])?;
    read_baseline_probe(&pdf, line_twips, letter_y)
}

/// the probe frames of one line height: (t mod 60, frame x, frame y).
fn baseline_probe_frames(line_twips: i64) -> Vec<(i64, i64, i64)> {
    let columns = 16;
    let rows_apart = 1 + (line_twips + 200) / TWIPS_PERIOD;
    (0..TWIPS_PERIOD)
        .map(|residue| {
            (
                residue,
                400 + (residue % columns) * 700,
                1200 + residue + TWIPS_PERIOD * rows_apart * (residue / columns),
            )
        })
        .collect()
}

fn write_baseline_probe(
    prober: &Prober,
    line_twips: i64,
) -> Result<(PathBuf, PathBuf), ExactError> {
    let parts = baseline_probe_frames(line_twips)
        .iter()
        .map(|&(_, x, y)| probe_frame(x, y, line_twips))
        .collect();
    let (docx, pdf) = prober.paths(&format!("baseline_{line_twips}"));
    prober.write(parts, 612.0, 792.0, &docx)?;
    Ok((docx, pdf))
}

fn read_baseline_probe(
    pdf: &Path,
    line_twips: i64,
    letter_y: &StepTable,
) -> Result<Vec<i64>, ExactError> {
    let objects = probe_objects(pdf)?;
    let mut offsets = vec![0; TWIPS_PERIOD as usize];
    for (residue, x, y) in baseline_probe_frames(line_twips) {
        let top = twips_pixel(y);
        let guess = top as f64 + BASELINE_SHARE * line_twips as f64 * 5.0 / 12.0;
        let object = objects
            .iter()
            .filter(|o| (o.x() - x as f64 / 20.0).abs() < 2.0)
            .min_by(|a, b| {
                (((792.0 - a.y()) / 0.12) - guess)
                    .abs()
                    .total_cmp(&(((792.0 - b.y()) / 0.12) - guess).abs())
            })
            .ok_or_else(|| {
                ExactError::WordFailed("The baseline measuring export lost a frame.".into())
            })?;
        let pixel = decode_y(object.y(), letter_y, 6600).ok_or_else(|| {
            ExactError::WordFailed(format!("A printed y of {} fits no pixel.", object.y()))
        })?;
        offsets[residue as usize] = pixel - top;
    }
    Ok(offsets)
}

/// measures the baseline tables of every line height the model lacks in
/// one word session (one probe document per height). the easy to edit mode
/// needs them for the line heights of real paragraphs: off the 16 heights
/// the recipe measured, the closed rule (recipe 6.6) put 57 of 393 lines a
/// pixel off (`tests/hybrid_probe.rs`, heights 200 to 330 twips).
/// returns the heights measured.
pub fn measure_baselines(
    model: &mut WordModel,
    line_heights: &[i64],
    bridge: &WordBridge,
    work_dir: &Path,
    cancel: &CancelFlag,
) -> Result<Vec<i64>, ExactError> {
    std::fs::create_dir_all(work_dir).map_err(crate::error::write_failed)?;
    let mut prober = Prober {
        bridge,
        cancel,
        dir: work_dir.to_path_buf(),
        seconds: 0.0,
        landscape: None,
    };
    let missing: Vec<i64> = line_heights
        .iter()
        .copied()
        .filter(|&l| !model.has_baseline(l))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if missing.is_empty() {
        return Ok(missing);
    }
    if !model.has_steps(Axis::Y, 6600, false) {
        let steps = measure_steps(&mut prober, Axis::Y, 612.0, 792.0)?;
        model.set_steps(Axis::Y, 6600, false, steps);
    }
    let letter_y = model.table(Axis::Y, 6600, false);
    let mut documents = Vec::new();
    for &line_twips in &missing {
        documents.push(write_baseline_probe(&prober, line_twips)?);
    }
    prober.export(&documents)?;
    for (&line_twips, (_, pdf)) in missing.iter().zip(&documents) {
        let offsets = read_baseline_probe(pdf, line_twips, &letter_y)?;
        model.set_baseline(line_twips, offsets);
    }
    Ok(missing)
}
/// what the calibration step did.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationReport {
    pub word_build: String,
    /// the facts came from the tables shipped with Letters
    pub built_in_tables: bool,
    pub measured_steps: Vec<String>,
    pub measured_baselines: Vec<i64>,
    /// step tables that had to be estimated because word wasn't there to measure them
    pub estimated_steps: Vec<String>,
    pub probe_seconds: f64,
}

/// measures whatever the model lacks for these page sizes and line heights.
pub fn ensure_measured(
    model: &mut WordModel,
    page_sizes: &[(f64, f64)],
    line_heights: &[i64],
    bridge: &WordBridge,
    work_dir: &Path,
    cancel: &CancelFlag,
) -> Result<CalibrationReport, ExactError> {
    std::fs::create_dir_all(work_dir).map_err(crate::error::write_failed)?;
    let mut prober = Prober {
        bridge,
        cancel,
        dir: work_dir.to_path_buf(),
        seconds: 0.0,
        landscape: None,
    };
    let mut report = CalibrationReport {
        word_build: model.build.clone(),
        ..CalibrationReport::default()
    };
    for &(width, height) in page_sizes {
        // the probe documents use the same section properties as the
        // converter, landscape included, so they measure the same lattice
        let landscape = is_landscape(width, height);
        for (axis, along) in [
            (Axis::X, page_pixels(width)),
            (Axis::Y, page_pixels(height)),
        ] {
            if model.has_steps(axis, along, landscape) {
                continue;
            }
            let steps = measure_steps(&mut prober, axis, width, height)?;
            let axis_name = if axis == Axis::X { "x" } else { "y" };
            let orientation = if landscape { ":landscape" } else { "" };
            report
                .measured_steps
                .push(format!("{axis_name}:{along}{orientation}"));
            model.set_steps(axis, along, landscape, steps);
        }
    }
    for &line_twips in line_heights {
        if model.has_baseline(line_twips) {
            continue;
        }
        if !model.has_steps(Axis::Y, 6600, false) {
            let steps = measure_steps(&mut prober, Axis::Y, 612.0, 792.0)?;
            model.set_steps(Axis::Y, 6600, false, steps);
            report.measured_steps.push("y:6600".into());
        }
        let letter_y = model.table(Axis::Y, 6600, false);
        let offsets = measure_baseline(&mut prober, line_twips, &letter_y)?;
        report.measured_baselines.push(line_twips);
        model.set_baseline(line_twips, offsets);
    }
    report.probe_seconds = prober.seconds;
    Ok(report)
}

/// measures both step tables of one page size, with the probe sections
/// marked landscape or not (`None`: as the converter marks them, by size).
/// for calibration tools and checks; conversions use `ensure_measured`.
pub fn measure_page_steps(
    bridge: &WordBridge,
    width_pt: f64,
    height_pt: f64,
    landscape: Option<bool>,
    work_dir: &Path,
    cancel: &CancelFlag,
) -> Result<(Vec<i64>, Vec<i64>, f64), ExactError> {
    std::fs::create_dir_all(work_dir).map_err(crate::error::write_failed)?;
    let mut prober = Prober {
        bridge,
        cancel,
        dir: work_dir.to_path_buf(),
        seconds: 0.0,
        landscape,
    };
    let x = measure_steps(&mut prober, Axis::X, width_pt, height_pt)?;
    let y = measure_steps(&mut prober, Axis::Y, width_pt, height_pt)?;
    Ok((x, y, prober.seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_count_each_unit_once() {
        let k = BTreeMap::from([(0, 0), (5, 0), (6, 1), (10, 1), (12, 2), (20, 4)]);
        assert_eq!(steps_from(&k), [6, 12, 20, 20]);
    }

    #[test]
    fn the_probe_font_is_valid() {
        let font = probe_font();
        let face = ttf_parser::Face::parse(&font.bytes, 0).unwrap();
        assert!(face.glyph_index('n').is_some());
    }
}
