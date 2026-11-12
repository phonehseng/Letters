//! what the export dialog may offer on this computer.
//!
//! the convert engine lists every conversion with whether it works here
//! (`letters_convert::capabilities`). two of its entries are built in
//! this crate instead: the "easy to edit" word and opendocument files
//! (see `easy_edit`), and the "exact match" word document comes from its
//! own engine through `exact`. this module puts those facts into the list
//! the dialog reads.

use letters_convert::{
    capabilities, Capability, ExportStyle, Format, OfficeConverters, EASY_TO_EDIT_EXPLAINED,
    EXACT_MATCH_EXPLAINED,
};
use serde::Serialize;

use super::exact;

/// everything the dialog needs to draw its list.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCapabilities {
    pub capabilities: Vec<Capability>,
    /// the engine's one sentence for each style, shown under the choice
    pub exact_match_explained: String,
    pub easy_to_edit_explained: String,
    /// word is installed, so exact match is measured through it
    pub word_installed: bool,
}

const EASY_WORD_NOTE: &str = "Headings, paragraphs, lists, tables, pictures and footnotes become a Word document whose text flows as you type.";
const EASY_ODT_NOTE: &str = "Headings, paragraphs, lists, tables, pictures and footnotes become an OpenDocument text file whose text flows as you type.";

/// the list for this computer, given what `find_office_converters` found.
pub fn export_capabilities(converters: &OfficeConverters) -> ExportCapabilities {
    let mut list = capabilities(converters);
    for capability in &mut list {
        adjust(capability);
    }
    ExportCapabilities {
        capabilities: list,
        exact_match_explained: EXACT_MATCH_EXPLAINED.to_string(),
        easy_to_edit_explained: EASY_TO_EDIT_EXPLAINED.to_string(),
        word_installed: crate::writer::export::word_installed(),
    }
}

/// the entries this crate builds, marked available with their own words.
fn adjust(capability: &mut Capability) {
    if capability.from != Format::Pdf {
        return;
    }
    match (capability.to, capability.style) {
        (Format::Docx, Some(ExportStyle::EasyToEdit)) => built_here(capability, EASY_WORD_NOTE),
        (Format::Odt, Some(ExportStyle::EasyToEdit)) => built_here(capability, EASY_ODT_NOTE),
        (Format::Docx, Some(ExportStyle::ExactMatch)) => {
            let status = exact::status();
            capability.available = status.available;
            capability.how = status.how.to_string();
            capability.note = status.note.to_string();
        }
        _ => {}
    }
}

fn built_here(capability: &mut Capability, note: &str) {
    capability.available = true;
    capability.how = "Letters".into();
    capability.note = note.into();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(list: &[Capability], to: Format, style: Option<ExportStyle>) -> Capability {
        list.iter()
            .find(|c| c.from == Format::Pdf && c.to == to && c.style == style)
            .cloned()
            .unwrap()
    }

    #[test]
    fn easy_to_edit_documents_are_offered() {
        let list = export_capabilities(&OfficeConverters::default()).capabilities;
        assert!(find(&list, Format::Docx, Some(ExportStyle::EasyToEdit)).available);
        assert!(find(&list, Format::Odt, Some(ExportStyle::EasyToEdit)).available);
        // the engine's own entries are untouched
        assert!(find(&list, Format::Xlsx, None).available);
    }

    #[test]
    fn exact_match_word_follows_its_engine() {
        let list = export_capabilities(&OfficeConverters::default()).capabilities;
        let exact = find(&list, Format::Docx, Some(ExportStyle::ExactMatch));
        assert_eq!(exact.available, exact::status().available);
        assert!(exact.note.ends_with('.'));
    }

    #[test]
    fn the_style_sentences_come_from_the_engine() {
        let found = export_capabilities(&OfficeConverters::default());
        assert_eq!(found.exact_match_explained, EXACT_MATCH_EXPLAINED);
        assert_eq!(found.easy_to_edit_explained, EASY_TO_EDIT_EXPLAINED);
    }

    #[test]
    fn our_notes_are_one_sentence_each() {
        for note in [EASY_WORD_NOTE, EASY_ODT_NOTE] {
            assert!(note.ends_with('.') && !note.contains(". "), "{note}");
        }
    }
}
