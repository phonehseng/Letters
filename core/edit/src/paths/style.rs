//! restyling paths (fill, outline, line width, dash, opacity) and
//! deleting them.

use super::*;

/// the operators for a new paint.
fn paint_operator(paint: NewPaint, stroke: bool) -> Option<String> {
    let numbers = |values: &[f32]| {
        values
            .iter()
            .map(|v| format_number(v.clamp(0.0, 1.0)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (values, fill_op, stroke_op) = match paint {
        NewPaint::None => return None,
        NewPaint::Rgb { color } => (
            vec![
                f32::from(color.r) / 255.0,
                f32::from(color.g) / 255.0,
                f32::from(color.b) / 255.0,
            ],
            "rg",
            "RG",
        ),
        NewPaint::Gray { value } => (vec![value], "g", "G"),
        NewPaint::Cmyk { c, m, y, k } => (vec![c, m, y, k], "k", "K"),
    };
    Some(format!(
        "{} {}",
        numbers(&values),
        if stroke { stroke_op } else { fill_op }
    ))
}

/// the painting operator for a fill and stroke choice, keeping whether
/// the original closed the path and used the even-odd rule.
fn painting_operator(fill: bool, stroke: bool, even_odd: bool, closes: bool) -> &'static str {
    match (fill, stroke, even_odd, closes) {
        (true, true, false, false) => "B",
        (true, true, true, false) => "B*",
        (true, true, false, true) => "b",
        (true, true, true, true) => "b*",
        (true, false, false, _) => "f",
        (true, false, true, _) => "f*",
        (false, true, _, false) => "S",
        (false, true, _, true) => "s",
        (false, false, _, _) => "n",
    }
}

fn check_style(style: &PathStyle) -> Result<(), EditError> {
    let bad = |value: f32| !(value.is_finite() && value >= 0.0);
    if style.stroke_width.is_some_and(bad)
        || style
            .dash
            .as_ref()
            .is_some_and(|dash| dash.iter().any(|v| bad(*v)))
    {
        return Err(EditError::InvalidValue(
            "Line width and dashes must be zero or more points.".into(),
        ));
    }
    if style
        .opacity
        .is_some_and(|value| !(0.0..=1.0).contains(&value))
    {
        return Err(EditError::InvalidValue(
            "Opacity must be between 0 and 1.".into(),
        ));
    }
    Ok(())
}

/// changes the fill, outline, line width, dash or opacity of paths.
pub fn set_path_style(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    style: &PathStyle,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    check_ids(ids)?;
    check_style(style)?;
    run_edit(doc, page, consent, |model, draft, plan| {
        let mut outcome = Outcome::default();
        for path in find_all(model, ids)? {
            restyle(draft, model, plan, path, style)?;
            outcome.landings.push(Landing::Path(model.path_rect(path)));
        }
        Ok(outcome)
    })
}

fn restyle(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    path: usize,
    style: &PathStyle,
) -> Result<(), EditError> {
    let record = &model.run.paths[path];
    if record.paint.shading {
        return Err(EditError::InvalidValue(
            "This is a colour blend; its colours canâ€™t be changed here.".into(),
        ));
    }
    let scale = model.visible_line_width(path) / record.line.width.max(1e-6);
    let to_user = |visible: f32| visible / scale.max(1e-6);
    let mut before = vec!["q".to_owned()];
    before.extend(style.fill.and_then(|paint| paint_operator(paint, false)));
    before.extend(style.stroke.and_then(|paint| paint_operator(paint, true)));
    if let Some(width) = style.stroke_width {
        before.push(format!("{} w", format_number(to_user(width))));
    }
    if let Some(dash) = &style.dash {
        let lengths: Vec<String> = dash.iter().map(|v| format_number(to_user(*v))).collect();
        before.push(format!("[{}] 0 d", lengths.join(" ")));
    }
    if let Some(opacity) = style.opacity {
        let mut state = Dictionary::new();
        state.set("Type", name("ExtGState"));
        state.set("ca", real(opacity));
        state.set("CA", real(opacity));
        let id = draft.add(Object::Dictionary(state));
        let resource =
            plan.resource_name(draft.doc, &model.run, Some(record.source), "ExtGState", id);
        before.push(format!("{} gs", writer::name(&resource)));
    }
    let fill = style
        .fill
        .map_or(record.paint.fill, |paint| paint != NewPaint::None);
    let stroke = style
        .stroke
        .map_or(record.paint.stroke, |paint| paint != NewPaint::None);
    if (fill, stroke) != (record.paint.fill, record.paint.stroke) {
        let original = &model.run.sources[record.source].operations[record.paint_op].operator;
        let closes = matches!(original.as_str(), "s" | "b" | "b*");
        let operator = painting_operator(fill, stroke, record.paint.even_odd, closes);
        plan.op(record.source, record.paint_op).replace = Some(operator.as_bytes().to_vec());
    }
    wrap(
        plan,
        record.source,
        record.first_op,
        record.paint_op,
        &before.join("\n"),
        "Q",
    );
    Ok(())
}

/// removes paths from the page.
pub fn delete_paths(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    check_ids(ids)?;
    run_edit(doc, page, consent, |model, _, plan| {
        for path in find_all(model, ids)? {
            let record = &model.run.paths[path];
            let first = if record.paint.shading && !shading_owns_clip(model, record) {
                record.paint_op
            } else {
                record.first_op
            };
            for op in first..=record.paint_op {
                plan.op(record.source, op).replace = Some(Vec::new());
            }
        }
        Ok(Outcome::default())
    })
}
