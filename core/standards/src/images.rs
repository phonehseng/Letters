//! where each picture lands on its page and how sharp it is there:
//! following the graphics state's matrix through page content and the
//! form xobjects it paints, to the size every image is drawn at.

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::objects::{
    get_dict, get_name, inherited, names_in, number_of, page_ids, resolve, stream_bytes,
};

/// an affine matrix [a b c d e f] (iso 32000 8.3.3).
pub(crate) type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
const POINTS_PER_INCH: f64 = 72.0;
/// form xobjects may nest; deeper than this is a broken or hostile file.
const MAX_NESTING: usize = 16;

/// one picture as drawn on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedImage {
    pub page: u32,
    pub object: Option<String>,
    pub width: i64,
    pub height: i64,
    /// the smaller of the across and down resolutions as placed
    pub pixels_per_inch: f64,
    pub color_space: String,
    pub bits_per_component: i64,
    pub filter: String,
}

pub(crate) fn multiply(first: Matrix, then: Matrix) -> Matrix {
    let [a, b, c, d, e, f] = first;
    let [a2, b2, c2, d2, e2, f2] = then;
    [
        a * a2 + b * c2,
        a * b2 + b * d2,
        c * a2 + d * c2,
        c * b2 + d * d2,
        e * a2 + f * c2 + e2,
        e * b2 + f * d2 + f2,
    ]
}

/// every picture drawn on every page, with its resolution as placed.
pub fn placed_images(doc: &Document) -> Vec<PlacedImage> {
    let mut out = Vec::new();
    for (index, page_id) in page_ids(doc).into_iter().enumerate() {
        let resources = inherited(doc, page_id, b"Resources")
            .and_then(|value| value.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        let content = doc.get_page_content(page_id);
        walk(
            doc,
            index as u32,
            &content,
            &resources,
            IDENTITY,
            &mut out,
            0,
        );
    }
    out
}

fn walk(
    doc: &Document,
    page: u32,
    content: &[u8],
    resources: &Dictionary,
    start: Matrix,
    out: &mut Vec<PlacedImage>,
    depth: usize,
) {
    let operations = Content::decode(content)
        .map(|content| content.operations)
        .unwrap_or_default();
    let xobjects = get_dict(doc, resources, b"XObject")
        .cloned()
        .unwrap_or_default();
    let mut matrix = start;
    let mut saved: Vec<Matrix> = Vec::new();
    for operation in operations {
        match operation.operator.as_str() {
            "q" => saved.push(matrix),
            "Q" => matrix = saved.pop().unwrap_or(start),
            "cm" => {
                let numbers: Vec<f64> = operation.operands.iter().filter_map(number_of).collect();
                if let [a, b, c, d, e, f] = numbers[..] {
                    matrix = multiply([a, b, c, d, e, f], matrix);
                }
            }
            "Do" => {
                let Some(name) = operation.operands.first().and_then(|o| o.as_name().ok()) else {
                    continue;
                };
                let Ok(value) = xobjects.get(name) else {
                    continue;
                };
                let id = value.as_reference().ok();
                let Ok(stream) = resolve(doc, value).as_stream() else {
                    continue;
                };
                match get_name(doc, &stream.dict, b"Subtype").as_deref() {
                    Some(b"Image") => out.push(placed(doc, page, id, &stream.dict, matrix)),
                    Some(b"Form") if depth < MAX_NESTING => {
                        let own = get_dict(doc, &stream.dict, b"Resources")
                            .cloned()
                            .unwrap_or_else(|| resources.clone());
                        let form_matrix = form_matrix(doc, &stream.dict);
                        let bytes = stream_bytes(stream);
                        walk(
                            doc,
                            page,
                            &bytes,
                            &own,
                            multiply(form_matrix, matrix),
                            out,
                            depth + 1,
                        );
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn form_matrix(doc: &Document, dict: &Dictionary) -> Matrix {
    let numbers: Vec<f64> = crate::objects::get_array(doc, dict, b"Matrix")
        .into_iter()
        .flatten()
        .filter_map(|value| number_of(resolve(doc, value)))
        .collect();
    match numbers[..] {
        [a, b, c, d, e, f] => [a, b, c, d, e, f],
        _ => IDENTITY,
    }
}

fn placed(
    doc: &Document,
    page: u32,
    id: Option<ObjectId>,
    dict: &Dictionary,
    matrix: Matrix,
) -> PlacedImage {
    let number = |key: &[u8]| crate::objects::get_number(doc, dict, key).unwrap_or(0.0) as i64;
    let (width, height) = (number(b"Width"), number(b"Height"));
    // the image fills the unit square, so the matrix's columns are its
    // drawn width and height in points
    let drawn_width = (matrix[0].powi(2) + matrix[1].powi(2)).sqrt();
    let drawn_height = (matrix[2].powi(2) + matrix[3].powi(2)).sqrt();
    let across = if drawn_width > 0.0 {
        width as f64 / (drawn_width / POINTS_PER_INCH)
    } else {
        0.0
    };
    let down = if drawn_height > 0.0 {
        height as f64 / (drawn_height / POINTS_PER_INCH)
    } else {
        0.0
    };
    let color_space = match dict
        .get(b"ColorSpace")
        .ok()
        .map(|value| resolve(doc, value))
    {
        Some(Object::Name(name)) => String::from_utf8_lossy(name).into_owned(),
        Some(Object::Array(items)) => items
            .first()
            .and_then(|first| first.as_name().ok())
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .unwrap_or_default(),
        _ => String::new(),
    };
    PlacedImage {
        page,
        object: id.map(crate::objects::object_label),
        width,
        height,
        pixels_per_inch: across.min(down),
        color_space,
        bits_per_component: number(b"BitsPerComponent"),
        filter: names_in(doc, dict.get(b"Filter").ok())
            .iter()
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .collect::<Vec<_>>()
            .join(" "),
    }
}
