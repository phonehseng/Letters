//! drawings in an opendocument text: custom shapes, rectangles, ovals,
//! lines, paths, polygons, connectors, groups (`draw:g`), text box frames
//! and floating tables (a frame holding nothing but a table).
//!
//! each object keeps the xml it was read from (`OriginalObject`) so an
//! untouched object is written back byte for byte; its graphic style is
//! read through its parents and the default style, as applications do.

use std::collections::HashMap;

use crate::model::*;
use crate::odt::graphics::{
    read_fill, read_stroke, read_text_area, GraphicProperties, MODELLED_GRAPHIC_ATTRIBUTES,
};
use crate::odt::paths::{from_enhanced, from_points, from_svg};
use crate::units::{emu_to_twips, parse_length_emu};
use crate::xml::{Element, XmlDocument};

use super::OdtReader;

/// the drawing elements that can float in a text.
const DRAWING_ELEMENTS: &[&str] = &[
    "draw:custom-shape",
    "draw:rect",
    "draw:ellipse",
    "draw:circle",
    "draw:line",
    "draw:path",
    "draw:polygon",
    "draw:polyline",
    "draw:connector",
    "draw:g",
    "draw:frame",
];

pub(super) fn is_drawing_element(element: &Element) -> bool {
    DRAWING_ELEMENTS.contains(&element.name.as_str())
}

/// where an object sits: offsets and size in emu, from its own attributes
/// or its `draw:transform`.
struct Placed {
    horizontal: FloatPosition,
    vertical: FloatPosition,
    cx: i64,
    cy: i64,
    rotation: Option<f64>,
    wrap: Wrap,
    z_order: Option<i64>,
    name: Option<String>,
    description: Option<String>,
    title: Option<String>,
}

fn length(element: &Element, name: &str) -> Option<i64> {
    element.attr(name).and_then(parse_length_emu)
}

fn position(relative_to: &str, emu: i64) -> FloatPosition {
    FloatPosition {
        relative_to: relative_to.to_string(),
        offset: emu_to_twips(emu),
        align: None,
        offset_emu: Some(emu),
    }
}

/// the model's anchor names for opendocument's position relations.
fn relation(value: Option<&str>, anchor: &str, vertical: bool) -> &'static str {
    let default = if anchor == "page" {
        "page"
    } else {
        "paragraph"
    };
    match value.unwrap_or(default) {
        "page" | "page-start-margin" | "page-end-margin" => "page",
        "page-content" => "margin",
        "char" => "character",
        "line" if vertical => "line",
        _ if vertical => "paragraph",
        _ => "column",
    }
}

/// how text flows around an object, from its graphic properties.
fn wrap_of(properties: &GraphicProperties) -> Wrap {
    let contour = properties
        .get("style:wrap-contour")
        .is_some_and(|c| c == "true");
    match properties.get("style:wrap").map(String::as_str) {
        Some("none") => Wrap::TopAndBottom,
        Some("run-through")
            if properties
                .get("style:run-through")
                .is_some_and(|r| r == "background") =>
        {
            Wrap::BehindText
        }
        Some("run-through") => Wrap::InFrontOfText,
        Some(_) if contour => Wrap::Tight,
        _ => Wrap::Square,
    }
}

