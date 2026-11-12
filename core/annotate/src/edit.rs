//! changing and deleting annotations.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Deserializer, Serialize};

use crate::annots::{self, PageContext};
use crate::color::{clamp_opacity, Color};
use crate::common::{self, Border};
use crate::error::AnnotateError;
use crate::fonts::StandardFont;
use crate::geometry::{Point, Rect};
use crate::ids::new_unique_name;
use crate::image;
use crate::ink;
use crate::note::NoteIcon;
use crate::page_space::PageSpace;
use crate::pdf_values::{dict_rect, name, number_array, numbers, real, resolve};
use crate::read::{self, AnnotationInfo};
use crate::redraw;
use crate::shapes::{self, LineEnding};
use crate::text_box::{self, TextAlign, TextBorder};
use crate::text_layout;
use crate::text_string;

/// what to change about an annotation. every field is optional: only
/// the ones given are changed.
///
/// fields written as `Option<Option<..>>` can also be cleared: leave the
/// field out to keep it, send `null` to remove it (for example to take
/// away a fill), or send a value to set it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnnotationChanges {
    /// move (same size) or resize (new size) the annotation to this
    /// rectangle. everything inside, such as ink strokes, is moved and
    /// stretched with it
    pub rect: Option<Rect>,
    /// the main colour: mark, outline, note, or text box text colour
    pub color: Option<Color>,
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub fill: Option<Option<Color>>,
    pub opacity: Option<f32>,
    /// line, outline or ink width in points
    pub stroke_width: Option<f32>,
    /// dash and gap lengths; empty for solid
    pub dash: Option<Vec<f32>>,
    /// the comment
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub contents: Option<Option<String>>,
    pub author: Option<String>,
    /// a text box's text
    pub text: Option<String>,
    pub font: Option<StandardFont>,
    pub font_size: Option<f32>,
    pub align: Option<TextAlign>,
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub border: Option<Option<TextBorder>>,
    pub icon: Option<NoteIcon>,
    /// whether a note's pop-up is open
    pub open: Option<bool>,
    /// a line's two ends, or a polygon's corners, in visible points
    pub points: Option<Vec<Point>>,
    /// an ink drawing's strokes, in visible points
    pub strokes: Option<Vec<Vec<Point>>>,
    pub start_ending: Option<LineEnding>,
    pub end_ending: Option<LineEnding>,
}

/// tells "left out" (none) apart from "sent as null" (some none).
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl AnnotationChanges {
    /// true when only the comment or author changes, which doesn't
    /// change how the mark looks (except for a text box, whose comment
    /// is its text).
    fn only_comment(&self) -> bool {
        let comment_only = AnnotationChanges {
            contents: self.contents.clone(),
            author: self.author.clone(),
            ..AnnotationChanges::default()
        };
        *self == comment_only
    }
}

/// changes an annotation and redraws it. returns the annotation as it
/// is now; its id may change from "ref:" to a stable name, because an
/// annotation from another app is given one the first time it changes.
pub fn update(
    doc: &mut Document,
    page: usize,
    id: &str,
    changes: &AnnotationChanges,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    annots::make_entries_objects(doc, page.id)?;
    let object = annots::find(doc, page.id, id)?;
    let annotation = doc
        .get_dictionary(object)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;
    let subtype = annots::subtype(doc, annotation);
    if subtype == "Widget" {
        return Err(AnnotateError::FormField);
    }
    let locked = common::flags(doc, annotation) & common::LOCKED_FLAG != 0;
    if !redraw::can_draw(&subtype) || locked {
        return Err(AnnotateError::CannotChange);
    }
    validate(changes, &subtype)?;

    // if drawing fails part way (for example a text box from another app
    // holding characters the built-in fonts lack), the annotation and its
    // pop-up are put back exactly as they were
    let saved = snapshot(doc, object);
    match change(doc, &page, object, &subtype, changes) {
        Ok(()) => read::info(doc, &page, object),
        Err(error) => {
            for (id, dictionary) in saved {
                doc.objects.insert(id, Object::Dictionary(dictionary));
            }
            Err(error)
        }
    }
}

/// copies of an annotation and its pop-up, to undo a failed change.
fn snapshot(doc: &Document, object: ObjectId) -> Vec<(ObjectId, Dictionary)> {
    let mut saved = Vec::new();
    if let Ok(annotation) = doc.get_dictionary(object) {
        saved.push((object, annotation.clone()));
        if let Some(popup) = common::reference(annotation, b"Popup") {
            if let Ok(popup_dict) = doc.get_dictionary(popup) {
                saved.push((popup, popup_dict.clone()));
            }
        }
    }
    saved
}

