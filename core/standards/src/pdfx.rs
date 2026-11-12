//! pdf/x-1a (iso 15930-4) and pdf/x-4 (iso 15930-7) pre-flight checks
//! for print shops. these only report; Letters doesn't convert to pdf/x,
//! because that needs colour conversion to the press's profile.

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

use crate::fonts::font_facts;
use crate::images::placed_images;
use crate::objects::{
    dict_of, get, get_array, get_dict, get_name, get_number, page_ids, rect_of, resolve,
};
use crate::report::{Finding, Findings, Fix};
use crate::walk::each_drawing;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfxLevel {
    /// pdf/x-1a: cmyk and spot colour only, no transparency, all fonts embedded
    #[serde(rename = "x1a")]
    X1a,
    /// pdf/x-4: colour managed, transparency allowed
    #[serde(rename = "x4")]
    X4,
}

impl PdfxLevel {
    fn clause(self, x1a: &str, x4: &str) -> String {
        match self {
            PdfxLevel::X1a => format!("ISO 15930-4 {x1a}"),
            PdfxLevel::X4 => format!("ISO 15930-7 {x4}"),
        }
    }
}

/// thresholds for picture resolution, in pixels per inch as placed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfxOptions {
    /// colour and gray pictures below this look soft in print
    #[serde(default = "default_colour_ppi")]
    pub min_colour_ppi: f64,
    /// one-bit (line art) pictures below this show jagged edges
    #[serde(default = "default_line_art_ppi")]
    pub min_line_art_ppi: f64,
}

/// common print shop minimums: 300 ppi for photos, 1200 for line art.
fn default_colour_ppi() -> f64 {
    300.0
}

fn default_line_art_ppi() -> f64 {
    1200.0
}