/// `rotate (a) translate (x y)`, the only transform shapes in a text use:
/// the rotation (radians, counterclockwise) turns the shape about its
/// top-left corner, which then moves to (x, y). the model keeps the
/// unturned box and a clockwise turn about the centre instead.
fn transform(value: &str, cx: i64, cy: i64) -> Option<(i64, i64, f64)> {
    let mut angle = 0.0;
    let mut offset = (0.0, 0.0);
    for part in value.split(')') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, arguments) = part.split_once('(')?;
        let numbers: Vec<&str> = arguments
            .split([' ', ','])
            .filter(|s| !s.is_empty())
            .collect();
        match name.trim() {
            "rotate" => angle = numbers.first()?.parse::<f64>().ok()?,
            "translate" => {
                let x = parse_length_emu(numbers.first()?)? as f64;
                let y = numbers
                    .get(1)
                    .and_then(|y| parse_length_emu(y))
                    .unwrap_or(0) as f64;
                offset = (offset.0 + x, offset.1 + y);
            }
            _ => return None,
        }
    }
    let (half_w, half_h) = (cx as f64 / 2.0, cy as f64 / 2.0);
    let (sin, cos) = angle.sin_cos();
    // where the centre ends up: the turned half-size from the moved corner
    let centre = (
        offset.0 + half_w * cos + half_h * sin,
        offset.1 - half_w * sin + half_h * cos,
    );
    let clockwise = (-angle.to_degrees()).rem_euclid(360.0);
    Some((
        (centre.0 - half_w).round() as i64,
        (centre.1 - half_h).round() as i64,
        clockwise,
    ))
}

