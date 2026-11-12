//! the PDF standards tools of Prepare & Check, the parts that need no
//! window: converting a document to pdf/a as a copy, making a smaller
//! copy, and lending the conversion fonts and a page renderer.
//!
//! the checks and conversions are core/standards; every conformance claim
//! is that crate's own checker (CHECKER_NOTE says so wherever a result is
//! shown). converting and optimising never change the open document: they
//! make the bytes of a new file, which the shell writes where the person
//! chose.

pub mod font_source;

use std::path::{Path, PathBuf};

use letters_pdf::RenderWorker;
use letters_standards::{
    convert_to_pdfa, optimize, save_pdfa, ConversionHelpers, ConversionOptions, ConversionReport,
    FontSource, OptimizeOptions, OptimizeReport, PageRenderer, PdfaLevel, RenderedPage,
    StandardsError,
};
use lopdf::Document;

use crate::export::renderer::draw_page;

pub use font_source::LibraryFonts;

/// why a signed document isn't converted, and what to do instead.
pub const SIGNED_SOURCE: &str = "This document is digitally signed. A converted copy is a rewritten file, so its signatures would no longer check out, and PDF/A doesn’t allow that. Convert the document before it is signed, then sign the PDF/A copy.";

/// a converted copy: what the conversion did, and the file's bytes.
pub struct ConvertedCopy {
    pub report: ConversionReport,
    pub bytes: Vec<u8>,
}

/// converts the pdf at `source` to `level` and returns the new file's
/// bytes, saved the way pdf/a needs (`save_pdfa`). the source is not
/// changed. `fonts` and `renderer` may be missing; then what needs them
/// stays a blocker in the report.
pub fn convert_copy(
    source: &Path,
    level: PdfaLevel,
    fonts: Option<&dyn FontSource>,
    renderer: Option<&dyn PageRenderer>,
) -> Result<ConvertedCopy, String> {
    // a converted copy is rewritten, so a signature in it no longer covers
    // the file, which pdf/a forbids (iso 19005-2 6.4.3; veraPDF refused such
    // a copy in the app while core/standards' checker passed it)
    if crate::forms::signed::looks_signed(source) {
        return Err(SIGNED_SOURCE.to_owned());
    }
    let mut document = load(source)?;
    let helpers = ConversionHelpers { fonts, renderer };
    let report = convert_to_pdfa(
        &mut document,
        level,
        &ConversionOptions::default(),
        &helpers,
    )
    .map_err(|error| error.to_string())?;
    let bytes = save_pdfa(&mut document).map_err(|error| error.to_string())?;
    Ok(ConvertedCopy { report, bytes })
}

/// a smaller copy of the pdf at `source`; `report.bytes` is the new file.
pub fn optimized_copy(source: &Path, options: &OptimizeOptions) -> Result<OptimizeReport, String> {
    let mut document = load(source)?;
    optimize(&mut document, options).map_err(|error| error.to_string())
}

fn load(source: &Path) -> Result<Document, String> {
    Document::load(source).map_err(|error| StandardsError::from(error).to_string())
}

/// draws pages of a pdf file with the app's engine, for pdf/a-1
/// transparency. the conversion asks for pages of the original document,
/// so this draws the file the conversion started from.
pub struct FileRenderer<'a> {
    pub worker: &'a RenderWorker,
    pub pdf: PathBuf,
}

impl PageRenderer for FileRenderer<'_> {
    fn render(&self, page: u32, pixels_per_point: f32) -> Option<RenderedPage> {
        let drawn = draw_page(self.worker, &self.pdf, page as usize, pixels_per_point).ok()?;
        Some(RenderedPage {
            width: drawn.width,
            height: drawn.height,
            rgb: rgb_on_white(&drawn.rgba),
        })
    }
}

/// rgba pixels as rgb, see-through parts laid on white paper.
pub fn rgb_on_white(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for pixel in rgba.as_chunks::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for channel in &pixel[..3] {
            let on_white = (u32::from(*channel) * alpha + 255 * (255 - alpha)) / 255;
            rgb.push(on_white as u8);
        }
    }
    rgb
}

#[cfg(test)]
mod tests;
