//! every glyph of a line planned on word's print lattice, and the spacing
//! that puts it there (recipe 7.4 and 7.5).

use serde::Serialize;

use super::lines::Line;
use crate::fonts::FontPlan;
use crate::units::{round_half_even, PT_PER_PIXEL};
use crate::word::spacing::spacing_for_pixel_steps;
use crate::word::writer_model::{nearest_exported_pixel, StepTable};

/// one char of the docx: one run, so one text object in word's pdf.
#[derive(Clone, Debug, PartialEq)]
pub struct CharRun {
    pub char: String,
    pub family: String,
    pub half_points: i64,
    /// "RRGGBB"
    pub color: String,
    pub spacing_twips: i64,
    /// invisible text (render mode 3 or 7) stays invisible: `w:vanish`
    pub hidden: bool,
}

/// what a line asks of word, kept so the checking export can re-plan it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinePlan {
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub targets_px: Vec<i64>,
    pub advances_px: Vec<i64>,
    pub spacing_twips: Vec<i64>,
    /// horizontal lines: the layout pixel of the line start
    pub start_pixel: Option<i64>,
    /// horizontal lines: the pdf y word prints for the baseline
    pub word_y: Option<f64>,
    pub rotated: bool,
    /// per char, the index of the line's glyph it was written for (none:
    /// a virtual space); the fitted text path places pieces from them
    #[serde(skip)]
    pub sources: Vec<Option<usize>>,
}

/// how a line's glyph targets are found.
pub enum Targets<'a> {
    /// horizontal: the pixel whose printed x is nearest each source x,
    /// counted from the line's start pixel
    Lattice {
        table: &'a StepTable,
        start_pixel: i64,
        word_y: f64,
    },
    /// rotated: the nearest pixel along the baseline from where word
    /// starts the line (`offset_pt` from the source start)
    Along { offset_pt: f64 },
}

/// counts for the report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphCounts {
    pub glyphs_without_char: usize,
    pub horizontally_scaled_glyphs: usize,
}

/// a colour as six hex digits.
pub fn hex_color(rgb: [f64; 3]) -> String {
    rgb.iter()
        .map(|c| format!("{:02X}", round_half_even(c * 255.0).clamp(0.0, 255.0) as u8))
        .collect()
}

struct Item {
    /// the line's glyph this char stands for; none for a virtual space
    source: Option<usize>,
    char: String,
    family: String,
    half_points: i64,
    advance_px: i64,
    color: [f64; 3],
    hidden: bool,
    along: f64,
}

/// plans a line's runs; `None` when no glyph of it can be written.
pub fn plan_line(
    line: &Line,
    fonts: &mut FontPlan,
    targets: Targets,
    counts: &mut GlyphCounts,
) -> Result<Option<(LinePlan, Vec<CharRun>)>, String> {
    let mut items: Vec<Item> = Vec::new();
    let text_glyphs: Vec<usize> = (0..line.glyphs.len())
        .filter(|&i| line.glyphs[i].text != " ")
        .collect();
    for (index, glyph) in line.glyphs.iter().enumerate() {
        let along = line.along(glyph.x, glyph.y);
        // a space takes the font of the text before it (or after it at the
        // line start): word pdfs draw list tabs as spaces in a font with no
        // other glyph, and word did not honour a docx font that holds
        // nothing but a space (recipe 7.5)
        let font_index = if glyph.text == " " {
            text_glyphs
                .iter()
                .rev()
                .find(|&&i| i < index)
                .or(text_glyphs.first())
                .copied()
                .unwrap_or(index)
        } else {
            index
        };
        let grid = fonts.grid_font(&line.glyphs[font_index])?;
        let (family, half_points) = (
            grid.family.clone(),
            round_half_even(grid.stored_size * 2.0) as i64,
        );
        let space_px = grid.pixels(' ');
        if line.virtual_spaces.contains(&index) {
            let previous = &line.glyphs[index - 1];
            items.push(Item {
                source: None,
                char: " ".into(),
                family: family.clone(),
                half_points,
                advance_px: space_px,
                color: glyph.color,
                hidden: glyph.invisible(),
                along: line.along(previous.x, previous.y) + previous.advance,
            });
        }
        let char = fonts.char_of(glyph);
        if char.is_empty() {
            counts.glyphs_without_char += 1;
            continue;
        }
        if (glyph.horizontal_ratio - 1.0).abs() > 0.005 {
            counts.horizontally_scaled_glyphs += 1;
        }
        let grid = fonts.grid_font(&line.glyphs[font_index])?;
        let advance_px = grid.pixels(char.chars().next().unwrap_or(' '));
        items.push(Item {
            source: Some(index),
            char,
            family,
            half_points,
            advance_px,
            color: glyph.color,
            hidden: glyph.invisible(),
            along,
        });
    }
    if items.is_empty() {
        return Ok(None);
    }
    let (mut target_px, start_pixel, word_y): (Vec<i64>, Option<i64>, Option<f64>) = match targets {
        Targets::Lattice {
            table,
            start_pixel,
            word_y,
        } => (
            items
                .iter()
                .map(|item| nearest_exported_pixel(line.x() + item.along, table) - start_pixel)
                .collect(),
            Some(start_pixel),
            Some(word_y),
        ),
        Targets::Along { offset_pt } => (
            items
                .iter()
                .map(|item| round_half_even((item.along - offset_pt) / PT_PER_PIXEL) as i64)
                .collect(),
            None,
            None,
        ),
    };
    // the first glyph is the line start word produced
    target_px[0] = 0;
    let advances_px: Vec<i64> = items.iter().map(|item| item.advance_px).collect();
    let spacing = spacing_for_pixel_steps(&advances_px, &target_px).map_err(|e| e.to_string())?;
    let runs = items
        .iter()
        .zip(&spacing)
        .map(|(item, &twips)| CharRun {
            char: item.char.clone(),
            family: item.family.clone(),
            half_points: item.half_points,
            color: hex_color(item.color),
            spacing_twips: twips,
            hidden: item.hidden,
        })
        .collect();
    let plan = LinePlan {
        page: line.page,
        x: line.x(),
        y: line.y(),
        text: items.iter().map(|item| item.char.as_str()).collect(),
        targets_px: target_px,
        advances_px,
        spacing_twips: spacing,
        start_pixel,
        word_y,
        rotated: start_pixel.is_none(),
        sources: items.iter().map(|item| item.source).collect(),
    };
    Ok(Some((plan, runs)))
}

/// the same runs with new spacing (the checking export re-plans lines).
pub fn with_spacing(runs: &[CharRun], spacing: &[i64]) -> Vec<CharRun> {
    runs.iter()
        .zip(spacing)
        .map(|(run, &twips)| CharRun {
            spacing_twips: twips,
            ..run.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_as_hex() {
        // the prototype's case (test_prototype.py)
        assert_eq!(hex_color([1.0, 0.5, 0.0]), "FF8000");
        assert_eq!(hex_color([0.0, 0.0, 0.0]), "000000");
    }
}
