//! finding places on a flat (field-less) form where answers go.
//!
//! many forms are just drawings: a line to write on, a row of small boxes,
//! a table with empty cells. this module reads where lines, boxes, circles
//! and letters are drawn (see `content`) and proposes fields from simple
//! rules about their shapes. it only proposes: the ui shows the proposals
//! and the person confirms each one before `add_field` makes it real.
//!
//! the rules work on the page as the reader sees it (after its turn), so
//! "horizontal" means horizontal on screen.

mod cmap;
mod content;

use lopdf::Document;
use serde::{Deserialize, Serialize};

use crate::error::FormResult;
use crate::geometry::{PageFrame, VisibleRect};
use crate::objects::{check_document, get_rect, page_frame, page_id, references_in};
use content::{read_page_marks, PageMarks, ShapeKind};

/// what kind of field a proposal is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DetectedKind {
    Text,
    MultilineText,
    Checkbox,
    Radio,
    /// a row of equal small boxes, one character each
    Comb,
}

/// one proposed field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedField {
    pub kind: DetectedKind,
    /// in points from the page's visible top left
    pub rect: VisibleRect,
    /// nearby text that probably names the field ("Name:", "Date of birth")
    pub label: Option<String>,
    /// a field name made from the label, unique among the proposals
    pub suggested_name: String,
    /// for comb fields: how many boxes
    pub max_length: Option<u32>,
    /// radio buttons in one row or column share a group number
    pub group: Option<u32>,
    /// 0 to 1: how sure the rules are. lines and boxes with a label nearby
    /// score higher than lone ones
    pub confidence: f32,
}

/// lines shorter than this (in points) are not offered as places to write
const MIN_LINE_LENGTH: f32 = 36.0;
/// the size range of checkboxes and radio buttons, in points
const MIN_TICK_BOX: f32 = 5.0;
const MAX_TICK_BOX: f32 = 24.0;
/// how close two drawn things must be to count as touching
const TOUCH: f32 = 1.5;

/// proposes fields for one page (0-based) from what is drawn on it.
pub fn detect_fields(doc: &Document, page: u32) -> FormResult<Vec<DetectedField>> {
    check_document(doc)?;
    let page_id = page_id(doc, page)?;
    let frame = page_frame(doc, page_id);
    let marks = read_page_marks(doc, page_id);
    let layout = Layout::from_marks(&marks, &frame);

    let mut found = Vec::new();
    let mut used_boxes = vec![false; layout.boxes.len()];
    found.extend(layout.comb_fields(&mut used_boxes));
    found.extend(layout.checkboxes(&mut used_boxes));
    found.extend(layout.box_fields(&used_boxes));
    found.extend(layout.radio_buttons());
    found.extend(layout.underscore_fields());
    found.extend(layout.line_fields());

    let existing = existing_widgets(doc, page_id, &frame);
    found.retain(|field| {
        !existing
            .iter()
            .any(|widget| overlap_share(&field.rect, widget) > 0.3)
    });
    let mut found = drop_duplicates(found);
    found.sort_by(|a, b| reading_order(&a.rect, &b.rect));
    name_fields(&mut found);
    Ok(found)
}

