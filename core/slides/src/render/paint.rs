//! fills and lines resolved to paints: theme style references looked up,
//! colours resolved, gradients placed in the shape's box.

use crate::color::{resolve, ColorContext, Rgba};
use crate::model::{
    ColorRef, Fill, GradientFill, Line, ShapeStyle, StyleRef, Theme, EMU_PER_POINT,
};

use super::scene::{Paint, StrokeStyle};

/// the background style references start at 1001 (ecma-376 part 1,
/// 20.1.4.2.10: values above 1000 index `a:bgFillStyleLst`)
const BACKGROUND_STYLE_BASE: u32 = 1000;

/// powerpoint's hairline when a line has no width: 0.75 pt (9525 emu is
/// what it writes for its own default lines)
const DEFAULT_LINE_WIDTH: f64 = 0.75;

/// what a fill draws with, or `None` for no fill.
pub fn fill_paint(
    fill: &Fill,
    colors: &ColorContext<'_>,
    width: f64,
    height: f64,
) -> Option<Paint> {
    match fill {
        Fill::None | Fill::Group => None,
        Fill::Solid { color } => Some(Paint::Solid(resolve(color, colors))),
        Fill::Gradient(gradient) => Some(gradient_paint(gradient, colors, width, height)),
        Fill::Pattern { foreground, .. } => {
            // patterns are drawn as their foreground at half strength for now
            let mut color = foreground
                .as_ref()
                .map_or(Rgba::BLACK, |c| resolve(c, colors));
            color.alpha *= 0.5;
            Some(Paint::Solid(color))
        }
        Fill::Picture { xml, .. } => xml.as_deref().and_then(|xml| duotone_paint(xml, colors)),
    }
}

/// a picture recoloured with two colours (`a:duotone`, which themes use
/// for textured backgrounds) drawn as the mix of the two: the picture
/// itself is not decoded, so its texture is lost but its tone is kept
fn duotone_paint(xml: &str, colors: &ColorContext<'_>) -> Option<Paint> {
    let document = crate::xml::parse(xml).ok()?;
    let mut found = None;
    document.root.walk(&mut |element| {
        if found.is_none() && element.name == "a:duotone" {
            let pair: Vec<ColorRef> = element
                .elements()
                .filter_map(crate::pptx::dml::read_color)
                .collect();
            found = Some(pair);
        }
    });
    let pair = found?;
    let (first, second) = (
        resolve(pair.first()?, colors),
        resolve(pair.get(1)?, colors),
    );
    Some(Paint::Solid(Rgba::opaque(
        (first.red + second.red) / 2.0,
        (first.green + second.green) / 2.0,
        (first.blue + second.blue) / 2.0,
    )))
}

/// true for a picture fill that is drawn as its duotone colours.
pub fn is_duotone(fill: &Fill) -> bool {
    matches!(fill, Fill::Picture { xml: Some(xml), .. } if xml.contains("duotone"))
}

fn gradient_paint(
    gradient: &GradientFill,
    colors: &ColorContext<'_>,
    width: f64,
    height: f64,
) -> Paint {
    let mut stops: Vec<(f64, Rgba)> = gradient
        .stops
        .iter()
        .map(|stop| {
            (
                f64::from(stop.position) / 100_000.0,
                resolve(&stop.color, colors),
            )
        })
        .collect();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    if gradient.path.is_some() {
        let [left, top, right, bottom] = gradient.focus.unwrap_or([50_000, 50_000, 50_000, 50_000]);
        let centre_x = width * f64::from(left) / 100_000.0
            + width * (1.0 - f64::from(left + right) / 100_000.0) / 2.0;
        let centre_y = height * f64::from(top) / 100_000.0
            + height * (1.0 - f64::from(top + bottom) / 100_000.0) / 2.0;
        let corner = centre_x
            .max(width - centre_x)
            .hypot(centre_y.max(height - centre_y));
        return Paint::Radial {
            stops,
            centre: (centre_x, centre_y),
            radius: corner,
        };
    }
    let angle = f64::from(gradient.angle.unwrap_or(0)) / 60_000.0;
    let (from, to) = linear_ends(angle, width, height);
    Paint::Linear { stops, from, to }
}

/// the ends of a linear gradient at `angle` (clockwise from left to
/// right) so that the box's corners fall exactly on the first and last
/// stop, which is how drawingml spreads a gradient over a shape.
fn linear_ends(angle: f64, width: f64, height: f64) -> ((f64, f64), (f64, f64)) {
    let (sin, cos) = angle.to_radians().sin_cos();
    let half = (width * cos.abs() + height * sin.abs()) / 2.0;
    let centre = (width / 2.0, height / 2.0);
    (
        (centre.0 - cos * half, centre.1 - sin * half),
        (centre.0 + cos * half, centre.1 + sin * half),
    )
}

