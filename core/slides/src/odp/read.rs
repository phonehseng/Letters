//! reading an opendocument presentation (.odp) into the model: master
//! pages become masters (each with one layout), pages become slides, and
//! every part, page and object keeps its original xml for the writer.

use std::collections::HashMap;

use super::images::natural_size;
use super::read_paint::read_fill;
use super::read_shapes::{read_objects, ShapeContext};
use super::read_text::{run_props, TextContext};
use super::styles::{get, StyleSheet};
use super::units::length;
use crate::error::SlidesError;
use crate::model::{
    Background, CarriedPart, Carry, Layout, LevelStyle, Master, MediaItem, Notes, Object,
    ParagraphProps, Presentation, Slide, SlideSize, SourceFormat, Spacing, TextAlign, TextBody,
    TextStyle, Theme, Transition,
};
use crate::package::Package;
use crate::preserve::{part_source_of, source_of};
use crate::xml::{self, Element, XmlDocument};

/// the id of the one theme an odp deck gets
pub const THEME_ID: &str = "odp-theme";

pub fn read_odp(bytes: &[u8]) -> Result<Presentation, SlidesError> {
    let package = Package::read(bytes)?;
    let mimetype = package
        .bytes("mimetype")
        .map(|m| String::from_utf8_lossy(m).trim().to_string());
    if !mimetype
        .as_deref()
        .is_some_and(|m| m.starts_with("application/vnd.oasis.opendocument.presentation"))
    {
        return Err(SlidesError::NotAPresentation(
            "it is not an OpenDocument presentation".into(),
        ));
    }
    let content = parse_part(&package, "content.xml")?;
    if let Some(original) = unchanged_shadow(&package, &content) {
        return crate::pptx::read_pptx(original);
    }
    let styles_document = parse_part(&package, "styles.xml").ok();
    let mut styles = StyleSheet::default();
    if let Some(document) = &styles_document {
        styles.collect(&document.root);
    }
    styles.collect(&content.root);
    let (media, media_items, sizes) = collect_media(&package);
    let mut presentation = Presentation {
        media: media_items,
        ..Default::default()
    };
    if let Some(document) = &styles_document {
        read_masters(document, &styles, &media, &sizes, &mut presentation);
    }
    read_pages(&content, &styles, &media, &sizes, &mut presentation);
    presentation.carry = Carry {
        format: Some(SourceFormat::Odp),
        parts: package
            .parts
            .iter()
            .filter(|part| !media.contains_key(&part.name))
            .map(|part| CarriedPart {
                name: part.name.clone(),
                bytes: part.bytes.clone(),
            })
            .collect(),
        deck_pieces: Vec::new(),
        part_names: Vec::new(),
    };
    Ok(presentation)
}

/// the pptx this odp was converted from, when every page is still
/// exactly as letters wrote it (see `crate::shadow`).
fn unchanged_shadow<'a>(package: &'a Package, content: &XmlDocument) -> Option<&'a [u8]> {
    let source = package.bytes(crate::shadow::ODP_SHADOW)?;
    let recorded = String::from_utf8_lossy(package.bytes(crate::shadow::ODP_PAGES)?).to_string();
    let mut pages = Vec::new();
    content.root.walk(&mut |element| {
        if element.name == "draw:page" {
            pages.push(&content.source[element.start..element.end]);
        }
    });
    crate::shadow::unchanged(&recorded, pages.into_iter()).then_some(source)
}

fn parse_part(package: &Package, name: &str) -> Result<XmlDocument, SlidesError> {
    let bytes = package
        .bytes(name)
        .ok_or_else(|| SlidesError::Damaged(format!("{name} is missing")))?;
    let text = xml::decode_text(bytes).map_err(|detail| SlidesError::bad_xml(name, detail))?;
    xml::parse(&text).map_err(|detail| SlidesError::bad_xml(name, detail))
}

type MediaTables = (
    HashMap<String, String>,
    Vec<MediaItem>,
    HashMap<String, (i64, i64)>,
);

/// pictures and media by their path in the package (also their id).
fn collect_media(package: &Package) -> MediaTables {
    let mut ids = HashMap::new();
    let mut items = Vec::new();
    let mut sizes = HashMap::new();
    for part in &package.parts {
        let is_media = part.name.starts_with("Pictures/") || part.name.starts_with("Media/");
        if !is_media {
            continue;
        }
        ids.insert(part.name.clone(), part.name.clone());
        if let Some(size) = natural_size(&part.bytes) {
            sizes.insert(part.name.clone(), size);
        }
        items.push(MediaItem {
            id: part.name.clone(),
            content_type: MediaItem::content_type_for(&part.name).to_string(),
            source_path: Some(part.name.clone()),
            bytes: part.bytes.clone(),
        });
    }
    (ids, items, sizes)
}

