//! editing operations: each changes what it says and its inverse puts
//! the deck back exactly; edited decks still save and reopen.

use std::path::Path;

use letters_slides::color::{resolve, ColorContext};
use letters_slides::edit::{apply, AlignEdge, Axis, Operation, SizeScaling, ZOrder};
use letters_slides::model::{ColorRef, Fill, GraphicKind, Object, Presentation};
use letters_slides::{read_pptx, write_pptx};

fn deck(name: &str) -> Presentation {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/decks")
        .join(name);
    read_pptx(&std::fs::read(path).unwrap()).unwrap()
}

fn json(presentation: &Presentation) -> String {
    serde_json::to_string(&(
        &presentation.slides,
        &presentation.themes,
        &presentation.slide_size,
        &presentation.sections,
        &presentation.layouts,
    ))
    .unwrap()
}

/// applies an operation, checks the deck changed, undoes it and checks
/// the deck is exactly as before; returns the changed deck.
fn apply_and_undo(mut presentation: Presentation, operation: Operation) -> Presentation {
    let before = json(&presentation);
    let applied = apply(&mut presentation, &operation).unwrap();
    let changed = presentation.clone();
    assert_ne!(json(&presentation), before, "{operation:?} changed nothing");
    apply(&mut presentation, &applied.inverse).unwrap();
    assert_eq!(json(&presentation), before, "undoing {operation:?}");
    changed
}

fn first_slide_id(presentation: &Presentation) -> String {
    presentation.slides[0].id.clone()
}

#[test]
fn slide_list_edits_undo_exactly() {
    let presentation = deck("pp_title_bullets.pptx");
    let layout = presentation.layouts[1].id.clone();
    let added = apply_and_undo(
        presentation.clone(),
        Operation::AddSlide {
            index: 1,
            layout_id: layout,
        },
    );
    assert_eq!(added.slides.len(), 3);
    let duplicated = apply_and_undo(presentation.clone(), Operation::DuplicateSlide { index: 0 });
    assert_eq!(
        duplicated.slides[1].objects.len(),
        presentation.slides[0].objects.len()
    );
    let deleted = apply_and_undo(presentation.clone(), Operation::DeleteSlide { index: 0 });
    assert_eq!(deleted.slides.len(), 1);
    let moved = apply_and_undo(
        presentation.clone(),
        Operation::MoveSlide { from: 0, to: 1 },
    );
    assert_eq!(moved.slides[1].id, presentation.slides[0].id);
}

