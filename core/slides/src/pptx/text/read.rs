//! drawingml text read into the model.

use crate::xml::Element;

use crate::model::{
    Anchor, Autofit, BodyProps, Bullet, BulletColor, BulletFont, BulletSize, FieldKind, LevelStyle,
    ParagraphProps, RunProps, SourceFormat, Spacing, TabStop, TextAlign, TextBody, TextParagraph,
    TextRun, TextStyle, Typeface,
};
use crate::pptx::dml::{
    attr_bool, attr_i32, attr_i64, find_color, find_fill, ReadContext, FILL_NAMES,
};
use crate::preserve::{element_text, source_of, unknown_attributes, unknown_children};

use super::*;

pub fn read_text_body(element: &Element, context: &ReadContext<'_>) -> TextBody {
    TextBody {
        body: element
            .child("a:bodyPr")
            .map(|body| read_body(body, context))
            .unwrap_or_default(),
        list_style: element
            .child("a:lstStyle")
            .map(|style| read_text_style(style, context))
            .unwrap_or_default(),
        paragraphs: element
            .children_named("a:p")
            .map(|paragraph| read_paragraph(paragraph, context))
            .collect(),
    }
}

pub(super) fn read_body(element: &Element, context: &ReadContext<'_>) -> BodyProps {
    let autofit = if element.child("a:noAutofit").is_some() {
        Some(Autofit::None)
    } else if let Some(norm) = element.child("a:normAutofit") {
        Some(Autofit::Shrink {
            font_scale: attr_i32(norm, "fontScale"),
            line_reduction: attr_i32(norm, "lnSpcReduction"),
        })
    } else if element.child("a:spAutoFit").is_some() {
        Some(Autofit::ResizeShape)
    } else {
        None
    };
    let mut extra = unknown_attributes(element, &BODY_ATTRIBUTES);
    extra.extend(unknown_children(context.text, element, &AUTOFIT));
    BodyProps {
        inset_left: attr_i64(element, "lIns"),
        inset_top: attr_i64(element, "tIns"),
        inset_right: attr_i64(element, "rIns"),
        inset_bottom: attr_i64(element, "bIns"),
        anchor: element.attr("anchor").map(anchor_from),
        anchor_center: attr_bool(element, "anchorCtr"),
        wrap: element.attr("wrap").map(|wrap| wrap != "none"),
        autofit,
        columns: element.attr("numCol").and_then(|n| n.parse().ok()),
        column_spacing: attr_i64(element, "spcCol"),
        vertical: element.attr("vert").map(str::to_string),
        rotation: attr_i32(element, "rot"),
        extra,
    }
}

pub(super) fn anchor_from(value: &str) -> Anchor {
    match value {
        "ctr" => Anchor::Middle,
        "b" => Anchor::Bottom,
        "just" => Anchor::Justified,
        "dist" => Anchor::Distributed,
        _ => Anchor::Top,
    }
}

pub(super) fn anchor_name(anchor: Anchor) -> &'static str {
    match anchor {
        Anchor::Top => "t",
        Anchor::Middle => "ctr",
        Anchor::Bottom => "b",
        Anchor::Justified => "just",
        Anchor::Distributed => "dist",
    }
}

pub fn read_text_style(element: &Element, context: &ReadContext<'_>) -> TextStyle {
    let mut style = TextStyle::default();
    for child in element.elements() {
        let name = child.name.as_str();
        if name == "a:defPPr" {
            style.default = Some(LevelStyle {
                paragraph: read_paragraph_props(child, context),
            });
        } else if let Some(level) = level_of(name) {
            if style.levels.len() <= level {
                style.levels.resize(level + 1, None);
            }
            style.levels[level] = Some(LevelStyle {
                paragraph: read_paragraph_props(child, context),
            });
        } else {
            style
                .extra
                .push(crate::model::RawChild::element(None, &context.raw(child)));
        }
    }
    style
}

/// "a:lvl3pPr" is level index 2.
pub(super) fn level_of(name: &str) -> Option<usize> {
    let number = name.strip_prefix("a:lvl")?.strip_suffix("pPr")?;
    let level: usize = number.parse().ok()?;
    (1..=9).contains(&level).then(|| level - 1)
}