impl OdtReader<'_> {
    /// a graphic style's properties with what it inherits from its parents
    /// and the default graphic style underneath.
    pub(super) fn graphic_properties(&self, style_name: Option<&str>) -> GraphicProperties {
        let mut layers: Vec<&Vec<(String, String)>> = Vec::new();
        let mut name = style_name.map(str::to_string);
        for _ in 0..16 {
            let Some(current) = name.take() else { break };
            let style = self
                .content_auto
                .get(&current)
                .or_else(|| self.styles_auto.get(&current))
                .or_else(|| self.graphic_common.get(&current));
            let Some(style) = style else { break };
            layers.push(&style.graphic);
            name = style.parent.clone();
        }
        let mut properties: GraphicProperties = self
            .graphic_default
            .as_ref()
            .map(|style| style.graphic.iter().cloned().collect())
            .unwrap_or_default();
        for layer in layers.into_iter().rev() {
            for (key, value) in layer {
                properties.insert(key.clone(), value.clone());
            }
        }
        properties
    }

    /// the style attributes the model does not read, kept with a drawing.
    fn preserved_graphic(&self, element: &Element) -> DrawingPreserved {
        let own = element.attr("draw:style-name").and_then(|name| {
            self.content_auto
                .get(name)
                .or_else(|| self.styles_auto.get(name))
        });
        let attributes: Vec<Attribute> = own
            .map(|style| {
                style
                    .graphic
                    .iter()
                    .filter(|(key, _)| !MODELLED_GRAPHIC_ATTRIBUTES.contains(&key.as_str()))
                    .map(|(key, value)| Attribute::new(key, value))
                    .collect()
            })
            .unwrap_or_default();
        DrawingPreserved {
            format: Some(SourceFormat::Odt),
            attributes,
            ..Default::default()
        }
    }

    fn placed(
        &self,
        element: &Element,
        properties: &GraphicProperties,
        in_group: bool,
    ) -> Option<Placed> {
        let anchor = element.attr("text:anchor-type").unwrap_or("paragraph");
        let (mut cx, mut cy) = (
            length(element, "svg:width").unwrap_or(0),
            length(element, "svg:height").unwrap_or(0),
        );
        let mut x = length(element, "svg:x").unwrap_or(0);
        let mut y = length(element, "svg:y").unwrap_or(0);
        if element.name == "draw:line" || element.name == "draw:connector" {
            let (x1, y1) = (length(element, "svg:x1")?, length(element, "svg:y1")?);
            let (x2, y2) = (length(element, "svg:x2")?, length(element, "svg:y2")?);
            x = x1.min(x2);
            y = y1.min(y2);
            cx = (x2 - x1).abs();
            cy = (y2 - y1).abs();
        }
        let mut rotation = None;
        if let Some(value) = element.attr("draw:transform") {
            let (left, top, degrees) = transform(value, cx, cy)?;
            x = left;
            y = top;
            rotation = (degrees.abs() > 1e-9).then_some(degrees);
        }
        let (horizontal, vertical) = if in_group {
            (position("group", x), position("group", y))
        } else {
            (
                position(
                    relation(
                        properties.get("style:horizontal-rel").map(String::as_str),
                        anchor,
                        false,
                    ),
                    x,
                ),
                position(
                    relation(
                        properties.get("style:vertical-rel").map(String::as_str),
                        anchor,
                        true,
                    ),
                    y,
                ),
            )
        };
        Some(Placed {
            horizontal,
            vertical,
            cx,
            cy,
            rotation,
            wrap: wrap_of(properties),
            z_order: element.attr("draw:z-index").and_then(|z| z.parse().ok()),
            name: element.attr("draw:name").map(str::to_string),
            description: element.child("svg:desc").map(Element::deep_text),
            title: element.child("svg:title").map(Element::deep_text),
        })
    }

    /// a drawing element floating in the text, as a block; `None` for
    /// pictures (read as inline pictures) and what the model cannot hold.
    pub(super) fn read_drawing(&mut self, element: &Element, xml: &XmlDocument) -> Option<Block> {
        if element.attr("text:anchor-type") == Some("as-char") {
            return None;
        }
        let mut block = match self.read_member(element, xml, false)? {
            // libreoffice writes word's text boxes as rectangles holding text
            GroupChild::Shape(shape) => match shape.as_text_box() {
                Some(text_box) => Block::TextBox(text_box),
                None => Block::Shape(shape),
            },
            GroupChild::TextBox(text_box) => match floating_table(&text_box) {
                Some(table) => Block::Table(table),
                None => Block::TextBox(text_box),
            },
            GroupChild::Group(group) => Block::Group(group),
            GroupChild::Picture(_) => return None,
        };
        let raw = xml.raw(element).to_string();
        match &mut block {
            Block::Shape(shape) => remember_original(shape, SourceFormat::Odt, raw),
            Block::Group(group) => remember_original(group, SourceFormat::Odt, raw),
            Block::TextBox(text_box) => remember_original(text_box, SourceFormat::Odt, raw),
            _ => {}
        }
        Some(block)
    }

    /// one drawing, on the page or inside a group.
    fn read_member(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        in_group: bool,
    ) -> Option<GroupChild> {
        let properties = self.graphic_properties(element.attr("draw:style-name"));
        let placed = self.placed(element, &properties, in_group)?;
        match element.name.as_str() {
            "draw:g" => self
                .read_group(element, xml, placed, in_group)
                .map(GroupChild::Group),
            "draw:frame" => self.read_frame_object(element, xml, &properties, placed),
            _ => self
                .read_shape(element, xml, &properties, placed)
                .map(GroupChild::Shape),
        }
    }

    fn read_shape(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        properties: &GraphicProperties,
        placed: Placed,
    ) -> Option<Shape> {
        let (geometry, flip_horizontal, flip_vertical) = self.geometry(element, &placed)?;
        let resources = self.resources.clone();
        let fill = {
            let mut picture = |path: &str| self.media_for(path);
            read_fill(properties, &resources, &mut picture)
        };
        let fill = match element.name.as_str() {
            "draw:line" | "draw:polyline" | "draw:connector" => Fill::None,
            _ => fill,
        };
        let stroke = read_stroke(properties, &self.resources);
        let text = self.shape_text(element, xml, properties);
        let connector = (element.name == "draw:connector").then(|| Connector {
            kind: match element.attr("draw:type") {
                Some("curve") => "curved",
                Some("standard") | Some("lines") => "elbow",
                _ => "straight",
            }
            .into(),
            start: element
                .attr("draw:start-shape")
                .map(|object| ConnectionEnd {
                    object: object.to_string(),
                    site: element
                        .attr("draw:start-glue-point")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                }),
            end: element.attr("draw:end-shape").map(|object| ConnectionEnd {
                object: object.to_string(),
                site: element
                    .attr("draw:end-glue-point")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            }),
        });
        Some(Shape {
            name: placed.name,
            description: placed.description,
            title: placed.title,
            horizontal: placed.horizontal,
            vertical: placed.vertical,
            width: emu_to_twips(placed.cx),
            height: emu_to_twips(placed.cy),
            width_emu: Some(placed.cx),
            height_emu: Some(placed.cy),
            wrap: placed.wrap,
            z_order: placed.z_order,
            rotation: placed.rotation,
            flip_horizontal,
            flip_vertical,
            geometry,
            fill,
            stroke,
            text,
            connector,
            preserved: self.preserved_graphic(element),
            anchor: None,
            original: None,
        })
    }

    /// the outline of a drawing element, and whether it is mirrored.
    fn geometry(&self, element: &Element, placed: &Placed) -> Option<(Geometry, bool, bool)> {
        let view = |element: &Element| -> (f64, f64) {
            element
                .attr("svg:viewBox")
                .map(|v| {
                    v.split_whitespace()
                        .filter_map(|n| n.parse::<f64>().ok())
                        .collect::<Vec<_>>()
                })
                .filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0)
                .map(|v| (v[2], v[3]))
                .unwrap_or((placed.cx.max(1) as f64, placed.cy.max(1) as f64))
        };
        let geometry = match element.name.as_str() {
            "draw:rect" => match element
                .attr("draw:corner-radius")
                .and_then(parse_length_emu)
            {
                Some(radius) if radius > 0 => {
                    let shorter = placed.cx.min(placed.cy).max(1);
                    Geometry::Preset {
                        name: "roundRect".into(),
                        adjust: vec![Adjust {
                            name: "adj".into(),
                            value: (radius * 100_000 / shorter).min(50_000),
                        }],
                    }
                }
                _ => Geometry::preset("rect"),
            },
            "draw:ellipse" | "draw:circle" => {
                if element.attr("draw:kind").is_some_and(|k| k != "full") {
                    return None;
                }
                Geometry::preset("ellipse")
            }
            "draw:line" | "draw:connector" => {
                let flip_h = length(element, "svg:x2")? < length(element, "svg:x1")?;
                let flip_v = length(element, "svg:y2")? < length(element, "svg:y1")?;
                let name = if element.name == "draw:line" {
                    "line"
                } else {
                    "straightConnector1"
                };
                return Some((Geometry::preset(name), flip_h, flip_v));
            }
            "draw:path" => {
                let (w, h) = view(element);
                Geometry::Path {
                    paths: vec![from_svg(element.attr("svg:d")?, w, h)?],
                }
            }
            "draw:polygon" | "draw:polyline" => {
                let (w, h) = view(element);
                let closed = element.name == "draw:polygon";
                Geometry::Path {
                    paths: vec![from_points(element.attr("draw:points")?, w, h, closed)?],
                }
            }
            "draw:custom-shape" => {
                return self.custom_geometry(element.child("draw:enhanced-geometry")?, placed)
            }
            _ => return None,
        };
        Some((geometry, false, false))
    }

    /// a custom shape's outline: its preset type, or its path when that
    /// uses plain numbers only.
    fn custom_geometry(
        &self,
        geometry: &Element,
        placed: &Placed,
    ) -> Option<(Geometry, bool, bool)> {
        let flip_h = geometry.attr("draw:mirror-horizontal") == Some("true");
        let flip_v = geometry.attr("draw:mirror-vertical") == Some("true");
        let kind = geometry.attr("draw:type").unwrap_or("non-primitive");
        if let Some(name) = crate::shapes::preset_for_odf_type(kind) {
            let modifiers: Vec<i64> = geometry
                .attr("draw:modifiers")
                .unwrap_or_default()
                .split_whitespace()
                .filter_map(|m| m.parse::<f64>().ok().map(|v| v.round() as i64))
                .collect();
            let adjust = adjustments(&name, kind.starts_with("ooxml-"), &modifiers);
            return Some((Geometry::Preset { name, adjust }, flip_h, flip_v));
        }
        let path = geometry.attr("draw:enhanced-path")?;
        let (w, h) = geometry
            .attr("svg:viewBox")
            .map(|v| {
                v.split_whitespace()
                    .filter_map(|n| n.parse::<f64>().ok())
                    .collect::<Vec<_>>()
            })
            .filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0)
            .map(|v| (v[2], v[3]))
            .unwrap_or((placed.cx.max(1) as f64, placed.cy.max(1) as f64));
        Some((
            Geometry::Path {
                paths: from_enhanced(path, w, h)?,
            },
            flip_h,
            flip_v,
        ))
    }

    /// the text inside a shape: its paragraphs, read like any others.
    fn shape_text(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        properties: &GraphicProperties,
    ) -> Option<ShapeText> {
        let saved = self.after_space;
        let mut blocks = Vec::new();
        for child in element.elements() {
            if matches!(child.name.as_str(), "text:p" | "text:h" | "text:list") {
                self.read_block(child, xml, &mut blocks);
            }
        }
        self.after_space = saved;
        // libreoffice gives every shape an empty paragraph; that is no text
        let empty = blocks
            .iter()
            .all(|b| b.paragraph().is_some_and(|p| p.content.is_empty()));
        if empty {
            return None;
        }
        let (margins, vertical_align, auto_fit) = read_text_area(properties);
        // the text colour a graphic style gives its shape's text
        let color = element
            .attr("draw:style-name")
            .and_then(|name| self.style(name))
            .and_then(|(style, _)| style.run.color.clone());
        Some(ShapeText {
            blocks,
            margins,
            vertical_align,
            auto_fit,
            color,
        })
    }

    /// a `draw:frame`: a text box (with its place in a chain of linked
    /// boxes), or a picture inside a group.
    fn read_frame_object(
        &mut self,
        frame: &Element,
        xml: &XmlDocument,
        properties: &GraphicProperties,
        placed: Placed,
    ) -> Option<GroupChild> {
        if frame.child("draw:image").is_some() && frame.child("draw:text-box").is_none() {
            if placed.horizontal.relative_to != "group" {
                return None;
            }
            let mut image = self.read_picture(frame, xml)?;
            image.width = emu_to_twips(placed.cx);
            image.height = emu_to_twips(placed.cy);
            image.width_emu = Some(placed.cx);
            image.height_emu = Some(placed.cy);
            image.rotation = placed.rotation;
            image.placement = Placement::Floating {
                horizontal: placed.horizontal,
                vertical: placed.vertical,
                wrap: Wrap::InFrontOfText,
            };
            return Some(GroupChild::Picture(image));
        }
        let content = frame.child("draw:text-box")?;
        let saved = self.after_space;
        let mut blocks = Vec::new();
        for child in content.elements() {
            if matches!(
                child.name.as_str(),
                "text:p" | "text:h" | "text:list" | "table:table" | "text:section"
            ) {
                self.read_block(child, xml, &mut blocks);
            }
        }
        self.after_space = saved;
        let fixed_height = length(frame, "svg:height");
        let min_height = length(content, "fo:min-height").or_else(|| {
            properties
                .get("fo:min-height")
                .and_then(|h| parse_length_emu(h))
        });
        let cy = fixed_height.or(min_height).unwrap_or(457_200);
        let style = frame
            .attr("draw:style-name")
            .and_then(|name| self.style(name))
            .map(|(style, _)| style.clone())
            .unwrap_or_default();
        let mut text_box = TextBox::new(0, 0, emu_to_twips(placed.cx), emu_to_twips(cy), blocks);
        text_box.name = placed.name;
        text_box.horizontal = placed.horizontal;
        text_box.vertical = placed.vertical;
        text_box.width_emu = Some(placed.cx);
        text_box.height_emu = Some(cy);
        text_box.auto_height = fixed_height.is_none();
        text_box.wrap = placed.wrap;
        text_box.z_order = placed.z_order;
        text_box.rotation = placed.rotation;
        text_box.margins = style.box_margins.unwrap_or_default();
        text_box.border = style.cell_borders.and_then(|borders| borders.top);
        text_box.fill = style.background;
        text_box.vertical_align = read_text_area(properties).1;
        text_box.chain = text_box
            .name
            .as_ref()
            .and_then(|name| self.chains.get(name))
            .cloned();
        text_box.preserved = self.preserved_graphic(frame);
        Some(GroupChild::TextBox(text_box))
    }

    fn read_group(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        placed: Placed,
        in_group: bool,
    ) -> Option<Group> {
        let mut members = Vec::new();
        for child in element.elements() {
            if is_drawing_element(child) {
                members.push(self.read_member(child, xml, true)?);
            } else if !matches!(
                child.name.as_str(),
                "svg:title" | "svg:desc" | "office:event-listeners"
            ) {
                return None;
            }
        }
        // a group has no box of its own: it is the box around its members,
        // whose positions are on the page; the model keeps them relative
        let bounds = members
            .iter()
            .map(member_box)
            .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))?;
        let (left, top, right, bottom) = bounds;
        for member in &mut members {
            shift_member(member, -left, -top);
        }
        let (cx, cy) = (right - left, bottom - top);
        let relative = |position: &FloatPosition| {
            if in_group {
                "group".to_string()
            } else {
                position.relative_to.clone()
            }
        };
        Some(Group {
            name: placed.name,
            description: placed.description,
            title: placed.title,
            horizontal: position(&relative(&placed.horizontal), left),
            vertical: position(&relative(&placed.vertical), top),
            width: emu_to_twips(cx),
            height: emu_to_twips(cy),
            width_emu: Some(cx),
            height_emu: Some(cy),
            wrap: placed.wrap,
            z_order: placed.z_order,
            rotation: None,
            flip_horizontal: false,
            flip_vertical: false,
            child_offset_x: 0,
            child_offset_y: 0,
            child_width: cx.max(1),
            child_height: cy.max(1),
            children: members,
            preserved: self.preserved_graphic(element),
            anchor: None,
            original: None,
        })
    }

    /// one child of `office:styles` that drawings need: a graphic style,
    /// the default graphic style, or a named drawing definition.
    pub(super) fn read_drawing_style(&mut self, element: &Element, xml: &XmlDocument) {
        let graphic = element.attr("style:family") == Some("graphic");
        match element.name.as_str() {
            "style:style" if graphic => {
                let style = super::read_style(element, xml, &self.fonts);
                self.graphic_common.insert(style.name.clone(), style);
            }
            "style:default-style" if graphic => {
                self.graphic_default = Some(super::read_style(element, xml, &self.fonts));
            }
            _ => self.resources.add(element),
        }
    }

    /// finds the chains of linked text boxes (`draw:chain-next-name`)
    /// before the body is read, so each box knows its place.
    pub(super) fn find_chains(&mut self, root: &Element) {
        let mut next_of: HashMap<String, String> = HashMap::new();
        root.walk(&mut |element| {
            if element.name != "draw:frame" {
                return;
            }
            let next = element
                .child("draw:text-box")
                .and_then(|b| b.attr("draw:chain-next-name"));
            if let (Some(name), Some(next)) = (element.attr("draw:name"), next) {
                next_of.insert(name.to_string(), next.trim_start_matches('#').to_string());
            }
        });
        let targets: std::collections::HashSet<String> = next_of.values().cloned().collect();
        for head in next_of.keys().filter(|name| !targets.contains(*name)) {
            let mut name = head.clone();
            for sequence in 0..1000 {
                self.chains.insert(
                    name.clone(),
                    TextChain {
                        id: head.clone(),
                        sequence,
                    },
                );
                match next_of.get(&name) {
                    Some(next) => name = next.clone(),
                    None => break,
                }
            }
        }
    }
}

