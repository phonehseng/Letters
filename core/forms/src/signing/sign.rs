//! signing a document.

use std::time::SystemTime;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::cms::build_signed_data;
use super::incremental::{write_update, HandWritten};
use super::keys::{open_pkcs12, Signer};
use crate::add::{add_field, ensure_form, unique_name, NewField, NewFieldKind};
use crate::appearance::{ensure_default_font, WidgetBox};
use crate::error::{FormError, FormResult};
use crate::fonts::{FontMetrics, StandardFont};
use crate::geometry::{number, VisibleRect};
use crate::image::add_png_image;
use crate::objects::{check_document, name, page_ids, push_to_array, real, text_string};
use crate::read::{kind_of, Alignment, FieldKind};
use crate::text::{literal_string, win_ansi_byte};
use crate::tree::find_field;

/// where the signature goes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SignaturePlacement {
    /// an empty signature field that is already in the form
    ExistingField { field: String },
    /// a new signature field drawn at this place (points from the visible top left)
    NewField {
        page: u32,
        rect: VisibleRect,
        #[serde(default)]
        name: Option<String>,
    },
    /// a signature with nothing to see on the page
    Invisible {
        #[serde(default)]
        name: Option<String>,
    },
}

/// what a visible signature shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureAppearance {
    /// a picture of a handwritten or typed signature, as png
    #[serde(default)]
    pub image_png: Option<Vec<u8>>,
    /// lines of text: who signed, when, and the reason and place if given
    #[serde(default = "yes")]
    pub show_details: bool,
}

fn yes() -> bool {
    true
}

impl Default for SignatureAppearance {
    fn default() -> Self {
        Self {
            image_png: None,
            show_details: true,
        }
    }
}

/// everything needed to sign.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignRequest {
    /// the certificate file (.p12 or .pfx) and its password
    pub pkcs12: Vec<u8>,
    pub password: String,
    pub placement: SignaturePlacement,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    /// the signer's name as shown; the certificate's name when not given
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub contact_info: Option<String>,
    #[serde(default)]
    pub appearance: SignatureAppearance,
}

/// room for the signature, in bytes, beyond the certificates it carries.
const SIGNATURE_OVERHEAD: usize = 4096;

/// signs a document and returns the signed file.
///
/// the original bytes are kept exactly; the signature and its field are
/// added after them as an incremental update, so earlier signatures stay
/// valid.
pub fn sign(doc_bytes: &[u8], request: &SignRequest) -> FormResult<Vec<u8>> {
    let signer = open_pkcs12(&request.pkcs12, &request.password)?;
    let before = Document::load_mem(doc_bytes)?;
    check_document(&before)?;
    let signing_time = SystemTime::now();
    let signer_name = request
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| common_name(&signer))
        .unwrap_or_else(|| "Unknown".to_owned());

    let mut after = before.clone();
    let (field_id, widget_id) = prepare_field(&mut after, &request.placement)?;
    let signature_id = after.new_object_id();
    after
        .get_dictionary_mut(field_id)?
        .set("V", Object::Reference(signature_id));
    let form = ensure_form(&mut after)?;
    // 1 = the document has signatures, 2 = only append to it from now on
    after
        .get_dictionary_mut(form)?
        .set("SigFlags", Object::Integer(3));
    draw_visible_signature(&mut after, widget_id, request, &signer_name, signing_time)?;

    let reserved = signature_room(&signer);
    let body = signature_dictionary(request, &signer_name, signing_time, reserved)?;
    let update = write_update(
        doc_bytes,
        &before,
        &after,
        &[HandWritten {
            id: signature_id,
            body,
        }],
    )?;
    let start = update.offsets[&signature_id];
    finish_signature(update.bytes, start, &signer, signing_time, reserved)
}

fn common_name(signer: &Signer) -> Option<String> {
    super::keys::common_name(&signer.certificate.tbs_certificate().subject().to_string())
}

/// enough room for the certificates the signature carries, plus the
/// signature and its attributes.
fn signature_room(signer: &Signer) -> usize {
    let certificates: usize =
        signer.certificate_der.len() + signer.chain_der.iter().map(Vec::len).sum::<usize>();
    certificates + signer.key.signature_size() + SIGNATURE_OVERHEAD
}

