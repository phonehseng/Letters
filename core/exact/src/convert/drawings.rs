//! a page's floating drawings: pictures, shapes and rasterised vectors,
//! in content order, all behind the text (recipe 10).

use lopdf::Object;

use crate::docx::xml::{line_drawing, picture_drawing, rect_drawing, Anchor, Picture};
use crate::docx::Package;
use crate::pdf::{ImageItem, PageContent, PathItem};
use crate::plan::glyphs::hex_color;
use crate::plan::raster::{rasterise, RasterPicture, RenderFn};
use crate::plan::vectors::{classify, Shape};
use crate::report::PageReport;

enum Drawing<'a> {
    Shape(Shape),
    Image(&'a ImageItem),
    Raster(RasterPicture),
}

fn sequence_of(drawing: &Drawing) -> usize {
    match drawing {
        Drawing::Shape(Shape::Rect { sequence, .. } | Shape::Line { sequence, .. }) => *sequence,
        Drawing::Image(image) => image.sequence,
        Drawing::Raster(raster) => raster.sequence,
    }
}

/// the drawings of one page as xml, counting what went where.
pub fn page_drawings(
    package: &mut Package,
    page: &PageContent,
    source: &lopdf::Document,
    resources: Option<&Object>,
    render: Option<&RenderFn>,
    report: &mut PageReport,
) -> Vec<String> {
    let mut drawings: Vec<Drawing> = Vec::new();
    let mut complex: Vec<&PathItem> = page.shadings.iter().collect();
    for item in &page.paths {
        match classify(item) {
            Some(shapes) => drawings.extend(shapes.into_iter().map(Drawing::Shape)),
            None => complex.push(item),
        }
    }
    drawings.extend(page.images.iter().map(Drawing::Image));
    if !complex.is_empty() {
        match render {
            Some(render) => match rasterise(source, resources, page, &complex, render) {
                Ok(Some(raster)) => {
                    report.rasterised_items += raster.items;
                    report.raster_pictures += 1;
                    drawings.push(Drawing::Raster(raster));
                }
                Ok(None) => {}
                Err(reason) => {
                    report.vectors_left_out += complex.len();
                    report.problems.push(format!(
                        "vector content couldn’t be drawn as a picture ({reason})"
                    ));
                }
            },
            None => report.vectors_left_out += complex.len(),
        }
    }
    // a stable sort keeps the shapes of one path in their order
    drawings.sort_by_key(sequence_of);
    let mut xml = Vec::new();
    for drawing in drawings {
        match drawing {
            Drawing::Shape(shape) => {
                xml.push(shape_xml(package, &shape, page.height));
                report.shapes += 1;
            }
            Drawing::Image(image) => {
                xml.push(image_xml(package, image, page.height));
                report.pictures += 1;
                report.jpeg_kept += usize::from(image.picture.extension == "jpeg");
            }
            Drawing::Raster(raster) => {
                let relationship = package.add_media("png", raster.png.clone());
                let anchor = Anchor {
                    id: package.shape_id(),
                    left_pt: raster.left,
                    top_pt: page.height - raster.top,
                    width_pt: raster.right - raster.left,
                    height_pt: raster.top - raster.bottom,
                    behind: true,
                };
                xml.push(picture_drawing(&Picture {
                    anchor,
                    relationship: &relationship,
                    rotation_deg: 0.0,
                    flip_h: false,
                    flip_v: false,
                }));
            }
        }
    }
    xml
}

/// a picture placed by its image matrix: the extent from the matrix,
/// flips for negative scales, a rotation for rotated matrices.
fn image_xml(package: &mut Package, image: &ImageItem, page_height: f64) -> String {
    let [a, b, c, d, e, f] = image.matrix;
    let relationship = package.add_media(image.picture.extension, image.picture.bytes.clone());
    let id = package.shape_id();
    if b.abs() < 1e-6 && c.abs() < 1e-6 {
        let (width, height) = (a.abs(), d.abs());
        let (left, bottom) = (e + a.min(0.0), f + d.min(0.0));
        let anchor = Anchor {
            id,
            left_pt: left,
            top_pt: page_height - bottom - height,
            width_pt: width,
            height_pt: height,
            behind: true,
        };
        return picture_drawing(&Picture {
            anchor,
            relationship: &relationship,
            rotation_deg: 0.0,
            flip_h: a < 0.0,
            flip_v: d < 0.0,
        });
    }
    let (width, height) = (a.hypot(b), c.hypot(d));
    let (centre_x, centre_y) = (e + (a + c) / 2.0, f + (b + d) / 2.0);
    let angle = b.atan2(a).to_degrees();
    let anchor = Anchor {
        id,
        left_pt: centre_x - width / 2.0,
        top_pt: page_height - centre_y - height / 2.0,
        width_pt: width,
        height_pt: height,
        behind: true,
    };
    picture_drawing(&Picture {
        anchor,
        relationship: &relationship,
        rotation_deg: -angle,
        flip_h: false,
        flip_v: false,
    })
}

fn shape_xml(package: &mut Package, shape: &Shape, page_height: f64) -> String {
    let id = package.shape_id();
    match shape {
        Shape::Rect {
            left,
            bottom,
            right,
            top,
            fill,
            stroke,
            line_width,
            ..
        } => {
            let anchor = Anchor {
                id,
                left_pt: *left,
                top_pt: page_height - top,
                width_pt: right - left,
                height_pt: top - bottom,
                behind: true,
            };
            let fill = fill.map(hex_color);
            let stroke = stroke.map(hex_color);
            rect_drawing(&anchor, fill.as_deref(), stroke.as_deref(), *line_width)
        }
        Shape::Line {
            start,
            end,
            color,
            line_width,
            ..
        } => {
            // top-down coordinates from the page's top left
            let (x0, y0, x1, y1) = (start.0, page_height - start.1, end.0, page_height - end.1);
            let anchor = Anchor {
                id,
                left_pt: x0.min(x1),
                top_pt: y0.min(y1),
                width_pt: (x1 - x0).abs(),
                height_pt: (y1 - y0).abs(),
                behind: true,
            };
            let flip_h = x1 < x0;
            let flip_v = (y1 < y0) != flip_h;
            line_drawing(&anchor, flip_v, &hex_color(*color), *line_width)
        }
    }
}