/// drawingml handle values for an opendocument shape's modifiers. shapes
/// libreoffice wrote from word ("ooxml-") keep word's units; of the
/// native ones only the rounded rectangle's corner has the same meaning.
fn adjustments(name: &str, from_word: bool, modifiers: &[i64]) -> Vec<Adjust> {
    if from_word {
        let names: Vec<String> = match modifiers.len() {
            0 => Vec::new(),
            1 => vec!["adj".into()],
            count => (1..=count).map(|n| format!("adj{n}")).collect(),
        };
        return names
            .into_iter()
            .zip(modifiers)
            .map(|(name, value)| Adjust {
                name,
                value: *value,
            })
            .collect();
    }
    match (name, modifiers.first()) {
        ("roundRect", Some(corner)) => {
            let value = crate::shapes::round_rect_adjust_from_odf(*corner);
            if value == crate::shapes::ROUND_RECT_DEFAULT {
                Vec::new()
            } else {
                vec![Adjust {
                    name: "adj".into(),
                    value,
                }]
            }
        }
        _ => Vec::new(),
    }
}

/// a group member's box in emu: left, top, right, bottom.
fn member_box(member: &GroupChild) -> (i64, i64, i64, i64) {
    let (h, v, (cx, cy)) = match member {
        GroupChild::Shape(s) => (&s.horizontal, &s.vertical, s.size_emu()),
        GroupChild::TextBox(t) => (&t.horizontal, &t.vertical, t.size_emu()),
        GroupChild::Group(g) => (&g.horizontal, &g.vertical, g.size_emu()),
        GroupChild::Picture(i) => match &i.placement {
            Placement::Floating {
                horizontal,
                vertical,
                ..
            } => (horizontal, vertical, i.size_emu()),
            Placement::Inline => return (0, 0, 0, 0),
        },
    };
    (h.emu(), v.emu(), h.emu() + cx, v.emu() + cy)
}

