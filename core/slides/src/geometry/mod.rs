//! outlines of shapes: preset and custom geometry turned into paths of
//! straight lines and cubic curves in the shape's own space (0,0 at its
//! top left, its width and height in whatever unit the caller uses).

mod guides;
mod names;
mod presets;

use crate::model::{Geometry, PathCommand, PathPoint, ShapePath};
use guides::to_radians;
pub use guides::Guides;
pub use names::is_preset_name;

/// one step of an outline, in absolute coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Move(f64, f64),
    Line(f64, f64),
    Cubic(f64, f64, f64, f64, f64, f64),
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutlinePath {
    pub segments: Vec<Segment>,
    pub fill: bool,
    pub stroke: bool,
}

/// a shape's outline and the rectangle its text goes in.
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    pub paths: Vec<OutlinePath>,
    /// left, top, right, bottom
    pub text_rect: [f64; 4],
    /// false when the preset is not in the table and its box is drawn
    pub exact: bool,
}

pub fn is_known_preset(name: &str) -> bool {
    presets::definition(name).is_some()
}

/// the names of a preset's handles in order ("adj1", "adj2"; "adj",
/// "hf", "vf" for a star), for presets in the table.
pub fn handle_names(name: &str) -> Option<Vec<&'static str>> {
    let definition = presets::definition(name)?;
    Some(
        definition
            .lines()
            .filter_map(|line| line.trim().strip_prefix("av "))
            .filter_map(|rest| rest.split_whitespace().next())
            .collect(),
    )
}

/// the outline of `geometry` (a rectangle when there is none) for a box
/// `width` by `height`.
pub fn outline(geometry: Option<&Geometry>, width: f64, height: f64) -> Outline {
    match geometry {
        Some(Geometry::Preset { name, adjust }) => preset_outline(name, adjust, width, height),
        Some(Geometry::Custom {
            adjust,
            guides,
            paths,
            ..
        }) => custom_outline(adjust, guides, paths, width, height),
        None => preset_outline("rect", &[], width, height),
    }
}

fn preset_outline(name: &str, adjust: &[(String, i64)], width: f64, height: f64) -> Outline {
    let (definition, exact) = match presets::definition(name) {
        Some(definition) => (definition, true),
        None => (presets::definition("rect").unwrap_or(""), false),
    };
    let mut outline = run_definition(definition, adjust, width, height);
    outline.exact = exact;
    outline
}

/// runs a preset definition (see `presets` for the form).
fn run_definition(definition: &str, adjust: &[(String, i64)], width: f64, height: f64) -> Outline {
    let mut guides = Guides::new(width, height);
    let mut builder = PathBuilder::default();
    let mut text_rect = [0.0, 0.0, width, height];
    for line in definition
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (keyword, rest) = line.split_once(' ').unwrap_or((line, ""));
        let words: Vec<&str> = rest.split_whitespace().collect();
        match keyword {
            "av" => {
                let own = adjust.iter().find(|(name, _)| name == words[0]);
                let value = own
                    .map(|(_, value)| *value as f64)
                    .unwrap_or_else(|| guides.value(words[1]));
                guides.set(words[0], value);
            }
            "gd" => guides.define(words[0], &words[1..].join(" ")),
            "path" => builder.start(!rest.contains("nofill"), !rest.contains("nostroke")),
            "text" => text_rect = [0, 1, 2, 3].map(|i| guides.value(words[i])),
            _ => {
                let values: Vec<f64> = words.iter().map(|w| guides.value(w)).collect();
                builder.step(keyword, &values);
            }
        }
    }
    Outline {
        paths: builder.finish(),
        text_rect,
        exact: true,
    }
}

fn custom_outline(
    adjust: &[(String, String)],
    guide_list: &[(String, String)],
    paths: &[ShapePath],
    width: f64,
    height: f64,
) -> Outline {
    let mut guides = Guides::new(width, height);
    for (name, formula) in adjust.iter().chain(guide_list) {
        guides.define(name, formula);
    }
    let mut builder = PathBuilder::default();
    for path in paths {
        // a path has its own coordinate space, stretched to the shape
        let scale_x = path
            .width
            .filter(|w| *w > 0)
            .map_or(1.0, |w| width / w as f64);
        let scale_y = path
            .height
            .filter(|h| *h > 0)
            .map_or(1.0, |h| height / h as f64);
        builder.start(path.fill.as_deref() != Some("none"), path.stroke);
        for command in &path.commands {
            custom_step(&mut builder, &guides, command, scale_x, scale_y);
        }
    }
    Outline {
        paths: builder.finish(),
        text_rect: [0.0, 0.0, width, height],
        exact: true,
    }
}

