//! the outline of an opendocument custom shape: a drawingml preset when
//! the shape names one (libreoffice keeps pptx presets as "ooxml-*"), or
//! a custom path from `draw:enhanced-path` when it uses plain numbers.

use crate::geometry::{handle_names, is_preset_name};
use crate::model::{Geometry, PathCommand, PathPoint, ShapePath};
use crate::xml::Element;

/// odf names for shapes that drawingml has as presets.
fn preset_for(kind: &str) -> Option<&str> {
    if let Some(name) = kind.strip_prefix("ooxml-") {
        // "ooxml-non-primitive" and the like are not presets
        return is_preset_name(name).then_some(name);
    }
    Some(match kind {
        "rectangle" => "rect",
        "round-rectangle" => "roundRect",
        "ellipse" | "circle" => "ellipse",
        "isosceles-triangle" => "triangle",
        "right-triangle" => "rtTriangle",
        "diamond" => "diamond",
        "parallelogram" => "parallelogram",
        "trapezoid" => "trapezoid",
        "pentagon" => "pentagon",
        "hexagon" => "hexagon",
        "octagon" => "octagon",
        "star5" => "star5",
        "right-arrow" => "rightArrow",
        "left-arrow" => "leftArrow",
        "up-arrow" => "upArrow",
        "down-arrow" => "downArrow",
        "chevron" => "chevron",
        "pentagon-right" => "homePlate",
        "cross" => "plus",
        "ring" => "donut",
        _ => return None,
    })
}

/// a custom shape's outline and its flips. for shapes libreoffice
/// imported from pptx ("ooxml-*") the modifiers are drawingml's own
/// handle values in order, so they map one to one.
pub fn read_geometry(element: &Element) -> (Option<Geometry>, bool, bool) {
    let Some(geometry) = element.child("draw:enhanced-geometry") else {
        let preset = if element.name == "draw:ellipse" {
            "ellipse"
        } else {
            "rect"
        };
        return (Some(Geometry::preset(preset)), false, false);
    };
    let flip_h = geometry.attr("draw:mirror-horizontal") == Some("true");
    let flip_v = geometry.attr("draw:mirror-vertical") == Some("true");
    let kind = geometry.attr("draw:type").unwrap_or("rectangle");
    let outline = match preset_for(kind) {
        Some(name) => preset_with_handles(name, kind, geometry),
        None => custom_path(geometry).unwrap_or_else(|| Geometry::preset("rect")),
    };
    (Some(outline), flip_h, flip_v)
}

fn preset_with_handles(name: &str, kind: &str, geometry: &Element) -> Geometry {
    let mut adjust = Vec::new();
    // handle values only map onto presets whose handle names are known;
    // a wrong name makes powerpoint refuse the file
    if let (true, Some(names)) = (kind.starts_with("ooxml-"), handle_names(name)) {
        let values = geometry
            .attr("draw:modifiers")
            .unwrap_or("")
            .split_whitespace()
            .filter_map(|v| v.parse::<f64>().ok().map(|v| v.round() as i64));
        adjust = names.into_iter().map(str::to_string).zip(values).collect();
    }
    Geometry::Preset {
        name: name.to_string(),
        adjust,
    }
}

/// a free outline from `draw:enhanced-path` in its `svg:viewBox` space.
/// paths with formulas ("?f0") or handles ("$0") are not evaluated here,
/// so such a shape keeps its box.
fn custom_path(geometry: &Element) -> Option<Geometry> {
    let view: Vec<i64> = geometry
        .attr("svg:viewBox")?
        .split_whitespace()
        .filter_map(|v| v.parse::<f64>().ok().map(|v| v.round() as i64))
        .collect();
    let path = geometry.attr("draw:enhanced-path")?;
    if path.contains('?') || path.contains('$') || view.len() != 4 {
        return None;
    }
    let tokens: Vec<&str> = path
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .collect();
    let mut reader = PathReader::new(&view);
    reader.read(&tokens)?;
    let paths: Vec<ShapePath> = reader
        .paths
        .into_iter()
        .filter(|p| !p.commands.is_empty())
        .collect();
    (!paths.is_empty()).then_some(Geometry::Custom {
        adjust: Vec::new(),
        guides: Vec::new(),
        paths,
        extra: Vec::new(),
    })
}

/// reads enhanced-path commands (m l c q z, and n f s which end a path,
/// leave it unfilled or unstroked) into drawingml paths.
struct PathReader {
    origin: (i64, i64),
    size: (i64, i64),
    paths: Vec<ShapePath>,
}

impl PathReader {
    fn new(view: &[i64]) -> PathReader {
        PathReader {
            origin: (view[0], view[1]),
            size: (view[2], view[3]),
            paths: vec![Self::empty_path((view[2], view[3]))],
        }
    }

    fn empty_path(size: (i64, i64)) -> ShapePath {
        ShapePath {
            width: Some(size.0),
            height: Some(size.1),
            fill: None,
            stroke: true,
            extrusion_ok: None,
            commands: Vec::new(),
        }
    }

    fn read(&mut self, tokens: &[&str]) -> Option<()> {
        let mut index = 0;
        let mut command = "M";
        while index < tokens.len() {
            let token = tokens[index];
            if token.chars().all(|c| c.is_ascii_alphabetic()) {
                command = token;
                index += 1;
                self.flag(command)?;
                continue;
            }
            index += self.step(command, &tokens[index..])?;
            // after a move, more coordinate pairs are lines (as in svg)
            if command == "M" {
                command = "L";
            }
        }
        Some(())
    }

    /// commands that take no coordinates.
    fn flag(&mut self, command: &str) -> Option<()> {
        let path = self.paths.last_mut()?;
        match command {
            "Z" => path.commands.push(PathCommand::Close),
            "N" => self.paths.push(Self::empty_path(self.size)),
            "F" => path.fill = Some("none".into()),
            "S" => path.stroke = false,
            _ => {}
        }
        Some(())
    }

    /// one drawing command; returns how many numbers it used.
    fn step(&mut self, command: &str, numbers: &[&str]) -> Option<usize> {
        let point = |at: usize| -> Option<PathPoint> {
            let x: f64 = numbers.get(at)?.parse().ok()?;
            let y: f64 = numbers.get(at + 1)?.parse().ok()?;
            let local_x = (x - self.origin.0 as f64).round() as i64;
            let local_y = (y - self.origin.1 as f64).round() as i64;
            Some(PathPoint::new(local_x, local_y))
        };
        let (step, used) = match command {
            "M" => (PathCommand::Move { to: point(0)? }, 2),
            "L" => (PathCommand::Line { to: point(0)? }, 2),
            "C" => (
                PathCommand::Cubic {
                    control1: point(0)?,
                    control2: point(2)?,
                    to: point(4)?,
                },
                6,
            ),
            "Q" => (
                PathCommand::Quadratic {
                    control: point(0)?,
                    to: point(2)?,
                },
                4,
            ),
            _ => return None,
        };
        self.paths.last_mut()?.commands.push(step);
        Some(used)
    }
}