/// finds or makes the signature field and returns its field and widget ids.
fn prepare_field(
    doc: &mut Document,
    placement: &SignaturePlacement,
) -> FormResult<(ObjectId, ObjectId)> {
    match placement {
        SignaturePlacement::ExistingField { field } => {
            let node = find_field(doc, field)?;
            if kind_of(doc, &node) != Some(FieldKind::Signature) {
                return Err(FormError::NotASignatureField(field.clone()));
            }
            if node.inherited(doc, b"V").is_some() {
                return Err(FormError::AlreadySigned(field.clone()));
            }
            Ok((node.id, node.widgets[0]))
        }
        SignaturePlacement::NewField { page, rect, name } => {
            let spec = NewField {
                name: name.clone().unwrap_or_else(|| "Signature".to_owned()),
                page: *page,
                rect: *rect,
                kind: NewFieldKind::Signature,
                tooltip: None,
                required: false,
                read_only: false,
                font_size: 0.0,
                alignment: Alignment::Left,
            };
            let made = add_field(doc, &spec)?;
            let node = find_field(doc, &made)?;
            Ok((node.id, node.widgets[0]))
        }
        SignaturePlacement::Invisible { name } => add_invisible_field(doc, name.as_deref()),
    }
}

/// an invisible signature still needs a field and a widget; the widget has
/// no size, sits on the first page and is marked as printable and locked.
fn add_invisible_field(
    doc: &mut Document,
    wanted: Option<&str>,
) -> FormResult<(ObjectId, ObjectId)> {
    let form = ensure_form(doc)?;
    let field_name = unique_name(doc, wanted.unwrap_or("Signature"));
    let first_page = *page_ids(doc).first().ok_or(FormError::UnknownPage(1))?;
    let mut dict = Dictionary::new();
    dict.set("FT", name("Sig"));
    dict.set("T", text_string(&field_name));
    dict.set("Type", name("Annot"));
    dict.set("Subtype", name("Widget"));
    dict.set(
        "Rect",
        Object::Array(vec![0.into(), 0.into(), 0.into(), 0.into()]),
    );
    dict.set("P", Object::Reference(first_page));
    // 4 = print, 128 = locked
    dict.set("F", Object::Integer(132));
    let id = doc.add_object(Object::Dictionary(dict));
    push_to_array(doc, form, b"Fields", Object::Reference(id))?;
    push_to_array(doc, first_page, b"Annots", Object::Reference(id))?;
    Ok((id, id))
}

/// the signature dictionary, written by hand so the places of /ByteRange
/// and /Contents are known. both start as placeholders of a fixed width.
fn signature_dictionary(
    request: &SignRequest,
    signer_name: &str,
    signing_time: SystemTime,
    reserved: usize,
) -> FormResult<Vec<u8>> {
    let mut body = b"<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached".to_vec();
    body.extend_from_slice(format!(" /ByteRange [0 {0} {0} {0}]", "0".repeat(10)).as_bytes());
    body.extend_from_slice(b" /Contents <");
    body.extend(std::iter::repeat_n(b'0', reserved * 2));
    body.extend_from_slice(b">");
    let mut text_entry = |key: &str, value: &str| -> FormResult<()> {
        body.extend_from_slice(format!(" /{key} ").as_bytes());
        lopdf::Writer::write_object(&mut body, &text_string(value))
            .map_err(|error| FormError::SigningFailed(error.to_string()))
    };
    text_entry("Name", signer_name)?;
    text_entry("M", &pdf_date(signing_time))?;
    for (key, value) in [
        ("Reason", &request.reason),
        ("Location", &request.location),
        ("ContactInfo", &request.contact_info),
    ] {
        if let Some(value) = value.as_deref().filter(|value| !value.trim().is_empty()) {
            text_entry(key, value)?;
        }
    }
    body.extend_from_slice(b" /Prop_Build << /App << /Name /Letters >> >> >>");
    Ok(body)
}