impl Default for PdfxOptions {
    fn default() -> Self {
        Self {
            min_colour_ppi: default_colour_ppi(),
            min_line_art_ppi: default_line_art_ppi(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfxReport {
    pub level: PdfxLevel,
    pub passes: bool,
    pub findings: Vec<Finding>,
    /// the spot colours used, by name, for the print shop
    pub spot_colours: Vec<String>,
    pub note: String,
}

const PDFX_NOTE: &str = "These are Letters’ own print checks; a print shop’s pre-flight tool has the final word. Letters doesn’t convert files to PDF/X.";

/// checks a document against a pdf/x level.
pub fn check_pdfx(doc: &Document, level: PdfxLevel, options: &PdfxOptions) -> PdfxReport {
    let mut findings = Findings::default();
    check_identification(doc, level, &mut findings);
    let intent_space = output_intent(doc, level, &mut findings);
    check_boxes(doc, level, &mut findings);
    check_fonts(doc, level, &mut findings);
    check_images(doc, options, &mut findings);
    let spot_colours = check_colours(doc, level, intent_space, &mut findings);
    if doc.trailer.has(b"Encrypt") {
        findings.add(
            "pdfx.encrypted",
            &level.clause("6.1", "6.1"),
            "The file is encrypted, which PDF/X forbids.".into(),
            Fix::NotPossible,
        );
    }
    let findings = findings.deduplicated();
    PdfxReport {
        level,
        passes: findings.is_empty(),
        findings,
        spot_colours,
        note: PDFX_NOTE.to_owned(),
    }
}

/// x-1a names its version in the info dictionary; x-4 in xmp
/// (pdfxid:GTS_PDFXVersion). both need /Trapped set to True or False.
fn check_identification(doc: &Document, level: PdfxLevel, findings: &mut Findings) {
    let info = doc
        .trailer
        .get(b"Info")
        .ok()
        .map(|value| resolve(doc, value))
        .and_then(|value| value.as_dict().ok());
    let trapped = info.and_then(|info| get_name(doc, info, b"Trapped"));
    if !matches!(trapped.as_deref(), Some(b"True") | Some(b"False")) {
        findings.add(
            "pdfx.trapped",
            &level.clause("6.7.3", "6.7.5"),
            "The document doesn’t say whether it was trapped (/Trapped True or False).".into(),
            Fix::Automatic,
        );
    }
    let named = match level {
        PdfxLevel::X1a => info
            .and_then(|info| get(doc, info, b"GTS_PDFXVersion"))
            .and_then(|value| value.as_str().ok())
            .is_some_and(|version| version.starts_with(b"PDF/X-1")),
        PdfxLevel::X4 => doc
            .catalog()
            .ok()
            .and_then(|catalog| get(doc, catalog, b"Metadata"))
            .and_then(|value| value.as_stream().ok())
            .is_some_and(|stream| {
                String::from_utf8_lossy(&crate::objects::stream_bytes(stream)).contains("PDF/X-4")
            }),
    };
    if !named {
        findings.add(
            "pdfx.version",
            &level.clause("6.7.4", "6.7.4"),
            "The document doesn’t identify itself as PDF/X.".into(),
            Fix::Automatic,
        );
    }
}

/// the pdf/x output intent (/S /GTS_PDFX) and its profile's colour space.
fn output_intent(doc: &Document, level: PdfxLevel, findings: &mut Findings) -> Option<[u8; 4]> {
    let catalog = doc.catalog().ok()?;
    let intent = get_array(doc, catalog, b"OutputIntents")
        .into_iter()
        .flatten()
        .filter_map(|intent| resolve(doc, intent).as_dict().ok())
        .find(|intent| get_name(doc, intent, b"S").as_deref() == Some(b"GTS_PDFX"));
    let Some(intent) = intent else {
        findings.add(
            "pdfx.outputIntent",
            &level.clause("6.2.2", "6.2.2"),
            "There is no PDF/X output intent naming the printing condition.".into(),
            Fix::NotPossible,
        );
        return None;
    };
    let profile = get(doc, intent, b"DestOutputProfile").and_then(|value| value.as_stream().ok());
    match profile {
        Some(stream) => {
            let bytes = crate::objects::stream_bytes(stream);
            (bytes.len() >= 20).then(|| bytes[16..20].try_into().unwrap_or(*b"    "))
        }
        None if level == PdfxLevel::X4 => {
            findings.add(
                "pdfx.outputProfile",
                "ISO 15930-7 6.2.2",
                "The output intent has no colour profile, which PDF/X-4 needs.".into(),
                Fix::NotPossible,
            );
            None
        }
        None => None,
    }
}

/// every page needs a trim (or art) box, inside the bleed box, inside the
/// media box (iso 15930-4 6.1.1).
fn check_boxes(doc: &Document, level: PdfxLevel, findings: &mut Findings) {
    for (index, page_id) in page_ids(doc).into_iter().enumerate() {
        let page = Some(index as u32);
        let Ok(dict) = doc.get_dictionary(page_id) else {
            continue;
        };
        let own = |key: &[u8]| get(doc, dict, key).and_then(|value| rect_of(doc, value));
        let media = crate::objects::inherited(doc, page_id, b"MediaBox")
            .and_then(|value| rect_of(doc, value));
        let trim = own(b"TrimBox").or_else(|| own(b"ArtBox"));
        let clause = level.clause("6.1.1", "6.1.1");
        let Some(trim) = trim else {
            findings
                .add(
                    "pdfx.trimBox",
                    &clause,
                    "The page has no trim box, so the print shop can’t tell where to cut.".into(),
                    Fix::NotPossible,
                )
                .on_page(page);
            continue;
        };
        let inside = |inner: [f64; 4], outer: [f64; 4]| {
            inner[0] >= outer[0] - 0.01
                && inner[1] >= outer[1] - 0.01
                && inner[2] <= outer[2] + 0.01
                && inner[3] <= outer[3] + 0.01
        };
        if let Some(bleed) = own(b"BleedBox") {
            if !inside(trim, bleed) {
                findings
                    .add(
                        "pdfx.bleedBox",
                        &clause,
                        "The trim box reaches outside the bleed box.".into(),
                        Fix::NotPossible,
                    )
                    .on_page(page);
            }
        }
        if media.is_some_and(|media| !inside(trim, media)) {
            findings
                .add(
                    "pdfx.mediaBox",
                    &clause,
                    "The trim box reaches outside the page.".into(),
                    Fix::NotPossible,
                )
                .on_page(page);
        }
    }
}

fn check_fonts(doc: &Document, level: PdfxLevel, findings: &mut Findings) {
    for object in doc.objects.values() {
        let Some(dict) = dict_of(object) else {
            continue;
        };
        if get_name(doc, dict, b"Type").as_deref() != Some(b"Font") {
            continue;
        }
        let facts = font_facts(doc, dict);
        let is_descendant = matches!(
            get_name(doc, dict, b"Subtype").as_deref(),
            Some(b"CIDFontType0") | Some(b"CIDFontType2")
        );
        if !facts.embedded && !facts.is_type3 && !is_descendant {
            findings.add(
                "pdfx.fontNotEmbedded",
                &level.clause("6.3.5", "6.3.5"),
                format!(
                    "The font {} isn’t embedded; print shops need every font embedded.",
                    facts.base_font
                ),
                Fix::NeedsFont,
            );
        }
    }
}

fn check_images(doc: &Document, options: &PdfxOptions, findings: &mut Findings) {
    for image in placed_images(doc) {
        let line_art = image.bits_per_component == 1;
        let minimum = if line_art {
            options.min_line_art_ppi
        } else {
            options.min_colour_ppi
        };
        if image.pixels_per_inch < minimum {
            findings
                .add(
                    "pdfx.resolution",
                    "Print shop practice",
                    format!(
                        "A picture is {:.0} pixels per inch as placed; print shops usually want at least {minimum:.0}.",
                        image.pixels_per_inch
                    ),
                    Fix::NotPossible,
                )
                .on_page(Some(image.page));
        }
    }
}

/// x-1a allows only cmyk, gray and spot colour, and no transparency;
/// x-4 allows rgb only through icc profiles or an rgb printing condition.
fn check_colours(
    doc: &Document,
    level: PdfxLevel,
    intent: Option<[u8; 4]>,
    findings: &mut Findings,
) -> Vec<String> {
    let mut spots: Vec<String> = Vec::new();
    for object in doc.objects.values() {
        let Some(dict) = dict_of(object) else {
            continue;
        };
        collect_spot_names(doc, dict, &mut spots);
        if level == PdfxLevel::X1a {
            let transparent = get(doc, dict, b"SMask")
                .is_some_and(|mask| mask.as_name().ok() != Some(b"None"))
                || [b"CA".as_slice(), b"ca"]
                    .iter()
                    .any(|key| get_number(doc, dict, key).is_some_and(|value| value < 1.0))
                || get_dict(doc, dict, b"Group")
                    .and_then(|group| get_name(doc, group, b"S"))
                    .is_some_and(|kind| kind == b"Transparency");
            if transparent {
                findings.add(
                    "pdfx.transparency",
                    "ISO 15930-4 6.3.2",
                    "The document uses transparency, which PDF/X-1a forbids.".into(),
                    Fix::NotPossible,
                );
            }
        }
    }
    let rgb_condition = intent.as_ref() == Some(b"RGB ");
    let mut rgb_used = false;
    each_drawing(doc, &mut |drawing| {
        rgb_used |= drawing.operations.iter().any(|operation| {
            matches!(operation.operator.as_str(), "rg" | "RG")
                || (matches!(operation.operator.as_str(), "cs" | "CS")
                    && operation.operands.first().and_then(|o| o.as_name().ok())
                        == Some(b"DeviceRGB"))
        });
    });
    rgb_used |= placed_images(doc).iter().any(|image| {
        image.color_space == "DeviceRGB"
            || image.color_space == "ICCBased" && level == PdfxLevel::X1a
    });
    let forbidden = match level {
        PdfxLevel::X1a => rgb_used,
        PdfxLevel::X4 => rgb_used && !rgb_condition,
    };
    if forbidden {
        findings.add(
            "pdfx.rgb",
            &level.clause("6.2.3", "6.2.3"),
            "The document uses RGB colour, which this PDF/X level doesn’t allow; the print shop would have to convert it.".into(),
            Fix::NotPossible,
        );
    }
    spots.sort();
    spots.dedup();
    spots
}

fn collect_spot_names(doc: &Document, dict: &lopdf::Dictionary, spots: &mut Vec<String>) {
    for (_, value) in dict.iter() {
        let Object::Array(items) = resolve(doc, value) else {
            continue;
        };
        if items.first().and_then(|first| first.as_name().ok()) == Some(b"Separation") {
            if let Some(name) = items.get(1).and_then(|name| name.as_name().ok()) {
                spots.push(String::from_utf8_lossy(name).into_owned());
            }
        }
    }
}
