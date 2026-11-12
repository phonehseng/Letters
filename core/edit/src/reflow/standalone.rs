//! writing set lines as text of their own, outside the stream they were
//! set for: on the next page (carried lines) or anywhere on a page (new
//! text that matches a paragraph). the text keeps the paragraph's font
//! objects, size, spacing and colour space exactly.

use lopdf::Object;

use crate::draft::Draft;
use crate::geometry::Matrix;
use crate::interpret::FillColor;
use crate::layout::PageModel;
use crate::moving::matrix_numbers;
use crate::pdf_values::format_number;
use crate::rewrite::{self, Plan};
use crate::text_write::{self, FontRef, Placed};
use crate::writer;

use super::paragraph::Paragraph;

/// where set lines go.
pub(super) struct Placement<'a> {
    /// the page the paragraph is on
    pub model: &'a PageModel,
    /// the page the text is written on (the same one, or the next)
    pub target: &'a PageModel,
    pub paragraph: &'a Paragraph,
    /// baseline distance in the paragraph's text space
    pub line_step: f32,
    /// a page-space change from the paragraph's place to the new one
    pub change: Matrix,
}

/// the drawing instructions for the glyphs, to add at page level.
pub(super) fn standalone_text(
    draft: &mut Draft,
    plan: &mut Plan,
    placement: &Placement,
    glyphs: &[Placed],
) -> String {
    let show = &placement.model.run.shows[placement.paragraph.anchor_show];
    let state = &show.before;
    let mut out = vec![
        "q BT".to_owned(),
        format!(
            "{} Tc {} Tw {} Tz {} Ts {} Tr",
            format_number(state.char_spacing),
            format_number(state.word_spacing),
            format_number(state.h_scale * 100.0),
            format_number(state.rise),
            state.render_mode
        ),
        color_on_page(draft, placement, plan, &state.fill),
    ];
    let mut index = 0;
    let mut current_font: Option<(Vec<u8>, f32)> = None;
    while index < glyphs.len() {
        let first = &glyphs[index];
        let FontRef::Object(font) = first.font else {
            index += 1;
            continue;
        };
        let name = plan.resource_name(draft.doc, &placement.target.run, None, "Font", font);
        if current_font.as_ref() != Some(&(name.clone(), first.size)) {
            out.push(format!(
                "{} {} Tf",
                writer::name(&name),
                format_number(first.size)
            ));
            current_font = Some((name, first.size));
        }
        let line_offset = -(first.line as f32) * placement.line_step;
        let start = Matrix::translate(first.x, line_offset)
            .then(&placement.paragraph.anchor_matrix)
            .then(&show.ctm)
            .then(&placement.change);
        out.push(format!("{} Tm", matrix_numbers(&start)));
        let (shown, following) = text_write::show_group(state, glyphs, index);
        out.push(shown);
        index = following;
    }
    out.push("ET Q".to_owned());
    out.join("\n")
}

/// the fill colour as set where the paragraph is, written for page-level
/// content of the target page: a named colour space (a spot or ICC
/// colour) is added to the target page's resources.
fn color_on_page(
    draft: &mut Draft,
    placement: &Placement,
    plan: &mut Plan,
    fill: &FillColor,
) -> String {
    let numbers: Vec<String> = fill
        .components
        .iter()
        .map(|value| format_number(*value))
        .collect();
    let numbers = numbers.join(" ");
    let device = match fill.space.as_str() {
        "DeviceGray" => Some("g"),
        "DeviceRGB" => Some("rg"),
        "DeviceCMYK" => Some("k"),
        _ => None,
    };
    if let (None, Some(operator)) = (&fill.space_operator, device) {
        return format!("{numbers} {operator}");
    }
    let space = fill.space.as_bytes();
    let found = rewrite::resource_in(
        draft.doc,
        &placement.model.run,
        fill.source,
        b"ColorSpace",
        space,
    )
    .cloned();
    let target = &placement.target.run;
    let name = match found {
        Some(Object::Reference(id)) => {
            plan.resource_name(draft.doc, target, None, "ColorSpace", id)
        }
        Some(inline) => {
            let id = draft.add(inline);
            plan.resource_name(draft.doc, target, None, "ColorSpace", id)
        }
        // a built-in space such as /DeviceRGB named with cs
        None => space.to_vec(),
    };
    format!("{} cs {numbers} scn", writer::name(&name))
}
