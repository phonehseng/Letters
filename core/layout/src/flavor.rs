//! the two personalities of the engine.
//!
//! word and libreoffice lay the same file out differently in small ways.
//! every rule where they were measured to differ is a field of `Rules`,
//! with the measurement that showed it next to it. `Flavor::Word` is the
//! default: most student documents come from word.
//!
//! the split this engine keeps: rules here decide *where lines and pages
//! break* (they must imitate the oracle, including its rounding when the
//! rounding decides a break). *where glyphs are drawn* keeps full
//! precision: positions in the result and in our pdf are fractional
//! points, never snapped to a device grid.
//!
//! where an oracle's rounding was measured to decide breaks it is a rule
//! here (libreoffice's whole-twip line heights). word's 1/600 inch layout
//! grid (recipe 6) is not modelled for breaks: exact widths and heights
//! already break the corpus' lines like word except where noted in the
//! report, and the grid would only matter for a line ending within a
//! hundredth of a point of the margin.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Flavor {
    /// microsoft word 16 (measured on build 16.0.20527, windows)
    #[default]
    Word,
    /// libreoffice writer (measured on 26.8.2, windows)
    LibreOffice,
}

/// the measured rule set of one flavor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// add gdi's external leading (hhea line gap beyond the win metrics)
    /// above the ascent. word: yes (times new roman 12 pt pitches 13.80,
    /// not 13.29). libreoffice reads hhea instead (see `metrics.rs`).
    pub add_external_leading: bool,
    /// the font metrics: word takes win (or typo when flagged) plus gdi's
    /// external leading; libreoffice takes hhea, its line gap above the
    /// ascent, each rounded to whole twips (calibri 11 pt: ascent 10.70,
    /// line 13.45 in libreoffice's pdf, against 10.68 and 13.43 exact).
    pub hhea_in_twips: bool,
    /// use the os/2 typo metrics when the font sets USE_TYPO_METRICS.
    /// word: yes (aptos sets the flag; its 11 pt lines pitch 13.44 pt, the
    /// typo height 2500/2048, not the win height 2631/2048 = 14.13).
    pub typo_metrics_when_flagged: bool,
    /// pair kerning from the font's kern/gpos tables. word: off unless a
    /// run asks for it (`w:kern`), recipe 7.5 and e3. libreoffice: the same
    /// for docx (fitted on the corpus).
    pub kerning: bool,
    /// standard ligatures (fi, fl). word: off unless `w14:ligatures`.
    pub ligatures: bool,
    /// widow and orphan control when the file says nothing. word treats a
    /// missing w:widowControl as on (its normal template default).
    pub widow_control_default: bool,
    /// space before is dropped for a paragraph that starts a page after a
    /// natural page break (word: compat 15 still suppresses it at the top
    /// of a page; recipe 12.15 for the page break case).
    pub drop_space_before_at_page_top: bool,
    /// ... and also after a hard page break or page break before
    pub drop_space_before_after_hard_break: bool,
    /// contextual spacing removes only the spacing between paragraphs of
    /// the same style (both word and libreoffice)
    pub contextual_spacing: bool,
    /// the share of a justified line's space width that may be squeezed
    /// away to keep its last word on it. word: about a quarter (see
    /// `paragraph::shrinkable`). libreoffice 26.8 squeezes the same way:
    /// on the six justification probes both applications took in exactly
    /// the same words (fit_justify_probes; the aptos probe aside, which
    /// libreoffice sets in another font), up to 0.25 of the spaces
    pub justify_space_shrink: f64,
    /// fonts office downloads on demand (aptos) are found. word: yes;
    /// libreoffice: no, it substitutes times new roman (font probes).
    pub office_cloud_fonts: bool,
    /// each glyph's advance rounded to this unit (points). word keeps
    /// fractional advances (recipe e3: its average advance stays within
    /// 0.02 pt of the exact one); so does libreoffice (measured below).
    pub advance_unit: Option<f64>,
    /// widow and orphan control decide where a table row splits at a page
    /// end. word: yes (w_tables_aptos: rows of two-line cells never split
    /// one and one; page breaks of the tables class 33 % -> 53 %).
    /// libreoffice: no (the same rule took its tables from 58 % to 32 %)
    pub widows_in_cells: bool,
}

impl Flavor {
    pub fn rules(self) -> Rules {
        match self {
            Flavor::Word => Rules {
                hhea_in_twips: false,
                add_external_leading: true,
                typo_metrics_when_flagged: true,
                kerning: false,
                ligatures: false,
                widow_control_default: true,
                drop_space_before_at_page_top: true,
                drop_space_before_after_hard_break: true,
                contextual_spacing: true,
                justify_space_shrink: 0.25,
                office_cloud_fonts: true,
                advance_unit: None,
                widows_in_cells: true,
            },
            Flavor::LibreOffice => Rules {
                hhea_in_twips: true,
                add_external_leading: true,
                typo_metrics_when_flagged: false,
                // measured over the whole corpus: kerning and ligatures off
                // break 78 % of lines like libreoffice, both on only 54 %;
                // it honours docx's w:kern and w14:ligatures like word
                kerning: false,
                ligatures: false,
                widow_control_default: true,
                drop_space_before_at_page_top: true,
                drop_space_before_after_hard_break: false,
                contextual_spacing: true,
                justify_space_shrink: 0.25,
                office_cloud_fonts: false,
                // rounding advances to whole twips broke 75.1 % of lines
                // like libreoffice against 79.1 % exact: its text layout
                // keeps fractional advances although its pages are twips
                advance_unit: None,
                widows_in_cells: false,
            },
        }
    }
}
