//! objects written as opendocument drawing elements with their effective
//! geometry, fill, line and text (inheritance resolved), so a deck from
//! pptx shows the same in an odp reader.

use std::collections::HashMap;

use super::units::write_length;
use super::write_styles::StyleRegistry;
use super::write_text::TextWriter;
use crate::color::{resolve, ColorContext};
use crate::inherit::{ancestors, effective_transform, Chain};
use crate::model::{
    Connector, Fill, Geometry, GraphicFrame, GraphicKind, Group, Line, Object, Picture,
    PlaceholderKind, Shape, Table, Transform,
};
use crate::render::paint::{reference_color, shape_fill_and_line};
use crate::xml::escape_attribute;

pub struct ShapeWriter<'a, 'b> {
    pub chain: &'a Chain<'a>,
    pub colors: &'a ColorContext<'a>,
    pub styles: &'b mut StyleRegistry,
    /// media id to its path in the package
    pub media: &'a HashMap<String, String>,
    /// things written approximately, in plain words
    pub notes: &'b mut Vec<String>,
}

impl ShapeWriter<'_, '_> {
    pub fn objects(&mut self, objects: &[Object]) -> String {
        objects
            .iter()
            .map(|object| self.object(object, None))
            .collect()
    }

    /// one object; `group` maps a group's child space onto the page.
    fn object(&mut self, object: &Object, group: Option<&GroupMap>) -> String {
        match object {
            Object::Shape(shape) => self.shape(object, shape, group),
            Object::Picture(picture) => self.picture(object, picture, group),
            Object::Group(group_object) => self.group(group_object, group),
            Object::Connector(connector) => self.connector(connector, group),
            Object::Frame(frame) => self.frame(object, frame, group),
            Object::Preserved { .. } => {
                self.notes.push(
                    "an object kept from the pptx file (ink or a newer kind) is not in the odp"
                        .into(),
                );
                String::new()
            }
        }
    }

    fn placed(&self, object: &Object, group: Option<&GroupMap>) -> Option<Transform> {
        let transform = effective_transform(self.chain, object)?;
        Some(match group {
            Some(map) => map.apply(&transform),
            None => transform,
        })
    }

    fn shape(&mut self, object: &Object, shape: &Shape, group: Option<&GroupMap>) -> String {
        let Some(transform) = self.placed(object, group) else {
            return String::new();
        };
        let graphic = self.graphic_properties(object, shape);
        let text = shape
            .text
            .as_ref()
            .map(|body| {
                let mut writer = TextWriter {
                    chain: self.chain,
                    colors: self.colors,
                    styles: self.styles,
                };
                writer.paragraphs(object, body)
            })
            .unwrap_or_default();
        let name = escape_attribute(&shape.name);
        let position = position(&transform);
        if let Some(placeholder) = &shape.placeholder {
            let style = self.styles.style("presentation", None, &graphic);
            let empty = if text.is_empty() {
                " presentation:placeholder=\"true\""
            } else {
                ""
            };
            return format!(
                "<draw:frame draw:name=\"{name}\" presentation:style-name=\"{style}\" draw:layer=\"layout\"{position} presentation:class=\"{}\"{empty}><draw:text-box>{text}</draw:text-box></draw:frame>",
                presentation_class(placeholder.kind)
            );
        }
        let style = self.styles.style("graphic", None, &graphic);
        if shape.text_box {
            return format!("<draw:frame draw:name=\"{name}\" draw:style-name=\"{style}\" draw:layer=\"layout\"{position}><draw:text-box>{text}</draw:text-box></draw:frame>");
        }
        format!(
            "<draw:custom-shape draw:name=\"{name}\" draw:style-name=\"{style}\" draw:layer=\"layout\"{position}>{text}{}</draw:custom-shape>",
            enhanced_geometry(shape.geometry.as_ref(), &transform)
        )
    }

