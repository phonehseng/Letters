//! pictures as `w:drawing` xml.

use crate::model::{Block, FloatPosition, Image, Placement, TextBox, Wrap};
use crate::units::twips_to_emu;
use crate::xml::{escape_attribute, XmlWriter};

use super::{DocxWriter, StoryOut};

impl DocxWriter<'_> {
    pub(crate) fn drawing_xml(&mut self, image: &Image, story: &mut StoryOut) -> String {
        if let Some(original) = self.reusable_original(image, story) {
            return original;
        }
        let Some(path) = self.media_paths.get(&image.media_id).cloned() else {
            // a picture with no bytes cannot be shown; leave it out
            return String::new();
        };
        let rel = story.media_rel(&image.media_id, &path);
        self.next_drawing_id += 1;
        let id = self.next_drawing_id;
        let (cx, cy) = image.size_emu();
        let (cx, cy) = (cx.max(1), cy.max(1));
        let name = escape_attribute(image.name.as_deref().unwrap_or(&format!("Picture {id}")));
        let description = escape_attribute(image.description.as_deref().unwrap_or(""));
        let crop = image
            .crop
            .map(|c| {
                format!(
                    "<a:srcRect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>",
                    c.left, c.top, c.right, c.bottom
                )
            })
            .unwrap_or_default();
        let graphic = format!(
            "<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
             <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
             <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
             <pic:nvPicPr><pic:cNvPr id=\"0\" name=\"{name}\"/><pic:cNvPicPr/></pic:nvPicPr>\
             <pic:blipFill><a:blip r:embed=\"{rel}\"/>{crop}<a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
             <pic:spPr><a:xfrm{}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
             <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\
             </pic:pic></a:graphicData></a:graphic>",
            rotation_attribute(image.rotation)
        );
        let doc_pr = format!("<wp:docPr id=\"{id}\" name=\"{name}\" descr=\"{description}\"/>");
        let extent = format!("<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>");
        let locks = match image.lock_aspect_ratio {
            Some(lock) => format!(
                "<wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" noChangeAspect=\"{}\"/></wp:cNvGraphicFramePr>",
                u8::from(lock)
            ),
            None => "<wp:cNvGraphicFramePr/>".to_string(),
        };
        let frame = match &image.placement {
            Placement::Inline => format!(
                "<wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">{extent}{doc_pr}{locks}{graphic}</wp:inline>"
            ),
            Placement::Floating { horizontal, vertical, wrap } => format!(
                "{}<wp:simplePos x=\"0\" y=\"0\"/>{}{}{extent}{}{doc_pr}{locks}{graphic}</wp:anchor>",
                anchor_open(*wrap, image.z_order.unwrap_or(251_658_240 + i64::from(id))),
                position_xml("wp:positionH", horizontal),
                position_xml("wp:positionV", vertical),
                wrap_xml(*wrap),
            ),
        };
        format!("<w:drawing>{frame}</w:drawing>")
    }

    /// a text box as word writes it: a drawingml shape for word 2010 and
    /// later, with a vml copy for older versions, in one compatibility
    /// wrapper. both hold the same content.
    pub(crate) fn text_box_run(&mut self, text_box: &TextBox, story: &mut StoryOut) -> String {
        self.next_drawing_id += 1;
        let id = self.next_drawing_id;
        let (cx, cy) = text_box.size_emu();
        let (cx, cy) = (cx.max(1), cy.max(1));
        let name = escape_attribute(
            text_box
                .name
                .as_deref()
                .unwrap_or(&format!("Text Box {id}")),
        );
        let mut content = XmlWriter::new();
        self.write_blocks(&text_box.blocks, story, &mut content, None, false);
        let ends_with_paragraph = matches!(
            text_box.blocks.last(),
            Some(Block::Paragraph(_)) | Some(Block::Heading(_))
        );
        if !ends_with_paragraph {
            content.raw("<w:p/>");
        }
        let content = content.finish();
        let fill = match &text_box.fill {
            Some(hex) => format!(
                "<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
                escape_attribute(hex)
            ),
            None => "<a:noFill/>".into(),
        };
        let line = match &text_box.border {
            Some(border) if border.style != "none" && border.style != "nil" => format!(
                "<a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>",
                i64::from(border.size) * 12700 / 8,
                escape_attribute(border.color.as_deref().unwrap_or("000000"))
            ),
            _ => "<a:ln><a:noFill/></a:ln>".into(),
        };
        let m = &text_box.margins;
        let fit = if text_box.auto_height {
            "<a:spAutoFit/>"
        } else {
            "<a:noAutofit/>"
        };
        let z = text_box.z_order.unwrap_or(251_658_240 + i64::from(id));
        let shape = format!(
            "<w:drawing>{}<wp:simplePos x=\"0\" y=\"0\"/>{}{}<wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
             <wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>{}<wp:docPr id=\"{id}\" name=\"{name}\"/><wp:cNvGraphicFramePr/>\
             <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
             <a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp>\
             <wps:cNvSpPr txBox=\"1\"/><wps:spPr><a:xfrm{}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
             <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{fill}{line}</wps:spPr>\
             <wps:txbx><w:txbxContent>{content}</w:txbxContent></wps:txbx>\
             <wps:bodyPr rot=\"0\" vert=\"horz\" wrap=\"square\" lIns=\"{}\" tIns=\"{}\" rIns=\"{}\" bIns=\"{}\" anchor=\"t\" anchorCtr=\"0\">{fit}</wps:bodyPr>\
             </wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing>",
            anchor_open(text_box.wrap, z),
            position_xml("wp:positionH", &text_box.horizontal),
            position_xml("wp:positionV", &text_box.vertical),
            wrap_xml(text_box.wrap),
            rotation_attribute(text_box.rotation),
            twips_to_emu(m.left),
            twips_to_emu(m.top),
            twips_to_emu(m.right),
            twips_to_emu(m.bottom),
        );
        let fallback = vml_text_box(text_box, id, z, &content);
        format!(
            "<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\">{shape}</mc:Choice>\
             <mc:Fallback><w:pict>{fallback}</w:pict></mc:Fallback></mc:AlternateContent></w:r>"
        )
    }
    /// the drawing xml as it was read, when the picture is unchanged and
    /// the relationship it names still points at its bytes from here.
    fn reusable_original(&self, image: &Image, story: &StoryOut) -> Option<String> {
        if !self.native {
            return None;
        }
        let original = image.original.as_ref()?;
        if original.as_read != image.geometry() {
            return None;
        }
        let path = self.media_paths.get(&image.media_id)?;
        let points_here = story.rels.iter().any(|rel| {
            !rel.external
                && crate::package::resolve_target(&story.part, &rel.target)
                    .eq_ignore_ascii_case(path)
                && original.xml.contains(&format!("=\"{}\"", rel.id))
        });
        points_here.then(|| original.xml.clone())
    }
}

