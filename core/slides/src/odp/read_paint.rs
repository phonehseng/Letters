//! fills and lines from opendocument graphic properties.

use super::read_shapes::ShapeContext;
use super::styles::{get, Properties};
use super::units::{color, length, percent};
use crate::model::{ColorRef, Fill, GradientFill, GradientStop, Line, LineEnd};

/// a fill from graphic properties.
pub fn read_fill(graphic: &Properties, context: &ShapeContext<'_>) -> Option<Fill> {
    let solid = |hex: Option<String>| {
        let mut color = ColorRef::rgb(&hex.unwrap_or_else(|| "729FCF".into()));
        if let Some(opacity) = get(graphic, "draw:opacity").and_then(percent) {
            color = color.with("alpha", opacity);
        }
        Fill::Solid { color }
    };
    match get(graphic, "draw:fill")? {
        "none" => Some(Fill::None),
        "solid" => Some(solid(get(graphic, "draw:fill-color").and_then(color))),
        "gradient" => Some(gradient_fill(graphic, context)),
        "bitmap" => {
            let name = get(graphic, "draw:fill-image-name").unwrap_or("");
            let media_id = context
                .styles
                .images
                .get(name)
                .and_then(|href| context.media.get(href).cloned());
            Some(Fill::Picture {
                media_id,
                xml: None,
            })
        }
        _ => Some(solid(get(graphic, "draw:fill-hatch-solid").and_then(color))),
    }
}

fn gradient_fill(graphic: &Properties, context: &ShapeContext<'_>) -> Fill {
    let name = get(graphic, "draw:fill-gradient-name").unwrap_or("");
    let Some((start, end, angle, style)) = context.styles.gradients.get(name) else {
        return Fill::None;
    };
    let stop = |position: i32, value: &str| GradientStop {
        position,
        color: ColorRef::rgb(&color(value).unwrap_or_else(|| "000000".into())),
    };
    // odf measures gradient angles counter-clockwise from "top to bottom";
    // drawingml clockwise from "left to right"
    let drawingml_angle = ((90.0 - angle).rem_euclid(360.0) * 60_000.0).round() as i32;
    Fill::Gradient(GradientFill {
        stops: vec![stop(0, start), stop(100_000, end)],
        angle: (style == "linear" || style == "axial").then_some(drawingml_angle),
        path: (style == "radial" || style == "ellipsoid").then(|| "circle".to_string()),
        ..Default::default()
    })
}

/// a line from graphic properties.
pub fn read_line(graphic: &Properties) -> Option<Line> {
    let stroke = get(graphic, "draw:stroke")?;
    if stroke == "none" {
        return Some(Line {
            fill: Some(Fill::None),
            ..Default::default()
        });
    }
    let hex = get(graphic, "svg:stroke-color")
        .and_then(color)
        .unwrap_or_else(|| "3465A4".into());
    let end = |name: &str| {
        get(graphic, name)
            .filter(|m| !m.is_empty())
            .map(|_| LineEnd {
                kind: "triangle".into(),
                width: None,
                length: None,
            })
    };
    Some(Line {
        width: get(graphic, "svg:stroke-width").and_then(length),
        fill: Some(Fill::Solid {
            color: ColorRef::rgb(&hex),
        }),
        dash: (stroke == "dash").then(|| "dash".to_string()),
        head: end("draw:marker-start"),
        tail: end("draw:marker-end"),
        ..Default::default()
    })
}
