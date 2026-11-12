//! the three ways opendocument writes an outline, to and from the model's
//! paths: svg path data (`svg:d` on `draw:path`), point lists
//! (`draw:points` on polygons and polylines) and the enhanced path of a
//! custom shape (`draw:enhanced-path`), which uses its own letters.

use crate::model::{PathCommand, ShapePath};

/// splits path data into letters and numbers ("M10-5.5e2" gives M, 10,
/// -550). commas and spaces only separate.
fn tokens(data: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut number = String::new();
    let mut previous = ' ';
    for c in data.chars() {
        let exponent_sign = (c == '-' || c == '+') && matches!(previous, 'e' | 'E');
        let starts_number = (c == '-' || c == '+') && !number.is_empty() && !exponent_sign;
        let decimal_again = c == '.' && number.contains('.') && !number.contains(['e', 'E']);
        if starts_number || decimal_again {
            out.push(std::mem::take(&mut number));
        }
        if c.is_ascii_digit()
            || c == '.'
            || c == '-'
            || c == '+'
            || (matches!(c, 'e' | 'E') && !number.is_empty())
        {
            number.push(c);
        } else {
            if !number.is_empty() {
                out.push(std::mem::take(&mut number));
            }
            if c.is_ascii_alphabetic() || c == '?' || c == '$' {
                out.push(c.to_string());
            }
        }
        previous = c;
    }
    if !number.is_empty() {
        out.push(number);
    }
    out
}

/// svg path data in a `width` by `height` view box. arcs become cubic
/// curves; `None` when the data cannot be read.
pub fn from_svg(data: &str, width: f64, height: f64) -> Option<ShapePath> {
    let tokens = tokens(data);
    let mut commands = Vec::new();
    let mut at = 0;
    let mut verb = 'M';
    let (mut x, mut y) = (0.0_f64, 0.0_f64);
    let (mut start_x, mut start_y) = (0.0, 0.0);
    // the last control point, which s and t mirror
    let mut last_control: Option<(f64, f64)> = None;
    let number = |at: &mut usize| -> Option<f64> {
        let value = tokens.get(*at)?.parse().ok();
        *at += 1;
        value
    };
    while at < tokens.len() {
        if let Some(letter) = tokens[at]
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
        {
            verb = letter;
            at += 1;
            if matches!(verb, 'Z' | 'z') {
                commands.push(PathCommand::Close);
                x = start_x;
                y = start_y;
                last_control = None;
                continue;
            }
        }
        let relative = verb.is_ascii_lowercase();
        let (base_x, base_y) = if relative { (x, y) } else { (0.0, 0.0) };
        match verb.to_ascii_uppercase() {
            'M' => {
                x = base_x + number(&mut at)?;
                y = base_y + number(&mut at)?;
                commands.push(PathCommand::Move { x, y });
                start_x = x;
                start_y = y;
                // further pairs after a move are lines
                verb = if relative { 'l' } else { 'L' };
                last_control = None;
            }
            'L' => {
                x = base_x + number(&mut at)?;
                y = base_y + number(&mut at)?;
                commands.push(PathCommand::Line { x, y });
                last_control = None;
            }
            'H' => {
                x = base_x + number(&mut at)?;
                commands.push(PathCommand::Line { x, y });
                last_control = None;
            }
            'V' => {
                y = base_y + number(&mut at)?;
                commands.push(PathCommand::Line { x, y });
                last_control = None;
            }
            'C' | 'S' => {
                let (x1, y1) = if verb.eq_ignore_ascii_case(&'C') {
                    (base_x + number(&mut at)?, base_y + number(&mut at)?)
                } else {
                    let (cx, cy) = last_control.unwrap_or((x, y));
                    (2.0 * x - cx, 2.0 * y - cy)
                };
                let (x2, y2) = (base_x + number(&mut at)?, base_y + number(&mut at)?);
                let (ex, ey) = (base_x + number(&mut at)?, base_y + number(&mut at)?);
                commands.push(PathCommand::Cubic {
                    x1,
                    y1,
                    x2,
                    y2,
                    x: ex,
                    y: ey,
                });
                last_control = Some((x2, y2));
                x = ex;
                y = ey;
            }
            'Q' | 'T' => {
                let (x1, y1) = if verb.eq_ignore_ascii_case(&'Q') {
                    (base_x + number(&mut at)?, base_y + number(&mut at)?)
                } else {
                    let (cx, cy) = last_control.unwrap_or((x, y));
                    (2.0 * x - cx, 2.0 * y - cy)
                };
                let (ex, ey) = (base_x + number(&mut at)?, base_y + number(&mut at)?);
                commands.push(PathCommand::Quadratic {
                    x1,
                    y1,
                    x: ex,
                    y: ey,
                });
                last_control = Some((x1, y1));
                x = ex;
                y = ey;
            }
            'A' => {
                let arc = SvgArc {
                    rx: number(&mut at)?,
                    ry: number(&mut at)?,
                    rotation: number(&mut at)?,
                    large: number(&mut at)? != 0.0,
                    sweep: number(&mut at)? != 0.0,
                    to: (base_x + number(&mut at)?, base_y + number(&mut at)?),
                };
                arc_to_cubics((x, y), &arc, &mut commands);
                (x, y) = arc.to;
                last_control = None;
            }
            _ => return None,
        }
    }
    (!commands.is_empty()).then_some(ShapePath {
        width,
        height,
        fill: true,
        stroke: true,
        commands,
    })
}

