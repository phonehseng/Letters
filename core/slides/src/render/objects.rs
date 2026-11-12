//! each kind of object drawn into the scene: shapes with their fill,
//! line and text, pictures with their crop, groups with their child
//! space, connectors, and graphic frames (tables, stand-in pictures).

use crate::color::{ColorContext, Rgba};
use crate::geometry::outline;
use crate::inherit::{ancestors, effective_transform, Chain};
use crate::model::{
    Background, Connector, Fill, GraphicFrame, GraphicKind, Group, Line, Object, Picture, Shape,
    Transform,
};

use super::paint::{fill_paint, reference_color, shape_fill_and_line, stroke_style, style_fill};
use super::scene::{multiply, rectangle, rotate, scale, translate, Item, Matrix, Paint};
use super::text::{draw_text, TextTarget};
use super::{points, RenderContext};

/// the matrix taking a shape's own points (0,0 to width,height) to its
/// parent's: translate, rotate about the centre, flip.
pub fn object_matrix(transform: &Transform) -> Matrix {
    let (width, height) = (points(transform.width), points(transform.height));
    let centre = translate(
        points(transform.x) + width / 2.0,
        points(transform.y) + height / 2.0,
    );
    let turn = rotate(f64::from(transform.rotation) / 60_000.0);
    let flip = scale(
        if transform.flip_h { -1.0 } else { 1.0 },
        if transform.flip_v { -1.0 } else { 1.0 },
    );
    let back = translate(-width / 2.0, -height / 2.0);
    multiply(&centre, &multiply(&turn, &multiply(&flip, &back)))
}

/// text turns with its shape but is never mirrored: a vertical flip
/// turns it upside down instead (that is how powerpoint shows it).
fn text_matrix(transform: &Transform) -> Matrix {
    let mut upright = transform.clone();
    upright.flip_h = false;
    upright.flip_v = false;
    if transform.flip_v {
        upright.rotation += 180 * 60_000;
    }
    object_matrix(&upright)
}

pub fn draw_object(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    object: &Object,
    parent: &Matrix,
    items: &mut Vec<Item>,
) {
    match object {
        Object::Shape(shape) if !shape.hidden => {
            draw_shape(context, chain, object, shape, parent, items)
        }
        Object::Picture(picture) if !picture.hidden => {
            draw_picture(context, chain, object, picture, parent, items)
        }
        Object::Group(group) if !group.hidden => draw_group(context, chain, group, parent, items),
        Object::Connector(connector) if !connector.hidden => {
            draw_connector(context, chain, connector, parent, items)
        }
        Object::Frame(frame) if !frame.hidden => {
            draw_frame(context, chain, object, frame, parent, items)
        }
        Object::Preserved { .. } => {
            context.note("an object kept from the file (ink or a newer kind) is not drawn".into())
        }
        _ => {}
    }
}

/// a shape's fill and line, from itself, its placeholders or its style.
fn inherited_paint<'a>(
    chain: &Chain<'a>,
    object: &'a Object,
    shape: &'a Shape,
) -> (Option<Fill>, Option<Line>) {
    let mut fill = shape.fill.clone();
    let mut line = shape.line.clone();
    for parent in ancestors(chain, object) {
        if let Object::Shape(parent) = parent {
            fill = fill.or_else(|| parent.fill.clone());
            line = line.or_else(|| parent.line.clone());
        }
    }
    (fill, line)
}

