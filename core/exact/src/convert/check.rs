//! the checking export: find glyphs word moved off the planned pixel and
//! re-plan those lines (recipe 7.6).
//!
//! every glyph of the docx starts its own text object, so word's pdf gives
//! each glyph's exact layout pixel. with the balanced spacing plan word
//! follows the plan for all but a few glyphs per thousand (the journal's
//! italic "W" was laid out a pixel wider than its grid advance). for each
//! line with a miss, the targets from the first wrong glyph on are shifted
//! by what word added there and the line is planned again; one export per
//! pass, at most two re-plans, and every line keeps its best plan.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::pdf::PdfFile;
use crate::plan::glyphs::LinePlan;
use crate::report::CheckReport;
use crate::verify::raw::{read_text_objects, TextObject};
use crate::word::spacing::spacing_for_pixel_steps;
use crate::word::writer_model::{decode_x, exported_x, StepTable};
use crate::ExactError;

pub const MAX_REPLANS: usize = 2;

/// per plan, word's layout pixels from the planned start pixel, or `None`
/// when the line wasn't found as one text object per glyph.
pub fn produced_offsets(
    objects: &[TextObject],
    plans: &[&LinePlan],
    tables: &HashMap<usize, StepTable>,
) -> Vec<Option<Vec<i64>>> {
    let mut by_page: HashMap<usize, Vec<&TextObject>> = HashMap::new();
    for object in objects {
        if !object.codes.is_empty() && object.tm[1].abs() < 1e-6 {
            by_page.entry(object.page).or_default().push(object);
        }
    }
    plans
        .iter()
        .map(|plan| {
            let (Some(start), Some(word_y)) = (plan.start_pixel, plan.word_y) else {
                return None;
            };
            let table = tables.get(&plan.page)?;
            let start_pt = exported_x(start, table);
            let wanted = plan.text.chars().count();
            let mut row: Vec<&TextObject> = by_page
                .get(&plan.page)
                .map(|objects| {
                    objects
                        .iter()
                        .copied()
                        .filter(|o| (o.y() - word_y).abs() < 0.01 && o.x() >= start_pt - 0.01)
                        .collect()
                })
                .unwrap_or_default();
            row.sort_by(|a, b| a.x().total_cmp(&b.x()));
            row.truncate(wanted);
            if row.len() != wanted || row.iter().any(|o| o.codes.len() != 1) {
                return None;
            }
            row.iter()
                .map(|o| decode_x(o.x(), table).map(|p| p - start))
                .collect()
        })
        .collect()
}

/// (glyph index, pixels word put it off the plan) of the first wrong glyph.
pub fn first_miss(targets: &[i64], produced: &[i64]) -> Option<(usize, i64)> {
    targets
        .iter()
        .zip(produced)
        .enumerate()
        .find(|(_, (want, got))| want != got)
        .map(|(i, (want, got))| (i, got - want))
}

/// spacing by line plan index, replacing a line's planned spacing.
pub type SpacingOverrides = HashMap<usize, Vec<i64>>;
/// writes the docx with some lines' spacing replaced and returns its path.
pub type BuildDocx<'a> = dyn FnMut(&SpacingOverrides) -> Result<PathBuf, ExactError> + 'a;
/// exports a docx to a pdf through word and returns the seconds it took.
pub type ExportPdf<'a> = dyn FnMut(&Path, &Path) -> Result<f64, ExactError> + 'a;

/// what the checking exports did.
pub struct CheckOutcome {
    pub report: CheckReport,
    /// the spacing each re-planned line keeps
    pub kept: SpacingOverrides,
    /// word's pdf of the final docx, when the last export was of it
    pub final_pdf: Option<PathBuf>,
}

