//! pdf/a rules about what the pages draw: fonts (embedded, widths,
//! unicode for 2u), device colour without a matching output intent,
//! transparency in part 1, image flags, and how deeply graphics states nest.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use super::level::PdfaLevel;
use super::rules_document::OutputIntent;
use crate::fonts::{font_facts, FontFacts};
use crate::objects::{get, get_dict, get_name, get_number, names_in, resolve};
use crate::report::{Findings, Fix};
use crate::walk::{each_drawing, Drawing};

/// pdf 1.4's limit on q nesting, which part 1 keeps (19005-1 6.1.12).
const PART_ONE_MAX_NESTING: usize = 28;

/// which device colour spaces a page paints with.
#[derive(Default)]
struct DeviceColours {
    rgb: bool,
    cmyk: bool,
    gray: bool,
}

pub(super) fn check_drawings(
    doc: &Document,
    level: PdfaLevel,
    intent: OutputIntent,
    findings: &mut Findings,
) {
    let mut fonts_seen: HashSet<ObjectId> = HashSet::new();
    each_drawing(doc, &mut |drawing| {
        let page = Some(drawing.page);
        let mut colours = DeviceColours::default();
        let mut depth = 0usize;
        let mut deepest = 0usize;
        for operation in &drawing.operations {
            match operation.operator.as_str() {
                "q" => {
                    depth += 1;
                    deepest = deepest.max(depth);
                }
                "Q" => depth = depth.saturating_sub(1),
                "rg" | "RG" => colours.rgb = true,
                "k" | "K" => colours.cmyk = true,
                "g" | "G" => colours.gray = true,
                "cs" | "CS" => note_colour_space(drawing, operation.operands.first(), &mut colours),
                "Tf" => check_font_use(
                    drawing,
                    operation.operands.first(),
                    level,
                    &mut fonts_seen,
                    findings,
                ),
                "gs" if !level.allows_transparency() => {
                    check_graphics_state(drawing, operation.operands.first(), findings)
                }
                "Do" => check_xobject(
                    drawing,
                    operation.operands.first(),
                    level,
                    &mut colours,
                    findings,
                ),
                _ => {}
            }
        }
        if level.is_part_one() && deepest > PART_ONE_MAX_NESTING {
            findings
                .add(
                    "limits.nesting",
                    "ISO 19005-1 6.1.12",
                    "The page saves its graphics state more than 28 levels deep.".into(),
                    Fix::NotPossible,
                )
                .on_page(page)
                .at(drawing.object);
        }
        report_colours(&colours, intent, page, level, findings);
    });
}

fn resource<'a>(
    drawing: &'a Drawing,
    category: &[u8],
    name: Option<&Object>,
) -> Option<(Option<ObjectId>, &'a Object)> {
    let name = name?.as_name().ok()?;
    let entries = get_dict(drawing.doc, &drawing.resources, category)?;
    let value = entries.get(name).ok()?;
    Some((value.as_reference().ok(), resolve(drawing.doc, value)))
}

fn note_colour_space(drawing: &Drawing, name: Option<&Object>, colours: &mut DeviceColours) {
    let Some(name) = name.and_then(|name| name.as_name().ok()) else {
        return;
    };
    let space = match name {
        b"DeviceRGB" | b"DeviceCMYK" | b"DeviceGray" => name.to_vec(),
        _ => resource(drawing, b"ColorSpace", Some(&Object::Name(name.to_vec())))
            .and_then(|(_, value)| base_device_space(drawing.doc, value))
            .unwrap_or_default(),
    };
    mark(&space, colours);
}

/// the device space a colour space resolves to, if it is a device one.
fn base_device_space(doc: &Document, value: &Object) -> Option<Vec<u8>> {
    match resolve(doc, value) {
        Object::Name(name) => Some(name.clone()),
        Object::Array(items) => {
            let family = resolve(doc, items.first()?).as_name().ok()?;
            match family {
                // an indexed space's base (or a pattern's) decides the colour
                b"Indexed" | b"Pattern" => base_device_space(doc, items.get(1)?),
                _ => None,
            }
        }
        _ => None,
    }
}

fn mark(space: &[u8], colours: &mut DeviceColours) {
    match space {
        b"DeviceRGB" | b"RGB" => colours.rgb = true,
        b"DeviceCMYK" | b"CMYK" => colours.cmyk = true,
        b"DeviceGray" | b"G" => colours.gray = true,
        _ => {}
    }
}

/// device colour needs an output intent of the same kind (19005-1 6.2.3.3,
/// 19005-2 6.2.4.3); gray only needs one of any kind.
fn report_colours(
    colours: &DeviceColours,
    intent: OutputIntent,
    page: Option<u32>,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let clause = level.clause("6.2.3.3", "6.2.4.3");
    let rgb_intent = intent.present && &intent.space == b"RGB ";
    let cmyk_intent = intent.present && &intent.space == b"CMYK";
    if colours.rgb && !rgb_intent {
        let fix = if cmyk_intent {
            Fix::NotPossible
        } else {
            Fix::Automatic
        };
        findings
            .add(
                "color.deviceRgb",
                &clause,
                "The page uses RGB colour without an RGB output intent.".into(),
                fix,
            )
            .on_page(page);
    }
    if colours.cmyk && !cmyk_intent {
        findings
            .add(
                "color.deviceCmyk",
                &clause,
                "The page uses CMYK colour without a CMYK output intent; Letters has no CMYK profile to add.".into(),
                Fix::NotPossible,
            )
            .on_page(page);
    }
    if colours.gray && !intent.present {
        findings
            .add(
                "color.deviceGray",
                &clause,
                "The page uses gray without an output intent.".into(),
                Fix::Automatic,
            )
            .on_page(page);
    }
}

