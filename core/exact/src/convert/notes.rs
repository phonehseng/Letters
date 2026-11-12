//! plain sentences for the person about the fonts and scripts of a conversion.

use crate::report::ConversionReport;

/// plain sentences about fonts that aren't the originals.
pub(super) fn describe_fonts(report: &mut ConversionReport) {
    for font in &report.fonts {
        if let Some(reason) = &font.not_used_because {
            if reason.starts_with("fsType") {
                report.warnings.push(format!(
                    "“{}” doesn’t allow embedding, so a similar font was used.",
                    font.pdf_font
                ));
            } else {
                report.warnings.push(format!(
                    "“{}” couldn’t be read from the PDF, so a similar font was used.",
                    font.pdf_font
                ));
            }
        } else if font.substitute.is_some() {
            report.warnings.push(format!(
                "“{}” isn’t included in the PDF, so a similar font was used.",
                font.pdf_font
            ));
        }
        if font.private_use_glyphs > 0 {
            report.warnings.push(format!(
                "{} glyph{} of “{}” {} no known text, so copying {} gives placeholder characters.",
                font.private_use_glyphs,
                if font.private_use_glyphs == 1 {
                    ""
                } else {
                    "s"
                },
                font.pdf_font,
                if font.private_use_glyphs == 1 {
                    "has"
                } else {
                    "have"
                },
                if font.private_use_glyphs == 1 {
                    "it"
                } else {
                    "them"
                }
            ));
        }
    }
    let unshaped = report.scripts.right_to_left + report.scripts.complex_script;
    if unshaped > 0 {
        report.warnings.push("Some text is in a right-to-left or complex script, which this conversion doesn’t shape; check it in Word.".into());
    }
    if report.glyph_counts.glyphs_without_char > 0 {
        report.warnings.push(format!(
            "{} glyphs couldn’t be written as text and were left out.",
            report.glyph_counts.glyphs_without_char
        ));
    }
}
