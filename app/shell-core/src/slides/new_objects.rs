//! the objects Insert adds: shapes, text boxes, pictures and tables, made
//! the way powerpoint makes them, so they look at home in any theme and
//! open in powerpoint and libreoffice as their own.

use letters_slides::model::{
    Anchor, Autofit, BodyProps, ColorRef, Fill, Geometry, GraphicFrame, GraphicKind, MediaItem,
    Object, ParagraphProps, Picture, Presentation, Shape, ShapeStyle, Slide, StyleRef, Table,
    TableCell, TableRow, TextAlign, TextBody, TextParagraph, Transform, EMU_PER_POINT,
};
use serde::Deserialize;

/// powerpoint's own default table style, "Medium Style 2 - Accent 1"
const DEFAULT_TABLE_STYLE: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";
/// powerpoint draws a new shape's outline in the accent darkened by 15 %
/// (`a:shade val="15000"` in the style reference it writes)
const OUTLINE_SHADE: i32 = 15_000;
/// style references are 1-based indexes into the theme's lists; a font
/// reference of 2 is the minor (body) font
const SUBTLE_STYLE: u32 = 1;
const MODERATE_STYLE: u32 = 2;
const MINOR_FONT: u32 = 2;

/// a box in points, as the editor sends it.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl PointBox {
    fn transform(self) -> Transform {
        let emu = |points: f64| (points * EMU_PER_POINT as f64).round() as i64;
        Transform::new(emu(self.x), emu(self.y), emu(self.width), emu(self.height))
    }
}

/// what Insert can add.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NewObject {
    /// a preset shape: "rect", "roundRect", "ellipse", "triangle", "rightArrow"…
    Shape {
        preset: String,
    },
    TextBox {
        text: String,
    },
    Table {
        rows: usize,
        columns: usize,
    },
}

/// the id the engine will give the next object (one past the highest,
/// groups included), so a new object can be named as powerpoint names it.
pub fn next_object_id(slide: &Slide) -> u32 {
    fn highest(objects: &[Object]) -> u32 {
        objects
            .iter()
            .map(|object| match object {
                Object::Group(group) => group.id.max(highest(&group.objects)),
                other => other.id(),
            })
            .max()
            .unwrap_or(1)
    }
    highest(&slide.objects) + 1
}

pub fn build(new: &NewObject, at: PointBox, id: u32) -> Object {
    match new {
        NewObject::Shape { preset } => shape(preset, at, id),
        NewObject::TextBox { text } => text_box(text, at, id),
        NewObject::Table { rows, columns } => table(*rows, *columns, at, id),
    }
}

/// a shape filled with the theme's first accent, with centred white text,
/// as powerpoint's Insert › Shapes makes it.
fn shape(preset: &str, at: PointBox, id: u32) -> Object {
    let accent = ColorRef::scheme("accent1");
    let centred = TextParagraph {
        props: ParagraphProps {
            align: Some(TextAlign::Center),
            ..Default::default()
        },
        ..Default::default()
    };
    Object::Shape(Shape {
        id,
        name: format!("{} {id}", shape_name(preset)),
        transform: Some(at.transform()),
        geometry: Some(Geometry::preset(preset)),
        style: Some(ShapeStyle {
            line: StyleRef {
                index: MODERATE_STYLE,
                color: Some(accent.clone().with("shade", OUTLINE_SHADE)),
            },
            fill: StyleRef {
                index: SUBTLE_STYLE,
                color: Some(accent.clone()),
            },
            effect: StyleRef {
                index: 0,
                color: Some(accent),
            },
            font: StyleRef {
                index: MINOR_FONT,
                color: Some(ColorRef::scheme("lt1")),
            },
        }),
        text: Some(TextBody {
            body: BodyProps {
                anchor: Some(Anchor::Middle),
                ..Default::default()
            },
            paragraphs: vec![centred],
            ..Default::default()
        }),
        ..Default::default()
    })
}

fn shape_name(preset: &str) -> &'static str {
    match preset {
        "ellipse" => "Oval",
        "roundRect" => "Rectangle: Rounded Corners",
        "triangle" => "Isosceles Triangle",
        "rightArrow" => "Arrow: Right",
        "star5" => "Star: 5 Points",
        "line" => "Straight Connector",
        _ => "Rectangle",
    }
}

/// a text box that grows with its text, with no fill or outline.
fn text_box(text: &str, at: PointBox, id: u32) -> Object {
    let mut body = TextBody::from_text(text);
    body.body = BodyProps {
        wrap: Some(true),
        autofit: Some(Autofit::ResizeShape),
        ..Default::default()
    };
    Object::Shape(Shape {
        id,
        name: format!("TextBox {id}"),
        text_box: true,
        transform: Some(at.transform()),
        geometry: Some(Geometry::preset("rect")),
        fill: Some(Fill::None),
        text: Some(body),
        ..Default::default()
    })
}

