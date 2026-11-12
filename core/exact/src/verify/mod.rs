//! verification: export the docx through word, read the glyph positions of
//! both pdfs with the same interpreter and report how close they are (the
//! same numbers as the prototype's harness), plus a pixel comparison when
//! the caller can render pages.

pub mod matching;
pub mod pixels;
pub mod raw;

use std::path::Path;

use serde::Serialize;

use crate::calibration::model::{is_landscape, Axis, WordModel};
use crate::pdf::{PageContent, PdfFile};
use crate::plan::lines::ANGLE_TOLERANCE_DEG;
use crate::plan::raster::RenderFn;
use crate::word::writer_model::{lattice_error, nearest_exported_y, page_pixels};
use matching::matched_pairs;
use pixels::{compare_pages, Gray, PageComparison};

/// the agreed target: every glyph within 0.125 pt, mean under 0.05 pt.
pub const THRESHOLD_MAX_PT: f64 = 0.125;
pub const THRESHOLD_MEAN_PT: f64 = 0.05;

/// one visible char and its pen origin.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub page: usize,
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub font: String,
}

/// every glyph with text that isn't white space (the harness's reading).
pub fn placed_glyphs(pages: &[PageContent]) -> Vec<PlacedGlyph> {
    pages
        .iter()
        .flat_map(|page| page.glyphs.iter())
        .filter(|g| !g.text.is_empty() && !g.text.chars().all(char::is_whitespace))
        .map(|g| PlacedGlyph {
            page: g.page,
            text: g.text.clone(),
            x: g.x,
            y: g.y,
            font: g.font.base_name.clone(),
        })
        .collect()
}

/// position statistics of matched glyphs on the same page.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphStats {
    pub source_glyphs: usize,
    pub word_glyphs: usize,
    pub matched: usize,
    pub matched_on_wrong_page: usize,
    /// matched glyphs on the same page: the ones measured
    pub glyphs: usize,
    pub max_pt: f64,
    pub mean_pt: f64,
    pub p99_pt: f64,
    pub within_0_025: f64,
    pub within_0_06: f64,
    pub within_0_125: f64,
    pub dx_max_pt: f64,
    pub dy_max_pt: f64,
    pub dx_mean_pt: f64,
    pub dy_mean_pt: f64,
}

/// the value below which `fraction` of the sorted values lie (numpy's linear rule).
fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = fraction * (sorted.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = (low + 1).min(sorted.len() - 1);
    sorted[low] + (sorted[high] - sorted[low]) * (rank - low as f64)
}

/// pairs the glyphs in reading order and measures the distances (combined x and y).
pub fn compare_glyphs(source: &[PlacedGlyph], word: &[PlacedGlyph]) -> GlyphStats {
    let a: Vec<&str> = source.iter().map(|g| g.text.as_str()).collect();
    let b: Vec<&str> = word.iter().map(|g| g.text.as_str()).collect();
    let pairs = matched_pairs(&a, &b);
    let same_page: Vec<(&PlacedGlyph, &PlacedGlyph)> = pairs
        .iter()
        .map(|&(i, j)| (&source[i], &word[j]))
        .filter(|(s, w)| s.page == w.page)
        .collect();
    let dx: Vec<f64> = same_page.iter().map(|(s, w)| w.x - s.x).collect();
    let dy: Vec<f64> = same_page.iter().map(|(s, w)| w.y - s.y).collect();
    let mut distance: Vec<f64> = dx.iter().zip(&dy).map(|(x, y)| x.hypot(*y)).collect();
    distance.sort_by(f64::total_cmp);
    let count = distance.len().max(1) as f64;
    let within = |limit: f64| distance.iter().filter(|&&d| d <= limit).count() as f64 / count;
    GlyphStats {
        source_glyphs: source.len(),
        word_glyphs: word.len(),
        matched: pairs.len(),
        matched_on_wrong_page: pairs.len() - same_page.len(),
        glyphs: same_page.len(),
        max_pt: distance.last().copied().unwrap_or(0.0),
        mean_pt: distance.iter().sum::<f64>() / count,
        p99_pt: percentile(&distance, 0.99),
        within_0_025: within(0.025),
        within_0_06: within(0.06),
        within_0_125: within(0.125),
        dx_max_pt: dx.iter().map(|v| v.abs()).fold(0.0, f64::max),
        dy_max_pt: dy.iter().map(|v| v.abs()).fold(0.0, f64::max),
        dx_mean_pt: dx.iter().map(|v| v.abs()).sum::<f64>() / count,
        dy_mean_pt: dy.iter().map(|v| v.abs()).sum::<f64>() / count,
    }
}

