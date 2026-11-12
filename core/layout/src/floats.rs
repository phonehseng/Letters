//! floating pictures and text boxes: where they sit on the page and how
//! text flows around them.
//!
//! a floating object is positioned relative to the page, the margins, the
//! column or its anchor paragraph, by an offset or an alignment. text then
//! wraps by the object's wrap mode:
//!
//! - square, tight, through: lines beside the object are shortened to the
//!   side with more room (word's "both sides" can put text on both; this
//!   model uses the larger side, a known gap), keeping word's default
//!   0.125 inch distance at the sides;
//! - top and bottom: lines that would meet the object move below it;
//! - behind and in front of text: text ignores the object.

use std::borrow::Cow;

use letters_office::model::{FloatPosition, Placement, TextBox, Wrap};

use crate::pages::{Flow, PreparedParagraph};
use crate::paragraph::{layout_paragraph_in, LineSpace};
use crate::result::{Frame, PlacedImage, Story};
use crate::text::ClusterKind;
use crate::unsupported::{Unsupported, UnsupportedKind};

/// word's default distance between a square-wrapped object and the text
/// at its sides (wp:anchor distL/distR = 114300 emu)
const SIDE_DISTANCE: f64 = 9.0;

/// a floating object's box on its page and how text treats it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub wrap: Wrap,
}

impl FloatBox {
    fn affects_text(&self) -> bool {
        !matches!(self.wrap, Wrap::BehindText | Wrap::InFrontOfText)
    }

    fn overlaps_vertically(&self, top: f64, bottom: f64) -> bool {
        top < self.y + self.height && bottom > self.y
    }
}

/// the box of a floating object of `width` by `height` on the current page,
/// for an anchor paragraph whose first line starts at `paragraph_top`.
pub fn float_box(
    flow: &Flow,
    horizontal: &FloatPosition,
    vertical: &FloatPosition,
    width: f64,
    height: f64,
    paragraph_top: f64,
    wrap: Wrap,
) -> FloatBox {
    let page = flow.page_ref();
    let geometry = &page.geometry;
    let (column_left, column_width) = flow.column_box();
    let (base_x, extent_x) = match horizontal.relative_to.as_str() {
        "page" => (0.0, geometry.width),
        "column" | "character" => (column_left, column_width),
        "leftMargin" => (0.0, geometry.left),
        "rightMargin" => (geometry.width - geometry.right, geometry.right),
        _ => (geometry.left, geometry.text_width()),
    };
    let x = match horizontal.align.as_deref() {
        Some("left") | Some("inside") => base_x,
        Some("right") | Some("outside") => base_x + extent_x - width,
        Some("center") => base_x + (extent_x - width) / 2.0,
        _ => base_x + crate::units::twips_to_points(horizontal.offset),
    };
    let (base_y, extent_y) = match vertical.relative_to.as_str() {
        "page" => (0.0, geometry.height),
        "margin" | "topMargin" => (
            geometry.top,
            geometry.height - geometry.top - geometry.bottom,
        ),
        "bottomMargin" => (geometry.height - geometry.bottom, geometry.bottom),
        _ => (paragraph_top, 0.0),
    };
    let y = match vertical.align.as_deref() {
        Some("top") => base_y,
        Some("bottom") => base_y + extent_y - height,
        Some("center") => base_y + (extent_y - height) / 2.0,
        _ => base_y + crate::units::twips_to_points(vertical.offset),
    };
    FloatBox {
        x,
        y,
        width,
        height,
        wrap,
    }
}