    /// `style:graphic-properties` with the shape's effective fill, line
    /// and text area, plus the text defaults of its style reference.
    fn graphic_properties(&mut self, object: &Object, shape: &Shape) -> String {
        let mut fill = shape.fill.clone();
        let mut line = shape.line.clone();
        for parent in ancestors(self.chain, object) {
            if let Object::Shape(parent) = parent {
                fill = fill.or_else(|| parent.fill.clone());
                line = line.or_else(|| parent.line.clone());
            }
        }
        let (fill, line) = shape_fill_and_line(
            fill.as_ref(),
            line.as_ref(),
            shape.style.as_ref(),
            self.chain.theme,
        );
        let mut colors = ColorContext {
            scheme: self.colors.scheme,
            map: self.colors.map,
            placeholder: None,
        };
        colors.placeholder = fill
            .as_ref()
            .and_then(|(_, r)| r.as_ref())
            .and_then(|r| reference_color(r, self.colors));
        let fill_xml = self.fill_attributes(fill.as_ref().map(|(f, _)| f), &colors);
        colors.placeholder = line
            .as_ref()
            .and_then(|(_, r)| r.as_ref())
            .and_then(|r| reference_color(r, self.colors));
        let line_xml = line_attributes(line.as_ref().map(|(l, _)| l), &colors);
        let body = shape
            .text
            .as_ref()
            .map(|t| crate::inherit::effective_body(self.chain, object, t))
            .unwrap_or_default();
        let inset = |value: Option<i64>, default: i64| write_length(value.unwrap_or(default));
        let anchor = match body.anchor {
            Some(crate::model::Anchor::Middle) => "middle",
            Some(crate::model::Anchor::Bottom) => "bottom",
            _ => "top",
        };
        format!(
            "<style:graphic-properties{fill_xml}{line_xml} draw:auto-grow-height=\"false\" draw:textarea-vertical-align=\"{anchor}\" fo:padding-left=\"{}\" fo:padding-top=\"{}\" fo:padding-right=\"{}\" fo:padding-bottom=\"{}\" fo:wrap-option=\"{}\"/>",
            inset(body.inset_left, 91_440),
            inset(body.inset_top, 45_720),
            inset(body.inset_right, 91_440),
            inset(body.inset_bottom, 45_720),
            if body.wrap == Some(false) { "no-wrap" } else { "wrap" }
        )
    }

    fn fill_attributes(&mut self, fill: Option<&Fill>, colors: &ColorContext<'_>) -> String {
        match fill {
            Some(Fill::Solid { color }) => {
                let rgba = resolve(color, colors);
                let opacity = if rgba.alpha < 0.999 {
                    format!(" draw:opacity=\"{:.0}%\"", rgba.alpha * 100.0)
                } else {
                    String::new()
                };
                format!(
                    " draw:fill=\"solid\" draw:fill-color=\"#{}\"{opacity}",
                    rgba.to_hex()
                )
            }
            Some(Fill::Gradient(gradient)) if gradient.stops.len() >= 2 => {
                let first = resolve(&gradient.stops[0].color, colors).to_hex();
                let last =
                    resolve(&gradient.stops[gradient.stops.len() - 1].color, colors).to_hex();
                // drawingml turns clockwise from left to right, odf counter-clockwise from top to bottom
                let angle =
                    (90.0 - f64::from(gradient.angle.unwrap_or(0)) / 60_000.0).rem_euclid(360.0);
                let name = self
                    .styles
                    .gradient(&first, &last, angle, gradient.path.is_some());
                format!(" draw:fill=\"gradient\" draw:fill-gradient-name=\"{name}\"")
            }
            Some(Fill::Pattern { foreground, .. }) => {
                let hex = foreground
                    .as_ref()
                    .map_or("000000".into(), |c| resolve(c, colors).to_hex());
                format!(" draw:fill=\"solid\" draw:fill-color=\"#{hex}\" draw:opacity=\"50%\"")
            }
            Some(Fill::Picture { .. }) => {
                self.notes
                    .push("a picture fill is written as no fill in odp".into());
                " draw:fill=\"none\"".into()
            }
            _ => " draw:fill=\"none\"".into(),
        }
    }