fn position_xml(name: &str, position: &FloatPosition) -> String {
    let inner = match &position.align {
        Some(align) => format!("<wp:align>{}</wp:align>", crate::xml::escape_text(align)),
        None => format!("<wp:posOffset>{}</wp:posOffset>", position.emu()),
    };
    let vertical = name == "wp:positionV";
    format!(
        "<{name} relativeFrom=\"{}\">{inner}</{name}>",
        docx_relative(vertical, &position.relative_to)
    )
}

fn wrap_xml(wrap: Wrap) -> String {
    // tight and through wrapping need an outline; a rectangle is what
    // word itself starts from
    let polygon = "<wp:wrapPolygon edited=\"0\"><wp:start x=\"0\" y=\"0\"/><wp:lineTo x=\"0\" y=\"21600\"/>\
                   <wp:lineTo x=\"21600\" y=\"21600\"/><wp:lineTo x=\"21600\" y=\"0\"/><wp:lineTo x=\"0\" y=\"0\"/></wp:wrapPolygon>";
    match wrap {
        Wrap::Square => "<wp:wrapSquare wrapText=\"bothSides\"/>".into(),
        Wrap::Tight => format!("<wp:wrapTight wrapText=\"bothSides\">{polygon}</wp:wrapTight>"),
        Wrap::Through => {
            format!("<wp:wrapThrough wrapText=\"bothSides\">{polygon}</wp:wrapThrough>")
        }
        Wrap::TopAndBottom => "<wp:wrapTopAndBottom/>".into(),
        Wrap::BehindText | Wrap::InFrontOfText => "<wp:wrapNone/>".into(),
    }
}

