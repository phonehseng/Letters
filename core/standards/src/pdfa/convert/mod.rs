//! converting a document to a pdf/a level: fixing everything that can be
//! fixed, listing what was removed or changed, and checking again. when
//! something can't be made compliant the report says what and why, and
//! `compliant` stays false; nothing is claimed that the checker didn't see.

mod appearances;
mod cleanup;
mod fonts;
mod icc;
mod metadata;
mod raster;

use std::time::SystemTime;

use lopdf::Document;
use serde::{Deserialize, Serialize};

pub use fonts::{FontFile, FontOutcome, FontRequest, FontSource};
pub use icc::srgb_profile;
pub use raster::{PageRenderer, RasterisedArea, RenderedPage};

use super::level::PdfaLevel;
use super::{check_pdfa, PdfaReport};
use crate::error::{StandardsError, StandardsResult};
use crate::report::{Finding, Fix};
use cleanup::Removals;

/// the default resolution for areas turned into pictures: 300 pixels per
/// inch, what print shops ask for.
const DEFAULT_PIXELS_PER_POINT: f32 = 300.0 / 72.0;

/// choices for a conversion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionOptions {
    /// pdf/a-1 only: turn transparent areas into pictures (with a renderer)
    #[serde(default = "yes")]
    pub rasterise_transparency: bool,
    /// the resolution of those pictures
    #[serde(default = "default_resolution")]
    pub pixels_per_point: f32,
}

fn yes() -> bool {
    true
}

fn default_resolution() -> f32 {
    DEFAULT_PIXELS_PER_POINT
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            rasterise_transparency: true,
            pixels_per_point: DEFAULT_PIXELS_PER_POINT,
        }
    }
}

/// what the caller can lend a conversion: fonts to embed and a renderer
/// for pdf/a-1 transparency. either may be missing; then those findings
/// stay as blockers.
#[derive(Default)]
pub struct ConversionHelpers<'a> {
    pub fonts: Option<&'a dyn FontSource>,
    pub renderer: Option<&'a dyn PageRenderer>,
}

/// what a conversion did, and the check afterwards.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionReport {
    pub level: PdfaLevel,
    /// the checker found nothing left (see `CHECKER_NOTE`)
    pub compliant: bool,
    /// every removal or change, one sentence each
    pub changes: Vec<String>,
    pub fonts: Vec<FontOutcome>,
    pub rasterised: Vec<RasterisedArea>,
    pub output_intent_added: bool,
    /// what is still wrong, with why it couldn't be fixed
    pub blockers: Vec<Finding>,
    pub before: PdfaReport,
    pub after: PdfaReport,
    /// one or two sentences for the interface
    pub summary: String,
}

/// converts a document in place. save it with `save_pdfa`.
pub fn convert_to_pdfa(
    doc: &mut Document,
    level: PdfaLevel,
    options: &ConversionOptions,
    helpers: &ConversionHelpers,
) -> StandardsResult<ConversionReport> {
    if doc.is_encrypted() || doc.trailer.has(b"Encrypt") {
        return Err(StandardsError::Encrypted);
    }
    let before = check_pdfa(doc, level);
    let mut removals = Removals::default();
    cleanup::remove_forbidden(doc, level, &mut removals);
    if level.part() == 3 {
        cleanup::describe_attachments(doc, &mut removals);
    }
    let fonts = embed_fonts(doc, &before, helpers.fonts);
    if !level.is_part_one() {
        let not_drawn = appearances::add_missing_appearances(doc, false, &mut removals);
        for kind in not_drawn {
            removals.note(&format!(
                "{kind} annotations have no drawing Letters can make."
            ));
        }
    }
    let output_intent_added = metadata::ensure_output_intent(doc)?;
    let rasterised = handle_transparency(
        doc,
        level,
        &before,
        options,
        helpers.renderer,
        &mut removals,
    );
    metadata::write_metadata(doc, level, &pdf_date_now())?;
    removals.note(&format!(
        "The XMP metadata was written again: it names the file {} and repeats the document’s information.",
        level.name()
    ));
    if metadata::ensure_file_id(doc) {
        removals.note("A file identifier was added.");
    }
    doc.version = if level.is_part_one() { "1.4" } else { "1.7" }.to_owned();
    // what nothing refers to any more goes, as save_pdfa would drop it
    doc.prune_objects();
    let after = check_pdfa(doc, level);
    let blockers = after.findings.clone();
    Ok(ConversionReport {
        level,
        compliant: blockers.is_empty(),
        summary: summary(level, &blockers),
        changes: removals.list,
        fonts,
        rasterised,
        output_intent_added,
        blockers,
        before,
        after,
    })
}

