//! decks to and from letters-office documents (feature "office"): a
//! document's headings become slides, and slides become "canvas pages"
//! (sections whose content is all positioned) and back, so a converted
//! pdf page or a flyer can become a deck.
//!
//! the bridge is behind a feature because letters-office changes often;
//! with it off this crate builds on its own.

use letters_office::model::{
    Block, Document, Fill as OfficeFill, FloatPosition, Geometry as OfficeGeometry, Inline, Mark,
    Paragraph, Section, Shape as OfficeShape, Stroke, TextBox,
};

use crate::blank::{blank_presentation, new_slide, BLANK_LAYOUT};
use crate::color::{resolve, ColorContext};
use crate::inherit::{
    effective_paragraph, effective_run, effective_transform, resolve_typeface, Chain,
};
use crate::model::{
    ColorRef, Fill, Geometry, Object, Presentation, RunProps, Shape, TextBody, TextParagraph,
    TextRun, Transform, Typeface,
};
use crate::outline::{from_outline, OutlineEntry};

/// emu in a twip (914400 emu and 1440 twips to the inch)
const EMU_PER_TWIP: i64 = 635;

fn twips(emu: i64) -> i32 {
    (emu / EMU_PER_TWIP) as i32
}

/// a deck from a document: each top-level heading starts a slide, deeper
/// headings and paragraphs become its bullets.
pub fn deck_from_document(document: &Document) -> Presentation {
    let mut entries = Vec::new();
    for block in &document.blocks {
        match block {
            Block::Heading(heading) => entries.push(OutlineEntry {
                level: heading.level.saturating_sub(1),
                text: plain(&heading.paragraph),
            }),
            Block::Paragraph(paragraph) if !plain(paragraph).trim().is_empty() => {
                entries.push(OutlineEntry {
                    level: 1 + paragraph.props.list.as_ref().map_or(0, |list| list.level),
                    text: plain(paragraph),
                })
            }
            _ => {}
        }
    }
    from_outline(&entries)
}