pub(super) fn paragraph_known() -> Vec<&'static str> {
    [
        &LINE_SPACING[..],
        &SPACE_BEFORE,
        &SPACE_AFTER,
        &BULLET_COLOR,
        &BULLET_SIZE,
        &BULLET_FONT,
        &BULLET,
        &TABS,
        &DEFAULT_RUN,
    ]
    .concat()
}

pub fn read_paragraph_props(element: &Element, context: &ReadContext<'_>) -> ParagraphProps {
    let spacing = |name: &str| element.child(name).and_then(read_spacing);
    let bullet_color = if element.child("a:buClrTx").is_some() {
        Some(BulletColor::FollowText)
    } else {
        element
            .child("a:buClr")
            .and_then(find_color)
            .map(BulletColor::Color)
    };
    let bullet_size = if element.child("a:buSzTx").is_some() {
        Some(BulletSize::FollowText)
    } else if let Some(size) = element.child("a:buSzPct") {
        attr_i32(size, "val").map(BulletSize::Percent)
    } else {
        element
            .child("a:buSzPts")
            .and_then(|size| attr_i32(size, "val"))
            .map(BulletSize::Points)
    };
    let bullet_font = if element.child("a:buFontTx").is_some() {
        Some(BulletFont::FollowText)
    } else {
        element
            .child("a:buFont")
            .and_then(read_typeface)
            .map(BulletFont::Font)
    };
    let mut extra = unknown_attributes(element, &PARAGRAPH_ATTRIBUTES);
    extra.extend(unknown_children(context.text, element, &paragraph_known()));
    ParagraphProps {
        level: element.attr("lvl").and_then(|level| level.parse().ok()),
        align: element.attr("algn").map(align_from),
        margin_left: attr_i64(element, "marL"),
        margin_right: attr_i64(element, "marR"),
        indent: attr_i64(element, "indent"),
        line_spacing: spacing("a:lnSpc"),
        space_before: spacing("a:spcBef"),
        space_after: spacing("a:spcAft"),
        bullet: read_bullet(element, context),
        bullet_font,
        bullet_color,
        bullet_size,
        default_tab: attr_i64(element, "defTabSz"),
        tabs: element.child("a:tabLst").map(|list| {
            list.children_named("a:tab")
                .map(|tab| TabStop {
                    position: attr_i64(tab, "pos").unwrap_or(0),
                    align: tab.attr("algn").unwrap_or("l").to_string(),
                })
                .collect()
        }),
        rtl: attr_bool(element, "rtl"),
        run: element
            .child("a:defRPr")
            .map(|run| read_run_props(run, context)),
        extra,
    }
}

pub(super) fn read_spacing(element: &Element) -> Option<Spacing> {
    if let Some(percent) = element.child("a:spcPct") {
        return attr_i32(percent, "val").map(Spacing::Percent);
    }
    element
        .child("a:spcPts")
        .and_then(|points| attr_i32(points, "val"))
        .map(Spacing::Points)
}

pub(super) fn read_bullet(element: &Element, context: &ReadContext<'_>) -> Option<Bullet> {
    if element.child("a:buNone").is_some() {
        return Some(Bullet::None);
    }
    if let Some(bullet) = element.child("a:buChar") {
        return Some(Bullet::Char {
            char: bullet.attr("char").unwrap_or("•").to_string(),
        });
    }
    if let Some(number) = element.child("a:buAutoNum") {
        return Some(Bullet::AutoNumber {
            scheme: number.attr("type").unwrap_or("arabicPeriod").to_string(),
            start_at: number.attr("startAt").and_then(|start| start.parse().ok()),
        });
    }
    element.child("a:buBlip").map(|blip| Bullet::Picture {
        media_id: blip
            .child("a:blip")
            .and_then(|b| b.attr("r:embed"))
            .and_then(|id| context.media_id(id)),
        xml: context.raw(blip),
    })
}