fn custom_step(
    builder: &mut PathBuilder,
    guides: &Guides,
    command: &PathCommand,
    scale_x: f64,
    scale_y: f64,
) {
    let point = |p: &PathPoint| [guides.value(&p.x) * scale_x, guides.value(&p.y) * scale_y];
    match command {
        PathCommand::Move { to } => builder.step("M", &point(to)),
        PathCommand::Line { to } => builder.step("L", &point(to)),
        PathCommand::Quadratic { control, to } => {
            builder.step("Q", &[point(control), point(to)].concat())
        }
        PathCommand::Cubic {
            control1,
            control2,
            to,
        } => builder.step("C", &[point(control1), point(control2), point(to)].concat()),
        PathCommand::Arc {
            width_radius,
            height_radius,
            start,
            swing,
        } => {
            let values = [
                guides.value(width_radius) * scale_x,
                guides.value(height_radius) * scale_y,
                guides.value(start),
                guides.value(swing),
            ];
            builder.step("A", &values);
        }
        PathCommand::Close => builder.step("Z", &[]),
    }
}

/// collects segments, keeping the current point for arcs and curves.
#[derive(Default)]
struct PathBuilder {
    paths: Vec<OutlinePath>,
    current: (f64, f64),
    start: (f64, f64),
}

impl PathBuilder {
    fn start(&mut self, fill: bool, stroke: bool) {
        self.paths.push(OutlinePath {
            segments: Vec::new(),
            fill,
            stroke,
        });
    }

    fn push(&mut self, segment: Segment) {
        if self.paths.is_empty() {
            self.start(true, true);
        }
        if let Some(path) = self.paths.last_mut() {
            path.segments.push(segment);
        }
    }

    fn step(&mut self, keyword: &str, values: &[f64]) {
        let value = |index: usize| values.get(index).copied().unwrap_or(0.0);
        match keyword {
            "M" => {
                self.current = (value(0), value(1));
                self.start = self.current;
                self.push(Segment::Move(value(0), value(1)));
            }
            "L" => {
                self.current = (value(0), value(1));
                self.push(Segment::Line(value(0), value(1)));
            }
            "C" => {
                self.current = (value(4), value(5));
                self.push(Segment::Cubic(
                    value(0),
                    value(1),
                    value(2),
                    value(3),
                    value(4),
                    value(5),
                ));
            }
            "Q" => self.quadratic(value(0), value(1), value(2), value(3)),
            "A" => self.arc(value(0), value(1), value(2), value(3)),
            "Z" => {
                self.current = self.start;
                self.push(Segment::Close);
            }
            _ => {}
        }
    }

    /// a quadratic curve is the cubic with control points two thirds of
    /// the way to its control point
    fn quadratic(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        let (x0, y0) = self.current;
        let first = (x0 + 2.0 / 3.0 * (cx - x0), y0 + 2.0 / 3.0 * (cy - y0));
        let second = (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
        self.current = (x, y);
        self.push(Segment::Cubic(first.0, first.1, second.0, second.1, x, y));
    }

    /// drawingml's arcTo: an elliptical arc from the current point. the
    /// angles are visual angles (where a ray from the centre at that
    /// angle meets the ellipse), turned into the ellipse's parametric
    /// angles before drawing; for circles the two are the same.
    fn arc(&mut self, width_radius: f64, height_radius: f64, start: f64, swing: f64) {
        if width_radius <= 0.0 || height_radius <= 0.0 {
            return;
        }
        let parametric =
            |visual: f64| (width_radius * visual.sin()).atan2(height_radius * visual.cos());
        let start_visual = to_radians(start);
        let end_visual = to_radians(start + swing);
        let from = parametric(start_visual);
        let mut sweep = parametric(end_visual) - from;
        let wanted = to_radians(swing);
        // keep the direction and the number of turns the swing asks for
        while wanted > 0.0 && sweep < wanted - std::f64::consts::PI {
            sweep += std::f64::consts::TAU;
        }
        while wanted < 0.0 && sweep > wanted + std::f64::consts::PI {
            sweep -= std::f64::consts::TAU;
        }
        let (x0, y0) = self.current;
        let centre = (
            x0 - width_radius * from.cos(),
            y0 - height_radius * from.sin(),
        );
        self.arc_segments(centre, width_radius, height_radius, from, sweep);
    }

    /// cubic curves of at most a quarter turn each (the usual 4/3 tan
    /// rule keeps the error under 0.03 % of the radius)
    fn arc_segments(&mut self, centre: (f64, f64), rx: f64, ry: f64, from: f64, sweep: f64) {
        let pieces = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
        let step = sweep / pieces as f64;
        let handle = 4.0 / 3.0 * (step / 4.0).tan();
        let mut angle = from;
        for _ in 0..pieces {
            let next = angle + step;
            let (cos0, sin0, cos1, sin1) = (angle.cos(), angle.sin(), next.cos(), next.sin());
            let x1 = centre.0 + rx * (cos0 - handle * sin0);
            let y1 = centre.1 + ry * (sin0 + handle * cos0);
            let x2 = centre.0 + rx * (cos1 + handle * sin1);
            let y2 = centre.1 + ry * (sin1 - handle * cos1);
            let x = centre.0 + rx * cos1;
            let y = centre.1 + ry * sin1;
            self.push(Segment::Cubic(x1, y1, x2, y2, x, y));
            self.current = (x, y);
            angle = next;
        }
    }

    fn finish(self) -> Vec<OutlinePath> {
        self.paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end_points(outline: &Outline) -> Vec<(f64, f64)> {
        outline.paths[0]
            .segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Move(x, y) | Segment::Line(x, y) | Segment::Cubic(_, _, _, _, x, y) => {
                    Some((*x, *y))
                }
                Segment::Close => None,
            })
            .collect()
    }

