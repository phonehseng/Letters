//! the easy to edit mode's pass of the pipeline: round one's planned lines
//! and the package go to the hybrid builder (`crate::hybrid`), which proves
//! paragraphs through word and writes the docx with the same package parts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::PagePart;
use crate::bridge::WordBridge;
use crate::calibration::WordModel;
use crate::docx::xml::{
    anchor_paragraph, page_break_paragraph, section_break_paragraph, section_properties,
};
use crate::docx::{EmbeddedFont, Package};
use crate::fitted::{build_fitted, FittedContext, FittedOutcome};
use crate::fonts::grid::GridFont;
use crate::hybrid::{self, HybridContext, PageBody, PlannedLine, PlannedPage};
use crate::pdf::PageContent;
use crate::plan::glyphs::with_spacing;
use crate::report::ConversionReport;
use crate::ExactError;

/// what the hybrid builder takes from the conversion besides the pages.
pub(super) struct HybridInputs<'a> {
    pub model: &'a mut WordModel,
    pub docx: &'a Path,
    pub work: &'a Path,
    pub locked_pdf: Option<PathBuf>,
    pub cache: Option<(PathBuf, String)>,
    pub bridge: Option<&'a WordBridge>,
    pub cancel: &'a crate::CancelFlag,
    pub calibration_dir: Option<PathBuf>,
}

/// the hybrid mode ("exact, easy to edit"): round one's planned lines go
/// to the paragraph builder, which writes the docx itself.
pub(super) fn build_hybrid_docx(
    pages: &[PageContent],
    parts: &[PagePart],
    package: &mut Package,
    kept_spacing: &HashMap<usize, Vec<i64>>,
    inputs: HybridInputs,
    export: Option<&mut hybrid::ExportPdf>,
    report: &mut ConversionReport,
) -> Result<hybrid::HybridOutcome, ExactError> {
    let planned = planned_pages(pages, parts, kept_spacing, report);
    let mut write = |bodies: &[PageBody], path: &Path| -> Result<(), ExactError> {
        write_bodies(package, parts, bodies, path)
    };
    let mut context = HybridContext {
        model: inputs.model,
        source: pages,
        export,
        write: &mut write,
        work: inputs.work,
        docx: inputs.docx,
        locked_pdf: inputs.locked_pdf,
        cache: inputs.cache,
        bridge: inputs.bridge,
        cancel: inputs.cancel,
        calibration_dir: inputs.calibration_dir,
    };
    hybrid::build_hybrid(&planned, &mut context)
}

/// round one's planned lines by page, with the spacing the checking export
/// kept and each line's frame as round one writes it.
fn planned_pages(
    pages: &[PageContent],
    parts: &[PagePart],
    kept_spacing: &HashMap<usize, Vec<i64>>,
    report: &ConversionReport,
) -> Vec<PlannedPage> {
    parts
        .iter()
        .zip(pages)
        .map(|(part, page)| PlannedPage {
            index: page.index,
            width: part.width,
            height: part.height,
            lines: part
                .frames
                .iter()
                .filter_map(|frame| {
                    let index = frame.plan_index?;
                    let runs = match kept_spacing.get(&index) {
                        Some(spacing) => with_spacing(&frame.runs, spacing),
                        None => frame.runs.clone(),
                    };
                    Some(PlannedLine {
                        line: frame.line.clone(),
                        plan: report.line_plans[index].clone(),
                        runs,
                        locked_xml: frame.xml(kept_spacing),
                        position: frame.position,
                    })
                })
                .collect(),
        })
        .collect()
}

/// the fitted text path (`crate::fitted`): round one's planned lines get
/// fitted fonts; two exports through word, the second is the result.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_fitted_docx(
    pages: &[PageContent],
    parts: &[PagePart],
    package: &mut Package,
    kept_spacing: &HashMap<usize, Vec<i64>>,
    grids: &HashMap<String, GridFont>,
    docx: &Path,
    work: &Path,
    export: &mut hybrid::ExportPdf,
    report: &mut ConversionReport,
) -> Result<FittedOutcome, ExactError> {
    let planned = planned_pages(pages, parts, kept_spacing, report);
    let base_fonts = package.fonts.clone();
    let mut write =
        |bodies: &[PageBody], fonts: &[EmbeddedFont], path: &Path| -> Result<(), ExactError> {
            package.fonts = fonts.to_vec();
            write_bodies(package, parts, bodies, path)
        };
    let mut context = FittedContext {
        source: pages,
        export,
        write: &mut write,
        work,
        docx,
    };
    build_fitted(&planned, grids, &base_fonts, &mut context)
}

/// writes the docx from hybrid page bodies: the same package as round
/// one's (`write_docx`), with each page's frames replaced by its items.
fn write_bodies(
    package: &mut Package,
    parts: &[PagePart],
    bodies: &[PageBody],
    path: &Path,
) -> Result<(), ExactError> {
    package.body.clear();
    for (index, (part, body)) in parts.iter().zip(bodies).enumerate() {
        package.body.push(anchor_paragraph(&part.drawings.concat()));
        package.body.extend(body.items.iter().cloned());
        if let Some(next) = parts.get(index + 1) {
            if (next.width, next.height) == (part.width, part.height) {
                package.body.push(page_break_paragraph());
            } else {
                package
                    .body
                    .push(section_break_paragraph(part.width, part.height));
            }
        }
    }
    let last = parts
        .last()
        .map(|p| (p.width, p.height))
        .unwrap_or((612.0, 792.0));
    package.final_section = section_properties(last.0, last.1);
    let bytes = package.to_bytes().map_err(crate::error::write_failed)?;
    std::fs::write(path, bytes).map_err(crate::error::write_failed)
}
