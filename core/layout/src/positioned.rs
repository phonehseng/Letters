//! exact-position content: pages whose glyphs carry their own positions,
//! such as a pdf page converted for editing. nothing here is laid out
//! again: every glyph is drawn exactly where it is given, so a pdf opened
//! as a document inside letters looks the same as the source.
//!
//! a document can mix such pages with ordinary flowing text
//! (`layout_mixed`): each part keeps its own pages, in order.

use std::collections::HashMap;
use std::sync::Arc;

use letters_office::model::Document;
use serde::{Deserialize, Serialize};

use crate::engine::{layout, LayoutOptions};
use crate::fonts::{FontLibrary, FontOrigin, FontSource, LoadedFace};
use crate::result::{FontInfo, GlyphRun, LayoutResult, Line, Page, PlacedImage, Rect, Story};

/// the font of a positioned run: the program from the source (best: the
/// glyph shapes and ids are the source's own) or a family to look up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExactFont {
    Program {
        name: String,
        #[serde(skip)]
        bytes: Arc<Vec<u8>>,
        index: u32,
    },
    Family {
        family: String,
        bold: bool,
        italic: bool,
    },
}

/// one glyph at its own pen position (points from the page's top left).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactGlyph {
    /// the text it stands for
    pub text: String,
    /// the glyph id in the run's font; looked up from `text` when absent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<u16>,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactRun {
    pub font: ExactFont,
    pub size: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    pub glyphs: Vec<ExactGlyph>,
}

/// a page of exact-position content.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactPage {
    pub width: f64,
    pub height: f64,
    pub runs: Vec<ExactRun>,
    pub rects: Vec<Rect>,
    pub images: Vec<PlacedImage>,
}

impl ExactPage {
    /// an empty page of `width` by `height` points, ready to be filled.
    pub fn new(width: f64, height: f64) -> ExactPage {
        ExactPage {
            width,
            height,
            ..ExactPage::default()
        }
    }

    /// adds glyphs drawn in one font, size and colour.
    pub fn push_run(
        &mut self,
        font: ExactFont,
        size: f64,
        color: Option<[u8; 3]>,
        glyphs: Vec<ExactGlyph>,
    ) {
        self.runs.push(ExactRun {
            font,
            size,
            color,
            glyphs,
        });
    }
}

/// one part of a mixed document.
#[derive(Clone, Debug)]
pub enum MixedPart {
    /// flowing text, laid out by the engine
    Flow(Box<Document>),
    /// a page drawn exactly as given
    Exact(ExactPage),
}

/// lays out a document made of flowing parts and exact pages, in order.
pub fn layout_mixed(parts: &[MixedPart], options: &LayoutOptions) -> LayoutResult {
    let mut combined = LayoutResult {
        flavor: options.flavor,
        ..LayoutResult::default()
    };
    let mut exact_fonts = ExactFonts::new(options.fonts.clone());
    for (part_index, part) in parts.iter().enumerate() {
        let pages = match part {
            MixedPart::Flow(document) => {
                let result = layout(document, options);
                let remap = merge_fonts(&mut combined, &result);
                combined.substitutions.extend(result.substitutions);
                combined.unsupported.extend(result.unsupported);
                result
                    .pages
                    .into_iter()
                    .map(|page| remap_page(page, &remap))
                    .collect::<Vec<_>>()
            }
            MixedPart::Exact(page) => {
                vec![exact_fonts.page(page, part_index as u32, &mut combined)]
            }
        };
        for mut page in pages {
            page.index = combined.pages.len() as u32;
            page.number = page.index + 1;
            combined.pages.push(page);
        }
    }
    combined
}

/// lays out exact pages alone.
pub fn layout_exact(pages: &[ExactPage], options: &LayoutOptions) -> LayoutResult {
    let parts: Vec<MixedPart> = pages.iter().cloned().map(MixedPart::Exact).collect();
    layout_mixed(&parts, options)
}

/// adds a result's faces to the combined list; returns old id -> new id.
fn merge_fonts(combined: &mut LayoutResult, result: &LayoutResult) -> Vec<u32> {
    result
        .faces
        .iter()
        .zip(&result.fonts)
        .map(|(face, info)| add_face(combined, face.clone(), info.clone()))
        .collect()
}

fn add_face(combined: &mut LayoutResult, face: Arc<LoadedFace>, info: FontInfo) -> u32 {
    if let Some(found) =
        combined
            .faces
            .iter()
            .zip(&combined.fonts)
            .position(|(existing, existing_info)| {
                Arc::ptr_eq(existing, &face) && *existing_info == info
            })
    {
        return found as u32;
    }
    combined.faces.push(face);
    combined.fonts.push(info);
    (combined.fonts.len() - 1) as u32
}

fn remap_page(mut page: Page, remap: &[u32]) -> Page {
    for line in &mut page.lines {
        for run in &mut line.runs {
            run.font = remap.get(run.font as usize).copied().unwrap_or(run.font);
        }
    }
    page
}

/// loads the fonts exact pages name, once each.
struct ExactFonts {
    library: FontLibrary,
    programs: HashMap<(String, usize), Option<Arc<LoadedFace>>>,
}

impl ExactFonts {
    fn new(source: Arc<dyn FontSource>) -> ExactFonts {
        ExactFonts {
            library: FontLibrary::new(source),
            programs: HashMap::new(),
        }
    }

