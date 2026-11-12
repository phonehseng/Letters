//! scenes written as pdf pages: vector paths, axial and radial shadings
//! for gradients, pictures as image xobjects, and real text in embedded
//! subset fonts (glyph ids as codes, with a /ToUnicode map so the text
//! can be searched and copied).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use lopdf::{Dictionary, Object, ObjectId, Stream};

use super::pdf_fonts::{add_font, FontUse};
use super::pdf_images::add_image;
use super::scene::{multiply, Item, Matrix, Paint, Scene, StrokeStyle};
use crate::color::Rgba;
use crate::error::SlidesError;
use crate::fonts::FontLibrary;
use crate::geometry::{OutlinePath, Segment};
use crate::model::MediaItem;

/// how much a made-up bold thickens glyph outlines, as a share of the
/// size (the 1/30 em stroke office and libreoffice use for fake bold)
const SYNTHETIC_BOLD_STROKE: f64 = 1.0 / 30.0;
/// the slant of a made-up italic (about 11 degrees)
const SYNTHETIC_ITALIC_SLANT: f64 = 0.2;

fn name(text: &str) -> Object {
    Object::Name(text.as_bytes().to_vec())
}

fn number(value: f64) -> String {
    let rounded = format!("{value:.4}");
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" || trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.to_string()
    }
}

/// one pdf page per scene.
pub fn write_pdf(
    scenes: &[Scene],
    fonts: &FontLibrary,
    media: &[MediaItem],
) -> Result<Vec<u8>, SlidesError> {
    let mut document = lopdf::Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_ids = add_fonts(&mut document, scenes, fonts);
    let mut writer = PageWriter {
        document: &mut document,
        media,
        images: HashMap::new(),
        font_ids: &font_ids,
    };
    let mut kids = Vec::new();
    for scene in scenes {
        kids.push(Object::Reference(writer.page(scene, pages_id)?));
    }
    let count = kids.len() as i64;
    let mut pages = Dictionary::new();
    pages.set("Type", name("Pages"));
    pages.set("Kids", Object::Array(kids));
    pages.set("Count", Object::Integer(count));
    document.objects.insert(pages_id, Object::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.set("Type", name("Catalog"));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = document.add_object(catalog);
    document.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    document
        .save_to(&mut out)
        .map_err(|error| SlidesError::RenderFailed(error.to_string()))?;
    Ok(out)
}

/// every font the scenes use, embedded once with the glyphs it draws.
fn add_fonts(
    document: &mut lopdf::Document,
    scenes: &[Scene],
    fonts: &FontLibrary,
) -> HashMap<u32, ObjectId> {
    let mut uses: BTreeMap<u32, FontUse> = BTreeMap::new();
    for item in scenes.iter().flat_map(|s| &s.items) {
        if let Item::Text { font, glyphs, .. } = item {
            let entry = uses.entry(*font).or_default();
            for (glyph, _, _, ch) in glyphs {
                entry.glyphs.entry(*glyph).or_insert_with(|| ch.to_string());
            }
        }
    }
    let mut ids = HashMap::new();
    for (index, (font, used)) in uses.iter().enumerate() {
        let Some(resolved) = fonts.fonts().get(*font as usize) else {
            continue;
        };
        ids.insert(
            *font,
            add_font(document, &resolved.face, used, &subset_tag(index)),
        );
    }
    ids
}

/// six capital letters naming a subset ("AAAAAB"), as pdf asks.
fn subset_tag(index: usize) -> String {
    let mut rest = index;
    let mut tag = String::new();
    for _ in 0..6 {
        tag.insert(0, char::from(b'A' + (rest % 26) as u8));
        rest /= 26;
    }
    tag
}

struct PageWriter<'a> {
    document: &'a mut lopdf::Document,
    media: &'a [MediaItem],
    images: HashMap<String, Option<ObjectId>>,
    font_ids: &'a HashMap<u32, ObjectId>,
}

/// the named resources one page refers to.
#[derive(Default)]
struct Resources {
    fonts: Dictionary,
    images: Dictionary,
    states: Dictionary,
    shadings: Dictionary,
}

