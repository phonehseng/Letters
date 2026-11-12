//! draws slides without any office application: the model is turned into
//! a scene (`scene`), which `pdf` writes with real text in embedded
//! subset fonts, vector shapes and pictures. thumbnails and the editor's
//! canvas are the same pdf drawn by the app's pdf renderer (letters-pdf).

mod objects;
pub(crate) mod paint;
pub mod pdf;
mod pdf_fonts;
mod pdf_images;
pub mod scene;
mod tables;
pub(crate) mod text;

use serde::{Deserialize, Serialize};

use crate::color::ColorContext;
use crate::fonts::{FontLibrary, FontSubstitution};
use crate::inherit::Chain;
use crate::model::{ColorMap, ColorScheme, Object, Presentation, EMU_PER_POINT};
use scene::{rectangle, Item, Paint, Scene, IDENTITY};

/// what drawing a deck had to approximate, for the person and the report.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderReport {
    /// fonts that were not installed and what stood in for them
    pub substitutions: Vec<FontSubstitution>,
    /// objects drawn approximately or not at all, in plain words
    pub approximations: Vec<String>,
}

/// everything drawing one slide needs.
pub struct RenderContext<'a> {
    pub presentation: &'a Presentation,
    pub fonts: &'a mut FontLibrary,
    pub scheme: ColorScheme,
    pub map: ColorMap,
    /// shown by slide number fields
    pub slide_number: usize,
    pub report: &'a mut RenderReport,
}

impl RenderContext<'_> {
    pub fn colors(&self) -> ColorContext<'_> {
        ColorContext {
            scheme: &self.scheme,
            map: &self.map,
            placeholder: None,
        }
    }

    pub fn note(&mut self, message: String) {
        if !self.report.approximations.contains(&message) {
            self.report.approximations.push(message);
        }
    }
}

pub fn points(emu: i64) -> f64 {
    emu as f64 / EMU_PER_POINT as f64
}

/// one slide as a scene. slide numbers count from the deck's first.
pub fn slide_scene(
    presentation: &Presentation,
    index: usize,
    fonts: &mut FontLibrary,
    report: &mut RenderReport,
) -> Scene {
    let slide = &presentation.slides[index];
    let chain = Chain::of_slide(presentation, slide);
    let map = slide
        .color_map
        .clone()
        .or_else(|| chain.layout.and_then(|l| l.color_map.clone()))
        .or_else(|| chain.master.map(|m| m.color_map.clone()))
        .unwrap_or_default();
    let first = presentation.first_slide_number.unwrap_or(1) as usize;
    let mut context = RenderContext {
        presentation,
        fonts,
        scheme: chain.theme.map(|t| t.colors.clone()).unwrap_or_default(),
        map,
        slide_number: first + index,
        report,
    };
    let mut scene = Scene {
        width: points(presentation.slide_size.width),
        height: points(presentation.slide_size.height),
        items: Vec::new(),
    };
    draw_background(&mut context, &chain, slide, &mut scene);
    draw_furniture(&mut context, &chain, slide, &mut scene.items);
    for object in &slide.objects {
        objects::draw_object(&mut context, &chain, object, &IDENTITY, &mut scene.items);
    }
    let substitutions = std::mem::take(&mut context.fonts.substitutions);
    for substitution in substitutions {
        if !report.substitutions.contains(&substitution) {
            report.substitutions.push(substitution);
        }
    }
    scene
}

/// the slide's background, else its layout's, else its master's.
fn draw_background(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    slide: &crate::model::Slide,
    scene: &mut Scene,
) {
    let background = slide
        .background
        .as_ref()
        .or_else(|| chain.layout.and_then(|l| l.background.as_ref()))
        .or_else(|| chain.master.and_then(|m| m.background.as_ref()));
    let paint = match background {
        Some(background) => {
            objects::background_paint(context, chain, background, scene.width, scene.height)
        }
        None => Some(Paint::Solid(crate::color::Rgba::WHITE)),
    };
    let area = rectangle(0.0, 0.0, scene.width, scene.height);
    if let Some(paint) = paint {
        scene.items.push(Item::Fill {
            matrix: IDENTITY,
            paths: area,
            paint,
        });
    }
    if let Some(media_id) = background.and_then(objects::background_picture) {
        scene.items.push(Item::Image {
            matrix: [scene.width, 0.0, 0.0, scene.height, 0.0, 0.0],
            media_id,
            clip: None,
            alpha: 1.0,
        });
    }
}

/// the master's and layout's own shapes (not their placeholders), drawn
/// behind the slide's unless the slide or layout turns them off.
fn draw_furniture(
    context: &mut RenderContext<'_>,
    chain: &Chain<'_>,
    slide: &crate::model::Slide,
    items: &mut Vec<Item>,
) {
    if !slide.show_master_shapes {
        return;
    }
    let decorative = |object: &&Object| object.placeholder().is_none();
    let layout_shows_master = chain.layout.is_none_or(|l| l.show_master_shapes);
    if let (Some(master), true) = (chain.master, layout_shows_master) {
        let master_chain = Chain::of_master(chain.presentation, master);
        for object in master.objects.iter().filter(decorative) {
            objects::draw_object(context, &master_chain, object, &IDENTITY, items);
        }
    }
    if let Some(layout) = chain.layout {
        let layout_chain = Chain::of_layout(chain.presentation, layout);
        for object in layout.objects.iter().filter(decorative) {
            objects::draw_object(context, &layout_chain, object, &IDENTITY, items);
        }
    }
}

/// the slides (all when `slides` is `None`) as one pdf, one page each,
/// with what had to be approximated.
pub fn render_pdf(
    presentation: &Presentation,
    slides: Option<&[usize]>,
    fonts: &mut FontLibrary,
) -> Result<(Vec<u8>, RenderReport), crate::SlidesError> {
    let mut report = RenderReport::default();
    let all: Vec<usize> = (0..presentation.slides.len()).collect();
    let chosen = slides.unwrap_or(&all);
    let scenes: Vec<Scene> = chosen
        .iter()
        .filter(|index| **index < presentation.slides.len())
        .map(|index| slide_scene(presentation, *index, fonts, &mut report))
        .collect();
    let bytes = pdf::write_pdf(&scenes, fonts, &presentation.media)?;
    Ok((bytes, report))
}
