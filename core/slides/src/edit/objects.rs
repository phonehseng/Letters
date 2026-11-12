//! edits of single objects: add, delete, replace, move, resize, rotate,
//! fill and line.

use super::{free_object_id, object_index, slide_mut, snapshot, Applied, Operation};
use crate::error::SlidesError;
use crate::inherit::{effective_transform, Chain};
use crate::model::{Object, Presentation, Transform};

pub fn apply_object(
    presentation: &mut Presentation,
    operation: &Operation,
) -> Result<Applied, SlidesError> {
    let slide_id = match operation {
        Operation::AddObject { slide_id, .. }
        | Operation::DeleteObject { slide_id, .. }
        | Operation::ReplaceObject { slide_id, .. }
        | Operation::Move { slide_id, .. }
        | Operation::Resize { slide_id, .. }
        | Operation::Rotate { slide_id, .. }
        | Operation::SetFill { slide_id, .. }
        | Operation::SetLine { slide_id, .. } => slide_id.clone(),
        _ => return Err(SlidesError::InvalidEdit("not an object edit".into())),
    };
    let inverse = snapshot(presentation, &slide_id)?;
    let placed = placed_transforms(presentation, &slide_id)?;
    let slide = slide_mut(presentation, &slide_id)?;
    match operation {
        Operation::AddObject { object, .. } => {
            let mut object = (**object).clone();
            object.set_id(free_object_id(slide));
            slide.objects.push(object);
        }
        Operation::DeleteObject { object_id, .. } => {
            let index = object_index(slide, *object_id)?;
            slide.objects.remove(index);
        }
        Operation::ReplaceObject { object, .. } => {
            let index = object_index(slide, object.id())?;
            slide.objects[index] = (**object).clone();
        }
        Operation::Move {
            object_ids, dx, dy, ..
        } => {
            for id in object_ids {
                let transform = transform_of(slide, *id, &placed)?;
                transform.x += dx;
                transform.y += dy;
            }
        }
        Operation::Resize {
            object_id,
            x,
            y,
            width,
            height,
            ..
        } => {
            let transform = transform_of(slide, *object_id, &placed)?;
            (transform.x, transform.y) = (*x, *y);
            (transform.width, transform.height) = ((*width).max(0), (*height).max(0));
        }
        Operation::Rotate {
            object_id,
            rotation,
            ..
        } => {
            transform_of(slide, *object_id, &placed)?.rotation = rotation.rem_euclid(21_600_000);
        }
        Operation::SetFill {
            object_id, fill, ..
        } => set_fill(slide, *object_id, fill)?,
        Operation::SetLine {
            object_id, line, ..
        } => set_line(slide, *object_id, line)?,
        _ => {}
    }
    Ok(Applied::slide(&slide_id, inverse))
}

/// the effective transform of every top-level object (placeholders
/// without their own position take their layout's), worked out before
/// the slide is borrowed for changing.
pub(crate) fn placed_transforms(
    presentation: &mut Presentation,
    slide_id: &str,
) -> Result<Vec<(u32, Transform)>, SlidesError> {
    let slide = slide_mut(presentation, slide_id)?.clone();
    let chain = Chain::of_slide(presentation, &slide);
    Ok(slide
        .objects
        .iter()
        .filter_map(|object| effective_transform(&chain, object).map(|t| (object.id(), t)))
        .collect())
}

/// an object's own transform for changing; a placeholder that inherits
/// its position gets its effective one first, so moving it moves it from
/// where it is shown.
pub(crate) fn transform_of<'a>(
    slide: &'a mut crate::model::Slide,
    id: u32,
    placed: &[(u32, Transform)],
) -> Result<&'a mut Transform, SlidesError> {
    let index = object_index(slide, id)?;
    let effective = placed
        .iter()
        .find(|(object, _)| *object == id)
        .map(|(_, t)| t.clone());
    match &mut slide.objects[index] {
        Object::Group(group) => Ok(&mut group.transform),
        object => {
            let transform = object
                .transform_mut()
                .ok_or_else(|| SlidesError::InvalidEdit("this object cannot be moved".into()))?;
            if transform.is_none() {
                *transform = effective;
            }
            transform
                .as_mut()
                .ok_or_else(|| SlidesError::InvalidEdit("this object has no position".into()))
        }
    }
}

fn set_fill(
    slide: &mut crate::model::Slide,
    id: u32,
    fill: &Option<crate::model::Fill>,
) -> Result<(), SlidesError> {
    let index = object_index(slide, id)?;
    match &mut slide.objects[index] {
        Object::Shape(shape) => shape.fill = fill.clone(),
        Object::Group(group) => group.fill = fill.clone(),
        _ => {
            return Err(SlidesError::InvalidEdit(
                "only shapes and groups have a fill".into(),
            ))
        }
    }
    Ok(())
}

fn set_line(
    slide: &mut crate::model::Slide,
    id: u32,
    line: &Option<crate::model::Line>,
) -> Result<(), SlidesError> {
    let index = object_index(slide, id)?;
    match &mut slide.objects[index] {
        Object::Shape(shape) => shape.line = line.clone(),
        Object::Picture(picture) => picture.line = line.clone(),
        Object::Connector(connector) => connector.line = line.clone(),
        _ => {
            return Err(SlidesError::InvalidEdit(
                "this object has no outline".into(),
            ))
        }
    }
    Ok(())
}