/// the best any plan through word can do: each source glyph's distance to
/// the nearest position word can print (recipe 11, "grid floor").
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorReport {
    pub glyphs: usize,
    pub max_pt: f64,
    pub mean_pt: f64,
    pub x_mean_pt: f64,
    pub y_mean_pt: f64,
}

pub fn lattice_floor(pages: &[PageContent], model: &mut WordModel) -> FloorReport {
    let (mut d, mut dxs, mut dys) = (Vec::new(), Vec::new(), Vec::new());
    for page in pages {
        let (width_px, height_px) = (page_pixels(page.width), page_pixels(page.height));
        let landscape = is_landscape(page.width, page.height);
        let x_table = model.table(Axis::X, width_px, landscape);
        let y_table = model.table(Axis::Y, height_px, landscape);
        for glyph in &page.glyphs {
            if glyph.text.trim().is_empty()
                || glyph.angle.abs() > ANGLE_TOLERANCE_DEG
                || glyph.invisible()
            {
                continue;
            }
            let dx = lattice_error(glyph.x, &x_table);
            let dy = (nearest_exported_y(glyph.y, &y_table, height_px) - glyph.y).abs();
            d.push(dx.hypot(dy));
            dxs.push(dx);
            dys.push(dy);
        }
    }
    let count = d.len().max(1) as f64;
    FloorReport {
        glyphs: d.len(),
        max_pt: d.iter().copied().fold(0.0, f64::max),
        mean_pt: d.iter().sum::<f64>() / count,
        x_mean_pt: dxs.iter().sum::<f64>() / count,
        y_mean_pt: dys.iter().sum::<f64>() / count,
    }
}

/// the verdict of one verification.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub glyphs: GlyphStats,
    pub floor: FloorReport,
    /// fonts that draw text in word's pdf
    pub fonts_in_word_pdf: Vec<String>,
    /// fonts word used that aren't the embedded ones (it renames those ___WRD_EMBED_SUB_nn)
    pub substituted_by_word: Vec<String>,
    /// faces the converter had to give stand-ins
    pub substituted_by_converter: Vec<String>,
    pub positions_pass: bool,
    pub fonts_pass: bool,
    pub pass: bool,
    /// per page, when pages could be rendered
    pub pages: Vec<PageComparison>,
    pub word_export_seconds: f64,
    /// one plain sentence for the person
    pub summary: String,
}

/// fonts that draw visible text in a pdf, without subset prefixes.
fn fonts_drawing_text(pages: &[PageContent]) -> Vec<String> {
    let mut fonts: Vec<String> = Vec::new();
    for glyph in pages.iter().flat_map(|p| p.glyphs.iter()) {
        if glyph.text.trim().is_empty()
            || glyph.invisible()
            || fonts.contains(&glyph.font.base_name)
        {
            continue;
        }
        fonts.push(glyph.font.base_name.clone());
    }
    fonts
}

/// measures a pdf word exported against its source.
pub fn measure(
    source_pages: &[PageContent],
    word_pdf: &Path,
    model: &mut WordModel,
    substituted_by_converter: Vec<String>,
    render: Option<&RenderFn>,
    source_pdf_bytes: Option<&[u8]>,
) -> Result<VerificationReport, crate::ExactError> {
    let word_file = PdfFile::open(word_pdf)?;
    let word_pages = word_file.read_pages();
    let glyphs = compare_glyphs(&placed_glyphs(source_pages), &placed_glyphs(&word_pages));
    let fonts_in_word_pdf = fonts_drawing_text(&word_pages);
    let substituted_by_word: Vec<String> = fonts_in_word_pdf
        .iter()
        .filter(|f| !f.contains("WRD_EMBED"))
        .cloned()
        .collect();
    let positions_pass = glyphs.glyphs > 0
        && glyphs.max_pt <= THRESHOLD_MAX_PT
        && glyphs.mean_pt < THRESHOLD_MEAN_PT;
    let fonts_pass = substituted_by_converter.is_empty() && substituted_by_word.is_empty();
    let mut pages = Vec::new();
    if let (Some(render), Some(source_bytes)) = (render, source_pdf_bytes) {
        let word_bytes =
            std::fs::read(word_pdf).map_err(|e| crate::ExactError::CannotOpen(format!("({e})")))?;
        pages = compare_renderings(source_bytes, &word_bytes, source_pages.len(), render);
    }
    let floor = lattice_floor(source_pages, model);
    let summary = summary_sentence(
        &glyphs,
        positions_pass,
        fonts_pass,
        &substituted_by_converter,
        &substituted_by_word,
    );
    Ok(VerificationReport {
        glyphs,
        floor,
        fonts_in_word_pdf,
        substituted_by_word,
        substituted_by_converter,
        positions_pass,
        fonts_pass,
        pass: positions_pass && fonts_pass,
        pages,
        word_export_seconds: 0.0,
        summary,
    })
}

