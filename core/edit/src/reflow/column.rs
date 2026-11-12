//! the column a paragraph sits in, what lies below it there, and how far
//! down that content may go.
//!
//! the column band is the paragraph's left-to-right extent widened by the
//! text blocks stacked in the same column. content below the paragraph
//! whose box lies inside the band moves with it; anything below that
//! reaches across the band edge (a full-width picture, the other column's
//! table) is an obstacle that the column must stop above. the bottom
//! limit is the first of: the caller's limit, an obstacle, the footer,
//! the bottom margin.

use lopdf::Document;
use serde::{Deserialize, Serialize};

use crate::draft::Draft;
use crate::geometry::Rect;
use crate::layout::PageModel;
use crate::model::PageObjects;
use crate::moving::{self, Annotation};

use super::footer;

/// a block counts as stacked in the column when at least this share of
/// its own width lies inside the edited paragraph's extent.
const STACKED_SHARE: f32 = 0.5;
/// points a box may stick out of the band and still count as inside it
/// (anti-aliased edges, italic overhangs).
const BAND_SLACK: f32 = 2.0;
/// content stops this many points above an obstacle.
const OBSTACLE_GAP: f32 = 2.0;
/// pages rarely have a bottom margin under half an inch; without a footer
/// the margin is taken to match the top margin, but never less than this.
const MIN_BOTTOM_MARGIN: f32 = 36.0;

/// what decided how far down the column may go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LimitReason {
    /// the limit the app passed in
    Caller,
    /// something below reaches across the column (stop above it)
    Obstacle,
    /// text that repeats at the foot of the neighbouring pages
    Footer,
    /// the page's bottom margin, matched to its top margin
    BottomMargin,
}

/// the left and right edges of a column on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Band {
    pub left: f32,
    pub right: f32,
}

impl Band {
    fn holds(&self, rect: &Rect) -> bool {
        rect.x >= self.left - BAND_SLACK && rect.right() <= self.right + BAND_SLACK
    }

    fn crosses(&self, rect: &Rect) -> bool {
        let overlap = rect.right().min(self.right) - rect.x.max(self.left);
        overlap > BAND_SLACK && !self.holds(rect)
    }
}

/// the content below the paragraph that moves with it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Below {
    pub blocks: Vec<usize>,
    pub images: Vec<usize>,
    pub paths: Vec<usize>,
    pub annotations: Vec<Annotation>,
    /// the lowest bottom edge among them
    pub lowest: Option<f32>,
}

impl Below {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
            && self.images.is_empty()
            && self.paths.is_empty()
            && self.annotations.is_empty()
    }

    fn take(&mut self, rect: &Rect) {
        self.lowest = Some(
            self.lowest
                .map_or(rect.bottom(), |low| low.max(rect.bottom())),
        );
    }
}

/// a paragraph's column, worked out once per edit.
#[derive(Clone, Debug)]
pub(crate) struct Column {
    pub band: Band,
    pub below: Below,
    pub limit: f32,
    pub reason: LimitReason,
}

/// what the analysis may look at besides the page itself.
pub(crate) struct ColumnInputs<'a> {
    pub doc: &'a Document,
    pub caller_limit: Option<f32>,
    pub look_at_neighbour_pages: bool,
}

/// works out the column of block `block` (with box `rect`).
pub(crate) fn analyse(
    draft: &Draft,
    model: &PageModel,
    objects: &PageObjects,
    block: usize,
    inputs: &ColumnInputs,
) -> Column {
    let rect = objects.text_blocks[block].rect;
    let band = band_of(objects, &rect);
    let (mut limit, mut reason) = page_limit(model, objects, inputs, &rect);
    if let Some(obstacle) = first_obstacle(objects, &band, &rect, limit) {
        limit = obstacle - OBSTACLE_GAP;
        reason = LimitReason::Obstacle;
    }
    let below = content_below(draft, model, objects, block, &band, &rect, limit);
    Column {
        band,
        below,
        limit,
        reason,
    }
}

/// the same column on another page, from the top of its content down:
/// where carried lines go and what they push. none when that band of
/// the page is empty (then the lines go at `fallback_top`).
pub(crate) fn column_on_page(
    draft: &Draft,
    model: &PageModel,
    objects: &PageObjects,
    band: Band,
    inputs: &ColumnInputs,
    fallback_top: f32,
) -> (f32, Column) {
    let top = content_rects(objects)
        .filter(|rect| band.holds(rect))
        .map(|rect| rect.y)
        .reduce(f32::min)
        .unwrap_or(fallback_top);
    // a line just above the top, so everything from the top down counts
    let start = Rect::new(band.left, top - 1.0, band.right - band.left, 0.5);
    let (limit, reason) = page_limit(model, objects, inputs, &start);
    let no_block = usize::MAX;
    let below = content_below(draft, model, objects, no_block, &band, &start, limit);
    let column = Column {
        band,
        below,
        limit,
        reason,
    };
    (top, column)
}

