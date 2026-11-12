//! the model's text written as drawingml; unchanged paragraphs keep
//! their original xml.

use crate::xml::escape_text;

use crate::model::{
    Autofit, BodyProps, Bullet, BulletColor, BulletFont, BulletSize, FieldKind, ParagraphProps,
    RunProps, SourceFormat, Spacing, TextBody, TextParagraph, TextRun, TextStyle, Typeface,
};
use crate::pptx::dml::{
    attr, bool_text, extra_attributes, opt_attr, ordered_children, write_color, write_fill,
    WriteContext, FILL_NAMES,
};
use crate::preserve::reusable;

use super::read::{align_name, anchor_name};
use super::*;

/// writes a text body under `name` (`p:txBody` for shapes, `a:txBody`
/// in table cells). unchanged paragraphs keep their original xml.
pub fn write_text_body(body: &TextBody, name: &str, context: &mut WriteContext<'_>) -> String {
    let mut out = format!("<{name}>");
    out.push_str(&write_body(&body.body));
    out.push_str(&write_text_style(&body.list_style, "a:lstStyle", context));
    for paragraph in &body.paragraphs {
        out.push_str(&write_paragraph(paragraph, context));
    }
    if body.paragraphs.is_empty() {
        // a text body needs at least one paragraph
        out.push_str("<a:p><a:endParaRPr lang=\"en-US\" dirty=\"0\"/></a:p>");
    }
    out.push_str(&format!("</{name}>"));
    out
}

pub fn write_body(body: &BodyProps) -> String {
    let mut out = format!(
        "<a:bodyPr{}{}{}{}{}{}{}{}{}{}{}{}",
        opt_attr("rot", body.rotation),
        opt_attr("vert", body.vertical.as_ref()),
        opt_attr(
            "wrap",
            body.wrap.map(|wrap| if wrap { "square" } else { "none" })
        ),
        opt_attr("lIns", body.inset_left),
        opt_attr("tIns", body.inset_top),
        opt_attr("rIns", body.inset_right),
        opt_attr("bIns", body.inset_bottom),
        opt_attr("numCol", body.columns),
        opt_attr("spcCol", body.column_spacing),
        opt_attr("anchor", body.anchor.map(anchor_name)),
        opt_attr("anchorCtr", body.anchor_center.map(bool_text)),
        extra_attributes(&body.extra),
    );
    let autofit = body.autofit.as_ref().map(|autofit| match autofit {
        Autofit::None => "<a:noAutofit/>".to_string(),
        Autofit::Shrink {
            font_scale,
            line_reduction,
        } => format!(
            "<a:normAutofit{}{}/>",
            opt_attr("fontScale", *font_scale),
            opt_attr("lnSpcReduction", *line_reduction)
        ),
        Autofit::ResizeShape => "<a:spAutoFit/>".to_string(),
    });
    let children = ordered_children(&[(&AUTOFIT, autofit)], &body.extra);
    if children.is_empty() {
        out.push_str("/>");
    } else {
        out.push('>');
        out.push_str(&children);
        out.push_str("</a:bodyPr>");
    }
    out
}

pub fn write_text_style(style: &TextStyle, name: &str, context: &mut WriteContext<'_>) -> String {
    if style.is_empty() {
        return format!("<{name}/>");
    }
    let mut out = format!("<{name}>");
    if let Some(default) = &style.default {
        out.push_str(&write_paragraph_props(
            &default.paragraph,
            "a:defPPr",
            context,
        ));
    }
    for (index, level) in style.levels.iter().enumerate() {
        if let Some(level) = level {
            let element = format!("a:lvl{}pPr", index + 1);
            out.push_str(&write_paragraph_props(&level.paragraph, &element, context));
        }
    }
    for extra in &style.extra {
        out.push_str(&extra.xml);
    }
    out.push_str(&format!("</{name}>"));
    out
}