/// a table with even columns and rows in the default table style, with
/// a header row and banded rows, as powerpoint's Insert › Table makes it.
fn table(rows: usize, columns: usize, at: PointBox, id: u32) -> Object {
    let rows = rows.clamp(1, 50);
    let columns = columns.clamp(1, 30);
    let transform = at.transform();
    let column_width = transform.width / columns as i64;
    let row_height = transform.height / rows as i64;
    let cell = TableCell {
        text: TextBody::from_text(""),
        grid_span: 1,
        row_span: 1,
        ..Default::default()
    };
    let row = TableRow {
        height: row_height,
        cells: vec![cell; columns],
        source: None,
    };
    Object::Frame(GraphicFrame {
        id,
        name: format!("Table {id}"),
        description: None,
        hidden: false,
        placeholder: None,
        transform: Some(transform),
        kind: GraphicKind::Table(Table {
            columns: vec![column_width; columns],
            rows: vec![row; rows],
            style_id: Some(DEFAULT_TABLE_STYLE.to_string()),
            flags: vec!["firstRow".into(), "bandRow".into()],
            extra: Vec::new(),
        }),
        fallback_media_id: None,
        source: None,
    })
}

/// a picture: its bytes join the deck's media table under a name no other
/// item uses, and the object shows it in `at`.
pub fn picture(
    presentation: &mut Presentation,
    file_name: &str,
    bytes: Vec<u8>,
    at: PointBox,
    id: u32,
) -> Object {
    let extension = file_name
        .rsplit('.')
        .next()
        .unwrap_or("png")
        .to_ascii_lowercase();
    let mut number = presentation.media.len() + 1;
    let media_id = loop {
        let candidate = format!("letters-picture-{number}.{extension}");
        if presentation.media_item(&candidate).is_none() {
            break candidate;
        }
        number += 1;
    };
    presentation.media.push(MediaItem {
        id: media_id.clone(),
        content_type: MediaItem::content_type_for(file_name).to_string(),
        source_path: None,
        bytes,
    });
    Object::Picture(Picture {
        id,
        name: format!("Picture {id}"),
        description: Some(file_name.to_string()),
        transform: Some(at.transform()),
        media_id: Some(media_id),
        geometry: Some(Geometry::preset("rect")),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::deck::SlideDeck;
    use crate::slides::test_decks::{deck_path, scratch_folder};
    use letters_slides::edit::Operation;

    const BOX: PointBox = PointBox {
        x: 100.0,
        y: 80.0,
        width: 200.0,
        height: 120.0,
    };

    fn add(deck: &mut SlideDeck, object: Object) -> u32 {
        let slide_id = deck.presentation.slides[0].id.clone();
        deck.apply(&Operation::AddObject {
            slide_id,
            object: Box::new(object),
        })
        .unwrap();
        deck.presentation.slides[0].objects.last().unwrap().id()
    }

    #[test]
    fn new_objects_get_the_id_their_name_says() {
        let mut deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let id = next_object_id(&deck.presentation.slides[0]);
        let shape = build(
            &NewObject::Shape {
                preset: "ellipse".into(),
            },
            BOX,
            id,
        );
        assert_eq!(add(&mut deck, shape), id);
        let added = deck.presentation.slides[0].objects.last().unwrap();
        assert_eq!(added.name(), format!("Oval {id}"));
        let transform = added.transform().unwrap();
        assert_eq!(transform.x, 100 * EMU_PER_POINT);
        assert_eq!(transform.width, 200 * EMU_PER_POINT);
    }

    #[test]
    fn inserted_objects_survive_saving_and_reopening() {
        let folder = scratch_folder("new-objects");
        let mut deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let slide = deck.presentation.slides[0].clone();
        let text_id = next_object_id(&slide);
        add(
            &mut deck,
            build(
                &NewObject::TextBox {
                    text: "Hello".into(),
                },
                BOX,
                text_id,
            ),
        );
        let table_id = next_object_id(&deck.presentation.slides[0]);
        add(
            &mut deck,
            build(
                &NewObject::Table {
                    rows: 2,
                    columns: 3,
                },
                BOX,
                table_id,
            ),
        );
        let png = std::fs::read(deck_path("../../../../app/shell/icons/32x32.png")).unwrap();
        let picture_id = next_object_id(&deck.presentation.slides[0]);
        let picture = picture(&mut deck.presentation, "logo.png", png, BOX, picture_id);
        add(&mut deck, picture);
        deck.save(Some(&folder.join("inserted.pptx"))).unwrap();
        let reopened = SlideDeck::open(&folder.join("inserted.pptx")).unwrap();
        let objects = &reopened.presentation.slides[0].objects;
        assert_eq!(objects.len(), slide.objects.len() + 3);
        let texts: Vec<String> = objects
            .iter()
            .filter_map(|o| o.text())
            .map(|t| t.plain_text())
            .collect();
        assert!(texts.contains(&"Hello".to_string()));
        assert!(objects.iter().any(|o| matches!(o, Object::Picture(_))));
        assert!(objects.iter().any(|o| matches!(o, Object::Frame(_))));
    }
}
