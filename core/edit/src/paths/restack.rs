//! drawing paths again on their own: to the front, to the back, or as a
//! moved copy, with the full graphics state they were drawn with.

use super::*;

/// draws paths above everything else on the page.
pub fn bring_paths_to_front(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    restack(doc, page, ids, consent, Restack::Front)
}

/// draws paths below everything else on the page.
pub fn send_paths_to_back(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    restack(doc, page, ids, consent, Restack::Back)
}

/// draws a copy of paths, moved by `dx`, `dy` points on screen, on top
/// of the page. the originals stay.
pub fn duplicate_paths(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    dx: f32,
    dy: f32,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    if !(dx.is_finite() && dy.is_finite()) {
        return Err(EditError::InvalidValue(
            "That move isnâ€™t a number of points.".into(),
        ));
    }
    restack(doc, page, ids, consent, Restack::Copy { dx, dy })
}

#[derive(Clone, Copy)]
enum Restack {
    Front,
    Back,
    Copy { dx: f32, dy: f32 },
}

/// writes paths again on their own at the start or end of the page, with
/// the full state they were drawn with. a clip that cut them where they
/// were is not carried along.
fn restack(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    consent: &EditConsent,
    how: Restack,
) -> Result<ContentEditResult, EditError> {
    check_ids(ids)?;
    run_edit(doc, page, consent, |model, draft, plan| {
        let paths = find_all(model, ids)?;
        let change = match how {
            Restack::Copy { dx, dy } => screen_move(model, dx, dy),
            _ => Matrix::IDENTITY,
        };
        let mut drawing = Vec::new();
        let mut outcome = Outcome::default();
        for path in &paths {
            drawing.push(standalone_path(draft, model, plan, *path, &change)?);
            outcome
                .landings
                .push(Landing::Path(landed_rect(model, *path, &change)));
            if !matches!(how, Restack::Copy { .. }) {
                let record = &model.run.paths[*path];
                for op in record.first_op..=record.paint_op {
                    plan.op(record.source, op).replace = Some(Vec::new());
                }
            }
        }
        let drawing = drawing.join("\n");
        match how {
            Restack::Back => plan.prepend(drawing.as_bytes()),
            _ => plan.append(drawing.as_bytes()),
        }
        Ok(outcome)
    })
}

/// a path's drawing written to stand on its own at page level, moved by
/// a page-space change.
fn standalone_path(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    path: usize,
    change: &Matrix,
) -> Result<String, EditError> {
    let record = &model.run.paths[path];
    if record.paint.clip && !record.paint.shading {
        return Err(EditError::InvalidValue(
            "This drawing also clips what comes after it, so it canâ€™t be moved on its own."
                .into(),
        ));
    }
    let mut out = vec![
        "q".to_owned(),
        format!("{} cm", matrix_numbers(&record.ctm.then(change))),
        color_ops::color_operators(draft, model, plan, &record.fill, false, None),
        color_ops::color_operators(draft, model, plan, &record.stroke, true, None),
        format!(
            "{} w {} J {} j {} M",
            format_number(record.line.width),
            record.line.cap,
            record.line.join,
            format_number(record.line.miter)
        ),
    ];
    if !record.line.dash.is_empty() {
        let lengths: Vec<String> = record.line.dash.iter().map(|v| format_number(*v)).collect();
        out.push(format!(
            "[{}] {} d",
            lengths.join(" "),
            format_number(record.line.dash_phase)
        ));
    }
    if record.fill_alpha < 1.0 || record.stroke_alpha < 1.0 {
        let mut state = Dictionary::new();
        state.set("Type", name("ExtGState"));
        state.set("ca", real(record.fill_alpha));
        state.set("CA", real(record.stroke_alpha));
        let id = draft.add(Object::Dictionary(state));
        let resource = plan.resource_name(draft.doc, &model.run, None, "ExtGState", id);
        out.push(format!("{} gs", writer::name(&resource)));
    }
    let source = &model.run.sources[record.source];
    for op in record.first_op..=record.paint_op {
        let operation = &source.operations[op];
        if operation.operator == "sh" {
            let Some(Object::Name(shading)) = operation.operands.first() else {
                continue;
            };
            let new_name = color_ops::copied_resource(
                draft,
                model,
                plan,
                Some(record.source),
                None,
                b"Shading",
                "Shading",
                shading.as_slice(),
            )
            .ok_or_else(|| EditError::Damaged("a colour blend is missing".into()))?;
            out.push(format!("{} sh", writer::name(&new_name)));
        } else {
            out.push(String::from_utf8_lossy(&source.content[operation.span.clone()]).into_owned());
        }
    }
    out.push("Q".to_owned());
    Ok(out.join("\n"))
}
