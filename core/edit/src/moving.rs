//! moving drawn objects by a transform of the page: text lines, images,
//! paths and annotations. reflow uses it to push content down, the path
//! tools to move and turn drawings, and the margin tools to fit content.
//!
//! a change is always given in page user space (pdf points, y up) and
//! applied to exactly one drawing:
//! - a text show operator is re-written with a text matrix that puts its
//!   glyphs where they should be, then the text state is set back the way
//!   the operator left it, so whatever follows in the stream is unmoved;
//! - an image or a path is wrapped in "q <change> cm ... Q", where the
//!   change is worked out in the drawing's own user space;
//! - an annotation's rectangle and points are moved in its dictionary.

use lopdf::{Dictionary, Object, ObjectId};

use crate::draft::Draft;
use crate::error::EditError;
use crate::geometry::{Matrix, Point, Rect};
use crate::interpret::{ShowOp, TextState};
use crate::layout::PageModel;
use crate::pdf_values::{format_number, number_array};
use crate::rewrite::{self, Plan};
use crate::writer;

/// a matrix written as the six numbers of an operator.
pub(crate) fn matrix_numbers(matrix: &Matrix) -> String {
    matrix
        .to_array()
        .iter()
        .map(|value| format_number(*value))
        .collect::<Vec<_>>()
        .join(" ")
}

/// the change in a drawing's own user space that moves it by `change`
/// in page space: new placement = ctm x change = local x ctm.
pub(crate) fn local_change(ctm: &Matrix, change: &Matrix) -> Matrix {
    let inverse = ctm.invert().unwrap_or(Matrix::IDENTITY);
    ctm.then(change).then(&inverse)
}

/// adds text before the first operator and after the last of a range, so
/// it wraps them. later wrappings go inside earlier ones.
pub(crate) fn wrap(
    plan: &mut Plan,
    source: usize,
    first: usize,
    last: usize,
    before: &str,
    after: &str,
) {
    let opening = plan.op(source, first);
    if !opening.before.is_empty() {
        opening.before.push(b'\n');
    }
    opening.before.extend_from_slice(before.as_bytes());
    let closing = plan.op(source, last);
    let mut ending = after.as_bytes().to_vec();
    if !closing.after.is_empty() {
        ending.push(b'\n');
        ending.extend_from_slice(&closing.after);
    }
    closing.after = ending;
}

/// how far a TJ number moves the pen in this text state.
fn adjustment(state: &TextState, distance: f32) -> Option<f32> {
    let scale = state.size * state.h_scale;
    (scale.abs() > 1e-9).then(|| -distance * 1000.0 / scale)
}

/// moves one text show operator's glyphs. returns false when the
/// operator was already changed by this edit (its glyphs are re-set).
pub(crate) fn move_show(
    model: &PageModel,
    plan: &mut Plan,
    show_index: usize,
    change: &Matrix,
) -> bool {
    let show = &model.run.shows[show_index];
    if plan.has_replacement(show.source, show.op) {
        return false;
    }
    let operation = &model.run.sources[show.source].operations[show.op];
    let bytes = moved_show_bytes(show, &operation.operator, &operation.operands, change);
    plan.op(show.source, show.op).replace = Some(bytes);
    let reach = show_reach(model, show_index, change);
    plan.reach(show.source, &reach);
    true
}

/// the bytes that draw a show operator's glyphs moved by `change`.
fn moved_show_bytes(
    show: &ShowOp,
    operator: &str,
    operands: &[Object],
    change: &Matrix,
) -> Vec<u8> {
    let state = &show.before;
    let inverse = show.ctm.invert().unwrap_or(Matrix::IDENTITY);
    let moved = state
        .text_matrix
        .then(&show.ctm)
        .then(change)
        .then(&inverse);
    let mut out: Vec<String> = Vec::new();
    if operator == "\"" {
        // " sets word and character spacing before showing
        if let [word, char, ..] = operands {
            out.push(
                String::from_utf8_lossy(&writer::operation("Tw", std::slice::from_ref(word)))
                    .into_owned(),
            );
            out.push(
                String::from_utf8_lossy(&writer::operation("Tc", std::slice::from_ref(char)))
                    .into_owned(),
            );
        }
    }
    out.push(format!("{} Tm", matrix_numbers(&moved)));
    let shown = match operator {
        "TJ" => writer::operation("TJ", operands),
        _ => writer::operation(
            "Tj",
            operands.last().map(std::slice::from_ref).unwrap_or(&[]),
        ),
    };
    out.push(String::from_utf8_lossy(&shown).into_owned());
    // set both text matrices back to what the operator left
    out.push(format!("{} Tm", matrix_numbers(&show.line_matrix_after)));
    let offset = show
        .text_matrix_after
        .then(&show.line_matrix_after.invert().unwrap_or(Matrix::IDENTITY));
    if offset.e.abs() > 1e-6 {
        if let Some(number) = adjustment(state, offset.e) {
            out.push(format!("[{}] TJ", format_number(number)));
        }
    }
    out.join("\n").into_bytes()
}