impl PageWriter<'_> {
    fn page(&mut self, scene: &Scene, parent: ObjectId) -> Result<ObjectId, SlidesError> {
        let mut resources = Resources::default();
        // drawing works in points from the top left, y down, as the scene does
        let mut content = format!("1 0 0 -1 0 {} cm\n", number(scene.height));
        for item in &scene.items {
            self.item(item, &mut content, &mut resources);
        }
        let stream = Stream::new(Dictionary::new(), content.into_bytes());
        let mut stream = stream;
        let _ = stream.compress();
        let content_id = self.document.add_object(stream);
        let mut resource_dictionary = Dictionary::new();
        resource_dictionary.set("Font", Object::Dictionary(resources.fonts));
        resource_dictionary.set("XObject", Object::Dictionary(resources.images));
        resource_dictionary.set("ExtGState", Object::Dictionary(resources.states));
        resource_dictionary.set("Shading", Object::Dictionary(resources.shadings));
        let mut page = Dictionary::new();
        page.set("Type", name("Page"));
        page.set("Parent", Object::Reference(parent));
        page.set(
            "MediaBox",
            Object::Array(vec![
                0.into(),
                0.into(),
                Object::Real(scene.width as f32),
                Object::Real(scene.height as f32),
            ]),
        );
        page.set("Contents", Object::Reference(content_id));
        page.set("Resources", Object::Dictionary(resource_dictionary));
        Ok(self.document.add_object(page))
    }

    fn item(&mut self, item: &Item, out: &mut String, resources: &mut Resources) {
        match item {
            Item::Fill {
                matrix,
                paths,
                paint,
            } => self.fill(matrix, paths, paint, out, resources),
            Item::Stroke {
                matrix,
                paths,
                style,
            } => stroke(matrix, paths, style, out, resources),
            Item::Image {
                matrix,
                media_id,
                clip,
                alpha,
            } => self.image(matrix, media_id, clip, *alpha, out, resources),
            Item::Text { .. } => self.text(item, out, resources),
        }
    }

    fn fill(
        &mut self,
        matrix: &Matrix,
        paths: &[OutlinePath],
        paint: &Paint,
        out: &mut String,
        resources: &mut Resources,
    ) {
        out.push_str("q\n");
        push_matrix(out, matrix);
        match paint {
            Paint::Solid(color) => {
                set_alpha(out, resources, color.alpha, true);
                let _ = writeln!(out, "{} rg", rgb(color));
                push_paths(out, paths);
                out.push_str("f\n");
            }
            _ => {
                // pdf shadings have no alpha of their own; the stops' mean
                // alpha keeps faint glows faint (ion's master oval is 7 %)
                set_alpha(out, resources, mean_alpha(paint), true);
                push_paths(out, paths);
                out.push_str("W n\n");
                let shading = self.shading(paint);
                let key = format!("Sh{}", resources.shadings.len() + 1);
                resources
                    .shadings
                    .set(key.as_bytes().to_vec(), Object::Reference(shading));
                let _ = writeln!(out, "/{key} sh");
            }
        }
        out.push_str("Q\n");
    }

    /// an axial or radial shading through all the stops.
    fn shading(&mut self, paint: &Paint) -> ObjectId {
        let (stops, kind, coords) = match paint {
            Paint::Linear { stops, from, to } => (stops, 2, vec![from.0, from.1, to.0, to.1]),
            Paint::Radial {
                stops,
                centre,
                radius,
            } => (
                stops,
                3,
                vec![centre.0, centre.1, 0.0, centre.0, centre.1, *radius],
            ),
            Paint::Solid(color) => (
                &vec![(0.0, *color), (1.0, *color)],
                2,
                vec![0.0, 0.0, 1.0, 0.0],
            ),
        };
        let function = stitched_function(stops);
        let mut shading = Dictionary::new();
        shading.set("ShadingType", Object::Integer(kind));
        shading.set("ColorSpace", name("DeviceRGB"));
        shading.set(
            "Coords",
            Object::Array(coords.iter().map(|c| Object::Real(*c as f32)).collect()),
        );
        shading.set("Function", function);
        shading.set(
            "Extend",
            Object::Array(vec![Object::Boolean(true), Object::Boolean(true)]),
        );
        self.document.add_object(shading)
    }

    #[allow(clippy::too_many_arguments)]
    fn image(
        &mut self,
        matrix: &Matrix,
        media_id: &str,
        clip: &Option<(Matrix, Vec<OutlinePath>)>,
        alpha: f64,
        out: &mut String,
        resources: &mut Resources,
    ) {
        let Some(image) = self.image_object(media_id) else {
            return;
        };
        let key = format!("Im{}", resources.images.len() + 1);
        resources
            .images
            .set(key.as_bytes().to_vec(), Object::Reference(image));
        out.push_str("q\n");
        set_alpha(out, resources, alpha, true);
        // an image fills the unit square bottom up; the scene is top down
        let mut upright = multiply(matrix, &[1.0, 0.0, 0.0, -1.0, 0.0, 1.0]);
        if let Some((clip_matrix, paths)) = clip {
            // the clip stays in force for the image, so the image's own
            // matrix is given relative to the clip's
            push_matrix(out, clip_matrix);
            push_paths(out, paths);
            out.push_str("W n\n");
            upright = multiply(&invert(clip_matrix), &upright);
        }
        push_matrix(out, &upright);
        let _ = writeln!(out, "/{key} Do\nQ");
    }

    fn image_object(&mut self, media_id: &str) -> Option<ObjectId> {
        if let Some(found) = self.images.get(media_id) {
            return *found;
        }
        let item = self.media.iter().find(|item| item.id == media_id);
        let id = item.and_then(|item| add_image(self.document, &item.bytes, &item.content_type));
        self.images.insert(media_id.to_string(), id);
        id
    }

    fn text(&mut self, item: &Item, out: &mut String, resources: &mut Resources) {
        let Item::Text {
            matrix,
            font,
            size,
            color,
            glyphs,
            synthetic_bold,
            synthetic_italic,
        } = item
        else {
            return;
        };
        let Some(font_id) = self.font_ids.get(font) else {
            return;
        };
        let key = format!("F{font}");
        resources
            .fonts
            .set(key.as_bytes().to_vec(), Object::Reference(*font_id));
        out.push_str("q\n");
        push_matrix(out, matrix);
        set_alpha(out, resources, color.alpha, true);
        let _ = writeln!(out, "BT\n/{key} {} Tf\n{} rg", number(*size), rgb(color));
        if *synthetic_bold {
            let _ = writeln!(
                out,
                "2 Tr {} w {} RG",
                number(size * SYNTHETIC_BOLD_STROKE),
                rgb(color)
            );
        }
        let slant = if *synthetic_italic {
            SYNTHETIC_ITALIC_SLANT
        } else {
            0.0
        };
        for (glyph, x, y, _) in glyphs {
            let _ = writeln!(
                out,
                "1 0 {} -1 {} {} Tm <{glyph:04X}> Tj",
                number(slant),
                number(*x),
                number(*y)
            );
        }
        out.push_str("ET\nQ\n");
    }
}

