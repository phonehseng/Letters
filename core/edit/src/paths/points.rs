//! editing the points of simple shapes: a rectangle's corners and a
//! line's ends, rewritten in the path's own coordinates.

use super::*;

/// moves the corners of a rectangle (top left, top right, bottom left,
/// bottom right on screen) or the two ends of a line. the line width
/// stays as it is, which is what moving a table rule needs.
pub fn set_path_points(
    doc: &mut Document,
    page: usize,
    id: &str,
    points: &[Point],
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    if points.iter().any(|point| !point.is_finite()) {
        return Err(EditError::InvalidValue(
            "Those points arenâ€™t on the page.".into(),
        ));
    }
    run_edit(doc, page, consent, |model, _, plan| {
        let path = model.find_path(id)?;
        reshape(model, plan, path, points)?;
        let mut outcome = Outcome::default();
        outcome.summary.paths_reshaped = 1;
        let mut rect = Rect::around(points.iter().copied()).expect("points were checked");
        if model.run.paths[path].paint.stroke {
            let half = model.visible_line_width(path) / 2.0;
            rect = Rect::new(
                rect.x - half,
                rect.y - half,
                rect.width + 2.0 * half,
                rect.height + 2.0 * half,
            );
        }
        outcome.landings.push(Landing::Path(rect));
        Ok(outcome)
    })
}

/// writes new construction operators for a simple shape.
pub(crate) fn reshape(
    model: &PageModel,
    plan: &mut Plan,
    path: usize,
    points: &[Point],
) -> Result<(), EditError> {
    let record = &model.run.paths[path];
    let (shape, _) = model.path_shape(path);
    let to_path = record.ctm.invert().unwrap_or(Matrix::IDENTITY);
    let local: Vec<Point> = points
        .iter()
        .map(|point| to_path.apply(model.run.space.user_point(*point)))
        .collect();
    let number = |value: f32| format_number(value);
    let construction = match (shape, local.as_slice()) {
        (PathShape::Line, [a, b]) => format!("{} {} m {} {} l", number(a.x), number(a.y), number(b.x), number(b.y)),
        (PathShape::Rectangle, [top_left, top_right, bottom_left, bottom_right]) => {
            rectangle_construction(*top_left, *top_right, *bottom_left, *bottom_right, uses_re(record))
        }
        (PathShape::Line, _) => return Err(EditError::InvalidValue("A line needs its two ends.".into())),
        (PathShape::Rectangle, _) => {
            return Err(EditError::InvalidValue("A rectangle needs its four corners: top left, top right, bottom left, bottom right.".into()))
        }
        _ => {
            return Err(EditError::InvalidValue(
                "Only the points of rectangles and lines can be edited. Move, resize or turn this drawing instead.".into(),
            ))
        }
    };
    plan.op(record.source, record.first_op).replace = Some(construction.into_bytes());
    for op in record.first_op + 1..record.paint_op {
        plan.op(record.source, op).replace = Some(Vec::new());
    }
    Ok(())
}

fn uses_re(record: &PathRecord) -> bool {
    matches!(record.segments.as_slice(), [Segment::Rect(..)])
}

/// a rectangle as "re" when its sides are upright in its own space,
/// otherwise as four lines.
fn rectangle_construction(
    top_left: Point,
    top_right: Point,
    bottom_left: Point,
    bottom_right: Point,
    prefer_re: bool,
) -> String {
    let number = |value: f32| format_number(value);
    let upright =
        (top_left.y - top_right.y).abs() < 1e-3 && (top_left.x - bottom_left.x).abs() < 1e-3;
    if upright && prefer_re {
        return format!(
            "{} {} {} {} re",
            number(bottom_left.x),
            number(bottom_left.y),
            number(bottom_right.x - bottom_left.x),
            number(top_left.y - bottom_left.y)
        );
    }
    format!(
        "{} {} m {} {} l {} {} l {} {} l h",
        number(bottom_left.x),
        number(bottom_left.y),
        number(bottom_right.x),
        number(bottom_right.y),
        number(top_right.x),
        number(top_right.y),
        number(top_left.x),
        number(top_left.y)
    )
}