/// fills in /ByteRange, digests the covered bytes and puts the signature
/// into /Contents.
fn finish_signature(
    mut bytes: Vec<u8>,
    start: usize,
    signer: &Signer,
    signing_time: SystemTime,
    reserved: usize,
) -> FormResult<Vec<u8>> {
    let missing = || FormError::SigningFailed("the signature placeholder went missing".into());
    let range_at =
        find(&bytes, start, b"/ByteRange [").ok_or_else(missing)? + b"/ByteRange [".len();
    let contents_open =
        find(&bytes, start, b"/Contents <").ok_or_else(missing)? + b"/Contents ".len();
    let contents_close = contents_open + 1 + reserved * 2;
    if bytes.get(contents_close) != Some(&b'>') {
        return Err(missing());
    }
    let after_contents = contents_close + 1;
    let tail_length = bytes.len() - after_contents;

    // the placeholder "0 0000000000 0000000000 0000000000" is 34 characters
    let range = format!("0 {} {} {}", contents_open, after_contents, tail_length);
    let placeholder_width = 1 + 3 * 11;
    if range.len() > placeholder_width {
        return Err(FormError::SigningFailed(
            "the file is too large to sign".into(),
        ));
    }
    let padded = format!("{range:<placeholder_width$}");
    bytes[range_at..range_at + placeholder_width].copy_from_slice(padded.as_bytes());

    let mut hasher = Sha256::new();
    hasher.update(&bytes[..contents_open]);
    hasher.update(&bytes[after_contents..]);
    let digest = hasher.finalize();

    let signature = build_signed_data(signer, &digest, signing_time)?;
    let hex: Vec<u8> = signature
        .iter()
        .flat_map(|byte| format!("{byte:02X}").into_bytes())
        .collect();
    if hex.len() > reserved * 2 {
        return Err(FormError::SigningFailed(
            "the signature is bigger than the room kept for it".into(),
        ));
    }
    bytes[contents_open + 1..contents_open + 1 + hex.len()].copy_from_slice(&hex);
    Ok(bytes)
}

fn find(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| from + position)
}

/// a pdf date string (D:YYYYMMDDHHmmSSZ) in utc.
pub(crate) fn pdf_date(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(time);
    format!("D:{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}Z")
}