pub(super) fn write_spacing(name: &str, spacing: Option<Spacing>) -> Option<String> {
    spacing.map(|spacing| match spacing {
        Spacing::Percent(value) => {
            format!("<a:{name}><a:spcPct{}/></a:{name}>", attr("val", value))
        }
        Spacing::Points(value) => format!("<a:{name}><a:spcPts{}/></a:{name}>", attr("val", value)),
    })
}

pub(super) fn write_typeface(name: &str, typeface: &Typeface) -> String {
    let mut out = format!("<{name}{}", attr("typeface", &typeface.typeface));
    for (key, value) in &typeface.attributes {
        out.push_str(&attr(key, value));
    }
    out.push_str("/>");
    out
}

pub fn write_paragraph_props(
    props: &ParagraphProps,
    name: &str,
    context: &mut WriteContext<'_>,
) -> String {
    let attributes = format!(
        "{}{}{}{}{}{}{}{}",
        opt_attr("marL", props.margin_left),
        opt_attr("marR", props.margin_right),
        opt_attr("lvl", props.level.filter(|_| name == "a:pPr")),
        opt_attr("indent", props.indent),
        opt_attr("algn", props.align.map(align_name)),
        opt_attr("defTabSz", props.default_tab),
        opt_attr("rtl", props.rtl.map(bool_text)),
        extra_attributes(&props.extra),
    );
    let bullet_color = props.bullet_color.as_ref().map(|color| match color {
        BulletColor::FollowText => "<a:buClrTx/>".to_string(),
        BulletColor::Color(color) => format!("<a:buClr>{}</a:buClr>", write_color(color)),
    });
    let bullet_size = props.bullet_size.map(|size| match size {
        BulletSize::FollowText => "<a:buSzTx/>".to_string(),
        BulletSize::Percent(value) => format!("<a:buSzPct{}/>", attr("val", value)),
        BulletSize::Points(value) => format!("<a:buSzPts{}/>", attr("val", value)),
    });
    let bullet_font = props.bullet_font.as_ref().map(|font| match font {
        BulletFont::FollowText => "<a:buFontTx/>".to_string(),
        BulletFont::Font(typeface) => write_typeface("a:buFont", typeface),
    });
    let bullet = props.bullet.as_ref().map(|bullet| match bullet {
        Bullet::None => "<a:buNone/>".to_string(),
        Bullet::Char { char } => format!("<a:buChar{}/>", attr("char", char)),
        Bullet::AutoNumber { scheme, start_at } => format!(
            "<a:buAutoNum{}{}/>",
            attr("type", scheme),
            opt_attr("startAt", *start_at)
        ),
        Bullet::Picture { xml, .. } => xml.clone(),
    });
    let tabs = props.tabs.as_ref().map(|tabs| {
        let mut out = String::from("<a:tabLst>");
        for tab in tabs {
            out.push_str(&format!(
                "<a:tab{}{}/>",
                attr("pos", tab.position),
                attr("algn", &tab.align)
            ));
        }
        out.push_str("</a:tabLst>");
        out
    });
    let run = props
        .run
        .as_ref()
        .map(|run| write_run_props(run, "a:defRPr", context));
    let children = ordered_children(
        &[
            (&LINE_SPACING, write_spacing("lnSpc", props.line_spacing)),
            (&SPACE_BEFORE, write_spacing("spcBef", props.space_before)),
            (&SPACE_AFTER, write_spacing("spcAft", props.space_after)),
            (&BULLET_COLOR, bullet_color),
            (&BULLET_SIZE, bullet_size),
            (&BULLET_FONT, bullet_font),
            (&BULLET, bullet),
            (&TABS, tabs),
            (&DEFAULT_RUN, run),
        ],
        &props.extra,
    );
    if children.is_empty() {
        format!("<{name}{attributes}/>")
    } else {
        format!("<{name}{attributes}>{children}</{name}>")
    }
}

