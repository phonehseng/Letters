//! the wordprocessingml and drawingml of the recipe, as small string
//! builders (recipe 7.1, 7.5, 8 and 10). every element here was chosen and
//! measured; nothing comes from a template.

use crate::plan::glyphs::CharRun;
use crate::units::{pt_to_emu, pt_to_twips};

pub const NAMESPACES: &str = concat!(
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#,
    r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
    r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
    r#"xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" "#,
    r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" "#,
    r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
    r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" mc:Ignorable="w14""#
);

/// each of these switches off something word would otherwise adjust on its own.
pub const LAYOUT_NEUTRAL: &str = concat!(
    r#"<w:widowControl w:val="0"/>"#,
    "<w:suppressAutoHyphens/>",
    r#"<w:snapToGrid w:val="0"/>"#,
    r#"<w:autoSpaceDE w:val="0"/>"#,
    r#"<w:autoSpaceDN w:val="0"/>"#,
    r#"<w:adjustRightInd w:val="0"/>"#
);

/// neighbouring runs that differ in any property become separate text
/// objects in word's pdf (e11), and only a glyph that starts a text object
/// is printed exactly where the writer model says. the east asian language
/// changes nothing for latin text, so it alternates from glyph to glyph
/// (recipe 6.4).
const SPLIT_LANGUAGES: [&str; 2] = ["en-US", "en-GB"];

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// one run per char (recipe 7.5). there is no `w:kern`: its presence
/// switches kerning on for sizes at or above its value, so `w:val="0"`
/// would kern everything.
pub fn runs_xml(runs: &[CharRun]) -> String {
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
        if run.spacing_twips != 0 {
            xml.push_str(&format!(r#"<w:spacing w:val="{}"/>"#, run.spacing_twips));
        }
        xml.push_str(&format!(
            r#"<w:sz w:val="{0}"/><w:szCs w:val="{0}"/><w:lang w:eastAsia="{1}"/><w14:ligatures w14:val="none"/>"#,
            run.half_points,
            SPLIT_LANGUAGES[index % 2]
        ));
        xml.push_str(&format!(
            r#"</w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
            escape(&run.char)
        ));
    }
    xml
}

pub fn line_paragraph_props(line_twips: i64) -> String {
    format!(
        r#"{LAYOUT_NEUTRAL}<w:spacing w:before="0" w:after="0" w:line="{line_twips}" w:lineRule="exact"/><w:ind w:left="0" w:right="0" w:firstLine="0"/>"#
    )
}

/// one line in a frame anchored to the page (recipe 7.1).
pub fn frame_paragraph(
    runs: &[CharRun],
    x_twips: i64,
    y_twips: i64,
    width_twips: i64,
    line_twips: i64,
) -> String {
    format!(
        r#"<w:p><w:pPr><w:framePr w:w="{width_twips}" w:wrap="through" w:vAnchor="page" w:hAnchor="page" w:x="{x_twips}" w:y="{y_twips}"/>{}</w:pPr>{}</w:p>"#,
        line_paragraph_props(line_twips),
        runs_xml(runs)
    )
}

/// a drawing anchored to the page, in points from the top left.
pub struct Anchor {
    pub id: u32,
    pub left_pt: f64,
    pub top_pt: f64,
    pub width_pt: f64,
    pub height_pt: f64,
    pub behind: bool,
}

fn anchor_xml(anchor: &Anchor, graphic: &str) -> String {
    format!(
        concat!(
            r#"<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{height}" "#,
            r#"behindDoc="{behind}" locked="1" layoutInCell="0" allowOverlap="1"><wp:simplePos x="0" y="0"/>"#,
            r#"<wp:positionH relativeFrom="page"><wp:posOffset>{left}</wp:posOffset></wp:positionH>"#,
            r#"<wp:positionV relativeFrom="page"><wp:posOffset>{top}</wp:posOffset></wp:positionV>"#,
            r#"<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>"#,
            r#"<wp:wrapNone/><wp:docPr id="{id}" name="Item {id}"/><wp:cNvGraphicFramePr/>{graphic}</wp:anchor>"#
        ),
        height = 251_658_240 + u64::from(anchor.id),
        behind = u8::from(anchor.behind),
        left = pt_to_emu(anchor.left_pt),
        top = pt_to_emu(anchor.top_pt),
        cx = pt_to_emu(anchor.width_pt).max(1),
        cy = pt_to_emu(anchor.height_pt).max(1),
        id = anchor.id,
        graphic = graphic,
    )
}

fn xfrm(width_pt: f64, height_pt: f64, rotation_deg: f64, flip_h: bool, flip_v: bool) -> String {
    let rotation = if rotation_deg != 0.0 {
        format!(
            r#" rot="{}""#,
            (rotation_deg * 60000.0)
                .round_ties_even()
                .rem_euclid(21_600_000.0) as i64
        )
    } else {
        String::new()
    };
    let flips = format!(
        "{}{}",
        if flip_h { r#" flipH="1""# } else { "" },
        if flip_v { r#" flipV="1""# } else { "" }
    );
    format!(
        r#"<a:xfrm{rotation}{flips}><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
        pt_to_emu(width_pt).max(1),
        pt_to_emu(height_pt).max(1)
    )
}

/// a floating picture (recipe 10, "pictures").
pub struct Picture<'a> {
    pub anchor: Anchor,
    pub relationship: &'a str,
    pub rotation_deg: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

pub fn picture_drawing(picture: &Picture) -> String {
    let a = &picture.anchor;
    let graphic = format!(
        concat!(
            r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic>"#,
            r#"<pic:nvPicPr><pic:cNvPr id="{id}" name="Picture {id}"/><pic:cNvPicPr/></pic:nvPicPr>"#,
            r#"<pic:blipFill><a:blip r:embed="{rel}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
            r#"<pic:spPr>{xfrm}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>"#,
            "</pic:pic></a:graphicData></a:graphic>"
        ),
        id = a.id,
        rel = picture.relationship,
        xfrm = xfrm(
            a.width_pt,
            a.height_pt,
            picture.rotation_deg,
            picture.flip_h,
            picture.flip_v
        ),
    );
    format!(
        "<w:r><w:drawing>{}</w:drawing></w:r>",
        anchor_xml(a, &graphic)
    )
}

/// a drawingml shape (rectangle, line or text box).
fn shape_drawing(anchor: &Anchor, shape_properties: &str, body: &str, text: &str) -> String {
    let text_box = if text.is_empty() { "" } else { r#" txBox="1""# };
    let graphic = format!(
        concat!(
            r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">"#,
            "<wps:wsp><wps:cNvSpPr{tb}/><wps:spPr>{sp}</wps:spPr>{text}{body}</wps:wsp></a:graphicData></a:graphic>"
        ),
        tb = text_box,
        sp = shape_properties,
        text = text,
        body = body,
    );
    format!(
        r#"<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing>{}</w:drawing></mc:Choice></mc:AlternateContent></w:r>"#,
        anchor_xml(anchor, &graphic)
    )
}

/// an axis-aligned rectangle, filled and/or stroked.
pub fn rect_drawing(
    anchor: &Anchor,
    fill: Option<&str>,
    line: Option<&str>,
    line_pt: f64,
) -> String {
    let fill_xml = match fill {
        Some(color) => format!(r#"<a:solidFill><a:srgbClr val="{color}"/></a:solidFill>"#),
        None => "<a:noFill/>".into(),
    };
    let line_xml = match line {
        Some(color) => format!(
            r#"<a:ln w="{}"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill></a:ln>"#,
            pt_to_emu(line_pt)
        ),
        None => "<a:ln><a:noFill/></a:ln>".into(),
    };
    let properties = format!(
        r#"{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill_xml}{line_xml}"#,
        xfrm(anchor.width_pt, anchor.height_pt, 0.0, false, false)
    );
    shape_drawing(anchor, &properties, "<wps:bodyPr/>", "")
}

/// a straight stroked segment; `flip_v` when the slope runs the other way.
pub fn line_drawing(anchor: &Anchor, flip_v: bool, color: &str, line_pt: f64) -> String {
    let properties = format!(
        r#"{}<a:prstGeom prst="line"><a:avLst/></a:prstGeom><a:ln w="{}"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill></a:ln>"#,
        xfrm(anchor.width_pt, anchor.height_pt, 0.0, false, flip_v),
        pt_to_emu(line_pt)
    );
    shape_drawing(anchor, &properties, "<wps:bodyPr/>", "")
}

/// a rotated text box holding one line (recipe 8). `wrap="square"`: with
/// `wrap="none"` word moved the line's start whenever its content got
/// wider or narrower (19.8 pt in round 3).
pub fn rotated_text_drawing(
    anchor: &Anchor,
    clockwise_deg: f64,
    runs: &[CharRun],
    line_twips: i64,
) -> String {
    let paragraph = format!(
        "<w:p><w:pPr>{}</w:pPr>{}</w:p>",
        line_paragraph_props(line_twips),
        runs_xml(runs)
    );
    let properties = format!(
        r#"{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln>"#,
        xfrm(
            anchor.width_pt,
            anchor.height_pt,
            clockwise_deg,
            false,
            false
        )
    );
    let body = r#"<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="0" tIns="0" rIns="0" bIns="0" anchor="t" upright="0"><a:noAutofit/></wps:bodyPr>"#;
    let text = format!("<wps:txbx><w:txbxContent>{paragraph}</w:txbxContent></wps:txbx>");
    shape_drawing(anchor, &properties, body, &text)
}

/// the 1 pt paragraph that carries a page's floating drawings (recipe 10).
pub fn anchor_paragraph(drawings: &str) -> String {
    format!(
        r#"<w:p><w:pPr>{LAYOUT_NEUTRAL}<w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/><w:rPr><w:sz w:val="2"/></w:rPr></w:pPr>{drawings}</w:p>"#
    )
}

/// a tiny paragraph that starts the next page of the same size. it is 1 pt
/// high so it cannot itself spill onto another page.
pub fn page_break_paragraph() -> String {
    format!(
        r#"<w:p><w:pPr><w:pageBreakBefore/>{LAYOUT_NEUTRAL}<w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/><w:rPr><w:sz w:val="2"/></w:rPr></w:pPr></w:p>"#
    )
}

/// one section per page size: zero margins, landscape when wider than tall.
pub fn section_properties(width_pt: f64, height_pt: f64) -> String {
    marked_section_properties(width_pt, height_pt, width_pt > height_pt)
}

/// a section marked landscape or not, whatever its size (probes measure both).
pub fn marked_section_properties(width_pt: f64, height_pt: f64, landscape: bool) -> String {
    let orient = if landscape {
        r#" w:orient="landscape""#
    } else {
        ""
    };
    format!(
        concat!(
            r#"<w:sectPr><w:pgSz w:w="{}" w:h="{}"{}/>"#,
            r#"<w:pgMar w:top="0" w:right="0" w:bottom="0" w:left="0" w:header="0" w:footer="0" w:gutter="0"/>"#,
            r#"<w:cols w:space="0"/><w:docGrid w:linePitch="360"/></w:sectPr>"#
        ),
        pt_to_twips(width_pt),
        pt_to_twips(height_pt),
        orient
    )
}

/// ends a section of this page size; the next page starts a new one.
pub fn section_break_paragraph(width_pt: f64, height_pt: f64) -> String {
    format!(
        r#"<w:p><w:pPr>{LAYOUT_NEUTRAL}<w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/>{}<w:rPr><w:sz w:val="2"/></w:rPr></w:pPr></w:p>"#,
        section_properties(width_pt, height_pt)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(char: &str, spacing: i64) -> CharRun {
        CharRun {
            char: char.into(),
            family: "Ltr 1 12".into(),
            half_points: 24,
            color: "000000".into(),
            spacing_twips: spacing,
            hidden: false,
        }
    }

    #[test]
    fn runs_alternate_the_east_asian_language() {
        let xml = runs_xml(&[run("a", 3), run("b", 0)]);
        assert!(xml.contains(r#"<w:spacing w:val="3"/>"#));
        assert_eq!(xml.matches(r#"<w:spacing"#).count(), 1);
        let first = xml.find("en-US").unwrap();
        let second = xml.find("en-GB").unwrap();
        assert!(first < second);
        assert!(!xml.contains("w:kern"));
    }

    #[test]
    fn hidden_and_coloured_runs() {
        let mut hidden = run("<", 0);
        hidden.hidden = true;
        hidden.color = "FF0000".into();
        let xml = runs_xml(&[hidden]);
        assert!(xml.contains("<w:vanish/>"));
        assert!(xml.contains(r#"<w:color w:val="FF0000"/>"#));
        assert!(xml.contains("&lt;"));
    }

    #[test]
    fn frames_are_anchored_to_the_page() {
        let xml = frame_paragraph(&[run("a", 0)], 1440, 2000, 3000, 240);
        assert!(xml.contains(
            r#"w:wrap="through" w:vAnchor="page" w:hAnchor="page" w:x="1440" w:y="2000""#
        ));
        assert!(xml.contains(r#"w:line="240" w:lineRule="exact""#));
    }

    #[test]
    fn rotation_is_clockwise_in_sixty_thousandths() {
        assert!(xfrm(10.0, 10.0, -90.0, false, false).contains(r#"rot="16200000""#));
        assert!(xfrm(10.0, 10.0, 90.0, false, false).contains(r#"rot="5400000""#));
    }

    #[test]
    fn landscape_sections() {
        assert!(section_properties(720.0, 405.0)
            .contains(r#"w:w="14400" w:h="8100" w:orient="landscape""#));
        assert!(!section_properties(612.0, 792.0).contains("landscape"));
    }
}
