//! glyph outlines rasterised for the look-alike measure of font completion:
//! how much of two glyphs' ink overlaps.

use crate::fonts::outline::{ContourPoint, Contours};

/// rasterising resolution of the similarity measure, pixels per em.
const RASTER_PIXELS: usize = 64;

/// a contour set rasterised into a square of `RASTER_PIXELS` per em (non-zero fill).
pub fn raster(contours: &Contours, units_per_em: f64) -> Vec<bool> {
    let n = RASTER_PIXELS;
    let scale = n as f64 / units_per_em;
    let edges = flattened_edges(contours, scale);
    let mut pixels = vec![false; n * n];
    for row in 0..n {
        // the em box from 0.25 em below the baseline
        let y = (row as f64 + 0.5) - 0.25 * n as f64;
        let mut crossings: Vec<(f64, i32)> = edges
            .iter()
            .filter(|((_, y0), (_, y1))| (y0 <= &y) != (y1 <= &y))
            .map(|((x0, y0), (x1, y1))| {
                let x = x0 + (y - y0) / (y1 - y0) * (x1 - x0);
                (x, if y1 > y0 { 1 } else { -1 })
            })
            .collect();
        crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut winding = 0;
        for pair in crossings.windows(2) {
            winding += pair[0].1;
            if winding != 0 {
                let from = pair[0].0.max(0.0).round() as usize;
                let to = pair[1].0.min(n as f64).round() as usize;
                for column in from..to.min(n) {
                    pixels[(n - 1 - row) * n + column] = true;
                }
            }
        }
    }
    pixels
}

type Edge = ((f64, f64), (f64, f64));

/// straight edges of truetype contours, quadratics split into 8 pieces.
fn flattened_edges(contours: &Contours, scale: f64) -> Vec<Edge> {
    let mut edges = Vec::new();
    for contour in contours {
        let points = on_curve_path(contour);
        for window in points.windows(2) {
            let (a, b) = (window[0], window[1]);
            edges.push(((a.0 * scale, a.1 * scale), (b.0 * scale, b.1 * scale)));
        }
    }
    edges
}

/// a closed polyline through a truetype contour (implied on-curve points
/// between two off-curve points, quadratics flattened).
fn on_curve_path(contour: &[ContourPoint]) -> Vec<(f64, f64)> {
    let count = contour.len();
    if count == 0 {
        return Vec::new();
    }
    let point = |i: usize| {
        let p = contour[i % count];
        (f64::from(p.x), f64::from(p.y), p.on_curve)
    };
    // start at an on-curve point (or the midpoint of the first two)
    let start_index = (0..count).find(|&i| contour[i].on_curve);
    let start = match start_index {
        Some(i) => {
            let p = point(i);
            (p.0, p.1)
        }
        None => {
            let (a, b) = (point(0), point(1));
            ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
        }
    };
    let offset = start_index.unwrap_or(0);
    let mut path = vec![start];
    let mut control: Option<(f64, f64)> = None;
    let mut pen = start;
    for step in 1..=count {
        let p = point(offset + step);
        let here = (p.0, p.1);
        if p.2 {
            match control.take() {
                Some(c) => quadratic(&mut path, pen, c, here),
                None => path.push(here),
            }
            pen = here;
        } else if let Some(c) = control {
            let middle = ((c.0 + here.0) / 2.0, (c.1 + here.1) / 2.0);
            quadratic(&mut path, pen, c, middle);
            pen = middle;
            control = Some(here);
        } else {
            control = Some(here);
        }
    }
    if let Some(c) = control {
        quadratic(&mut path, pen, c, start);
    } else if pen != start {
        path.push(start);
    }
    path
}

fn quadratic(path: &mut Vec<(f64, f64)>, from: (f64, f64), control: (f64, f64), to: (f64, f64)) {
    for piece in 1..=8 {
        let t = piece as f64 / 8.0;
        let u = 1.0 - t;
        path.push((
            u * u * from.0 + 2.0 * u * t * control.0 + t * t * to.0,
            u * u * from.1 + 2.0 * u * t * control.1 + t * t * to.1,
        ));
    }
}

/// intersection over union of two rasters (1 when both are empty).
pub fn overlap(a: &[bool], b: &[bool]) -> f64 {
    let both = a.iter().zip(b).filter(|(x, y)| **x && **y).count();
    let either = a.iter().zip(b).filter(|(x, y)| **x || **y).count();
    if either == 0 {
        1.0
    } else {
        both as f64 / either as f64
    }
}
