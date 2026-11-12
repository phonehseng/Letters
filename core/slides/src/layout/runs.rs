//! a paragraph's runs turned into clusters: one character each, with the
//! font that draws it, its advance and where a line may break after it.

use unicode_linebreak::{linebreaks, BreakOpportunity};

use crate::fonts::{FontId, FontLibrary};

/// one run as the layout needs it: text and resolved formatting, in points.
#[derive(Clone, Debug)]
pub struct StyledRun {
    pub text: String,
    /// latin, east asian and complex script families, theme references
    /// already resolved
    pub family: String,
    pub east_asian_family: String,
    pub complex_family: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    /// extra space after each character
    pub spacing: f64,
    /// raised (positive) or lowered, as a fraction of the size
    pub baseline_shift: f64,
    pub all_caps: bool,
    /// a line break (`a:br`), not text
    pub is_break: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakAfter {
    Never,
    Allowed,
    Mandatory,
}

#[derive(Clone, Debug)]
pub struct Cluster {
    pub ch: char,
    pub font: FontId,
    pub glyph: u16,
    pub advance: f64,
    pub size: f64,
    /// the run it came from
    pub run: usize,
    pub is_space: bool,
    pub break_after: BreakAfter,
}

/// the script a character is drawn with: east asian characters take the
/// run's east asian font and right-to-left letters its complex font
/// (powerpoint chooses fonts per character the same way).
fn family_for(run: &StyledRun, ch: char) -> &str {
    let code = ch as u32;
    let east_asian = (0x2E80..=0x9FFF).contains(&code)
        || (0xAC00..=0xD7AF).contains(&code)
        || (0xFF00..=0xFFEF).contains(&code);
    let complex = (0x0590..=0x08FF).contains(&code) || (0x0E00..=0x0E7F).contains(&code);
    if east_asian && !run.east_asian_family.is_empty() {
        &run.east_asian_family
    } else if complex && !run.complex_family.is_empty() {
        &run.complex_family
    } else {
        &run.family
    }
}

/// the clusters of a paragraph, with break opportunities from unicode's
/// line breaking rules (uax 14).
pub fn clusters(runs: &[StyledRun], fonts: &mut FontLibrary) -> Vec<Cluster> {
    let mut clusters = Vec::new();
    let mut text = String::new();
    let mut starts = Vec::new();
    for (index, run) in runs.iter().enumerate() {
        let content = if run.is_break {
            "\n".to_string()
        } else if run.all_caps {
            run.text.to_uppercase()
        } else {
            run.text.clone()
        };
        for ch in content.chars() {
            starts.push(text.len());
            text.push(ch);
            clusters.push(cluster(run, index, ch, fonts));
        }
    }
    mark_breaks(&text, &starts, &mut clusters);
    clusters
}

fn cluster(run: &StyledRun, index: usize, ch: char, fonts: &mut FontLibrary) -> Cluster {
    let is_break = ch == '\n';
    let drawn = if is_break || ch == '\t' { ' ' } else { ch };
    let family = family_for(run, drawn);
    let mut font = fonts.resolve(family, run.bold, run.italic).unwrap_or(0);
    let mut glyph = fonts
        .fonts()
        .get(font as usize)
        .and_then(|f| f.face.glyph(drawn));
    if glyph.is_none() {
        if let Some(other) = fonts.fallback(font, drawn) {
            font = other;
            glyph = fonts.font(font).face.glyph(drawn);
        }
    }
    let (glyph, units) = glyph.unwrap_or((0, 0));
    let scale = fonts
        .fonts()
        .get(font as usize)
        .map_or(0.0, |f| f.face.scale(run.size));
    let advance = if is_break {
        0.0
    } else {
        f64::from(units) * scale + run.spacing
    };
    Cluster {
        ch,
        font,
        glyph,
        advance,
        size: run.size,
        run: index,
        is_space: ch == ' ' || ch == '\u{a0}' || ch == '\t',
        break_after: BreakAfter::Never,
    }
}

fn mark_breaks(text: &str, starts: &[usize], clusters: &mut [Cluster]) {
    for (position, opportunity) in linebreaks(text) {
        // a break opportunity at `position` means "before the character
        // starting there", so it belongs to the cluster before it
        let Some(after) = starts.iter().rposition(|start| *start < position) else {
            continue;
        };
        if position == text.len() {
            continue;
        }
        clusters[after].break_after = match opportunity {
            BreakOpportunity::Mandatory => BreakAfter::Mandatory,
            BreakOpportunity::Allowed => BreakAfter::Allowed,
        };
    }
    for cluster in clusters.iter_mut().filter(|c| c.ch == '\n') {
        cluster.break_after = BreakAfter::Mandatory;
    }
}