pub fn write_run_props(props: &RunProps, name: &str, context: &mut WriteContext<'_>) -> String {
    let attributes = format!(
        "{}{}{}{}{}{}{}{}{}{}{}",
        opt_attr("lang", props.lang.as_ref()),
        opt_attr("sz", props.size),
        opt_attr("b", props.bold.map(bool_text)),
        opt_attr("i", props.italic.map(bool_text)),
        opt_attr("u", props.underline.as_ref()),
        opt_attr("strike", props.strike.as_ref()),
        opt_attr("kern", props.kern),
        opt_attr("cap", props.caps.as_ref()),
        opt_attr("spc", props.spacing),
        opt_attr("baseline", props.baseline),
        extra_attributes(&props.extra),
    );
    let fill = props.fill.as_ref().map(|fill| write_fill(fill, context));
    let highlight = props
        .highlight
        .as_ref()
        .map(|color| format!("<a:highlight>{}</a:highlight>", write_color(color)));
    let face = |name: &str, typeface: &Option<Typeface>| {
        typeface
            .as_ref()
            .map(|typeface| write_typeface(name, typeface))
    };
    let children = ordered_children(
        &[
            (&FILL_NAMES, fill),
            (&HIGHLIGHT, highlight),
            (&LATIN, face("a:latin", &props.latin)),
            (&EAST_ASIAN, face("a:ea", &props.east_asian)),
            (&COMPLEX, face("a:cs", &props.complex)),
            (&SYMBOL, face("a:sym", &props.symbol)),
        ],
        &props.extra,
    );
    if children.is_empty() {
        format!("<{name}{attributes}/>")
    } else {
        format!("<{name}{attributes}>{children}</{name}>")
    }
}

pub fn write_paragraph(paragraph: &TextParagraph, context: &mut WriteContext<'_>) -> String {
    let mut plain = paragraph.clone();
    plain.source = None;
    if let Some(xml) = reusable(paragraph.source.as_ref(), SourceFormat::Pptx, &plain) {
        return xml.to_string();
    }
    let mut out = String::from("<a:p>");
    if paragraph.props != ParagraphProps::default() {
        out.push_str(&write_paragraph_props(&paragraph.props, "a:pPr", context));
    }
    for run in &paragraph.runs {
        out.push_str(&write_run(run, context));
    }
    if let Some(end) = &paragraph.end_props {
        out.push_str(&write_run_props(end, "a:endParaRPr", context));
    }
    out.push_str("</a:p>");
    out
}

pub(super) fn write_run(run: &TextRun, context: &mut WriteContext<'_>) -> String {
    let props =
        |props: &RunProps, context: &mut WriteContext<'_>| write_run_props(props, "a:rPr", context);
    match run {
        TextRun::Text { text, props: run } => format!(
            "<a:r>{}<a:t>{}</a:t></a:r>",
            props(run, context),
            escape_text(text)
        ),
        TextRun::Break { props: run } => format!("<a:br>{}</a:br>", props(run, context)),
        TextRun::Field {
            kind,
            id,
            text,
            props: run,
        } => {
            let kind = match kind {
                FieldKind::SlideNumber => "slidenum".to_string(),
                FieldKind::DateTime(kind) | FieldKind::Other(kind) => kind.clone(),
            };
            format!(
                "<a:fld{}{}>{}<a:t>{}</a:t></a:fld>",
                attr("id", id.clone().unwrap_or_else(|| field_guid(&kind))),
                attr("type", kind),
                props(run, context),
                escape_text(text)
            )
        }
        TextRun::Preserved { xml, .. } => xml.clone(),
    }
}

/// a stable guid for a new field (fields need one; any unique value works).
pub(super) fn field_guid(kind: &str) -> String {
    let hash = crate::model::fingerprint_of(&kind);
    format!(
        "{{{}-{}-4{}-8{}-{}}}",
        &hash[0..8],
        &hash[8..12],
        &hash[12..15],
        &hash[13..16],
        "B4F2A1C3D5E6"
    )
    .to_uppercase()
}
