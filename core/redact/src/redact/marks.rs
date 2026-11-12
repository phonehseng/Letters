//! the visible part of a redaction: a filled box over each area, with an
//! optional label such as "Redacted", drawn on top of everything else.
//!
//! the boxes are wrapped in a marked content sequence tagged
//! `/LettersRedaction`, so a later redaction of the same page can tell
//! them apart from the page's own content. that is what makes redacting
//! twice the same as redacting once: earlier boxes are kept as they are,
//! and a box is only added for an area that does not have one yet.

use lopdf::{dictionary, Object, StringFormat};

use crate::content::Op;
use crate::fonts::standard::standard_font;
use crate::fonts::standard_metrics_winansi_name;
use crate::geometry::{Point, Rect};
use crate::page::PageFrame;
use crate::redact::paths::{real, rect_op};

pub const MARK_TAG: &[u8] = b"LettersRedaction";

/// how the boxes look.
#[derive(Clone, Debug)]
pub struct MarkStyle {
    pub fill: [f64; 3],
    pub label: Option<String>,
}

/// takes the boxes of earlier redactions off the end of a page's content,
/// returning the rest, the boxes' operations and the rectangles they cover.
pub fn split_marks(mut ops: Vec<Op>) -> (Vec<Op>, Vec<Op>, Vec<Rect>) {
    let mut blocks: Vec<Vec<Op>> = Vec::new();
    while ops.last().map(Op::name) == Some("EMC") {
        let Some(start) = ops
            .iter()
            .rposition(|op| op.name() == "BMC" || op.name() == "BDC")
        else {
            break;
        };
        let is_mark = ops[start].operands().first() == Some(&Object::Name(MARK_TAG.to_vec()));
        if !is_mark {
            break;
        }
        blocks.push(ops.split_off(start));
    }
    blocks.reverse();
    let marks: Vec<Op> = blocks.into_iter().flatten().collect();
    let rects = marks
        .iter()
        .filter(|op| op.name() == "re")
        .filter_map(rect_of)
        .collect();
    (ops, marks, rects)
}

fn rect_of(op: &Op) -> Option<Rect> {
    let values: Vec<f64> = op
        .operands()
        .iter()
        .filter_map(|object| match object {
            Object::Integer(value) => Some(*value as f64),
            Object::Real(value) => Some(f64::from(*value)),
            _ => None,
        })
        .collect();
    match values[..] {
        [x, y, width, height] => Some(Rect::from_corners(x, y, x + width, y + height)),
        _ => None,
    }
}

/// a page redacted before is wrapped as `q ... Q` followed by the boxes.
/// taking the wrapper off again means a second redaction does not wrap
/// the page twice.
pub fn unwrap_body(mut body: Vec<Op>) -> Vec<Op> {
    if body.first().map(Op::name) != Some("q") {
        return body;
    }
    let mut depth = 0i64;
    for (index, op) in body.iter().enumerate() {
        match op.name() {
            "q" => depth += 1,
            "Q" => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            if index == body.len() - 1 {
                body.pop();
                body.remove(0);
            }
            return body;
        }
    }
    body
}

/// the areas that do not have a box yet.
pub fn areas_without_marks(areas: &[Rect], marked: &[Rect]) -> Vec<Rect> {
    let same = |a: &Rect, b: &Rect| {
        (a.x0 - b.x0).abs() < 0.01
            && (a.y0 - b.y0).abs() < 0.01
            && (a.x1 - b.x1).abs() < 0.01
            && (a.y1 - b.y1).abs() < 0.01
    };
    let mut fresh: Vec<Rect> = Vec::new();
    for area in areas {
        if !marked.iter().any(|old| same(old, area)) && !fresh.iter().any(|new| same(new, area)) {
            fresh.push(*area);
        }
    }
    fresh
}

/// the operations that draw the boxes (and labels) for some areas.
/// `font` is the resource name of a Helvetica font for the labels.
pub fn mark_ops(
    areas: &[Rect],
    style: &MarkStyle,
    frame: &PageFrame,
    font: Option<&[u8]>,
) -> Vec<Op> {
    if areas.is_empty() {
        return Vec::new();
    }
    let [r, g, b] = style.fill;
    let mut ops = vec![
        Op::new("BMC", vec![Object::Name(MARK_TAG.to_vec())]),
        Op::new("q", vec![]),
        Op::new("rg", vec![real(r), real(g), real(b)]),
    ];
    for area in areas {
        ops.push(rect_op(area));
    }
    ops.push(Op::new("f", vec![]));
    if let (Some(label), Some(font)) = (style.label.as_deref(), font) {
        for area in areas {
            ops.extend(label_ops(label, area, style, frame, font));
        }
    }
    ops.push(Op::new("Q", vec![]));
    ops.push(Op::new("EMC", vec![]));
    ops
}

