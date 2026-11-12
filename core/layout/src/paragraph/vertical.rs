//! a line's height and where its baseline sits, from the line spacing rule.

use crate::flavor::Flavor;
use crate::fonts::LineMetrics;
use crate::style::{LineSpacingRule, ParaStyle};

/// what a line holds, vertically: its text and pictures, its text alone,
/// and its superscripts and subscripts (which follow other rules).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineContent {
    pub content: LineMetrics,
    pub text: LineMetrics,
    pub scripts: Option<LineMetrics>,
}

impl LineContent {
    /// a line of plain text and pictures with these metrics
    pub fn plain(metrics: LineMetrics) -> LineContent {
        LineContent {
            content: metrics,
            text: metrics,
            scripts: None,
        }
    }

    /// how far the scripts reach above and below the rest of the line.
    pub fn script_overhang(&self) -> (f64, f64) {
        match self.scripts {
            Some(scripts) => (
                (scripts.ascent - self.content.ascent).max(0.0),
                (scripts.descent - self.content.descent).max(0.0),
            ),
            None => (0.0, 0.0),
        }
    }
}

/// the vertical box of one line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineBox {
    /// what the line takes from the column
    pub height: f64,
    /// what must fit above the bottom of the page: the extra space of a
    /// multiple line spacing may hang below it, as space after does (word
    /// fits a 1.08 calibri line whose text ends at the margin, headings
    /// in the corpus)
    pub fit_height: f64,
    /// baseline below the top of the line
    pub baseline: f64,
    /// the content's own ascent and descent (for selection and carets)
    pub ascent: f64,
    pub descent: f64,
}

/// the line box for content with `content` metrics under the paragraph's
/// line spacing.
///
/// - multiple ("single", 1.08, 1.15, double): the content's height plus
///   (factor - 1) times the text's height, the extra space below the text.
/// - exact: the height is the value, whatever the fonts; word puts the
///   baseline at 0.8 of it (recipe 6.6, 960 of 960 frames).
/// - at least: the larger of the value and the content's height, the
///   extra above the text as for multiples.
pub fn line_box(line: &LineContent, style: &ParaStyle, flavor: Flavor) -> LineBox {
    let content = line.content;
    let text = line.text;
    let natural = content.height();
    let (height, baseline) = match style.line {
        LineSpacingRule::Multiple(factor) => {
            // only the text's height is multiplied: a 36 pt inline picture
            // in 1.08 calibri 11 makes a 36 + 2.95 + 0.08 x 13.43 pt line
            // in word (images_inline in the corpus)
            let extra = (factor - 1.0).max(0.0) * text.height();
            let height = natural + (factor - 1.0) * text.height();
            // both put the extra space of a multiple below the text: word's
            // 1.08 lines in calibri 11 start where single lines start
            // (alignments and lists in the corpus, first pass 1.04 pt off
            // with the space above)
            // superscripts and subscripts reaching beyond the text use up
            // that extra space first, and only then grow the line: double
            // spaced lines keep their height with a footnote number in
            // them, single spaced note lines grow by the number's overhang
            // (footnotes in the corpus)
            let (above, below) = line.script_overhang();
            let grow_above = (above - extra).max(0.0);
            let grow_below = (below - (extra - above).max(0.0)).max(0.0);
            let height = height + grow_above + grow_below;
            // libreoffice keeps line heights in whole twips: a 1.08 line of
            // calibri 11 pitches 14.50 pt in its pdf, not 14.515
            let height = match flavor {
                Flavor::LibreOffice => (height * 20.0).round() / 20.0,
                Flavor::Word => height,
            };
            (height, content.ascent + grow_above)
        }
        LineSpacingRule::Exact(value) => {
            let baseline = match flavor {
                Flavor::Word => 0.8 * value,
                Flavor::LibreOffice => value - content.descent,
            };
            (value, baseline)
        }
        LineSpacingRule::AtLeast(value) => {
            let (above, below) = line.script_overhang();
            let natural = natural + above + below;
            let height = natural.max(value);
            (height, height - content.descent - below)
        }
    };
    let fit_height = match style.line {
        LineSpacingRule::Multiple(_) => height.min(natural),
        _ => height,
    };
    LineBox {
        height,
        fit_height,
        baseline,
        ascent: content.ascent,
        descent: content.descent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Document, Paragraph};

    fn style(rule: LineSpacingRule) -> ParaStyle {
        let mut style =
            crate::style::para_style(&Document::new(), &Paragraph::with_text(""), Flavor::Word);
        style.line = rule;
        style
    }

    const CALIBRI_11: LineMetrics = LineMetrics {
        ascent: 10.474,
        descent: 2.954,
    };

    #[test]
    fn single_spacing_is_the_font_height() {
        let line = line_box(
            &LineContent::plain(CALIBRI_11),
            &style(LineSpacingRule::Multiple(1.0)),
            Flavor::Word,
        );
        assert!((line.height - 13.428).abs() < 1e-9);
        assert!((line.baseline - 10.474).abs() < 1e-9);
    }

    #[test]
    fn multiples_scale_the_height() {
        let line = line_box(
            &LineContent::plain(CALIBRI_11),
            &style(LineSpacingRule::Multiple(259.0 / 240.0)),
            Flavor::Word,
        );
        assert!((line.height - 13.428 * 259.0 / 240.0).abs() < 1e-9);
    }

    #[test]
    fn exact_ignores_the_fonts() {
        let line = line_box(
            &LineContent::plain(CALIBRI_11),
            &style(LineSpacingRule::Exact(15.0)),
            Flavor::Word,
        );
        assert_eq!(line.height, 15.0);
        assert!((line.baseline - 12.0).abs() < 1e-9);
    }

    #[test]
    fn at_least_never_shrinks() {
        let small = line_box(
            &LineContent::plain(CALIBRI_11),
            &style(LineSpacingRule::AtLeast(5.0)),
            Flavor::Word,
        );
        assert!((small.height - 13.428).abs() < 1e-9);
        let tall = line_box(
            &LineContent::plain(CALIBRI_11),
            &style(LineSpacingRule::AtLeast(20.0)),
            Flavor::Word,
        );
        assert_eq!(tall.height, 20.0);
    }
}