/// the opening tag of a floating frame.
fn anchor_open(wrap: Wrap, z_order: i64) -> String {
    let behind = if wrap == Wrap::BehindText { "1" } else { "0" };
    format!(
        "<wp:anchor distT=\"0\" distB=\"0\" distL=\"114300\" distR=\"114300\" simplePos=\"0\" \
         relativeHeight=\"{}\" behindDoc=\"{behind}\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\">",
        z_order.max(0)
    )
}

fn rotation_attribute(rotation: Option<f64>) -> String {
    match rotation {
        Some(degrees) if degrees != 0.0 => {
            format!(" rot=\"{}\"", (degrees * 60000.0).round() as i64)
        }
        _ => String::new(),
    }
}

/// word accepts only some anchors per direction; the nearest one is used.
pub(crate) fn docx_relative(vertical: bool, relative_to: &str) -> &'static str {
    match (vertical, relative_to) {
        (_, "page") => "page",
        (_, "margin") => "margin",
        (false, "character") => "character",
        (false, "leftMargin") => "leftMargin",
        (false, "rightMargin") => "rightMargin",
        (false, _) => "column",
        (true, "line") => "line",
        (true, "topMargin") => "topMargin",
        (true, "bottomMargin") => "bottomMargin",
        (true, _) => "paragraph",
    }
}

/// the vml version of a text box, which word 2007 and older read.
fn vml_text_box(text_box: &TextBox, id: u32, z: i64, content: &str) -> String {
    let points = |twips: i32| format!("{}pt", f64::from(twips) / 20.0);
    let relative = |vertical: bool, position: &FloatPosition| match docx_relative(
        vertical,
        &position.relative_to,
    ) {
        "column" | "paragraph" => "text",
        "character" => "char",
        other => other,
    };
    let z = if text_box.wrap == Wrap::BehindText {
        -z
    } else {
        z
    };
    let mut style = format!(
        "position:absolute;margin-left:{};margin-top:{};width:{};height:{};z-index:{z};\
         mso-position-horizontal-relative:{};mso-position-vertical-relative:{}",
        points(text_box.horizontal.offset),
        points(text_box.vertical.offset),
        points(text_box.width),
        points(text_box.height),
        relative(false, &text_box.horizontal),
        relative(true, &text_box.vertical),
    );
    if let Some(rotation) = text_box.rotation.filter(|r| *r != 0.0) {
        style.push_str(&format!(";rotation:{rotation}"));
    }
    if text_box.auto_height {
        style.push_str(";mso-fit-shape-to-text:t");
    }
    let fill = match &text_box.fill {
        Some(hex) => format!(" fillcolor=\"#{}\"", escape_attribute(hex)),
        None => " filled=\"f\"".into(),
    };
    let stroke = match &text_box.border {
        Some(border) if border.style != "none" && border.style != "nil" => format!(
            " strokecolor=\"#{}\" strokeweight=\"{}pt\"",
            escape_attribute(border.color.as_deref().unwrap_or("000000")),
            f64::from(border.size) / 8.0
        ),
        _ => " stroked=\"f\"".into(),
    };
    let m = &text_box.margins;
    let inset = format!(
        "{},{},{},{}",
        points(m.left),
        points(m.top),
        points(m.right),
        points(m.bottom)
    );
    let wrap = match text_box.wrap {
        Wrap::Square => "square",
        Wrap::Tight => "tight",
        Wrap::Through => "through",
        Wrap::TopAndBottom => "topAndBottom",
        Wrap::BehindText | Wrap::InFrontOfText => "none",
    };
    format!(
        "<v:rect id=\"Text Box {id}\" o:spid=\"_x0000_s{}\" style=\"{style}\"{fill}{stroke}>\
         <v:textbox inset=\"{inset}\"><w:txbxContent>{content}</w:txbxContent></v:textbox>\
         <w10:wrap type=\"{wrap}\"/></v:rect>",
        1024 + id
    )
}