fn plain(paragraph: &Paragraph) -> String {
    paragraph
        .content
        .iter()
        .filter_map(|inline| match inline {
            Inline::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// every slide as a canvas page: text as text boxes, outlines as shapes,
/// pictures as shapes filled with the picture, at their slide positions.
pub fn to_canvas_pages(presentation: &Presentation) -> Document {
    let mut document = Document::new();
    document.sections.clear();
    document.blocks.clear();
    document.media = presentation
        .media
        .iter()
        .map(|item| letters_office::model::MediaItem {
            id: item.id.clone(),
            content_type: item.content_type.clone(),
            source_path: None,
            bytes: item.bytes.clone(),
        })
        .collect();
    for (index, slide) in presentation.slides.iter().enumerate() {
        if index > 0 {
            document.blocks.push(Block::SectionBreak);
        }
        let mut section = Section {
            canvas: true,
            ..Default::default()
        };
        section.page.width = twips(presentation.slide_size.width);
        section.page.height = twips(presentation.slide_size.height);
        document.sections.push(section);
        let chain = Chain::of_slide(presentation, slide);
        for object in &slide.objects {
            if let Some(block) = canvas_block(&chain, object) {
                document.blocks.push(block);
            }
        }
    }
    document
}

fn canvas_block(chain: &Chain<'_>, object: &Object) -> Option<Block> {
    let transform = effective_transform(chain, object)?;
    match object {
        Object::Shape(shape) if shape.text.as_ref().is_some_and(|t| !t.is_empty()) => {
            let blocks = text_blocks(chain, object, shape.text.as_ref()?);
            let mut text_box = TextBox::new(
                twips(transform.x),
                twips(transform.y),
                twips(transform.width),
                twips(transform.height),
                blocks,
            );
            text_box.border = None;
            text_box.fill = None;
            text_box.width_emu = Some(transform.width);
            text_box.height_emu = Some(transform.height);
            text_box.rotation =
                (transform.rotation != 0).then(|| f64::from(transform.rotation) / 60_000.0);
            Some(Block::TextBox(text_box))
        }
        Object::Shape(shape) => Some(Block::Shape(office_shape(
            chain,
            &transform,
            shape.geometry.as_ref(),
            shape.fill.as_ref(),
        ))),
        Object::Picture(picture) => {
            let mut shape = office_shape(chain, &transform, None, None);
            shape.fill = OfficeFill::Picture {
                media_id: picture.media_id.clone()?,
                tile: false,
            };
            shape.stroke = None;
            Some(Block::Shape(shape))
        }
        _ => None,
    }
}

fn office_shape(
    chain: &Chain<'_>,
    transform: &Transform,
    geometry: Option<&Geometry>,
    fill: Option<&Fill>,
) -> OfficeShape {
    let name = geometry.and_then(|g| g.preset_name()).unwrap_or("rect");
    let colors = colors_of(chain);
    let fill = match fill {
        Some(Fill::Solid { color }) => resolve(color, &colors.context()).to_hex(),
        _ => "FFFFFF".to_string(),
    };
    let mut shape = OfficeShape::preset(
        name,
        twips(transform.x),
        twips(transform.y),
        twips(transform.width),
        twips(transform.height),
        &fill,
    );
    if let Some(Geometry::Preset { adjust, .. }) = geometry {
        shape.geometry = OfficeGeometry::Preset {
            name: name.to_string(),
            adjust: adjust
                .iter()
                .map(|(name, value)| letters_office::model::Adjust {
                    name: name.clone(),
                    value: *value,
                })
                .collect(),
        };
    }
    shape.horizontal.offset_emu = Some(transform.x);
    shape.vertical.offset_emu = Some(transform.y);
    shape.width_emu = Some(transform.width);
    shape.height_emu = Some(transform.height);
    shape.rotation = (transform.rotation != 0).then(|| f64::from(transform.rotation) / 60_000.0);
    shape.flip_horizontal = transform.flip_h;
    shape.flip_vertical = transform.flip_v;
    shape.stroke = Some(Stroke::solid("000000", 15));
    shape
}

/// the deck's colour scheme and map, owned, for resolving colours.
struct Colors {
    scheme: crate::model::ColorScheme,
    map: crate::model::ColorMap,
}

impl Colors {
    fn context(&self) -> ColorContext<'_> {
        ColorContext {
            scheme: &self.scheme,
            map: &self.map,
            placeholder: None,
        }
    }
}

fn colors_of(chain: &Chain<'_>) -> Colors {
    Colors {
        scheme: chain.theme.map(|t| t.colors.clone()).unwrap_or_default(),
        map: chain
            .master
            .map(|m| m.color_map.clone())
            .unwrap_or_default(),
    }
}

/// a text body as office paragraphs with the effective formatting as marks.
fn text_blocks(chain: &Chain<'_>, object: &Object, body: &TextBody) -> Vec<Block> {
    let colors = colors_of(chain);
    body.paragraphs
        .iter()
        .map(|paragraph| {
            let effective = effective_paragraph(chain, object, body, paragraph);
            let content = paragraph
                .runs
                .iter()
                .filter_map(|run| match run {
                    TextRun::Text { text, props } | TextRun::Field { text, props, .. } => {
                        let marks = marks_of(chain, &effective_run(&effective, props), &colors);
                        Some(Inline::Text {
                            text: text.clone(),
                            marks,
                        })
                    }
                    _ => None,
                })
                .collect();
            Block::Paragraph(Paragraph {
                content,
                ..Default::default()
            })
        })
        .collect()
}

fn marks_of(chain: &Chain<'_>, props: &RunProps, colors: &Colors) -> Vec<Mark> {
    let mut marks = Vec::new();
    if props.bold == Some(true) {
        marks.push(Mark::Bold);
    }
    if props.italic == Some(true) {
        marks.push(Mark::Italic);
    }
    if let Some(size) = props.size {
        marks.push(Mark::FontSize {
            half_points: (size / 50) as u32,
        });
    }
    if let Some(Fill::Solid { color }) = &props.fill {
        marks.push(Mark::Color {
            hex: resolve(color, &colors.context()).to_hex(),
        });
    }
    let face = props
        .latin
        .as_ref()
        .map_or("+mn-lt", |t| t.typeface.as_str());
    marks.push(Mark::FontFamily {
        name: resolve_typeface(chain.theme, face),
        theme: None,
        extra: Vec::new(),
    });
    marks
}

/// canvas pages (sections marked as canvas) as slides; the deck is sized
/// like the first page. text boxes keep their text and its basic marks.
pub fn from_canvas_pages(document: &Document) -> Presentation {
    let mut deck = blank_presentation();
    if let Some(first) = document.sections.first() {
        deck.slide_size.width = i64::from(first.page.width) * EMU_PER_TWIP;
        deck.slide_size.height = i64::from(first.page.height) * EMU_PER_TWIP;
    }
    let mut slide = new_slide(&deck, "slide-1", BLANK_LAYOUT);
    let mut next_id = 2;
    for block in &document.blocks {
        if matches!(block, Block::SectionBreak) {
            let number = deck.slides.len() + 2;
            deck.slides.push(std::mem::replace(
                &mut slide,
                new_slide(&deck, &format!("slide-{number}"), BLANK_LAYOUT),
            ));
            continue;
        }
        if let Some(object) = slide_object(block, next_id) {
            slide.objects.push(object);
            next_id += 1;
        }
    }
    deck.slides.push(slide);
    deck.media = document
        .media
        .iter()
        .map(|item| crate::model::MediaItem {
            id: item.id.clone(),
            content_type: item.content_type.clone(),
            source_path: None,
            bytes: item.bytes.clone(),
        })
        .collect();
    deck
}

fn position(
    horizontal: &FloatPosition,
    vertical: &FloatPosition,
    width: (i32, Option<i64>),
    height: (i32, Option<i64>),
) -> Transform {
    let exact = |emu: Option<i64>, value: i32| emu.unwrap_or(i64::from(value) * EMU_PER_TWIP);
    Transform::new(
        exact(horizontal.offset_emu, horizontal.offset),
        exact(vertical.offset_emu, vertical.offset),
        exact(width.1, width.0),
        exact(height.1, height.0),
    )
}

fn slide_object(block: &Block, id: u32) -> Option<Object> {
    match block {
        Block::TextBox(text_box) => Some(Object::Shape(Shape {
            id,
            name: text_box
                .name
                .clone()
                .unwrap_or_else(|| format!("Text {id}")),
            text_box: true,
            transform: Some(position(
                &text_box.horizontal,
                &text_box.vertical,
                (text_box.width, text_box.width_emu),
                (text_box.height, text_box.height_emu),
            )),
            geometry: Some(Geometry::preset("rect")),
            text: Some(TextBody {
                paragraphs: text_box
                    .blocks
                    .iter()
                    .filter_map(Block::paragraph)
                    .map(slide_paragraph)
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        })),
        Block::Shape(shape) => Some(Object::Shape(Shape {
            id,
            name: shape.name.clone().unwrap_or_else(|| format!("Shape {id}")),
            transform: Some(position(
                &shape.horizontal,
                &shape.vertical,
                (shape.width, shape.width_emu),
                (shape.height, shape.height_emu),
            )),
            geometry: Some(Geometry::preset(shape.preset_name().unwrap_or("rect"))),
            fill: match &shape.fill {
                OfficeFill::Solid { color, .. } => Some(Fill::solid(ColorRef::rgb(color))),
                _ => None,
            },
            ..Default::default()
        })),
        _ => None,
    }
}

fn slide_paragraph(paragraph: &Paragraph) -> TextParagraph {
    let runs = paragraph
        .content
        .iter()
        .filter_map(|inline| match inline {
            Inline::Text { text, marks } => Some(TextRun::Text {
                text: text.clone(),
                props: run_props(marks),
            }),
            _ => None,
        })
        .collect();
    TextParagraph {
        runs,
        ..Default::default()
    }
}

fn run_props(marks: &[Mark]) -> RunProps {
    let mut props = RunProps::default();
    for mark in marks {
        match mark {
            Mark::Bold => props.bold = Some(true),
            Mark::Italic => props.italic = Some(true),
            Mark::FontSize { half_points } => props.size = Some(*half_points as i32 * 50),
            Mark::Color { hex } => props.fill = Some(Fill::solid(ColorRef::rgb(hex))),
            Mark::FontFamily { name, .. } => props.latin = Some(Typeface::named(name)),
            _ => {}
        }
    }
    props
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_outline_becomes_a_deck() {
        let mut document = Document::new();
        document.blocks = vec![
            Block::Heading(letters_office::model::Heading {
                level: 1,
                paragraph: Paragraph::with_text("Cells"),
            }),
            Block::Paragraph(Paragraph::with_text("Every living thing is made of cells")),
            Block::Heading(letters_office::model::Heading {
                level: 1,
                paragraph: Paragraph::with_text("Membranes"),
            }),
        ];
        let deck = deck_from_document(&document);
        let view = crate::outline::outline(&deck);
        assert_eq!(view.len(), 2);
        assert_eq!(
            view[0].items,
            vec![(0, "Every living thing is made of cells".to_string())]
        );
    }

    #[test]
    fn slides_become_canvas_pages_and_back_with_text_and_positions() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/decks/pp_shapes.pptx");
        let deck = crate::read_pptx(&std::fs::read(path).unwrap()).unwrap();
        let document = to_canvas_pages(&deck);
        assert!(document.sections[0].canvas);
        let back = from_canvas_pages(&document);
        assert_eq!(back.slides.len(), deck.slides.len());
        let texts = |p: &Presentation| {
            p.slides[0]
                .objects
                .iter()
                .filter_map(|o| o.text())
                .map(|t| t.plain_text())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(&back), texts(&deck));
        let first = |p: &Presentation| {
            p.slides[0]
                .objects
                .iter()
                .find_map(|o| o.transform().cloned())
                .map(|t| (t.x, t.y, t.width, t.height))
        };
        assert_eq!(first(&back), first(&deck));
    }
}