fn read_masters(
    document: &XmlDocument,
    styles: &StyleSheet,
    media: &HashMap<String, String>,
    sizes: &HashMap<String, (i64, i64)>,
    presentation: &mut Presentation,
) {
    let mut masters = Vec::new();
    document.root.walk(&mut |element| {
        if element.name == "style:master-page" {
            masters.push(element.clone());
        }
    });
    let mut theme = Theme {
        id: THEME_ID.into(),
        name: "OpenDocument".into(),
        ..Default::default()
    };
    for (index, master_page) in masters.iter().enumerate() {
        if index == 0 {
            presentation.slide_size =
                page_size(&document.root, master_page.attr("style:page-layout-name"));
        }
        let master = read_master(master_page, document, styles, media, sizes);
        if index == 0 {
            theme.fonts.major_latin = font_of(&master.title_style).unwrap_or_default();
            theme.fonts.minor_latin = font_of(&master.body_style).unwrap_or_default();
        }
        presentation.layouts.push(Layout {
            id: format!("{}/layout", master.id),
            name: master.name.clone(),
            master_id: master.id.clone(),
            show_master_shapes: true,
            ..Default::default()
        });
        presentation.masters.push(master);
    }
    presentation.themes.push(theme);
}

fn font_of(style: &TextStyle) -> Option<String> {
    style
        .level(0)?
        .paragraph
        .run
        .as_ref()?
        .latin
        .as_ref()
        .map(|t| t.typeface.clone())
}

/// the page size of a page layout (`style:page-layout-properties`).
fn page_size(root: &Element, layout: Option<&str>) -> SlideSize {
    let mut size = SlideSize::default();
    root.walk(&mut |element| {
        if element.name == "style:page-layout" && element.attr("style:name") == layout {
            if let Some(properties) = element.child("style:page-layout-properties") {
                size.width = properties
                    .attr("fo:page-width")
                    .and_then(length)
                    .unwrap_or(size.width);
                size.height = properties
                    .attr("fo:page-height")
                    .and_then(length)
                    .unwrap_or(size.height);
            }
        }
    });
    size
}

fn read_master(
    element: &Element,
    document: &XmlDocument,
    styles: &StyleSheet,
    media: &HashMap<String, String>,
    sizes: &HashMap<String, (i64, i64)>,
) -> Master {
    let name = element.attr("style:name").unwrap_or("Default").to_string();
    let context = ShapeContext {
        styles,
        media,
        sizes,
        text: &document.source,
        next_id: std::cell::Cell::new(2),
    };
    let mut master = Master {
        id: name.clone(),
        name: element
            .attr("style:display-name")
            .unwrap_or(&name)
            .to_string(),
        theme_id: Some(THEME_ID.into()),
        background: page_background(styles, element.attr("draw:style-name"), &context),
        objects: master_placeholders(read_objects(element, &context)),
        title_style: level_styles(styles, &[format!("{name}-title")]),
        body_style: level_styles(
            styles,
            &(1..=9)
                .map(|level| format!("{name}-outline{level}"))
                .collect::<Vec<_>>(),
        ),
        layout_ids: vec![format!("{name}/layout")],
        ..Default::default()
    };
    master.source = Some(source_of(
        SourceFormat::Odp,
        &document.source[element.start..element.end],
        &master,
        Vec::new(),
    ));
    master
}

/// a slide master may only hold the title, body, date, footer and slide
/// number placeholders (powerpoint refuses a master with a subtitle, as
/// libreoffice's title-slide master pages have): any other placeholder
/// becomes the body when there is none yet, or a plain text box.
fn master_placeholders(mut objects: Vec<Object>) -> Vec<Object> {
    use crate::model::PlaceholderKind as Kind;
    let mut has_body = objects
        .iter()
        .any(|o| o.placeholder().is_some_and(|p| p.kind == Kind::Body));
    for object in &mut objects {
        let Object::Shape(shape) = object else {
            continue;
        };
        let Some(placeholder) = &mut shape.placeholder else {
            continue;
        };
        match placeholder.kind {
            Kind::Title | Kind::Body | Kind::Date | Kind::Footer | Kind::SlideNumber => {}
            Kind::CenteredTitle => placeholder.kind = Kind::Title,
            _ if !has_body => {
                placeholder.kind = Kind::Body;
                has_body = true;
            }
            _ => {
                shape.placeholder = None;
                shape.text_box = true;
                // it is still a placeholder to libreoffice, which never draws a
                // master's placeholders on its slides; drawn as a plain text
                // box its prompt ("Click to edit Master subtitle style") sat
                // over every title slide's own subtitle (lo_theme_facet)
                shape.hidden = true;
            }
        }
    }
    objects
}