/// runs the checking exports and re-plans the lines word moved.
pub fn check_and_replan(
    plans: &[LinePlan],
    tables: &HashMap<usize, StepTable>,
    work: &Path,
    build: &mut BuildDocx,
    export: &mut ExportPdf,
) -> Result<CheckOutcome, ExactError> {
    let indexes: Vec<usize> = (0..plans.len()).filter(|&i| !plans[i].rotated).collect();
    let horizontal: Vec<&LinePlan> = indexes.iter().map(|&i| &plans[i]).collect();
    let mut asked: Vec<Vec<i64>> = horizontal.iter().map(|p| p.targets_px.clone()).collect();
    let mut best: HashMap<usize, (usize, Vec<i64>)> = HashMap::new();
    let mut current: HashMap<usize, Vec<i64>> = HashMap::new();
    let mut report = CheckReport::default();
    let mut docx = build(&current)?;
    let mut last_pdf = None;
    for number in 0..=MAX_REPLANS {
        let pdf = work.join(format!("check_{number}.word.pdf"));
        report.seconds += export(&docx, &pdf)?;
        let objects = read_text_objects(&PdfFile::open(&pdf)?);
        let offsets = produced_offsets(&objects, &horizontal, tables);
        report.lines_not_found = offsets.iter().filter(|o| o.is_none()).count();
        let mut changed: HashMap<usize, Vec<i64>> = HashMap::new();
        let (mut missing_lines, mut off_glyphs) = (0, 0);
        for (index, (plan, produced)) in horizontal.iter().zip(&offsets).enumerate() {
            let Some(produced) = produced else { continue };
            let misses = plan
                .targets_px
                .iter()
                .zip(produced)
                .filter(|(want, got)| want != got)
                .count();
            let spacing = spacing_for_pixel_steps(&plan.advances_px, &asked[index])
                .unwrap_or_else(|_| plan.spacing_twips.clone());
            if best.get(&index).is_none_or(|(fewest, _)| misses < *fewest) {
                best.insert(index, (misses, spacing));
            }
            if misses == 0 {
                continue;
            }
            missing_lines += 1;
            off_glyphs += misses;
            let Some((glyph, delta)) = first_miss(&plan.targets_px, produced) else {
                continue;
            };
            if number == MAX_REPLANS {
                continue;
            }
            // ask for that one step shorter (or longer) by the pixels word added there
            for target in asked[index][glyph..].iter_mut() {
                *target -= delta;
            }
            if let Ok(spacing) = spacing_for_pixel_steps(&plan.advances_px, &asked[index]) {
                changed.insert(indexes[index], spacing);
            }
        }
        report.lines_with_misses.push(missing_lines);
        report.glyphs_off_plan.push(off_glyphs);
        report.passes = number;
        last_pdf = Some(pdf);
        if changed.is_empty() {
            break;
        }
        current.extend(changed);
        docx = build(&current)?;
        last_pdf = None;
    }
    let mut kept = HashMap::new();
    if !current.is_empty() {
        // every line keeps the spacing of the pass where it had the fewest misses
        kept = best
            .iter()
            .map(|(&index, (_, spacing))| (indexes[index], spacing.clone()))
            .collect();
        build(&kept)?;
        last_pdf = None;
    }
    report.glyphs_off_plan_final = best.values().map(|(misses, _)| misses).sum();
    Ok(CheckOutcome {
        report,
        kept,
        final_pdf: last_pdf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_miss_finds_the_first_wrong_glyph() {
        // the prototype's case (test_writer_model.py)
        assert_eq!(first_miss(&[0, 10, 20, 30], &[0, 10, 20, 30]), None);
        assert_eq!(first_miss(&[0, 10, 20, 30], &[0, 10, 21, 31]), Some((2, 1)));
    }

    #[test]
    fn lines_are_found_one_text_object_per_glyph() {
        let table = StepTable::new(vec![506, 1586, 2665, 3743, 4823]);
        let tables = HashMap::from([(0, table.clone())]);
        let plan = LinePlan {
            page: 0,
            x: 72.0,
            y: 700.0,
            text: "ab".into(),
            targets_px: vec![0, 50],
            advances_px: vec![50, 50],
            spacing_twips: vec![0, 0],
            start_pixel: Some(600),
            word_y: Some(700.0),
            rotated: false,
            sources: vec![Some(0), Some(1)],
        };
        let object = |pixel: i64| TextObject {
            page: 0,
            tm: [1.0, 0.0, 0.0, 1.0, exported_x(pixel, &table), 700.0],
            codes: vec![97],
        };
        let found = produced_offsets(&[object(651), object(600)], &[&plan], &tables);
        assert_eq!(found, [Some(vec![0, 51])]);
    }
}
