//! writing a colour exactly as the file set it, in another place.
//!
//! a colour is more than red, green and blue: it may be set in a spot
//! colour (Separation), an ICC-based space, CMYK or a pattern. new or
//! moved content reuses the very colour space and numbers of what it
//! copies, so a corporate blue stays the same ink and doesn't become an
//! RGB look-alike. a named colour space only exists in the resources of
//! the stream that named it, so when the colour is written in another
//! stream the space is added there too, under a name of its own.

use lopdf::Object;

use crate::draft::Draft;
use crate::interpret::{FillColor, Resources};
use crate::layout::PageModel;
use crate::pdf_values::format_number;
use crate::rewrite::{self, Plan};
use crate::writer;

/// spaces that need no resource: their names mean the same everywhere.
fn is_built_in(space: &str) -> bool {
    matches!(
        space,
        "DeviceRGB" | "DeviceGray" | "DeviceCMYK" | "Pattern" | "RGB" | "G" | "CMYK"
    )
}

fn resources_of(model: &PageModel, source: Option<usize>) -> Resources {
    source.map_or(Resources::Page, |source| {
        model.run.sources[source].resources.clone()
    })
}

fn numbers(components: &[f32]) -> String {
    components
        .iter()
        .map(|value| format_number(*value))
        .collect::<Vec<_>>()
        .join(" ")
}

/// the operators that set `color` as the fill (or stroke) colour in the
/// stream `target` (none for content added to the page itself).
pub(crate) fn color_operators(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    color: &FillColor,
    stroke: bool,
    target: Option<usize>,
) -> String {
    let (space_op, color_op) = if stroke { ("CS", "SCN") } else { ("cs", "scn") };
    let device = match (color.space.as_str(), color.components.len()) {
        ("DeviceGray", 1) => Some(if stroke { "G" } else { "g" }),
        ("DeviceRGB", 3) => Some(if stroke { "RG" } else { "rg" }),
        ("DeviceCMYK", 4) => Some(if stroke { "K" } else { "k" }),
        _ => None,
    };
    if color.space_operator.is_none() {
        if let Some(operator) = device {
            return format!("{} {operator}", numbers(&color.components));
        }
        // a colour never set in this stream: the default black
        return if stroke { "0 G".into() } else { "0 g".into() };
    }
    let same_place = color
        .source
        .is_some_and(|source| resources_of(model, Some(source)) == resources_of(model, target));
    let space_name = if is_built_in(&color.space) || same_place {
        color.space.as_bytes().to_vec()
    } else {
        match copied_space(draft, model, plan, color, target) {
            Some(name) => name,
            // the space can't be found: fall back to how it looks
            None => return rgb_operator(color, stroke),
        }
    };
    let mut operands = numbers(&color.components);
    if let Some(pattern) = &color.pattern {
        let name = if same_place {
            pattern.clone()
        } else {
            match copied_resource(
                draft,
                model,
                plan,
                color.source,
                target,
                b"Pattern",
                "Pattern",
                pattern,
            ) {
                Some(name) => name,
                None => return rgb_operator(color, stroke),
            }
        };
        if !operands.is_empty() {
            operands.push(' ');
        }
        operands.push_str(&writer::name(&name));
    }
    format!(
        "{} {space_op} {operands} {color_op}",
        writer::name(&space_name)
    )
}

fn rgb_operator(color: &FillColor, stroke: bool) -> String {
    let operator = color.color.fill_operator();
    if stroke {
        operator.replace(" rg", " RG")
    } else {
        operator
    }
}

/// the colour's named space added to the target's resources.
fn copied_space(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    color: &FillColor,
    target: Option<usize>,
) -> Option<Vec<u8>> {
    copied_resource(
        draft,
        model,
        plan,
        color.source,
        target,
        b"ColorSpace",
        "ColorSpace",
        color.space.as_bytes(),
    )
}

/// a resource of one stream given a name in another stream's resources.
#[allow(clippy::too_many_arguments)]
pub(crate) fn copied_resource(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    from: Option<usize>,
    target: Option<usize>,
    category: &[u8],
    category_name: &'static str,
    name: &[u8],
) -> Option<Vec<u8>> {
    let object = rewrite::resource_in(draft.doc, &model.run, from, category, name)?.clone();
    let id = match object {
        Object::Reference(id) => id,
        // written inline: give it an object of its own so both can use it
        inline => draft.add(inline),
    };
    Some(plan.resource_name(draft.doc, &model.run, target, category_name, id))
}
