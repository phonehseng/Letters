//! images, forms and shadings met in a content stream.
//!
//! - an image fully under the areas is removed; one partly under them is
//!   replaced by a copy with the covered pixels filled (see `images.rs`).
//! - a form (a reusable piece of content drawn with `Do`) that crosses an
//!   area is walked like a page, with its own matrix. if anything inside
//!   changes, the page gets its own edited copy of the form under a new
//!   name, so other pages that use the same form keep the original.
//! - a shading (`sh`, a gradient) is clipped so it paints nothing inside
//!   the areas. gradients are formulas, not pictures, so there is nothing
//!   to cut out of them; a mesh shading's points do stay in the file.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::content::{self, inline_entry, Op};
use crate::geometry::{Matrix, Rect};
use crate::objects::{as_dict, dict_array, dict_get, flate_stream, numbers, resolve};
use crate::redact::images::{
    coverage, decode_array, decode_image, paint_covered, read_space, DecodedImage, FillWith,
};
use crate::redact::paths::complement_clip;
use crate::redact::walker::{WalkMode, Walker, MAX_FORM_DEPTH};

/// what happens to one drawn image.
enum ImageResult {
    /// no pixel is under an area
    Unchanged,
    /// every pixel is under an area, or the image is squashed flat
    Covered,
    /// some pixels changed; the new image (not built in a preview)
    Rewritten(Option<Stream>),
    /// it would need changing but cannot be decoded here
    Undecodable(String),
}

/// images with more pixels than this (a 10000 by 10000 scan) are not
/// edited pixel by pixel; one that crosses a box is removed whole.
const MAX_IMAGE_PIXELS: i64 = 100_000_000;

fn unit_square() -> Rect {
    Rect::from_corners(0.0, 0.0, 1.0, 1.0)
}

/// keys that describe or duplicate an image or form and must not be
/// carried over into an edited copy.
const STALE_KEYS: [&[u8]; 9] = [
    b"Length",
    b"Filter",
    b"DecodeParms",
    b"Metadata",
    b"Alternates",
    b"OPI",
    b"PieceInfo",
    b"LastModified",
    b"Thumb",
];

impl Walker<'_, '_> {
    pub fn draw_xobject(&mut self, op: Op) -> Result<(), String> {
        let Some(Object::Name(name)) = op.operands().first() else {
            self.emit(op);
            return Ok(());
        };
        let Some((id, Object::Stream(stream))) = self.referenced_resource(b"XObject", name) else {
            self.emit(op);
            return Ok(());
        };
        let name = name.clone();
        let before = self.out.len();
        match stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
        {
            Some(b"Image") => self.draw_image_xobject(op, id, stream),
            Some(b"Form") => self.draw_form(op, stream)?,
            _ => self.emit(op),
        }
        // the original name is still drawn only if its own Do was written
        let kept = self.out[before..].iter().any(|op| {
            op.name() == "Do" && op.operands().first() == Some(&Object::Name(name.clone()))
        });
        self.note_xobject(&name, kept);
        Ok(())
    }

    fn draw_image_xobject(&mut self, op: Op, id: ObjectId, stream: &Stream) {
        let ctm = self.state.ctm;
        let Some(area) = self.first_area_hit(&unit_square().transform(&ctm)) else {
            self.emit(op);
            return;
        };
        let result = self.redact_image(&stream.dict, &stream.content, &ctm);
        if matches!(result, ImageResult::Unchanged) {
            self.emit(op);
            return;
        }
        self.touch();
        self.tally(Some(area), true);
        if self.ctx.reference_counts.get(&id).copied().unwrap_or(0) > 1 {
            self.ctx.findings.shared_images.push(id);
        }
        match result {
            ImageResult::Rewritten(Some(new_image)) => {
                self.ctx.findings.counts.images_rewritten += 1;
                let new_id = self.ctx.new_objects.add(Object::Stream(new_image));
                let new_name = self.add_resource(b"XObject", Object::Reference(new_id));
                self.emit(Op::new("Do", vec![Object::Name(new_name)]));
            }
            ImageResult::Rewritten(None) => {
                self.ctx.findings.counts.images_rewritten += 1;
                self.emit(op);
            }
            ImageResult::Undecodable(reason) => self.remove_whole_image(&reason),
            _ => self.ctx.findings.counts.images_removed += 1,
        }
    }

    fn remove_whole_image(&mut self, reason: &str) {
        self.ctx.findings.counts.images_removed += 1;
        self.ctx
            .findings
            .wholesale
            .push(format!("an image was removed whole because {reason}"));
    }