/// the content inside `band` below `rect` (a table row), and how far it
/// may go, the way `analyse` finds them for a paragraph.
pub(crate) fn below_area(
    draft: &Draft,
    model: &PageModel,
    objects: &PageObjects,
    rect: &Rect,
    band: Band,
    inputs: &ColumnInputs,
) -> Column {
    let (mut limit, mut reason) = page_limit(model, objects, inputs, rect);
    if let Some(obstacle) = first_obstacle(objects, &band, rect, limit) {
        limit = obstacle - OBSTACLE_GAP;
        reason = LimitReason::Obstacle;
    }
    let no_block = usize::MAX;
    let below = content_below(draft, model, objects, no_block, &band, rect, limit);
    Column {
        band,
        below,
        limit,
        reason,
    }
}

/// the paragraph's extent widened by the blocks stacked with it.
fn band_of(objects: &PageObjects, rect: &Rect) -> Band {
    let mut band = Band {
        left: rect.x,
        right: rect.right(),
    };
    for other in &objects.text_blocks {
        let overlap = other.rect.right().min(rect.right()) - other.rect.x.max(rect.x);
        if overlap >= STACKED_SHARE * other.rect.width {
            band.left = band.left.min(other.rect.x);
            band.right = band.right.max(other.rect.right());
        }
    }
    band
}

/// the limit from the caller, the footer or the margins.
fn page_limit(
    model: &PageModel,
    objects: &PageObjects,
    inputs: &ColumnInputs,
    rect: &Rect,
) -> (f32, LimitReason) {
    if let Some(limit) = inputs.caller_limit {
        return (limit, LimitReason::Caller);
    }
    let footer = if inputs.look_at_neighbour_pages {
        footer::footer_top(inputs.doc, model.page, objects)
    } else {
        None
    };
    let lowest_body = lowest_content_above(objects, footer.unwrap_or(f32::MAX)).max(rect.bottom());
    match footer {
        // the gap the page leaves above its footer stays
        Some(top) => (lowest_body.max(top - OBSTACLE_GAP), LimitReason::Footer),
        None => {
            let margin = top_margin(objects).max(MIN_BOTTOM_MARGIN);
            (
                (objects.height - margin).max(lowest_body),
                LimitReason::BottomMargin,
            )
        }
    }
}

/// every content box on the page, text, pictures and drawings.
fn content_rects(objects: &PageObjects) -> impl Iterator<Item = Rect> + '_ {
    let page_area = objects.width * objects.height;
    let blocks = objects.text_blocks.iter().map(|block| block.rect);
    let images = objects.images.iter().map(|image| image.rect);
    // a path covering most of the page is a background, not content
    let paths = objects
        .paths
        .iter()
        .map(|path| path.rect)
        .filter(move |rect| rect.width * rect.height < 0.5 * page_area);
    blocks.chain(images).chain(paths)
}

fn lowest_content_above(objects: &PageObjects, line: f32) -> f32 {
    content_rects(objects)
        .filter(|rect| rect.bottom() < line)
        .map(|rect| rect.bottom())
        .fold(0.0, f32::max)
}

fn top_margin(objects: &PageObjects) -> f32 {
    content_rects(objects)
        .map(|rect| rect.y)
        .fold(objects.height, f32::min)
}

/// the top of the first thing below the paragraph that reaches across
/// the band's edge, above the limit.
fn first_obstacle(objects: &PageObjects, band: &Band, rect: &Rect, limit: f32) -> Option<f32> {
    content_rects(objects)
        .filter(|other| other.y >= rect.bottom() - 0.5 && other.y < limit)
        .filter(|other| band.crosses(other))
        .map(|other| other.y)
        .reduce(f32::min)
}

/// everything inside the band below the paragraph and above the limit.
fn content_below(
    draft: &Draft,
    model: &PageModel,
    objects: &PageObjects,
    block: usize,
    band: &Band,
    rect: &Rect,
    limit: f32,
) -> Below {
    let moves =
        |other: &Rect| other.y >= rect.bottom() - 0.5 && other.y < limit && band.holds(other);
    let mut below = Below::default();
    for (index, other) in objects.text_blocks.iter().enumerate() {
        if index != block && moves(&other.rect) {
            below.blocks.push(index);
            below.take(&other.rect);
        }
    }
    for (index, image) in objects.images.iter().enumerate() {
        if moves(&image.rect) {
            below.images.push(index);
            below.take(&image.rect);
        }
    }
    for (index, path) in objects.paths.iter().enumerate() {
        if moves(&path.rect) {
            below.paths.push(index);
            below.take(&path.rect);
        }
    }
    for annotation in moving::annotations(draft, model) {
        if moves(&annotation.rect) {
            below.take(&annotation.rect);
            below.annotations.push(annotation);
        }
    }
    below
}
