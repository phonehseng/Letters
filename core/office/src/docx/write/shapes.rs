//! shapes and groups as word writes them: a drawingml shape (`wps:wsp`)
//! or group (`wpg:wgp`) for word 2010 and later, with a vml copy for older
//! versions, in one compatibility wrapper.
//!
//! an object that has not changed since it was read is written back from
//! its original xml instead (see `model::reusable_original`).

use crate::docx::drawingml::{fill_xml, geometry_xml, stroke_xml};
use crate::model::{
    reusable_original, Block, Fill, Geometry, Group, GroupChild, Image, PathCommand, Shape,
    ShapeText, SourceFormat, VerticalAlign, Wrap,
};
use crate::units::twips_to_emu;
use crate::xml::{escape_attribute, XmlWriter};

use super::drawing::{anchor_open, position_xml, wrap_xml};
use super::{DocxWriter, StoryOut};

const SHAPE_URI: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const GROUP_URI: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
const A_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// a shape's text colour without a style of its own: word's own text
/// boxes carry this style reference, which leaves fill and line to the
/// shape and only says the text colour.
fn font_style(color: Option<&str>) -> String {
    let font = match color {
        Some(hex) => format!("<a:srgbClr val=\"{}\"/>", escape_attribute(hex)),
        None => "<a:schemeClr val=\"tx1\"/>".into(),
    };
    format!(
        "<wps:style><a:lnRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:lnRef>\
         <a:fillRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:fillRef>\
         <a:effectRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:effectRef>\
         <a:fontRef idx=\"minor\">{font}</a:fontRef></wps:style>"
    )
}

/// emu as the vml points word writes in a style attribute.
fn points(emu: i64) -> String {
    let points = emu as f64 / 12700.0;
    let rounded = (points * 100.0).round() / 100.0;
    format!("{rounded}pt")
}

