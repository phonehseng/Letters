//! deck-wide edits: theme colours and fonts (everything that follows the
//! theme changes with them), the slide size with scaling, and replacing
//! a picture while keeping its crop.

use serde::{Deserialize, Serialize};

use super::{object_index, slide_mut, Applied, Operation};
use crate::error::SlidesError;
use crate::model::{Object, Presentation, Transform};

/// what happens to the content when the slide size changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SizeScaling {
    /// everything keeps its size and position
    DontScale,
    /// everything shrinks or grows by the same factor so it all fits
    /// (powerpoint's "ensure fit"), centred
    Fit,
    /// widths follow the width, heights the height
    Stretch,
}

pub fn apply_deck(
    presentation: &mut Presentation,
    operation: &Operation,
) -> Result<Applied, SlidesError> {
    match operation {
        Operation::SetThemeColor {
            theme_id,
            slot,
            color,
        } => {
            let inverse = Operation::RestoreThemes {
                themes: presentation.themes.clone(),
            };
            let theme = theme_mut(presentation, theme_id)?;
            let target = theme.colors.slot_mut(slot).ok_or_else(|| {
                SlidesError::InvalidEdit(format!("\"{slot}\" is not a theme colour"))
            })?;
            *target = color.clone();
            Ok(Applied::deck(inverse))
        }
        Operation::SetThemeFonts {
            theme_id,
            major,
            minor,
        } => {
            let inverse = Operation::RestoreThemes {
                themes: presentation.themes.clone(),
            };
            let theme = theme_mut(presentation, theme_id)?;
            theme.fonts.major_latin = major.clone();
            theme.fonts.minor_latin = minor.clone();
            Ok(Applied::deck(inverse))
        }
        Operation::SetSlideSize {
            width,
            height,
            scaling,
        } => set_size(presentation, *width, *height, *scaling),
        Operation::ReplacePicture {
            slide_id,
            object_id,
            media,
        } => replace_picture(presentation, slide_id, *object_id, media),
        _ => Err(SlidesError::InvalidEdit("not a deck edit".into())),
    }
}

fn theme_mut<'a>(
    presentation: &'a mut Presentation,
    id: &str,
) -> Result<&'a mut crate::model::Theme, SlidesError> {
    presentation
        .themes
        .iter_mut()
        .find(|theme| theme.id == id)
        .ok_or_else(|| SlidesError::InvalidEdit(format!("there is no theme \"{id}\"")))
}

/// the slide size changed on slides, layouts and masters alike.
fn set_size(
    presentation: &mut Presentation,
    width: i64,
    height: i64,
    scaling: SizeScaling,
) -> Result<Applied, SlidesError> {
    if width < 914_400 || height < 914_400 {
        return Err(SlidesError::InvalidEdit(
            "slides must be at least an inch on each side".into(),
        ));
    }
    let inverse = Operation::RestoreGeometry {
        size: presentation.slide_size.clone(),
        slides: presentation.slides.clone(),
        layouts: presentation.layouts.clone(),
        masters: presentation.masters.clone(),
    };
    let old = presentation.slide_size.clone();
    let scale_x = width as f64 / old.width.max(1) as f64;
    let scale_y = height as f64 / old.height.max(1) as f64;
    let (sx, sy, dx, dy) = match scaling {
        SizeScaling::DontScale => (1.0, 1.0, 0.0, 0.0),
        SizeScaling::Stretch => (scale_x, scale_y, 0.0, 0.0),
        SizeScaling::Fit => {
            let factor = scale_x.min(scale_y);
            let dx = (width as f64 - old.width as f64 * factor) / 2.0;
            let dy = (height as f64 - old.height as f64 * factor) / 2.0;
            (factor, factor, dx, dy)
        }
    };
    let mapping = Mapping { sx, sy, dx, dy };
    for objects in presentation
        .slides
        .iter_mut()
        .map(|s| &mut s.objects)
        .chain(presentation.layouts.iter_mut().map(|l| &mut l.objects))
        .chain(presentation.masters.iter_mut().map(|m| &mut m.objects))
    {
        for object in objects.iter_mut() {
            mapping.apply(object);
        }
    }
    presentation.slide_size.width = width;
    presentation.slide_size.height = height;
    presentation.slide_size.kind = Some("custom".into());
    Ok(Applied::deck(inverse))
}

struct Mapping {
    sx: f64,
    sy: f64,
    dx: f64,
    dy: f64,
}

impl Mapping {
    fn transform(&self, transform: &mut Transform) {
        transform.x = (transform.x as f64 * self.sx + self.dx).round() as i64;
        transform.y = (transform.y as f64 * self.sy + self.dy).round() as i64;
        transform.width = (transform.width as f64 * self.sx).round() as i64;
        transform.height = (transform.height as f64 * self.sy).round() as i64;
    }

    /// top-level objects move and resize; a group's children keep their
    /// child coordinates, which the group's box now stretches further.
    fn apply(&self, object: &mut Object) {
        match object {
            Object::Group(group) => self.transform(&mut group.transform),
            other => {
                if let Some(Some(transform)) = other.transform_mut() {
                    self.transform(transform);
                }
            }
        }
    }
}

/// a new picture for a picture object. the media table gains the new
/// bytes; the object keeps its crop, outline and effects.
fn replace_picture(
    presentation: &mut Presentation,
    slide_id: &str,
    object_id: u32,
    media: &crate::model::MediaItem,
) -> Result<Applied, SlidesError> {
    let slide_before = slide_mut(presentation, slide_id)?.clone();
    let inverse = Operation::RestoreMedia {
        media: presentation.media.clone(),
        slide: Box::new(slide_before),
    };
    let mut item = media.clone();
    item.source_path = None;
    while presentation.media.iter().any(|m| m.id == item.id) {
        item.id = format!("new-{}", item.id);
    }
    let new_id = item.id.clone();
    presentation.media.push(item);
    let slide = slide_mut(presentation, slide_id)?;
    let index = object_index(slide, object_id)?;
    let Object::Picture(picture) = &mut slide.objects[index] else {
        return Err(SlidesError::InvalidEdit(
            "that object is not a picture".into(),
        ));
    };
    picture.media_id = Some(new_id);
    Ok(Applied::slide(slide_id, inverse))
}
