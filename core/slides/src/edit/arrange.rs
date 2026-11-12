//! arranging objects: stacking order, aligning, distributing, grouping
//! and ungrouping.

use serde::{Deserialize, Serialize};

use super::objects::{placed_transforms, transform_of};
use super::{free_object_id, object_index, slide_mut, snapshot, Applied, Operation};
use crate::error::SlidesError;
use crate::model::{Group, Object, Presentation, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZOrder {
    Front,
    Back,
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlignEdge {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    Horizontal,
    Vertical,
}

pub fn apply_arrange(
    presentation: &mut Presentation,
    operation: &Operation,
) -> Result<Applied, SlidesError> {
    let slide_id = match operation {
        Operation::Reorder { slide_id, .. }
        | Operation::Align { slide_id, .. }
        | Operation::Distribute { slide_id, .. }
        | Operation::Group { slide_id, .. }
        | Operation::Ungroup { slide_id, .. } => slide_id.clone(),
        _ => return Err(SlidesError::InvalidEdit("not an arrange edit".into())),
    };
    let inverse = snapshot(presentation, &slide_id)?;
    let placed = placed_transforms(presentation, &slide_id)?;
    let size = presentation.slide_size.clone();
    let slide = slide_mut(presentation, &slide_id)?;
    match operation {
        Operation::Reorder { object_id, to, .. } => {
            let index = object_index(slide, *object_id)?;
            reorder(&mut slide.objects, index, *to);
        }
        Operation::Align {
            object_ids,
            edge,
            to_slide,
            ..
        } => {
            let bounds = if *to_slide {
                (0, 0, size.width, size.height)
            } else {
                selection_bounds(&placed, object_ids)
            };
            for id in object_ids {
                align(transform_of(slide, *id, &placed)?, *edge, bounds);
            }
        }
        Operation::Distribute {
            object_ids, axis, ..
        } => distribute(slide, object_ids, *axis, &placed)?,
        Operation::Group { object_ids, .. } => group(slide, object_ids, &placed)?,
        Operation::Ungroup { group_id, .. } => ungroup(slide, *group_id)?,
        _ => {}
    }
    Ok(Applied::slide(&slide_id, inverse))
}

fn reorder(objects: &mut Vec<Object>, index: usize, to: ZOrder) {
    let object = objects.remove(index);
    let last = objects.len();
    let target = match to {
        ZOrder::Front => last,
        ZOrder::Back => 0,
        ZOrder::Forward => (index + 1).min(last),
        ZOrder::Backward => index.saturating_sub(1),
    };
    objects.insert(target, object);
}

/// left, top, right, bottom around the chosen objects.
fn selection_bounds(placed: &[(u32, Transform)], ids: &[u32]) -> (i64, i64, i64, i64) {
    let chosen = placed
        .iter()
        .filter(|(id, _)| ids.contains(id))
        .map(|(_, t)| t);
    chosen.fold(
        (i64::MAX, i64::MAX, i64::MIN, i64::MIN),
        |(l, t, r, b), x| {
            (
                l.min(x.x),
                t.min(x.y),
                r.max(x.x + x.width),
                b.max(x.y + x.height),
            )
        },
    )
}

fn align(
    transform: &mut Transform,
    edge: AlignEdge,
    (left, top, right, bottom): (i64, i64, i64, i64),
) {
    match edge {
        AlignEdge::Left => transform.x = left,
        AlignEdge::Center => transform.x = (left + right - transform.width) / 2,
        AlignEdge::Right => transform.x = right - transform.width,
        AlignEdge::Top => transform.y = top,
        AlignEdge::Middle => transform.y = (top + bottom - transform.height) / 2,
        AlignEdge::Bottom => transform.y = bottom - transform.height,
    }
}

/// equal gaps between the objects, the outermost two staying put.
fn distribute(
    slide: &mut crate::model::Slide,
    ids: &[u32],
    axis: Axis,
    placed: &[(u32, Transform)],
) -> Result<(), SlidesError> {
    let horizontal = axis == Axis::Horizontal;
    let start_of = |t: &Transform| if horizontal { t.x } else { t.y };
    let length_of = |t: &Transform| if horizontal { t.width } else { t.height };
    let mut chosen: Vec<(u32, Transform)> = placed
        .iter()
        .filter(|(id, _)| ids.contains(id))
        .cloned()
        .collect();
    if chosen.len() < 3 {
        return Ok(());
    }
    chosen.sort_by_key(|(_, t)| start_of(t));
    let first = &chosen[0].1;
    let last = &chosen[chosen.len() - 1].1;
    let span = start_of(last) + length_of(last) - start_of(first);
    let total: i64 = chosen.iter().map(|(_, t)| length_of(t)).sum();
    let gap = (span - total) / (chosen.len() as i64 - 1);
    let mut cursor = start_of(first);
    for (id, original) in &chosen {
        let transform = transform_of(slide, *id, placed)?;
        if horizontal {
            transform.x = cursor;
        } else {
            transform.y = cursor;
        }
        cursor += length_of(original) + gap;
    }
    Ok(())
}

/// the chosen objects become one group, in the place of the topmost of
/// them, with a child space equal to the slide's so no child moves.
fn group(
    slide: &mut crate::model::Slide,
    ids: &[u32],
    placed: &[(u32, Transform)],
) -> Result<(), SlidesError> {
    if ids.len() < 2 {
        return Err(SlidesError::InvalidEdit(
            "grouping needs at least two objects".into(),
        ));
    }
    for id in ids {
        // placeholders that inherit their position get it written down
        let _ = transform_of(slide, *id, placed);
    }
    let (left, top, right, bottom) = selection_bounds(placed, ids);
    let position = ids
        .iter()
        .filter_map(|id| object_index(slide, *id).ok())
        .max()
        .unwrap_or(0);
    let id = free_object_id(slide);
    let mut children = Vec::new();
    slide.objects.retain(|object| {
        let take = ids.contains(&object.id());
        if take {
            children.push(object.clone());
        }
        !take
    });
    let transform = Transform {
        child_x: Some(left),
        child_y: Some(top),
        child_width: Some(right - left),
        child_height: Some(bottom - top),
        ..Transform::new(left, top, right - left, bottom - top)
    };
    let insert_at = position
        .saturating_sub(children.len() - 1)
        .min(slide.objects.len());
    slide.objects.insert(
        insert_at,
        Object::Group(Group {
            id,
            name: format!("Group {id}"),
            transform,
            objects: children,
            ..Default::default()
        }),
    );
    Ok(())
}

/// a group's children back on the slide, where the group showed them.
fn ungroup(slide: &mut crate::model::Slide, group_id: u32) -> Result<(), SlidesError> {
    let index = object_index(slide, group_id)?;
    let Object::Group(group) = slide.objects.remove(index) else {
        return Err(SlidesError::InvalidEdit(
            "that object is not a group".into(),
        ));
    };
    let t = &group.transform;
    let child_width = t.child_width.unwrap_or(t.width).max(1) as f64;
    let child_height = t.child_height.unwrap_or(t.height).max(1) as f64;
    let (scale_x, scale_y) = (t.width as f64 / child_width, t.height as f64 / child_height);
    let mut children = group.objects;
    for child in &mut children {
        if let Some(Some(own)) = child.transform_mut() {
            own.x = t.x + ((own.x - t.child_x.unwrap_or(0)) as f64 * scale_x).round() as i64;
            own.y = t.y + ((own.y - t.child_y.unwrap_or(0)) as f64 * scale_y).round() as i64;
            own.width = (own.width as f64 * scale_x).round() as i64;
            own.height = (own.height as f64 * scale_y).round() as i64;
            own.rotation = (own.rotation + t.rotation).rem_euclid(21_600_000);
        }
    }
    for (offset, child) in children.into_iter().enumerate() {
        slide.objects.insert(index + offset, child);
    }
    Ok(())
}
