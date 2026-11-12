//! writing themes, masters and layouts: each is copied while its model is
//! unchanged and rebuilt from its original xml otherwise.

use std::collections::HashMap;

use super::super::dml::WriteContext;
use super::super::read::style_piece;
use super::super::rels::{self, Rels};
use super::super::text::write_text_style;
use super::super::theme::{default_theme_xml, write_theme};
use super::canvas::{
    fresh_canvas, patched_part, with_root_attributes, Canvas, NAMESPACES, XML_DECLARATION,
};
use super::names::PartNames;
use super::package::OutPackage;
use crate::model::{Layout, Master, Presentation, Source, SourceFormat};
use crate::preserve::reusable;

pub use super::slides::write_slides;

pub const THEME_TYPE: &str = "application/vnd.openxmlformats-officedocument.theme+xml";
pub const MASTER_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml";
pub const LAYOUT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";

/// the first id a master or layout may use in `p:sldMasterIdLst` and
/// `p:sldLayoutIdLst` (ecma-376 part 1, 19.2.1.27: ids start at 2^31)
pub const FIRST_MASTER_ID: u64 = 2_147_483_648;

/// the number a layout gets in its master's `p:sldLayoutIdLst`: unique
/// across the deck, and above every master's number (see `master_list_id`).
fn layout_list_id(presentation: &Presentation, layout_id: &str) -> Option<u64> {
    let index = presentation
        .layouts
        .iter()
        .position(|l| l.id == layout_id)?;
    Some(FIRST_MASTER_ID + presentation.masters.len() as u64 + index as u64)
}

/// the number a master gets in `p:sldMasterIdLst`.
pub fn master_list_id(index: usize) -> u64 {
    FIRST_MASTER_ID + index as u64
}

pub fn write_themes(presentation: &Presentation, names: &PartNames, package: &mut OutPackage) {
    for theme in &presentation.themes {
        let part = &names.themes[&theme.id];
        let text = match &theme.source {
            Some(source) if source.format == SourceFormat::Pptx => write_theme(theme),
            _ => None,
        };
        let text = text.unwrap_or_else(|| fresh_theme(theme));
        if package.text(part).as_deref() != Some(text.as_str()) {
            package.set_text(part, &text);
        }
        package.content_types.set_override(part, THEME_TYPE);
    }
}

/// a theme part for a theme that has no pptx original: the default
/// theme's format scheme with this theme's colours and fonts.
fn fresh_theme(theme: &crate::model::Theme) -> String {
    let mut seeded = theme.clone();
    let document = default_theme_xml();
    seeded.source = Some(crate::preserve::source_of(
        SourceFormat::Pptx,
        &document,
        &(),
        Vec::new(),
    ));
    write_theme(&seeded).unwrap_or(document)
}

/// writes the master's copy of a theme another master also uses.
fn copy_shared_theme(master: &Master, names: &PartNames, own: &str, package: &mut OutPackage) {
    let shared = master.theme_id.as_ref().and_then(|id| names.themes.get(id));
    if shared.is_some_and(|shared| shared != own) {
        if let Some(text) = shared.and_then(|shared| package.text(shared)) {
            package.set_text(own, &text);
            package.content_types.set_override(own, THEME_TYPE);
        }
    }
}

/// the original text of a part when its model is unchanged.
fn unchanged_text<'a, T: serde::Serialize>(
    source: Option<&'a Source>,
    plain: &T,
) -> Option<&'a str> {
    reusable(source, SourceFormat::Pptx, plain)
}

fn pptx_source(source: Option<&Source>) -> Option<&Source> {
    source.filter(|source| source.format == SourceFormat::Pptx)
}

pub fn write_masters(
    presentation: &Presentation,
    names: &PartNames,
    media: &HashMap<String, String>,
    package: &mut OutPackage,
) {
    for master in &presentation.masters {
        let part = names.masters[&master.id].clone();
        let mut rels = package.rels_of(&part);
        if let Some(theme) = names.master_themes.get(&master.id) {
            copy_shared_theme(master, names, theme, package);
            rels.set_single(&part, theme, rels::THEME);
        }
        let layout_rel_ids: Vec<(u64, String)> = master
            .layout_ids
            .iter()
            .filter_map(|id| Some((layout_list_id(presentation, id)?, names.layouts.get(id)?)))
            .map(|(number, layout)| (number, rels.ensure(&part, layout, rels::SLIDE_LAYOUT)))
            .collect();
        let mut context = WriteContext {
            part: part.clone(),
            rels,
            media_parts: media,
        };
        let text = master_text(master, &layout_rel_ids, &mut context);
        package.set_text(&part, &text);
        package.set_rels(&part, &context.rels);
        package.content_types.set_override(&part, MASTER_TYPE);
    }
}

