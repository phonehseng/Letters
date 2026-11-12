//! opendocument drawing objects (`draw:frame`, `draw:custom-shape`,
//! `draw:rect`, `draw:line`, `draw:connector`, `draw:g`...) read into the
//! model's objects. each object keeps its original xml for the writer.

use std::collections::HashMap;

use super::read_geometry::read_geometry;
use super::read_paint::{read_fill, read_line};
use super::read_tables::read_table_frame;
use super::read_text::{read_text_body, TextContext};
use super::styles::{get, Style, StyleSheet};
use super::units::{length, transform};
use crate::model::{
    Connector, Geometry, GraphicFrame, GraphicKind, Group, Object, Picture, Placeholder,
    PlaceholderKind, Shape, SourceFormat, Transform,
};
use crate::preserve::source_of;
use crate::xml::Element;

pub struct ShapeContext<'a> {
    pub styles: &'a StyleSheet,
    /// picture path in the package to media id
    pub media: &'a HashMap<String, String>,
    /// picture path to its natural size in emu, for crops
    pub sizes: &'a HashMap<String, (i64, i64)>,
    pub text: &'a str,
    pub next_id: std::cell::Cell<u32>,
}

impl ShapeContext<'_> {
    pub fn id(&self) -> u32 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    fn raw(&self, element: &Element) -> String {
        self.text[element.start..element.end].to_string()
    }

    /// the merged style of a shape: its presentation style (placeholders)
    /// or its graphic style.
    fn style_of(&self, element: &Element) -> Style {
        match element.attr("presentation:style-name") {
            Some(name) => self.styles.resolve("presentation", Some(name)),
            None => self
                .styles
                .resolve("graphic", element.attr("draw:style-name")),
        }
    }
}

const DRAWING_ELEMENTS: [&str; 7] = [
    "draw:frame",
    "draw:custom-shape",
    "draw:rect",
    "draw:ellipse",
    "draw:line",
    "draw:connector",
    "draw:g",
];

/// the objects of a page, master page or group, back to front.
pub fn read_objects(parent: &Element, context: &ShapeContext<'_>) -> Vec<Object> {
    let mut objects = Vec::new();
    for child in parent.elements() {
        let object = match child.name.as_str() {
            name if DRAWING_ELEMENTS.contains(&name) => read_object(child, context),
            "presentation:notes"
            | "draw:page-thumbnail"
            | "office:forms"
            | "anim:par"
            | "anim:seq" => continue,
            name if name.starts_with("draw:") => Object::Preserved {
                xml: context.raw(child),
            },
            _ => continue,
        };
        objects.push(remember(object, &context.raw(child)));
    }
    objects
}

fn remember(mut object: Object, xml: &str) -> Object {
    if matches!(object, Object::Preserved { .. }) {
        return object;
    }
    object.set_source(None);
    let source = source_of(SourceFormat::Odp, xml, &object, Vec::new());
    object.set_source(Some(source));
    object
}

fn read_object(element: &Element, context: &ShapeContext<'_>) -> Object {
    match element.name.as_str() {
        "draw:frame" => read_frame(element, context),
        "draw:line" | "draw:connector" => Object::Connector(read_line_shape(element, context)),
        "draw:g" => Object::Group(read_group(element, context)),
        _ => Object::Shape(read_shape(element, context)),
    }
}

/// position, size and rotation. odf rotates counter-clockwise about the
/// shape's corner (`draw:transform`); the model turns clockwise about the
/// centre, so the corner is moved to where the centre lands.
pub fn read_transform(element: &Element) -> Transform {
    let get_length = |name: &str| element.attr(name).and_then(length).unwrap_or(0);
    let (width, height) = (get_length("svg:width"), get_length("svg:height"));
    let mut placed = Transform::new(get_length("svg:x"), get_length("svg:y"), width, height);
    if let Some(text) = element.attr("draw:transform") {
        let (radians, offset) = transform(text);
        let (x, y) = offset.unwrap_or((placed.x, placed.y));
        let clockwise = -radians;
        let (sin, cos) = clockwise.sin_cos();
        let (half_width, half_height) = (width as f64 / 2.0, height as f64 / 2.0);
        let centre_x = x as f64 + half_width * cos - half_height * sin;
        let centre_y = y as f64 + half_width * sin + half_height * cos;
        placed.x = (centre_x - half_width).round() as i64;
        placed.y = (centre_y - half_height).round() as i64;
        placed.rotation =
            (clockwise.to_degrees().rem_euclid(360.0) * 60_000.0).round() as i32 % 21_600_000;
    }
    placed
}

pub fn placeholder_of(element: &Element) -> Option<Placeholder> {
    let class = element.attr("presentation:class")?;
    let kind = match class {
        "title" => PlaceholderKind::Title,
        "subtitle" => PlaceholderKind::Subtitle,
        "outline" | "text" | "notes" => PlaceholderKind::Body,
        "graphic" => PlaceholderKind::Picture,
        "chart" => PlaceholderKind::Chart,
        "table" => PlaceholderKind::Table,
        "date-time" => PlaceholderKind::Date,
        "footer" => PlaceholderKind::Footer,
        "header" => PlaceholderKind::Header,
        "page-number" => PlaceholderKind::SlideNumber,
        "page" => PlaceholderKind::SlideImage,
        _ => PlaceholderKind::Object,
    };
    Some(Placeholder {
        kind,
        index: None,
        orient: None,
        size: None,
        custom_prompt: false,
    })
}