fn shift(position: &mut FloatPosition, by: i64) {
    let emu = position.emu() + by;
    *position = FloatPosition {
        relative_to: "group".into(),
        offset: emu_to_twips(emu),
        align: None,
        offset_emu: Some(emu),
    };
}

fn shift_member(member: &mut GroupChild, dx: i64, dy: i64) {
    match member {
        GroupChild::Shape(s) => {
            shift(&mut s.horizontal, dx);
            shift(&mut s.vertical, dy);
        }
        GroupChild::TextBox(t) => {
            shift(&mut t.horizontal, dx);
            shift(&mut t.vertical, dy);
        }
        GroupChild::Group(g) => {
            shift(&mut g.horizontal, dx);
            shift(&mut g.vertical, dy);
        }
        GroupChild::Picture(i) => {
            if let Placement::Floating {
                horizontal,
                vertical,
                ..
            } = &mut i.placement
            {
                shift(horizontal, dx);
                shift(vertical, dy);
            }
        }
    }
}

/// a frame that holds nothing but one table and draws no frame of its
/// own is how opendocument floats a table; the model has floating tables.
fn floating_table(text_box: &TextBox) -> Option<Table> {
    let [Block::Table(table)] = text_box.blocks.as_slice() else {
        return None;
    };
    if text_box.border.is_some()
        || text_box.fill.is_some()
        || text_box.chain.is_some()
        || text_box.rotation.is_some()
    {
        return None;
    }
    let mut table = table.clone();
    table.position = Some(TablePosition {
        horizontal: text_box.horizontal.clone(),
        vertical: text_box.vertical.clone(),
        distance: Distances::default(),
        overlap: true,
        anchor: None,
    });
    if table.width.is_none() {
        table.width = Some(text_box.width);
    }
    Some(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_become_a_box_and_a_clockwise_turn() {
        // a 1 by 0.5 inch box at the origin, turned 90 degrees clockwise
        // about its centre: its turned corner lands at (0.75in, -0.25in)
        let value = "rotate (-1.5707963267949) translate (0.75in -0.25in)";
        let (left, top, turn) = transform(value, 914_400, 457_200).unwrap();
        assert!((turn - 90.0).abs() < 1e-6, "{turn}");
        assert!(left.abs() <= 1 && top.abs() <= 1, "{left} {top}");
        assert!(transform("skewX (0.5)", 10, 10).is_none());
    }
}