    fn picture(&mut self, object: &Object, picture: &Picture, group: Option<&GroupMap>) -> String {
        let Some(transform) = self.placed(object, group) else {
            return String::new();
        };
        let Some(path) = picture.media_id.as_ref().and_then(|id| self.media.get(id)) else {
            return String::new();
        };
        let style = self.styles.style(
            "graphic",
            None,
            "<style:graphic-properties draw:fill=\"none\" draw:stroke=\"none\"/>",
        );
        format!(
            "<draw:frame draw:name=\"{}\" draw:style-name=\"{style}\" draw:layer=\"layout\"{}><draw:image xlink:href=\"{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
            escape_attribute(&picture.name),
            position(&transform),
            escape_attribute(path)
        )
    }

    fn group(&mut self, group: &Group, outer: Option<&GroupMap>) -> String {
        let own = GroupMap::of(&group.transform, outer);
        let children: String = group
            .objects
            .iter()
            .map(|child| self.object(child, Some(&own)))
            .collect();
        format!(
            "<draw:g draw:name=\"{}\">{children}</draw:g>",
            escape_attribute(&group.name)
        )
    }

    fn connector(&mut self, connector: &Connector, group: Option<&GroupMap>) -> String {
        let Some(transform) = connector.transform.as_ref().map(|t| match group {
            Some(map) => map.apply(t),
            None => t.clone(),
        }) else {
            return String::new();
        };
        let (_, line) = shape_fill_and_line(
            None,
            connector.line.as_ref(),
            connector.style.as_ref(),
            self.chain.theme,
        );
        let colors = ColorContext {
            scheme: self.colors.scheme,
            map: self.colors.map,
            placeholder: connector
                .style
                .as_ref()
                .and_then(|s| reference_color(&s.line, self.colors)),
        };
        let properties = format!(
            "<style:graphic-properties draw:fill=\"none\"{}/>",
            line_attributes(line.as_ref().map(|(l, _)| l), &colors)
        );
        let style = self.styles.style("graphic", None, &properties);
        let (x1, x2) = if transform.flip_h {
            (transform.x + transform.width, transform.x)
        } else {
            (transform.x, transform.x + transform.width)
        };
        let (y1, y2) = if transform.flip_v {
            (transform.y + transform.height, transform.y)
        } else {
            (transform.y, transform.y + transform.height)
        };
        format!(
            "<draw:line draw:name=\"{}\" draw:style-name=\"{style}\" draw:layer=\"layout\" svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\"/>",
            escape_attribute(&connector.name),
            write_length(x1),
            write_length(y1),
            write_length(x2),
            write_length(y2)
        )
    }

    fn frame(&mut self, object: &Object, frame: &GraphicFrame, group: Option<&GroupMap>) -> String {
        let Some(transform) = self.placed(object, group) else {
            return String::new();
        };
        match &frame.kind {
            GraphicKind::Table(table) => self.table(frame, table, &transform),
            _ => match frame.fallback_media_id.as_ref().and_then(|id| self.media.get(id)) {
                Some(path) => format!(
                    "<draw:frame draw:name=\"{}\" draw:layer=\"layout\"{}><draw:image xlink:href=\"{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
                    escape_attribute(&frame.name),
                    position(&transform),
                    escape_attribute(path)
                ),
                None => {
                    self.notes.push(format!("\"{}\" (a chart, diagram or embedded object) is not in the odp", frame.name));
                    String::new()
                }
            },
        }
    }