impl Flow<'_, '_> {
    /// puts the floating pictures anchored in this paragraph on the page.
    pub fn place_anchored_objects(&mut self, paragraph: &PreparedParagraph, paragraph_top: f64) {
        for cluster in &paragraph.text.clusters {
            let ClusterKind::Anchor(index) = cluster.kind else {
                continue;
            };
            let object = &paragraph.text.objects[index as usize];
            let Placement::Floating {
                horizontal,
                vertical,
                wrap,
            } = &object.placement
            else {
                continue;
            };
            if *wrap == Wrap::Tight || *wrap == Wrap::Through {
                let at = crate::pages::location(&paragraph.story, &paragraph.path);
                self.context
                    .unsupported
                    .push(Unsupported::new(UnsupportedKind::TightWrap, &at));
            }
            let found = float_box(
                self,
                horizontal,
                vertical,
                object.width,
                object.height,
                paragraph_top,
                *wrap,
            );
            let page = self.page();
            page.page.images.push(PlacedImage {
                media_id: object.media_id.clone(),
                x: found.x,
                y: found.y,
                width: found.width,
                height: found.height,
                story: paragraph.story.clone(),
                block: paragraph.path.clone(),
                offset: cluster.offset,
                floating: true,
            });
            page.floats.push(found);
        }
    }

    /// a text box block: its frame and text on the page, at its position.
    pub fn place_text_box(&mut self, text_box: &TextBox, block: u32) {
        let width = crate::units::twips_to_points(text_box.width.max(0));
        let height = crate::units::twips_to_points(text_box.height.max(0));
        let found = float_box(
            self,
            &text_box.horizontal,
            &text_box.vertical,
            width,
            height,
            self.y,
            text_box.wrap,
        );
        let margins = text_box.margins;
        let inner_left = found.x + crate::units::twips_to_points(margins.left);
        let inner_width =
            (width - crate::units::twips_to_points(margins.left + margins.right)).max(1.0);
        let inner_top = found.y + crate::units::twips_to_points(margins.top);
        let story = Story::TextBox { block };
        let (emitted, content_height) = crate::stack::stack_blocks(
            self.context,
            &text_box.blocks,
            &story,
            &[],
            inner_left,
            inner_width,
            inner_top,
            crate::text::FieldValues::default(),
        );
        let frame_height = if text_box.auto_height {
            content_height + crate::units::twips_to_points(margins.top + margins.bottom)
        } else {
            height
        };
        let page = self.page();
        page.page.frames.push(Frame {
            x: found.x,
            y: found.y,
            width,
            height: frame_height,
            fill: text_box
                .fill
                .as_deref()
                .and_then(crate::style::parse_hex_color),
            border: text_box
                .border
                .as_ref()
                .filter(|border| border.style != "none")
                .map(|border| {
                    let color = border
                        .color
                        .as_deref()
                        .and_then(crate::style::parse_hex_color)
                        .unwrap_or([0, 0, 0]);
                    (color, f64::from(border.size) / 8.0)
                }),
            block,
        });
        emitted.apply_to(&mut page.page);
        page.floats.push(FloatBox {
            height: frame_height,
            ..found
        });
    }

    /// the paragraph laid out again around the floats on this page, when
    /// any of them reaches its lines; otherwise unchanged.
    pub fn with_wrapping<'p>(
        &self,
        paragraph: &'p PreparedParagraph,
        before: f64,
    ) -> Cow<'p, PreparedParagraph> {
        let page = self.page_ref();
        let top = self.y + before;
        let estimate = paragraph.layout.height() * 1.5 + 72.0;
        let floats: Vec<FloatBox> = page
            .floats
            .iter()
            .filter(|f| f.affects_text() && f.overlaps_vertically(top, top + estimate))
            .copied()
            .collect();
        if floats.is_empty() {
            return Cow::Borrowed(paragraph);
        }
        let (column_left, column_width) = self.column_box();
        let heights: Vec<f64> = paragraph
            .layout
            .lines
            .iter()
            .map(|l| l.vertical.height)
            .collect();
        let flavor = self.context.flavor;
        let mut space = |index: usize, offset: f64| {
            let height = heights
                .get(index)
                .or(heights.last())
                .copied()
                .unwrap_or(12.0);
            room_beside(&floats, column_left, column_width, top + offset, height)
        };
        let layout = layout_paragraph_in(&paragraph.text, &paragraph.style, flavor, &mut space);
        let mut wrapped = paragraph.clone();
        wrapped.layout = layout;
        Cow::Owned(wrapped)
    }
}

