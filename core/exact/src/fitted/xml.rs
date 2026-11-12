//! the runs of a fitted line: one `w:r` per piece, with the w14 property
//! that switches its slot's feature on and no character spacing.
//!
//! each piece is its own run with the east asian language alternating:
//! word starts a new pdf text object at every run whose properties differ
//! (recipe 6.4), so each piece's glyph is placed by its own `Tm`, while
//! shaping still runs across such runs, so chained rules keep working
//! (fitted-font report, "fitted_package"). `w:spacing` would stop the rules
//! from applying (probe c1), so there is none.

use crate::docx::xml::{escape, line_paragraph_props};
use crate::report::FramePosition;

const LANGUAGES: [&str; 2] = ["en-US", "en-GB"];

/// how a run asks word for its slot's feature (iso 29500 w14 extensions).
fn feature_properties(feature: Option<&[u8; 4]>) -> String {
    match feature {
        Some(b"liga") => r#"<w14:ligatures w14:val="standard"/>"#.into(),
        Some(tag) => {
            let number: u8 = std::str::from_utf8(&tag[2..])
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(1);
            format!(r#"<w14:stylisticSets><w14:styleSet w14:id="{number}"/></w14:stylisticSets>"#)
        }
        // plain text: no rule may apply
        None => r#"<w14:ligatures w14:val="none"/>"#.into(),
    }
}

/// what one `w:r` of a fitted line holds.
pub struct TextRun {
    pub text: String,
    pub family: String,
    pub half_points: i64,
    pub color: String,
    pub hidden: bool,
    pub feature: Option<&'static [u8; 4]>,
}

/// the runs, alternating the east asian language from run to run.
pub fn runs_xml(runs: &[TextRun]) -> String {
    let mut xml = String::new();
    for (index, run) in runs.iter().enumerate() {
        let family = escape(&run.family);
        xml.push_str("<w:r><w:rPr>");
        xml.push_str(&format!(
            r#"<w:rFonts w:ascii="{family}" w:hAnsi="{family}" w:cs="{family}" w:eastAsia="{family}"/>"#
        ));
        if run.hidden {
            xml.push_str("<w:vanish/>");
        }
        if run.color != "000000" {
            xml.push_str(&format!(r#"<w:color w:val="{}"/>"#, run.color));
        }
        xml.push_str(&format!(
            r#"<w:sz w:val="{0}"/><w:szCs w:val="{0}"/><w:lang w:eastAsia="{1}"/>{2}"#,
            run.half_points,
            LANGUAGES[index % 2],
            feature_properties(run.feature)
        ));
        xml.push_str(&format!(
            r#"</w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
            escape(&run.text)
        ));
    }
    xml
}

/// one line in its round one frame (recipe 7.1), with fitted runs.
pub fn frame_paragraph(runs: &[TextRun], position: &FramePosition) -> String {
    format!(
        r#"<w:p><w:pPr><w:framePr w:w="{}" w:wrap="through" w:vAnchor="page" w:hAnchor="page" w:x="{}" w:y="{}"/>{}</w:pPr>{}</w:p>"#,
        position.width_twips,
        position.x_twips,
        position.y_twips,
        line_paragraph_props(position.line_twips),
        runs_xml(runs)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_become_w14_properties_and_no_spacing() {
        let run = |feature| TextRun {
            text: "word ".into(),
            family: "Ltr 1 10".into(),
            half_points: 20,
            color: "000000".into(),
            hidden: false,
            feature,
        };
        let xml = runs_xml(&[run(Some(b"liga")), run(Some(b"ss07")), run(None)]);
        assert!(xml.contains(r#"<w14:ligatures w14:val="standard"/>"#));
        assert!(xml.contains(r#"<w14:styleSet w14:id="7"/>"#));
        assert!(xml.contains(r#"<w14:ligatures w14:val="none"/>"#));
        assert!(!xml.contains("w:spacing"));
        assert!(xml.find("en-US").unwrap() < xml.find("en-GB").unwrap());
    }
}
