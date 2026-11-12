//! the fonts and default appearance (/DA) a field's drawing uses: reading
//! /DA, finding the font in the form's /DR, and adding helvetica when needed.

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::FormResult;
use crate::fonts::{metrics_of, FontMetrics, StandardFont};
use crate::geometry::number;
use crate::objects::{acroform_id, get_dict, get_name, name, number_of, resolve};

/// the resource name letters gives its own helvetica in the form's fonts.
pub const DEFAULT_FONT_NAME: &str = "Helv";

/// the parts of a /DA string ("/Helv 12 Tf 0 g") that matter here.
#[derive(Clone, Debug, PartialEq)]
pub struct DefaultAppearance {
    pub font: Option<String>,
    /// 0 means "fit the text to the field"
    pub size: f32,
    /// the colour operator that sets the fill colour, like "0 g"
    pub color: String,
}

impl DefaultAppearance {
    pub fn parse(da: &[u8]) -> Self {
        let mut parsed = Self {
            font: None,
            size: 0.0,
            color: "0 g".to_owned(),
        };
        let Ok(content) = Content::decode(da) else {
            return parsed;
        };
        for operation in content.operations {
            let numbers: Vec<String> = operation
                .operands
                .iter()
                .filter_map(number_of)
                .map(number)
                .collect();
            match operation.operator.as_str() {
                "Tf" => {
                    parsed.font = operation
                        .operands
                        .first()
                        .and_then(|operand| operand.as_name().ok())
                        .map(|font| String::from_utf8_lossy(font).into_owned());
                    parsed.size = operation
                        .operands
                        .get(1)
                        .and_then(number_of)
                        .unwrap_or(0.0)
                        .max(0.0);
                }
                "g" | "rg" | "k" if numbers.len() == operation.operands.len() => {
                    parsed.color = format!("{} {}", numbers.join(" "), operation.operator);
                }
                _ => {}
            }
        }
        parsed
    }

    /// the same colour as a stroke colour, for check marks drawn as lines.
    pub fn stroke_color(&self) -> String {
        stroke_version(&self.color)
    }
}

fn stroke_version(fill: &str) -> String {
    if let Some(start) = fill.strip_suffix(" rg") {
        format!("{start} RG")
    } else if let Some(start) = fill.strip_suffix(" k") {
        format!("{start} K")
    } else if let Some(start) = fill.strip_suffix(" g") {
        format!("{start} G")
    } else {
        "0 G".to_owned()
    }
}

/// a font ready to be used in an appearance.
#[derive(Clone, Debug)]
pub struct AppearanceFont {
    /// the name the drawing uses for it in its own resources
    pub resource_name: String,
    pub object: Object,
    pub metrics: FontMetrics,
}

/// finds the font a field's /DA asks for in the form's fonts (/DR). when
/// it is missing, or can't draw the text (fonts with two-byte codes, or a
/// font without winansi for non-ascii text), helvetica is used instead.
pub fn resolve_font(
    doc: &mut Document,
    wanted: Option<&str>,
    needs_win_ansi: bool,
) -> FormResult<AppearanceFont> {
    if let Some(wanted) = wanted {
        if let Some(font) = form_font(doc, wanted) {
            if font_can_draw(doc, &font, needs_win_ansi) {
                return Ok(AppearanceFont {
                    resource_name: wanted.to_owned(),
                    metrics: metrics_of(doc, &font),
                    object: font,
                });
            }
        }
    }
    let helvetica = ensure_default_font(doc)?;
    Ok(AppearanceFont {
        resource_name: DEFAULT_FONT_NAME.to_owned(),
        object: Object::Reference(helvetica),
        metrics: FontMetrics::standard(StandardFont::Helvetica),
    })
}

/// the font object called `wanted` in /AcroForm /DR /Font.
fn form_font(doc: &Document, wanted: &str) -> Option<Object> {
    let form = crate::objects::acroform(doc)?;
    let resources = get_dict(doc, form, b"DR")?;
    let fonts = get_dict(doc, resources, b"Font")?;
    fonts.get(wanted.as_bytes()).ok().cloned()
}

fn font_can_draw(doc: &Document, font: &Object, needs_win_ansi: bool) -> bool {
    let Ok(dict) = resolve(doc, font).as_dict() else {
        return false;
    };
    let simple = matches!(
        get_name(doc, dict, b"Subtype").as_deref(),
        Some(b"Type1") | Some(b"TrueType") | Some(b"MMType1")
    );
    let win_ansi = get_name(doc, dict, b"Encoding").as_deref() == Some(b"WinAnsiEncoding");
    simple && (win_ansi || !needs_win_ansi)
}

/// makes sure the form has letters' helvetica in its fonts and returns it.
/// a form whose /Helv is something else keeps it; ours is then added
/// under another name only inside each appearance.
pub fn ensure_default_font(doc: &mut Document) -> FormResult<ObjectId> {
    if let Some(Object::Reference(id)) = form_font(doc, DEFAULT_FONT_NAME) {
        if font_can_draw(doc, &Object::Reference(id), true) {
            return Ok(id);
        }
    }
    let font = doc.add_object(standard_font_dict(StandardFont::Helvetica));
    if let Some(form_id) = acroform_id(doc)? {
        let has_helv = form_font(doc, DEFAULT_FONT_NAME).is_some();
        if !has_helv {
            add_form_font(doc, form_id, DEFAULT_FONT_NAME, font)?;
        }
    }
    Ok(font)
}

/// a dictionary for one of the standard fonts in winansi encoding.
pub fn standard_font_dict(font: StandardFont) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", name("Font"));
    dict.set("Subtype", name("Type1"));
    dict.set("BaseFont", name(font.base_font_name()));
    dict.set("Encoding", name("WinAnsiEncoding"));
    dict
}

/// adds a font to /AcroForm /DR /Font, making the dictionaries on the way.
pub fn add_form_font(
    doc: &mut Document,
    form_id: ObjectId,
    font_name: &str,
    font: ObjectId,
) -> FormResult<()> {
    let form = doc.get_dictionary(form_id)?.clone();
    let mut resources = match form.get(b"DR").ok().map(|value| resolve(doc, value)) {
        Some(Object::Dictionary(dict)) => dict.clone(),
        _ => Dictionary::new(),
    };
    let mut fonts = match resources.get(b"Font").ok().map(|value| resolve(doc, value)) {
        Some(Object::Dictionary(dict)) => dict.clone(),
        _ => Dictionary::new(),
    };
    fonts.set(font_name.as_bytes().to_vec(), Object::Reference(font));
    resources.set("Font", Object::Dictionary(fonts));
    doc.get_dictionary_mut(form_id)?
        .set("DR", Object::Dictionary(resources));
    Ok(())
}
