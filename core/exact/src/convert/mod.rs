//! the pipeline (recipe 1): read the pdf, rebuild its fonts, group lines,
//! make sure the model facts are known, plan every line, place rotated
//! lines and drawings, write the docx, check it through word and re-plan
//! the few lines word moved, and say what happened.

mod check;
mod drawings;
mod hybrid_pass;
mod notes;
mod rotated_pass;
mod word_steps;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use lopdf::Object;

use crate::bridge::{find_word, WordBridge};
use crate::calibration::model::is_landscape;
use crate::calibration::{Axis, WordModel};
use crate::docx::xml::{
    anchor_paragraph, frame_paragraph, page_break_paragraph, section_break_paragraph,
    section_properties,
};
use crate::docx::{EmbeddedFont, Package};
use crate::fonts::embed::font_key_from;
use crate::fonts::rebuild::fnv1a;
use crate::fonts::FontPlan;
use crate::hybrid;
use crate::pdf::objects::get;
use crate::pdf::{PageContent, PdfFile};
use crate::plan::glyphs::{plan_line, with_spacing, CharRun, GlyphCounts, Targets};
use crate::plan::lines::{group_lines, script_counts, Line};
use crate::plan::placement::{place, BASELINE_SHARE};
use crate::plan::rotated::{RotatedLine, EXTRA_WIDTH_PT};
use crate::report::{ConversionReport, FramePosition, PageReport};
use crate::units::{pt_to_twips, TWIPS_PER_PT};
use crate::word::sizes::line_twips_for;
use crate::word::writer_model::{page_pixels, StepTable};
use crate::{ConvertOptions, ExactError, OutputMode, Progress, Stage};
use hybrid_pass::{build_fitted_docx, build_hybrid_docx, HybridInputs};
use notes::describe_fonts;
use rotated_pass::place_rotated;
use word_steps::{prepare_model, verify_result};

pub(super) const NOT_CHECKED: &str = "Not checked against Word on this computer.";

/// one horizontal line's frame.
struct Frame {
    /// the source line, for the hybrid mode's paragraph detection
    line: Line,
    plan_index: Option<usize>,
    runs: Vec<CharRun>,
    position: FramePosition,
}

impl Frame {
    fn xml(&self, overrides: &HashMap<usize, Vec<i64>>) -> String {
        let runs = match self.plan_index.and_then(|i| overrides.get(&i)) {
            Some(spacing) => with_spacing(&self.runs, spacing),
            None => self.runs.clone(),
        };
        let p = &self.position;
        frame_paragraph(&runs, p.x_twips, p.y_twips, p.width_twips, p.line_twips)
    }
}

/// a page of the docx being built.
struct PagePart {
    width: f64,
    height: f64,
    drawings: Vec<String>,
    frames: Vec<Frame>,
}

/// a work folder, removed afterwards unless the caller wants it kept.
struct WorkFolder {
    path: PathBuf,
    keep: bool,
}

impl WorkFolder {
    fn new(options: &ConvertOptions) -> Result<Self, ExactError> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let (path, keep) = match &options.work_dir {
            Some(dir) => (dir.clone(), true),
            None => {
                let number = NEXT.fetch_add(1, Ordering::SeqCst);
                (
                    std::env::temp_dir()
                        .join("letters-exact")
                        .join(format!("{}-{number}", std::process::id())),
                    options.keep_work,
                )
            }
        };
        std::fs::create_dir_all(&path).map_err(crate::error::write_failed)?;
        Ok(Self { path, keep })
    }
}

