//! adding new text to a page as real page content (not an annotation).

use std::collections::BTreeSet;

use lopdf::{Document, Object, StringFormat};
use serde::{Deserialize, Serialize};

use crate::colors::Color;
use crate::draft::Draft;
use crate::error::EditError;
use crate::fonts::embed::{self, WriteFont};
use crate::fonts::standard::StandardFont;
use crate::fonts::truetype::FontProgram;
use crate::geometry::Point;
use crate::layout;
use crate::pdf_values::format_number;
use crate::rewrite::{self, Plan};
use crate::text_edit::TextEditResult;
use crate::writer;

/// the font new text is written in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NewTextFont {
    /// a standard font every reader has; nothing is embedded
    Standard { font: StandardFont },
    /// a TrueType font file (.ttf): only the glyphs the text uses are
    /// copied into the pdf
    Embedded { bytes: Vec<u8> },
}

/// what the app sends to add text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewText {
    /// the top left corner of the first line, in visible page points
    pub position: Point,
    /// new lines start new lines, 1.2 sizes apart
    pub text: String,
    pub font: NewTextFont,
    /// in points
    pub size: f32,
    pub color: Color,
}

/// the distance between baselines, as a multiple of the size.
const LINE_SPACING: f32 = 1.2;

/// adds text to the page, upright on screen whatever the page rotation.
pub fn add_text(
    doc: &mut Document,
    page: usize,
    new_text: &NewText,
) -> Result<TextEditResult, EditError> {
    if !(new_text.size.is_finite() && (0.5..=1000.0).contains(&new_text.size)) {
        return Err(EditError::InvalidValue(
            "Text size must be between 0.5 and 1000 points.".into(),
        ));
    }
    if !new_text.position.is_finite() {
        return Err(EditError::InvalidValue(
            "That position isn’t on the page. Try placing it again.".into(),
        ));
    }
    if new_text.text.trim().is_empty() {
        return Err(EditError::InvalidValue("Type some text to add.".into()));
    }
    let model = layout::build(doc, page)?;
    let changes;
    {
        let mut draft = Draft::new(doc);
        let mut plan = Plan::default();
        let font = font_for(&mut draft, new_text)?;
        let name = plan.resource_name(draft.doc, &model.run, None, "Font", font.object);
        plan.append(&text_content(&model.run.space, &font, &name, new_text));
        rewrite::apply(&mut draft, &model.run, plan)?;
        changes = draft.finish();
    }
    changes.commit(doc);
    Ok(TextEditResult {
        objects: layout::build(doc, page)?.objects(),
        notices: Vec::new(),
        overflow: None,
    })
}

fn font_for(draft: &mut Draft, new_text: &NewText) -> Result<WriteFont, EditError> {
    let characters: BTreeSet<char> = new_text.text.chars().filter(|c| *c != '\n').collect();
    match &new_text.font {
        NewTextFont::Standard { font } => {
            let missing = embed::not_in_standard_fonts(&new_text.text.replace('\n', ""));
            if !missing.is_empty() {
                return Err(EditError::UnsupportedCharacters(
                    missing.into_iter().collect(),
                ));
            }
            Ok(embed::standard_font(draft, *font))
        }
        NewTextFont::Embedded { bytes } => {
            let program = FontProgram::parse(bytes, 0).ok_or_else(|| {
                EditError::UnsupportedFont("It isn’t a font file Letters can read.".into())
            })?;
            let display = program
                .family_name()
                .unwrap_or_else(|| "the chosen font".into());
            embed::truetype_font(draft, &program, &characters, &display)
        }
    }
}

/// the drawing instructions for the new text. each line is placed with
/// a text matrix whose axes follow the screen, so it reads upright even
/// on a turned page.
fn text_content(
    space: &crate::page_space::PageSpace,
    font: &WriteFont,
    name: &[u8],
    new_text: &NewText,
) -> Vec<u8> {
    let right = space.user_vector(Point::new(1.0, 0.0));
    let up = space.user_vector(Point::new(0.0, -1.0));
    let mut out = vec![
        "q BT".to_owned(),
        format!("{} {} Tf", writer::name(name), format_number(new_text.size)),
        new_text.color.fill_operator(),
    ];
    let first_baseline = new_text.position.y + font.ascent / 1000.0 * new_text.size;
    for (number, line) in new_text.text.split('\n').enumerate() {
        let baseline = Point::new(
            new_text.position.x,
            first_baseline + number as f32 * LINE_SPACING * new_text.size,
        );
        let origin = space.user_point(baseline);
        let matrix = [right.x, right.y, up.x, up.y, origin.x, origin.y]
            .map(format_number)
            .join(" ");
        out.push(format!("{matrix} Tm"));
        let mut items = Vec::new();
        let mut codes = Vec::new();
        for character in line.chars() {
            match font.codes.get(&character) {
                Some(code) => codes.extend_from_slice(&code.code),
                // a space the font has no glyph for: a plain quarter-size move
                None => {
                    if !codes.is_empty() {
                        items.push(Object::String(
                            std::mem::take(&mut codes),
                            StringFormat::Hexadecimal,
                        ));
                    }
                    items.push(Object::Integer(-250));
                }
            }
        }
        if !codes.is_empty() {
            items.push(Object::String(codes, StringFormat::Hexadecimal));
        }
        out.push(
            String::from_utf8_lossy(&writer::operation("TJ", &[Object::Array(items)])).into_owned(),
        );
    }
    out.push("ET Q".to_owned());
    out.join("\n").into_bytes()
}