fn draw_shape(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    object: &Object,
    shape: &Shape,
    parent: &Matrix,
    items: &mut Vec<Item>,
) {
    let Some(transform) = effective_transform(chain, object) else {
        return;
    };
    let matrix = multiply(parent, &object_matrix(&transform));
    let (width, height) = (points(transform.width), points(transform.height));
    let shape_outline = outline(shape.geometry.as_ref(), width, height);
    if !shape_outline.exact {
        let name = shape
            .geometry
            .as_ref()
            .and_then(|g| g.preset_name())
            .unwrap_or("");
        context.note(format!("the outline \"{name}\" is drawn as a rectangle"));
    }
    let (fill, line) = inherited_paint(chain, object, shape);
    paint_outline(
        context,
        chain,
        &shape_outline.paths,
        fill.as_ref(),
        line.as_ref(),
        shape.style.as_ref(),
        matrix,
        items,
    );
    let Some(body) = &shape.text else {
        return;
    };
    let colors = context.colors();
    let color = shape
        .style
        .as_ref()
        .and_then(|s| reference_color(&s.font, &colors));
    let target = TextTarget {
        object,
        body,
        rect: shape_outline.text_rect,
        matrix: multiply(parent, &text_matrix(&transform)),
        color,
    };
    draw_text(context, chain, &target, items);
}

/// fills and strokes an outline with resolved paints.
#[allow(clippy::too_many_arguments)]
fn paint_outline(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    paths: &[crate::geometry::OutlinePath],
    fill: Option<&Fill>,
    line: Option<&Line>,
    style: Option<&crate::model::ShapeStyle>,
    matrix: Matrix,
    items: &mut Vec<Item>,
) {
    let (fill, line) = shape_fill_and_line(fill, line, style, chain.theme);
    let bounds = path_bounds(paths);
    if let Some((fill, reference)) = fill {
        let colors = with_placeholder(context, reference.as_ref());
        if let Some(paint) = fill_paint(&fill, &colors, bounds.0, bounds.1) {
            let filled: Vec<_> = paths.iter().filter(|p| p.fill).cloned().collect();
            items.push(Item::Fill {
                matrix,
                paths: filled,
                paint,
            });
        }
    }
    if let Some((line, reference)) = line {
        let colors = with_placeholder(context, reference.as_ref());
        if let Some(style) = stroke_style(&line, &colors) {
            let stroked: Vec<_> = paths.iter().filter(|p| p.stroke).cloned().collect();
            items.push(Item::Stroke {
                matrix,
                paths: stroked,
                style,
            });
        }
    }
}

/// the colour context with `phClr` set from a style reference.
fn with_placeholder<'a>(
    context: &'a RenderContext<'_>,
    reference: Option<&crate::model::StyleRef>,
) -> ColorContext<'a> {
    let mut colors = context.colors();
    colors.placeholder = reference.and_then(|r| reference_color(r, &colors));
    colors
}

/// width and height covered by paths, for spreading gradients.
fn path_bounds(paths: &[crate::geometry::OutlinePath]) -> (f64, f64) {
    let mut right: f64 = 0.0;
    let mut bottom: f64 = 0.0;
    for segment in paths.iter().flat_map(|p| &p.segments) {
        if let crate::geometry::Segment::Move(x, y) | crate::geometry::Segment::Line(x, y) = segment
        {
            right = right.max(*x);
            bottom = bottom.max(*y);
        }
    }
    (right, bottom)
}

fn draw_picture(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    object: &Object,
    picture: &Picture,
    parent: &Matrix,
    items: &mut Vec<Item>,
) {
    let Some(transform) = effective_transform(chain, object) else {
        return;
    };
    let matrix = multiply(parent, &object_matrix(&transform));
    let (width, height) = (points(transform.width), points(transform.height));
    let shape_outline = outline(picture.geometry.as_ref(), width, height);
    if let Some(media_id) = &picture.media_id {
        items.push(picture_item(
            media_id,
            picture.crop,
            width,
            height,
            matrix,
            &shape_outline.paths,
        ));
    }
    if let Some(line) = &picture.line {
        if let Some(style) = stroke_style(line, &context.colors()) {
            items.push(Item::Stroke {
                matrix,
                paths: shape_outline.paths,
                style,
            });
        }
    }
}