impl DocxWriter<'_> {
    /// the content of the run that holds a text box, shape or group.
    pub(crate) fn object_xml(&mut self, block: &Block, story: &mut StoryOut) -> String {
        if self.native {
            let original = match block {
                Block::TextBox(object) => reusable_original(object, SourceFormat::Docx),
                Block::Shape(object) => reusable_original(object, SourceFormat::Docx),
                Block::Group(object) => reusable_original(object, SourceFormat::Docx),
                _ => None,
            };
            if let Some(xml) = original {
                return xml.to_string();
            }
        }
        match block {
            Block::TextBox(text_box) => self.text_box_content(text_box, story),
            Block::Shape(shape) => self.shape_content(shape, story),
            Block::Group(group) => self.group_content(group, story),
            _ => String::new(),
        }
    }

    fn next_id(&mut self) -> u32 {
        self.next_drawing_id += 1;
        self.next_drawing_id
    }

    /// `wp:anchor` around a graphic.
    fn anchored(&mut self, id: u32, frame: &AnchorFrame, uri: &str, graphic: &str) -> String {
        let mut doc_pr = format!(
            "<wp:docPr id=\"{id}\" name=\"{}\"",
            escape_attribute(&frame.name)
        );
        if let Some(description) = &frame.description {
            doc_pr.push_str(&format!(" descr=\"{}\"", escape_attribute(description)));
        }
        if let Some(title) = &frame.title {
            doc_pr.push_str(&format!(" title=\"{}\"", escape_attribute(title)));
        }
        doc_pr.push_str("/>");
        format!(
            "<w:drawing>{}<wp:simplePos x=\"0\" y=\"0\"/>{}{}<wp:extent cx=\"{}\" cy=\"{}\"/>\
             <wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>{}{doc_pr}<wp:cNvGraphicFramePr/>\
             <a:graphic xmlns:a=\"{A_NAMESPACE}\"><a:graphicData uri=\"{uri}\">{graphic}</a:graphicData></a:graphic>\
             </wp:anchor></w:drawing>",
            anchor_open(frame.wrap, frame.z),
            position_xml("wp:positionH", &frame.horizontal),
            position_xml("wp:positionV", &frame.vertical),
            frame.cx.max(1),
            frame.cy.max(1),
            wrap_xml(frame.wrap),
        )
    }

    fn shape_content(&mut self, shape: &Shape, story: &mut StoryOut) -> String {
        let id = self.next_id();
        let (cx, cy) = shape.size_emu();
        let frame = AnchorFrame {
            horizontal: shape.horizontal.clone(),
            vertical: shape.vertical.clone(),
            cx,
            cy,
            wrap: shape.wrap,
            z: shape.z_order.unwrap_or(251_658_240 + i64::from(id)),
            name: shape.name.clone().unwrap_or_else(|| format!("Shape {id}")),
            description: shape.description.clone(),
            title: shape.title.clone(),
        };
        let wsp = self.wsp_xml(shape, None, false, story);
        let drawing = self.anchored(id, &frame, SHAPE_URI, &wsp);
        let fallback = self.vml_shape(shape, &frame, id, story);
        format!(
            "<mc:AlternateContent><mc:Choice Requires=\"wps\">{drawing}</mc:Choice>\
             <mc:Fallback><w:pict>{fallback}</w:pict></mc:Fallback></mc:AlternateContent>"
        )
    }

    /// a `wps:wsp`. `in_group` gives the shape's id when it is a member of
    /// a group (its position is then in the group's child coordinates).
    fn wsp_xml(
        &mut self,
        shape: &Shape,
        in_group: Option<u32>,
        text_box: bool,
        story: &mut StoryOut,
    ) -> String {
        let (cx, cy) = shape.size_emu();
        let mut xml = String::from("<wps:wsp>");
        if let Some(id) = in_group {
            xml.push_str(&format!(
                "<wps:cNvPr id=\"{id}\" name=\"{}\"/>",
                escape_attribute(shape.name.as_deref().unwrap_or("Shape"))
            ));
        }
        if shape.connector.is_some() {
            xml.push_str("<wps:cNvCnPr/>");
        } else if text_box {
            xml.push_str("<wps:cNvSpPr txBox=\"1\"/>");
        } else {
            xml.push_str("<wps:cNvSpPr/>");
        }
        let (x, y) = if in_group.is_some() {
            (shape.horizontal.emu(), shape.vertical.emu())
        } else {
            (0, 0)
        };
        let mut turn = String::new();
        if let Some(rotation) = shape.rotation.filter(|r| *r != 0.0) {
            turn.push_str(&format!(" rot=\"{}\"", (rotation * 60000.0).round() as i64));
        }
        if shape.flip_horizontal {
            turn.push_str(" flipH=\"1\"");
        }
        if shape.flip_vertical {
            turn.push_str(" flipV=\"1\"");
        }
        let fill = {
            let media_paths = &self.media_paths;
            let mut picture = |media_id: &str| {
                let path = media_paths.get(media_id)?.clone();
                Some(story.media_rel(media_id, &path))
            };
            fill_xml(&shape.fill, &mut picture)
        };
        xml.push_str(&format!(
            "<wps:spPr><a:xfrm{turn}><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>{}{fill}{}",
            cx.max(1),
            cy.max(1),
            geometry_xml(&shape.geometry),
            stroke_xml(shape.stroke.as_ref()),
        ));
        let preserved = shape
            .preserved
            .in_format(SourceFormat::Docx)
            .filter(|_| self.native);
        if let Some(preserved) = preserved {
            for child in &preserved.children {
                xml.push_str(child);
            }
        }
        xml.push_str("</wps:spPr>");
        let text_colour = shape.text.as_ref().and_then(|text| text.color.as_deref());
        match preserved.and_then(|p| p.style.as_ref()) {
            Some(style) => xml.push_str(style),
            None => xml.push_str(&font_style(text_colour)),
        }
        if let Some(text) = &shape.text {
            let mut content = XmlWriter::new();
            self.write_blocks(&text.blocks, story, &mut content, None, false);
            if !ends_with_paragraph(&text.blocks) {
                content.raw("<w:p/>");
            }
            xml.push_str(&format!(
                "<wps:txbx><w:txbxContent>{}</w:txbxContent></wps:txbx>",
                content.finish()
            ));
        }
        xml.push_str(&body_xml(shape.text.as_ref()));
        xml.push_str("</wps:wsp>");
        xml
    }

    fn group_content(&mut self, group: &Group, story: &mut StoryOut) -> String {
        let id = self.next_id();
        let (cx, cy) = group.size_emu();
        let frame = AnchorFrame {
            horizontal: group.horizontal.clone(),
            vertical: group.vertical.clone(),
            cx,
            cy,
            wrap: group.wrap,
            z: group.z_order.unwrap_or(251_658_240 + i64::from(id)),
            name: group.name.clone().unwrap_or_else(|| format!("Group {id}")),
            description: group.description.clone(),
            title: group.title.clone(),
        };
        let mut xml = String::from("<wpg:wgp><wpg:cNvGrpSpPr/>");
        xml.push_str(&self.group_properties(group, (0, 0, cx, cy)));
        for child in &group.children {
            xml.push_str(&self.group_child_xml(child, story));
        }
        xml.push_str("</wpg:wgp>");
        let drawing = self.anchored(id, &frame, GROUP_URI, &xml);
        let fallback = self.vml_group(group, &frame, story);
        format!(
            "<mc:AlternateContent><mc:Choice Requires=\"wpg\">{drawing}</mc:Choice>\
             <mc:Fallback><w:pict>{fallback}</w:pict></mc:Fallback></mc:AlternateContent>"
        )
    }

    fn group_properties(&self, group: &Group, (x, y, cx, cy): (i64, i64, i64, i64)) -> String {
        let mut turn = String::new();
        if let Some(rotation) = group.rotation.filter(|r| *r != 0.0) {
            turn.push_str(&format!(" rot=\"{}\"", (rotation * 60000.0).round() as i64));
        }
        if group.flip_horizontal {
            turn.push_str(" flipH=\"1\"");
        }
        if group.flip_vertical {
            turn.push_str(" flipV=\"1\"");
        }
        let mut extra = String::new();
        if let Some(preserved) = group
            .preserved
            .in_format(SourceFormat::Docx)
            .filter(|_| self.native)
        {
            for child in &preserved.children {
                extra.push_str(child);
            }
        }
        format!(
            "<wpg:grpSpPr><a:xfrm{turn}><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{}\" cy=\"{}\"/>\
             <a:chOff x=\"{}\" y=\"{}\"/><a:chExt cx=\"{}\" cy=\"{}\"/></a:xfrm>{extra}</wpg:grpSpPr>",
            cx.max(1),
            cy.max(1),
            group.child_offset_x,
            group.child_offset_y,
            group.child_width.max(1),
            group.child_height.max(1),
        )
    }

    fn group_child_xml(&mut self, child: &GroupChild, story: &mut StoryOut) -> String {
        let id = self.next_id();
        match child {
            GroupChild::Shape(shape) => self.wsp_xml(shape, Some(id), false, story),
            GroupChild::TextBox(text_box) => {
                let shape = text_box.as_shape();
                self.wsp_xml(&shape, Some(id), true, story)
            }
            GroupChild::Picture(image) => self.group_picture_xml(image, id, story),
            GroupChild::Group(inner) => {
                let (cx, cy) = inner.size_emu();
                let mut xml = format!(
                    "<wpg:grpSp><wpg:cNvPr id=\"{id}\" name=\"{}\"/><wpg:cNvGrpSpPr/>",
                    escape_attribute(inner.name.as_deref().unwrap_or("Group"))
                );
                xml.push_str(&self.group_properties(
                    inner,
                    (inner.horizontal.emu(), inner.vertical.emu(), cx, cy),
                ));
                for member in &inner.children {
                    xml.push_str(&self.group_child_xml(member, story));
                }
                xml.push_str("</wpg:grpSp>");
                xml
            }
        }
    }

    fn group_picture_xml(&mut self, image: &Image, id: u32, story: &mut StoryOut) -> String {
        let Some(path) = self.media_paths.get(&image.media_id).cloned() else {
            return String::new();
        };
        let rel = story.media_rel(&image.media_id, &path);
        let (cx, cy) = image.size_emu();
        let (x, y) = match &image.placement {
            crate::model::Placement::Floating {
                horizontal,
                vertical,
                ..
            } => (horizontal.emu(), vertical.emu()),
            crate::model::Placement::Inline => (0, 0),
        };
        let crop = image
            .crop
            .map(|c| {
                format!(
                    "<a:srcRect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>",
                    c.left, c.top, c.right, c.bottom
                )
            })
            .unwrap_or_default();
        let rotation = image
            .rotation
            .filter(|r| *r != 0.0)
            .map(|r| format!(" rot=\"{}\"", (r * 60000.0).round() as i64))
            .unwrap_or_default();
        format!(
            "<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
             <pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"{}\" descr=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr>\
             <pic:blipFill><a:blip r:embed=\"{rel}\"/>{crop}<a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
             <pic:spPr><a:xfrm{rotation}><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>\
             <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>",
            escape_attribute(image.name.as_deref().unwrap_or("Picture")),
            escape_attribute(image.description.as_deref().unwrap_or("")),
            cx.max(1),
            cy.max(1),
        )
    }

    // the vml copies --------------------------------------------------------

    /// the vml version of a shape. presets vml has no element for are drawn
    /// as their bounding rectangle (word 2007 and older only).
    fn vml_shape(
        &mut self,
        shape: &Shape,
        frame: &AnchorFrame,
        id: u32,
        story: &mut StoryOut,
    ) -> String {
        let style = vml_style(
            frame,
            shape.rotation,
            shape.flip_horizontal,
            shape.flip_vertical,
            true,
        );
        self.vml_element(shape, &style, id, frame.wrap, story)
    }

    fn vml_element(
        &mut self,
        shape: &Shape,
        style: &str,
        id: u32,
        wrap: Wrap,
        story: &mut StoryOut,
    ) -> String {
        let fill = match &shape.fill {
            Fill::Solid { color, .. } => format!(" fillcolor=\"#{}\"", escape_attribute(color)),
            Fill::Gradient(gradient) => gradient
                .stops
                .first()
                .map(|stop| format!(" fillcolor=\"#{}\"", escape_attribute(&stop.color)))
                .unwrap_or_default(),
            Fill::Picture { .. } => String::new(),
            Fill::None => " filled=\"f\"".into(),
        };
        let stroke = match &shape.stroke {
            Some(stroke) => format!(
                " strokecolor=\"#{}\" strokeweight=\"{}\"",
                escape_attribute(&stroke.color),
                points(stroke.width_emu())
            ),
            None => " stroked=\"f\"".into(),
        };
        let text = match &shape.text {
            Some(text) => {
                let mut content = XmlWriter::new();
                self.write_blocks(&text.blocks, story, &mut content, None, false);
                if !ends_with_paragraph(&text.blocks) {
                    content.raw("<w:p/>");
                }
                format!(
                    "<v:textbox><w:txbxContent>{}</w:txbxContent></v:textbox>",
                    content.finish()
                )
            }
            None => String::new(),
        };
        let wrap = vml_wrap(wrap);
        let spid = format!("_x0000_s{}", 1024 + id);
        let name = escape_attribute(shape.name.as_deref().unwrap_or("Shape"));
        let (element, extra) = match &shape.geometry {
            Geometry::Preset { name, adjust } if name == "roundRect" => {
                let arc = adjust
                    .first()
                    .map(|a| a.value as f64 / 50000.0)
                    .unwrap_or(16667.0 / 50000.0);
                (
                    "v:roundrect",
                    format!(" arcsize=\"{}f\"", (arc * 65536.0).round() as i64),
                )
            }
            Geometry::Preset { name, .. } if name == "ellipse" => ("v:oval", String::new()),
            Geometry::Path { paths } => match paths.first() {
                Some(path) => (
                    "v:shape",
                    format!(
                        " coordsize=\"{},{}\" path=\"{}\"",
                        path.width.round() as i64,
                        path.height.round() as i64,
                        vml_path(&path.commands)
                    ),
                ),
                None => ("v:rect", String::new()),
            },
            _ => ("v:rect", String::new()),
        };
        format!(
            "<{element} id=\"{name}\" o:spid=\"{spid}\" style=\"{style}\"{extra}{fill}{stroke}>{text}{wrap}</{element}>"
        )
    }

    fn vml_group(&mut self, group: &Group, frame: &AnchorFrame, story: &mut StoryOut) -> String {
        let style = vml_style(
            frame,
            group.rotation,
            group.flip_horizontal,
            group.flip_vertical,
            true,
        );
        let body = self.vml_group_body(group, story);
        format!(
            "<v:group id=\"{}\" style=\"{style}\" coordorigin=\"{},{}\" coordsize=\"{},{}\">{body}{}</v:group>",
            escape_attribute(&frame.name),
            group.child_offset_x / 100,
            group.child_offset_y / 100,
            (group.child_width / 100).max(1),
            (group.child_height / 100).max(1),
            vml_wrap(frame.wrap)
        )
    }

    fn vml_group_body(&mut self, group: &Group, story: &mut StoryOut) -> String {
        let mut body = String::new();
        for child in &group.children {
            let id = self.next_id();
            match child {
                GroupChild::Shape(shape) => {
                    let (cx, cy) = shape.size_emu();
                    let style = member_style(shape.horizontal.emu(), shape.vertical.emu(), cx, cy);
                    body.push_str(&self.vml_element(shape, &style, id, Wrap::InFrontOfText, story));
                }
                GroupChild::TextBox(text_box) => {
                    let shape = text_box.as_shape();
                    let (cx, cy) = shape.size_emu();
                    let style = member_style(shape.horizontal.emu(), shape.vertical.emu(), cx, cy);
                    body.push_str(&self.vml_element(
                        &shape,
                        &style,
                        id,
                        Wrap::InFrontOfText,
                        story,
                    ));
                }
                GroupChild::Picture(_) => {}
                GroupChild::Group(inner) => {
                    let (cx, cy) = inner.size_emu();
                    let style = member_style(inner.horizontal.emu(), inner.vertical.emu(), cx, cy);
                    let inner_body = self.vml_group_body(inner, story);
                    body.push_str(&format!(
                        "<v:group style=\"{style}\" coordorigin=\"{},{}\" coordsize=\"{},{}\">{inner_body}</v:group>",
                        inner.child_offset_x / 100,
                        inner.child_offset_y / 100,
                        (inner.child_width / 100).max(1),
                        (inner.child_height / 100).max(1),
                    ));
                }
            }
        }
        body
    }
}