/// moves, restyles and redraws an annotation that has been checked.
fn change(
    doc: &mut Document,
    page: &PageContext,
    object: ObjectId,
    subtype: &str,
    changes: &AnnotationChanges,
) -> Result<(), AnnotateError> {
    let annotation = doc
        .get_dictionary(object)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;
    let old_frame = dict_rect(doc, annotation, b"Rect").map(|rect| page.space.visible_rect(rect));

    // a pure move keeps the existing drawing: viewers place an
    // appearance on /Rect, so it follows the move exactly
    let style_changes = AnnotationChanges {
        rect: None,
        ..changes.clone()
    };
    let looks_different =
        !style_changes.only_comment() || (subtype == "FreeText" && changes.contents.is_some());
    let mut resized = false;
    if let (Some(new_rect), Some(old_frame)) = (changes.rect, old_frame) {
        resized = !same_size(new_rect, old_frame) && subtype != "Text";
        move_or_resize(doc, page, object, subtype, old_frame, new_rect)?;
    }
    // an annotation from another app without a drawing gets one, so it
    // looks the same in every viewer from now on
    let has_drawing = doc
        .get_dictionary(object)
        .map(|annotation| !crate::appearance::appearance_objects(annotation).is_empty())
        .unwrap_or(false);
    let needs_redraw = looks_different || resized || !has_drawing;
    apply_changes(doc, &page.space, object, subtype, changes)?;
    if needs_redraw {
        redraw_or_keep(doc, page, object, subtype)?;
    }
    annots::remove(doc, page.id, &HashSet::new())
}

fn same_size(a: Rect, b: Rect) -> bool {
    (a.width - b.width).abs() < 0.01 && (a.height - b.height).abs() < 0.01
}

/// redraws, except for a stamp from another app whose drawing Letters
/// can't recreate: its /Rect has already moved, and viewers stretch the
/// drawing to fit it.
fn redraw_or_keep(
    doc: &mut Document,
    page: &PageContext,
    object: ObjectId,
    subtype: &str,
) -> Result<(), AnnotateError> {
    if subtype == "Stamp" {
        let annotation = doc
            .get_dictionary(object)
            .map_err(|_| AnnotateError::UnknownAnnotation)?;
        if !image::is_picture_stamp(doc, annotation) {
            return Ok(());
        }
    }
    redraw::redraw(doc, page, object)
}

/// refuses values that would make an unreadable or broken mark.
fn validate(changes: &AnnotationChanges, subtype: &str) -> Result<(), AnnotateError> {
    if let Some(rect) = changes.rect {
        common::check_rect(rect)?;
    }
    if let Some(width) = changes.stroke_width {
        common::check_stroke_width(width)?;
    }
    if let Some(Some(border)) = changes.border {
        common::check_stroke_width(border.width)?;
    }
    if let Some(size) = changes.font_size {
        text_box::check_font_size(size)?;
    }
    if let Some(opacity) = changes.opacity {
        if !opacity.is_finite() {
            return Err(AnnotateError::InvalidValue(
                "Opacity must be a number from 0 to 1.".into(),
            ));
        }
    }
    if subtype == "FreeText" {
        let new_text = changes
            .text
            .as_ref()
            .or(changes.contents.as_ref().and_then(Option::as_ref));
        if let Some(text) = new_text {
            text_layout::check_drawable(text)?;
        }
    }
    if let Some(points) = &changes.points {
        let needed = match subtype {
            "Line" | "PolyLine" => 2,
            "Polygon" => 3,
            _ => 0,
        };
        let usable = points.iter().filter(|point| point.is_finite()).count();
        if usable < needed || usable != points.len() {
            return Err(AnnotateError::EmptyArea);
        }
    }
    if let Some(strokes) = &changes.strokes {
        if ink::usable_strokes(strokes).is_empty() {
            return Err(AnnotateError::EmptyArea);
        }
    }
    Ok(())
}