    pub fn draw_inline_image(&mut self, op: Op) {
        let Op::InlineImage { dict, data } = &op else {
            self.emit(op);
            return;
        };
        let ctm = self.state.ctm;
        let Some(area) = self.first_area_hit(&unit_square().transform(&ctm)) else {
            self.emit(op);
            return;
        };
        let full = expand_inline_entries(dict);
        let result = self.redact_image(&full, data, &ctm);
        if matches!(result, ImageResult::Unchanged) {
            self.emit(op);
            return;
        }
        self.touch();
        self.tally(Some(area), true);
        match result {
            ImageResult::Rewritten(Some(new_image)) => {
                self.ctx.findings.counts.images_rewritten += 1;
                self.emit(inline_image_op(dict, &new_image));
            }
            ImageResult::Rewritten(None) => {
                self.ctx.findings.counts.images_rewritten += 1;
                self.emit(op.clone());
            }
            ImageResult::Undecodable(reason) => self.remove_whole_image(&reason),
            _ => self.ctx.findings.counts.images_removed += 1,
        }
    }

    /// decides what happens to an image drawn with `ctm`.
    fn redact_image(&mut self, dict: &Dictionary, data: &[u8], ctm: &Matrix) -> ImageResult {
        let doc = self.doc();
        let size = |key: &[u8]| {
            dict_get(doc, dict, key)
                .and_then(|o| o.as_i64().ok())
                .filter(|v| *v > 0)
        };
        let (Some(width), Some(height)) = (size(b"Width"), size(b"Height")) else {
            return ImageResult::Covered;
        };
        if width.saturating_mul(height) > MAX_IMAGE_PIXELS {
            return ImageResult::Undecodable("it is too large to change".into());
        }
        let hits = self.areas_hitting(&unit_square().transform(ctm).bounds());
        let Some(covered) = coverage(width as usize, height as usize, ctm, &hits) else {
            return ImageResult::Covered;
        };
        if covered.covered == 0 {
            return ImageResult::Unchanged;
        }
        if covered.is_full() {
            return ImageResult::Covered;
        }
        let is_mask = matches!(
            dict_get(doc, dict, b"ImageMask"),
            Some(Object::Boolean(true))
        );
        let space = if is_mask {
            None
        } else {
            match dict
                .get(b"ColorSpace")
                .ok()
                .and_then(|cs| read_space(doc, cs, &|name| self.named_colour_space(name)))
            {
                Some(space) => Some(space),
                None => return ImageResult::Undecodable("its colours can't be read".into()),
            }
        };
        if self.settings().mode == WalkMode::Preview {
            return match unsupported_filter(doc, dict) {
                Some(reason) => ImageResult::Undecodable(reason),
                None => ImageResult::Rewritten(None),
            };
        }
        let components = space.as_ref().map_or(1, |space| space.components());
        let mut decoded = match decode_image(doc, dict, data, components) {
            Ok(decoded) => decoded,
            Err(reason) => return ImageResult::Undecodable(reason),
        };
        if decoded.samples.components != components {
            return ImageResult::Undecodable("its data does not match its colours".into());
        }
        let decode = if decoded.was_jpeg {
            Vec::new()
        } else {
            decode_array(doc, dict)
        };
        let with = match space {
            Some(space) => FillWith::Colour {
                space,
                fill: self.settings().fill,
            },
            None => FillWith::NothingPainted,
        };
        paint_covered(&mut decoded, &covered, &with, &decode);
        let mut new_dict = dict.clone();
        for key in STALE_KEYS {
            new_dict.remove(key);
        }
        if !is_mask {
            new_dict.set("BitsPerComponent", decoded.samples.bits as i64);
        }
        self.redact_masks(&mut new_dict, ctm);
        ImageResult::Rewritten(Some(flate_stream(new_dict, &decoded.samples.data)))
    }

    /// soft masks (/SMask) and stencil masks (/Mask) are images too, and
    /// can carry the same shapes, so their covered pixels are replaced as
    /// well: opaque for a soft mask, shown for a stencil.
    fn redact_masks(&mut self, dict: &mut Dictionary, ctm: &Matrix) {
        for (key, with) in [
            (b"SMask".as_slice(), FillWith::Opaque),
            (b"Mask".as_slice(), FillWith::Shown),
        ] {
            let doc = self.doc();
            let Some(Object::Stream(mask)) = dict.get(key).ok().map(|o| resolve(doc, o)) else {
                continue;
            };
            match self.redact_mask(mask, ctm, &with) {
                Some(new_mask) => {
                    let id = self.ctx.new_objects.add(Object::Stream(new_mask));
                    dict.set(key, Object::Reference(id));
                }
                None => {
                    dict.remove(key);
                    self.ctx.findings.wholesale.push(
                        "an image's transparency mask was removed because it can't be read".into(),
                    );
                }
            }
        }
    }