    fn table(&mut self, frame: &GraphicFrame, table: &Table, transform: &Transform) -> String {
        let mut out = format!(
            "<draw:frame draw:name=\"{}\" draw:layer=\"layout\"{}><table:table>",
            escape_attribute(&frame.name),
            position(transform)
        );
        for width in &table.columns {
            let style = self.styles.style(
                "table-column",
                None,
                &format!(
                    "<style:table-column-properties style:column-width=\"{}\"/>",
                    write_length(*width)
                ),
            );
            out.push_str(&format!(
                "<table:table-column table:style-name=\"{style}\"/>"
            ));
        }
        let object = Object::Frame(frame.clone());
        for row in &table.rows {
            let style = self.styles.style(
                "table-row",
                None,
                &format!(
                    "<style:table-row-properties style:row-height=\"{}\"/>",
                    write_length(row.height)
                ),
            );
            out.push_str(&format!("<table:table-row table:style-name=\"{style}\">"));
            for cell in &row.cells {
                out.push_str(&self.cell(&object, cell));
            }
            out.push_str("</table:table-row>");
        }
        out.push_str("</table:table></draw:frame>");
        out
    }

    fn cell(&mut self, object: &Object, cell: &crate::model::TableCell) -> String {
        if cell.h_merge || cell.v_merge {
            return "<table:covered-table-cell/>".into();
        }
        let fill = self.fill_attributes(
            cell.fill.as_ref(),
            &ColorContext {
                scheme: self.colors.scheme,
                map: self.colors.map,
                placeholder: None,
            },
        );
        let style = self.styles.style(
            "table-cell",
            None,
            &format!("<style:graphic-properties{fill}/>"),
        );
        let mut writer = TextWriter {
            chain: self.chain,
            colors: self.colors,
            styles: self.styles,
        };
        let text = writer.paragraphs(object, &cell.text);
        let span = |name: &str, value: u32| {
            if value > 1 {
                format!(" {name}=\"{value}\"")
            } else {
                String::new()
            }
        };
        format!(
            "<table:table-cell table:style-name=\"{style}\"{}{}>{text}</table:table-cell>",
            span("table:number-columns-spanned", cell.grid_span),
            span("table:number-rows-spanned", cell.row_span)
        )
    }
}

fn line_attributes(line: Option<&Line>, colors: &ColorContext<'_>) -> String {
    let Some(line) = line else {
        return " draw:stroke=\"none\"".into();
    };
    let color = match &line.fill {
        Some(Fill::Solid { color }) => resolve(color, colors),
        Some(Fill::None) => return " draw:stroke=\"none\"".into(),
        _ => crate::color::Rgba::BLACK,
    };
    let width = line.width.unwrap_or(9_525);
    format!(
        " draw:stroke=\"solid\" svg:stroke-color=\"#{}\" svg:stroke-width=\"{}\"",
        color.to_hex(),
        write_length(width)
    )
}

/// svg position and size, or a transform for a rotated object: odf
/// rotates counter-clockwise about the corner, so the corner is where the
/// model's clockwise turn about the centre puts it.
fn position(transform: &Transform) -> String {
    let size = format!(
        " svg:width=\"{}\" svg:height=\"{}\"",
        write_length(transform.width),
        write_length(transform.height)
    );
    if transform.rotation == 0 {
        return format!(
            "{size} svg:x=\"{}\" svg:y=\"{}\"",
            write_length(transform.x),
            write_length(transform.y)
        );
    }
    let clockwise = f64::from(transform.rotation) / 60_000.0;
    let (sin, cos) = clockwise.to_radians().sin_cos();
    let (half_width, half_height) = (transform.width as f64 / 2.0, transform.height as f64 / 2.0);
    let centre = (
        transform.x as f64 + half_width,
        transform.y as f64 + half_height,
    );
    let corner_x = centre.0 - half_width * cos + half_height * sin;
    let corner_y = centre.1 - half_width * sin - half_height * cos;
    format!(
        "{size} draw:transform=\"rotate ({:.6}) translate ({} {})\"",
        -clockwise.to_radians(),
        write_length(corner_x.round() as i64),
        write_length(corner_y.round() as i64)
    )
}

