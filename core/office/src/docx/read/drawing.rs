//! pictures (`w:drawing` holding a `pic:pic`). shapes, groups and text
//! boxes are read in `shapes`; charts and smartart stay preserved xml.

use crate::model::{
    Crop, FloatPosition, Image, Inline, Mark, MediaItem, OriginalDrawing, Placement, Wrap,
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
    pub(super) fn media_for(&mut self, story: &Story, rel_id: &str) -> Option<String> {
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

pub(super) fn floating_placement(anchor: &Element) -> Option<Placement> {
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
