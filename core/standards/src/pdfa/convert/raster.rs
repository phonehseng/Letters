//! pdf/a-1 forbids transparency (19005-1 6.4). where a page uses it, the
//! affected area is turned into a picture of how it looks, rendered by the
//! caller from the original document. a picture with a transparent
//! background drawn straight on the page becomes a picture of just its
//! area; any other transparency turns the whole page into a picture, with
//! its text kept on top as invisible text so it can still be searched and
//! copied. every area turned into a picture is reported.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::objects::{get_dict, get_name, inherited, page_ids, rect_of, resolve};

/// a page rendered by the caller: rgb pixels, row by row from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// renders a page of the original document (before conversion) at a
/// scale, showing its crop box. in the app, letters-pdf's renderer.
pub trait PageRenderer {
    fn render(&self, page: u32, pixels_per_point: f32) -> Option<RenderedPage>;
}

/// one area turned into a picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RasterisedArea {
    pub page: u32,
    /// in pdf units from the page's bottom left: left, bottom, right, top
    pub area: [f64; 4],
    /// the whole page became a picture (its text kept as invisible text)
    pub whole_page: bool,
}

/// an affine matrix [a b c d e f] (iso 32000 8.3.3).
type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn multiply(first: Matrix, then: Matrix) -> Matrix {
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

fn invert(matrix: Matrix) -> Option<Matrix> {
    let [a, b, c, d, e, f] = matrix;
    let determinant = a * d - b * c;
    if determinant.abs() < 1e-12 {
        return None;
    }
    let (ia, ib, ic, id) = (
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
    );
    Some([ia, ib, ic, id, -(e * ia + f * ic), -(e * ib + f * id)])
}

fn operand_numbers(operation: &Operation) -> Vec<f64> {
    operation
        .operands
        .iter()
        .filter_map(crate::objects::number_of)
        .collect()
}

/// the visible box of a page (crop box within media box), and whether it
/// is turned, which this module doesn't handle.
fn page_box(doc: &Document, page_id: ObjectId) -> Option<([f64; 4], bool)> {
    let media = inherited(doc, page_id, b"MediaBox").and_then(|value| rect_of(doc, value))?;
    let crop = inherited(doc, page_id, b"CropBox")
        .and_then(|value| rect_of(doc, value))
        .unwrap_or(media);
    let rotate = inherited(doc, page_id, b"Rotate")
        .and_then(crate::objects::number_of)
        .unwrap_or(0.0);
    Some((crop, rotate as i64 % 360 != 0))
}

/// turns the transparent parts of `pages` into pictures. returns the areas
/// and the pages it couldn't do (turned pages, or no rendering).
pub(super) fn rasterise(
    doc: &mut Document,
    pages: &[u32],
    renderer: &dyn PageRenderer,
    pixels_per_point: f32,
) -> (Vec<RasterisedArea>, Vec<u32>) {
    let ids = page_ids(doc);
    let mut areas = Vec::new();
    let mut failed = Vec::new();
    for &page in pages {
        let Some(&page_id) = ids.get(page as usize) else {
            continue;
        };
        let Some((crop, turned)) = page_box(doc, page_id) else {
            failed.push(page);
            continue;
        };
        let rendered = renderer.render(page, pixels_per_point);
        let (Some(rendered), false) = (rendered, turned) else {
            failed.push(page);
            continue;
        };
        let picture = PagePicture {
            rendered,
            crop,
            scale: pixels_per_point as f64,
        };
        match only_soft_masked_images(doc, page_id) {
            true => areas.extend(replace_images(doc, page_id, page, &picture)),
            false => {
                replace_page(doc, page_id, &picture);
                areas.push(RasterisedArea {
                    page,
                    area: crop,
                    whole_page: true,
                });
            }
        }
    }
    (areas, failed)
}

struct PagePicture {
    rendered: RenderedPage,
    crop: [f64; 4],
    scale: f64,
}

impl PagePicture {
    /// the pixels covering an area of the page, as an image xobject.
    fn image(&self, area: [f64; 4]) -> Option<Stream> {
        let [left, bottom, right, top] = area;
        let to_x = |x: f64| {
            ((x - self.crop[0]) * self.scale).clamp(0.0, self.rendered.width as f64) as u32
        };
        let to_y = |y: f64| {
            ((self.crop[3] - y) * self.scale).clamp(0.0, self.rendered.height as f64) as u32
        };
        let (x0, x1, y0, y1) = (to_x(left), to_x(right), to_y(top), to_y(bottom));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let width = self.rendered.width as usize;
        let mut rgb = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 3) as usize);
        for y in y0..y1 {
            let start = (y as usize * width + x0 as usize) * 3;
            let end = (y as usize * width + x1 as usize) * 3;
            rgb.extend_from_slice(self.rendered.rgb.get(start..end)?);
        }
        let mut stream = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image",
                "Width" => (x1 - x0) as i64, "Height" => (y1 - y0) as i64,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            },
            rgb,
        );
        let _ = stream.compress();
        Some(stream)
    }
}

