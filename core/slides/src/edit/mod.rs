//! editing operations as pure functions on the model. each operation
//! says what it changed and returns its inverse, which `apply` undoes it
//! with; the interface keeps the inverses as its undo stack.
//!
//! inverses restore snapshots of what an operation touched (a slide, the
//! slide list, the themes), so undoing is always exact, and an object the
//! operation did not touch keeps its original xml through edit and undo.

mod arrange;
mod deck;
mod objects;
mod slides;
mod tables;
mod text;

use serde::{Deserialize, Serialize};

use crate::error::SlidesError;
use crate::model::{
    ColorRef, Fill, Layout, Line, Master, MediaItem, Object, Presentation, Slide, SlideSection,
    SlideSize, Theme,
};

pub use arrange::{AlignEdge, Axis, ZOrder};
pub use deck::SizeScaling;

/// one edit, as the interface sends it (json, tagged by `op`). the
/// restore variants carry whole slides; an operation is short-lived, so
/// the size difference between variants does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Operation {
    AddSlide {
        index: usize,
        layout_id: String,
    },
    DuplicateSlide {
        index: usize,
    },
    DeleteSlide {
        index: usize,
    },
    MoveSlide {
        from: usize,
        to: usize,
    },
    ApplyLayout {
        slide_id: String,
        layout_id: String,
    },
    /// replaces an object's text, one paragraph per entry, keeping each
    /// paragraph's formatting
    SetText {
        slide_id: String,
        object_id: u32,
        paragraphs: Vec<String>,
    },
    AddObject {
        slide_id: String,
        object: Box<Object>,
    },
    DeleteObject {
        slide_id: String,
        object_id: u32,
    },
    /// replaces an object whole (restyling it, or undoing an edit)
    ReplaceObject {
        slide_id: String,
        object: Box<Object>,
    },
    Move {
        slide_id: String,
        object_ids: Vec<u32>,
        dx: i64,
        dy: i64,
    },
    Resize {
        slide_id: String,
        object_id: u32,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
    },
    Rotate {
        slide_id: String,
        object_id: u32,
        rotation: i32,
    },
    SetFill {
        slide_id: String,
        object_id: u32,
        fill: Option<Fill>,
    },
    SetLine {
        slide_id: String,
        object_id: u32,
        line: Option<Line>,
    },
    Reorder {
        slide_id: String,
        object_id: u32,
        to: ZOrder,
    },
    Align {
        slide_id: String,
        object_ids: Vec<u32>,
        edge: AlignEdge,
        to_slide: bool,
    },
    Distribute {
        slide_id: String,
        object_ids: Vec<u32>,
        axis: Axis,
    },
    Group {
        slide_id: String,
        object_ids: Vec<u32>,
    },
    Ungroup {
        slide_id: String,
        group_id: u32,
    },
    SetCellText {
        slide_id: String,
        object_id: u32,
        row: usize,
        column: usize,
        text: String,
    },
    InsertRow {
        slide_id: String,
        object_id: u32,
        at: usize,
    },
    DeleteRow {
        slide_id: String,
        object_id: u32,
        at: usize,
    },
    InsertColumn {
        slide_id: String,
        object_id: u32,
        at: usize,
    },
    DeleteColumn {
        slide_id: String,
        object_id: u32,
        at: usize,
    },
    /// a new picture in a picture object; its crop and effects stay
    ReplacePicture {
        slide_id: String,
        object_id: u32,
        media: MediaItem,
    },
    SetThemeColor {
        theme_id: String,
        slot: String,
        color: ColorRef,
    },
    SetThemeFonts {
        theme_id: String,
        major: String,
        minor: String,
    },
    SetSlideSize {
        width: i64,
        height: i64,
        scaling: SizeScaling,
    },
    SetNotes {
        slide_id: String,
        text: String,
    },
    FindReplace {
        find: String,
        replace: String,
        match_case: bool,
    },
    /// inverses: put back what an operation changed
    RestoreSlide {
        slide: Box<Slide>,
    },
    RestoreSlides {
        slides: Vec<Slide>,
        sections: Vec<SlideSection>,
    },
    RestoreThemes {
        themes: Vec<Theme>,
    },
    RestoreGeometry {
        size: SlideSize,
        slides: Vec<Slide>,
        layouts: Vec<Layout>,
        masters: Vec<Master>,
    },
    RestoreMedia {
        media: Vec<MediaItem>,
        slide: Box<Slide>,
    },
}

/// what an operation did: the slides whose drawing changed (empty when
/// every slide may have changed, as with a theme) and how to undo it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub changed_slides: Vec<String>,
    pub all_slides: bool,
    pub inverse: Operation,
}

impl Applied {
    fn slide(id: &str, inverse: Operation) -> Applied {
        Applied {
            changed_slides: vec![id.to_string()],
            all_slides: false,
            inverse,
        }
    }