/// what every anchored drawing needs for its `wp:anchor`.
pub(crate) struct AnchorFrame {
    pub horizontal: crate::model::FloatPosition,
    pub vertical: crate::model::FloatPosition,
    pub cx: i64,
    pub cy: i64,
    pub wrap: Wrap,
    pub z: i64,
    pub name: String,
    pub description: Option<String>,
    pub title: Option<String>,
}

fn ends_with_paragraph(blocks: &[Block]) -> bool {
    matches!(
        blocks.last(),
        Some(Block::Paragraph(_)) | Some(Block::Heading(_))
    )
}

/// `wps:bodyPr` for a shape's text (or for a shape without any).
fn body_xml(text: Option<&ShapeText>) -> String {
    let Some(text) = text else {
        return "<wps:bodyPr rot=\"0\" vert=\"horz\" wrap=\"square\" anchor=\"ctr\" anchorCtr=\"0\"><a:noAutofit/></wps:bodyPr>".into();
    };
    let anchor = match text.vertical_align {
        Some(VerticalAlign::Center) => "ctr",
        Some(VerticalAlign::Bottom) => "b",
        _ => "t",
    };
    let m = &text.margins;
    format!(
        "<wps:bodyPr rot=\"0\" vert=\"horz\" wrap=\"square\" lIns=\"{}\" tIns=\"{}\" rIns=\"{}\" bIns=\"{}\" anchor=\"{anchor}\" anchorCtr=\"0\">{}</wps:bodyPr>",
        twips_to_emu(m.left),
        twips_to_emu(m.top),
        twips_to_emu(m.right),
        twips_to_emu(m.bottom),
        if text.auto_fit {
            "<a:spAutoFit/>"
        } else {
            "<a:noAutofit/>"
        }
    )
}

