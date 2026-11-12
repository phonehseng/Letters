//! giving annotations the appearance streams pdf/a-2 and -3 ask for
//! (19005-2 6.3.3): form fields are drawn by letters-forms; boxes, ovals,
//! lines, ink, highlights and notes are drawn here from their own
//! geometry and colour, the way readers draw them by default.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use super::cleanup::Removals;
use crate::objects::{get, get_array, get_name, get_number, number_of, page_ids, rect_of, resolve};
use crate::pdfa::can_draw_appearance;

/// the default line width readers use when /BS gives none.
const DEFAULT_LINE_WIDTH: f64 = 1.0;
/// the size of the square drawn for a sticky note, about the size of the
/// note icon readers draw.
const NOTE_ICON_SIZE: f64 = 18.0;

pub(super) fn add_missing_appearances(
    doc: &mut Document,
    part_one: bool,
    removals: &mut Removals,
) -> Vec<String> {
    let mut not_drawn = Vec::new();
    if letters_forms::redraw_fields(doc, true).is_ok() {
        removals.note("Form fields without a drawing were drawn by Letters.");
    }
    for page_id in page_ids(doc) {
        for id in annotation_ids(doc, page_id) {
            let Ok(dict) = doc.get_dictionary(id).cloned() else {
                continue;
            };
            let subtype = get_name(doc, &dict, b"Subtype").unwrap_or_default();
            if dict.has(b"AP") || subtype == b"Popup" || subtype == b"Link" || subtype == b"Widget"
            {
                continue;
            }
            if !can_draw_appearance(&subtype) {
                not_drawn.push(String::from_utf8_lossy(&subtype).into_owned());
                continue;
            }
            if let Some(content) = drawing_for(doc, &dict, &subtype, part_one) {
                store(doc, id, &dict, content);
                removals.note("Annotations without a drawing were given one, as readers draw them by default.");
            }
        }
    }
    not_drawn
}

fn annotation_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    doc.get_dictionary(page_id)
        .ok()
        .and_then(|page| get_array(doc, page, b"Annots"))
        .into_iter()
        .flatten()
        .filter_map(|annotation| annotation.as_reference().ok())
        .collect()
}

/// the stroke and fill colour operators from /C (and /IC for interiors).
fn colour(doc: &Document, dict: &Dictionary, key: &[u8], stroke: bool) -> Option<String> {
    let values: Vec<f64> = get_array(doc, dict, key)?
        .iter()
        .filter_map(|value| number_of(resolve(doc, value)))
        .collect();
    let operator = match (values.len(), stroke) {
        (1, true) => "G",
        (1, false) => "g",
        (3, true) => "RG",
        (3, false) => "rg",
        (4, true) => "K",
        (4, false) => "k",
        _ => return None,
    };
    let numbers: Vec<String> = values.iter().map(|value| format!("{value:.3}")).collect();
    Some(format!("{} {operator}", numbers.join(" ")))
}

fn line_width(doc: &Document, dict: &Dictionary) -> f64 {
    get(doc, dict, b"BS")
        .and_then(|bs| bs.as_dict().ok())
        .and_then(|bs| get_number(doc, bs, b"W"))
        .unwrap_or(DEFAULT_LINE_WIDTH)
}

fn points(doc: &Document, dict: &Dictionary, key: &[u8]) -> Vec<f64> {
    get_array(doc, dict, key)
        .into_iter()
        .flatten()
        .filter_map(|value| number_of(resolve(doc, value)))
        .collect()
}

/// the drawing in page coordinates (the stream's bbox is the /Rect, with
/// the identity matrix, so page coordinates can be used directly).
fn drawing_for(
    doc: &Document,
    dict: &Dictionary,
    subtype: &[u8],
    part_one: bool,
) -> Option<String> {
    let [left, bottom, right, top] = get(doc, dict, b"Rect").and_then(|rect| rect_of(doc, rect))?;
    let stroke = colour(doc, dict, b"C", true).unwrap_or_else(|| "0 G".to_owned());
    let fill = colour(doc, dict, b"C", false).unwrap_or_else(|| "1 1 0 rg".to_owned());
    let width = line_width(doc, dict);
    let half = width / 2.0;
    let interior = colour(doc, dict, b"IC", false);
    let content = match subtype {
        b"Square" => {
            let paint = if interior.is_some() { "B" } else { "S" };
            format!(
                "{} {stroke} {width} w {} {} {} {} re {paint}",
                interior.unwrap_or_default(),
                left + half,
                bottom + half,
                right - left - width,
                top - bottom - width
            )
        }
        b"Circle" => format!(
            "{stroke} {width} w {} S",
            ellipse(left + half, bottom + half, right - half, top - half)
        ),
        b"Line" => {
            let line = points(doc, dict, b"L");
            let [x1, y1, x2, y2] = line[..] else {
                return None;
            };
            format!("{stroke} {width} w {x1} {y1} m {x2} {y2} l S")
        }
        b"Polygon" | b"PolyLine" => {
            let vertices = points(doc, dict, b"Vertices");
            let close = if subtype == b"Polygon" { "s" } else { "S" };
            format!("{stroke} {width} w {} {close}", path(&vertices))
        }
        b"Ink" => {
            let mut out = format!("{stroke} {width} w 1 J 1 j ");
            for stroke_points in get_array(doc, dict, b"InkList").into_iter().flatten() {
                let values: Vec<f64> = resolve(doc, stroke_points)
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| number_of(resolve(doc, item)))
                            .collect()
                    })
                    .unwrap_or_default();
                out.push_str(&format!("{} S ", path(&values)));
            }
            out
        }
        b"Highlight" | b"Underline" | b"StrikeOut" | b"Squiggly" => {
            markup(doc, dict, subtype, &fill, &stroke, part_one)
        }
        b"Text" => format!(
            "{fill} {stroke} 1 w {left} {} {NOTE_ICON_SIZE} {NOTE_ICON_SIZE} re B",
            top - NOTE_ICON_SIZE
        ),
        b"FreeText" => format!(
            "{stroke} {width} w {} {} {} {} re S",
            left + half,
            bottom + half,
            right - left - width,
            top - bottom - width
        ),
        _ => return None,
    };
    Some(content)
}