struct SvgArc {
    rx: f64,
    ry: f64,
    rotation: f64,
    large: bool,
    sweep: bool,
    to: (f64, f64),
}

/// an svg elliptical arc as cubic curves, by the centre construction of
/// the svg specification (appendix f.6.5).
fn arc_to_cubics(from: (f64, f64), arc: &SvgArc, out: &mut Vec<PathCommand>) {
    let (mut rx, mut ry) = (arc.rx.abs(), arc.ry.abs());
    if rx == 0.0 || ry == 0.0 || from == arc.to {
        out.push(PathCommand::Line {
            x: arc.to.0,
            y: arc.to.1,
        });
        return;
    }
    let phi = arc.rotation.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (from.0 - arc.to.0) / 2.0;
    let dy = (from.1 - arc.to.1) / 2.0;
    let x1 = cos_phi * dx + sin_phi * dy;
    let y1 = -sin_phi * dx + cos_phi * dy;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let numerator = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut factor = (numerator / denominator).sqrt();
    if arc.large == arc.sweep {
        factor = -factor;
    }
    let cx1 = factor * rx * y1 / ry;
    let cy1 = -factor * ry * x1 / rx;
    let centre = (
        cos_phi * cx1 - sin_phi * cy1 + (from.0 + arc.to.0) / 2.0,
        sin_phi * cx1 + cos_phi * cy1 + (from.1 + arc.to.1) / 2.0,
    );
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let start = angle(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut sweep = angle(
        (x1 - cx1) / rx,
        (y1 - cy1) / ry,
        (-x1 - cx1) / rx,
        (-y1 - cy1) / ry,
    );
    if !arc.sweep && sweep > 0.0 {
        sweep -= std::f64::consts::TAU;
    } else if arc.sweep && sweep < 0.0 {
        sweep += std::f64::consts::TAU;
    }
    let pieces = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / pieces as f64;
    let point = |t: f64| {
        let (s, c) = t.sin_cos();
        (
            centre.0 + rx * c * cos_phi - ry * s * sin_phi,
            centre.1 + rx * c * sin_phi + ry * s * cos_phi,
        )
    };
    let derivative = |t: f64| {
        let (s, c) = t.sin_cos();
        (
            -rx * s * cos_phi - ry * c * sin_phi,
            -rx * s * sin_phi + ry * c * cos_phi,
        )
    };
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut t = start;
    for _ in 0..pieces {
        let (p0, d0) = (point(t), derivative(t));
        let (p1, d1) = (point(t + step), derivative(t + step));
        out.push(PathCommand::Cubic {
            x1: p0.0 + k * d0.0,
            y1: p0.1 + k * d0.1,
            x2: p1.0 - k * d1.0,
            y2: p1.1 - k * d1.1,
            x: p1.0,
            y: p1.1,
        });
        t += step;
    }
}

/// a polygon's or polyline's `draw:points` ("0,0 100,0 100,100").
pub fn from_points(points: &str, width: f64, height: f64, closed: bool) -> Option<ShapePath> {
    let numbers: Vec<f64> = tokens(points)
        .iter()
        .map(|t| t.parse().ok())
        .collect::<Option<_>>()?;
    let mut commands = Vec::new();
    for (index, pair) in numbers.chunks(2).enumerate() {
        let [x, y] = pair else { return None };
        commands.push(if index == 0 {
            PathCommand::Move { x: *x, y: *y }
        } else {
            PathCommand::Line { x: *x, y: *y }
        });
    }
    if closed {
        commands.push(PathCommand::Close);
    }
    (commands.len() > 1).then_some(ShapePath {
        width,
        height,
        fill: closed,
        stroke: true,
        commands,
    })
}

/// a custom shape's enhanced path, when it uses only plain numbers (paths
/// that refer to formulas are read through their preset type instead).
pub fn from_enhanced(data: &str, width: f64, height: f64) -> Option<Vec<ShapePath>> {
    let tokens = tokens(data);
    let mut paths = Vec::new();
    let mut current = ShapePath {
        width,
        height,
        fill: true,
        stroke: true,
        commands: Vec::new(),
    };
    let mut at = 0;
    let mut verb = 'M';
    let number = |at: &mut usize| -> Option<f64> {
        let value = tokens.get(*at)?.parse().ok();
        *at += 1;
        value
    };
    while at < tokens.len() {
        if let Some(letter) = tokens[at]
            .chars()
            .next()
            .filter(|c| !c.is_ascii_digit() && *c != '-' && *c != '.' && *c != '+')
        {
            at += 1;
            match letter {
                'Z' => current.commands.push(PathCommand::Close),
                'N' => {
                    if !current.commands.is_empty() {
                        paths.push(std::mem::replace(
                            &mut current,
                            ShapePath {
                                width,
                                height,
                                fill: true,
                                stroke: true,
                                commands: Vec::new(),
                            },
                        ));
                    }
                }
                'F' => current.fill = false,
                'S' => current.stroke = false,
                'M' | 'L' | 'C' | 'Q' => verb = letter,
                // formulas, arcs and the like: not plain numbers
                _ => return None,
            }
            continue;
        }
        match verb {
            'M' => {
                let (x, y) = (number(&mut at)?, number(&mut at)?);
                current.commands.push(PathCommand::Move { x, y });
                verb = 'L';
            }
            'L' => {
                let (x, y) = (number(&mut at)?, number(&mut at)?);
                current.commands.push(PathCommand::Line { x, y });
            }
            'C' => {
                let values = [
                    number(&mut at)?,
                    number(&mut at)?,
                    number(&mut at)?,
                    number(&mut at)?,
                    number(&mut at)?,
                    number(&mut at)?,
                ];
                let [x1, y1, x2, y2, x, y] = values;
                current.commands.push(PathCommand::Cubic {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                });
            }
            'Q' => {
                let values = [
                    number(&mut at)?,
                    number(&mut at)?,
                    number(&mut at)?,
                    number(&mut at)?,
                ];
                let [x1, y1, x, y] = values;
                current
                    .commands
                    .push(PathCommand::Quadratic { x1, y1, x, y });
            }
            _ => return None,
        }
    }
    if !current.commands.is_empty() {
        paths.push(current);
    }
    (!paths.is_empty()).then_some(paths)
}

/// whole numbers where they are whole, so paths stay short and exact.
fn number_text(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{}", (value * 1000.0).round() / 1000.0)
    }
}