/// the page draws transparency only through pictures with soft masks that
/// it paints itself (not inside other drawings or through graphics states).
fn only_soft_masked_images(doc: &Document, page_id: ObjectId) -> bool {
    let Some(resources) =
        inherited(doc, page_id, b"Resources").and_then(|value| value.as_dict().ok())
    else {
        return false;
    };
    let state_transparency = get_dict(doc, resources, b"ExtGState").is_some_and(|states| {
        states.iter().any(|(_, state)| {
            resolve(doc, state).as_dict().is_ok_and(|state| {
                state.has(b"SMask") || state.has(b"ca") || state.has(b"CA") || state.has(b"BM")
            })
        })
    });
    let form_drawings = get_dict(doc, resources, b"XObject").is_some_and(|xobjects| {
        xobjects.iter().any(|(_, value)| {
            resolve(doc, value).as_stream().is_ok_and(|stream| {
                get_name(doc, &stream.dict, b"Subtype").as_deref() == Some(b"Form")
            })
        })
    });
    !state_transparency && !form_drawings
}

fn page_operations(doc: &Document, page_id: ObjectId) -> Vec<Operation> {
    let bytes = doc.get_page_content(page_id);
    Content::decode(&bytes)
        .map(|content| content.operations)
        .unwrap_or_default()
}

fn set_page_content(
    doc: &mut Document,
    page_id: ObjectId,
    operations: Vec<Operation>,
    resources: Dictionary,
) {
    let bytes = Content { operations }.encode().unwrap_or_default();
    let mut stream = Stream::new(Dictionary::new(), bytes);
    let _ = stream.compress();
    let content = doc.add_object(stream);
    if let Ok(page) = doc.get_dictionary_mut(page_id) {
        page.set("Contents", Object::Reference(content));
        page.set("Resources", Object::Dictionary(resources));
        page.remove(b"Group");
    }
}

fn page_resources(doc: &Document, page_id: ObjectId) -> Dictionary {
    inherited(doc, page_id, b"Resources")
        .and_then(|value| value.as_dict().ok())
        .cloned()
        .unwrap_or_default()
}

fn add_xobject(doc: &mut Document, resources: &mut Dictionary, name: &str, image: Stream) {
    let id = doc.add_object(image);
    let mut xobjects = get_dict(doc, resources, b"XObject")
        .cloned()
        .unwrap_or_default();
    xobjects.set(name.as_bytes().to_vec(), Object::Reference(id));
    resources.set("XObject", Object::Dictionary(xobjects));
}

