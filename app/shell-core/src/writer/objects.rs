//! moving things around the page, through the office engine's
//! `floating` module: text blocks made from text and dissolved back,
//! pictures toggled between "in the line" and "free", stacking order,
//! aligning and distributing, and strict word mode's fixes.
//!
//! the editor sends the whole model and gets the whole model back; it
//! then replaces only the blocks that changed, so each command is one
//! undo step and the rest of the document is left alone.

use letters_office::compat::{self, Feature, Report};
use letters_office::floating::{self, Edge, ObjectRef};
use letters_office::model::{Document, Twips, Wrap};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::writer::document::ui_model;

/// one command on the page's objects, as the editor names it.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ObjectOp {
    /// the paragraph at `block` becomes two text blocks, split `offset`
    /// characters in
    #[serde(rename_all = "camelCase")]
    SplitIntoTextBoxes {
        block: usize,
        offset: usize,
    },
    /// blocks `start..end` move into one new text block
    #[serde(rename_all = "camelCase")]
    TextBoxFromBlocks {
        start: usize,
        end: usize,
    },
    /// the text block's content goes back into the flow at block `at`
    #[serde(rename_all = "camelCase")]
    DissolveTextBox {
        block: usize,
        at: usize,
    },
    /// a picture in the line floats; `hint` is where it shows now, in page
    /// coordinates (twips from the page corner)
    #[serde(rename_all = "camelCase")]
    FloatImage {
        block: usize,
        inline: usize,
        hint: Option<(Twips, Twips)>,
        wrap: Wrap,
    },
    /// a free picture goes back in the line, at (block, character offset)
    #[serde(rename_all = "camelCase")]
    InlineImage {
        block: usize,
        inline: usize,
        to: Option<(usize, usize)>,
    },
    BringToFront {
        object: ObjectRef,
    },
    SendToBack {
        object: ObjectRef,
    },
    Align {
        objects: Vec<ObjectRef>,
        edge: Edge,
    },
    Distribute {
        objects: Vec<ObjectRef>,
        horizontal: bool,
    },
    /// the automatic fixes strict word mode offers: all of them, or only
    /// the report's issue number `only`
    ApplyFixes {
        #[serde(default)]
        only: Option<usize>,
    },
}

/// what a command did: the new model (without carry and picture bytes),
/// whether anything changed, and where the thing it acted on is now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectAnswer {
    pub model: Value,
    pub changed: bool,
    /// the blocks the command made or moved (start, end), when it says
    pub blocks: Option<(usize, usize)>,
    /// a picture's new place (block, inline) after it went back in the line
    pub image: Option<(usize, usize)>,
    /// a plain sentence when the command could not do what was asked
    pub refused: Option<String>,
}

/// applies `op` to `document` (already merged with carry and pictures).
pub fn apply(document: &mut Document, op: &ObjectOp) -> ObjectAnswer {
    let before = serde_json::to_value(&document.blocks).unwrap_or(Value::Null);
    let mut blocks = None;
    let mut image = None;
    let mut refused = None;
    match op {
        ObjectOp::SplitIntoTextBoxes { block, offset } => {
            match floating::split_paragraph_into_text_boxes(document, *block, *offset) {
                Some((first, second)) => blocks = Some((first, second + 1)),
                None => refused = Some("Click inside a paragraph to split it into text blocks.".into()),
            }
        }
        ObjectOp::TextBoxFromBlocks { start, end } => match floating::text_box_from_blocks(document, *start, *end) {
            Some(index) => blocks = Some((index, index + 1)),
            None => refused = Some("A text block can’t hold a section break or another text block. Select paragraphs only.".into()),
        },
        ObjectOp::DissolveTextBox { block, at } => match floating::dissolve_text_box(document, *block, *at) {
            Some(range) => blocks = Some((range.start, range.end)),
            None => refused = Some("Click inside a text block first.".into()),
        },
        ObjectOp::FloatImage { block, inline, hint, wrap } => {
            if !floating::float_image(document, *block, *inline, *hint, *wrap) {
                refused = Some("Click a picture first.".into());
            }
        }
        ObjectOp::InlineImage { block, inline, to } => match floating::inline_image(document, *block, *inline, *to) {
            Some(place) => image = Some(place),
            None => refused = Some("Click a picture first.".into()),
        },
        ObjectOp::BringToFront { object } => floating::bring_to_front(document, *object),
        ObjectOp::SendToBack { object } => floating::send_to_back(document, *object),
        ObjectOp::Align { objects, edge } => {
            if floating::align(document, objects, *edge).is_empty() {
                refused = Some(placed_only_sentence(2));
            }
        }
        ObjectOp::Distribute { objects, horizontal } => {
            if floating::distribute(document, objects, *horizontal).is_empty() {
                refused = Some(placed_only_sentence(3));
            }
        }
        ObjectOp::ApplyFixes { only } => {
            let mut report = compat::word_compatibility(document);
            if let Some(index) = only {
                report.issues = report.issues.into_iter().skip(*index).take(1).collect();
            }
            compat::apply_fixes(document, &report);
        }
    }
    let after = serde_json::to_value(&document.blocks).unwrap_or(Value::Null);
    let changed = before != after || matches!(op, ObjectOp::ApplyFixes { .. });
    ObjectAnswer {
        model: ui_model(document),
        changed,
        blocks,
        image,
        refused,
    }
}