fn embed_fonts(
    doc: &mut Document,
    before: &PdfaReport,
    source: Option<&dyn FontSource>,
) -> Vec<FontOutcome> {
    let needs_fonts = before
        .findings
        .iter()
        .any(|finding| finding.fix == Fix::NeedsFont);
    if !needs_fonts {
        return Vec::new();
    }
    fonts::promote_direct_fonts(doc);
    let ids = fonts::fonts_to_embed(doc);
    ids.into_iter()
        .map(|id| match source {
            Some(source) => fonts::embed(doc, id, source),
            None => FontOutcome {
                font: doc
                    .get_dictionary(id)
                    .ok()
                    .and_then(|font| font.get(b"BaseFont").ok())
                    .and_then(|name| name.as_name().ok())
                    .map(|name| String::from_utf8_lossy(name).into_owned())
                    .unwrap_or_default(),
                embedded_as: None,
                widths_differ: false,
                problem: Some("No fonts were offered to embed.".into()),
            },
        })
        .collect()
}

/// pdf/a-1: rasterises pages with transparency (when allowed and a
/// renderer is given) and drops page groups from the others.
fn handle_transparency(
    doc: &mut Document,
    level: PdfaLevel,
    before: &PdfaReport,
    options: &ConversionOptions,
    renderer: Option<&dyn PageRenderer>,
    removals: &mut Removals,
) -> Vec<RasterisedArea> {
    if !level.is_part_one() {
        return Vec::new();
    }
    let mut pages: Vec<u32> = before
        .findings
        .iter()
        .filter(|finding| finding.fix == Fix::NeedsRendering)
        .filter_map(|finding| finding.page)
        .collect();
    pages.dedup();
    cleanup::drop_page_groups(doc, &pages, removals);
    let (true, Some(renderer)) = (options.rasterise_transparency, renderer) else {
        return Vec::new();
    };
    let (areas, failed) = raster::rasterise(doc, &pages, renderer, options.pixels_per_point);
    for page in failed {
        removals.note(&format!(
            "Page {} couldn’t be turned into a picture (it is turned, or couldn’t be drawn).",
            page + 1
        ));
    }
    for area in &areas {
        let what = if area.whole_page {
            "The whole page"
        } else {
            "An area"
        };
        removals.note(&format!(
            "{what} on page {} with transparency was turned into a picture of how it looks.",
            area.page + 1
        ));
    }
    areas
}

fn summary(level: PdfaLevel, blockers: &[Finding]) -> String {
    if blockers.is_empty() {
        return format!(
            "The document now meets {} by Letters’ own checks. Confirm with an independent validator before relying on it.",
            level.name()
        );
    }
    let first = &blockers[0].message;
    format!(
        "The document can’t be made {} yet: {} problem(s) remain, such as: {first}",
        level.name(),
        blockers.len()
    )
}

/// now as a pdf date in utc (D:YYYYMMDDHHmmSSZ).
fn pdf_date_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    pdf_date(seconds)
}

/// days to a civil date by howard hinnant's algorithm.
pub(crate) fn pdf_date(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let in_day = seconds.rem_euclid(86_400);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    format!(
        "D:{year:04}{month:02}{day:02}{:02}{:02}{:02}Z",
        in_day / 3600,
        in_day % 3600 / 60,
        in_day % 60
    )
}

/// saves a converted document the way pdf/a needs: no unused objects, a classic
/// cross-reference table (no object or cross-reference streams, which
/// pdf/a-1 forbids), a binary comment after the header, and %%EOF last.
pub fn save_pdfa(doc: &mut Document) -> StandardsResult<Vec<u8>> {
    // objects nothing refers to any more (removed attachments, replaced
    // fonts) would still be in the file, and validators read every object
    doc.prune_objects();
    doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
    doc.binary_mark = vec![0xE2, 0xE3, 0xCF, 0xD3];
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|error| StandardsError::SaveFailed(error.to_string()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc() {
        assert_eq!(pdf_date(1_794_475_800), "D:20261112093000Z");
        assert_eq!(pdf_date(0), "D:19700101000000Z");
    }
}