    fn deck(inverse: Operation) -> Applied {
        Applied {
            changed_slides: Vec::new(),
            all_slides: true,
            inverse,
        }
    }
}

/// applies one operation. on an error nothing has changed.
pub fn apply(
    presentation: &mut Presentation,
    operation: &Operation,
) -> Result<Applied, SlidesError> {
    use Operation as O;
    match operation {
        O::AddSlide { .. }
        | O::DuplicateSlide { .. }
        | O::DeleteSlide { .. }
        | O::MoveSlide { .. } => slides::apply_slide_list(presentation, operation),
        O::ApplyLayout {
            slide_id,
            layout_id,
        } => slides::apply_layout(presentation, slide_id, layout_id),
        O::SetText { .. } | O::SetNotes { .. } | O::FindReplace { .. } => {
            text::apply_text(presentation, operation)
        }
        O::SetCellText { .. }
        | O::InsertRow { .. }
        | O::DeleteRow { .. }
        | O::InsertColumn { .. }
        | O::DeleteColumn { .. } => tables::apply_table(presentation, operation),
        O::Reorder { .. }
        | O::Align { .. }
        | O::Distribute { .. }
        | O::Group { .. }
        | O::Ungroup { .. } => arrange::apply_arrange(presentation, operation),
        O::SetThemeColor { .. }
        | O::SetThemeFonts { .. }
        | O::SetSlideSize { .. }
        | O::ReplacePicture { .. } => deck::apply_deck(presentation, operation),
        O::RestoreSlide { .. }
        | O::RestoreSlides { .. }
        | O::RestoreThemes { .. }
        | O::RestoreGeometry { .. }
        | O::RestoreMedia { .. } => restore(presentation, operation),
        _ => objects::apply_object(presentation, operation),
    }
}

/// the slide with this id, for changing.
pub(crate) fn slide_mut<'a>(
    presentation: &'a mut Presentation,
    id: &str,
) -> Result<&'a mut Slide, SlidesError> {
    presentation
        .slides
        .iter_mut()
        .find(|slide| slide.id == id)
        .ok_or_else(|| SlidesError::InvalidEdit(format!("there is no slide \"{id}\"")))
}

/// a snapshot of a slide and the inverse that puts it back.
pub(crate) fn snapshot(
    presentation: &mut Presentation,
    id: &str,
) -> Result<Operation, SlidesError> {
    let slide = slide_mut(presentation, id)?.clone();
    Ok(Operation::RestoreSlide {
        slide: Box::new(slide),
    })
}

/// the object with this id on a slide (top level).
pub(crate) fn object_index(slide: &Slide, id: u32) -> Result<usize, SlidesError> {
    slide
        .objects
        .iter()
        .position(|object| object.id() == id)
        .ok_or_else(|| SlidesError::InvalidEdit(format!("there is no object {id} on this slide")))
}

/// an id no object on the slide (or inside its groups) uses.
pub(crate) fn free_object_id(slide: &Slide) -> u32 {
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

fn restore(presentation: &mut Presentation, operation: &Operation) -> Result<Applied, SlidesError> {
    match operation {
        Operation::RestoreSlide { slide } => {
            let inverse = snapshot(presentation, &slide.id)?;
            *slide_mut(presentation, &slide.id)? = (**slide).clone();
            Ok(Applied::slide(&slide.id, inverse))
        }
        Operation::RestoreSlides { slides, sections } => {
            let inverse = Operation::RestoreSlides {
                slides: std::mem::replace(&mut presentation.slides, slides.clone()),
                sections: std::mem::replace(&mut presentation.sections, sections.clone()),
            };
            Ok(Applied::deck(inverse))
        }
        Operation::RestoreThemes { themes } => {
            let inverse = Operation::RestoreThemes {
                themes: std::mem::replace(&mut presentation.themes, themes.clone()),
            };
            Ok(Applied::deck(inverse))
        }
        Operation::RestoreGeometry {
            size,
            slides,
            layouts,
            masters,
        } => {
            let inverse = Operation::RestoreGeometry {
                size: std::mem::replace(&mut presentation.slide_size, size.clone()),
                slides: std::mem::replace(&mut presentation.slides, slides.clone()),
                layouts: std::mem::replace(&mut presentation.layouts, layouts.clone()),
                masters: std::mem::replace(&mut presentation.masters, masters.clone()),
            };
            Ok(Applied::deck(inverse))
        }
        Operation::RestoreMedia { media, slide } => {
            let inverse_slide = slide_mut(presentation, &slide.id)?.clone();
            *slide_mut(presentation, &slide.id)? = (**slide).clone();
            let inverse = Operation::RestoreMedia {
                media: std::mem::replace(&mut presentation.media, media.clone()),
                slide: Box::new(inverse_slide),
            };
            Ok(Applied::slide(&slide.id, inverse))
        }
        _ => Err(SlidesError::InvalidEdit("not a restore".into())),
    }
}