/// the room a line between `top` and `top + height` has beside the floats:
/// the free part of the column left of the floats it meets and, when there
/// is room on both sides, the part right of them as a second piece of the
/// same line. word wraps square text on both sides by default (wrapText
/// "bothSides"): w_floating_aptos put words left of a picture standing
/// 60 pt into the column and went on right of it on the same baseline.
fn room_beside(
    floats: &[FloatBox],
    column_left: f64,
    column_width: f64,
    top: f64,
    height: f64,
) -> LineSpace {
    let mut move_down: f64 = 0.0;
    let mut blocked: Vec<(f64, f64)> = Vec::new();
    for float in floats {
        if !float.overlaps_vertically(top + move_down, top + move_down + height) {
            continue;
        }
        if float.wrap == Wrap::TopAndBottom {
            move_down = move_down.max(float.y + float.height - top);
            continue;
        }
        let start = float.x - SIDE_DISTANCE - column_left;
        let end = float.x + float.width + SIDE_DISTANCE - column_left;
        blocked.push((start.max(0.0), end.min(column_width)));
    }
    let free = free_parts(&blocked, column_width);
    let mut parts = free.into_iter().filter(|(_, width)| *width > 0.0);
    let first = parts.next().unwrap_or((0.0, 0.0));
    LineSpace {
        left: first.0,
        width: first.1,
        move_down,
        second: parts.next(),
    }
}

/// the parts of `0..width` no blocked interval covers, left to right, as
/// (left, width).
fn free_parts(blocked: &[(f64, f64)], width: f64) -> Vec<(f64, f64)> {
    let mut sorted = blocked.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut parts = Vec::new();
    let mut from = 0.0;
    for (start, end) in sorted {
        if start > from {
            parts.push((from, start - from));
        }
        from = f64::max(from, end);
    }
    if width > from {
        parts.push((from, width - from));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64) -> FloatBox {
        FloatBox {
            x,
            y,
            width: 100.0,
            height: 50.0,
            wrap: Wrap::Square,
        }
    }

    #[test]
    fn text_goes_beside_a_picture_at_either_edge() {
        // a picture at the left of a column from 72 to 540
        let room = room_beside(&[square(72.0, 100.0)], 72.0, 468.0, 110.0, 14.0);
        assert_eq!(room.left, 109.0);
        assert_eq!(room.width, 359.0);
        assert_eq!(room.second, None);
        // and one at the right
        let room = room_beside(&[square(440.0, 100.0)], 72.0, 468.0, 110.0, 14.0);
        assert_eq!(room.left, 0.0);
        assert_eq!(room.width, 359.0);
    }

    #[test]
    fn a_picture_in_the_middle_leaves_room_on_both_sides() {
        let room = room_beside(&[square(200.0, 100.0)], 72.0, 468.0, 110.0, 14.0);
        assert_eq!((room.left, room.width), (0.0, 119.0));
        assert_eq!(room.second, Some((237.0, 231.0)));
    }

    #[test]
    fn lines_above_and_below_are_not_shortened() {
        let room = room_beside(&[square(72.0, 100.0)], 72.0, 468.0, 160.0, 14.0);
        assert_eq!(room.width, 468.0);
    }

    #[test]
    fn top_and_bottom_moves_lines_below() {
        let float = FloatBox {
            wrap: Wrap::TopAndBottom,
            ..square(200.0, 100.0)
        };
        let room = room_beside(&[float], 72.0, 468.0, 110.0, 14.0);
        assert_eq!(room.move_down, 40.0);
        assert_eq!(room.width, 468.0);
    }
}