impl Drop for WorkFolder {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn report_progress(
    options: &ConvertOptions,
    stage: Stage,
    fraction: f32,
) -> Result<(), ExactError> {
    if let Some(progress) = &options.progress {
        progress(Progress { stage, fraction });
    }
    if options.cancel.is_cancelled() {
        return Err(ExactError::Cancelled);
    }
    Ok(())
}

/// converts `pdf_path` to `docx_path` (replacing it).
pub fn convert(
    pdf_path: &Path,
    docx_path: &Path,
    options: &ConvertOptions,
) -> Result<ConversionReport, ExactError> {
    let started = Instant::now();
    report_progress(options, Stage::Reading, 0.0)?;
    let pdf_bytes =
        std::fs::read(pdf_path).map_err(|e| ExactError::CannotOpen(format!("({e})")))?;
    let file = PdfFile::from_bytes(&pdf_bytes)?;
    let pages = file.read_pages();
    let all_glyphs: Vec<_> = pages
        .iter()
        .flat_map(|p| p.glyphs.iter().cloned())
        .collect();
    if all_glyphs.is_empty() {
        return Err(ExactError::NoText);
    }
    let work = WorkFolder::new(options)?;
    let mut report = ConversionReport {
        source: pdf_path.display().to_string(),
        docx: docx_path.display().to_string(),
        glyphs: all_glyphs.len(),
        scripts: script_counts(&all_glyphs),
        pdf_bytes: pdf_bytes.len() as u64,
        ..ConversionReport::default()
    };

    report_progress(options, Stage::Fonts, 0.1)?;
    let mut fonts = FontPlan::new(&all_glyphs, fnv1a(&pdf_bytes));
    report.fonts = fonts.reports.clone();
    if options.mode == OutputMode::EasyToEdit {
        // text a person types must stay in the document's typeface
        report.font_completion = fonts.complete_faces();
        for completion in &report.font_completion {
            report.warnings.extend(completion.warning.clone());
        }
    }
    let lines = group_lines(&all_glyphs);
    report.lines = lines.len();

    report_progress(options, Stage::Measuring, 0.2)?;
    let word = if options.use_word { find_word() } else { None };
    let bridge = WordBridge {
        seconds_per_document: options.word_seconds_per_document,
        work_dir: work.path.join("word"),
    };
    let mut model = prepare_model(
        &word,
        &pages,
        &lines,
        &bridge,
        options,
        &work.path,
        &mut report,
    )?;

    report_progress(options, Stage::Planning, 0.4)?;
    let mut counts = GlyphCounts::default();
    let mut package = Package::default();
    let mut parts = Vec::new();
    let mut rotated: Vec<(RotatedLine, Line)> = Vec::new();
    let page_ids = file.page_ids();
    for page in &pages {
        let mut page_report = PageReport {
            index: page.index,
            width_pt: page.width,
            height_pt: page.height,
            glyphs: page.glyphs.len(),
            problems: page.problems.clone(),
            ..PageReport::default()
        };
        let resources = page_resources(&file, page_ids.get(page.index).copied());
        let drawings = drawings::page_drawings(
            &mut package,
            page,
            &file.document,
            resources.as_ref(),
            options.render.as_deref(),
            &mut page_report,
        );
        let mut frames = Vec::new();
        for line in lines.iter().filter(|l| l.page == page.index) {
            page_report.lines += 1;
            page_report.hidden_lines += usize::from(line.hidden());
            let line_twips = line_twips_for(line.max_size());
            if line.is_rotated() {
                page_report.rotated_lines += 1;
                if let Some((plan, runs)) = plan_line(
                    line,
                    &mut fonts,
                    Targets::Along { offset_pt: 0.0 },
                    &mut counts,
                )
                .map_err(ExactError::WordFailed)?
                {
                    report.line_plans.push(plan);
                    rotated.push((
                        RotatedLine {
                            page: line.page,
                            text: runs.iter().map(|r| r.char.as_str()).collect(),
                            runs,
                            origin_top_down: (line.x(), page.height - line.y()),
                            angle: line.angle,
                            width_pt: line.width() + EXTRA_WIDTH_PT,
                            line_twips,
                            baseline_pt: BASELINE_SHARE * line_twips as f64 / TWIPS_PER_PT,
                            correction: (0.0, 0.0),
                            plan_index: Some(report.line_plans.len() - 1),
                        },
                        line.clone(),
                    ));
                }
                continue;
            }
            if let Some(frame) = plan_frame(
                line,
                page,
                line_twips,
                &mut model,
                &mut fonts,
                &mut counts,
                &mut report,
            )? {
                frames.push(frame);
            }
        }
        report.pages.push(page_report);
        parts.push(PagePart {
            width: page.width,
            height: page.height,
            drawings,
            frames,
        });
    }
    report.glyph_counts = counts;
    report.frames = parts
        .iter()
        .flat_map(|p| p.frames.iter().map(|f| f.position))
        .collect();

    // every grid font is known now: embed them
    let document_tag = fnv1a(&pdf_bytes);
    for grid in fonts.grid_fonts() {
        let seed = document_tag ^ fnv1a(grid.family.as_bytes());
        package.fonts.push(EmbeddedFont {
            family: grid.family.clone(),
            bytes: grid.bytes.clone(),
            font_key: font_key_from(seed),
        });
    }
    report.grid_fonts = package.fonts.len();
    describe_fonts(&mut report);

    let page_sizes: Vec<(f64, f64)> = pages.iter().map(|p| (p.width, p.height)).collect();
    let mut export = |docx: &Path, pdf: &Path| -> Result<f64, ExactError> {
        let run = bridge.export_one(docx, pdf, &options.cancel)?;
        Ok(run.seconds)
    };
    let mut rotated_lines = place_rotated(
        &mut rotated,
        &page_sizes,
        &package.fonts,
        &word,
        &work.path,
        &mut fonts,
        &mut report,
        &mut export,
    )?;
    for (page_index, part) in parts.iter_mut().enumerate() {
        for line in rotated_lines.iter_mut().filter(|l| l.page == page_index) {
            let id = package.shape_id();
            part.drawings.push(line.drawing(id));
        }
    }

    report_progress(options, Stage::Writing, 0.6)?;
    let mut build = |overrides: &HashMap<usize, Vec<i64>>| -> Result<PathBuf, ExactError> {
        write_docx(&mut package, &parts, overrides, docx_path)?;
        Ok(docx_path.to_path_buf())
    };
    let mut checked_pdf = None;
    let mut kept_spacing: HashMap<usize, Vec<i64>> = HashMap::new();
    if word.is_some() && options.check_passes {
        report_progress(options, Stage::Checking, 0.7)?;
        let tables: HashMap<usize, StepTable> = pages
            .iter()
            .map(|p| {
                let landscape = is_landscape(p.width, p.height);
                (
                    p.index,
                    model.table(Axis::X, page_pixels(p.width), landscape),
                )
            })
            .collect();
        let outcome = check::check_and_replan(
            &report.line_plans,
            &tables,
            &work.path,
            &mut build,
            &mut export,
        )?;
        if outcome.report.lines_not_found > 0 {
            report.warnings.push(format!(
                "{} line{} couldn’t be checked one by one in Word’s output, because Word printed positions the measured tables don’t explain.",
                outcome.report.lines_not_found,
                if outcome.report.lines_not_found == 1 { "" } else { "s" }
            ));
        }
        // the plans say what the docx asks of word now
        for (index, spacing) in &outcome.kept {
            report.line_plans[*index].spacing_twips = spacing.clone();
        }
        report.check = Some(outcome.report);
        checked_pdf = outcome.final_pdf;
        kept_spacing = outcome.kept;
        report.word.checked = true;
    } else {
        build(&HashMap::new())?;
    }
    report.mode = options.mode;
    if options.mode == OutputMode::EasyToEdit {
        report_progress(options, Stage::Promoting, 0.75)?;
        let outcome = build_hybrid_docx(
            &pages,
            &parts,
            &mut package,
            &kept_spacing,
            HybridInputs {
                model: &mut model,
                docx: docx_path,
                work: &work.path,
                locked_pdf: checked_pdf.take(),
                cache: options.cache_dir.as_ref().map(|dir| {
                    let build = word.as_ref().map(|w| w.build.as_str()).unwrap_or("none");
                    (dir.clone(), hybrid::cache::key(document_tag, build))
                }),
                bridge: word.as_ref().map(|_| &bridge),
                cancel: &options.cancel,
                calibration_dir: options.cache_dir.clone(),
            },
            if word.is_some() {
                Some(&mut export as &mut hybrid::ExportPdf)
            } else {
                None
            },
            &mut report,
        )?;
        checked_pdf = outcome.final_pdf;
        report.hybrid = Some(outcome.report);
    }
    if options.mode == OutputMode::FittedLayout {
        match word.is_some() {
            true => {
                report_progress(options, Stage::Promoting, 0.75)?;
                let grids = fonts.grid_fonts().into_iter().map(|g| (g.family.clone(), g.clone())).collect();
                let outcome = build_fitted_docx(&pages, &parts, &mut package, &kept_spacing, &grids, docx_path, &work.path, &mut export, &mut report)?;
                checked_pdf = Some(outcome.final_pdf);
                report.fitted = Some(outcome.report);
            }
            // no checking export is possible: round one's recipe is kept
            false => report.warnings.push("Word isn’t on this computer, so the fitted fonts couldn’t be checked; every line keeps its exact positioned form instead.".into()),
        }
    }
    report.docx_bytes = std::fs::metadata(docx_path).map(|m| m.len()).unwrap_or(0);

    if options.verify {
        report_progress(options, Stage::Verifying, 0.9)?;
        verify_result(
            &pages,
            docx_path,
            checked_pdf,
            &word,
            &mut model,
            &pdf_bytes,
            options,
            &work.path,
            &mut export,
            &mut report,
        )?;
        if let (Some(fitted), Some(verification)) = (&report.fitted, report.verification.as_mut()) {
            crate::verify::use_fitted_glyphs(verification, fitted.glyphs.clone());
        }
    }
    report.calibration.estimated_steps = model.estimated.clone();
    if !model.estimated.is_empty() {
        report.warnings.push(format!(
            "Word’s print positions for this page size ({}) were estimated, not measured, so positions may be up to a few hundredths of a point further off.",
            model.estimated.join(", ")
        ));
    }
    report.seconds = started.elapsed().as_secs_f64();
    report_progress(options, Stage::Done, 1.0)?;
    Ok(report)
}

/// the page's resources object, for copying into rasterised pages.
fn page_resources(file: &PdfFile, page_id: Option<lopdf::ObjectId>) -> Option<Object> {
    let document = &file.document;
    let mut node = document.get_dictionary(page_id?).ok()?;
    for _ in 0..32 {
        if let Ok(resources) = node.get(b"Resources") {
            return Some(resources.clone());
        }
        node = match get(document, node, b"Parent") {
            Some(Object::Dictionary(parent)) => parent,
            _ => return None,
        };
    }
    None
}

/// plans one horizontal line in its frame (recipe 7.1 to 7.5).
fn plan_frame(
    line: &Line,
    page: &PageContent,
    line_twips: i64,
    model: &mut WordModel,
    fonts: &mut FontPlan,
    counts: &mut GlyphCounts,
    report: &mut ConversionReport,
) -> Result<Option<Frame>, ExactError> {
    let baseline_from_top = page.height - line.y();
    let placement = place(
        model,
        line.x(),
        baseline_from_top,
        line_twips,
        page.width,
        page.height,
    );
    let landscape = is_landscape(page.width, page.height);
    let x_table = model.table(Axis::X, page_pixels(page.width), landscape);
    let targets = Targets::Lattice {
        table: &x_table,
        start_pixel: placement.x_pixel,
        word_y: page.height - placement.baseline_from_top_pt,
    };
    let Some((plan, runs)) =
        plan_line(line, fonts, targets, counts).map_err(ExactError::WordFailed)?
    else {
        return Ok(None);
    };
    report.line_plans.push(plan);
    let width = (line.width() + 40.0).min((page.width - placement.x_pt).max(line.width() + 4.0));
    Ok(Some(Frame {
        line: line.clone(),
        plan_index: Some(report.line_plans.len() - 1),
        runs,
        position: FramePosition {
            page: page.index,
            x_twips: placement.x_twips,
            y_twips: placement.y_twips,
            width_twips: pt_to_twips(width),
            line_twips,
        },
    }))
}

/// writes the docx, with some lines' spacing replaced.
fn write_docx(
    package: &mut Package,
    parts: &[PagePart],
    overrides: &HashMap<usize, Vec<i64>>,
    path: &Path,
) -> Result<(), ExactError> {
    package.body.clear();
    for (index, part) in parts.iter().enumerate() {
        package.body.push(anchor_paragraph(&part.drawings.concat()));
        for frame in &part.frames {
            package.body.push(frame.xml(overrides));
        }
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
