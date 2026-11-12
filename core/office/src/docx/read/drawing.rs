//! pictures (`w:drawing` holding a `pic:pic`). other drawings (charts,
//! shapes, smartart, text boxes) are kept as preserved xml.

use crate::model::{
    Border, BoxMargins, Crop, FloatPosition, Image, Inline, Mark, MediaItem, OriginalDrawing,
    Placement, TextBox, Wrap,
};
use crate::units::emu_to_twips;
use crate::xml::Element;

use super::{DocxReader, Story};
use crate::docx::names::{relationship_kind, KIND_IMAGE};
use crate::docx::rels;

const PICTURE_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";

impl DocxReader<'_> {
    pub(super) fn read_drawing(
        &mut self,
        drawing: &Element,
        story: &mut Story,
        marks: Vec<Mark>,
    ) -> Inline {
        match self.read_picture(drawing, story) {
            Some(mut image) => {
                image.marks = marks;
                Inline::Image(image)
            }
            None => Inline::PreservedRun {
                xml: story.keep(drawing),
                marks,
            },
        }
    }

    fn read_picture(&mut self, drawing: &Element, story: &mut Story) -> Option<Image> {
        let frame = drawing.elements().next()?;
        let floating = match frame.name.as_str() {
            "wp:inline" => false,
            "wp:anchor" => true,
            _ => return None,
        };
        let data = frame.child("a:graphic")?.child("a:graphicData")?;
        if data.attr("uri") != Some(PICTURE_URI) {
            return None;
        }
        let blip = data
            .child("pic:pic")?
            .child("pic:blipFill")?
            .child("a:blip")?;
        let rel_id = blip.attr("r:embed")?;
        let media_id = self.media_for(story, rel_id)?;
        let extent = frame.child("wp:extent")?;
        let doc_pr = frame.child("wp:docPr");
        let placement = if floating {
            floating_placement(frame)?
        } else {
            Placement::Inline
        };
        let (cx, cy) = (emu(extent, "cx")?, emu(extent, "cy")?);
        let picture = data.child("pic:pic")?;
        let shape = picture.child("pic:spPr");
        let mut image = Image {
            media_id,
            width: emu_to_twips(cx),
            height: emu_to_twips(cy),
            width_emu: Some(cx),
            height_emu: Some(cy),
            description: doc_pr.and_then(|pr| pr.attr("descr")).map(str::to_string),
            name: doc_pr.and_then(|pr| pr.attr("name")).map(str::to_string),
            placement,
            z_order: floating.then(|| z_order(frame)).flatten(),
            rotation: shape.and_then(rotation),
            crop: picture.child("pic:blipFill").and_then(crop),
            lock_aspect_ratio: frame
                .child("wp:cNvGraphicFramePr")
                .and_then(|pr| pr.child("a:graphicFrameLocks"))
                .and_then(|locks| locks.attr("noChangeAspect"))
                .map(|value| value == "1" || value == "true"),
            ..Default::default()
        }; // the original xml (with its relationship id, which is then kept)
           // is written back while the geometry stays the same
        image.original = Some(OriginalDrawing {
            xml: story.keep(drawing),
            as_read: image.geometry(),
        });
        Some(image)
    }

    /// the media id for an image relationship, adding the picture to the
    /// media table the first time it is seen.
    fn media_for(&mut self, story: &Story, rel_id: &str) -> Option<String> {
        let rel = story.rel(rel_id)?;
        if rel.external || relationship_kind(&rel.kind) != KIND_IMAGE {
            return None;
        }
        let path = rels::target_path(&story.part, rel);
        if let Some(id) = self.media_by_path.get(&path.to_ascii_lowercase()) {
            return Some(id.clone());
        }
        let part = self.package.get(&path)?;
        let file_name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let mut id = file_name.clone();
        let mut counter = 1;
        while self.doc.media_item(&id).is_some() {
            counter += 1;
            id = format!("{counter}-{file_name}");
        }
        let content_type = self
            .types
            .of(&path)
            .unwrap_or_else(|| MediaItem::content_type_for(&file_name).to_string());
        self.doc.media.push(MediaItem {
            id: id.clone(),
            content_type,
            source_path: Some(part.name.clone()),
            bytes: part.bytes.clone(),
        });
        self.media_by_path
            .insert(path.to_ascii_lowercase(), id.clone());
        self.consumed.insert(path.to_ascii_lowercase());
        Some(id)
    }
}

fn floating_placement(anchor: &Element) -> Option<Placement> {
    let horizontal = position(anchor.child("wp:positionH")?)?;
    let vertical = position(anchor.child("wp:positionV")?)?;
    let behind = anchor
        .attr("behindDoc")
        .is_some_and(|v| v == "1" || v == "true");
    let wrap = anchor
        .elements()
        .find_map(|child| match child.name.as_str() {
            "wp:wrapSquare" => Some(Wrap::Square),
            "wp:wrapTight" => Some(Wrap::Tight),
            "wp:wrapThrough" => Some(Wrap::Through),
            "wp:wrapTopAndBottom" => Some(Wrap::TopAndBottom),
            "wp:wrapNone" if behind => Some(Wrap::BehindText),
            "wp:wrapNone" => Some(Wrap::InFrontOfText),
            _ => None,
        })?;
    Some(Placement::Floating {
        horizontal,
        vertical,
        wrap,
    })
}