/// replaces the glyph numbers of a verification with the fitted path's,
/// where every piece's glyph was read as its components (`fitted::measure`),
/// and says again whether it passes.
pub fn use_fitted_glyphs(report: &mut VerificationReport, glyphs: GlyphStats) {
    report.positions_pass = glyphs.glyphs > 0
        && glyphs.max_pt <= THRESHOLD_MAX_PT
        && glyphs.mean_pt < THRESHOLD_MEAN_PT;
    report.pass = report.positions_pass && report.fonts_pass;
    report.summary = summary_sentence(
        &glyphs,
        report.positions_pass,
        report.fonts_pass,
        &report.substituted_by_converter,
        &report.substituted_by_word,
    );
    report.glyphs = glyphs;
}

/// a page of a pdf drawn by the caller: the render callback draws page 0,
/// so each page is cut out into its own one-page pdf first.
fn compare_renderings(
    source: &[u8],
    word: &[u8],
    page_count: usize,
    render: &RenderFn,
) -> Vec<PageComparison> {
    const DPI: f32 = 150.0;
    let mut pages = Vec::new();
    for page in 0..page_count {
        let (Some(a), Some(b)) = (single_page(source, page), single_page(word, page)) else {
            continue;
        };
        let (Ok(a), Ok(b)) = (render(&a, DPI), render(&b, DPI)) else {
            continue;
        };
        let (a, b) = (
            Gray::from_rgba(a.width, a.height, &a.rgba),
            Gray::from_rgba(b.width, b.height, &b.rgba),
        );
        pages.push(compare_pages(page, &a, &b));
    }
    pages
}

/// a copy of a pdf holding only one of its pages.
pub fn single_page(bytes: &[u8], page: usize) -> Option<Vec<u8>> {
    let mut document = lopdf::Document::load_mem(bytes).ok()?;
    let numbers: Vec<u32> = document.get_pages().keys().copied().collect();
    let keep = *numbers.get(page)?;
    let others: Vec<u32> = numbers.into_iter().filter(|&n| n != keep).collect();
    document.delete_pages(&others);
    let mut out = Vec::new();
    document.save_to(&mut out).ok()?;
    Some(out)
}

fn summary_sentence(
    glyphs: &GlyphStats,
    positions: bool,
    fonts: bool,
    converter: &[String],
    word: &[String],
) -> String {
    let mut text = if positions {
        format!(
            "Checked in Word: every glyph is within {:.3} pt of the PDF (on average {:.3} pt), inside the 0.125 pt target.",
            glyphs.max_pt, glyphs.mean_pt
        )
    } else if glyphs.glyphs == 0 {
        "Checked in Word, but no glyph could be matched with the PDF.".to_string()
    } else {
        format!(
            "Checked in Word: {:.1}% of glyphs are within 0.125 pt of the PDF; the furthest is {:.3} pt off (on average {:.3} pt), so the 0.125 pt target is missed.",
            100.0 * glyphs.within_0_125,
            glyphs.max_pt,
            glyphs.mean_pt
        )
    };
    if !fonts {
        let mut names: Vec<String> = converter.to_vec();
        names.extend(word.iter().map(|w| format!("{w} (chosen by Word)")));
        text.push_str(&format!(
            " Not every font is the original: {}.",
            names.join(", ")
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(text: &str, x: f64, y: f64) -> PlacedGlyph {
        PlacedGlyph {
            page: 0,
            text: text.into(),
            x,
            y,
            font: "f".into(),
        }
    }

    #[test]
    fn percentiles_like_numpy() {
        let values: Vec<f64> = (0..=100).map(f64::from).collect();
        assert!((percentile(&values, 0.99) - 99.0).abs() < 1e-12);
        assert!((percentile(&[1.0, 2.0], 0.5) - 1.5).abs() < 1e-12);
    }

    #[test]
    fn distances_combine_x_and_y() {
        let source = [
            placed("a", 10.0, 10.0),
            placed("b", 20.0, 10.0),
            placed("c", 30.0, 10.0),
        ];
        let word = [placed("a", 10.03, 10.04), placed("c", 30.0, 10.0)];
        let stats = compare_glyphs(&source, &word);
        assert_eq!(stats.matched, 2);
        assert!((stats.max_pt - 0.05).abs() < 1e-9);
        assert!((stats.mean_pt - 0.025).abs() < 1e-9);
        assert_eq!(stats.within_0_06, 1.0);
        assert_eq!(stats.within_0_025, 0.5);
    }
}
