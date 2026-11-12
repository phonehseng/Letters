//! drawings word positions on the page: shapes (`wps:wsp`), groups
//! (`wpg:wgp`) and text boxes, from word 2010's drawingml with the vml copy
//! beside it, or from older files that only have vml (see `vml`).
//!
//! every object keeps the xml it was read from (`OriginalObject`), so an
//! untouched object is written back byte for byte. an object the model
//! cannot describe (a chart, smartart, a drawing canvas, ink) is not read
//! here and stays preserved where it is.

use crate::docx::drawingml::{
    read_fill, read_geometry, read_stroke, style_fill, style_stroke, StyleRef,
};
use crate::model::{
    remember_original, Block, Border, BoxMargins, ConnectionEnd, Connector, Crop, DrawingPreserved,
    FloatPosition, Geometry, Group, GroupChild, Image, Placement, Shape, ShapeText, SourceFormat,
    TextBox, TextChain, VerticalAlign, Wrap,
};
use crate::units::emu_to_twips;
use crate::xml::Element;

use super::drawing::floating_placement;
use super::{DocxReader, Story};

pub(super) const SHAPE_URI: &str =
    "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
pub(super) const GROUP_URI: &str =
    "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";

/// where an object sits and how big it is: from the anchor for an object
/// on the page, from its `a:xfrm` for an object inside a group.
#[derive(Clone, Debug)]
pub(super) struct Frame {
    pub horizontal: FloatPosition,
    pub vertical: FloatPosition,
    pub cx: i64,
    pub cy: i64,
    pub wrap: Wrap,
    pub z_order: Option<i64>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub title: Option<String>,
}

fn emu(element: &Element, name: &str) -> Option<i64> {
    element.attr(name)?.trim().parse().ok()
}

fn on(element: &Element, name: &str) -> bool {
    matches!(element.attr(name), Some("1" | "true"))
}

/// a group child's frame from its own `a:xfrm`, in the group's child
/// coordinates.
fn child_frame(xfrm: Option<&Element>, names: Option<&Element>) -> Frame {
    let off = xfrm.and_then(|x| x.child("a:off"));
    let ext = xfrm.and_then(|x| x.child("a:ext"));
    let x = off.and_then(|o| emu(o, "x")).unwrap_or(0);
    let y = off.and_then(|o| emu(o, "y")).unwrap_or(0);
    let position = |value: i64| FloatPosition {
        relative_to: "group".into(),
        offset: emu_to_twips(value),
        align: None,
        offset_emu: Some(value),
    };
    Frame {
        horizontal: position(x),
        vertical: position(y),
        cx: ext.and_then(|e| emu(e, "cx")).unwrap_or(0),
        cy: ext.and_then(|e| emu(e, "cy")).unwrap_or(0),
        wrap: Wrap::InFrontOfText,
        z_order: None,
        name: names.and_then(|n| n.attr("name")).map(str::to_string),
        description: names.and_then(|n| n.attr("descr")).map(str::to_string),
        title: names.and_then(|n| n.attr("title")).map(str::to_string),
    }
}

/// rotation and flips from an `a:xfrm`.
fn turn(xfrm: Option<&Element>) -> (Option<f64>, bool, bool) {
    let Some(xfrm) = xfrm else {
        return (None, false, false);
    };
    let rotation = emu(xfrm, "rot")
        .map(|rot| rot as f64 / 60000.0)
        .filter(|degrees| *degrees != 0.0);
    (rotation, on(xfrm, "flipH"), on(xfrm, "flipV"))
}

/// shape property children the model reads; the others are preserved.
const MODELLED_PROPERTIES: &[&str] = &[
    "a:xfrm",
    "a:prstGeom",
    "a:custGeom",
    "a:noFill",
    "a:solidFill",
    "a:gradFill",
    "a:blipFill",
    "a:pattFill",
    "a:ln",
];

