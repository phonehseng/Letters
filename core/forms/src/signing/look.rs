//! what a visible signature shows: the optional picture, then "Digitally
//! signed by", the date, reason and location, sized to fit the box.

use std::time::SystemTime;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::dates::readable_date;
use super::sign::SignRequest;
use crate::appearance::{ensure_default_font, WidgetBox};
use crate::error::FormResult;
use crate::fonts::{FontMetrics, StandardFont};
use crate::geometry::number;
use crate::image::add_png_image;
use crate::objects::{name, real};
use crate::text::{literal_string, win_ansi_byte};

/// draws what a visible signature shows: the picture, then who and when.
pub(super) fn draw_visible_signature(
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

    #[test]
    fn pictures_keep_their_shape() {
        assert_eq!(fit(2.0, 100.0, 100.0), (100.0, 50.0));
        assert_eq!(fit(0.5, 100.0, 100.0), (50.0, 100.0));
    }
}