    fn redact_mask(&self, mask: &Stream, ctm: &Matrix, with: &FillWith) -> Option<Stream> {
        let doc = self.doc();
        let size = |key: &[u8]| {
            dict_get(doc, &mask.dict, key)?
                .as_i64()
                .ok()
                .filter(|v| *v > 0)
                .map(|v| v as usize)
        };
        let width = size(b"Width")?;
        let height = size(b"Height")?;
        if (width as i64).saturating_mul(height as i64) > MAX_IMAGE_PIXELS {
            return None;
        }
        let hits = self.areas_hitting(&unit_square().transform(ctm).bounds());
        let covered = coverage(width, height, ctm, &hits)?;
        let mut decoded: DecodedImage = decode_image(doc, &mask.dict, &mask.content, 1).ok()?;
        if decoded.samples.components != 1 {
            return None;
        }
        let decode = if decoded.was_jpeg {
            Vec::new()
        } else {
            decode_array(doc, &mask.dict)
        };
        paint_covered(&mut decoded, &covered, with, &decode);
        let mut dict = mask.dict.clone();
        for key in STALE_KEYS {
            dict.remove(key);
        }
        if !matches!(
            dict_get(doc, &dict, b"ImageMask"),
            Some(Object::Boolean(true))
        ) {
            dict.set("BitsPerComponent", decoded.samples.bits as i64);
        }
        Some(flate_stream(dict, &decoded.samples.data))
    }

    fn named_colour_space(&self, name: &[u8]) -> Option<Object> {
        self.resource_owned(b"ColorSpace", name)
    }

    fn draw_form(&mut self, op: Op, stream: &Stream) -> Result<(), String> {
        let doc = self.doc();
        let matrix = dict_array(doc, &stream.dict, b"Matrix")
            .and_then(|array| Matrix::from_numbers(&numbers(doc, array)))
            .unwrap_or(Matrix::IDENTITY);
        let form_ctm = matrix.then(&self.state.ctm);
        let bbox = dict_array(doc, &stream.dict, b"BBox")
            .map(|array| numbers(doc, array))
            .and_then(|values| match values.as_slice() {
                [x0, y0, x1, y1] => {
                    Some(Rect::from_corners(*x0, *y0, *x1, *y1).transform(&form_ctm))
                }
                _ => None,
            });
        let collecting = self.settings().mode == WalkMode::Collect;
        let crosses = bbox.is_none_or(|quad| self.first_area_hit(&quad).is_some());
        if !collecting && !crosses {
            self.emit(op);
            return Ok(());
        }
        if self.depth >= MAX_FORM_DEPTH {
            if !collecting {
                self.touch();
                self.ctx
                    .findings
                    .wholesale
                    .push("a drawing nested too deeply to follow was removed".into());
            } else {
                self.emit(op);
            }
            return Ok(());
        }
        let content = crate::objects::decoded_content(stream)
            .map_err(|error| format!("a drawing on it can't be read ({error})"))?;
        let ops = content::parse(&content)
            .map_err(|error| format!("a drawing on it can't be read ({error})"))?;
        let (resources, own_resources) = self.form_resources(&stream.dict);
        let output = self.walk_child(resources.clone(), own_resources, form_ctm, ops)?;
        if !output.changed {
            self.emit(op);
            return Ok(());
        }
        self.touch();
        let fully_covered =
            bbox.is_some_and(|quad| self.settings().areas.iter().any(|area| quad.inside(area)));
        if fully_covered {
            return Ok(());
        }
        if self.settings().mode != WalkMode::Redact {
            self.emit(op);
            return Ok(());
        }
        let mut dict = stream.dict.clone();
        for key in STALE_KEYS {
            dict.remove(key);
        }
        let merged = merge_resources(doc, &resources, &output.additions, &output.unused_xobjects);
        dict.set("Resources", Object::Dictionary(merged));
        let mut ops = output.ops;
        ops.extend((0..output.open_saves).map(|_| Op::new("Q", vec![])));
        let new_form = flate_stream(dict, &content::serialize(&ops));
        let new_id = self.ctx.new_objects.add(Object::Stream(new_form));
        let new_name = self.add_resource(b"XObject", Object::Reference(new_id));
        self.emit(Op::new("Do", vec![Object::Name(new_name)]));
        Ok(())
    }

    pub fn draw_shading(&mut self, op: Op) {
        if self.settings().areas.is_empty() {
            self.emit(op);
            return;
        }
        let ctm = self.state.ctm;
        let bounds = self
            .shading_bounds(&op)
            .map(|rect| rect.transform(&ctm).bounds());
        let hits: Vec<Rect> = match bounds {
            Some(bounds) => self.areas_hitting(&bounds),
            None => self.settings().areas.clone(),
        };
        if hits.is_empty() {
            self.emit(op);
            return;
        }
        self.touch();
        let Some(clip) = complement_clip(&hits, &ctm) else {
            // a squashed matrix paints nothing anyway
            return;
        };
        self.ctx.findings.counts.shadings_clipped += 1;
        self.emit(Op::new("q", vec![]));
        for clip_op in clip {
            self.emit(clip_op);
        }
        self.emit(op);
        self.emit(Op::new("Q", vec![]));
    }