fn master_text(
    master: &Master,
    layout_rel_ids: &[(u64, String)],
    context: &mut WriteContext<'_>,
) -> String {
    let mut plain = master.clone();
    plain.source = None;
    let source = pptx_source(master.source.as_ref());
    if let Some(text) = unchanged_text(source, &plain) {
        return text.to_string();
    }
    let canvas = Canvas {
        name: Some(master.name.as_str()).filter(|n| !n.is_empty()),
        background: master.background.as_ref(),
        objects: &master.objects,
    };
    let Some(source) = source else {
        return fresh_master(master, &canvas, layout_rel_ids, context);
    };
    let Some(mut text) = patched_part(&source.xml, &canvas, context) else {
        return source.xml.clone();
    };
    let (name, value) = style_piece(master);
    if source.piece(&name) != Some(value.as_str()) {
        text = with_new_text_styles(&text, master, context);
    }
    text
}

fn text_styles(master: &Master, context: &mut WriteContext<'_>) -> String {
    format!(
        "<p:txStyles>{}{}{}</p:txStyles>",
        write_text_style(&master.title_style, "p:titleStyle", context),
        write_text_style(&master.body_style, "p:bodyStyle", context),
        write_text_style(&master.other_style, "p:otherStyle", context)
    )
}

fn with_new_text_styles(text: &str, master: &Master, context: &mut WriteContext<'_>) -> String {
    let Ok(document) = crate::xml::parse(text) else {
        return text.to_string();
    };
    let Some(styles) = document.root.child("p:txStyles") else {
        return text.to_string();
    };
    let mut out = text.to_string();
    out.replace_range(styles.start..styles.end, &text_styles(master, context));
    out
}

fn fresh_master(
    master: &Master,
    canvas: &Canvas<'_>,
    layout_rel_ids: &[(u64, String)],
    context: &mut WriteContext<'_>,
) -> String {
    let color_map: String = master
        .color_map
        .entries
        .iter()
        .map(|(key, value)| format!(" {key}=\"{value}\""))
        .collect();
    let layout_list: String = layout_rel_ids
        .iter()
        .map(|(id, rel)| format!("<p:sldLayoutId id=\"{id}\" r:id=\"{rel}\"/>"))
        .collect();
    format!(
        "{XML_DECLARATION}<p:sldMaster{NAMESPACES}>{}<p:clrMap{color_map}/><p:sldLayoutIdLst>{layout_list}</p:sldLayoutIdLst>{}</p:sldMaster>",
        fresh_canvas(canvas, context),
        text_styles(master, context)
    )
}

pub fn write_layouts(
    presentation: &Presentation,
    names: &PartNames,
    media: &HashMap<String, String>,
    package: &mut OutPackage,
) {
    for layout in &presentation.layouts {
        let part = names.layouts[&layout.id].clone();
        let mut rels: Rels = package.rels_of(&part);
        if let Some(master) = names.masters.get(&layout.master_id) {
            rels.set_single(&part, master, rels::SLIDE_MASTER);
        }
        let mut context = WriteContext {
            part: part.clone(),
            rels,
            media_parts: media,
        };
        let text = layout_text(layout, &mut context);
        package.set_text(&part, &text);
        package.set_rels(&part, &context.rels);
        package.content_types.set_override(&part, LAYOUT_TYPE);
    }
}

fn layout_text(layout: &Layout, context: &mut WriteContext<'_>) -> String {
    let mut plain = layout.clone();
    plain.source = None;
    let source = pptx_source(layout.source.as_ref());
    if let Some(text) = unchanged_text(source, &plain) {
        return text.to_string();
    }
    let canvas = Canvas {
        name: Some(layout.name.as_str()).filter(|n| !n.is_empty()),
        background: layout.background.as_ref(),
        objects: &layout.objects,
    };
    let show_master = (!layout.show_master_shapes).then(|| "0".to_string());
    match source.and_then(|s| patched_part(&s.xml, &canvas, context)) {
        Some(text) => with_root_attributes(&text, &[("showMasterSp", show_master)]),
        None => {
            let kind = layout
                .kind
                .as_ref()
                .map(|k| format!(" type=\"{k}\""))
                .unwrap_or_default();
            let show = show_master
                .map(|v| format!(" showMasterSp=\"{v}\""))
                .unwrap_or_default();
            format!(
                "{XML_DECLARATION}<p:sldLayout{NAMESPACES}{kind}{show} preserve=\"1\">{}<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",
                fresh_canvas(&canvas, context)
            )
        }
    }
}