fn vml_relative(vertical: bool, relative_to: &str) -> &'static str {
    match super::drawing::docx_relative(vertical, relative_to) {
        "column" | "paragraph" => "text",
        "character" => "char",
        "leftMargin" => "left-margin-area",
        "rightMargin" => "right-margin-area",
        "topMargin" => "top-margin-area",
        "bottomMargin" => "bottom-margin-area",
        "line" => "line",
        "page" => "page",
        _ => "margin",
    }
}

fn vml_style(
    frame: &AnchorFrame,
    rotation: Option<f64>,
    flip_x: bool,
    flip_y: bool,
    absolute: bool,
) -> String {
    let z = if frame.wrap == Wrap::BehindText {
        -frame.z
    } else {
        frame.z
    };
    let mut style = format!(
        "position:{};margin-left:{};margin-top:{};width:{};height:{};z-index:{z};\
         mso-position-horizontal-relative:{};mso-position-vertical-relative:{}",
        if absolute { "absolute" } else { "relative" },
        points(frame.horizontal.emu()),
        points(frame.vertical.emu()),
        points(frame.cx),
        points(frame.cy),
        vml_relative(false, &frame.horizontal.relative_to),
        vml_relative(true, &frame.vertical.relative_to),
    );
    if let Some(rotation) = rotation.filter(|r| *r != 0.0) {
        style.push_str(&format!(";rotation:{rotation}"));
    }
    match (flip_x, flip_y) {
        (true, true) => style.push_str(";flip:x y"),
        (true, false) => style.push_str(";flip:x"),
        (false, true) => style.push_str(";flip:y"),
        _ => {}
    }
    style
}