pub(super) fn align_from(value: &str) -> TextAlign {
    match value {
        "ctr" => TextAlign::Center,
        "r" => TextAlign::Right,
        "just" => TextAlign::Justify,
        "dist" => TextAlign::Distributed,
        "justLow" => TextAlign::JustifyLow,
        "thaiDist" => TextAlign::Thai,
        _ => TextAlign::Left,
    }
}

pub(super) fn align_name(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "l",
        TextAlign::Center => "ctr",
        TextAlign::Right => "r",
        TextAlign::Justify => "just",
        TextAlign::Distributed => "dist",
        TextAlign::JustifyLow => "justLow",
        TextAlign::Thai => "thaiDist",
    }
}

pub(super) fn read_typeface(element: &Element) -> Option<Typeface> {
    Some(Typeface {
        typeface: element.attr("typeface")?.to_string(),
        attributes: element
            .attributes
            .iter()
            .filter(|(key, _)| key != "typeface")
            .cloned()
            .collect(),
    })
}

pub fn read_run_props(element: &Element, context: &ReadContext<'_>) -> RunProps {
    let known: Vec<&str> = [
        &FILL_NAMES[..],
        &HIGHLIGHT,
        &LATIN,
        &EAST_ASIAN,
        &COMPLEX,
        &SYMBOL,
    ]
    .concat();
    let mut extra = unknown_attributes(element, &RUN_ATTRIBUTES);
    extra.extend(unknown_children(context.text, element, &known));
    RunProps {
        size: attr_i32(element, "sz"),
        bold: attr_bool(element, "b"),
        italic: attr_bool(element, "i"),
        underline: element.attr("u").map(str::to_string),
        strike: element.attr("strike").map(str::to_string),
        caps: element.attr("cap").map(str::to_string),
        baseline: attr_i32(element, "baseline"),
        spacing: attr_i32(element, "spc"),
        kern: attr_i32(element, "kern"),
        lang: element.attr("lang").map(str::to_string),
        fill: find_fill(element, context),
        highlight: element.child("a:highlight").and_then(find_color),
        latin: element.child("a:latin").and_then(read_typeface),
        east_asian: element.child("a:ea").and_then(read_typeface),
        complex: element.child("a:cs").and_then(read_typeface),
        symbol: element.child("a:sym").and_then(read_typeface),
        extra,
    }
}

pub fn read_paragraph(element: &Element, context: &ReadContext<'_>) -> TextParagraph {
    let mut paragraph = TextParagraph {
        props: element
            .child("a:pPr")
            .map(|props| read_paragraph_props(props, context))
            .unwrap_or_default(),
        runs: Vec::new(),
        end_props: element
            .child("a:endParaRPr")
            .map(|props| read_run_props(props, context)),
        source: None,
    };
    for child in element.elements() {
        let run_props = || {
            child
                .child("a:rPr")
                .map(|props| read_run_props(props, context))
                .unwrap_or_default()
        };
        let text = || child.child("a:t").map(element_text).unwrap_or_default();
        match child.name.as_str() {
            "a:pPr" | "a:endParaRPr" => {}
            "a:r" => paragraph.runs.push(TextRun::Text {
                text: text(),
                props: run_props(),
            }),
            "a:br" => paragraph.runs.push(TextRun::Break { props: run_props() }),
            "a:fld" => paragraph.runs.push(TextRun::Field {
                kind: field_kind(child.attr("type").unwrap_or("")),
                id: child.attr("id").map(str::to_string),
                text: text(),
                props: run_props(),
            }),
            _ => paragraph.runs.push(TextRun::Preserved {
                xml: context.raw(child),
                text: child.deep_text(),
                props: RunProps::default(),
            }),
        }
    }
    paragraph.source = Some(source_of(
        SourceFormat::Pptx,
        &context.raw(element),
        &paragraph,
        Vec::new(),
    ));
    paragraph
}

pub(super) fn field_kind(kind: &str) -> FieldKind {
    match kind {
        "slidenum" => FieldKind::SlideNumber,
        kind if kind.starts_with("datetime") => FieldKind::DateTime(kind.to_string()),
        other => FieldKind::Other(other.to_string()),
    }
}