/// a line's stroke, or `None` when it draws nothing.
pub fn stroke_style(line: &Line, colors: &ColorContext<'_>) -> Option<StrokeStyle> {
    let color = match &line.fill {
        Some(Fill::Solid { color }) => resolve(color, colors),
        Some(Fill::Gradient(gradient)) => gradient
            .stops
            .first()
            .map_or(Rgba::BLACK, |stop| resolve(&stop.color, colors)),
        None => Rgba::BLACK,
        Some(_) => return None,
    };
    let width = line
        .width
        .map_or(DEFAULT_LINE_WIDTH, |w| w as f64 / EMU_PER_POINT as f64);
    Some(StrokeStyle {
        color,
        width,
        dashes: dashes(line.dash.as_deref(), width),
        cap: match line.cap.as_deref() {
            Some("rnd") => 1,
            Some("sq") => 2,
            _ => 0,
        },
        join: match line.join.as_deref() {
            Some("round") => 1,
            Some("bevel") => 2,
            _ => 0,
        },
    })
}

/// preset dashes as multiples of the line width (ecma-376 part 1,
/// 20.1.10.48 describes each pattern in widths)
fn dashes(dash: Option<&str>, width: f64) -> Vec<f64> {
    let pattern: &[f64] = match dash {
        Some("dot") => &[1.0, 3.0],
        Some("dash") => &[4.0, 3.0],
        Some("lgDash") => &[8.0, 3.0],
        Some("dashDot") => &[4.0, 3.0, 1.0, 3.0],
        Some("lgDashDot") => &[8.0, 3.0, 1.0, 3.0],
        Some("lgDashDotDot") => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
        Some("sysDash") => &[3.0, 1.0],
        Some("sysDot") => &[1.0, 1.0],
        Some("sysDashDot") => &[3.0, 1.0, 1.0, 1.0],
        Some("sysDashDotDot") => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        _ => &[],
    };
    pattern.iter().map(|units| units * width).collect()
}

/// the fill a style reference stands for: the theme's fill style at that
/// index, with `phClr` taken from the reference's colour.
pub fn style_fill(reference: &StyleRef, theme: Option<&Theme>) -> Option<Fill> {
    let theme = theme?;
    if reference.index == 0 {
        return None;
    }
    let list = if reference.index > BACKGROUND_STYLE_BASE {
        &theme.formats.background_fills
    } else {
        &theme.formats.fills
    };
    let index = (reference.index % BACKGROUND_STYLE_BASE).checked_sub(1)? as usize;
    list.get(index).cloned()
}

pub fn style_line(reference: &StyleRef, theme: Option<&Theme>) -> Option<Line> {
    let index = reference.index.checked_sub(1)? as usize;
    theme?.formats.lines.get(index).cloned()
}

/// the colour a style reference hands to `phClr`.
pub fn reference_color(reference: &StyleRef, colors: &ColorContext<'_>) -> Option<Rgba> {
    reference
        .color
        .as_ref()
        .map(|color: &ColorRef| resolve(color, colors))
}

/// a fill and, when it came from a theme style, the reference that
/// hands its `phClr` colour in.
pub type StyledFill = (Fill, Option<StyleRef>);
pub type StyledLine = (Line, Option<StyleRef>);

/// a shape's effective fill and line: its own, or its style's.
pub fn shape_fill_and_line(
    fill: Option<&Fill>,
    line: Option<&Line>,
    style: Option<&ShapeStyle>,
    theme: Option<&Theme>,
) -> (Option<StyledFill>, Option<StyledLine>) {
    let fill = match (fill, style) {
        (Some(fill), _) => Some((fill.clone(), None)),
        (None, Some(style)) => {
            style_fill(&style.fill, theme).map(|f| (f, Some(style.fill.clone())))
        }
        _ => None,
    };
    let style_line =
        style.and_then(|s| style_line(&s.line, theme).map(|l| (l, Some(s.line.clone()))));
    let line = match (line, style_line) {
        // a shape's own line fills in only what it sets over the style's
        (Some(own), Some((base, reference))) => Some((merge_line(own, &base), reference)),
        (Some(own), None) => Some((own.clone(), None)),
        (None, from_style) => from_style,
    };
    (fill, line)
}

fn merge_line(own: &Line, base: &Line) -> Line {
    Line {
        width: own.width.or(base.width),
        fill: own.fill.clone().or_else(|| base.fill.clone()),
        dash: own.dash.clone().or_else(|| base.dash.clone()),
        cap: own.cap.clone().or_else(|| base.cap.clone()),
        compound: own.compound.clone().or_else(|| base.compound.clone()),
        join: own.join.clone().or_else(|| base.join.clone()),
        head: own.head.clone().or_else(|| base.head.clone()),
        tail: own.tail.clone().or_else(|| base.tail.clone()),
        extra: own.extra.clone(),
    }
}