fn placed_only_sentence(at_least: usize) -> String {
    format!(
        "Select at least {at_least} objects placed on the page. Objects that move with a paragraph have no fixed place until they are dragged once."
    )
}

/// strict word mode's two answers about a document: the engine's feature
/// table and its report of what word would show differently.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compatibility {
    pub features: Vec<Feature>,
    pub report: Report,
}

pub fn compatibility(document: &Document) -> Compatibility {
    Compatibility {
        features: compat::feature_matrix(),
        report: compat::word_compatibility(document),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Block, Inline, Paragraph};

    fn document(text: &str) -> Document {
        let mut document = Document::new();
        document.blocks = vec![Block::text("first"), Block::text(text), Block::text("last")];
        document
    }

    fn op(json: Value) -> ObjectOp {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn a_paragraph_splits_into_two_text_blocks_and_comes_back() {
        let mut doc = document("Hello world");
        let answer = apply(
            &mut doc,
            &op(serde_json::json!({ "op": "splitIntoTextBoxes", "block": 1, "offset": 6 })),
        );
        assert!(answer.changed);
        assert_eq!(answer.blocks, Some((1, 3)));
        let blocks = answer.model["blocks"].as_array().unwrap();
        assert_eq!(blocks[1]["type"], "textBox");
        assert_eq!(blocks[2]["type"], "textBox");
        assert_eq!(blocks[1]["blocks"][0]["content"][0]["text"], "Hello ");
        assert_eq!(blocks[2]["blocks"][0]["content"][0]["text"], "world");

        let back = apply(
            &mut doc,
            &op(serde_json::json!({ "op": "dissolveTextBox", "block": 2, "at": 2 })),
        );
        assert_eq!(back.blocks, Some((2, 3)));
        assert_eq!(back.model["blocks"][2]["type"], "paragraph");
    }

    #[test]
    fn a_selection_of_paragraphs_becomes_one_text_block() {
        let mut doc = document("middle");
        let answer = apply(
            &mut doc,
            &op(serde_json::json!({ "op": "textBoxFromBlocks", "start": 0, "end": 2 })),
        );
        assert_eq!(answer.blocks, Some((0, 1)));
        assert_eq!(
            answer.model["blocks"][0]["blocks"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        // a text block inside a text block is refused with a sentence
        let refused = apply(
            &mut doc,
            &op(serde_json::json!({ "op": "textBoxFromBlocks", "start": 0, "end": 2 })),
        );
        assert!(refused.refused.is_some());
        assert!(!refused.changed);
    }

    #[test]
    fn a_picture_floats_at_its_place_and_goes_back_in_the_line() {
        let mut doc = Document::new();
        let image: Inline = serde_json::from_value(serde_json::json!({ "type": "image", "mediaId": "a.png", "width": 1440, "height": 720 })).unwrap();
        doc.blocks = vec![Block::Paragraph(Paragraph {
            content: vec![Inline::text("Before "), image, Inline::text("after")],
            ..Default::default()
        })];
        let floated = apply(
            &mut doc,
            &op(
                serde_json::json!({ "op": "floatImage", "block": 0, "inline": 1, "hint": [2000, 3000], "wrap": "square" }),
            ),
        );
        let placement = &floated.model["blocks"][0]["content"][1]["placement"];
        assert_eq!(placement["type"], "floating");
        assert_eq!(placement["horizontal"]["relativeTo"], "page");
        assert_eq!(placement["vertical"]["offset"], 3000);
        let inline = apply(
            &mut doc,
            &op(serde_json::json!({ "op": "inlineImage", "block": 0, "inline": 1, "to": [0, 3] })),
        );
        assert_eq!(inline.image, Some((0, 1)));
        assert_eq!(
            inline.model["blocks"][0]["content"][1]["placement"]["type"],
            "inline"
        );
        // floating again restores the remembered place exactly
        let again = apply(
            &mut doc,
            &op(
                serde_json::json!({ "op": "floatImage", "block": 0, "inline": 1, "hint": null, "wrap": "square" }),
            ),
        );
        assert_eq!(
            again.model["blocks"][0]["content"][1]["placement"]["vertical"]["offset"],
            3000
        );
    }

    /// writes the editor's round-trip fixtures with text boxes, made by
    /// word and by libreoffice through the engine's bridge
    /// (app/ui/src/lib/writer/fixtures). run with LETTERS_FIXTURE_DIR set:
    /// `cargo test -p letters-shell-core write_text_box_fixtures -- --ignored`
    #[test]
    #[ignore = "writes the interface's fixtures into LETTERS_FIXTURE_DIR"]
    fn write_text_box_fixtures() {
        use letters_office::bridge;
        use letters_office::model::TextBox;
        let out = std::path::PathBuf::from(std::env::var("LETTERS_FIXTURE_DIR").unwrap());
        let mut document = Document::new();
        let mut boxed = TextBox::new(
            2880,
            2160,
            3600,
            1440,
            vec![
                Block::text("Inside the box"),
                Block::text("Its second paragraph"),
            ],
        );
        boxed.z_order = Some(1);
        document.blocks = vec![
            Block::text("Before the box."),
            Block::TextBox(boxed),
            Block::text("Hello world, split into two blocks."),
            Block::text("After the box."),
        ];
        apply(
            &mut document,
            &op(serde_json::json!({ "op": "splitIntoTextBoxes", "block": 2, "offset": 13 })),
        );
        let docx = letters_office::write_docx(&document).unwrap();
        let pretty = |value: &Value| serde_json::to_string_pretty(value).unwrap();
        // word opens our file and saves it as its own docx
        let (word_made, engine) = bridge::convert_word_format(&docx, false).unwrap();
        assert_eq!(engine, bridge::Engine::Word);
        let from_word = letters_office::open(&word_made).unwrap();
        assert_eq!(
            from_word
                .blocks
                .iter()
                .filter(|block| matches!(block, Block::TextBox(_)))
                .count(),
            3
        );
        std::fs::write(
            out.join("word-made-text-boxes.json"),
            pretty(&ui_model(&from_word)),
        )
        .unwrap();
        // libreoffice turns our docx into its own odt
        let odt = bridge::libreoffice_convert(&docx, "docx", "odt", "odt").unwrap();
        let from_libreoffice = letters_office::open(&odt).unwrap();
        std::fs::write(
            out.join("libreoffice-made-text-boxes.json"),
            pretty(&ui_model(&from_libreoffice)),
        )
        .unwrap();
    }

    #[test]
    fn the_compatibility_answer_has_the_matrix_and_a_report() {
        let answer = compatibility(&document("x"));
        assert!(answer
            .features
            .iter()
            .any(|feature| feature.key == "textBoxes"));
        assert!(answer.report.issues.is_empty());
    }
}