/// a group member's place, in the group's vml coordinates (emu / 100).
fn member_style(x: i64, y: i64, cx: i64, cy: i64) -> String {
    format!(
        "position:absolute;left:{};top:{};width:{};height:{}",
        x / 100,
        y / 100,
        (cx / 100).max(1),
        (cy / 100).max(1)
    )
}

fn vml_wrap(wrap: Wrap) -> String {
    let kind = match wrap {
        Wrap::Square => "square",
        Wrap::Tight => "tight",
        Wrap::Through => "through",
        Wrap::TopAndBottom => "topAndBottom",
        Wrap::BehindText | Wrap::InFrontOfText => return String::new(),
    };
    format!("<w10:wrap type=\"{kind}\"/>")
}

/// a path in vml's syntax: m (move), l (line), c (curve), x (close), e (end).
fn vml_path(commands: &[PathCommand]) -> String {
    let n = |value: f64| (value.round() as i64).to_string();
    let mut path = String::new();
    for command in commands {
        match command {
            PathCommand::Move { x, y } => path.push_str(&format!("m{},{}", n(*x), n(*y))),
            PathCommand::Line { x, y } => path.push_str(&format!("l{},{}", n(*x), n(*y))),
            PathCommand::Cubic {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => path.push_str(&format!(
                "c{},{},{},{},{},{}",
                n(*x1),
                n(*y1),
                n(*x2),
                n(*y2),
                n(*x),
                n(*y)
            )),
            // vml has no quadratic curve; the cubic with the same shape
            PathCommand::Quadratic { x1, y1, x, y } => {
                path.push_str(&format!("qb{},{},{},{}", n(*x1), n(*y1), n(*x), n(*y)))
            }
            PathCommand::Close => path.push('x'),
        }
    }
    path.push('e');
    path
}
