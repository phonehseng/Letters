//! the two word steps of the pipeline that bracket the planning: making
//! sure the model facts are known (recipe 9) and the verifying export.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::NOT_CHECKED;
use crate::bridge::{WordBridge, WordInstall};
use crate::calibration::{ensure_measured, CalibrationReport, WordModel, MEASURED_BUILD};
use crate::pdf::PageContent;
use crate::plan::lines::Line;
use crate::report::{ConversionReport, WordReport};
use crate::word::sizes::line_twips_for;
use crate::{verify, ConvertOptions, ExactError};

/// the model facts for this conversion, measured through word when it is
/// there and something is missing.
pub(super) fn prepare_model(
    word: &Option<WordInstall>,
    pages: &[PageContent],
    lines: &[Line],
    bridge: &WordBridge,
    options: &ConvertOptions,
    work: &Path,
    report: &mut ConversionReport,
) -> Result<WordModel, ExactError> {
    let Some(word) = word else {
        report.word = WordReport {
            available: false,
            build: None,
            checked: false,
            note: NOT_CHECKED.into(),
        };
        report.warnings.push(NOT_CHECKED.into());
        let model = WordModel::for_build(MEASURED_BUILD, options.cache_dir.as_deref());
        report.calibration = CalibrationReport {
            word_build: MEASURED_BUILD.into(),
            built_in_tables: true,
            ..CalibrationReport::default()
        };
        return Ok(model);
    };
    report.word = WordReport {
        available: true,
        build: Some(word.build.clone()),
        checked: false,
        note: String::new(),
    };
    let mut model = WordModel::for_build(&word.build, options.cache_dir.as_deref());
    let mut sizes: Vec<(f64, f64)> = Vec::new();
    for page in pages {
        if !sizes.contains(&(page.width, page.height)) {
            sizes.push((page.width, page.height));
        }
    }
    let heights: Vec<i64> = lines
        .iter()
        .filter(|l| !l.is_rotated())
        .map(|l| line_twips_for(l.max_size()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    match ensure_measured(
        &mut model,
        &sizes,
        &heights,
        bridge,
        &work.join("calibration"),
        &options.cancel,
    ) {
        Ok(mut calibration) => {
            calibration.built_in_tables = word.build == MEASURED_BUILD;
            report.calibration = calibration;
            if let Some(dir) = &options.cache_dir {
                if !report.calibration.measured_steps.is_empty()
                    || !report.calibration.measured_baselines.is_empty()
                {
                    let _ = model.save(dir);
                }
            }
        }
        Err(ExactError::Cancelled) => return Err(ExactError::Cancelled),
        Err(error) => {
            // measuring failed: convert with what is known, and say so
            report.warnings.push(format!("Word’s positions couldn’t be measured on this computer, so the measured tables of Word {MEASURED_BUILD} were used. ({error})"));
            model = WordModel::for_build(MEASURED_BUILD, options.cache_dir.as_deref());
            report.calibration = CalibrationReport {
                word_build: MEASURED_BUILD.into(),
                built_in_tables: true,
                ..CalibrationReport::default()
            };
        }
    }
    Ok(model)
}

/// exports the result through word and measures it.
#[allow(clippy::too_many_arguments)]
pub(super) fn verify_result(
    pages: &[PageContent],
    docx: &Path,
    checked_pdf: Option<PathBuf>,
    word: &Option<WordInstall>,
    model: &mut WordModel,
    pdf_bytes: &[u8],
    options: &ConvertOptions,
    work: &Path,
    export: &mut dyn FnMut(&Path, &Path) -> Result<f64, ExactError>,
    report: &mut ConversionReport,
) -> Result<(), ExactError> {
    if word.is_none() {
        report.warnings.push("The result couldn’t be checked, because Microsoft Word isn’t available on this computer.".into());
        return Ok(());
    }
    // the last checking export is of the final docx when no line was re-planned after it
    let (pdf, seconds) = match checked_pdf {
        Some(pdf) => (pdf, 0.0),
        None => {
            let pdf = work.join("verify.word.pdf");
            let seconds = export(docx, &pdf)?;
            (pdf, seconds)
        }
    };
    let substituted: Vec<String> = report
        .fonts
        .iter()
        .filter(|f| f.substitute.is_some())
        .map(|f| f.pdf_font.clone())
        .collect();
    let mut verification = verify::measure(
        pages,
        &pdf,
        model,
        substituted,
        options.render.as_deref(),
        Some(pdf_bytes),
    )?;
    verification.word_export_seconds = seconds;
    report.verification = Some(verification);
    Ok(())
}
