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