fn read_frame(element: &Element, context: &ShapeContext<'_>) -> Object {
    let style = context.style_of(element);
    if let Some(image) = element
        .child("draw:image")
        .filter(|_| element.child("draw:text-box").is_none())
    {
        return Object::Picture(read_picture(element, image, &style, context));
    }
    if let Some(table) = element.child("table:table") {
        return Object::Frame(read_table_frame(element, table, context));
    }
    if element.child("draw:object").is_some() || element.child("draw:object-ole").is_some() {
        return Object::Frame(read_embedded(element, context));
    }
    let text_box = element.child("draw:text-box");
    let text_context = TextContext {
        styles: context.styles,
        paragraph: style.paragraph.clone(),
        text: style.text.clone(),
    };
    Object::Shape(Shape {
        id: context.id(),
        name: element.attr("draw:name").unwrap_or("").to_string(),
        text_box: element.attr("presentation:class").is_none(),
        placeholder: placeholder_of(element),
        transform: Some(read_transform(element)),
        fill: read_fill(&style.graphic, context),
        line: read_line(&style.graphic),
        text: text_box.map(|text_box| read_text_body(text_box, &style.graphic, &text_context)),
        ..Default::default()
    })
}

fn read_shape(element: &Element, context: &ShapeContext<'_>) -> Shape {
    let style = context.style_of(element);
    let (geometry, flip_h, flip_v) = read_geometry(element);
    let mut transform = read_transform(element);
    transform.flip_h = flip_h;
    transform.flip_v = flip_v;
    let text_context = TextContext {
        styles: context.styles,
        paragraph: style.paragraph.clone(),
        text: style.text.clone(),
    };
    let has_text = element.elements().any(|c| c.name.starts_with("text:"));
    Shape {
        id: context.id(),
        name: element.attr("draw:name").unwrap_or("").to_string(),
        transform: Some(transform),
        geometry,
        fill: read_fill(&style.graphic, context),
        line: read_line(&style.graphic),
        text: has_text.then(|| read_text_body(element, &style.graphic, &text_context)),
        ..Default::default()
    }
}

fn read_line_shape(element: &Element, context: &ShapeContext<'_>) -> Connector {
    let style = context.style_of(element);
    let coordinate = |name: &str| element.attr(name).and_then(length).unwrap_or(0);
    let (x1, y1, x2, y2) = (
        coordinate("svg:x1"),
        coordinate("svg:y1"),
        coordinate("svg:x2"),
        coordinate("svg:y2"),
    );
    let mut transform = Transform::new(x1.min(x2), y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs());
    transform.flip_h = x2 < x1;
    transform.flip_v = y2 < y1;
    let preset = match element.attr("draw:type") {
        Some("curve") => "curvedConnector3",
        Some("standard") => "bentConnector3",
        _ => "straightConnector1",
    };
    Connector {
        id: context.id(),
        name: element.attr("draw:name").unwrap_or("").to_string(),
        transform: Some(transform),
        geometry: Some(Geometry::preset(preset)),
        line: read_line(&style.graphic),
        ..Default::default()
    }
}

fn read_group(element: &Element, context: &ShapeContext<'_>) -> Group {
    let id = context.id();
    let objects = read_objects(element, context);
    let bounds = objects
        .iter()
        .filter_map(|o| o.transform())
        .fold(None, |acc: Option<(i64, i64, i64, i64)>, t| {
            let (r, b) = (t.x + t.width, t.y + t.height);
            Some(match acc {
                None => (t.x, t.y, r, b),
                Some((x, y, right, bottom)) => {
                    (x.min(t.x), y.min(t.y), right.max(r), bottom.max(b))
                }
            })
        })
        .unwrap_or((0, 0, 0, 0));
    let (width, height) = (bounds.2 - bounds.0, bounds.3 - bounds.1);
    // children keep page coordinates: the group's own space is the page
    let transform = Transform {
        child_x: Some(bounds.0),
        child_y: Some(bounds.1),
        child_width: Some(width),
        child_height: Some(height),
        ..Transform::new(bounds.0, bounds.1, width, height)
    };
    Group {
        id,
        name: element.attr("draw:name").unwrap_or("").to_string(),
        transform,
        objects,
        ..Default::default()
    }
}

fn read_picture(
    frame: &Element,
    image: &Element,
    style: &Style,
    context: &ShapeContext<'_>,
) -> Picture {
    let href = image.attr("xlink:href").unwrap_or("");
    let media_id = context.media.get(href).cloned();
    Picture {
        id: context.id(),
        name: frame.attr("draw:name").unwrap_or("").to_string(),
        placeholder: placeholder_of(frame),
        transform: Some(read_transform(frame)),
        media_id,
        crop: get(&style.graphic, "fo:clip")
            .and_then(|clip| super::images::crop_fractions(clip, href, context)),
        line: read_line(&style.graphic),
        ..Default::default()
    }
}

fn read_embedded(frame: &Element, context: &ShapeContext<'_>) -> GraphicFrame {
    let object = frame
        .child("draw:object")
        .or_else(|| frame.child("draw:object-ole"));
    let href = object
        .and_then(|o| o.attr("xlink:href"))
        .unwrap_or("")
        .trim_start_matches("./")
        .to_string();
    let replacement = frame
        .child("draw:image")
        .and_then(|i| i.attr("xlink:href"))
        .and_then(|href| context.media.get(href).cloned());
    GraphicFrame {
        id: context.id(),
        name: frame.attr("draw:name").unwrap_or("").to_string(),
        description: None,
        hidden: false,
        placeholder: placeholder_of(frame),
        transform: Some(read_transform(frame)),
        kind: GraphicKind::Ole {
            prog_id: object
                .and_then(|o| o.attr("draw:class-id"))
                .map(str::to_string),
            part: Some(href),
        },
        fallback_media_id: replacement,
        source: None,
    }
}
