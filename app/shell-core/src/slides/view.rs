//! what the slide editor's interface is shown of a deck: the slide list,
//! the layouts and theme, and each slide's objects with their boxes in
//! points, already resolved through placeholders and the theme.
//!
//! the model itself carries every part's original xml, which the
//! interface never needs, so it gets these small summaries instead and
//! sends edits back as the engine's operations.

use letters_slides::color::{resolve, ColorContext};
use letters_slides::inherit::{effective_transform, Chain};
use letters_slides::model::{
    ColorMap, ColorScheme, GraphicKind, Object, Presentation, Slide, EMU_PER_POINT,
};
use serde::Serialize;

use super::view_text::{table_view, text_view, TableView, TextView};

/// sixty thousand units to the degree, as pptx stores angles
const UNITS_PER_DEGREE: f64 = 60_000.0;

pub fn points(emu: i64) -> f64 {
    emu as f64 / EMU_PER_POINT as f64
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckView {
    /// the slide size in points
    pub width: f64,
    pub height: f64,
    pub slides: Vec<SlideSummary>,
    pub layouts: Vec<LayoutChoice>,
    pub theme: Option<ThemeView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideSummary {
    pub id: String,
    pub hidden: bool,
    pub layout_id: Option<String>,
    /// the first title's words, for the rail's labels and screen readers
    pub title: String,
    pub notes: String,
    /// the transition's kind ("fade", "push"…), kept as it is
    pub transition: Option<String>,
    pub animation_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutChoice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeView {
    pub id: String,
    pub name: String,
    /// the twelve slots in pptx's order, as "#rrggbb"
    pub colors: Vec<ThemeColor>,
    pub major_font: String,
    pub minor_font: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeColor {
    pub slot: &'static str,
    pub hex: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideView {
    pub id: String,
    pub objects: Vec<ObjectView>,
}

/// one object on a slide, where it is drawn and what can be done with it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectView {
    pub id: u32,
    pub name: String,
    /// "shape", "textBox", "placeholder", "picture", "group", "connector",
    /// "table", "chart", "diagram", "embedded" or "kept"
    pub kind: &'static str,
    /// pptx's placeholder type ("title", "body"…) for placeholders
    pub placeholder: Option<&'static str>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// clockwise, in degrees
    pub rotation: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub text: Option<TextView>,
    pub table: Option<TableView>,
    /// a sentence for what the file keeps that Letters doesn't draw or edit
    pub kept: Option<String>,
}

/// the summary of a whole deck.
pub fn deck_view(presentation: &Presentation) -> DeckView {
    DeckView {
        width: points(presentation.slide_size.width),
        height: points(presentation.slide_size.height),
        slides: presentation.slides.iter().map(slide_summary).collect(),
        layouts: layout_choices(presentation),
        theme: theme_view(presentation),
    }
}

fn slide_summary(slide: &Slide) -> SlideSummary {
    let title = slide
        .objects
        .iter()
        .filter(|object| object.placeholder().is_some_and(|p| p.kind.is_title()))
        .find_map(|object| object.text().map(|text| text.plain_text()))
        .unwrap_or_default();
    SlideSummary {
        id: slide.id.clone(),
        hidden: slide.hidden,
        layout_id: slide.layout_id.clone(),
        title: title.trim().to_string(),
        notes: slide
            .notes
            .as_ref()
            .map(|n| n.plain_text())
            .unwrap_or_default(),
        transition: slide.transition.as_ref().map(|t| t.kind.clone()),
        animation_count: slide.animations.len(),
    }
}

/// every layout by name, in the file's order (powerpoint's New Slide menu
/// order); a layout without a name is called by its type.
fn layout_choices(presentation: &Presentation) -> Vec<LayoutChoice> {
    presentation
        .layouts
        .iter()
        .map(|layout| LayoutChoice {
            id: layout.id.clone(),
            name: if layout.name.is_empty() {
                layout.kind.clone().unwrap_or_else(|| "Layout".into())
            } else {
                layout.name.clone()
            },
        })
        .collect()
}

fn theme_view(presentation: &Presentation) -> Option<ThemeView> {
    let master = presentation
        .slides
        .first()
        .and_then(|slide| presentation.chain_of(slide).1)
        .or_else(|| presentation.masters.first());
    let theme = presentation.theme_of(master)?;
    let map = master.map(|m| m.color_map.clone()).unwrap_or_default();
    let context = ColorContext {
        scheme: &theme.colors,
        map: &map,
        placeholder: None,
    };
    let colors = ColorScheme::SLOTS
        .iter()
        .filter_map(|slot| {
            let color = theme.colors.slot(slot)?;
            Some(ThemeColor {
                slot,
                hex: format!("#{}", resolve(color, &context).to_hex()),
            })
        })
        .collect();
    Some(ThemeView {
        id: theme.id.clone(),
        name: theme.name.clone(),
        colors,
        major_font: theme.fonts.major_latin.clone(),
        minor_font: theme.fonts.minor_latin.clone(),
    })
}

/// the colours a slide's objects resolve against: the theme's scheme and
/// the colour map of the slide, else its layout's, else its master's.
pub fn slide_colors(presentation: &Presentation, slide: &Slide) -> (ColorScheme, ColorMap) {
    let chain = Chain::of_slide(presentation, slide);
    let map = slide
        .color_map
        .clone()
        .or_else(|| chain.layout.and_then(|l| l.color_map.clone()))
        .or_else(|| chain.master.map(|m| m.color_map.clone()))
        .unwrap_or_default();
    let scheme = chain.theme.map(|t| t.colors.clone()).unwrap_or_default();
    (scheme, map)
}

/// one slide's objects, back to front.
pub fn slide_view(presentation: &Presentation, slide: &Slide) -> SlideView {
    let chain = Chain::of_slide(presentation, slide);
    let (scheme, map) = slide_colors(presentation, slide);
    let colors = ColorContext {
        scheme: &scheme,
        map: &map,
        placeholder: None,
    };
    SlideView {
        id: slide.id.clone(),
        objects: slide
            .objects
            .iter()
            .filter_map(|object| object_view(&chain, &colors, object))
            .collect(),
    }
}

fn object_view(
    chain: &Chain<'_>,
    colors: &ColorContext<'_>,
    object: &Object,
) -> Option<ObjectView> {
    // kept xml has no box, and hidden objects are not shown in powerpoint either
    if is_hidden(object) {
        return None;
    }
    let transform = effective_transform(chain, object)?;
    let (width, height) = (points(transform.width), points(transform.height));
    Some(ObjectView {
        id: object.id(),
        name: object.name().to_string(),
        kind: kind_of(object),
        placeholder: object.placeholder().map(|p| p.kind.pptx_name()),
        x: points(transform.x),
        y: points(transform.y),
        width,
        height,
        rotation: f64::from(transform.rotation) / UNITS_PER_DEGREE,
        flip_h: transform.flip_h,
        flip_v: transform.flip_v,
        text: text_view(chain, colors, object, width, height),
        table: table_view(object),
        kept: kept_sentence(object),
    })
}

fn is_hidden(object: &Object) -> bool {
    match object {
        Object::Shape(shape) => shape.hidden,
        Object::Picture(picture) => picture.hidden,
        Object::Group(group) => group.hidden,
        Object::Connector(connector) => connector.hidden,
        Object::Frame(frame) => frame.hidden,
        Object::Preserved { .. } => true,
    }
}

fn kind_of(object: &Object) -> &'static str {
    match object {
        Object::Shape(shape) if shape.placeholder.is_some() => "placeholder",
        Object::Shape(shape) if shape.text_box => "textBox",
        Object::Shape(_) => "shape",
        Object::Picture(_) => "picture",
        Object::Group(_) => "group",
        Object::Connector(_) => "connector",
        Object::Frame(frame) => match frame.kind {
            GraphicKind::Table(_) => "table",
            GraphicKind::Chart { .. } => "chart",
            GraphicKind::Diagram { .. } => "diagram",
            GraphicKind::Ole { .. } | GraphicKind::Other { .. } => "embedded",
        },
        Object::Preserved { .. } => "kept",
    }
}

/// what the file keeps for an object that Letters doesn't draw or edit,
/// said once, plainly, for its frame and the Explain panel.
pub fn kept_sentence(object: &Object) -> Option<String> {
    let sentence = match object {
        Object::Frame(frame) => match &frame.kind {
            GraphicKind::Chart { .. } => frame_sentence("chart", frame.fallback_media_id.is_some()),
            GraphicKind::Diagram { .. } => {
                frame_sentence("SmartArt diagram", frame.fallback_media_id.is_some())
            }
            GraphicKind::Ole { .. } | GraphicKind::Other { .. } => {
                frame_sentence("embedded object", frame.fallback_media_id.is_some())
            }
            GraphicKind::Table(_) => return None,
        },
        Object::Shape(shape) if !shape.extra.is_empty() => {
            "Its shadow, glow or 3-D effect is kept in the file but not drawn here.".to_string()
        }
        Object::Connector(connector) if has_arrowheads(connector.line.as_ref()) => {
            "Its arrowheads are kept in the file but not drawn here.".to_string()
        }
        Object::Shape(shape) if has_arrowheads(shape.line.as_ref()) => {
            "Its arrowheads are kept in the file but not drawn here.".to_string()
        }
        Object::Picture(picture) if !picture.blip_extra.is_empty() => {
            "Its recolouring is kept in the file but not drawn here.".to_string()
        }
        _ => return None,
    };
    Some(sentence)
}

fn frame_sentence(what: &str, has_picture: bool) -> String {
    let shown = if has_picture {
        "the picture saved with it is shown"
    } else {
        "it shows as an empty box"
    };
    format!(
        "This {what} is kept exactly as it is in the file. Letters can’t draw or change it yet, so {shown} here; PowerPoint and LibreOffice still show and edit it. It can be moved, resized or deleted."
    )
}

fn has_arrowheads(line: Option<&letters_slides::model::Line>) -> bool {
    line.is_some_and(|line| {
        let drawn = |end: &Option<letters_slides::model::LineEnd>| {
            end.as_ref().is_some_and(|end| end.kind != "none")
        };
        drawn(&line.head) || drawn(&line.tail)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::deck::SlideDeck;
    use crate::slides::test_decks::deck_path;

    fn deck(name: &str) -> Presentation {
        SlideDeck::open(&deck_path(name)).unwrap().presentation
    }

    #[test]
    fn a_deck_lists_its_slides_layouts_and_theme() {
        let presentation = deck("pp_title_bullets.pptx");
        let view = deck_view(&presentation);
        assert_eq!(view.slides.len(), 2);
        assert!((view.width - 960.0).abs() < 0.01, "16:9 is 960 points wide");
        assert!(!view.layouts.is_empty());
        let theme = view.theme.unwrap();
        assert_eq!(theme.colors.len(), 12);
        assert!(theme.colors.iter().all(|c| c.hex.len() == 7));
        assert!(!view.slides[0].title.is_empty());
    }

    #[test]
    fn placeholders_get_their_box_from_the_layout() {
        let presentation = deck("pp_title_bullets.pptx");
        let view = slide_view(&presentation, &presentation.slides[0]);
        let title = view
            .objects
            .iter()
            .find(|o| o.kind == "placeholder")
            .unwrap();
        assert!(title.width > 100.0 && title.height > 10.0);
        assert!(title.text.is_some());
    }

    #[test]
    fn tables_are_listed_with_their_cells() {
        let presentation = deck("pp_table.pptx");
        let view = slide_view(&presentation, &presentation.slides[0]);
        let table = view.objects.iter().find(|o| o.kind == "table").unwrap();
        let cells = &table.table.as_ref().unwrap().rows;
        assert!(cells.len() >= 2);
        assert!(cells[0].cells.iter().any(|cell| !cell.text.is_empty()));
    }

    #[test]
    fn rotation_is_in_degrees() {
        let presentation = deck("pp_pictures.pptx");
        let view = slide_view(&presentation, &presentation.slides[0]);
        assert!(view.objects.iter().any(|o| o.kind == "picture"));
        assert!(view.objects.iter().all(|o| o.rotation.abs() <= 360.0));
    }
}