/// a horizontal line on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
struct HorizontalLine {
    y: f32,
    left: f32,
    right: f32,
    /// part of a drawn box, so not a line to write on by itself
    box_edge: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VerticalLine {
    x: f32,
    top: f32,
    bottom: f32,
}

/// a letter on screen.
#[derive(Clone, Debug, PartialEq)]
struct Letter {
    character: char,
    rect: VisibleRect,
}

/// a run of words on one line with no big gap in it.
#[derive(Clone, Debug, PartialEq)]
struct Phrase {
    text: String,
    rect: VisibleRect,
}

/// a run of underscores ("Name: ________").
#[derive(Clone, Debug, PartialEq)]
struct UnderscoreRun {
    rect: VisibleRect,
}

/// the page's marks, turned to screen space and sorted into kinds.
struct Layout {
    horizontal: Vec<HorizontalLine>,
    boxes: Vec<VisibleRect>,
    circles: Vec<VisibleRect>,
    letters: Vec<Letter>,
    phrases: Vec<Phrase>,
    underscores: Vec<UnderscoreRun>,
}

impl Layout {
    fn from_marks(marks: &PageMarks, frame: &PageFrame) -> Self {
        let (page_width, page_height) = frame.visible_size();
        let page_area = page_width * page_height;
        let mut horizontal = Vec::new();
        let mut vertical = Vec::new();
        let mut boxes = Vec::new();
        let mut circles = Vec::new();

        for shape in &marks.shapes {
            let rect = frame.to_visible_rect(&shape.bounds);
            match shape.kind {
                ShapeKind::Rectangle => {
                    if rect.height <= 2.5 && rect.width > 2.5 {
                        horizontal.push(HorizontalLine {
                            y: rect.y + rect.height / 2.0,
                            left: rect.x,
                            right: rect.right(),
                            box_edge: false,
                        });
                    } else if rect.width <= 2.5 && rect.height > 2.5 {
                        vertical.push(VerticalLine {
                            x: rect.x + rect.width / 2.0,
                            top: rect.y,
                            bottom: rect.bottom(),
                        });
                    } else if rect.width > 2.5
                        && rect.height > 2.5
                        && rect.width * rect.height < page_area * 0.5
                    {
                        boxes.push(rect);
                        add_box_edges(&rect, &mut horizontal, &mut vertical);
                    }
                }
                ShapeKind::Lines => {
                    for segment in &shape.segments {
                        let (x0, y0) = frame.to_visible_point(segment.from);
                        let (x1, y1) = frame.to_visible_point(segment.to);
                        if (y0 - y1).abs() < 1.0 && (x0 - x1).abs() > 2.5 {
                            horizontal.push(HorizontalLine {
                                y: (y0 + y1) / 2.0,
                                left: x0.min(x1),
                                right: x0.max(x1),
                                box_edge: false,
                            });
                        } else if (x0 - x1).abs() < 1.0 && (y0 - y1).abs() > 2.5 {
                            vertical.push(VerticalLine {
                                x: (x0 + x1) / 2.0,
                                top: y0.min(y1),
                                bottom: y0.max(y1),
                            });
                        }
                    }
                }
                ShapeKind::Curved => {
                    let side = rect.width.max(rect.height);
                    let round = (rect.width - rect.height).abs() <= 0.2 * side;
                    if round && (MIN_TICK_BOX..=MAX_TICK_BOX).contains(&side) {
                        circles.push(rect);
                    }
                }
            }
        }
        let mut horizontal = merge_horizontal(horizontal);
        let vertical = merge_vertical(vertical);
        boxes.extend(grid_cells(&mut horizontal, &vertical));
        let boxes = drop_containers(dedupe_boxes(boxes));
        let circles = dedupe_boxes(circles);

        let letters: Vec<Letter> = marks
            .glyphs
            .iter()
            .filter(|glyph| !glyph.character.is_whitespace())
            .map(|glyph| Letter {
                character: glyph.character,
                rect: frame.to_visible_rect(&glyph.bounds),
            })
            .collect();
        let (phrases, underscores) = phrases_and_underscores(&letters);
        Self {
            horizontal,
            boxes,
            circles,
            letters,
            phrases,
            underscores,
        }
    }

    /// whether any letter (other than underscores) sits inside a rectangle.
    fn has_text_inside(&self, rect: &VisibleRect) -> bool {
        self.letters
            .iter()
            .any(|letter| letter.character != '_' && contains_center(rect, &letter.rect))
    }