fn check_font_use(
    drawing: &Drawing,
    name: Option<&Object>,
    level: PdfaLevel,
    seen: &mut HashSet<ObjectId>,
    findings: &mut Findings,
) {
    let Some((id, font)) = resource(drawing, b"Font", name) else {
        return;
    };
    if let Some(id) = id {
        if !seen.insert(id) {
            return;
        }
    }
    let Ok(font) = font.as_dict() else { return };
    let facts = font_facts(drawing.doc, font);
    report_font(&facts, id, Some(drawing.page), level, findings);
}

fn report_font(
    facts: &FontFacts,
    id: Option<ObjectId>,
    page: Option<u32>,
    level: PdfaLevel,
    findings: &mut Findings,
) {
    let name = &facts.base_font;
    if !facts.embedded && !facts.is_type3 {
        findings
            .add(
                "fonts.notEmbedded",
                &level.clause("6.3.4", "6.2.11.4.1"),
                format!(
                    "The font {name} isn’t embedded, so the text may look different elsewhere."
                ),
                Fix::NeedsFont,
            )
            .on_page(page)
            .at(id);
    }
    if facts.is_simple && !facts.is_type3 && !facts.has_widths {
        let fix = if facts.embedded_truetype {
            Fix::Automatic
        } else {
            Fix::NeedsFont
        };
        findings
            .add(
                "fonts.widths",
                &level.clause("6.3.6", "6.2.11.5"),
                format!("The font {name} doesn’t list its glyph widths."),
                fix,
            )
            .on_page(page)
            .at(id);
    }
    if level.needs_unicode() && !facts.unicode_known {
        findings
            .add(
                "fonts.unicode",
                "ISO 19005-2 6.2.11.7",
                format!("The text in font {name} can’t be read out as Unicode (no ToUnicode map)."),
                if facts.is_simple {
                    Fix::Automatic
                } else {
                    Fix::NotPossible
                },
            )
            .on_page(page)
            .at(id);
    }
}

/// part 1 forbids soft masks, opacity below 1 and blend modes (19005-1 6.4).
fn check_graphics_state(drawing: &Drawing, name: Option<&Object>, findings: &mut Findings) {
    let Some((id, state)) = resource(drawing, b"ExtGState", name) else {
        return;
    };
    let Ok(state) = state.as_dict() else { return };
    let doc = drawing.doc;
    let soft_mask =
        get(doc, state, b"SMask").is_some_and(|mask| mask.as_name().ok() != Some(b"None"));
    let opacity = [b"CA".as_slice(), b"ca"]
        .iter()
        .any(|key| get_number(doc, state, key).is_some_and(|value| value < 1.0));
    let blend = names_in(doc, state.get(b"BM").ok())
        .iter()
        .any(|mode| mode != b"Normal" && mode != b"Compatible");
    if soft_mask || opacity || blend {
        findings
            .add(
                "transparency.state",
                "ISO 19005-1 6.4",
                "The page draws with transparency (opacity, a soft mask or a blend mode), which PDF/A-1 forbids.".into(),
                Fix::NeedsRendering,
            )
            .on_page(Some(drawing.page))
            .at(id);
    }
}

fn check_xobject(
    drawing: &Drawing,
    name: Option<&Object>,
    level: PdfaLevel,
    colours: &mut DeviceColours,
    findings: &mut Findings,
) {
    let Some((id, object)) = resource(drawing, b"XObject", name) else {
        return;
    };
    let Ok(stream) = object.as_stream() else {
        return;
    };
    let dict = &stream.dict;
    let doc = drawing.doc;
    let page = Some(drawing.page);
    match get_name(doc, dict, b"Subtype").as_deref() {
        Some(b"Image") => check_image(doc, dict, id, page, level, colours, findings),
        Some(b"Form") if !level.allows_transparency() => {
            let transparent_group = get_dict(doc, dict, b"Group")
                .and_then(|group| get_name(doc, group, b"S"))
                .is_some_and(|kind| kind == b"Transparency");
            if transparent_group {
                findings
                    .add(
                        "transparency.group",
                        "ISO 19005-1 6.4",
                        "A drawing on the page is a transparency group, which PDF/A-1 forbids."
                            .into(),
                        Fix::NeedsRendering,
                    )
                    .on_page(page)
                    .at(id);
            }
        }
        _ => {}
    }
}

fn check_image(
    doc: &Document,
    dict: &Dictionary,
    id: Option<ObjectId>,
    page: Option<u32>,
    level: PdfaLevel,
    colours: &mut DeviceColours,
    findings: &mut Findings,
) {
    if let Some(space) = dict
        .get(b"ColorSpace")
        .ok()
        .and_then(|value| base_device_space(doc, value))
    {
        mark(&space, colours);
    }
    if matches!(get(doc, dict, b"Interpolate"), Some(Object::Boolean(true))) {
        findings
            .add(
                "images.interpolate",
                &level.clause("6.2.4", "6.2.8"),
                "A picture asks readers to smooth it (Interpolate), which PDF/A forbids.".into(),
                Fix::Automatic,
            )
            .on_page(page)
            .at(id);
    }
    if dict.has(b"Alternates") {
        findings
            .add(
                "images.alternates",
                &level.clause("6.2.4", "6.2.8"),
                "A picture has alternative versions, which PDF/A forbids.".into(),
                Fix::Automatic,
            )
            .on_page(page)
            .at(id);
    }
    if !level.allows_transparency() && dict.has(b"SMask") {
        findings
            .add(
                "transparency.softMask",
                "ISO 19005-1 6.4",
                "A picture has a transparent background (a soft mask), which PDF/A-1 forbids."
                    .into(),
                Fix::NeedsRendering,
            )
            .on_page(page)
            .at(id);
    }
}
