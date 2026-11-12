//! drawingml text: `a:txBody`, paragraphs, runs and the per-level text
//! styles, read (`read`) and written (`write`). the element names below
//! are shared: they say which children each side models.

mod read;
mod write;

pub use read::{
    read_paragraph, read_paragraph_props, read_run_props, read_text_body, read_text_style,
};
pub use write::{
    write_body, write_paragraph, write_paragraph_props, write_run_props, write_text_body,
    write_text_style,
};

pub(super) const BODY_ATTRIBUTES: [&str; 11] = [
    "lIns",
    "tIns",
    "rIns",
    "bIns",
    "anchor",
    "anchorCtr",
    "wrap",
    "numCol",
    "spcCol",
    "vert",
    "rot",
];
pub(super) const AUTOFIT: [&str; 3] = ["a:noAutofit", "a:normAutofit", "a:spAutoFit"];

pub(super) const PARAGRAPH_ATTRIBUTES: [&str; 7] =
    ["marL", "marR", "lvl", "indent", "algn", "defTabSz", "rtl"];
pub(super) const LINE_SPACING: [&str; 1] = ["a:lnSpc"];
pub(super) const SPACE_BEFORE: [&str; 1] = ["a:spcBef"];
pub(super) const SPACE_AFTER: [&str; 1] = ["a:spcAft"];
pub(super) const BULLET_COLOR: [&str; 2] = ["a:buClrTx", "a:buClr"];
pub(super) const BULLET_SIZE: [&str; 3] = ["a:buSzTx", "a:buSzPct", "a:buSzPts"];
pub(super) const BULLET_FONT: [&str; 2] = ["a:buFontTx", "a:buFont"];
pub(super) const BULLET: [&str; 4] = ["a:buNone", "a:buAutoNum", "a:buChar", "a:buBlip"];
pub(super) const TABS: [&str; 1] = ["a:tabLst"];
pub(super) const DEFAULT_RUN: [&str; 1] = ["a:defRPr"];

pub(super) const RUN_ATTRIBUTES: [&str; 10] = [
    "lang", "sz", "b", "i", "u", "strike", "kern", "cap", "spc", "baseline",
];
pub(super) const HIGHLIGHT: [&str; 1] = ["a:highlight"];
pub(super) const LATIN: [&str; 1] = ["a:latin"];
pub(super) const EAST_ASIAN: [&str; 1] = ["a:ea"];
pub(super) const COMPLEX: [&str; 1] = ["a:cs"];
pub(super) const SYMBOL: [&str; 1] = ["a:sym"];

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::model::{Autofit, TextRun};
    use crate::pptx::dml::{ReadContext, WriteContext};
    use crate::pptx::rels::Rels;
    use crate::xml::parse;

    const A: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"";

    #[test]
    fn paragraphs_round_trip_and_keep_unknown_pieces() {
        let text = format!(
            "<p:txBody {A}><a:bodyPr wrap=\"square\" rtlCol=\"0\"><a:normAutofit fontScale=\"62500\" lnSpcReduction=\"20000\"/></a:bodyPr><a:lstStyle/><a:p><a:pPr marL=\"342900\" indent=\"-342900\" eaLnBrk=\"1\"><a:buFont typeface=\"Arial\"/><a:buChar char=\"•\"/></a:pPr><a:r><a:rPr lang=\"en-US\" sz=\"2400\" dirty=\"0\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:effectLst/><a:latin typeface=\"+mn-lt\"/></a:rPr><a:t>Hello &amp; welcome</a:t></a:r></a:p></p:txBody>"
        );
        let document = parse(&text).unwrap();
        let (rels, media) = (Rels::default(), HashMap::new());
        let context = ReadContext {
            part: "x",
            text: &text,
            rels: &rels,
            media: &media,
        };
        let mut body = read_text_body(&document.root, &context);
        assert_eq!(body.plain_text(), "Hello & welcome");
        assert_eq!(
            body.body.autofit,
            Some(Autofit::Shrink {
                font_scale: Some(62500),
                line_reduction: Some(20000)
            })
        );
        let media_parts = HashMap::new();
        let mut write = WriteContext {
            part: "x".into(),
            rels: Rels::default(),
            media_parts: &media_parts,
        };
        // untouched: the paragraph is its original xml
        let original = write_paragraph(&body.paragraphs[0], &mut write);
        assert!(text.contains(&original));
        // edited: rebuilt, unknown pieces still there
        if let TextRun::Text { text, .. } = &mut body.paragraphs[0].runs[0] {
            *text = "Hi".into();
        }
        let rebuilt = write_paragraph(&body.paragraphs[0], &mut write);
        assert_eq!(
            rebuilt,
            "<a:p><a:pPr marL=\"342900\" indent=\"-342900\" eaLnBrk=\"1\"><a:buFont typeface=\"Arial\"/><a:buChar char=\"•\"/></a:pPr><a:r><a:rPr lang=\"en-US\" sz=\"2400\" dirty=\"0\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:effectLst/><a:latin typeface=\"+mn-lt\"/></a:rPr><a:t>Hi</a:t></a:r></a:p>"
        );
        assert_eq!(
            write_body(&body.body),
            "<a:bodyPr wrap=\"square\" rtlCol=\"0\"><a:normAutofit fontScale=\"62500\" lnSpcReduction=\"20000\"/></a:bodyPr>"
        );
    }
}