    fn shading_bounds(&self, op: &Op) -> Option<Rect> {
        let doc = self.doc();
        let Some(Object::Name(name)) = op.operands().first() else {
            return None;
        };
        let shading = self.resource_owned(b"Shading", name)?;
        let values = numbers(doc, dict_array(doc, as_dict(&shading)?, b"BBox")?);
        match values.as_slice() {
            [x0, y0, x1, y1] => Some(Rect::from_corners(*x0, *y0, *x1, *y1)),
            _ => None,
        }
    }
}

/// the reason an image's filters cannot be undone here, if they can't.
fn unsupported_filter(doc: &Document, dict: &Dictionary) -> Option<String> {
    let names: Vec<Vec<u8>> = match dict_get(doc, dict, b"Filter") {
        Some(Object::Name(name)) => vec![name.clone()],
        Some(Object::Array(items)) => items
            .iter()
            .filter_map(|item| resolve(doc, item).as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    };
    names.iter().find_map(|name| match name.as_slice() {
        b"JPXDecode" => Some("it is stored as JPEG 2000".to_owned()),
        b"JBIG2Decode" => Some("it is stored as JBIG2".to_owned()),
        b"CCITTFaxDecode" | b"CCF" => Some("it is stored as CCITT fax".to_owned()),
        _ => None,
    })
}

/// an inline image's entries under their full names, so the same code
/// reads inline images and image objects.
fn expand_inline_entries(dict: &Dictionary) -> Dictionary {
    let pairs: [(&[u8], &[u8]); 9] = [
        (b"W", b"Width"),
        (b"H", b"Height"),
        (b"BPC", b"BitsPerComponent"),
        (b"CS", b"ColorSpace"),
        (b"D", b"Decode"),
        (b"DP", b"DecodeParms"),
        (b"F", b"Filter"),
        (b"IM", b"ImageMask"),
        (b"I", b"Interpolate"),
    ];
    let mut full = Dictionary::new();
    for (short, long) in pairs {
        if let Some(value) = inline_entry(dict, short, long) {
            full.set(long, value.clone());
        }
    }
    full
}

/// a rewritten inline image, stored as ascii hex so its bytes can never
/// be mistaken for the `EI` that ends it.
fn inline_image_op(original: &Dictionary, image: &Stream) -> Op {
    let plain = image.decompressed_content().unwrap_or_default();
    let mut dict = Dictionary::new();
    for (short, long) in [
        (b"W".as_slice(), b"Width".as_slice()),
        (b"H", b"Height"),
        (b"CS", b"ColorSpace"),
        (b"D", b"Decode"),
        (b"IM", b"ImageMask"),
        (b"I", b"Interpolate"),
    ] {
        if let Some(value) = inline_entry(original, short, long) {
            dict.set(short, value.clone());
        }
    }
    if let Ok(bits) = image.dict.get(b"BitsPerComponent") {
        if !matches!(
            inline_entry(original, b"IM", b"ImageMask"),
            Some(Object::Boolean(true))
        ) {
            dict.set("BPC", bits.clone());
        }
    }
    dict.set("F", Object::Name(b"AHx".to_vec()));
    let mut data: Vec<u8> = plain
        .iter()
        .flat_map(|byte| format!("{byte:02X}").into_bytes())
        .collect();
    data.push(b'>');
    Op::InlineImage { dict, data }
}

/// a resources dictionary with new entries added and xobjects that are
/// no longer drawn taken out. changed categories are written out directly
/// so a shared original is left as it was.
pub fn merge_resources(
    doc: &Document,
    base: &Dictionary,
    additions: &BTreeMap<Vec<u8>, Vec<(Vec<u8>, Object)>>,
    unused_xobjects: &[Vec<u8>],
) -> Dictionary {
    let mut merged = base.clone();
    if !unused_xobjects.is_empty() {
        if let Some(Object::Dictionary(xobjects)) =
            base.get(b"XObject").ok().map(|o| resolve(doc, o))
        {
            let mut kept = xobjects.clone();
            for name in unused_xobjects {
                kept.remove(name);
            }
            merged.set("XObject", Object::Dictionary(kept));
        }
    }
    for (category, entries) in additions {
        let mut inner = match merged.get(category).ok().map(|o| resolve(doc, o)) {
            Some(Object::Dictionary(dict)) => dict.clone(),
            _ => Dictionary::new(),
        };
        for (name, value) in entries {
            inner.set(name.clone(), value.clone());
        }
        merged.set(category.clone(), Object::Dictionary(inner));
    }
    merged
}
