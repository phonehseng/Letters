//! a new, empty deck: one theme, one master with a title and a body
//! placeholder and powerpoint's usual text styles, and three layouts
//! (title slide, title and content, blank). used for new decks and for
//! turning a document outline into slides.

use crate::model::{
    Autofit, Bullet, BulletFont, Layout, LevelStyle, Master, Object, ParagraphProps, Placeholder,
    PlaceholderKind, Presentation, RunProps, Shape, Slide, SlideSize, Spacing, TextAlign, TextBody,
    TextParagraph, TextStyle, Theme, Transform, Typeface, EMU_PER_POINT,
};

pub const MASTER_ID: &str = "master-1";
pub const THEME_ID: &str = "theme-1";
pub const TITLE_LAYOUT: &str = "layout-title";
pub const CONTENT_LAYOUT: &str = "layout-content";
pub const BLANK_LAYOUT: &str = "layout-blank";

/// a 16:9 deck with no slides.
pub fn blank_presentation() -> Presentation {
    let size = SlideSize::default();
    Presentation {
        slide_size: size.clone(),
        themes: vec![Theme {
            id: THEME_ID.into(),
            name: "Office Theme".into(),
            ..Default::default()
        }],
        masters: vec![master(&size)],
        layouts: layouts(&size),
        default_text_style: level_text_style(1_800, false),
        ..Default::default()
    }
}

/// a placeholder shape with an empty text body.
pub fn placeholder(
    id: u32,
    name: &str,
    kind: PlaceholderKind,
    index: Option<u32>,
    transform: Option<Transform>,
) -> Object {
    Object::Shape(Shape {
        id,
        name: name.to_string(),
        placeholder: Some(Placeholder {
            kind,
            index,
            orient: None,
            size: None,
            custom_prompt: false,
        }),
        transform,
        text: Some(TextBody::default()),
        ..Default::default()
    })
}

/// the title and body placeholders' boxes, in powerpoint's 16:9 places
/// scaled to the slide size.
fn boxes(size: &SlideSize) -> (Transform, Transform) {
    let scale_x = |emu: i64| (emu as f64 * size.width as f64 / 12_192_000.0).round() as i64;
    let scale_y = |emu: i64| (emu as f64 * size.height as f64 / 6_858_000.0).round() as i64;
    let title = Transform::new(
        scale_x(838_200),
        scale_y(365_125),
        scale_x(10_515_600),
        scale_y(1_325_563),
    );
    let body = Transform::new(
        scale_x(838_200),
        scale_y(1_825_625),
        scale_x(10_515_600),
        scale_y(4_351_338),
    );
    (title, body)
}

fn master(size: &SlideSize) -> Master {
    let (title, body) = boxes(size);
    let mut title_object = placeholder(
        2,
        "Title Placeholder 1",
        PlaceholderKind::Title,
        None,
        Some(title),
    );
    let mut body_object = placeholder(
        3,
        "Text Placeholder 2",
        PlaceholderKind::Body,
        Some(1),
        Some(body),
    );
    if let Some(text) = title_object.text_mut() {
        text.body.anchor = Some(crate::model::Anchor::Middle);
        text.body.autofit = Some(Autofit::Shrink {
            font_scale: None,
            line_reduction: None,
        });
    }
    if let Some(text) = body_object.text_mut() {
        text.body.autofit = Some(Autofit::Shrink {
            font_scale: None,
            line_reduction: None,
        });
    }
    Master {
        id: MASTER_ID.into(),
        name: "Office Theme".into(),
        theme_id: Some(THEME_ID.into()),
        objects: vec![title_object, body_object],
        title_style: title_style(),
        body_style: body_style(),
        other_style: level_text_style(1_800, false),
        layout_ids: vec![
            TITLE_LAYOUT.into(),
            CONTENT_LAYOUT.into(),
            BLANK_LAYOUT.into(),
        ],
        ..Default::default()
    }
}

fn layouts(size: &SlideSize) -> Vec<Layout> {
    let (_, body) = boxes(size);
    let centred_title = Transform::new(
        body.x + body.width / 8,
        (size.height as f64 * 0.16).round() as i64,
        body.width * 3 / 4,
        (size.height as f64 * 0.35).round() as i64,
    );
    let subtitle = Transform::new(
        centred_title.x,
        centred_title.y + centred_title.height + EMU_PER_POINT * 12,
        centred_title.width,
        (size.height as f64 * 0.24).round() as i64,
    );
    let layout = |id: &str, name: &str, kind: &str, objects: Vec<Object>| Layout {
        id: id.into(),
        name: name.into(),
        master_id: MASTER_ID.into(),
        kind: Some(kind.into()),
        objects,
        show_master_shapes: true,
        ..Default::default()
    };
    vec![
        layout(
            TITLE_LAYOUT,
            "Title Slide",
            "title",
            vec![
                placeholder(
                    2,
                    "Title 1",
                    PlaceholderKind::CenteredTitle,
                    None,
                    Some(centred_title),
                ),
                placeholder(
                    3,
                    "Subtitle 2",
                    PlaceholderKind::Subtitle,
                    Some(1),
                    Some(subtitle),
                ),
            ],
        ),
        layout(
            CONTENT_LAYOUT,
            "Title and Content",
            "obj",
            vec![
                placeholder(2, "Title 1", PlaceholderKind::Title, None, None),
                placeholder(
                    3,
                    "Content Placeholder 2",
                    PlaceholderKind::Object,
                    Some(1),
                    None,
                ),
            ],
        ),
        layout(BLANK_LAYOUT, "Blank", "blank", Vec::new()),
    ]
}