fn position(element: &Element) -> Option<FloatPosition> {
    let relative_to = element.attr("relativeFrom")?.to_string();
    let offset = element
        .child("wp:posOffset")
        .and_then(|offset| offset.text().trim().parse::<i64>().ok())
        .map(emu_to_twips)
        .unwrap_or(0);
    let exact = element
        .child("wp:posOffset")
        .and_then(|offset| offset.text().trim().parse::<i64>().ok());
    let align = element.child("wp:align").map(|align| align.text());
    Some(FloatPosition {
        relative_to,
        offset,
        align,
        offset_emu: exact,
    })
}

fn emu(element: &Element, name: &str) -> Option<i64> {
    element.attr(name)?.trim().parse().ok()
}

/// word stacks floating objects by `relativeHeight`.
fn z_order(anchor: &Element) -> Option<i64> {
    emu(anchor, "relativeHeight")
}

/// drawingml rotation is in 60000ths of a degree.
fn rotation(shape: &Element) -> Option<f64> {
    let degrees = emu(shape.child("a:xfrm")?, "rot")? as f64 / 60000.0;
    (degrees != 0.0).then_some(degrees)
}

fn crop(fill: &Element) -> Option<Crop> {
    let rect = fill.child("a:srcRect")?;
    let side = |name: &str| rect.attr(name).and_then(|v| v.parse().ok()).unwrap_or(0);
    let crop = Crop {
        left: side("l"),
        top: side("t"),
        right: side("r"),
        bottom: side("b"),
    };
    (crop != Crop::default()).then_some(crop)
}

const SHAPE_URI: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";

/// the anchor of a text box, found in word's compatibility wrapper or
/// directly in a drawing.
fn text_box_anchor(element: &Element) -> Option<&Element> {
    let drawing = match element.name.as_str() {
        "mc:AlternateContent" => element.child("mc:Choice")?.child("w:drawing")?,
        "w:drawing" => element,
        _ => return None,
    };
    let anchor = drawing.child("wp:anchor")?;
    let data = anchor.child("a:graphic")?.child("a:graphicData")?;
    (data.attr("uri") == Some(SHAPE_URI) && data.child("wps:wsp")?.child("wps:txbx").is_some())
        .then_some(anchor)
}

/// the text boxes of a paragraph that holds nothing else, or `None`.
pub(super) fn text_box_elements(paragraph: &Element) -> Option<Vec<&Element>> {
    let mut found = Vec::new();
    for child in paragraph.elements() {
        match child.name.as_str() {
            "w:pPr" | "w:proofErr" | "w:bookmarkStart" | "w:bookmarkEnd" => {}
            "w:r" => {
                for inner in child.elements() {
                    match inner.name.as_str() {
                        "w:rPr" | "w:lastRenderedPageBreak" => {}
                        _ => found.push(text_box_anchor(inner).map(|_| inner)?),
                    }
                }
            }
            _ => return None,
        }
    }
    (!found.is_empty()).then_some(found)
}

impl DocxReader<'_> {
    /// a text box from word's drawingml shape. the vml fallback beside it
    /// says the same and is not read.
    pub(super) fn read_text_box(
        &mut self,
        element: &Element,
        story: &mut Story,
    ) -> Option<TextBox> {
        let anchor = text_box_anchor(element)?;
        let Placement::Floating {
            horizontal,
            vertical,
            wrap,
        } = floating_placement(anchor)?
        else {
            return None;
        };
        let extent = anchor.child("wp:extent")?;
        let (cx, cy) = (emu(extent, "cx")?, emu(extent, "cy")?);
        let shape = anchor
            .child("a:graphic")?
            .child("a:graphicData")?
            .child("wps:wsp")?;
        let properties = shape.child("wps:spPr");
        let body = shape.child("wps:bodyPr");
        let inset = |name: &str, default: i64| {
            body.and_then(|b| emu(b, name))
                .map(emu_to_twips)
                .unwrap_or(emu_to_twips(default))
        };
        let content = shape.child("wps:txbx")?.child("w:txbxContent")?;
        let mut ignored_sections = Vec::new();
        let blocks = self.read_blocks(content, story, &mut ignored_sections);
        Some(TextBox {
            name: anchor
                .child("wp:docPr")
                .and_then(|pr| pr.attr("name"))
                .map(str::to_string),
            horizontal,
            vertical,
            width: emu_to_twips(cx),
            height: emu_to_twips(cy),
            width_emu: Some(cx),
            height_emu: Some(cy),
            auto_height: body.is_some_and(|b| b.child("a:spAutoFit").is_some()),
            wrap,
            z_order: z_order(anchor),
            rotation: properties.and_then(rotation),
            margins: BoxMargins {
                left: inset("lIns", 91440),
                top: inset("tIns", 45720),
                right: inset("rIns", 91440),
                bottom: inset("bIns", 45720),
            },
            border: properties
                .and_then(|p| p.child("a:ln"))
                .and_then(line_border),
            fill: properties.and_then(solid_fill),
            blocks,
        })
    }
}

fn srgb(fill: &Element) -> Option<String> {
    fill.child("a:srgbClr")?
        .attr("val")
        .map(str::to_ascii_uppercase)
}

fn solid_fill(properties: &Element) -> Option<String> {
    srgb(properties.child("a:solidFill")?)
}

/// a drawingml line as a border; its width is in emu (12700 to the point).
fn line_border(line: &Element) -> Option<Border> {
    if line.child("a:noFill").is_some() {
        return None;
    }
    let width = emu(line, "w").unwrap_or(9525);
    Some(Border {
        style: "single".into(),
        size: ((width * 8) / 12700).max(1) as u32,
        color: line.child("a:solidFill").and_then(srgb),
    })
}
