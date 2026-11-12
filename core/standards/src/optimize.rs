//! making a file smaller without changing how it looks: dropping unused
//! objects, storing identical streams once, compressing what isn't,
//! optionally downsampling pictures sharper than needed (by averaging,
//! which is lossless in the sense that no compression artefacts are
//! added), and optionally packing objects into object streams. the bytes
//! each measure saved are reported, and what isn't done is said.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{StandardsError, StandardsResult};
use crate::images::placed_images;
use crate::objects::{get_name, names_in};

/// choices for optimising.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeOptions {
    /// downsample pictures sharper than this many pixels per inch as
    /// placed; none keeps every picture as it is
    #[serde(default)]
    pub max_pixels_per_inch: Option<f64>,
    /// pack objects into object streams (pdf 1.5); smaller, but pdf/a-1
    /// forbids it
    #[serde(default)]
    pub object_streams: bool,
}

impl Default for OptimizeOptions {
    fn default() -> Self {
        Self {
            max_pixels_per_inch: None,
            object_streams: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measure {
    pub name: String,
    pub bytes_saved: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeReport {
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub measures: Vec<Measure>,
    /// what wasn't done, and why
    pub not_done: Vec<String>,
    /// the optimised file
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

/// what optimising doesn't do yet, said every time.
const NOT_DONE: [&str; 3] = [
    "Fonts embedded whole aren’t cut down to the characters used yet.",
    "The file isn’t linearised (fast web view): Letters doesn’t write linearised files yet.",
    "JPEG pictures aren’t compressed again: Letters has no JPEG encoder, and doing so would lose quality.",
];

/// optimises a document and returns the new file with what each measure saved.
pub fn optimize(doc: &mut Document, options: &OptimizeOptions) -> StandardsResult<OptimizeReport> {
    let before = size(doc)?;
    let mut measures = Vec::new();
    let mut current = before;
    let mut step =
        |doc: &mut Document, name: &str, apply: &dyn Fn(&mut Document)| -> StandardsResult<()> {
            apply(doc);
            let now = size(doc)?;
            measures.push(Measure {
                name: name.to_owned(),
                bytes_saved: current as i64 - now as i64,
            });
            current = now;
            Ok(())
        };
    step(doc, "Unused objects removed", &|doc| {
        doc.prune_objects();
    })?;
    step(doc, "Identical streams stored once", &deduplicate_streams)?;
    step(doc, "Uncompressed streams compressed", &compress_streams)?;
    if let Some(limit) = options.max_pixels_per_inch {
        step(doc, "Pictures sharper than needed downsampled", &|doc| {
            downsample(doc, limit)
        })?;
    }
    let classic = save(doc)?;
    let bytes = if options.object_streams {
        let mut modern = Vec::new();
        doc.save_modern(&mut modern)
            .map_err(|error| StandardsError::SaveFailed(error.to_string()))?;
        // tiny files can grow from the stream's own overhead; then the
        // classic layout is kept
        let smaller = modern.len() < classic.len();
        measures.push(Measure {
            name: "Objects packed into object streams".to_owned(),
            bytes_saved: if smaller {
                classic.len() as i64 - modern.len() as i64
            } else {
                0
            },
        });
        if smaller {
            modern
        } else {
            classic
        }
    } else {
        classic
    };
    Ok(OptimizeReport {
        bytes_before: before,
        bytes_after: bytes.len() as u64,
        measures,
        not_done: NOT_DONE.iter().map(|text| (*text).to_owned()).collect(),
        bytes,
    })
}

fn save(doc: &mut Document) -> StandardsResult<Vec<u8>> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|error| StandardsError::SaveFailed(error.to_string()))?;
    Ok(bytes)
}

fn size(doc: &Document) -> StandardsResult<u64> {
    Ok(save(&mut doc.clone())?.len() as u64)
}

/// streams with the same dictionary (apart from /Length) and the same bytes
/// are kept once, and every reference to a copy points at the first.
fn deduplicate_streams(doc: &mut Document) {
    let mut first_by_hash: HashMap<Vec<u8>, ObjectId> = HashMap::new();
    let mut replace: HashMap<ObjectId, ObjectId> = HashMap::new();
    for (id, object) in &doc.objects {
        let Object::Stream(stream) = object else {
            continue;
        };
        let mut dict = stream.dict.clone();
        dict.remove(b"Length");
        let mut hasher = Sha256::new();
        hasher.update(format!("{dict:?}").as_bytes());
        hasher.update(&stream.content);
        let hash = hasher.finalize().to_vec();
        match first_by_hash.get(&hash) {
            Some(first) => {
                replace.insert(*id, *first);
            }
            None => {
                first_by_hash.insert(hash, *id);
            }
        }
    }
    if replace.is_empty() {
        return;
    }
    for object in doc.objects.values_mut() {
        redirect(object, &replace, 0);
    }
    for (_, value) in doc.trailer.iter_mut() {
        redirect(value, &replace, 0);
    }
    for copy in replace.keys() {
        doc.objects.remove(copy);
    }
}

fn redirect(object: &mut Object, replace: &HashMap<ObjectId, ObjectId>, depth: usize) {
    if depth > 32 {
        return;
    }
    match object {
        Object::Reference(id) => {
            if let Some(first) = replace.get(id) {
                *id = *first;
            }
        }
        Object::Array(items) => items
            .iter_mut()
            .for_each(|item| redirect(item, replace, depth + 1)),
        Object::Dictionary(dict) => dict
            .iter_mut()
            .for_each(|(_, value)| redirect(value, replace, depth + 1)),
        Object::Stream(stream) => stream
            .dict
            .iter_mut()
            .for_each(|(_, value)| redirect(value, replace, depth + 1)),
        _ => {}
    }
}

/// flate-compresses streams stored without a filter. xmp metadata stays
/// plain text, so tools that scan files for it still find it.
fn compress_streams(doc: &mut Document) {
    for object in doc.objects.values_mut() {
        let Object::Stream(stream) = object else {
            continue;
        };
        let is_metadata =
            stream.dict.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"Metadata");
        if stream.dict.has(b"Filter") || is_metadata {
            continue;
        }
        let _ = stream.compress();
    }
}

/// pictures drawn sharper than `limit` pixels per inch are scaled down by
/// averaging blocks of pixels. only 8-bit gray and rgb pictures stored
/// plain or with flate, without masks or decode arrays, are touched.
fn downsample(doc: &mut Document, limit: f64) {
    let mut sharpest: HashMap<ObjectId, f64> = HashMap::new();
    for placed in placed_images(doc) {
        let Some(id) = placed.object.as_deref().and_then(parse_label) else {
            continue;
        };
        // a picture drawn twice keeps the detail its smaller drawing needs
        let entry = sharpest.entry(id).or_insert(f64::INFINITY);
        *entry = entry.min(placed.pixels_per_inch);
    }
    for (id, pixels_per_inch) in sharpest {
        if pixels_per_inch <= limit || !pixels_per_inch.is_finite() {
            continue;
        }
        let factor = (pixels_per_inch / limit).floor() as usize;
        if factor < 2 {
            continue;
        }
        if let Some(smaller) = shrink(doc, id, factor) {
            doc.objects.insert(id, Object::Stream(smaller));
        }
    }
}

fn parse_label(label: &str) -> Option<ObjectId> {
    let mut parts = label.split_whitespace();
    let number = parts.next()?.parse().ok()?;
    let generation = parts.next()?.parse().ok()?;
    Some((number, generation))
}

fn shrink(doc: &Document, id: ObjectId, factor: usize) -> Option<Stream> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let dict = &stream.dict;
    let channels = match get_name(doc, dict, b"ColorSpace")?.as_slice() {
        b"DeviceRGB" => 3,
        b"DeviceGray" => 1,
        _ => return None,
    };
    let simple = names_in(doc, dict.get(b"Filter").ok())
        .iter()
        .all(|filter| filter == b"FlateDecode")
        && !dict.has(b"SMask")
        && !dict.has(b"Mask")
        && !dict.has(b"Decode")
        && !dict.has(b"DecodeParms")
        && dict
            .get(b"BitsPerComponent")
            .ok()
            .and_then(|value| value.as_i64().ok())
            == Some(8);
    if !simple {
        return None;
    }
    let width = dict.get(b"Width").ok()?.as_i64().ok()? as usize;
    let height = dict.get(b"Height").ok()?.as_i64().ok()? as usize;
    let pixels = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    if pixels.len() < width * height * channels {
        return None;
    }
    let (new_width, new_height) = (width / factor, height / factor);
    if new_width == 0 || new_height == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(new_width * new_height * channels);
    for y in 0..new_height {
        for x in 0..new_width {
            for channel in 0..channels {
                let mut total = 0usize;
                for dy in 0..factor {
                    for dx in 0..factor {
                        let index =
                            ((y * factor + dy) * width + x * factor + dx) * channels + channel;
                        total += pixels[index] as usize;
                    }
                }
                out.push((total / (factor * factor)) as u8);
            }
        }
    }
    let mut new_dict: Dictionary = dict.clone();
    new_dict.set("Width", Object::Integer(new_width as i64));
    new_dict.set("Height", Object::Integer(new_height as i64));
    new_dict.remove(b"Filter");
    let mut smaller = Stream::new(new_dict, out);
    let _ = smaller.compress();
    Some(smaller)
}