/// moves (or moves and stretches) everything an annotation is made of.
///
/// what counts is the drawn part, not the padding around it: ink is
/// stretched so its strokes fill the new rectangle less the same margin
/// as before, so the annotation's /Rect ends up where the ui asked.
fn move_or_resize(
    doc: &mut Document,
    page: &PageContext,
    object: ObjectId,
    subtype: &str,
    old_frame: Rect,
    new_rect: Rect,
) -> Result<(), AnnotateError> {
    let space = page.space;
    // a note's icon keeps its size; only its position changes
    let map: Box<dyn Fn(Point) -> Point> = if subtype == "Text" || same_size(old_frame, new_rect) {
        let (dx, dy) = (new_rect.x - old_frame.x, new_rect.y - old_frame.y);
        Box::new(move |point: Point| Point::new(point.x + dx, point.y + dy))
    } else {
        let drawn = drawn_bounds(doc, object, subtype, &space).unwrap_or(old_frame);
        let target = Rect::new(
            new_rect.x + (drawn.x - old_frame.x),
            new_rect.y + (drawn.y - old_frame.y),
            (new_rect.width - (old_frame.width - drawn.width)).max(0.0),
            (new_rect.height - (old_frame.height - drawn.height)).max(0.0),
        );
        Box::new(stretch(drawn, target))
    };
    transform_geometry(doc, object, &space, &map)?;
    if subtype == "Text" {
        move_popup(doc, object, &space, &map)?;
    }
    Ok(())
}

/// a function that maps `from` onto `to`. a side with no length (a
/// flat line) is moved, not stretched, since it can't be scaled.
fn stretch(from: Rect, to: Rect) -> impl Fn(Point) -> Point {
    let axis = |start: f32, length: f32, new_start: f32, new_length: f32| {
        move |value: f32| {
            if length.abs() < 1e-6 {
                value - start + new_start + new_length / 2.0
            } else {
                new_start + (value - start) * new_length / length
            }
        }
    };
    let x = axis(from.x, from.width, to.x, to.width);
    let y = axis(from.y, from.height, to.y, to.height);
    move |point: Point| Point::new(x(point.x), y(point.y))
}

/// the box around an annotation's drawn points (quads, vertices, line
/// ends, ink), or none for kinds that are simply their rectangle.
fn drawn_bounds(
    doc: &Document,
    object: ObjectId,
    subtype: &str,
    space: &PageSpace,
) -> Option<Rect> {
    let annotation = doc.get_dictionary(object).ok()?;
    let points: Vec<Point> = match subtype {
        "Highlight" | "Underline" | "StrikeOut" | "Squiggly" => {
            crate::markup::quads(doc, annotation, space)
                .into_iter()
                .flatten()
                .collect()
        }
        "Line" => {
            let (start, end) = shapes::line_ends(doc, annotation, space)?;
            vec![start, end]
        }
        "Polygon" | "PolyLine" => shapes::vertices(doc, annotation, space),
        "Ink" => ink::strokes(doc, annotation, space)
            .into_iter()
            .flatten()
            .collect(),
        _ => return None,
    };
    Rect::around(points)
}

/// the entries that hold flat lists of x, y pairs in user space.
const POINT_LIST_KEYS: [&[u8]; 4] = [b"QuadPoints", b"Vertices", b"L", b"CL"];

/// applies `map` (in visible points) to every position the annotation
/// stores: its rectangle and any point lists.
fn transform_geometry(
    doc: &mut Document,
    object: ObjectId,
    space: &PageSpace,
    map: &dyn Fn(Point) -> Point,
) -> Result<(), AnnotateError> {
    let annotation = doc
        .get_dictionary(object)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;
    let map_pairs = |values: &[f32]| -> Vec<f32> {
        let (pairs, _) = values.as_chunks::<2>();
        pairs
            .iter()
            .flat_map(|[x, y]| {
                let visible = space.visible_point(Point::new(*x, *y));
                let user = space.user_point(map(visible));
                [user.x, user.y]
            })
            .collect()
    };
    // read everything first, then write, since reading may follow references
    let mut updates: Vec<(&[u8], Object)> = Vec::new();
    if let Some(rect) = dict_rect(doc, annotation, b"Rect") {
        let visible = space.visible_rect(rect);
        let corners = visible.corners().map(map);
        let moved = Rect::around(corners).expect("four corners");
        updates.push((b"Rect", number_array(&space.user_rect(moved))));
    }
    for key in POINT_LIST_KEYS {
        if let Some(values) = annotation.get(key).ok().and_then(|list| numbers(doc, list)) {
            updates.push((key, number_array(&map_pairs(&values))));
        }
    }
    if let Some(list) = annotation
        .get(b"InkList")
        .ok()
        .map(|list| resolve(doc, list))
    {
        if let Ok(strokes) = list.as_array() {
            let moved = strokes
                .iter()
                .filter_map(|stroke| numbers(doc, stroke))
                .map(|values| number_array(&map_pairs(&values)))
                .collect();
            updates.push((b"InkList", Object::Array(moved)));
        }
    }
    let annotation = doc
        .get_dictionary_mut(object)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;
    for (key, value) in updates {
        annotation.set(key.to_vec(), value);
    }
    Ok(())
}

