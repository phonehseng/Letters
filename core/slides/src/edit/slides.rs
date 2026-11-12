//! slide list edits: add, duplicate, delete and move slides, and apply a
//! layout to a slide.

use super::{slide_mut, snapshot, Applied, Operation};
use crate::blank::new_slide;
use crate::error::SlidesError;
use crate::model::{Object, Presentation};

pub fn apply_slide_list(
    presentation: &mut Presentation,
    operation: &Operation,
) -> Result<Applied, SlidesError> {
    let inverse = Operation::RestoreSlides {
        slides: presentation.slides.clone(),
        sections: presentation.sections.clone(),
    };
    let count = presentation.slides.len();
    let out_of_range =
        |index: usize| SlidesError::InvalidEdit(format!("there is no slide {}", index + 1));
    match operation {
        Operation::AddSlide { index, layout_id } => {
            if presentation.layout(layout_id).is_none() {
                return Err(SlidesError::InvalidEdit(format!(
                    "there is no layout \"{layout_id}\""
                )));
            }
            let slide = new_slide(presentation, &fresh_slide_id(presentation), layout_id);
            presentation.slides.insert((*index).min(count), slide);
        }
        Operation::DuplicateSlide { index } => {
            let mut copy = presentation
                .slides
                .get(*index)
                .ok_or_else(|| out_of_range(*index))?
                .clone();
            copy.id = fresh_slide_id(presentation);
            // a new pptx slide id; the copy keeps its source so its
            // objects keep their xml and their relationships
            copy.number_id = None;
            copy.comments.clear();
            add_to_section_of(presentation, *index, &copy.id);
            presentation.slides.insert(index + 1, copy);
        }
        Operation::DeleteSlide { index } => {
            if *index >= count {
                return Err(out_of_range(*index));
            }
            let removed = presentation.slides.remove(*index);
            for section in &mut presentation.sections {
                section.slide_ids.retain(|id| *id != removed.id);
            }
        }
        Operation::MoveSlide { from, to } => {
            if *from >= count || *to >= count {
                return Err(out_of_range((*from).max(*to)));
            }
            let slide = presentation.slides.remove(*from);
            presentation.slides.insert(*to, slide);
            reorder_sections(presentation);
        }
        _ => return Err(SlidesError::InvalidEdit("not a slide list edit".into())),
    }
    Ok(Applied {
        changed_slides: Vec::new(),
        all_slides: true,
        inverse,
    })
}

/// an id no slide has ("slide-new-3").
fn fresh_slide_id(presentation: &Presentation) -> String {
    (1..)
        .map(|n| format!("slide-new-{n}"))
        .find(|id| presentation.slide_index(id).is_none())
        .unwrap_or_default()
}

fn add_to_section_of(presentation: &mut Presentation, index: usize, new_id: &str) {
    let Some(original) = presentation.slides.get(index).map(|s| s.id.clone()) else {
        return;
    };
    for section in &mut presentation.sections {
        if let Some(position) = section.slide_ids.iter().position(|id| *id == original) {
            section.slide_ids.insert(position + 1, new_id.to_string());
        }
    }
}

/// sections keep slides in deck order after a move.
fn reorder_sections(presentation: &mut Presentation) {
    let order: Vec<String> = presentation.slides.iter().map(|s| s.id.clone()).collect();
    for section in &mut presentation.sections {
        section
            .slide_ids
            .sort_by_key(|id| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
    }
}

/// puts a slide on another layout. placeholders the new layout also has
/// (same kind) keep their text and take the new layout's position;
/// placeholders it lacks are kept as they are, so nothing typed is lost.
pub fn apply_layout(
    presentation: &mut Presentation,
    slide_id: &str,
    layout_id: &str,
) -> Result<Applied, SlidesError> {
    let layout = presentation
        .layout(layout_id)
        .cloned()
        .ok_or_else(|| SlidesError::InvalidEdit(format!("there is no layout \"{layout_id}\"")))?;
    let inverse = snapshot(presentation, slide_id)?;
    let slide = slide_mut(presentation, slide_id)?;
    slide.layout_id = Some(layout_id.to_string());
    for object in &mut slide.objects {
        let Some(kind) = object.placeholder().map(|p| p.kind) else {
            continue;
        };
        let matches = layout
            .objects
            .iter()
            .any(|o| o.placeholder().is_some_and(|p| p.kind == kind));
        if matches {
            if let Some(transform) = object.transform_mut() {
                *transform = None;
            }
        }
    }
    let present: Vec<_> = slide
        .objects
        .iter()
        .filter_map(|o| o.placeholder().map(|p| p.kind))
        .collect();
    let first_id = super::free_object_id(slide);
    let missing = layout
        .objects
        .iter()
        .filter(|o| o.placeholder().is_some_and(|p| !present.contains(&p.kind)));
    for (offset, object) in missing.enumerate() {
        let mut copy: Object = object.clone();
        copy.set_source(None);
        copy.set_id(first_id + offset as u32);
        if let Some(transform) = copy.transform_mut() {
            *transform = None;
        }
        slide.objects.push(copy);
    }
    Ok(Applied::slide(slide_id, inverse))
}