/// a picture spread so that its uncropped part fills the box: a crop of
/// a fraction on one side pushes that much of the picture outside.
fn picture_item(
    media_id: &str,
    crop: Option<[i32; 4]>,
    width: f64,
    height: f64,
    matrix: Matrix,
    clip: &[crate::geometry::OutlinePath],
) -> Item {
    let [left, top, right, bottom] = crop.unwrap_or([0; 4]).map(|v| f64::from(v) / 100_000.0);
    let visible_width = (1.0 - left - right).max(1e-6);
    let visible_height = (1.0 - top - bottom).max(1e-6);
    let full_width = width / visible_width;
    let full_height = height / visible_height;
    let placed = [
        full_width,
        0.0,
        0.0,
        full_height,
        -left * full_width,
        -top * full_height,
    ];
    Item::Image {
        matrix: multiply(&matrix, &placed),
        media_id: media_id.to_string(),
        clip: Some((matrix, clip.to_vec())),
        alpha: 1.0,
    }
}

fn draw_group(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    group: &Group,
    parent: &Matrix,
    items: &mut Vec<Item>,
) {
    let transform = &group.transform;
    let box_matrix = multiply(parent, &object_matrix(transform));
    // children are placed in the group's own space (chOff, chExt), which
    // is stretched over the group's box
    let child_width = transform.child_width.unwrap_or(transform.width).max(1);
    let child_height = transform.child_height.unwrap_or(transform.height).max(1);
    let stretch = scale(
        transform.width as f64 / child_width as f64,
        transform.height as f64 / child_height as f64,
    );
    let shift = translate(
        -points(transform.child_x.unwrap_or(0)),
        -points(transform.child_y.unwrap_or(0)),
    );
    let child_space = multiply(&box_matrix, &multiply(&stretch, &shift));
    for child in &group.objects {
        draw_object(context, chain, child, &child_space, items);
    }
}

fn draw_connector(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    connector: &Connector,
    parent: &Matrix,
    items: &mut Vec<Item>,
) {
    let Some(transform) = &connector.transform else {
        return;
    };
    let matrix = multiply(parent, &object_matrix(transform));
    let shape_outline = outline(
        connector.geometry.as_ref(),
        points(transform.width),
        points(transform.height),
    );
    paint_outline(
        context,
        chain,
        &shape_outline.paths,
        None,
        connector.line.as_ref(),
        connector.style.as_ref(),
        matrix,
        items,
    );
}

fn draw_frame(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    object: &Object,
    frame: &GraphicFrame,
    parent: &Matrix,
    items: &mut Vec<Item>,
) {
    let Some(transform) = effective_transform(chain, object) else {
        return;
    };
    let matrix = multiply(parent, &object_matrix(&transform));
    let (width, height) = (points(transform.width), points(transform.height));
    if let GraphicKind::Table(table) = &frame.kind {
        super::tables::draw_table(context, chain, object, table, matrix, items);
        return;
    }
    match &frame.fallback_media_id {
        Some(media_id) => items.push(picture_item(
            media_id,
            None,
            width,
            height,
            matrix,
            &rectangle(0.0, 0.0, width, height),
        )),
        None => context.note(format!(
            "\"{}\" (a chart, diagram or embedded object) is not drawn yet",
            frame.name
        )),
    }
}

/// a background's paint: its fill, or the theme background style it names.
pub fn background_paint(
    context: &RenderContext<'_>,
    chain: &Chain<'_>,
    background: &Background,
    width: f64,
    height: f64,
) -> Option<Paint> {
    let mut colors = context.colors();
    let fill = match (&background.fill, &background.style_ref) {
        (Some(fill), _) => Some(fill.clone()),
        (None, Some(reference)) => {
            colors.placeholder = reference_color(reference, &colors);
            style_fill(reference, chain.theme)
        }
        _ => None,
    };
    fill.and_then(|fill| fill_paint(&fill, &colors, width, height))
        .or(Some(Paint::Solid(Rgba::WHITE)))
}

pub fn background_picture(background: &Background) -> Option<String> {
    match &background.fill {
        Some(fill @ Fill::Picture { media_id, .. }) if !super::paint::is_duotone(fill) => {
            media_id.clone()
        }
        _ => None,
    }
}