    /// rows of four or more equal, touching small boxes become comb fields.
    fn comb_fields(&self, used: &mut [bool]) -> Vec<DetectedField> {
        let mut order: Vec<usize> = (0..self.boxes.len()).collect();
        order.sort_by(|a, b| {
            let (a, b) = (&self.boxes[*a], &self.boxes[*b]);
            (a.y, a.x)
                .partial_cmp(&(b.y, b.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let small = |rect: &VisibleRect| rect.width <= 36.0 && rect.height <= 40.0;
        let mut fields = Vec::new();
        let mut start = 0;
        while start < order.len() {
            let first = self.boxes[order[start]];
            let mut run = vec![order[start]];
            for &candidate in &order[start + 1..] {
                let last = self.boxes[*run.last().unwrap()];
                let next = self.boxes[candidate];
                let same_row = (next.y - first.y).abs() <= TOUCH
                    && (next.height - first.height).abs() <= TOUCH;
                let touching = (next.x - last.right()).abs() <= TOUCH;
                let same_width = (next.width - first.width).abs() <= TOUCH;
                if same_row && touching && same_width {
                    run.push(candidate);
                } else if !same_row {
                    break;
                }
            }
            let all_empty = run
                .iter()
                .all(|&index| !self.has_text_inside(&self.boxes[index]));
            if run.len() >= 4 && small(&first) && all_empty {
                let last = self.boxes[*run.last().unwrap()];
                let rect = VisibleRect::new(first.x, first.y, last.right() - first.x, first.height);
                for &index in &run {
                    used[index] = true;
                }
                let label = self.label_left(&rect).or_else(|| self.label_above(&rect));
                fields.push(proposal(
                    DetectedKind::Comb,
                    rect,
                    label,
                    0.8,
                    Some(run.len() as u32),
                ));
                start += run.len();
            } else {
                start += 1;
            }
        }
        fields
    }

    /// small empty squares become checkboxes.
    fn checkboxes(&self, used: &mut [bool]) -> Vec<DetectedField> {
        let mut fields = Vec::new();
        for (index, rect) in self.boxes.iter().enumerate() {
            if used[index] || !is_tick_sized(rect) || self.has_text_inside(rect) {
                continue;
            }
            used[index] = true;
            let label = self.label_right(rect).or_else(|| self.label_left(rect));
            let confidence = if label.is_some() { 0.9 } else { 0.6 };
            fields.push(proposal(
                DetectedKind::Checkbox,
                *rect,
                label,
                confidence,
                None,
            ));
        }
        fields
    }

    /// bigger boxes and table cells: the empty part becomes a text field.
    fn box_fields(&self, used: &[bool]) -> Vec<DetectedField> {
        let mut fields = Vec::new();
        for (index, rect) in self.boxes.iter().enumerate() {
            if used[index] || rect.width < 40.0 || rect.height < 10.0 || rect.height > 200.0 {
                continue;
            }
            let inside: Vec<&Phrase> = self
                .phrases
                .iter()
                .filter(|phrase| contains_center(rect, &phrase.rect))
                .collect();
            if !inside.is_empty() && self.labels_a_neighbour(index, used) {
                continue;
            }
            let (area, label, confidence) = if inside.is_empty() {
                let label = self.label_left(rect).or_else(|| self.label_above(rect));
                let confidence = if label.is_some() { 0.8 } else { 0.55 };
                (inset(rect, 1.0), label, confidence)
            } else {
                match empty_part_of_cell(rect, &inside) {
                    Some((area, label)) => (area, Some(label), 0.75),
                    None => continue,
                }
            };
            let kind = if area.height >= 36.0 {
                DetectedKind::MultilineText
            } else {
                DetectedKind::Text
            };
            fields.push(proposal(kind, area, label, confidence, None));
        }
        fields
    }

    /// a table cell holding text, with an empty cell right of it or under
    /// it, is the label of that cell rather than a field itself.
    fn labels_a_neighbour(&self, index: usize, used: &[bool]) -> bool {
        let cell = &self.boxes[index];
        self.boxes.iter().enumerate().any(|(other_index, other)| {
            if other_index == index || used[other_index] || self.has_text_inside(other) {
                return false;
            }
            let right_of = (other.x - cell.right()).abs() <= TOUCH
                && vertical_overlap(other, cell) > 0.5 * cell.height;
            let below = (other.y - cell.bottom()).abs() <= TOUCH
                && (other.right().min(cell.right()) - other.x.max(cell.x)) > 0.5 * cell.width;
            right_of || below
        })
    }

    /// small empty circles become radio buttons; ones in a row or a
    /// column share a group.
    fn radio_buttons(&self) -> Vec<DetectedField> {
        let circles: Vec<VisibleRect> = self
            .circles
            .iter()
            .copied()
            .filter(|rect| !self.has_text_inside(rect))
            .collect();
        let groups = group_aligned(&circles);
        circles
            .iter()
            .zip(groups)
            .map(|(rect, group)| {
                let label = self.label_right(rect).or_else(|| self.label_left(rect));
                let mut field = proposal(DetectedKind::Radio, *rect, label, 0.85, None);
                field.group = Some(group);
                field
            })
            .collect()
    }

    /// "Name: ________" written with underscore letters.
    fn underscore_fields(&self) -> Vec<DetectedField> {
        self.underscores
            .iter()
            .map(|run| {
                let height = run.rect.height.max(14.0);
                let rect = VisibleRect::new(
                    run.rect.x,
                    run.rect.bottom() - height,
                    run.rect.width,
                    height,
                );
                let label = self.label_left(&rect).or_else(|| self.label_above(&rect));
                let confidence = if label.is_some() { 0.85 } else { 0.6 };
                proposal(DetectedKind::Text, rect, label, confidence, None)
            })
            .collect()
    }

    /// long lines that are not part of a box and not underlining text.
    fn line_fields(&self) -> Vec<DetectedField> {
        let mut fields = Vec::new();
        for line in self.horizontal.iter().filter(|line| !line.box_edge) {
            let Some(left) = self.free_start_of_line(line) else {
                continue;
            };
            if line.right - left < MIN_LINE_LENGTH {
                continue;
            }
            let height = self.room_above(line, left);
            let rect = VisibleRect::new(left, line.y - height, line.right - left, height);
            let label = self
                .label_on_line(line, left)
                .or_else(|| self.label_left(&rect))
                .or_else(|| self.label_above(&rect));
            let confidence = if label.is_some() { 0.8 } else { 0.5 };
            fields.push(proposal(DetectedKind::Text, rect, label, confidence, None));
        }
        fields
    }

    /// letters sitting right on a line: most of it covered means the line
    /// underlines text (no field); a label at its start means the field
    /// starts after the label.
    fn letters_on_line(&self, line: &HorizontalLine) -> Vec<&Letter> {
        self.letters
            .iter()
            .filter(|letter| {
                let bottom = letter.rect.bottom();
                bottom <= line.y + 1.5
                    && bottom >= line.y - 6.0
                    && letter.rect.right() > line.left
                    && letter.rect.x < line.right
                    && letter.character != '_'
            })
            .collect()
    }

    fn free_start_of_line(&self, line: &HorizontalLine) -> Option<f32> {
        let on_line = self.letters_on_line(line);
        if on_line.is_empty() {
            return Some(line.left);
        }
        let covered: f32 = on_line
            .iter()
            .map(|letter| letter.rect.right().min(line.right) - letter.rect.x.max(line.left))
            .sum();
        if covered >= 0.5 * (line.right - line.left) {
            return None;
        }
        let text_end = on_line
            .iter()
            .map(|letter| letter.rect.right())
            .fold(line.left, f32::max);
        let text_starts_at_left = on_line
            .iter()
            .all(|letter| letter.rect.x < line.left + 0.5 * (line.right - line.left));
        text_starts_at_left.then_some(text_end + 3.0)
    }

    fn label_on_line(&self, line: &HorizontalLine, free_start: f32) -> Option<String> {
        if free_start <= line.left {
            return None;
        }
        let rect = VisibleRect::new(line.left, line.y - 20.0, free_start - line.left, 20.0);
        self.phrases
            .iter()
            .find(|phrase| contains_center(&rect, &phrase.rect))
            .map(|phrase| clean_label(&phrase.text))
            .filter(|label| !label.is_empty())
    }

    /// how tall a field on a line can be before it runs into what is above.
    fn room_above(&self, line: &HorizontalLine, left: f32) -> f32 {
        let overlaps = |x0: f32, x1: f32| x1 > left + 1.0 && x0 < line.right - 1.0;
        let letters = self
            .letters
            .iter()
            .filter(|letter| {
                overlaps(letter.rect.x, letter.rect.right()) && letter.rect.bottom() < line.y - 1.0
            })
            .map(|letter| letter.rect.bottom());
        let lines = self
            .horizontal
            .iter()
            .filter(|other| overlaps(other.left, other.right) && other.y < line.y - 1.0)
            .map(|other| other.y);
        let nearest = letters.chain(lines).fold(f32::MIN, f32::max);
        let gap = if nearest == f32::MIN {
            18.0
        } else {
            line.y - nearest - 1.0
        };
        gap.clamp(10.0, 18.0)
    }

    /// the phrase just left of a rectangle on the same line.
    fn label_left(&self, rect: &VisibleRect) -> Option<String> {
        self.phrases
            .iter()
            .filter(|phrase| {
                vertical_overlap(&phrase.rect, rect) > 0.3 * phrase.rect.height
                    && phrase.rect.right() <= rect.x + 2.0
                    && rect.x - phrase.rect.right() < 200.0
            })
            .max_by(|a, b| a.rect.right().total_cmp(&b.rect.right()))
            .map(|phrase| clean_label(&phrase.text))
            .filter(|label| !label.is_empty())
    }

    /// the phrase just right of a rectangle on the same line (checkbox labels).
    fn label_right(&self, rect: &VisibleRect) -> Option<String> {
        self.phrases
            .iter()
            .filter(|phrase| {
                vertical_overlap(&phrase.rect, rect) > 0.3 * phrase.rect.height
                    && phrase.rect.x >= rect.right() - 2.0
                    && phrase.rect.x - rect.right() < 150.0
            })
            .min_by(|a, b| a.rect.x.total_cmp(&b.rect.x))
            .map(|phrase| clean_label(&phrase.text))
            .filter(|label| !label.is_empty())
    }

    /// the phrase just above a rectangle, over its left part.
    fn label_above(&self, rect: &VisibleRect) -> Option<String> {
        self.phrases
            .iter()
            .filter(|phrase| {
                phrase.rect.bottom() <= rect.y + 2.0
                    && rect.y - phrase.rect.bottom() < 24.0
                    && phrase.rect.x < rect.right()
                    && phrase.rect.right() > rect.x - 10.0
            })
            .max_by(|a, b| a.rect.bottom().total_cmp(&b.rect.bottom()))
            .map(|phrase| clean_label(&phrase.text))
            .filter(|label| !label.is_empty())
    }
}

fn add_box_edges(
    rect: &VisibleRect,
    horizontal: &mut Vec<HorizontalLine>,
    vertical: &mut Vec<VerticalLine>,
) {
    for y in [rect.y, rect.bottom()] {
        horizontal.push(HorizontalLine {
            y,
            left: rect.x,
            right: rect.right(),
            box_edge: true,
        });
    }
    for x in [rect.x, rect.right()] {
        vertical.push(VerticalLine {
            x,
            top: rect.y,
            bottom: rect.bottom(),
        });
    }
}

/// joins pieces of one line drawn as several strokes. a line counts as a
/// box edge when any of its pieces was one.
fn merge_horizontal(mut lines: Vec<HorizontalLine>) -> Vec<HorizontalLine> {
    lines.sort_by(|a, b| {
        (a.y, a.left)
            .partial_cmp(&(b.y, b.left))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut merged: Vec<HorizontalLine> = Vec::new();
    for line in lines {
        if let Some(last) = merged.last_mut() {
            if (last.y - line.y).abs() <= 0.75 && line.left <= last.right + 1.0 {
                last.right = last.right.max(line.right);
                last.box_edge |= line.box_edge;
                continue;
            }
        }
        merged.push(line);
    }
    merged
}

fn merge_vertical(mut lines: Vec<VerticalLine>) -> Vec<VerticalLine> {
    lines.sort_by(|a, b| {
        (a.x, a.top)
            .partial_cmp(&(b.x, b.top))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut merged: Vec<VerticalLine> = Vec::new();
    for line in lines {
        if let Some(last) = merged.last_mut() {
            if (last.x - line.x).abs() <= 0.75 && line.top <= last.bottom + 1.0 {
                last.bottom = last.bottom.max(line.bottom);
                continue;
            }
        }
        merged.push(line);
    }
    merged
}

/// cells of tables drawn with separate lines: between two horizontal
/// lines with nothing between them, each pair of neighbouring vertical
/// lines that run from one to the other makes a cell. lines that border
/// a cell are marked as box edges, so they aren't offered as lines to
/// write on as well.
fn grid_cells(horizontal: &mut [HorizontalLine], vertical: &[VerticalLine]) -> Vec<VisibleRect> {
    let mut cells = Vec::new();
    let mut edges = Vec::new();
    for upper_index in 0..horizontal.len() {
        let upper = horizontal[upper_index];
        for (lower_index, lower) in horizontal.iter().enumerate().skip(upper_index + 1) {
            let gap = lower.y - upper.y;
            if !(6.0..=200.0).contains(&gap) {
                continue;
            }
            let left = upper.left.max(lower.left);
            let right = upper.right.min(lower.right);
            if right - left < 5.0 {
                continue;
            }
            let between = horizontal.iter().any(|other| {
                other.y > upper.y + TOUCH
                    && other.y < lower.y - TOUCH
                    && other.left < right - TOUCH
                    && other.right > left + TOUCH
            });
            if between {
                continue;
            }
            let mut walls: Vec<f32> = vertical
                .iter()
                .filter(|wall| {
                    wall.x >= left - TOUCH
                        && wall.x <= right + TOUCH
                        && wall.top <= upper.y + TOUCH
                        && wall.bottom >= lower.y - TOUCH
                })
                .map(|wall| wall.x)
                .collect();
            walls.sort_by(f32::total_cmp);
            walls.dedup_by(|a, b| (*a - *b).abs() <= TOUCH);
            for pair in walls.windows(2) {
                if pair[1] - pair[0] >= 5.0 {
                    cells.push(VisibleRect::new(pair[0], upper.y, pair[1] - pair[0], gap));
                    edges.push(upper_index);
                    edges.push(lower_index);
                }
            }
        }
    }
    for index in edges {
        horizontal[index].box_edge = true;
    }
    cells
}

fn same_box(a: &VisibleRect, b: &VisibleRect) -> bool {
    (a.x - b.x).abs() <= TOUCH
        && (a.y - b.y).abs() <= TOUCH
        && (a.right() - b.right()).abs() <= TOUCH
        && (a.bottom() - b.bottom()).abs() <= TOUCH
}

fn dedupe_boxes(boxes: Vec<VisibleRect>) -> Vec<VisibleRect> {
    let mut kept: Vec<VisibleRect> = Vec::new();
    for rect in boxes {
        if !kept.iter().any(|other| same_box(other, &rect)) {
            kept.push(rect);
        }
    }
    kept
}

/// a box around two or more other boxes is a frame or a table border,
/// not a place to write.
fn drop_containers(boxes: Vec<VisibleRect>) -> Vec<VisibleRect> {
    boxes
        .iter()
        .filter(|outer| {
            let inner = boxes
                .iter()
                .filter(|other| !same_box(other, outer) && outer.contains(other, TOUCH))
                .count();
            inner < 2
        })
        .copied()
        .collect()
}

fn is_tick_sized(rect: &VisibleRect) -> bool {
    let side = rect.width.max(rect.height);
    (MIN_TICK_BOX..=MAX_TICK_BOX).contains(&side)
        && (rect.width - rect.height).abs() <= 2.0_f32.max(0.15 * side)
}

fn contains_center(outer: &VisibleRect, inner: &VisibleRect) -> bool {
    let (cx, cy) = (inner.x + inner.width / 2.0, inner.y + inner.height / 2.0);
    cx > outer.x && cx < outer.right() && cy > outer.y && cy < outer.bottom()
}

fn vertical_overlap(a: &VisibleRect, b: &VisibleRect) -> f32 {
    (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0)
}

fn inset(rect: &VisibleRect, by: f32) -> VisibleRect {
    VisibleRect::new(
        rect.x + by,
        rect.y + by,
        (rect.width - 2.0 * by).max(1.0),
        (rect.height - 2.0 * by).max(1.0),
    )
}

/// a cell with a label in it: the empty part to the right of, or below,
/// the label becomes the field.
fn empty_part_of_cell(cell: &VisibleRect, inside: &[&Phrase]) -> Option<(VisibleRect, String)> {
    let text_right = inside
        .iter()
        .map(|phrase| phrase.rect.right())
        .fold(cell.x, f32::max);
    let text_bottom = inside
        .iter()
        .map(|phrase| phrase.rect.bottom())
        .fold(cell.y, f32::max);
    let label = clean_label(
        &inside
            .iter()
            .map(|phrase| phrase.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    );
    let room_right = cell.right() - text_right - 4.0;
    let room_below = cell.bottom() - text_bottom - 2.0;
    if room_right >= 40.0 && text_right < cell.x + 0.6 * cell.width {
        let rect = VisibleRect::new(
            text_right + 3.0,
            cell.y + 1.0,
            room_right,
            cell.height - 2.0,
        );
        return Some((rect, label));
    }
    if room_below >= 12.0 && text_bottom < cell.y + 0.5 * cell.height {
        let rect = VisibleRect::new(
            cell.x + 1.0,
            text_bottom + 1.0,
            cell.width - 2.0,
            room_below,
        );
        return Some((rect, label));
    }
    None
}

/// groups letters into lines and phrases, and pulls out underscore runs.
fn phrases_and_underscores(letters: &[Letter]) -> (Vec<Phrase>, Vec<UnderscoreRun>) {
    let mut sorted: Vec<&Letter> = letters.iter().collect();
    sorted.sort_by(|a, b| center_y(&a.rect).total_cmp(&center_y(&b.rect)));
    let mut lines: Vec<Vec<&Letter>> = Vec::new();
    for letter in sorted {
        let joins = lines.last().is_some_and(|line| {
            let reference = line[0];
            (center_y(&letter.rect) - center_y(&reference.rect)).abs()
                < 0.5 * reference.rect.height.min(letter.rect.height)
        });
        if joins {
            lines.last_mut().unwrap().push(letter);
        } else {
            lines.push(vec![letter]);
        }
    }
    let mut phrases = Vec::new();
    let mut underscores = Vec::new();
    for mut line in lines {
        line.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
        let (words, runs) = split_underscores(&line);
        underscores.extend(runs);
        phrases.extend(split_phrases(&words));
    }
    (phrases, underscores)
}

fn center_y(rect: &VisibleRect) -> f32 {
    rect.y + rect.height / 2.0
}

/// takes runs of three or more touching underscores out of a line.
fn split_underscores<'a>(line: &[&'a Letter]) -> (Vec<&'a Letter>, Vec<UnderscoreRun>) {
    let mut words = Vec::new();
    let mut runs = Vec::new();
    let mut index = 0;
    while index < line.len() {
        if line[index].character == '_' {
            let mut end = index + 1;
            while end < line.len()
                && line[end].character == '_'
                && line[end].rect.x - line[end - 1].rect.right() <= TOUCH
            {
                end += 1;
            }
            if end - index >= 3 {
                let first = &line[index].rect;
                let last = &line[end - 1].rect;
                runs.push(UnderscoreRun {
                    rect: VisibleRect::new(first.x, first.y, last.right() - first.x, first.height),
                });
            } else {
                words.extend_from_slice(&line[index..end]);
            }
            index = end;
        } else {
            words.push(line[index]);
            index += 1;
        }
    }
    (words, runs)
}

/// splits a line's letters where the gap is much wider than a space.
fn split_phrases(letters: &[&Letter]) -> Vec<Phrase> {
    let mut phrases: Vec<Phrase> = Vec::new();
    let mut previous: Option<&Letter> = None;
    for letter in letters {
        let height = letter.rect.height.max(1.0);
        let gap = previous.map(|before| letter.rect.x - before.rect.right());
        match (phrases.last_mut(), gap) {
            (Some(phrase), Some(gap)) if gap < 0.8 * height => {
                if gap > 0.15 * height {
                    phrase.text.push(' ');
                }
                phrase.text.push(letter.character);
                let right = phrase.rect.right().max(letter.rect.right());
                let top = phrase.rect.y.min(letter.rect.y);
                let bottom = phrase.rect.bottom().max(letter.rect.bottom());
                phrase.rect =
                    VisibleRect::new(phrase.rect.x, top, right - phrase.rect.x, bottom - top);
            }
            _ => phrases.push(Phrase {
                text: letter.character.to_string(),
                rect: letter.rect,
            }),
        }
        previous = Some(letter);
    }
    phrases
}

/// trims a label: no trailing colon, underscores or spaces.
fn clean_label(text: &str) -> String {
    text.trim()
        .trim_end_matches(|c: char| c == ':' || c == '_' || c.is_whitespace())
        .trim()
        .to_owned()
}

fn proposal(
    kind: DetectedKind,
    rect: VisibleRect,
    label: Option<String>,
    confidence: f32,
    max_length: Option<u32>,
) -> DetectedField {
    DetectedField {
        kind,
        rect,
        label,
        suggested_name: String::new(),
        max_length,
        group: None,
        confidence,
    }
}

/// numbers circles that sit in one row or one column the same.
fn group_aligned(circles: &[VisibleRect]) -> Vec<u32> {
    let mut groups: Vec<u32> = (0..circles.len() as u32).collect();
    for a in 0..circles.len() {
        for b in a + 1..circles.len() {
            let (first, second) = (&circles[a], &circles[b]);
            let same_row = (center_y(first) - center_y(second)).abs() <= 3.0
                && (first.x - second.x).abs() < 250.0;
            let same_column =
                (first.x - second.x).abs() <= 3.0 && (first.y - second.y).abs() < 60.0;
            if same_row || same_column {
                let (keep, replace) = (groups[a].min(groups[b]), groups[a].max(groups[b]));
                for group in groups.iter_mut() {
                    if *group == replace {
                        *group = keep;
                    }
                }
            }
        }
    }
    // renumber 0, 1, 2... in order of first appearance
    let mut seen: Vec<u32> = Vec::new();
    groups
        .iter()
        .map(|group| match seen.iter().position(|other| other == group) {
            Some(position) => position as u32,
            None => {
                seen.push(*group);
                seen.len() as u32 - 1
            }
        })
        .collect()
}

/// how much of the smaller rectangle the two share, 0 to 1.
fn overlap_share(a: &VisibleRect, b: &VisibleRect) -> f32 {
    let width = (a.right().min(b.right()) - a.x.max(b.x)).max(0.0);
    let height = (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0);
    let smaller = (a.width * a.height).min(b.width * b.height).max(0.01);
    width * height / smaller
}

/// when two proposals cover the same place (an underscore run drawn over
/// a line, say) the surer one stays.
fn drop_duplicates(mut found: Vec<DetectedField>) -> Vec<DetectedField> {
    found.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    let mut kept: Vec<DetectedField> = Vec::new();
    for field in found {
        if !kept
            .iter()
            .any(|other| overlap_share(&other.rect, &field.rect) > 0.5)
        {
            kept.push(field);
        }
    }
    kept
}

fn reading_order(a: &VisibleRect, b: &VisibleRect) -> std::cmp::Ordering {
    let row = a.height.min(b.height) / 2.0;
    if (a.y - b.y).abs() <= row {
        a.x.total_cmp(&b.x)
    } else {
        a.y.total_cmp(&b.y)
    }
}

/// field names from labels ("Date of birth"), or from the kind, made unique.
fn name_fields(found: &mut [DetectedField]) {
    let mut used: Vec<String> = Vec::new();
    for field in found.iter_mut() {
        let base = match (&field.label, field.kind) {
            (Some(label), _) => label
                .replace('.', " ")
                .chars()
                .take(40)
                .collect::<String>()
                .trim()
                .to_owned(),
            (None, DetectedKind::Checkbox) => "Checkbox".to_owned(),
            (None, DetectedKind::Radio) => "Choice".to_owned(),
            (None, _) => "Text".to_owned(),
        };
        let base = if base.is_empty() {
            "Field".to_owned()
        } else {
            base
        };
        let mut candidate = base.clone();
        let mut counter = 2;
        while used.contains(&candidate) {
            candidate = format!("{base} {counter}");
            counter += 1;
        }
        used.push(candidate.clone());
        field.suggested_name = candidate;
    }
}

/// the rectangles of widgets the page already has, so they aren't proposed again.
fn existing_widgets(
    doc: &Document,
    page_id: lopdf::ObjectId,
    frame: &PageFrame,
) -> Vec<VisibleRect> {
    let Ok(page) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };
    references_in(doc, page, b"Annots")
        .into_iter()
        .filter_map(|id| {
            let dict = doc.get_dictionary(id).ok()?;
            Some(frame.to_visible_rect(&get_rect(doc, dict, b"Rect")?))
        })
        .collect()
}
