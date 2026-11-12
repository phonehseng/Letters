//! the office engine's whole-document helpers, called with the model json
//! the ui holds: the mla and apa presets, and the ruler's plain sentences.
//!
//! the ui sends the model without carry and picture bytes (see
//! document.rs); neither is needed here, so the json goes straight in.

use letters_office::model::Document;
use letters_office::presets::{apply_apa, apply_mla, ApaDetails, MlaDetails};
use letters_office::ruler::{describe_ruler, RulerState};
use serde::Deserialize;
use serde_json::Value;

use crate::writer::document::ui_model;

/// what the Document tab asks for before a preset (all optional).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PresetDetails {
    pub name: String,
    pub instructor: String,
    pub course: String,
    pub date: String,
    pub title: String,
    /// add mla's name block, or apa's title page, at the top
    pub add_heading: bool,
}

fn read(model: Value) -> Result<Document, String> {
    serde_json::from_value(model)
        .map_err(|error| format!("The document couldn’t be read. ({error})"))
}

/// the model after "MLA" or "APA" has been applied, for the editor to
/// take over as one undoable step.
pub fn apply_preset(model: Value, preset: &str, details: &PresetDetails) -> Result<Value, String> {
    let mut document = read(model)?;
    match preset {
        "mla" => apply_mla(
            &mut document,
            &MlaDetails {
                student_name: details.name.clone(),
                instructor: details.instructor.clone(),
                course: details.course.clone(),
                date: details.date.clone(),
                title: details.title.clone(),
                insert_heading: details.add_heading,
            },
        ),
        "apa" => apply_apa(
            &mut document,
            &ApaDetails {
                title: details.title.clone(),
                author: details.name.clone(),
                affiliation: String::new(),
                course: details.course.clone(),
                instructor: details.instructor.clone(),
                due_date: details.date.clone(),
                running_head: None,
                insert_title_page: details.add_heading,
            },
        ),
        other => return Err(format!("There is no preset called “{other}”.")),
    }
    Ok(ui_model(&document))
}

/// the ruler for one top-level block, with the engine's sentences.
pub fn ruler_for(model: Value, block_index: usize) -> Result<RulerState, String> {
    let document = read(model)?;
    Ok(describe_ruler(&document, block_index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::Block;

    fn model() -> Value {
        let mut document = Document::new();
        document.blocks = vec![Block::text("Body text.")];
        ui_model(&document)
    }

    #[test]
    fn mla_sets_double_spacing_and_the_running_header() {
        let details = PresetDetails {
            name: "Ada Lovelace".into(),
            ..Default::default()
        };
        let after: Document =
            serde_json::from_value(apply_preset(model(), "mla", &details).unwrap()).unwrap();
        let normal = after.style("Normal").unwrap();
        assert_eq!(normal.paragraph.spacing.line.unwrap().value, 480);
        let header = after.header_footer("mla-header").unwrap();
        assert_eq!(
            header.blocks[0].paragraph().unwrap().plain_text(),
            "Lovelace 1"
        );
    }

    #[test]
    fn apa_with_a_title_page_adds_a_page_break() {
        let details = PresetDetails {
            title: "On Engines".into(),
            add_heading: true,
            ..Default::default()
        };
        let after: Document =
            serde_json::from_value(apply_preset(model(), "apa", &details).unwrap()).unwrap();
        assert!(after
            .blocks
            .iter()
            .any(|block| matches!(block, Block::PageBreak)));
    }

    #[test]
    fn an_unknown_preset_is_refused() {
        assert!(apply_preset(model(), "chicago", &PresetDetails::default()).is_err());
    }

    #[test]
    fn the_ruler_speaks_in_sentences() {
        let ruler = ruler_for(model(), 0).unwrap();
        assert_eq!(ruler.left_margin, 1440);
        assert!(ruler.description[0].starts_with("The page is 8.50 in wide."));
    }
}