#[test]
fn edited_slide_lists_save_and_reopen() {
    let mut presentation = deck("pp_title_bullets.pptx");
    apply(&mut presentation, &Operation::DuplicateSlide { index: 1 }).unwrap();
    apply(&mut presentation, &Operation::MoveSlide { from: 2, to: 0 }).unwrap();
    apply(&mut presentation, &Operation::DeleteSlide { index: 2 }).unwrap();
    let layout = presentation.layouts[1].id.clone();
    apply(
        &mut presentation,
        &Operation::AddSlide {
            index: 2,
            layout_id: layout,
        },
    )
    .unwrap();
    let reread = read_pptx(&write_pptx(&presentation).unwrap()).unwrap();
    assert_eq!(reread.slides.len(), 3);
    let texts = |p: &Presentation| {
        p.slides
            .iter()
            .map(|s| {
                s.objects
                    .iter()
                    .filter_map(|o| o.text())
                    .map(|t| t.plain_text())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(texts(&reread), texts(&presentation));
}

#[test]
fn object_edits_undo_exactly() {
    let presentation = deck("pp_shapes.pptx");
    let slide = first_slide_id(&presentation);
    for operation in shape_operations(&presentation, &slide) {
        apply_and_undo(presentation.clone(), operation);
    }
}

/// one of each object edit on the shapes deck's first slide.
fn shape_operations(presentation: &Presentation, slide: &str) -> Vec<Operation> {
    let slide = slide.to_string();
    let ids: Vec<u32> = presentation.slides[0]
        .objects
        .iter()
        .map(Object::id)
        .collect();
    vec![
        Operation::Move {
            slide_id: slide.clone(),
            object_ids: vec![ids[0]],
            dx: 12_700,
            dy: -12_700,
        },
        Operation::Resize {
            slide_id: slide.clone(),
            object_id: ids[1],
            x: 0,
            y: 0,
            width: 914_400,
            height: 457_200,
        },
        Operation::Rotate {
            slide_id: slide.clone(),
            object_id: ids[2],
            rotation: 45 * 60_000,
        },
        Operation::SetFill {
            slide_id: slide.clone(),
            object_id: ids[0],
            fill: Some(Fill::solid(ColorRef::scheme("accent2"))),
        },
        Operation::SetLine {
            slide_id: slide.clone(),
            object_id: ids[0],
            line: Some(letters_slides::model::Line {
                width: Some(38_100),
                ..Default::default()
            }),
        },
        Operation::Reorder {
            slide_id: slide.clone(),
            object_id: ids[0],
            to: ZOrder::Front,
        },
        Operation::Align {
            slide_id: slide.clone(),
            object_ids: ids[..3].to_vec(),
            edge: AlignEdge::Left,
            to_slide: false,
        },
        Operation::Distribute {
            slide_id: slide.clone(),
            object_ids: ids[1..4].to_vec(),
            axis: Axis::Horizontal,
        },
        Operation::DeleteObject {
            slide_id: slide.clone(),
            object_id: ids[3],
        },
        Operation::SetText {
            slide_id: slide.clone(),
            object_id: ids[0],
            paragraphs: vec!["New".into(), "Lines".into()],
        },
        Operation::SetNotes {
            slide_id: slide.clone(),
            text: "Say this".into(),
        },
        Operation::FindReplace {
            find: "rounded".into(),
            replace: "Round".into(),
            match_case: false,
        },
    ]
}

#[test]
fn grouping_and_ungrouping_keep_positions() {
    let mut presentation = deck("pp_shapes.pptx");
    let slide = first_slide_id(&presentation);
    let before: Vec<_> = presentation.slides[0]
        .objects
        .iter()
        .map(|o| o.transform().cloned())
        .collect();
    let ids: Vec<u32> = presentation.slides[0]
        .objects
        .iter()
        .take(2)
        .map(Object::id)
        .collect();
    apply(
        &mut presentation,
        &Operation::Group {
            slide_id: slide.clone(),
            object_ids: ids,
        },
    )
    .unwrap();
    let group = presentation.slides[0]
        .objects
        .iter()
        .find(|o| matches!(o, Object::Group(_)))
        .unwrap()
        .id();
    apply(
        &mut presentation,
        &Operation::Ungroup {
            slide_id: slide,
            group_id: group,
        },
    )
    .unwrap();
    let after: Vec<_> = presentation.slides[0]
        .objects
        .iter()
        .map(|o| o.transform().cloned())
        .collect();
    let mut sorted_before: Vec<_> = before
        .iter()
        .map(|t| t.as_ref().map(|t| (t.x, t.y, t.width, t.height)))
        .collect();
    let mut sorted_after: Vec<_> = after
        .iter()
        .map(|t| t.as_ref().map(|t| (t.x, t.y, t.width, t.height)))
        .collect();
    sorted_before.sort();
    sorted_after.sort();
    assert_eq!(sorted_before, sorted_after);
}

#[test]
fn table_edits_undo_and_keep_the_grid_whole() {
    let presentation = deck("pp_table.pptx");
    let slide = first_slide_id(&presentation);
    let table_id = presentation.slides[0]
        .objects
        .iter()
        .find(|o| matches!(o, Object::Frame(_)))
        .unwrap()
        .id();
    let grid = |p: &Presentation| match &p.slides[0]
        .objects
        .iter()
        .find(|o| o.id() == table_id)
        .unwrap()
    {
        Object::Frame(frame) => match &frame.kind {
            GraphicKind::Table(table) => (
                table.columns.len(),
                table.rows.len(),
                table
                    .rows
                    .iter()
                    .all(|r| r.cells.len() == table.columns.len()),
            ),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let inserted = apply_and_undo(
        presentation.clone(),
        Operation::InsertRow {
            slide_id: slide.clone(),
            object_id: table_id,
            at: 2,
        },
    );
    assert_eq!(grid(&inserted), (3, 5, true));
    let widened = apply_and_undo(
        presentation.clone(),
        Operation::InsertColumn {
            slide_id: slide.clone(),
            object_id: table_id,
            at: 1,
        },
    );
    assert_eq!(grid(&widened), (4, 4, true));
    let narrowed = apply_and_undo(
        presentation.clone(),
        Operation::DeleteColumn {
            slide_id: slide.clone(),
            object_id: table_id,
            at: 2,
        },
    );
    assert_eq!(grid(&narrowed), (2, 4, true));
    apply_and_undo(
        presentation.clone(),
        Operation::SetCellText {
            slide_id: slide.clone(),
            object_id: table_id,
            row: 1,
            column: 1,
            text: "3".into(),
        },
    );
    let shorter = apply_and_undo(
        presentation,
        Operation::DeleteRow {
            slide_id: slide,
            object_id: table_id,
            at: 1,
        },
    );
    assert_eq!(grid(&shorter), (3, 3, true));
}

#[test]
fn theme_colour_changes_reach_everything_that_follows_the_theme() {
    let presentation = deck("pp_title_bullets.pptx");
    let theme = presentation.themes[0].id.clone();
    let changed = apply_and_undo(
        presentation.clone(),
        Operation::SetThemeColor {
            theme_id: theme.clone(),
            slot: "accent1".into(),
            color: ColorRef::rgb("FF0000"),
        },
    );
    let map = presentation.masters[0].color_map.clone();
    let colors = ColorContext {
        scheme: &changed.themes[0].colors,
        map: &map,
        placeholder: None,
    };
    let lighter = ColorRef::scheme("accent1")
        .with("lumMod", 60000)
        .with("lumOff", 40000);
    // red at 50 % lightness, times 0.6 plus 0.4: 70 %
    assert_eq!(resolve(&lighter, &colors).to_hex(), "FF6666");
    let fonts = apply_and_undo(
        presentation,
        Operation::SetThemeFonts {
            theme_id: theme,
            major: "Georgia".into(),
            minor: "Verdana".into(),
        },
    );
    let reread = read_pptx(&write_pptx(&fonts).unwrap()).unwrap();
    assert_eq!(reread.themes[0].fonts.major_latin, "Georgia");
    assert_eq!(reread.themes[0].colors, fonts.themes[0].colors);
}

#[test]
fn slide_size_changes_scale_content() {
    let presentation = deck("pp_shapes.pptx");
    let (width, height) = (
        presentation.slide_size.width,
        presentation.slide_size.height,
    );
    let before = presentation.slides[0].objects[0]
        .transform()
        .cloned()
        .unwrap();
    let stretched = apply_and_undo(
        presentation.clone(),
        Operation::SetSlideSize {
            width: width / 2,
            height,
            scaling: SizeScaling::Stretch,
        },
    );
    let after = stretched.slides[0].objects[0].transform().cloned().unwrap();
    assert_eq!(after.width, (before.width as f64 / 2.0).round() as i64);
    assert_eq!(after.height, before.height);
    let fitted = apply_and_undo(
        presentation,
        Operation::SetSlideSize {
            width: width / 2,
            height,
            scaling: SizeScaling::Fit,
        },
    );
    let after = fitted.slides[0].objects[0].transform().cloned().unwrap();
    assert_eq!(after.width, (before.width as f64 / 2.0).round() as i64);
    assert_eq!(after.height, (before.height as f64 / 2.0).round() as i64);
    let reread = read_pptx(&write_pptx(&fitted).unwrap()).unwrap();
    assert_eq!(reread.slide_size.width, width / 2);
}

#[test]
fn replacing_a_picture_keeps_its_crop() {
    let presentation = deck("pp_pictures.pptx");
    let slide = first_slide_id(&presentation);
    let cropped = presentation.slides[0]
        .objects
        .iter()
        .find_map(|o| match o {
            Object::Picture(p) if p.crop.is_some() => Some((p.id, p.crop)),
            _ => None,
        })
        .unwrap();
    let mut media = presentation
        .media
        .iter()
        .find(|m| m.content_type == "image/png")
        .unwrap()
        .clone();
    media.id = "replacement.png".into();
    let changed = apply_and_undo(
        presentation,
        Operation::ReplacePicture {
            slide_id: slide,
            object_id: cropped.0,
            media,
        },
    );
    let reread = read_pptx(&write_pptx(&changed).unwrap()).unwrap();
    let picture = reread.slides[0].objects.iter().find_map(|o| match o {
        Object::Picture(p) if p.id == cropped.0 => Some(p.clone()),
        _ => None,
    });
    assert_eq!(picture.unwrap().crop, cropped.1);
}