/// moves a note's pop-up window along with its icon.
fn move_popup(
    doc: &mut Document,
    note: ObjectId,
    space: &PageSpace,
    map: &dyn Fn(Point) -> Point,
) -> Result<(), AnnotateError> {
    let popup = doc
        .get_dictionary(note)
        .ok()
        .and_then(|note| common::reference(note, b"Popup"))
        .filter(|popup| doc.get_dictionary(*popup).is_ok());
    match popup {
        Some(popup) => transform_geometry(doc, popup, space, map),
        None => Ok(()),
    }
}

/// writes the style and text changes into the dictionary.
fn apply_changes(
    doc: &mut Document,
    space: &PageSpace,
    object: ObjectId,
    subtype: &str,
    changes: &AnnotationChanges,
) -> Result<(), AnnotateError> {
    // the text box style lives in one string, so read it before borrowing
    let text_box_style = (subtype == "FreeText").then(|| {
        let annotation = doc.get_dictionary(object).expect("found above");
        text_box::read_style(doc, annotation)
    });
    let current_border = {
        let annotation = doc
            .get_dictionary(object)
            .map_err(|_| AnnotateError::UnknownAnnotation)?;
        common::border(doc, annotation, 1.0)
    };
    let annotation = doc
        .get_dictionary_mut(object)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;

    if let Some(style) = text_box_style {
        apply_text_box_changes(annotation, style, changes);
    } else {
        apply_shared_style(annotation, subtype, current_border, changes);
    }
    if let Some(opacity) = changes.opacity {
        annotation.set("CA", real(clamp_opacity(opacity)));
    }
    if let Some(contents) = &changes.contents {
        common::set_text(annotation, "Contents", contents.as_deref());
    }
    if let Some(author) = &changes.author {
        common::set_text(annotation, "T", Some(author));
    }
    if let Some(icon) = changes.icon {
        annotation.set("Name", name(icon.pdf_name()));
    }
    if let Some(open) = changes.open {
        annotation.set("Open", Object::Boolean(open));
    }
    if subtype == "Line" || subtype == "Polygon" || subtype == "PolyLine" || subtype == "Ink" {
        apply_geometry_changes(annotation, space, subtype, changes);
    }
    if !annotation.has(b"NM") {
        // from now on the annotation keeps this id, through save and reload
        annotation.set("NM", text_string::encode(&new_unique_name()));
    }
    common::touch(annotation);
    let popup = common::reference(annotation, b"Popup");
    // the pop-up has its own open state, which viewers read first
    if let (Some(open), Some(popup)) = (changes.open, popup) {
        if let Ok(popup) = doc.get_dictionary_mut(popup) {
            popup.set("Open", Object::Boolean(open));
        }
    }
    Ok(())
}

fn apply_text_box_changes(
    annotation: &mut Dictionary,
    mut style: text_box::TextBoxStyle,
    changes: &AnnotationChanges,
) {
    if let Some(color) = changes.color {
        style.text_color = color;
    }
    if let Some(fill) = changes.fill {
        style.fill = fill;
    }
    if let Some(font) = changes.font {
        style.font = font;
    }
    if let Some(size) = changes.font_size {
        style.font_size = size;
    }
    if let Some(align) = changes.align {
        style.align = align;
    }
    if let Some(border) = changes.border {
        style.border = border;
    }
    text_box::write_style(annotation, &style);
    if let Some(text) = &changes.text {
        annotation.set("Contents", text_string::encode(text));
    }
}

fn apply_shared_style(
    annotation: &mut Dictionary,
    subtype: &str,
    current: Border,
    changes: &AnnotationChanges,
) {
    if let Some(color) = changes.color {
        annotation.set("C", color.to_object());
    }
    if let Some(fill) = changes.fill {
        match fill {
            Some(fill) => annotation.set("IC", fill.to_object()),
            None => {
                annotation.remove(b"IC");
            }
        }
    }
    let has_border = matches!(
        subtype,
        "Square" | "Circle" | "Line" | "Polygon" | "PolyLine" | "Ink"
    );
    if has_border && (changes.stroke_width.is_some() || changes.dash.is_some()) {
        let border = Border {
            width: changes.stroke_width.unwrap_or(current.width),
            dash: changes
                .dash
                .clone()
                .map(common::clean_dash)
                .unwrap_or(current.dash),
        };
        common::set_border(annotation, &border);
    }
}

