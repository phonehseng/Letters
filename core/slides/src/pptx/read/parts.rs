//! the parts of a pptx deck that hold shapes: masters (with their themes
//! and text styles), layouts and slides, with backgrounds and colour maps.

use crate::xml::Element;

use super::extras::{read_animations, read_comments, read_notes, read_transition};
use super::Reader;
use crate::error::SlidesError;
use crate::model::*;
use crate::pptx::dml::{attr_bool, find_color, find_fill, ReadContext};
use crate::pptx::objects::read_objects;
use crate::pptx::rels::{self};
use crate::pptx::text::read_text_style;
use crate::pptx::theme::read_theme;
use crate::preserve::part_source_of;

/// a master, its theme and its layouts.
pub(super) fn read_master_tree(
    reader: &Reader,
    part: &str,
    presentation: &mut Presentation,
) -> Result<(), SlidesError> {
    let Some(document) = reader.xml(part)? else {
        return Ok(());
    };
    let master_rels = reader.rels(part);
    let context = ReadContext {
        part,
        text: &document.source,
        rels: &master_rels,
        media: &reader.media,
    };
    let root = &document.root;
    let theme_id = master_rels
        .first_of(rels::THEME)
        .map(|rel| rel.resolved(part));
    if let Some(theme_part) = &theme_id {
        if presentation.theme(theme_part).is_none() {
            if let Some(theme_document) = reader.xml(theme_part)? {
                let theme_rels = reader.rels(theme_part);
                let theme_context = ReadContext {
                    part: theme_part,
                    text: &theme_document.source,
                    rels: &theme_rels,
                    media: &reader.media,
                };
                presentation
                    .themes
                    .push(read_theme(theme_part, &theme_document, &theme_context));
            }
        }
    }
    let common = root.child("p:cSld");
    let styles = root.child("p:txStyles");
    let style = |name: &str| {
        styles
            .and_then(|s| s.child(name))
            .map(|s| read_text_style(s, &context))
            .unwrap_or_default()
    };
    let layout_parts: Vec<String> = root
        .child("p:sldLayoutIdLst")
        .map(|list| {
            list.children_named("p:sldLayoutId")
                .filter_map(|entry| entry.attr("r:id"))
                .filter_map(|id| master_rels.get(id))
                .map(|rel| rel.resolved(part))
                .collect()
        })
        .unwrap_or_default();
    let mut master = Master {
        id: part.to_string(),
        name: common
            .and_then(|c| c.attr("name"))
            .unwrap_or("")
            .to_string(),
        theme_id,
        background: common
            .and_then(|c| c.child("p:bg"))
            .map(|bg| read_background(bg, &context)),
        objects: common
            .and_then(|c| c.child("p:spTree"))
            .map(|tree| read_objects(tree, &context))
            .unwrap_or_default(),
        color_map: root
            .child("p:clrMap")
            .map(read_color_map)
            .unwrap_or_default(),
        title_style: style("p:titleStyle"),
        body_style: style("p:bodyStyle"),
        other_style: style("p:otherStyle"),
        layout_ids: layout_parts.clone(),
        source: None,
    };
    let styles = vec![style_piece(&master)];
    master.source = Some(part_source_of(
        SourceFormat::Pptx,
        part,
        &document.source,
        &master,
        styles,
    ));
    presentation.masters.push(master);
    for layout_part in layout_parts {
        if presentation.layout(&layout_part).is_none() {
            if let Some(layout) = read_layout(reader, &layout_part, part)? {
                presentation.layouts.push(layout);
            }
        }
    }
    Ok(())
}