fn presentation_class(kind: PlaceholderKind) -> &'static str {
    match kind {
        PlaceholderKind::Title | PlaceholderKind::CenteredTitle => "title",
        PlaceholderKind::Subtitle => "subtitle",
        PlaceholderKind::Body | PlaceholderKind::Object => "outline",
        PlaceholderKind::Picture | PlaceholderKind::ClipArt => "graphic",
        PlaceholderKind::Chart => "chart",
        PlaceholderKind::Table => "table",
        PlaceholderKind::Date => "date-time",
        PlaceholderKind::Footer => "footer",
        PlaceholderKind::Header => "header",
        PlaceholderKind::SlideNumber => "page-number",
        _ => "object",
    }
}

/// drawingml preset names libreoffice knows under its own names; any
/// other preset is written as "ooxml-<name>", the name libreoffice gives
/// presets it imported from pptx, with the handle values as modifiers.
fn enhanced_geometry(geometry: Option<&Geometry>, transform: &Transform) -> String {
    let (kind, modifiers) = match geometry {
        Some(Geometry::Preset { name, adjust }) => match name.as_str() {
            "rect" => ("rectangle".to_string(), String::new()),
            "ellipse" => ("ellipse".to_string(), String::new()),
            _ => {
                let values: Vec<String> = adjust.iter().map(|(_, v)| v.to_string()).collect();
                let modifiers = if values.is_empty() {
                    String::new()
                } else {
                    format!(" draw:modifiers=\"{}\"", values.join(" "))
                };
                (format!("ooxml-{name}"), modifiers)
            }
        },
        _ => ("rectangle".to_string(), String::new()),
    };
    let mirror = |flag: bool, name: &str| {
        if flag {
            format!(" draw:{name}=\"true\"")
        } else {
            String::new()
        }
    };
    format!(
        "<draw:enhanced-geometry svg:viewBox=\"0 0 21600 21600\" draw:type=\"{kind}\"{modifiers}{}{}/>",
        mirror(transform.flip_h, "mirror-horizontal"),
        mirror(transform.flip_v, "mirror-vertical")
    )
}

/// how a group's child space maps onto the page (rotation of the group
/// itself is not carried into odp children yet).
pub struct GroupMap {
    scale_x: f64,
    scale_y: f64,
    offset_x: f64,
    offset_y: f64,
}

impl GroupMap {
    fn of(transform: &Transform, outer: Option<&GroupMap>) -> GroupMap {
        let child_width = transform.child_width.unwrap_or(transform.width).max(1) as f64;
        let child_height = transform.child_height.unwrap_or(transform.height).max(1) as f64;
        let own = GroupMap {
            scale_x: transform.width as f64 / child_width,
            scale_y: transform.height as f64 / child_height,
            offset_x: transform.x as f64
                - transform.child_x.unwrap_or(0) as f64 * transform.width as f64 / child_width,
            offset_y: transform.y as f64
                - transform.child_y.unwrap_or(0) as f64 * transform.height as f64 / child_height,
        };
        match outer {
            Some(outer) => GroupMap {
                scale_x: own.scale_x * outer.scale_x,
                scale_y: own.scale_y * outer.scale_y,
                offset_x: own.offset_x * outer.scale_x + outer.offset_x,
                offset_y: own.offset_y * outer.scale_y + outer.offset_y,
            },
            None => own,
        }
    }

    fn apply(&self, transform: &Transform) -> Transform {
        Transform {
            x: (transform.x as f64 * self.scale_x + self.offset_x).round() as i64,
            y: (transform.y as f64 * self.scale_y + self.offset_y).round() as i64,
            width: (transform.width as f64 * self.scale_x).round() as i64,
            height: (transform.height as f64 * self.scale_y).round() as i64,
            child_x: None,
            child_y: None,
            child_width: None,
            child_height: None,
            ..transform.clone()
        }
    }
}
