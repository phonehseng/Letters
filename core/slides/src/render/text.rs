//! a shape's text drawn: effective formatting resolved through the
//! inheritance chain, laid out in the text area, turned into glyph items.

use crate::color::{resolve, ColorContext, Rgba};
use crate::inherit::{effective_body, effective_paragraph, effective_run, resolve_typeface, Chain};
use crate::layout::bullets::NumberCounter;
use crate::layout::runs::StyledRun;
use crate::layout::{layout_box, BoxInput, LaidText, ParagraphInput};
use crate::model::{
    Anchor, Autofit, BulletColor, BulletFont, BulletSize, FieldKind, Fill, Object, ParagraphProps,
    RunProps, Spacing, TextAlign, TextBody, TextRun, EMU_PER_POINT,
};

use super::scene::{multiply, translate, Item, Matrix};
use super::RenderContext;

/// powerpoint's default inset: 0.1 inch at the sides, 0.05 inch top and bottom
const DEFAULT_INSET_SIDE: i64 = 91_440;
const DEFAULT_INSET_TOP: i64 = 45_720;
/// the size text has when nothing sets one: 18 pt (ecma-376 part 1, 21.1.2.3.9)
const DEFAULT_SIZE: i32 = 1_800;

fn points(emu: i64) -> f64 {
    emu as f64 / EMU_PER_POINT as f64
}

/// what the text of one object is drawn with.
pub struct TextTarget<'a> {
    pub object: &'a Object,
    pub body: &'a TextBody,
    /// the text rectangle in the shape's own points: left, top, right, bottom
    pub rect: [f64; 4],
    pub matrix: Matrix,
    /// the default text colour (a style's font reference), if any
    pub color: Option<Rgba>,
}

pub fn draw_text(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    target: &TextTarget<'_>,
    items: &mut Vec<Item>,
) -> LaidText {
    let body = effective_body(chain, target.object, target.body);
    let insets = [
        body.inset_left.unwrap_or(DEFAULT_INSET_SIDE),
        body.inset_top.unwrap_or(DEFAULT_INSET_TOP),
        body.inset_right.unwrap_or(DEFAULT_INSET_SIDE),
        body.inset_bottom.unwrap_or(DEFAULT_INSET_TOP),
    ]
    .map(points);
    let [left, top, right, bottom] = target.rect;
    let (font_scale, line_reduction) = match &body.autofit {
        Some(Autofit::Shrink {
            font_scale,
            line_reduction,
        }) => (
            f64::from(font_scale.unwrap_or(100_000)) / 100_000.0,
            f64::from(line_reduction.unwrap_or(0)) / 100_000.0,
        ),
        _ => (1.0, 0.0),
    };
    let input = BoxInput {
        width: (right - left - insets[0] - insets[2]).max(1.0),
        height: (bottom - top - insets[1] - insets[3]).max(0.0),
        anchor: body.anchor.unwrap_or(Anchor::Top),
        wrap: body.wrap.unwrap_or(true),
        font_scale,
        line_reduction,
        columns: body.columns.unwrap_or(1),
        column_spacing: body.column_spacing.map_or(0.0, points),
        // powerpoint shows the scale it stored and never works one out when
        // drawing (pp_autofit_notes: its unscaled overflow is in its pdf);
        // libreoffice works it out, so only decks from odp shrink here
        shrink_to_fit: from_odp(context)
            && matches!(
                body.autofit,
                Some(Autofit::Shrink {
                    font_scale: None,
                    ..
                })
            ),
    };
    let (paragraphs, colors) = paragraph_inputs(context, chain, target);
    let laid = layout_box(&input, &paragraphs, context.fonts);
    let origin = multiply(
        &target.matrix,
        &translate(left + insets[0], top + insets[1]),
    );
    emit_glyphs(context, &laid, &colors, origin, items);
    laid
}

/// per paragraph: its layout input, and the colour of each run (the
/// last entry is the bullet's).
fn paragraph_inputs(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    target: &TextTarget<'_>,
) -> (Vec<ParagraphInput>, Vec<Vec<Rgba>>) {
    let mut counter = NumberCounter::default();
    let mut inputs = Vec::new();
    let mut colors = Vec::new();
    for paragraph in &target.body.paragraphs {
        let effective = effective_paragraph(chain, target.object, target.body, paragraph);
        let mut run_colors = Vec::new();
        let mut runs = Vec::new();
        for run in &paragraph.runs {
            let props = effective_run(&effective, run.props());
            run_colors.push(run_color(&props, context.colors(), target.color));
            runs.push(styled_run(
                chain,
                &props,
                &run_text(run, context.slide_number),
            ));
        }
        let mark_props = effective_run(
            &effective,
            paragraph.end_props.as_ref().unwrap_or(&RunProps::default()),
        );
        let mark = styled_run(chain, &mark_props, "");
        let first_props = paragraph
            .runs
            .first()
            .map(|r| effective_run(&effective, r.props()))
            .unwrap_or(mark_props);
        let bullet = counter
            .label(&effective)
            .map(|label| bullet_run(chain, &effective, &first_props, &label));
        let first_color = run_colors.first().copied().unwrap_or(Rgba::BLACK);
        run_colors.push(bullet_color(&effective, context.colors(), first_color));
        inputs.push(paragraph_input(&effective, runs, mark, bullet));
        colors.push(run_colors);
    }
    (inputs, colors)
}

