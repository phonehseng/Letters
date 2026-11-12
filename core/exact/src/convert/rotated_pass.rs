//! placing rotated text boxes by measuring exports (recipe 8).
//!
//! 1. export a docx holding only the rotated boxes; move each box by
//!    (target - produced first pen)
//! 2. repeat while a start is off by more than 0.06 pt, at most 3 more
//!    moves (the produced start moves in steps of about 0.36 pt, so this
//!    stops at the best)
//! 3. one more export measures where each line now starts along its
//!    baseline; the glyphs are then planned against that start

use std::path::Path;

use crate::bridge::WordInstall;
use crate::fonts::FontPlan;
use crate::plan::glyphs::{plan_line, GlyphCounts, Targets};
use crate::plan::lines::Line;
use crate::plan::rotated::{MEASURING_PASSES, START_TOLERANCE_PT};
use crate::report::{ConversionReport, RotatedReport};

use crate::docx::xml::{
    anchor_paragraph, page_break_paragraph, section_break_paragraph, section_properties,
};
use crate::docx::{EmbeddedFont, Package};
use crate::pdf::PdfFile;
use crate::plan::lines::group_lines;
use crate::plan::rotated::{along_offset, RotatedLine};
use crate::ExactError;

/// exports only the rotated boxes and finds each line's first pen.
/// with `apply` the boxes are moved by the difference. returns each
/// line's start along its baseline relative to the source start.
pub fn measure_offsets(
    lines: &mut [RotatedLine],
    page_sizes: &[(f64, f64)],
    fonts: &[EmbeddedFont],
    work: &Path,
    name: &str,
    apply: bool,
    export: &mut dyn FnMut(&Path, &Path) -> Result<f64, ExactError>,
) -> Result<Vec<Option<f64>>, ExactError> {
    let mut package = Package {
        fonts: fonts.to_vec(),
        ..Package::default()
    };
    for (index, &(width, height)) in page_sizes.iter().enumerate() {
        let mut drawings = String::new();
        for line in lines.iter().filter(|l| l.page == index) {
            let id = package.shape_id();
            drawings.push_str(&line.drawing(id));
        }
        package.body.push(anchor_paragraph(&drawings));
        if let Some(&next) = page_sizes.get(index + 1) {
            package.body.push(if next == (width, height) {
                page_break_paragraph()
            } else {
                section_break_paragraph(width, height)
            });
        }
    }
    let (width, height) = *page_sizes.last().unwrap_or(&(612.0, 792.0));
    package.final_section = section_properties(width, height);
    let docx = work.join(format!("{name}.docx"));
    let pdf = work.join(format!("{name}.word.pdf"));
    std::fs::write(&docx, package.to_bytes().map_err(ExactError::WordFailed)?)
        .map_err(crate::error::write_failed)?;
    export(&docx, &pdf)?;
    let pages = PdfFile::open(&pdf)?.read_pages();
    let glyphs: Vec<_> = pages.into_iter().flat_map(|p| p.glyphs).collect();
    let measured = group_lines(&glyphs);
    let mut offsets = Vec::new();
    for line in lines.iter_mut() {
        let height = page_sizes.get(line.page).map(|s| s.1).unwrap_or(792.0);
        let wanted: String = line.text.chars().filter(|c| *c != ' ').collect();
        let best = measured
            .iter()
            .filter(|m| m.page == line.page && (m.angle - line.angle).abs() < 0.5)
            .filter(|m| {
                m.glyphs
                    .iter()
                    .map(|g| g.text.as_str())
                    .collect::<String>()
                    .replace(' ', "")
                    == wanted
            })
            .min_by(|a, b| {
                let distance = |m: &&crate::plan::lines::Line| {
                    (m.x() - line.origin_top_down.0)
                        .hypot((height - m.y()) - line.origin_top_down.1)
                };
                distance(a).total_cmp(&distance(b))
            });
        let Some(best) = best else {
            offsets.push(None);
            continue;
        };
        let produced = (best.x(), height - best.y());
        let (dx, dy) = (
            line.origin_top_down.0 - produced.0,
            line.origin_top_down.1 - produced.1,
        );
        if apply {
            line.correction = (line.correction.0 + dx, line.correction.1 + dy);
        }
        offsets.push(Some(along_offset(
            line.angle,
            line.origin_top_down,
            produced,
        )));
    }
    Ok(offsets)
}

/// rotated lines: boxes placed by measuring exports when word is there,
/// by geometry alone otherwise (and the report says so).
#[allow(clippy::too_many_arguments)]
pub(super) fn place_rotated(
    rotated: &mut [(RotatedLine, Line)],
    page_sizes: &[(f64, f64)],
    embedded: &[EmbeddedFont],
    word: &Option<WordInstall>,
    work: &Path,
    fonts: &mut FontPlan,
    report: &mut ConversionReport,
    export: &mut dyn FnMut(&Path, &Path) -> Result<f64, ExactError>,
) -> Result<Vec<RotatedLine>, ExactError> {
    report.rotated = RotatedReport {
        lines: rotated.len(),
        ..RotatedReport::default()
    };
    if rotated.is_empty() {
        return Ok(Vec::new());
    }
    if word.is_none() {
        report.warnings.push(format!(
            "{} rotated line{} placed by geometry alone, because Word wasn’t there to measure them; they may be off by several points.",
            rotated.len(),
            if rotated.len() == 1 { " was" } else { "s were" }
        ));
        return Ok(rotated.iter().map(|(line, _)| line.clone()).collect());
    }
    let work = work.join("rotated");
    std::fs::create_dir_all(&work).map_err(crate::error::write_failed)?;
    let mut lines: Vec<RotatedLine> = rotated.iter().map(|(line, _)| line.clone()).collect();
    let mut history = Vec::new();
    history.push(measure_offsets(
        &mut lines,
        page_sizes,
        embedded,
        &work,
        "rotated_probe",
        true,
        export,
    )?);
    for number in 1..MEASURING_PASSES {
        // the box moves in steps word snaps to, so a few translations may be needed
        let settled = history.last().is_some_and(|offsets: &Vec<Option<f64>>| {
            offsets
                .iter()
                .all(|o| o.is_none_or(|v| v.abs() <= START_TOLERANCE_PT))
        });
        if settled {
            break;
        }
        history.push(measure_offsets(
            &mut lines,
            page_sizes,
            embedded,
            &work,
            &format!("rotated_move_{number}"),
            true,
            export,
        )?);
    }
    // the last measurement is where word starts each line now: plan the glyphs against it
    let last = measure_offsets(
        &mut lines,
        page_sizes,
        embedded,
        &work,
        "rotated_check",
        false,
        export,
    )?;
    let mut scratch = GlyphCounts::default();
    for ((line, (_, source)), offset) in lines.iter_mut().zip(rotated.iter()).zip(&last) {
        let Some(offset) = offset else { continue };
        if let Some((plan, runs)) = plan_line(
            source,
            fonts,
            Targets::Along { offset_pt: *offset },
            &mut scratch,
        )
        .map_err(ExactError::WordFailed)?
        {
            line.runs = runs;
            if let Some(index) = line.plan_index {
                report.line_plans[index] = plan;
            }
        }
    }
    history.push(last);
    report.rotated.measured = true;
    report.rotated.along_offsets_pt = history;
    report.rotated.corrections_pt = lines.iter().map(|l| l.correction).collect();
    Ok(lines)
}
