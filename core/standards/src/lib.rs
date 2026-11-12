//! checks and converts pdfs to archival standards such as pdf/a, and
//! validates files before they leave the app.
//!
//! see README.md for what each check covers, how it is tested, and the
//! limits. every conformance claim comes from Letters' own checker; the
//! README explains how to confirm one with an independent validator.

mod error;
mod fonts;
mod images;
mod objects;
mod optimize;
pub mod pdfa;
mod pdfx;
mod preflight;
mod report;
mod walk;

pub use error::{StandardsError, StandardsResult};
pub use images::PlacedImage;
pub use optimize::{optimize, Measure, OptimizeOptions, OptimizeReport};
pub use pdfa::{
    check_pdfa, check_pdfa_file, convert_to_pdfa, save_pdfa, ConversionHelpers, ConversionOptions,
    ConversionReport, FontFile, FontOutcome, FontRequest, FontSource, PageRenderer, PdfaLevel,
    PdfaReport, RasterisedArea, RenderedPage, CHECKER_NOTE,
};
pub use pdfx::{check_pdfx, PdfxLevel, PdfxOptions, PdfxReport};
pub use preflight::{
    preflight, FontLine, PageBoxes, PreflightReport, SizeLine, LOW_RESOLUTION_PPI,
};
pub use report::{Finding, Fix};