/// where a moved show operator's glyphs reach, for a form's box.
fn show_reach(model: &PageModel, show_index: usize, change: &Matrix) -> Vec<Point> {
    model
        .run
        .glyphs
        .iter()
        .filter(|glyph| glyph.show == show_index)
        .flat_map(|glyph| glyph.corners)
        .map(|corner| change.apply(corner))
        .collect()
}

/// moves one image drawing.
pub(crate) fn move_image(model: &PageModel, plan: &mut Plan, image: usize, change: &Matrix) {
    let record = &model.run.images[image];
    let local = local_change(&record.ctm, change);
    let reach = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
        .map(|(x, y)| change.apply(record.ctm.apply(Point::new(x, y))));
    plan.reach(record.source, &reach);
    wrap(
        plan,
        record.source,
        record.op,
        record.op,
        &format!("q {} cm", matrix_numbers(&local)),
        "Q",
    );
}

/// moves one path drawing (with the clip of a shading). a pattern fill
/// is moved with it by giving the drawing a copy of the pattern whose
/// matrix includes the change.
pub(crate) fn move_path(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    path: usize,
    change: &Matrix,
) -> Result<(), EditError> {
    let record = &model.run.paths[path];
    if record.paint.clip && !record.paint.shading {
        return Err(EditError::InvalidValue(
            "This drawing also clips what comes after it, so it can’t be moved on its own.".into(),
        ));
    }
    let local = local_change(&record.ctm, change);
    let mut before = format!("q {} cm", matrix_numbers(&local));
    if let Some(pattern) = moved_pattern(draft, model, plan, path, change)? {
        before.push('\n');
        before.push_str(&pattern);
    }
    let rect = model.path_rect(path);
    let reach: Vec<Point> = rect
        .corners()
        .iter()
        .map(|corner| change.apply(model.run.space.user_point(*corner)))
        .collect();
    plan.reach(record.source, &reach);
    wrap(
        plan,
        record.source,
        record.first_op,
        record.paint_op,
        &before,
        "Q",
    );
    Ok(())
}

/// the colour operators that fill with a moved copy of the path's
/// pattern, or none when the path isn't filled with a pattern.
fn moved_pattern(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    path: usize,
    change: &Matrix,
) -> Result<Option<String>, EditError> {
    let record = &model.run.paths[path];
    let Some(name) = record
        .fill
        .pattern
        .as_ref()
        .filter(|_| record.paint.fill || record.paint.shading)
    else {
        return Ok(None);
    };
    let Some(object) =
        rewrite::resource_in(draft.doc, &model.run, Some(record.source), b"Pattern", name)
    else {
        return Ok(None);
    };
    let Ok(pattern_id) = object.as_reference() else {
        return Ok(None);
    };
    let Some(original) = draft.object(pattern_id).cloned() else {
        return Ok(None);
    };
    let base = model.run.sources[record.source].base_ctm;
    let base_inverse = base.invert().unwrap_or(Matrix::IDENTITY);
    let dict = match &original {
        Object::Stream(stream) => &stream.dict,
        Object::Dictionary(dict) => dict,
        _ => return Ok(None),
    };
    let matrix = dict
        .get(b"Matrix")
        .ok()
        .and_then(|value| crate::pdf_values::numbers(draft.doc, value))
        .and_then(|values| Matrix::from_slice(&values))
        .unwrap_or(Matrix::IDENTITY);
    // pattern space maps to the stream's default space; the change is in
    // page space, so it is carried through the stream's base matrix
    let moved = matrix.then(&base).then(change).then(&base_inverse);
    let copy = match original {
        Object::Stream(mut stream) => {
            stream.dict.set("Matrix", number_array(&moved.to_array()));
            Object::Stream(stream)
        }
        Object::Dictionary(mut dict) => {
            dict.set("Matrix", number_array(&moved.to_array()));
            Object::Dictionary(dict)
        }
        _ => return Ok(None),
    };
    let copy_id = draft.add(copy);
    let new_name = plan.resource_name(
        draft.doc,
        &model.run,
        Some(record.source),
        "Pattern",
        copy_id,
    );
    let space = record
        .fill
        .space_operator
        .as_ref()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_else(|| "/Pattern cs".into());
    let components: Vec<String> = record
        .fill
        .components
        .iter()
        .map(|value| format_number(*value))
        .collect();
    let mut operands = components.join(" ");
    if !operands.is_empty() {
        operands.push(' ');
    }
    Ok(Some(format!(
        "{space} {operands}{} scn",
        writer::name(&new_name)
    )))
}

/// an annotation of the page, and its box on screen.
#[derive(Clone, Debug)]
pub(crate) struct Annotation {
    /// the annotation object, or its place in the page's /Annots array
    /// when it is written inline
    pub object: Option<ObjectId>,
    pub index: usize,
    pub rect: Rect,
}