fn apply_geometry_changes(
    annotation: &mut Dictionary,
    space: &PageSpace,
    subtype: &str,
    changes: &AnnotationChanges,
) {
    if let Some(points) = &changes.points {
        if subtype == "Line" {
            shapes::write_line_ends(annotation, space, points[0], points[1]);
        } else if subtype != "Ink" {
            shapes::write_vertices(annotation, space, points);
        }
    }
    if subtype == "Line" && (changes.start_ending.is_some() || changes.end_ending.is_some()) {
        let ending_name = |ending: LineEnding| name(ending.pdf_name());
        let current: Vec<Object> = annotation
            .get(b"LE")
            .ok()
            .and_then(|endings| endings.as_array().ok())
            .cloned()
            .unwrap_or_else(|| vec![name("None"), name("None")]);
        let start = changes
            .start_ending
            .map(ending_name)
            .or_else(|| current.first().cloned())
            .unwrap_or(name("None"));
        let end = changes
            .end_ending
            .map(ending_name)
            .or_else(|| current.get(1).cloned())
            .unwrap_or(name("None"));
        annotation.set("LE", Object::Array(vec![start, end]));
    }
    if let (Some(strokes), "Ink") = (&changes.strokes, subtype) {
        ink::write_strokes(annotation, space, &ink::usable_strokes(strokes));
    }
}

/// deletes an annotation with its pop-up and every reply in its thread.
/// returns the ids of the annotations removed (pop-ups aren't listed, so
/// they aren't in it).
pub fn delete(doc: &mut Document, page: usize, id: &str) -> Result<Vec<String>, AnnotateError> {
    let page = annots::page(doc, page)?;
    annots::make_entries_objects(doc, page.id)?;
    let target = annots::find(doc, page.id, id)?;
    let target_dict = doc
        .get_dictionary(target)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;
    if annots::subtype(doc, target_dict) == "Widget" {
        return Err(AnnotateError::FormField);
    }
    let thread = thread_of(doc, page.id, target);
    let mut removed_ids = Vec::new();
    let mut appearance_objects = Vec::new();
    for object in &thread {
        if let Ok(annotation) = doc.get_dictionary(*object) {
            if annots::subtype(doc, annotation) != "Popup" {
                removed_ids.push(annots::id_of(doc, *object, annotation));
            }
            appearance_objects.extend(crate::appearance::appearance_objects(annotation));
        }
    }
    // a pop-up being deleted on its own leaves its note without one
    clear_popup_links(doc, page.id, &thread);
    annots::remove(doc, page.id, &thread)?;
    for object in &thread {
        doc.objects.remove(object);
    }
    annots::remove_unused(doc, page.id, &appearance_objects);
    annots::remove(doc, page.id, &HashSet::new())?;
    Ok(removed_ids)
}

/// an annotation together with its pop-up and all replies to it (and
/// replies to those), found among the annotations on the page.
pub(crate) fn thread_of(doc: &Document, page: ObjectId, root: ObjectId) -> HashSet<ObjectId> {
    let on_page = annots::annotation_ids(doc, page);
    let mut thread = HashSet::from([root]);
    loop {
        let before = thread.len();
        for object in &on_page {
            let Ok(annotation) = doc.get_dictionary(*object) else {
                continue;
            };
            let points_into_thread = |key: &[u8]| {
                common::reference(annotation, key).is_some_and(|target| thread.contains(&target))
            };
            if points_into_thread(b"IRT") || points_into_thread(b"Parent") {
                thread.insert(*object);
            }
        }
        let popups: Vec<ObjectId> = thread
            .iter()
            .filter_map(|object| doc.get_dictionary(*object).ok())
            .filter_map(|annotation| common::reference(annotation, b"Popup"))
            .collect();
        thread.extend(popups);
        if thread.len() == before {
            return thread;
        }
    }
}

/// removes /Popup entries that point at annotations about to be removed.
fn clear_popup_links(doc: &mut Document, page: ObjectId, removed: &HashSet<ObjectId>) {
    for object in annots::annotation_ids(doc, page) {
        if removed.contains(&object) {
            continue;
        }
        if let Ok(annotation) = doc.get_dictionary_mut(object) {
            if common::reference(annotation, b"Popup").is_some_and(|popup| removed.contains(&popup))
            {
                annotation.remove(b"Popup");
            }
        }
    }
}