    #[test]
    fn rectangles_are_their_box() {
        let outline = outline(None, 100.0, 50.0);
        assert_eq!(
            end_points(&outline),
            vec![(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)]
        );
    }

    #[test]
    fn rounded_rectangle_corners_follow_the_handle() {
        // adj 35000 on a 200 by 120 box: radius 0.35 of the short side
        let geometry = Geometry::Preset {
            name: "roundRect".into(),
            adjust: vec![("adj".into(), 35000)],
        };
        let outline = outline(Some(&geometry), 200.0, 120.0);
        let points = end_points(&outline);
        assert_eq!(points[0], (0.0, 42.0));
        let top_left_end = points[1];
        assert!((top_left_end.0 - 42.0).abs() < 1e-9 && top_left_end.1.abs() < 1e-9);
    }

    #[test]
    fn right_arrow_handles_set_shaft_and_head() {
        // adj1 30000: the shaft is 30 % of the height; adj2 70000: the head
        // is 70 % of the short side long
        let geometry = Geometry::Preset {
            name: "rightArrow".into(),
            adjust: vec![("adj1".into(), 30000), ("adj2".into(), 70000)],
        };
        let points = end_points(&outline(Some(&geometry), 220.0, 120.0));
        assert_eq!(points[0], (0.0, 42.0));
        assert_eq!(points[1], (220.0 - 84.0, 42.0));
        assert_eq!(points[3], (220.0, 60.0));
    }

    #[test]
    fn ellipses_pass_through_their_four_sides() {
        let points = end_points(&outline(Some(&Geometry::preset("ellipse")), 200.0, 100.0));
        let near =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
        assert!(near(points[0], (0.0, 50.0)));
        assert!(near(points[1], (100.0, 0.0)));
        assert!(near(points[2], (200.0, 50.0)));
        assert!(near(points[3], (100.0, 100.0)));
    }

    #[test]
    fn unknown_presets_draw_their_box_and_say_so() {
        let outline = outline(Some(&Geometry::preset("cloudCallout")), 10.0, 10.0);
        assert!(!outline.exact);
        assert_eq!(outline.paths[0].segments.len(), 5);
    }

    #[test]
    fn custom_paths_scale_to_the_shape() {
        let geometry = Geometry::Custom {
            adjust: Vec::new(),
            guides: Vec::new(),
            paths: vec![ShapePath {
                width: Some(10),
                height: Some(10),
                fill: None,
                stroke: true,
                extrusion_ok: None,
                commands: vec![
                    PathCommand::Move {
                        to: PathPoint::new(0, 0),
                    },
                    PathCommand::Line {
                        to: PathPoint::new(10, 5),
                    },
                ],
            }],
            extra: Vec::new(),
        };
        let points = end_points(&outline(Some(&geometry), 100.0, 40.0));
        assert_eq!(points, vec![(0.0, 0.0), (100.0, 20.0)]);
    }
}