/// one level per entry, sizes in hundredths of a point.
fn level_text_style(size: i32, bulleted: bool) -> TextStyle {
    let levels = (0..9)
        .map(|level| {
            let margin = i64::from(level) * 457_200;
            Some(LevelStyle {
                paragraph: ParagraphProps {
                    margin_left: Some(if bulleted { margin + 228_600 } else { margin }),
                    indent: Some(if bulleted { -228_600 } else { 0 }),
                    align: Some(TextAlign::Left),
                    run: Some(RunProps {
                        size: Some(size),
                        latin: Some(Typeface::named("+mn-lt")),
                        fill: Some(crate::model::Fill::solid(crate::model::ColorRef::scheme(
                            "tx1",
                        ))),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            })
        })
        .collect();
    TextStyle {
        levels,
        ..Default::default()
    }
}

/// powerpoint's title: 44 pt in the heading font, 90 % line spacing.
fn title_style() -> TextStyle {
    let mut style = level_text_style(4_400, false);
    if let Some(Some(level)) = style.levels.get_mut(0) {
        level.paragraph.line_spacing = Some(Spacing::Percent(90_000));
        if let Some(run) = &mut level.paragraph.run {
            run.latin = Some(Typeface::named("+mj-lt"));
        }
    }
    style.levels.truncate(1);
    style
}

/// powerpoint's body: 28 pt, then 24, 20, 18 pt, bulleted, 90 % line
/// spacing and 10 pt (first level) or 5 pt before each paragraph.
fn body_style() -> TextStyle {
    let mut style = level_text_style(2_800, true);
    for (index, level) in style.levels.iter_mut().enumerate() {
        let Some(level) = level else {
            continue;
        };
        let paragraph = &mut level.paragraph;
        paragraph.line_spacing = Some(Spacing::Percent(90_000));
        paragraph.space_before = Some(Spacing::Points(if index == 0 { 1_000 } else { 500 }));
        paragraph.bullet = Some(Bullet::Char { char: "•".into() });
        paragraph.bullet_font = Some(BulletFont::Font(Typeface::named("Arial")));
        if let Some(run) = &mut paragraph.run {
            run.size = Some(match index {
                0 => 2_800,
                1 => 2_400,
                2 => 2_000,
                _ => 1_800,
            });
        }
    }
    style
}

/// a slide on a layout, with empty placeholders copied from the layout
/// (empty placeholders take their position from the layout). like
/// powerpoint's New Slide, the date, footer and slide number placeholders
/// stay on the layout, and the layout's prompt words ("Click to edit Master
/// title style") are not copied: a new slide's placeholders hold no text.
pub fn new_slide(presentation: &Presentation, id: &str, layout_id: &str) -> Slide {
    let objects = presentation
        .layout(layout_id)
        .map(|layout| {
            layout
                .objects
                .iter()
                .filter(|o| o.placeholder().is_some_and(|p| !p.kind.is_furniture()))
                .map(empty_placeholder)
                .collect()
        })
        .unwrap_or_default();
    Slide {
        id: id.to_string(),
        layout_id: Some(layout_id.to_string()),
        objects,
        show_master_shapes: true,
        ..Default::default()
    }
}

/// a slide's copy of a layout placeholder: its id, name and placeholder
/// link only, so its place, look and text style all come from the layout
/// (what powerpoint writes: an empty `p:spPr` and one empty paragraph).
fn empty_placeholder(object: &Object) -> Object {
    match object {
        Object::Shape(shape) => Object::Shape(Shape {
            id: shape.id,
            name: shape.name.clone(),
            placeholder: shape.placeholder.clone(),
            text: Some(TextBody {
                paragraphs: vec![TextParagraph::default()],
                ..Default::default()
            }),
            ..Default::default()
        }),
        other => {
            let mut copy = other.clone();
            copy.set_source(None);
            if let Some(transform) = copy.transform_mut() {
                *transform = None;
            }
            copy
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_slide_has_empty_placeholders_and_no_footer_ones() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/decks/pp_title_bullets.pptx");
        let deck = crate::read_pptx(&std::fs::read(path).unwrap()).unwrap();
        let layout = deck
            .layouts
            .iter()
            .find(|layout| {
                layout
                    .objects
                    .iter()
                    .any(|o| o.text().is_some_and(|t| !t.is_empty()))
            })
            .unwrap();
        let slide = new_slide(&deck, "new", &layout.id);
        assert!(!slide.objects.is_empty());
        for object in &slide.objects {
            assert!(
                object.text().is_none_or(TextBody::is_empty),
                "{}",
                object.name()
            );
            assert!(!object.placeholder().unwrap().kind.is_furniture());
            assert!(
                object.transform().is_none(),
                "the place comes from the layout"
            );
        }
    }
}
