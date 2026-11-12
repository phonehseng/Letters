//! pictures to a pdf: one page per picture, through letters-ocr, which
//! can also read each picture's text and lay it over the page invisibly
//! so the pdf can be searched.

use std::path::{Path, PathBuf};

use letters_ocr::{CancelFlag, Engine, ImageSource, ImagesToPdfOptions, OcrOptions, Progress};

use crate::ConvertError;

/// makes a pdf at `output` from the pictures, in the order given (png,
/// jpeg or tiff). pass `read_text` to add a searchable text layer. the
/// file is written next to `output` and moved into place when complete.
pub fn images_to_pdf_file(
    images: &[PathBuf],
    options: &ImagesToPdfOptions,
    read_text: Option<(&Engine, &OcrOptions)>,
    output: &Path,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<(), ConvertError> {
    let sources: Vec<ImageSource> = images.iter().cloned().map(ImageSource::File).collect();
    let mut document = letters_ocr::images_to_pdf(&sources, options, read_text, progress, cancel)?;
    document.compress();
    let partial = output.with_extension("pdf.letters-partial");
    let saved = document
        .save(&partial)
        .map_err(|error| ConvertError::WriteFailed(format!("({error})")))
        .and_then(|_| std::fs::rename(&partial, output).map_err(crate::error::write_failed));
    if saved.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    saved
}