/// each soft-masked picture becomes a picture of its area as it looks.
fn replace_images(
    doc: &mut Document,
    page_id: ObjectId,
    page: u32,
    picture: &PagePicture,
) -> Vec<RasterisedArea> {
    let mut resources = page_resources(doc, page_id);
    let xobjects = get_dict(doc, &resources, b"XObject")
        .cloned()
        .unwrap_or_default();
    let mut matrix = IDENTITY;
    let mut saved: Vec<Matrix> = Vec::new();
    let mut out = Vec::new();
    let mut areas = Vec::new();
    for operation in page_operations(doc, page_id) {
        match operation.operator.as_str() {
            "q" => saved.push(matrix),
            "Q" => matrix = saved.pop().unwrap_or(IDENTITY),
            "cm" => {
                if let [a, b, c, d, e, f] = operand_numbers(&operation)[..] {
                    matrix = multiply([a, b, c, d, e, f], matrix);
                }
            }
            "Do" => {
                let name = operation
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                let masked = xobjects
                    .get(&name)
                    .ok()
                    .map(|value| resolve(doc, value))
                    .and_then(|value| value.as_stream().ok())
                    .is_some_and(|stream| stream.dict.has(b"SMask"));
                if masked {
                    let area = unit_square_bounds(matrix);
                    let replacement_name = format!("LtrRaster{}", areas.len() + 1);
                    if let (Some(image), Some(inverse)) = (picture.image(area), invert(matrix)) {
                        add_xobject(doc, &mut resources, &replacement_name, image);
                        let placed = multiply(
                            [
                                area[2] - area[0],
                                0.0,
                                0.0,
                                area[3] - area[1],
                                area[0],
                                area[1],
                            ],
                            inverse,
                        );
                        out.extend(draw_xobject(&replacement_name, placed));
                        areas.push(RasterisedArea {
                            page,
                            area,
                            whole_page: false,
                        });
                        continue;
                    }
                }
            }
            _ => {}
        }
        out.push(operation);
    }
    set_page_content(doc, page_id, out, resources);
    areas
}

/// q matrix cm /name Do Q
fn draw_xobject(name: &str, matrix: Matrix) -> Vec<Operation> {
    let numbers: Vec<Object> = matrix
        .iter()
        .map(|value| Object::Real(*value as f32))
        .collect();
    vec![
        Operation::new("q", vec![]),
        Operation::new("cm", numbers),
        Operation::new("Do", vec![Object::Name(name.as_bytes().to_vec())]),
        Operation::new("Q", vec![]),
    ]
}

/// where an image (drawn into the unit square) lands on the page.
fn unit_square_bounds(matrix: Matrix) -> [f64; 4] {
    let [a, b, c, d, e, f] = matrix;
    let corners = [
        (e, f),
        (a + e, b + f),
        (c + e, d + f),
        (a + c + e, b + d + f),
    ];
    let xs = corners.iter().map(|(x, _)| *x);
    let ys = corners.iter().map(|(_, y)| *y);
    [
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    ]
}

/// the operators that position and show text; everything else is dropped
/// when the page becomes a picture.
const TEXT_OPERATORS: [&str; 22] = [
    "q", "Q", "cm", "BT", "ET", "Tf", "Td", "TD", "Tm", "T*", "Tj", "TJ", "'", "\"", "Tc", "Tw",
    "Tz", "TL", "Ts", "Tr", "BMC", "EMC",
];

/// the whole page becomes a picture, with its text kept invisible on top
/// (text render mode 3, iso 32000 9.3.6) for searching and copying.
fn replace_page(doc: &mut Document, page_id: ObjectId, picture: &PagePicture) {
    let mut resources = page_resources(doc, page_id);
    let Some(image) = picture.image(picture.crop) else {
        return;
    };
    add_xobject(doc, &mut resources, "LtrPage", image);
    let [left, bottom, right, top] = picture.crop;
    let mut operations = draw_xobject(
        "LtrPage",
        [right - left, 0.0, 0.0, top - bottom, left, bottom],
    );
    for operation in page_operations(doc, page_id) {
        if !TEXT_OPERATORS.contains(&operation.operator.as_str()) {
            continue;
        }
        let is_begin = operation.operator == "BT";
        let operation = if operation.operator == "Tr" {
            Operation::new("Tr", vec![Object::Integer(3)])
        } else {
            operation
        };
        operations.push(operation);
        if is_begin {
            operations.push(Operation::new("Tr", vec![Object::Integer(3)]));
        }
    }
    // the transparency settings are no longer drawn with
    resources.remove(b"ExtGState");
    set_page_content(doc, page_id, operations, resources);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices_invert_and_place_images() {
        let matrix = [2.0, 0.0, 0.0, 3.0, 10.0, 20.0];
        let inverse = invert(matrix).unwrap();
        let back = multiply(matrix, inverse);
        for (value, expected) in back.iter().zip(IDENTITY) {
            assert!((value - expected).abs() < 1e-9);
        }
        assert_eq!(unit_square_bounds(matrix), [10.0, 20.0, 12.0, 23.0]);
    }
}
