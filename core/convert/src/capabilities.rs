//! what can be converted to what on this computer, each with one plain
//! sentence for the export dialog to show.

use serde::{Deserialize, Serialize};

use crate::office::{OfficeConverter, OfficeConverters, OfficeFormat};

/// the sentence the export dialog shows for "exact match".
pub const EXACT_MATCH_EXPLAINED: &str = "Exact match keeps every page looking just like the PDF, with each line of text placed where it was, so it is best for reading and printing but harder to edit.";

/// the sentence the export dialog shows for "easy to edit".
pub const EASY_TO_EDIT_EXPLAINED: &str = "Easy to edit rebuilds the text as headings, paragraphs, lists and tables that flow as you type, so it is best for changing the document but the layout may shift a little.";

/// every format a conversion starts from or ends in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    Pdf,
    Png,
    Jpeg,
    Tiff,
    Text,
    Html,
    Docx,
    Doc,
    Odt,
    Rtf,
    Xlsx,
    Xls,
    Ods,
    Pptx,
    Ppt,
    Odp,
}

impl From<OfficeFormat> for Format {
    fn from(format: OfficeFormat) -> Self {
        match format {
            OfficeFormat::Docx => Format::Docx,
            OfficeFormat::Doc => Format::Doc,
            OfficeFormat::Odt => Format::Odt,
            OfficeFormat::Rtf => Format::Rtf,
            OfficeFormat::Xlsx => Format::Xlsx,
            OfficeFormat::Xls => Format::Xls,
            OfficeFormat::Ods => Format::Ods,
            OfficeFormat::Pptx => Format::Pptx,
            OfficeFormat::Ppt => Format::Ppt,
            OfficeFormat::Odp => Format::Odp,
        }
    }
}

/// the two ways the export dialog offers for formats that have both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportStyle {
    ExactMatch,
    EasyToEdit,
}

/// one conversion and how it is done here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub from: Format,
    pub to: Format,
    /// exact match or easy to edit, where the format offers both
    pub style: Option<ExportStyle>,
    /// how it is done, in a few words ("Letters", "Microsoft Word")
    pub how: String,
    /// what to expect, in one plain sentence
    pub note: String,
    /// whether it can be done on this computer now
    pub available: bool,
}

fn built_in(from: Format, to: Format, style: Option<ExportStyle>, note: &str) -> Capability {
    Capability {
        from,
        to,
        style,
        how: "Letters".into(),
        note: note.into(),
        available: true,
    }
}

fn not_yet(from: Format, to: Format, style: ExportStyle, note: &str) -> Capability {
    Capability {
        from,
        to,
        style: Some(style),
        how: "Not in this version".into(),
        note: note.into(),
        available: false,
    }
}

/// every conversion Letters knows, with whether it works here. pass what
/// `find_office_converters` found.
pub fn capabilities(converters: &OfficeConverters) -> Vec<Capability> {
    use ExportStyle::*;
    let mut list = vec![
        built_in(Format::Pdf, Format::Png, None, "Each page becomes a picture at the resolution you choose; its text can’t be edited or searched."),
        built_in(Format::Pdf, Format::Jpeg, None, "Each page becomes a smaller picture that suits photos, at the quality you choose; its text can’t be edited or searched."),
        built_in(Format::Pdf, Format::Tiff, None, "Each page becomes a picture that loses nothing, in one file or one per page, as scanners and archives like."),
        built_in(Format::Pdf, Format::Text, Some(EasyToEdit), "Gives just the words in reading order, a paragraph per line, without pictures or formatting."),
        built_in(Format::Pdf, Format::Text, Some(ExactMatch), "Keeps every line where it is on the page using spaces, which suits tables and forms."),
        built_in(Format::Pdf, Format::Html, Some(ExactMatch), "Makes a web page that looks like each PDF page, with real text you can select over a picture of the rest."),
        built_in(Format::Pdf, Format::Html, Some(EasyToEdit), "Makes a clean web page of headings, paragraphs, lists, tables and pictures that fits any screen."),
        built_in(Format::Pdf, Format::Xlsx, None, "Every table becomes real cells with numbers, dates and merged cells, sized like the PDF."),
        built_in(Format::Pdf, Format::Pptx, Some(ExactMatch), "Each page becomes a slide that looks the same, with real text boxes over a picture of anything that can’t be rebuilt."),
        not_yet(Format::Pdf, Format::Docx, ExactMatch, "Word pages that look exactly like the PDF are made by a separate engine that isn’t part of this version yet."),
        not_yet(Format::Pdf, Format::Docx, EasyToEdit, "An editable Word document is coming in a later version."),
        not_yet(Format::Pdf, Format::Odt, EasyToEdit, "An editable OpenDocument text file is coming in a later version."),
    ];
    for picture in [Format::Png, Format::Jpeg, Format::Tiff] {
        list.push(built_in(picture, Format::Pdf, None, "Each picture becomes a page; with text recognition on, its words can be searched and copied."));
    }
    for format in OfficeFormat::ALL {
        list.push(office_capability(format, converters));
    }
    list
}

fn office_capability(format: OfficeFormat, converters: &OfficeConverters) -> Capability {
    let candidates = converters.for_kind(format.kind());
    let (how, note, available) = match candidates.first() {
        Some(OfficeConverter::LibreOffice { .. }) => (
            "LibreOffice".to_string(),
            "Laid out by LibreOffice, which can place some things a little differently from Microsoft Office.".to_string(),
            true,
        ),
        Some(converter) => (
            converter.name().to_string(),
            format!("Laid out by {}, so the PDF looks just as the file does there.", converter.name()),
            true,
        ),
        None => (
            "Needs another program".to_string(),
            "Letters has no layout engine for office files, so this needs Microsoft Office or the free LibreOffice, and neither is installed.".to_string(),
            false,
        ),
    };
    Capability {
        from: format.into(),
        to: Format::Pdf,
        style: None,
        how,
        note,
        available,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_files_need_a_program() {
        let none = capabilities(&OfficeConverters::default());
        let docx = none
            .iter()
            .find(|c| c.from == Format::Docx && c.to == Format::Pdf)
            .unwrap();
        assert!(!docx.available);
        let with_word = capabilities(&OfficeConverters {
            available: vec![OfficeConverter::MicrosoftWord],
            found_but_unused: Vec::new(),
        });
        let docx = with_word.iter().find(|c| c.from == Format::Docx).unwrap();
        assert!(docx.available);
        assert_eq!(docx.how, "Microsoft Word");
        let xlsx = with_word.iter().find(|c| c.from == Format::Xlsx).unwrap();
        assert!(!xlsx.available, "word can't lay out spreadsheets");
    }

    #[test]
    fn every_note_is_one_sentence() {
        for capability in capabilities(&OfficeConverters::default()) {
            let note = &capability.note;
            assert!(note.ends_with('.'), "{note}");
            assert_eq!(
                note.matches(". ").count(),
                0,
                "more than one sentence: {note}"
            );
        }
        for sentence in [EXACT_MATCH_EXPLAINED, EASY_TO_EDIT_EXPLAINED] {
            assert_eq!(sentence.matches(". ").count(), 0);
            assert!(sentence.ends_with('.'));
        }
    }
}
