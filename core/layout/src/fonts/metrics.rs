//! the vertical metrics a word processor takes from a font for one line.
//!
//! a font carries three ascent/descent pairs (os/2 win, os/2 typo, hhea).
//! which one decides the line height is the first thing that differs
//! between word processors; the choice is a flavor rule, measured on the
//! font probes of the oracle corpus (see ORACLE_REPORT.md, "fonts").

use serde::{Deserialize, Serialize};

use super::face::FaceMetrics;
use crate::flavor::Flavor;

/// a font's share of one line at one size, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMetrics {
    /// baseline to the top of the line box
    pub ascent: f64,
    /// baseline to the bottom of the line box
    pub descent: f64,
}

impl LineMetrics {
    pub fn height(&self) -> f64 {
        self.ascent + self.descent
    }

    /// the larger of two, as a line holding both fonts needs.
    pub fn max(self, other: LineMetrics) -> LineMetrics {
        LineMetrics {
            ascent: self.ascent.max(other.ascent),
            descent: self.descent.max(other.descent),
        }
    }
}

/// gdi's "external leading": the part of hhea's line gap not already
/// inside the win ascent and descent. windows applications see a font
/// through these numbers (tmHeight + tmExternalLeading).
pub fn external_leading(metrics: &FaceMetrics) -> f64 {
    let win = metrics.win_ascent + metrics.win_descent;
    let hhea = metrics.hhea_ascent - metrics.hhea_descent;
    (metrics.hhea_line_gap - (win - hhea)).max(0.0)
}

/// the line metrics of a face at `size` points, by the flavor's rule.
///
/// word: win ascent and descent, with the external leading added above
/// the ascent. measured on the font probes (word 16.0.20527): calibri
/// 11 pt pitches 13.43 pt (= 11 x 2500/2048, win 1950 + 550, no external
/// leading), times new roman 12 pt pitches 13.80 pt (= 12 x (2268 + 87)
/// / 2048, the 87 units being hhea's line gap left over); the first
/// baseline sits ascent + leading below the top margin.
///
/// libreoffice (windows build): the same win values; it also adds the
/// external leading for documents imported from word (its "add external
/// leading" compatibility switch is on for docx), see the flavor notes.
pub fn line_metrics(metrics: &FaceMetrics, size: f64, flavor: Flavor) -> LineMetrics {
    let scale = size / metrics.units_per_em.max(1.0);
    let rules = flavor.rules();
    if rules.hhea_in_twips {
        let twips = |points: f64| (points * 20.0).round() / 20.0;
        return LineMetrics {
            ascent: twips((metrics.hhea_ascent + metrics.hhea_line_gap.max(0.0)) * scale),
            descent: twips(-metrics.hhea_descent * scale),
        };
    }
    let typo = rules.typo_metrics_when_flagged && metrics.use_typo_metrics;
    let (ascent, descent) = if typo {
        (metrics.typo_ascent, -metrics.typo_descent)
    } else {
        (metrics.win_ascent, metrics.win_descent)
    };
    let leading = if typo {
        metrics.typo_line_gap
    } else if rules.add_external_leading {
        external_leading(metrics)
    } else {
        0.0
    };
    LineMetrics {
        ascent: (ascent + leading) * scale,
        descent: descent * scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calibri() -> FaceMetrics {
        FaceMetrics {
            units_per_em: 2048.0,
            win_ascent: 1950.0,
            win_descent: 550.0,
            hhea_ascent: 1536.0,
            hhea_descent: -512.0,
            hhea_line_gap: 452.0,
            typo_ascent: 1536.0,
            typo_descent: -512.0,
            typo_line_gap: 452.0,
            ..FaceMetrics::default()
        }
    }

    fn times() -> FaceMetrics {
        FaceMetrics {
            units_per_em: 2048.0,
            win_ascent: 1825.0,
            win_descent: 443.0,
            hhea_ascent: 1825.0,
            hhea_descent: -443.0,
            hhea_line_gap: 87.0,
            typo_ascent: 1420.0,
            typo_descent: -442.0,
            typo_line_gap: 307.0,
            ..FaceMetrics::default()
        }
    }

    #[test]
    fn calibri_has_no_external_leading() {
        assert_eq!(external_leading(&calibri()), 0.0);
        let line = line_metrics(&calibri(), 11.0, Flavor::Word);
        assert!((line.height() - 13.4277).abs() < 1e-3, "{line:?}");
    }

    #[test]
    fn times_adds_its_line_gap() {
        assert_eq!(external_leading(&times()), 87.0);
        let line = line_metrics(&times(), 12.0, Flavor::Word);
        assert!((line.height() - 13.799).abs() < 1e-3, "{line:?}");
    }
}