/// the model's paths as one enhanced path in the first path's view box;
/// later paths with another size are scaled into it.
pub fn to_enhanced(paths: &[ShapePath]) -> (String, f64, f64) {
    let (width, height) = paths
        .first()
        .map(|p| (p.width, p.height))
        .unwrap_or((21600.0, 21600.0));
    let mut out = Vec::new();
    for path in paths {
        let sx = if path.width > 0.0 {
            width / path.width
        } else {
            1.0
        };
        let sy = if path.height > 0.0 {
            height / path.height
        } else {
            1.0
        };
        let p = |x: f64, y: f64| format!("{} {}", number_text(x * sx), number_text(y * sy));
        for command in &path.commands {
            out.push(match command {
                PathCommand::Move { x, y } => format!("M {}", p(*x, *y)),
                PathCommand::Line { x, y } => format!("L {}", p(*x, *y)),
                PathCommand::Cubic {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    format!("C {} {} {}", p(*x1, *y1), p(*x2, *y2), p(*x, *y))
                }
                PathCommand::Quadratic { x1, y1, x, y } => {
                    format!("Q {} {}", p(*x1, *y1), p(*x, *y))
                }
                PathCommand::Close => "Z".into(),
            });
        }
        if !path.fill {
            out.push("F".into());
        }
        if !path.stroke {
            out.push("S".into());
        }
        out.push("N".into());
    }
    (out.join(" "), width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_paths_with_relative_steps_and_arcs() {
        let path = from_svg("M10 10 h80 v80 H10 z m5,5 a5 5 0 0 1 10 0", 100.0, 100.0).unwrap();
        assert_eq!(path.commands[1], PathCommand::Line { x: 90.0, y: 10.0 });
        assert_eq!(path.commands[4], PathCommand::Close);
        assert_eq!(path.commands[5], PathCommand::Move { x: 15.0, y: 15.0 });
        let PathCommand::Cubic { x, y, .. } = path.commands.last().unwrap() else {
            panic!("no arc")
        };
        assert!((x - 25.0).abs() < 1e-9 && (y - 15.0).abs() < 1e-9);
    }

    #[test]
    fn enhanced_paths_round_trip() {
        let paths = from_enhanced(
            "M 0 254000 L 1016000 0 C 1524000 381000 1778000 1143000 1016000 1524000 Z N",
            1778001.0,
            1524001.0,
        )
        .unwrap();
        assert_eq!(paths[0].commands.len(), 4);
        let (text, _, _) = to_enhanced(&paths);
        assert_eq!(
            text,
            "M 0 254000 L 1016000 0 C 1524000 381000 1778000 1143000 1016000 1524000 Z N"
        );
        assert!(from_enhanced("M 0 ?f7 Z N", 1.0, 1.0).is_none());
    }

    #[test]
    fn point_lists() {
        let path = from_points("0,0 100,0 100,100", 100.0, 100.0, true).unwrap();
        assert_eq!(path.commands.len(), 4);
        assert!(path.fill);
    }
}