fn stroke(
    matrix: &Matrix,
    paths: &[OutlinePath],
    style: &StrokeStyle,
    out: &mut String,
    resources: &mut Resources,
) {
    if paths.is_empty() || style.width <= 0.0 {
        return;
    }
    out.push_str("q\n");
    push_matrix(out, matrix);
    set_alpha(out, resources, style.color.alpha, false);
    let dashes: Vec<String> = style.dashes.iter().map(|d| number(*d)).collect();
    let _ = writeln!(
        out,
        "{} w {} J {} j [{}] 0 d {} RG",
        number(style.width),
        style.cap,
        style.join,
        dashes.join(" "),
        rgb(&style.color)
    );
    push_paths(out, paths);
    out.push_str("S\nQ\n");
}

/// the inverse of an affine matrix (the identity for a flat one, which
/// draws nothing anyway).
fn invert(matrix: &Matrix) -> Matrix {
    let [a, b, c, d, e, f] = *matrix;
    let determinant = a * d - b * c;
    if determinant.abs() < 1e-12 {
        return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }
    let (ia, ib, ic, id) = (
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
    );
    [ia, ib, ic, id, -(ia * e + ic * f), -(ib * e + id * f)]
}

fn mean_alpha(paint: &Paint) -> f64 {
    let stops = match paint {
        Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => stops,
        Paint::Solid(color) => return color.alpha,
    };
    if stops.is_empty() {
        return 1.0;
    }
    stops.iter().map(|(_, color)| color.alpha).sum::<f64>() / stops.len() as f64
}