fn from_odp(context: &RenderContext<'_>) -> bool {
    context.presentation.carry.format == Some(crate::model::SourceFormat::Odp)
}

fn run_text(run: &TextRun, slide_number: usize) -> String {
    match run {
        TextRun::Text { text, .. } | TextRun::Preserved { text, .. } => text.clone(),
        TextRun::Field {
            kind: FieldKind::SlideNumber,
            ..
        } => slide_number.to_string(),
        TextRun::Field { text, .. } => text.clone(),
        TextRun::Break { .. } => "\n".into(),
    }
}

fn paragraph_input(
    props: &ParagraphProps,
    runs: Vec<StyledRun>,
    mark: StyledRun,
    bullet: Option<StyledRun>,
) -> ParagraphInput {
    ParagraphInput {
        runs,
        mark,
        align: props.align.unwrap_or(TextAlign::Left),
        margin_left: props.margin_left.map_or(0.0, points),
        margin_right: props.margin_right.map_or(0.0, points),
        indent: props.indent.map_or(0.0, points),
        line_spacing: props.line_spacing.unwrap_or(Spacing::Percent(100_000)),
        space_before: props.space_before.unwrap_or(Spacing::Points(0)),
        space_after: props.space_after.unwrap_or(Spacing::Points(0)),
        bullet,
    }
}

fn styled_run(chain: &Chain<'_>, props: &RunProps, text: &str) -> StyledRun {
    let face = |typeface: &Option<crate::model::Typeface>, default: &str| {
        let name = typeface.as_ref().map_or(default, |t| t.typeface.as_str());
        resolve_typeface(chain.theme, name)
    };
    StyledRun {
        text: text.to_string(),
        family: face(&props.latin, "+mn-lt"),
        east_asian_family: face(&props.east_asian, "+mn-ea"),
        complex_family: face(&props.complex, "+mn-cs"),
        size: f64::from(props.size.unwrap_or(DEFAULT_SIZE)) / 100.0,
        bold: props.bold.unwrap_or(false),
        italic: props.italic.unwrap_or(false),
        spacing: f64::from(props.spacing.unwrap_or(0)) / 100.0,
        baseline_shift: f64::from(props.baseline.unwrap_or(0)) / 100_000.0,
        all_caps: props.caps.as_deref() == Some("all"),
        is_break: text == "\n",
    }
}

fn bullet_run(
    chain: &Chain<'_>,
    props: &ParagraphProps,
    first: &RunProps,
    label: &str,
) -> StyledRun {
    let mut run = styled_run(chain, first, label);
    if let Some(BulletFont::Font(typeface)) = &props.bullet_font {
        run.family = resolve_typeface(chain.theme, &typeface.typeface);
    }
    run.size = match props.bullet_size {
        Some(BulletSize::Percent(thousandths)) => run.size * f64::from(thousandths) / 100_000.0,
        Some(BulletSize::Points(hundredths)) => f64::from(hundredths) / 100.0,
        _ => run.size,
    };
    run.spacing = 0.0;
    run.baseline_shift = 0.0;
    run
}

fn run_color(props: &RunProps, colors: ColorContext<'_>, default: Option<Rgba>) -> Rgba {
    match &props.fill {
        Some(Fill::Solid { color }) => resolve(color, &colors),
        Some(Fill::Gradient(gradient)) => gradient
            .stops
            .first()
            .map_or(Rgba::BLACK, |s| resolve(&s.color, &colors)),
        _ => default.unwrap_or(Rgba::BLACK),
    }
}

fn bullet_color(props: &ParagraphProps, colors: ColorContext<'_>, text: Rgba) -> Rgba {
    match &props.bullet_color {
        Some(BulletColor::Color(color)) => resolve(color, &colors),
        _ => text,
    }
}

fn emit_glyphs(
    context: &RenderContext<'_>,
    laid: &LaidText,
    colors: &[Vec<Rgba>],
    origin: Matrix,
    items: &mut Vec<Item>,
) {
    for line in &laid.lines {
        let paragraph_colors = &colors[line.paragraph];
        for piece in &line.pieces {
            let color = match piece.run {
                Some(run) => paragraph_colors.get(run).copied().unwrap_or(Rgba::BLACK),
                None => paragraph_colors.last().copied().unwrap_or(Rgba::BLACK),
            };
            let resolved = context.fonts.fonts().get(piece.font as usize);
            let glyphs = piece
                .glyphs
                .iter()
                .map(|glyph| {
                    (
                        glyph.0,
                        glyph.1,
                        line.baseline - piece.baseline_shift,
                        glyph.2,
                    )
                })
                .collect::<Vec<_>>();
            if glyphs.is_empty() {
                continue;
            }
            items.push(Item::Text {
                matrix: origin,
                font: piece.font,
                size: piece.size,
                color,
                glyphs,
                synthetic_bold: resolved.is_some_and(|f| f.synthetic_bold),
                synthetic_italic: resolved.is_some_and(|f| f.synthetic_italic),
            });
        }
    }
}