    fn face(&mut self, font: &ExactFont) -> Option<(Arc<LoadedFace>, FontInfo)> {
        match font {
            ExactFont::Program { name, bytes, index } => {
                let key = (name.clone(), Arc::as_ptr(bytes) as usize);
                let face = self
                    .programs
                    .entry(key)
                    .or_insert_with(|| {
                        LoadedFace::load(bytes.clone(), *index, FontOrigin::Provided).map(Arc::new)
                    })
                    .clone();
                match face {
                    Some(face) => Some((face.clone(), info_of(&face, false, false))),
                    // a program that cannot be read falls back to a family
                    None => self.face(&ExactFont::Family {
                        family: name.clone(),
                        bold: false,
                        italic: false,
                    }),
                }
            }
            ExactFont::Family {
                family,
                bold,
                italic,
            } => {
                let id = self.library.resolve(family, *bold, *italic)?;
                let resolved = self.library.font(id).clone();
                let info = info_of(
                    &resolved.face,
                    resolved.synthetic_bold,
                    resolved.synthetic_italic,
                );
                Some((resolved.face, info))
            }
        }
    }

    /// turns an exact page into a result page: one line per run.
    fn page(&mut self, exact: &ExactPage, part: u32, combined: &mut LayoutResult) -> Page {
        let mut page = Page {
            width: exact.width,
            height: exact.height,
            rects: exact.rects.clone(),
            images: exact.images.clone(),
            exact: true,
            ..Page::default()
        };
        for run in &exact.runs {
            let Some((face, info)) = self.face(&run.font) else {
                continue;
            };
            let font = add_face(combined, face.clone(), info);
            page.lines.extend(lines_of_run(run, &face, font, part));
        }
        page
    }
}

fn info_of(face: &LoadedFace, synthetic_bold: bool, synthetic_italic: bool) -> FontInfo {
    FontInfo {
        family: face.family.clone(),
        postscript_name: face.postscript_name.clone(),
        bold: face.bold,
        italic: face.italic,
        synthetic_bold,
        synthetic_italic,
        origin: face.origin.clone(),
    }
}

/// a run becomes one result line per baseline (glyphs of one run usually
/// share it; superscripts in the source are runs of their own).
fn lines_of_run(run: &ExactRun, face: &LoadedFace, font: u32, part: u32) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for glyph in &run.glyphs {
        let id = glyph
            .glyph
            .or_else(|| {
                glyph
                    .text
                    .chars()
                    .next()
                    .and_then(|ch| face.glyph(ch).map(|(id, _)| id))
            })
            .unwrap_or(0);
        let starts_new = lines
            .last()
            .is_none_or(|line| (line.baseline - glyph.y).abs() > 1e-6);
        if starts_new {
            lines.push(empty_line(run, glyph, font, part));
        }
        let line = lines.last_mut().expect("just pushed");
        let glyph_run = &mut line.runs[0];
        glyph_run
            .clusters
            .push(glyph_run.text.chars().count() as u32);
        glyph_run.text.push_str(&glyph.text);
        glyph_run.glyphs.push(id);
        glyph_run.x.push(glyph.x);
        let advance = f64::from(face.advance_of_glyph(id)) * face.scale(run.size);
        line.width = (glyph.x + advance - line.x).max(line.width);
    }
    lines
}

fn empty_line(run: &ExactRun, glyph: &ExactGlyph, font: u32, part: u32) -> Line {
    Line {
        id: 0,
        story: Story::Positioned { page: part },
        block: vec![part],
        start: 0,
        end: 0,
        x: glyph.x,
        width: 0.0,
        top: glyph.y - run.size * 0.8,
        height: run.size,
        baseline: glyph.y,
        ascent: run.size * 0.8,
        descent: run.size * 0.2,
        carets: vec![glyph.x],
        runs: vec![GlyphRun {
            font,
            size: run.size,
            y: glyph.y,
            color: run.color,
            glyphs: Vec::new(),
            x: Vec::new(),
            text: String::new(),
            clusters: Vec::new(),
            start: 0,
            end: 0,
            skew: 0.0,
            embolden: 0.0,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_glyphs_keep_their_positions() {
        let mut page = ExactPage::new(612.0, 792.0);
        page.push_run(
            ExactFont::Family {
                family: "Times New Roman".into(),
                bold: false,
                italic: false,
            },
            12.0,
            None,
            vec![
                ExactGlyph {
                    text: "A".into(),
                    glyph: None,
                    x: 72.123,
                    y: 100.456,
                },
                ExactGlyph {
                    text: "b".into(),
                    glyph: None,
                    x: 80.789,
                    y: 100.456,
                },
            ],
        );
        let result = layout_exact(&[page], &LayoutOptions::default());
        if result.pages[0].lines.is_empty() {
            // no fonts on this machine at all: nothing to check
            return;
        }
        let run = &result.pages[0].lines[0].runs[0];
        assert_eq!(run.x, vec![72.123, 80.789]);
        assert_eq!(run.y, 100.456);
        assert!(result.pages[0].exact);
    }

    #[test]
    fn mixed_documents_keep_their_parts_in_order() {
        let mut document = Document::new();
        document.blocks = vec![letters_office::model::Block::text("flowing text")];
        let parts = vec![
            MixedPart::Exact(ExactPage::new(300.0, 400.0)),
            MixedPart::Flow(Box::new(document)),
            MixedPart::Exact(ExactPage::new(300.0, 400.0)),
        ];
        let result = layout_mixed(&parts, &LayoutOptions::default());
        assert_eq!(result.pages.len(), 3);
        assert!(result.pages[0].exact && !result.pages[1].exact && result.pages[2].exact);
        assert_eq!(
            result.pages.iter().map(|p| p.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }
}