/// a readable utc time for the visible signature, like 2026-11-11 14:05 UTC.
pub(crate) fn readable_date(time: SystemTime) -> String {
    let (year, month, day, hour, minute, _) = utc_parts(time);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// splits a moment into its utc calendar parts, using the well known
/// days-to-civil-date conversion (howard hinnant's algorithm).
pub(crate) fn utc_parts(time: SystemTime) -> (i64, u32, u32, u32, u32, u32) {
    let seconds = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let in_day = seconds.rem_euclid(86_400);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (
        year,
        month,
        day,
        (in_day / 3600) as u32,
        (in_day % 3600 / 60) as u32,
        (in_day % 60) as u32,
    )
}

/// draws what a visible signature shows: the picture, then who and when.
fn draw_visible_signature(
    doc: &mut Document,
    widget_id: ObjectId,
    request: &SignRequest,
    signer_name: &str,
    signing_time: SystemTime,
) -> FormResult<()> {
    let Some(widget) = WidgetBox::read(doc, widget_id) else {
        return Ok(());
    };
    if widget.width < 1.0 || widget.height < 1.0 {
        return Ok(());
    }
    let appearance = &request.appearance;
    let mut lines = Vec::new();
    if appearance.show_details {
        lines.push(format!("Digitally signed by {signer_name}"));
        lines.push(format!("Date: {}", readable_date(signing_time)));
        if let Some(reason) = request.reason.as_deref().filter(|text| !text.is_empty()) {
            lines.push(format!("Reason: {reason}"));
        }
        if let Some(location) = request.location.as_deref().filter(|text| !text.is_empty()) {
            lines.push(format!("Location: {location}"));
        }
    }

    let mut content = String::new();
    let mut resources = Dictionary::new();
    let mut text_left = 2.0;
    if let Some(png) = &appearance.image_png {
        let image = add_png_image(doc, png)?;
        let aspect = image_aspect(doc, image);
        let area_width = if lines.is_empty() {
            widget.width
        } else {
            widget.width * 0.45
        };
        let (draw_width, draw_height) = fit(aspect, area_width - 4.0, widget.height - 4.0);
        let x = 2.0 + (area_width - 4.0 - draw_width) / 2.0;
        let y = (widget.height - draw_height) / 2.0;
        content.push_str(&format!(
            "q {} 0 0 {} {} {} cm /Im Do Q\n",
            number(draw_width),
            number(draw_height),
            number(x),
            number(y)
        ));
        let mut xobjects = Dictionary::new();
        xobjects.set("Im", Object::Reference(image));
        resources.set("XObject", Object::Dictionary(xobjects));
        text_left = area_width + 2.0;
    }
    if !lines.is_empty() {
        let font = ensure_default_font(doc)?;
        let mut fonts = Dictionary::new();
        fonts.set("Helv", Object::Reference(font));
        resources.set("Font", Object::Dictionary(fonts));
        content.push_str(&detail_lines(&lines, text_left, &widget));
    }
    let mut dict = Dictionary::new();
    dict.set("Type", name("XObject"));
    dict.set("Subtype", name("Form"));
    dict.set("BBox", crate::objects::rect_object(&widget.bbox()));
    let m = widget.matrix;
    dict.set(
        "Matrix",
        Object::Array(
            [m.a, m.b, m.c, m.d, m.e, m.f]
                .iter()
                .map(|value| real(*value))
                .collect(),
        ),
    );
    dict.set("Resources", Object::Dictionary(resources));
    let stream = doc.add_object(Stream::new(dict, content.into_bytes()));
    let mut appearances = Dictionary::new();
    appearances.set("N", Object::Reference(stream));
    doc.get_dictionary_mut(widget_id)?
        .set("AP", Object::Dictionary(appearances));
    Ok(())
}

/// width over height of an image xobject.
fn image_aspect(doc: &Document, image: ObjectId) -> f32 {
    let size = doc
        .get_object(image)
        .ok()
        .and_then(|object| object.as_stream().ok())
        .map(|stream| {
            let get = |key: &[u8]| {
                stream
                    .dict
                    .get(key)
                    .ok()
                    .and_then(|value| value.as_i64().ok())
                    .unwrap_or(1) as f32
            };
            (get(b"Width"), get(b"Height"))
        });
    let (width, height) = size.unwrap_or((1.0, 1.0));
    width / height.max(1.0)
}

/// the largest size with this width-to-height ratio that fits the space.
fn fit(aspect: f32, width: f32, height: f32) -> (f32, f32) {
    let width = width.max(1.0);
    let height = height.max(1.0);
    if width / height > aspect {
        (height * aspect, height)
    } else {
        (width, width / aspect)
    }
}

/// the detail lines, as large as fits (up to 10 points). letters outside
/// the standard font's range show as "?", since this is only the picture
/// of the signature; the signature itself keeps the full name.
fn detail_lines(lines: &[String], left: f32, widget: &WidgetBox) -> String {
    let metrics = FontMetrics::standard(StandardFont::Helvetica);
    let encoded: Vec<Vec<u8>> = lines
        .iter()
        .map(|line| {
            line.chars()
                .map(|c| win_ansi_byte(c).unwrap_or(b'?'))
                .collect()
        })
        .collect();
    let width = (widget.width - left - 2.0).max(1.0);
    let widest = encoded
        .iter()
        .map(|line| metrics.text_width(line, 1.0))
        .fold(0.0, f32::max);
    let by_width = if widest > 0.0 { width / widest } else { 10.0 };
    let by_height = (widget.height - 4.0) / (lines.len() as f32 * 1.2);
    let size = by_width.min(by_height).clamp(2.0, 10.0);
    let leading = size * 1.2;
    let block = leading * lines.len() as f32;
    let mut y = (widget.height + block) / 2.0 - size;
    let mut out = format!("BT /Helv {} Tf 0 g\n", number(size));
    let mut previous = (0.0, 0.0);
    for line in &encoded {
        out.push_str(&format!(
            "{} {} Td {} Tj\n",
            number(left - previous.0),
            number(y - previous.1),
            literal_string(line)
        ));
        previous = (left, y);
        y -= leading;
    }
    out.push_str("ET\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn dates_are_written_in_utc() {
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_398_706);
        assert_eq!(pdf_date(time), "D:20261111120506Z");
        assert_eq!(
            readable_date(SystemTime::UNIX_EPOCH),
            "1970-01-01 00:00 UTC"
        );
        let leap_day = SystemTime::UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(pdf_date(leap_day), "D:20000229000000Z");
    }

    #[test]
    fn pictures_keep_their_shape() {
        assert_eq!(fit(2.0, 100.0, 100.0), (100.0, 50.0));
        assert_eq!(fit(0.5, 100.0, 100.0), (50.0, 100.0));
    }
}