/// the largest label size, in points.
const LABEL_MAX_SIZE: f64 = 10.0;
/// labels smaller than this are left out: they could not be read.
const LABEL_MIN_SIZE: f64 = 3.0;

/// the label centred in its box, upright the way the page is shown.
fn label_ops(
    label: &str,
    area: &Rect,
    style: &MarkStyle,
    frame: &PageFrame,
    font: &[u8],
) -> Vec<Op> {
    let bytes = win_ansi_bytes(label);
    let visible = frame.to_visible(area);
    let units = text_width_units(&bytes);
    if units <= 0.0 {
        return Vec::new();
    }
    let mut size = LABEL_MAX_SIZE.min(visible.height * 0.6);
    size = size.min(visible.width * 0.9 * 1000.0 / units);
    if size < LABEL_MIN_SIZE {
        return Vec::new();
    }
    let width = units * size / 1000.0;
    let to_user = frame.visible_to_user();
    // baseline a third of the size below the middle, so capitals look centred
    let origin = to_user.apply(Point::new(
        visible.x + (visible.width - width) / 2.0,
        visible.y + visible.height / 2.0 + size * 0.35,
    ));
    let across = to_user.apply_vector(1.0, 0.0);
    let up = to_user.apply_vector(0.0, -1.0);
    let ink = if luminance(style.fill) < 0.5 {
        1.0
    } else {
        0.0
    };
    vec![
        Op::new("BT", vec![]),
        Op::new("Tf", vec![Object::Name(font.to_vec()), real(size)]),
        Op::new("rg", vec![real(ink), real(ink), real(ink)]),
        Op::new(
            "Tm",
            vec![
                real(across.x),
                real(across.y),
                real(up.x),
                real(up.y),
                real(origin.x),
                real(origin.y),
            ],
        ),
        Op::new("Tj", vec![Object::String(bytes, StringFormat::Literal)]),
        Op::new("ET", vec![]),
    ]
}

fn luminance([r, g, b]: [f64; 3]) -> f64 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// the label in winansi encoding; letters it does not have become '?'.
fn win_ansi_bytes(label: &str) -> Vec<u8> {
    label
        .chars()
        .map(|letter| match letter {
            ' '..='~' => letter as u8,
            '\u{A0}'..='\u{FF}' => letter as u32 as u8,
            _ => b'?',
        })
        .collect()
}

/// the width of the label in Helvetica, in thousandths of the size.
fn text_width_units(bytes: &[u8]) -> f64 {
    let helvetica = standard_font(b"Helvetica").expect("helvetica is a standard font");
    bytes
        .iter()
        .map(|&byte| {
            standard_metrics_winansi_name(byte)
                .and_then(|name| helvetica.width(name))
                .unwrap_or(556.0)
        })
        .sum()
}

/// the font dictionary for labels: Helvetica, built into every reader.
pub fn label_font() -> lopdf::Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> PageFrame {
        PageFrame {
            crop: Rect::from_corners(0.0, 0.0, 612.0, 792.0),
            rotate: 0,
        }
    }

    #[test]
    fn marks_from_an_earlier_redaction_are_found_again() {
        let style = MarkStyle {
            fill: [0.0; 3],
            label: None,
        };
        let area = Rect::from_corners(10.0, 10.0, 50.0, 30.0);
        let mut ops = vec![Op::new("q", vec![]), Op::new("Q", vec![])];
        ops.extend(mark_ops(&[area], &style, &frame(), None));
        let (body, marks, rects) = split_marks(ops);
        assert_eq!(body.len(), 2);
        assert!(!marks.is_empty());
        assert_eq!(rects, vec![area]);
        assert!(areas_without_marks(&[area], &rects).is_empty());
    }

    #[test]
    fn the_wrapper_comes_off_only_when_it_wraps_everything() {
        let wrapped = vec![
            Op::new("q", vec![]),
            Op::new("m", vec![]),
            Op::new("Q", vec![]),
        ];
        assert_eq!(unwrap_body(wrapped).len(), 1);
        let separate = vec![
            Op::new("q", vec![]),
            Op::new("Q", vec![]),
            Op::new("q", vec![]),
            Op::new("Q", vec![]),
        ];
        assert_eq!(unwrap_body(separate).len(), 4);
    }

    #[test]
    fn labels_fit_their_box() {
        let style = MarkStyle {
            fill: [0.0; 3],
            label: Some("Redacted".into()),
        };
        let area = Rect::from_corners(100.0, 100.0, 200.0, 120.0);
        let ops = mark_ops(&[area], &style, &frame(), Some(b"LR1"));
        assert!(ops.iter().any(|op| op.name() == "Tj"));
        let tiny = Rect::from_corners(100.0, 100.0, 104.0, 102.0);
        let ops = mark_ops(&[tiny], &style, &frame(), Some(b"LR1"));
        assert!(!ops.iter().any(|op| op.name() == "Tj"));
    }
}
