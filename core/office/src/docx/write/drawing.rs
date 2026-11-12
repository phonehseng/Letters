//! pictures as `w:drawing` xml.

use crate::model::{FloatPosition, Image, Placement, Wrap};
use crate::units::twips_to_emu;
use crate::xml::escape_attribute;

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
        let cx = twips_to_emu(image.width).max(1);
        let cy = twips_to_emu(image.height).max(1);
        let name = escape_attribute(image.name.as_deref().unwrap_or(&format!("Picture {id}")));
        let description = escape_attribute(image.description.as_deref().unwrap_or(""));
        let graphic = format!(
            "<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
             <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
             <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
             <pic:nvPicPr><pic:cNvPr id=\"0\" name=\"{name}\"/><pic:cNvPicPr/></pic:nvPicPr>\
             <pic:blipFill><a:blip r:embed=\"{rel}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
             <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
             <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\
             </pic:pic></a:graphicData></a:graphic>"
        );
        let doc_pr = format!("<wp:docPr id=\"{id}\" name=\"{name}\" descr=\"{description}\"/>");
        let extent = format!("<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>");
        let frame = match &image.placement {
            Placement::Inline => format!(
                "<wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">{extent}{doc_pr}\
                 <wp:cNvGraphicFramePr/>{graphic}</wp:inline>"
            ),
            Placement::Floating {
                horizontal,
                vertical,
                wrap,
            } => {
                let behind = if *wrap == Wrap::BehindText { "1" } else { "0" };
                format!(
                    "<wp:anchor distT=\"0\" distB=\"0\" distL=\"114300\" distR=\"114300\" simplePos=\"0\" \
                     relativeHeight=\"{}\" behindDoc=\"{behind}\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\">\
                     <wp:simplePos x=\"0\" y=\"0\"/>{}{}{extent}{}{doc_pr}<wp:cNvGraphicFramePr/>{graphic}</wp:anchor>",
                    251_658_240 + id,
                    position_xml("wp:positionH", horizontal),
                    position_xml("wp:positionV", vertical),
                    wrap_xml(*wrap),
                )
            }
        };
        format!("<w:drawing>{frame}</w:drawing>")
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
        None => format!(
            "<wp:posOffset>{}</wp:posOffset>",
            twips_to_emu(position.offset)
        ),
    };
    format!(
        "<{name} relativeFrom=\"{}\">{inner}</{name}>",
        escape_attribute(&position.relative_to)
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
