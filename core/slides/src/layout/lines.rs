//! greedy line breaking of a paragraph's clusters, and the height and
//! baseline of each line.

use super::runs::{BreakAfter, Cluster};
use crate::fonts::FontLibrary;

/// a line: a range of clusters and where each one starts.
#[derive(Clone, Debug)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
    /// width without the spaces hanging at its end
    pub width: f64,
}

/// breaks clusters into lines no wider than `first_width` (the first
/// line) and `width` (the others). `wrap` false only breaks at line
/// breaks. a word wider than the line is split, as powerpoint does.
pub fn break_lines(
    clusters: &[Cluster],
    first_width: f64,
    width: f64,
    wrap: bool,
) -> Vec<LineRange> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start < clusters.len() {
        let available = if lines.is_empty() { first_width } else { width };
        let end = line_end(clusters, start, available, wrap);
        lines.push(LineRange {
            start,
            end,
            width: visible_width(&clusters[start..end]),
        });
        start = end;
    }
    lines
}

/// where the line starting at `start` ends (exclusive).
fn line_end(clusters: &[Cluster], start: usize, available: f64, wrap: bool) -> usize {
    let mut used = 0.0;
    let mut last_break: Option<usize> = None;
    for (index, cluster) in clusters.iter().enumerate().skip(start) {
        if cluster.break_after == BreakAfter::Mandatory {
            return index + 1;
        }
        // spaces may hang past the edge; only ink has to fit
        if wrap && !cluster.is_space && used + cluster.advance > available && index > start {
            return last_break.map_or(index, |after| after + 1);
        }
        used += cluster.advance;
        if cluster.break_after == BreakAfter::Allowed {
            last_break = Some(index);
        }
    }
    clusters.len()
}

fn visible_width(clusters: &[Cluster]) -> f64 {
    let trailing = clusters
        .iter()
        .rev()
        .take_while(|c| c.is_space || c.ch == '\n')
        .count();
    clusters[..clusters.len() - trailing]
        .iter()
        .map(|c| c.advance)
        .sum()
}

/// the biggest size on a line and where its font puts the baseline, as
/// a share of the line's height from the top: hhea ascent over ascent
/// plus descent. measured against powerpoint (FIDELITY_REPORT.md, cause
/// 1): with calibri, 0.75 of a 90 % line of 24 pt and 28 pt text gives
/// every baseline of pp_title_bullets within 0.1 pt.
pub fn line_extent(clusters: &[Cluster], fonts: &FontLibrary, mark: (u32, f64)) -> (f64, f64) {
    let mut size: f64 = 0.0;
    let mut font = mark.0;
    for cluster in clusters.iter().filter(|c| c.ch != '\n') {
        if cluster.size > size {
            size = cluster.size;
            font = cluster.font;
        }
    }
    if size == 0.0 {
        // an empty line takes the height of its paragraph mark
        size = mark.1;
    }
    let ratio = fonts
        .fonts()
        .get(font as usize)
        .map_or(DEFAULT_ASCENT_SHARE, |resolved| {
            let metrics = &resolved.face.metrics;
            let total = metrics.hhea_ascent - metrics.hhea_descent;
            if total > 0.0 {
                metrics.hhea_ascent / total
            } else {
                DEFAULT_ASCENT_SHARE
            }
        });
    (size, ratio)
}

/// the share most latin fonts give the ascent, for a face with no metrics
const DEFAULT_ASCENT_SHARE: f64 = 0.8;