impl DocxReader<'_> {
    /// a drawing in a run (`mc:AlternateContent`, `w:drawing` or a vml
    /// `w:pict`) as a block, when it is a floating shape, group or text box.
    pub(super) fn read_object(&mut self, element: &Element, story: &mut Story) -> Option<Block> {
        let mut block = match element.name.as_str() {
            "mc:AlternateContent" => {
                let from_choice = element
                    .children_named("mc:Choice")
                    .filter_map(|choice| choice.child("w:drawing"))
                    .find_map(|drawing| self.read_drawing_object(drawing, story));
                from_choice.or_else(|| {
                    let pict = element.child("mc:Fallback")?.child("w:pict")?;
                    self.read_vml(pict, story)
                })?
            }
            "w:drawing" => self.read_drawing_object(element, story)?,
            "w:pict" => self.read_vml(element, story)?,
            _ => return None,
        };
        let xml = story.keep(element);
        match &mut block {
            Block::Shape(shape) => remember_original(shape, SourceFormat::Docx, xml),
            Block::Group(group) => remember_original(group, SourceFormat::Docx, xml),
            Block::TextBox(text_box) => remember_original(text_box, SourceFormat::Docx, xml),
            _ => {}
        }
        Some(block)
    }

    fn read_drawing_object(&mut self, drawing: &Element, story: &mut Story) -> Option<Block> {
        let anchor = drawing.child("wp:anchor")?;
        let data = anchor.child("a:graphic")?.child("a:graphicData")?;
        let Placement::Floating {
            horizontal,
            vertical,
            wrap,
        } = floating_placement(anchor)?
        else {
            return None;
        };
        let extent = anchor.child("wp:extent")?;
        let doc_pr = anchor.child("wp:docPr");
        let frame = Frame {
            horizontal,
            vertical,
            cx: emu(extent, "cx")?,
            cy: emu(extent, "cy")?,
            wrap,
            z_order: emu(anchor, "relativeHeight"),
            name: doc_pr.and_then(|pr| pr.attr("name")).map(str::to_string),
            description: doc_pr.and_then(|pr| pr.attr("descr")).map(str::to_string),
            title: doc_pr.and_then(|pr| pr.attr("title")).map(str::to_string),
        };
        match data.attr("uri")? {
            SHAPE_URI => {
                let child = self.read_wsp(data.child("wps:wsp")?, frame, story)?;
                Some(match child {
                    GroupChild::Shape(shape) => Block::Shape(shape),
                    GroupChild::TextBox(text_box) => Block::TextBox(text_box),
                    _ => return None,
                })
            }
            GROUP_URI => {
                let group = self.read_group(data.child("wpg:wgp")?, frame, story)?;
                Some(Block::Group(group))
            }
            _ => None,
        }
    }

    /// a `wps:wsp`: a text box when it is one in word's sense (a plain
    /// rectangle holding text, or a linked box), a shape otherwise.
    pub(super) fn read_wsp(
        &mut self,
        wsp: &Element,
        frame: Frame,
        story: &mut Story,
    ) -> Option<GroupChild> {
        let properties = wsp.child("wps:spPr")?;
        let geometry = read_geometry(properties, (frame.cx, frame.cy))?;
        let palette = self.palette.clone();
        let style_element = wsp.child("wps:style");
        let style = style_element
            .map(|style| StyleRef::read(style, &palette))
            .unwrap_or_default();
        let placeholder = style
            .fill
            .as_ref()
            .and_then(|(_, colour)| colour.as_ref().map(|c| c.0.clone()));
        let fill = {
            let mut picture = |id: &str| self.media_for(story, id);
            read_fill(properties, &palette, placeholder.as_deref(), &mut picture)
        }
        .unwrap_or_else(|| style_fill(&style));
        let stroke = match read_stroke(properties, &palette, &style) {
            Ok(stroke) => stroke,
            Err(()) => style_stroke(&palette, &style),
        };
        let (rotation, flip_horizontal, flip_vertical) = turn(properties.child("a:xfrm"));
        let mut preserved = DrawingPreserved {
            format: Some(crate::model::SourceFormat::Docx),
            ..Default::default()
        };
        for child in properties.elements() {
            if !MODELLED_PROPERTIES.contains(&child.name.as_str()) {
                preserved.children.push(story.keep(child));
            }
        }
        preserved.style = style_element.map(|style| story.keep(style));
        let body = wsp.child("wps:bodyPr");
        let margins = body_margins(body);
        let vertical_align = body.and_then(|b| match b.attr("anchor") {
            Some("ctr") => Some(VerticalAlign::Center),
            Some("b") => Some(VerticalAlign::Bottom),
            _ => None,
        });
        let auto_fit = body.is_some_and(|b| b.child("a:spAutoFit").is_some());
        let linked = wsp.child("wps:linkedTxbx");
        let text_element = wsp.child("wps:txbx");
        let blocks = match text_element.and_then(|t| t.child("w:txbxContent")) {
            Some(content) => {
                let mut ignored_sections = Vec::new();
                Some(self.read_blocks(content, story, &mut ignored_sections))
            }
            None => None,
        };
        let chain = match (linked, text_element) {
            (Some(linked), _) => Some(TextChain {
                id: linked.attr("id")?.to_string(),
                sequence: linked.attr("seq").and_then(|s| s.parse().ok()).unwrap_or(1),
            }),
            (None, Some(text)) => text.attr("id").map(|id| TextChain {
                id: id.to_string(),
                sequence: 0,
            }),
            _ => None,
        };
        let marked_text_box = wsp.child("wps:cNvSpPr").is_some_and(|pr| on(pr, "txBox"));
        let plain_box = matches!(&geometry, Geometry::Preset { name, adjust } if name == "rect" && adjust.is_empty())
            && matches!(
                &fill,
                crate::model::Fill::None | crate::model::Fill::Solid { opacity: None, .. }
            )
            && stroke.as_ref().is_none_or(|s| {
                s.dash == "solid" && s.head.is_none() && s.tail.is_none() && s.opacity.is_none()
            })
            && !flip_horizontal
            && !flip_vertical;
        if (marked_text_box || chain.is_some()) && plain_box {
            let border = stroke.as_ref().map(|stroke| Border {
                style: "single".into(),
                size: ((stroke.width_emu() * 8) / 12700).max(1) as u32,
                color: Some(stroke.color.clone()),
            });
            let mut text_box = TextBox::new(
                0,
                0,
                emu_to_twips(frame.cx),
                emu_to_twips(frame.cy),
                blocks.unwrap_or_default(),
            );
            text_box.name = frame.name;
            text_box.horizontal = frame.horizontal;
            text_box.vertical = frame.vertical;
            text_box.width_emu = Some(frame.cx);
            text_box.height_emu = Some(frame.cy);
            text_box.auto_height = auto_fit;
            text_box.wrap = frame.wrap;
            text_box.z_order = frame.z_order;
            text_box.rotation = rotation;
            text_box.margins = margins;
            text_box.border = border;
            text_box.fill = match &fill {
                crate::model::Fill::Solid { color, .. } => Some(color.clone()),
                _ => None,
            };
            text_box.vertical_align = vertical_align;
            text_box.chain = chain;
            text_box.preserved = preserved;
            return Some(GroupChild::TextBox(text_box));
        }
        let connector = wsp.child("wps:cNvCnPr").map(|connection| {
            let end = |name: &str| {
                let element = connection.child(name)?;
                Some(ConnectionEnd {
                    object: element.attr("id")?.to_string(),
                    site: element
                        .attr("idx")
                        .and_then(|i| i.parse().ok())
                        .unwrap_or(0),
                })
            };
            Connector {
                kind: match &geometry {
                    Geometry::Preset { name, .. } if name.starts_with("bent") => "elbow",
                    Geometry::Preset { name, .. } if name.starts_with("curved") => "curved",
                    _ => "straight",
                }
                .into(),
                start: end("a:stCxn"),
                end: end("a:endCxn"),
            }
        });
        let text = blocks.map(|blocks| ShapeText {
            blocks,
            margins,
            vertical_align,
            auto_fit,
            color: style.font_colour.clone(),
        });
        let names = wsp.child("wps:cNvPr");
        Some(GroupChild::Shape(Shape {
            name: frame
                .name
                .or_else(|| names.and_then(|n| n.attr("name")).map(str::to_string)),
            description: frame.description,
            title: frame.title,
            horizontal: frame.horizontal,
            vertical: frame.vertical,
            width: emu_to_twips(frame.cx),
            height: emu_to_twips(frame.cy),
            width_emu: Some(frame.cx),
            height_emu: Some(frame.cy),
            wrap: frame.wrap,
            z_order: frame.z_order,
            rotation,
            flip_horizontal,
            flip_vertical,
            geometry,
            fill,
            stroke,
            text,
            connector,
            preserved,
            anchor: None,
            original: None,
        }))
    }

    /// a group (`wpg:wgp`, or `wpg:grpSp` inside one). `None` when any of
    /// its members is something the model cannot hold, so the whole group
    /// stays preserved.
    fn read_group(&mut self, element: &Element, frame: Frame, story: &mut Story) -> Option<Group> {
        let properties = element.child("wpg:grpSpPr");
        let xfrm = properties.and_then(|p| p.child("a:xfrm"));
        let child_offset = xfrm.and_then(|x| x.child("a:chOff"));
        let child_extent = xfrm.and_then(|x| x.child("a:chExt"));
        let (rotation, flip_horizontal, flip_vertical) = turn(xfrm);
        let mut children = Vec::new();
        for child in element.elements() {
            match child.name.as_str() {
                "wpg:cNvPr" | "wpg:cNvGrpSpPr" | "wpg:grpSpPr" => {}
                "wps:wsp" => {
                    let names = child.child("wps:cNvPr");
                    let frame = child_frame(
                        child.child("wps:spPr").and_then(|p| p.child("a:xfrm")),
                        names,
                    );
                    children.push(self.read_wsp(child, frame, story)?);
                }
                "wpg:grpSp" => {
                    let names = child.child("wpg:cNvPr");
                    let frame = child_frame(
                        child.child("wpg:grpSpPr").and_then(|p| p.child("a:xfrm")),
                        names,
                    );
                    children.push(GroupChild::Group(self.read_group(child, frame, story)?));
                }
                "pic:pic" => {
                    children.push(GroupChild::Picture(self.read_group_picture(child, story)?))
                }
                _ => return None,
            }
        }
        let mut preserved = DrawingPreserved::default();
        if let Some(properties) = properties {
            for child in properties.elements() {
                if child.name != "a:xfrm" {
                    preserved.format = Some(SourceFormat::Docx);
                    preserved.children.push(story.keep(child));
                }
            }
        }
        Some(Group {
            name: frame.name,
            description: frame.description,
            title: frame.title,
            horizontal: frame.horizontal,
            vertical: frame.vertical,
            width: emu_to_twips(frame.cx),
            height: emu_to_twips(frame.cy),
            width_emu: Some(frame.cx),
            height_emu: Some(frame.cy),
            wrap: frame.wrap,
            z_order: frame.z_order,
            rotation,
            flip_horizontal,
            flip_vertical,
            child_offset_x: child_offset.and_then(|o| emu(o, "x")).unwrap_or(0),
            child_offset_y: child_offset.and_then(|o| emu(o, "y")).unwrap_or(0),
            child_width: child_extent.and_then(|e| emu(e, "cx")).unwrap_or(frame.cx),
            child_height: child_extent.and_then(|e| emu(e, "cy")).unwrap_or(frame.cy),
            children,
            preserved,
            anchor: None,
            original: None,
        })
    }

    fn read_group_picture(&mut self, picture: &Element, story: &mut Story) -> Option<Image> {
        let fill = picture.child("pic:blipFill")?;
        let media_id = self.media_for(story, fill.child("a:blip")?.attr("r:embed")?)?;
        let properties = picture.child("pic:spPr");
        let xfrm = properties.and_then(|p| p.child("a:xfrm"));
        let names = picture
            .child("pic:nvPicPr")
            .and_then(|n| n.child("pic:cNvPr"));
        let frame = child_frame(xfrm, names);
        let (rotation, _, _) = turn(xfrm);
        let crop = fill.child("a:srcRect").map(|rect| {
            let side = |name: &str| rect.attr(name).and_then(|v| v.parse().ok()).unwrap_or(0);
            Crop {
                left: side("l"),
                top: side("t"),
                right: side("r"),
                bottom: side("b"),
            }
        });
        Some(Image {
            media_id,
            width: emu_to_twips(frame.cx),
            height: emu_to_twips(frame.cy),
            width_emu: Some(frame.cx),
            height_emu: Some(frame.cy),
            description: frame.description,
            name: frame.name,
            placement: Placement::Floating {
                horizontal: frame.horizontal,
                vertical: frame.vertical,
                wrap: Wrap::InFrontOfText,
            },
            rotation,
            crop: crop.filter(|c| *c != Crop::default()),
            ..Default::default()
        })
    }
}

/// the inner margins of `wps:bodyPr`, word's defaults where it says none.
pub(super) fn body_margins(body: Option<&Element>) -> BoxMargins {
    let inset =
        |name: &str, default: i64| emu_to_twips(body.and_then(|b| emu(b, name)).unwrap_or(default));
    BoxMargins {
        left: inset("lIns", 91440),
        top: inset("tIns", 45720),
        right: inset("rIns", 91440),
        bottom: inset("bIns", 45720),
    }
}