/// the page's annotations with their boxes on screen.
pub(crate) fn annotations(draft: &Draft, model: &PageModel) -> Vec<Annotation> {
    let Some(page) = draft.dictionary(model.run.page_id) else {
        return Vec::new();
    };
    let Some(Object::Array(list)) = page.get(b"Annots").ok().map(|annots| draft.resolve(annots))
    else {
        return Vec::new();
    };
    list.iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let object = entry.as_reference().ok();
            let dict = draft.resolve(entry).as_dict().ok()?;
            let numbers = crate::pdf_values::rect_from_object(draft.doc, dict.get(b"Rect").ok()?)?;
            let corners = [
                Point::new(numbers[0], numbers[1]),
                Point::new(numbers[2], numbers[3]),
            ];
            Some(Annotation {
                object,
                index,
                rect: model.run.space.visible_rect_of(&corners)?,
            })
        })
        .collect()
}

/// moves annotations by a page-space change: their rectangles and every
/// list of points they keep (highlight quads, ink, polygon vertices, line
/// ends). the appearance follows the rectangle by itself.
pub(crate) fn move_annotations(
    draft: &mut Draft,
    model: &PageModel,
    which: &[Annotation],
    change: &Matrix,
) -> Result<(), EditError> {
    if which.is_empty() {
        return Ok(());
    }
    let mut page = draft
        .dictionary(model.run.page_id)
        .cloned()
        .ok_or_else(|| EditError::Damaged("the page is missing".into()))?;
    let mut list = match page.get(b"Annots").ok().map(|annots| draft.resolve(annots)) {
        Some(Object::Array(list)) => list.clone(),
        _ => return Ok(()),
    };
    let mut inline_changed = false;
    for annotation in which {
        match annotation.object {
            Some(id) => {
                let Some(dict) = draft.dictionary(id).cloned() else {
                    continue;
                };
                draft.set(id, Object::Dictionary(moved_annotation(dict, change)));
            }
            None => {
                if let Some(Object::Dictionary(dict)) = list.get(annotation.index).cloned() {
                    list[annotation.index] = Object::Dictionary(moved_annotation(dict, change));
                    inline_changed = true;
                }
            }
        }
    }
    if inline_changed {
        match page.get(b"Annots") {
            Ok(Object::Reference(id)) => {
                let id = *id;
                draft.set(id, Object::Array(list));
            }
            _ => {
                page.set("Annots", Object::Array(list));
                draft.set(model.run.page_id, Object::Dictionary(page));
            }
        }
    }
    Ok(())
}

fn moved_annotation(mut dict: Dictionary, change: &Matrix) -> Dictionary {
    let move_pairs = |values: &[f32]| -> Vec<f32> {
        values
            .chunks(2)
            .flat_map(|pair| match pair {
                [x, y] => {
                    let moved = change.apply(Point::new(*x, *y));
                    vec![moved.x, moved.y]
                }
                other => other.to_vec(),
            })
            .collect()
    };
    let numbers = |object: &Object| -> Option<Vec<f32>> {
        object
            .as_array()
            .ok()?
            .iter()
            .map(|item| item.as_float().ok())
            .collect()
    };
    if let Some(rect) = dict
        .get(b"Rect")
        .ok()
        .and_then(numbers)
        .filter(|rect| rect.len() == 4)
    {
        let corners = [
            Point::new(rect[0], rect[1]),
            Point::new(rect[2], rect[1]),
            Point::new(rect[0], rect[3]),
            Point::new(rect[2], rect[3]),
        ]
        .map(|corner| change.apply(corner));
        let around = Rect::around(corners).expect("four corners");
        dict.set(
            "Rect",
            number_array(&[around.x, around.y, around.right(), around.bottom()]),
        );
    }
    for key in [b"QuadPoints".as_slice(), b"Vertices", b"L", b"CL"] {
        if let Some(values) = dict.get(key).ok().and_then(numbers) {
            dict.set(key.to_vec(), number_array(&move_pairs(&values)));
        }
    }
    if let Some(Object::Array(strokes)) = dict.get(b"InkList").ok().cloned() {
        let moved: Vec<Object> = strokes
            .iter()
            .map(|stroke| match numbers(stroke) {
                Some(values) => number_array(&move_pairs(&values)),
                None => stroke.clone(),
            })
            .collect();
        dict.set("InkList", Object::Array(moved));
    }
    dict
}

/// a page-space matrix for a change given on screen (visible points,
/// y down): to user space, through the change, and back.
pub(crate) fn page_change(model: &PageModel, visible: &Matrix) -> Matrix {
    let to_visible = model.run.space.to_visible_matrix();
    let to_user = model.run.space.to_user_matrix();
    to_visible.then(visible).then(&to_user)
}

/// a move on screen by dx, dy points as a page-space matrix.
pub(crate) fn screen_move(model: &PageModel, dx: f32, dy: f32) -> Matrix {
    page_change(model, &Matrix::translate(dx, dy))
}