/// text markup is drawn over each quadrilateral of /QuadPoints.
fn markup(
    doc: &Document,
    dict: &Dictionary,
    subtype: &[u8],
    fill: &str,
    stroke: &str,
    part_one: bool,
) -> String {
    let quads = points(doc, dict, b"QuadPoints");
    let mut out = String::new();
    for quad in quads.chunks(8) {
        let [x1, y1, x2, y2, x3, y3, x4, y4] = quad[..] else {
            continue;
        };
        let (low, high) = (y3.min(y4), y1.max(y2));
        let (start, end) = (x1.min(x3), x2.max(x4));
        let height = high - low;
        match subtype {
            // a filled box multiplied over the text would need a blend mode,
            // which part 1 forbids; there the box is drawn as an outline
            b"Highlight" if part_one => out.push_str(&format!(
                "{stroke} 1 w {start} {low} {} {height} re S ",
                end - start
            )),
            b"Highlight" => out.push_str(&format!(
                "/Multiply BM {fill} {start} {low} {} {height} re f ",
                end - start
            )),
            b"Underline" => out.push_str(&format!(
                "{stroke} 1 w {start} {} m {end} {} l S ",
                low + 1.0,
                low + 1.0
            )),
            b"StrikeOut" => {
                let middle = low + height / 2.0;
                out.push_str(&format!(
                    "{stroke} 1 w {start} {middle} m {end} {middle} l S "
                ));
            }
            _ => out.push_str(&format!("{stroke} 1 w {start} {low} m {end} {low} l S ")),
        }
    }
    out
}

fn path(values: &[f64]) -> String {
    let mut out = String::new();
    for (index, point) in values.chunks(2).enumerate() {
        let [x, y] = point[..] else { continue };
        let operator = if index == 0 { "m" } else { "l" };
        out.push_str(&format!("{x} {y} {operator} "));
    }
    out
}

/// an ellipse drawn with four curves (the usual 0.5523 control distance).
fn ellipse(left: f64, bottom: f64, right: f64, top: f64) -> String {
    let (cx, cy) = ((left + right) / 2.0, (bottom + top) / 2.0);
    let (rx, ry) = ((right - left) / 2.0, (top - bottom) / 2.0);
    let (kx, ky) = (0.5523 * rx, 0.5523 * ry);
    format!(
        "{} {cy} m {} {} {} {} {cx} {} c {} {} {} {} {} {cy} c {} {} {} {} {cx} {} c {} {} {} {} {} {cy} c",
        cx + rx,
        cx + rx, cy + ky, cx + kx, cy + ry, cy + ry,
        cx - kx, cy + ry, cx - rx, cy + ky, cx - rx,
        cx - rx, cy - ky, cx - kx, cy - ry, cy - ry,
        cx + kx, cy - ry, cx + rx, cy - ky, cx + rx
    )
}

fn store(doc: &mut Document, id: ObjectId, dict: &Dictionary, content: String) {
    let Some(rect) = get(doc, dict, b"Rect").cloned() else {
        return;
    };
    let uses_blend = content.contains(" BM ");
    let mut resources = Dictionary::new();
    let content = if uses_blend {
        resources.set(
            "ExtGState",
            dictionary! { "Multiply" => dictionary! { "BM" => "Multiply" } },
        );
        content.replace("/Multiply BM", "/Multiply gs")
    } else {
        content
    };
    let stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => rect,
            "Resources" => Object::Dictionary(resources),
        },
        content.into_bytes(),
    );
    let appearance = doc.add_object(stream);
    if let Ok(target) = doc.get_dictionary_mut(id) {
        target.set("AP", dictionary! { "N" => Object::Reference(appearance) });
    }
}