/// presentation styles as per-level text styles, one style per level.
fn level_styles(styles: &StyleSheet, names: &[String]) -> TextStyle {
    let levels = names
        .iter()
        .map(|name| {
            let style = styles.resolve("presentation", Some(name));
            let context = TextContext {
                styles,
                paragraph: style.paragraph.clone(),
                text: style.text.clone(),
            };
            let paragraph = ParagraphProps {
                align: get(&context.paragraph, "fo:text-align").map(|a| match a {
                    "center" => TextAlign::Center,
                    "end" | "right" => TextAlign::Right,
                    _ => TextAlign::Left,
                }),
                margin_left: get(&context.paragraph, "fo:margin-left").and_then(length),
                indent: get(&context.paragraph, "fo:text-indent").and_then(length),
                space_before: get(&context.paragraph, "fo:margin-top")
                    .and_then(length)
                    .map(|emu| Spacing::Points((emu * 100 / 12_700) as i32)),
                run: Some(run_props(&context.text)),
                ..Default::default()
            };
            Some(LevelStyle { paragraph })
        })
        .collect();
    TextStyle {
        levels,
        ..Default::default()
    }
}

/// a page's or master's background from its drawing-page style.
fn page_background(
    styles: &StyleSheet,
    name: Option<&str>,
    context: &ShapeContext<'_>,
) -> Option<Background> {
    let style = styles.resolve("drawing-page", name);
    let fill = read_fill(&style.page, context)?;
    Some(Background {
        fill: Some(fill),
        style_ref: None,
        xml: None,
    })
}

fn read_pages(
    content: &XmlDocument,
    styles: &StyleSheet,
    media: &HashMap<String, String>,
    sizes: &HashMap<String, (i64, i64)>,
    presentation: &mut Presentation,
) {
    let mut pages = Vec::new();
    content.root.walk(&mut |element| {
        if element.name == "draw:page" {
            pages.push(element.clone());
        }
    });
    for (index, page) in pages.iter().enumerate() {
        let slide = read_page(page, index, content, styles, media, sizes);
        presentation.slides.push(slide);
    }
}

fn read_page(
    page: &Element,
    index: usize,
    content: &XmlDocument,
    styles: &StyleSheet,
    media: &HashMap<String, String>,
    sizes: &HashMap<String, (i64, i64)>,
) -> Slide {
    let context = ShapeContext {
        styles,
        media,
        sizes,
        text: &content.source,
        next_id: std::cell::Cell::new(2),
    };
    let master = page.attr("draw:master-page-name").unwrap_or("Default");
    let page_style = styles.resolve("drawing-page", page.attr("draw:style-name"));
    let mut slide = Slide {
        id: format!("page-{}", index + 1),
        name: page.attr("draw:name").map(str::to_string),
        layout_id: Some(format!("{master}/layout")),
        background: page_background(styles, page.attr("draw:style-name"), &context),
        objects: read_objects(page, &context),
        notes: read_notes(page, &context),
        transition: transition(&page_style.page),
        hidden: get(&page_style.page, "presentation:visibility") == Some("hidden"),
        show_master_shapes: true,
        ..Default::default()
    };
    let raw = &content.source[page.start..page.end];
    slide.source = Some(part_source_of(
        SourceFormat::Odp,
        "content.xml",
        raw,
        &slide,
        Vec::new(),
    ));
    slide
}

/// the transition named by a page style (kept, not modelled further).
fn transition(page: &super::styles::Properties) -> Option<Transition> {
    let kind = get(page, "smil:type").or_else(|| get(page, "presentation:transition-style"))?;
    Some(Transition {
        kind: kind.to_string(),
        duration_ms: get(page, "presentation:transition-speed").map(|speed| match speed {
            "fast" => 500,
            "slow" => 1000,
            _ => 750,
        }),
        xml: page
            .iter()
            .filter(|(k, _)| k.starts_with("smil:") || k.starts_with("presentation:transition"))
            .map(|(k, v)| format!("{k}=\"{}\"", xml::escape_attribute(v)))
            .collect::<Vec<_>>()
            .join(" "),
        format: SourceFormat::Odp,
    })
}

fn read_notes(page: &Element, context: &ShapeContext<'_>) -> Option<Notes> {
    let notes = page.child("presentation:notes")?;
    let objects = read_objects(notes, context);
    let text = objects
        .into_iter()
        .find_map(|object| match object {
            Object::Shape(shape) if shape.placeholder.is_some() => shape.text,
            _ => None,
        })
        .unwrap_or_else(TextBody::default);
    let mut model = Notes { text, source: None };
    let raw = &context.text[notes.start..notes.end];
    model.source = Some(source_of(SourceFormat::Odp, raw, &model, Vec::new()));
    Some(model)
}
