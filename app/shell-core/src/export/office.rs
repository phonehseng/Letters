//! office files (word, spreadsheets, slides, opendocument, rtf) turned
//! into pdfs by a program on this computer, for "Convert to PDF" and for
//! showing a spreadsheet or a presentation as a pdf.

use std::path::{Path, PathBuf};

use letters_convert::{
    find_office_converters, office_to_pdf, CancelFlag, OfficeFormat, OfficeOptions,
};
use serde::Serialize;

/// the pdf made and the program that laid it out.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertedToPdf {
    pub pdf: PathBuf,
    /// "Microsoft Word", "LibreOffice"
    pub program: String,
    /// "Made with Microsoft Excel."
    pub summary: String,
}

/// whether a file is one an office program can turn into a pdf.
pub fn is_office_file(path: &Path) -> bool {
    OfficeFormat::of_path(path).is_some()
}

/// converts `input` into a pdf at `output`.
pub fn convert_to_pdf(
    input: &Path,
    output: &Path,
    cancel: &CancelFlag,
) -> Result<ConvertedToPdf, String> {
    let converters = find_office_converters();
    let done = office_to_pdf(
        input,
        output,
        &converters,
        &OfficeOptions::default(),
        cancel,
    )
    .map_err(|error| error.to_string())?;
    Ok(ConvertedToPdf {
        pdf: output.to_path_buf(),
        summary: format!("Made with {}.", done.program),
        program: done.program,
    })
}

/// a pdf of an office file in the temporary folder, for showing it in the
/// viewer without writing anything beside the person's file.
pub fn shown_as_pdf(input: &Path, cancel: &CancelFlag) -> Result<ConvertedToPdf, String> {
    let folder = std::env::temp_dir().join("letters-shown-as-pdf");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into());
    let output = folder.join(format!("{stem}.pdf"));
    convert_to_pdf(input, &output, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_files_are_recognised_by_name() {
        for name in [
            "a.docx", "b.XLSX", "c.pptx", "d.odt", "e.ods", "f.odp", "g.rtf",
        ] {
            assert!(is_office_file(Path::new(name)), "{name}");
        }
        assert!(!is_office_file(Path::new("h.pdf")));
        assert!(!is_office_file(Path::new("i.png")));
    }
}