pub(super) fn read_layout(
    reader: &Reader,
    part: &str,
    master_id: &str,
) -> Result<Option<Layout>, SlidesError> {
    let Some(document) = reader.xml(part)? else {
        return Ok(None);
    };
    let layout_rels = reader.rels(part);
    let context = ReadContext {
        part,
        text: &document.source,
        rels: &layout_rels,
        media: &reader.media,
    };
    let root = &document.root;
    let common = root.child("p:cSld");
    let mut layout = Layout {
        id: part.to_string(),
        name: common
            .and_then(|c| c.attr("name"))
            .unwrap_or("")
            .to_string(),
        master_id: layout_rels
            .first_of(rels::SLIDE_MASTER)
            .map(|rel| rel.resolved(part))
            .unwrap_or_else(|| master_id.to_string()),
        kind: root.attr("type").map(str::to_string),
        background: common
            .and_then(|c| c.child("p:bg"))
            .map(|bg| read_background(bg, &context)),
        objects: common
            .and_then(|c| c.child("p:spTree"))
            .map(|tree| read_objects(tree, &context))
            .unwrap_or_default(),
        color_map: read_color_map_override(root),
        show_master_shapes: attr_bool(root, "showMasterSp").unwrap_or(true),
        source: None,
    };
    layout.source = Some(part_source_of(
        SourceFormat::Pptx,
        part,
        &document.source,
        &layout,
        Vec::new(),
    ));
    Ok(Some(layout))
}

pub(super) fn read_slide(
    reader: &Reader,
    part: &str,
    number: Option<u32>,
    presentation: &mut Presentation,
) -> Result<Option<Slide>, SlidesError> {
    let Some(document) = reader.xml(part)? else {
        return Ok(None);
    };
    let slide_rels = reader.rels(part);
    let context = ReadContext {
        part,
        text: &document.source,
        rels: &slide_rels,
        media: &reader.media,
    };
    let root = &document.root;
    let common = root.child("p:cSld");
    let layout_id = slide_rels
        .first_of(rels::SLIDE_LAYOUT)
        .map(|rel| rel.resolved(part));
    // a slide can name a layout that no master lists; read it anyway
    if let Some(layout_part) = &layout_id {
        if presentation.layout(layout_part).is_none() {
            let master = presentation
                .masters
                .first()
                .map(|m| m.id.clone())
                .unwrap_or_default();
            if let Some(layout) = read_layout(reader, layout_part, &master)? {
                presentation.layouts.push(layout);
            }
        }
    }
    let id = match number {
        Some(number) => format!("slide-{number}"),
        None => format!("slide-{}", presentation.slides.len() + 1),
    };
    let mut slide = Slide {
        id,
        number_id: number,
        name: common.and_then(|c| c.attr("name")).map(str::to_string),
        layout_id,
        background: common
            .and_then(|c| c.child("p:bg"))
            .map(|bg| read_background(bg, &context)),
        objects: common
            .and_then(|c| c.child("p:spTree"))
            .map(|tree| read_objects(tree, &context))
            .unwrap_or_default(),
        notes: read_notes(reader, part, &slide_rels)?,
        transition: read_transition(root, &document.source),
        animations: root
            .child("p:timing")
            .map(read_animations)
            .unwrap_or_default(),
        hidden: root.attr("show") == Some("0"),
        comments: read_comments(reader, part, &slide_rels),
        color_map: read_color_map_override(root),
        show_master_shapes: attr_bool(root, "showMasterSp").unwrap_or(true),
        source: None,
    };
    slide.source = Some(part_source_of(
        SourceFormat::Pptx,
        part,
        &document.source,
        &slide,
        Vec::new(),
    ));
    Ok(Some(slide))
}

pub fn read_background(element: &Element, context: &ReadContext<'_>) -> Background {
    let properties = element.child("p:bgPr");
    let reference = element.child("p:bgRef");
    Background {
        fill: properties.and_then(|p| find_fill(p, context)),
        style_ref: reference.map(|r| StyleRef {
            index: r.attr("idx").and_then(|i| i.parse().ok()).unwrap_or(0),
            color: find_color(r),
        }),
        xml: Some(context.raw(element)),
    }
}

pub fn read_color_map(element: &Element) -> ColorMap {
    ColorMap {
        entries: element
            .attributes
            .iter()
            .filter(|(key, _)| !key.starts_with("xmlns"))
            .cloned()
            .collect(),
    }
}

pub(super) fn read_color_map_override(root: &Element) -> Option<ColorMap> {
    root.child("p:clrMapOvr")?
        .child("a:overrideClrMapping")
        .map(read_color_map)
}

/// changed keeps its original `p:txStyles` xml.
pub fn style_piece(master: &Master) -> (String, String) {
    let styles = (&master.title_style, &master.body_style, &master.other_style);
    ("styles".to_string(), crate::model::fingerprint_of(&styles))
}