fn rgb(color: &Rgba) -> String {
    format!(
        "{} {} {}",
        number(color.red),
        number(color.green),
        number(color.blue)
    )
}

fn push_matrix(out: &mut String, matrix: &Matrix) {
    let values: Vec<String> = matrix.iter().map(|v| number(*v)).collect();
    let _ = writeln!(out, "{} cm", values.join(" "));
}

fn push_paths(out: &mut String, paths: &[OutlinePath]) {
    for segment in paths.iter().flat_map(|p| &p.segments) {
        let _ = match segment {
            Segment::Move(x, y) => writeln!(out, "{} {} m", number(*x), number(*y)),
            Segment::Line(x, y) => writeln!(out, "{} {} l", number(*x), number(*y)),
            Segment::Cubic(x1, y1, x2, y2, x, y) => writeln!(
                out,
                "{} {} {} {} {} {} c",
                number(*x1),
                number(*y1),
                number(*x2),
                number(*y2),
                number(*x),
                number(*y)
            ),
            Segment::Close => writeln!(out, "h"),
        };
    }
}

/// a graphics state for a see-through fill or stroke, named on the page.
fn set_alpha(out: &mut String, resources: &mut Resources, alpha: f64, fill: bool) {
    if alpha >= 0.999 {
        return;
    }
    let key = format!("GS{}", resources.states.len() + 1);
    let mut state = Dictionary::new();
    state.set(if fill { "ca" } else { "CA" }, Object::Real(alpha as f32));
    resources
        .states
        .set(key.as_bytes().to_vec(), Object::Dictionary(state));
    let _ = writeln!(out, "/{key} gs");
}

/// a pdf function through every gradient stop: one exponential piece
/// per pair of neighbouring stops, stitched together.
fn stitched_function(stops: &[(f64, Rgba)]) -> Object {
    let piece = |from: &Rgba, to: &Rgba| {
        let mut function = Dictionary::new();
        function.set("FunctionType", Object::Integer(2));
        function.set("Domain", Object::Array(vec![0.into(), 1.into()]));
        let channels = |c: &Rgba| {
            vec![
                Object::Real(c.red as f32),
                Object::Real(c.green as f32),
                Object::Real(c.blue as f32),
            ]
        };
        function.set("C0", Object::Array(channels(from)));
        function.set("C1", Object::Array(channels(to)));
        function.set("N", Object::Integer(1));
        Object::Dictionary(function)
    };
    if stops.len() < 2 {
        let only = stops.first().map_or(Rgba::BLACK, |s| s.1);
        return piece(&only, &only);
    }
    let functions: Vec<Object> = stops
        .windows(2)
        .map(|pair| piece(&pair[0].1, &pair[1].1))
        .collect();
    let bounds: Vec<Object> = stops[1..stops.len() - 1]
        .iter()
        .map(|s| Object::Real(s.0 as f32))
        .collect();
    let encode: Vec<Object> = stops
        .windows(2)
        .flat_map(|_| [0.into(), 1.into()])
        .collect();
    let mut stitch = Dictionary::new();
    stitch.set("FunctionType", Object::Integer(3));
    stitch.set(
        "Domain",
        Object::Array(vec![
            Object::Real(stops[0].0 as f32),
            Object::Real(stops[stops.len() - 1].0 as f32),
        ]),
    );
    stitch.set("Functions", Object::Array(functions));
    stitch.set("Bounds", Object::Array(bounds));
    stitch.set("Encode", Object::Array(encode));
    Object::Dictionary(stitch)
}
