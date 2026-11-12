//! one slide's xml: pictures, shapes and text boxes, each placed exactly
//! where the pdf draws it.

use super::parts::{GROUP_START, NAMESPACES};
use crate::content::{Rgb, Subpath};
use crate::geometry::Rect;
use crate::package::{xml_escape, XML_HEADER};
use crate::structure::fonts::office_family;
use crate::structure::fragments::Fragment;

/// emu (the unit of office drawings) in one point
pub(super) const EMU_PER_POINT: f32 = 12_700.0;

/// where the page sits on the slide: its points scaled and moved (pages
/// of a different size from the slide are shrunk to fit and centred).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Place {
    pub scale: f32,
    pub dx: f32,
    pub dy: f32,
}

impl Place {
    fn x(&self, x: f32) -> i64 {
        ((x * self.scale + self.dx) * EMU_PER_POINT).round() as i64
    }

    fn y(&self, y: f32) -> i64 {
        ((y * self.scale + self.dy) * EMU_PER_POINT).round() as i64
    }

    fn length(&self, length: f32) -> i64 {
        (length * self.scale * EMU_PER_POINT).round().max(0.0) as i64
    }
}

struct Relationship {
    kind: &'static str,
    target: String,
    external: bool,
}

pub(super) struct SlideWriter {
    place: Place,
    background: String,
    shapes: String,
    next_id: u32,
    relationships: Vec<Relationship>,
}

impl SlideWriter {
    pub fn new(place: Place) -> Self {
        Self {
            place,
            background: String::new(),
            shapes: String::new(),
            next_id: 2,
            relationships: vec![Relationship {
                kind: "slideLayout",
                target: "../slideLayouts/slideLayout1.xml".into(),
                external: false,
            }],
        }
    }

    fn relationship(&mut self, kind: &'static str, target: String, external: bool) -> String {
        self.relationships.push(Relationship {
            kind,
            target,
            external,
        });
        format!("rId{}", self.relationships.len())
    }

    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id - 1
    }

    fn transform(&self, rect: &Rect) -> String {
        format!(
            "<a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
            self.place.x(rect.x),
            self.place.y(rect.y),
            self.place.length(rect.width),
            self.place.length(rect.height)
        )
    }

    /// the picture of what couldn't be rebuilt, under everything else.
    /// it is locked in place, so dragging over the slide doesn't move it.
    /// (it can still be selected: a selection lock stops scripts and
    /// add-ins from even listing the slide's shapes.)
    pub fn set_background(&mut self, media: &str, page: &Rect) {
        let rel = self.relationship("image", format!("../media/{media}"), false);
        let id = self.id();
        self.background = format!(
            "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Page background\" descr=\"Everything on the page that isn&apos;t text, a picture or a simple shape\"/><p:cNvPicPr><a:picLocks noGrp=\"1\" noMove=\"1\" noResize=\"1\" noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr userDrawn=\"1\"/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rel}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
            self.transform(page)
        );
    }

    pub fn add_picture(&mut self, media: &str, rect: &Rect) {
        let rel = self.relationship("image", format!("../media/{media}"), false);
        let id = self.id();
        self.shapes.push_str(&format!(
            "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Picture {id}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rel}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
            self.transform(rect)
        ));
    }

    fn fill_and_line(&self, fill: Option<Rgb>, stroke: Option<Rgb>, width: f32) -> String {
        let fill = match fill {
            Some(color) => format!(
                "<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
                color.hex()
            ),
            None => "<a:noFill/>".into(),
        };
        let line = match stroke {
            Some(color) => format!(
                "<a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>",
                self.place.length(width.max(0.25)),
                color.hex()
            ),
            None => "<a:ln><a:noFill/></a:ln>".into(),
        };
        format!("{fill}{line}")
    }

    pub fn add_box(&mut self, rect: &Rect, fill: Option<Rgb>, stroke: Option<Rgb>, width: f32) {
        let id = self.id();
        self.shapes.push_str(&format!(
            "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Rectangle {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{}</p:spPr></p:sp>",
            self.transform(rect),
            self.fill_and_line(fill, stroke, width)
        ));
    }

    /// a straight line; flips say which way it runs across its box.
    pub fn add_line(&mut self, from: (f32, f32), to: (f32, f32), color: Rgb, width: f32) {
        let id = self.id();
        let rect = Rect::from_corners(from.0, from.1, to.0, to.1);
        let flip_h = if to.0 < from.0 { " flipH=\"1\"" } else { "" };
        let flip_v = if to.1 < from.1 { " flipV=\"1\"" } else { "" };
        self.shapes.push_str(&format!(
            "<p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"{id}\" name=\"Line {id}\"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr><p:spPr><a:xfrm{flip_h}{flip_v}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom>{}</p:spPr></p:cxnSp>",
            self.place.x(rect.x),
            self.place.y(rect.y),
            self.place.length(rect.width),
            self.place.length(rect.height),
            self.fill_and_line(None, Some(color), width)
        ));
    }

    /// a shape of straight sides, as a free-form drawing.
    pub fn add_polygon(
        &mut self,
        subpath: &Subpath,
        fill: Option<Rgb>,
        stroke: Option<Rgb>,
        width: f32,
    ) {
        let id = self.id();
        let rect = crate::content::bounds(&subpath.points);
        let (w, h) = (
            self.place.length(rect.width).max(1),
            self.place.length(rect.height).max(1),
        );
        let point = |p: &(f32, f32)| {
            format!(
                "<a:pt x=\"{}\" y=\"{}\"/>",
                self.place.length(p.0 - rect.x),
                self.place.length(p.1 - rect.y)
            )
        };
        let mut path = String::new();
        for (index, p) in subpath.points.iter().enumerate() {
            let tag = if index == 0 { "moveTo" } else { "lnTo" };
            path.push_str(&format!("<a:{tag}>{}</a:{tag}>", point(p)));
        }
        if subpath.closed {
            path.push_str("<a:close/>");
        }
        self.shapes.push_str(&format!(
            "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Shape {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l=\"0\" t=\"0\" r=\"r\" b=\"b\"/><a:pathLst><a:path w=\"{w}\" h=\"{h}\">{path}</a:path></a:pathLst></a:custGeom>{}</p:spPr></p:sp>",
            self.transform(&rect),
            self.fill_and_line(fill, stroke, width)
        ));
    }

    /// one line of text as a text box that doesn't wrap or resize, its
    /// baseline where the pdf's is. returns false for a line with no text.
    pub fn add_text(&mut self, fragment: &Fragment) -> bool {
        let runs = fragment.placed_runs();
        let text: String = runs.iter().map(|(run, _)| run.text.as_str()).collect();
        if text.trim().is_empty() {
            return false;
        }
        let size = fragment.size.max(1.0);
        // powerpoint puts the first baseline about 0.9 of the size below
        // the top of a box with no inner margins (with its usual fonts)
        let drop = 0.9 * size;
        let height = size * 1.2;
        let (rect, rotation) = if fragment.rotated {
            let angle = fragment.angle.to_radians();
            let length = fragment.rect.width.hypot(fragment.rect.height);
            let width = (fragment.rect.width * angle.cos().abs()
                + fragment.rect.height * angle.sin().abs())
            .min(length);
            // the box turns about its middle, so place the middle where the
            // turned box's middle should be
            let middle_x =
                fragment.origin.0 + angle.cos() * width / 2.0 - angle.sin() * (height / 2.0 - drop);
            let middle_y =
                fragment.origin.1 + angle.sin() * width / 2.0 + angle.cos() * (height / 2.0 - drop);
            (
                Rect::new(
                    middle_x - width / 2.0,
                    middle_y - height / 2.0,
                    width,
                    height,
                ),
                format!(" rot=\"{}\"", (fragment.angle * 60_000.0).round() as i64),
            )
        } else {
            let left = runs
                .iter()
                .find(|(run, _)| !run.text.trim().is_empty())
                .map(|(_, rect)| rect.x)
                .unwrap_or(fragment.rect.x);
            (
                Rect::new(
                    left,
                    fragment.baseline - drop,
                    fragment.rect.right() - left + size * 0.5,
                    height,
                ),
                String::new(),
            )
        };
        let mut paragraph = String::new();
        let trimmed = trim_ends(runs.into_iter().map(|(run, _)| run).collect());
        for run in trimmed {
            paragraph.push_str(&self.run_xml(&run, fragment.invisible));
        }
        let id = self.id();
        self.shapes.push_str(&format!(
            "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Text {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm{rotation}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap=\"none\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" rtlCol=\"0\" anchor=\"t\"><a:noAutofit/></a:bodyPr><a:lstStyle/><a:p>{paragraph}</a:p></p:txBody></p:sp>",
            self.place.x(rect.x),
            self.place.y(rect.y),
            self.place.length(rect.width),
            self.place.length(rect.height)
        ));
        true
    }

    fn run_xml(&mut self, run: &crate::structure::model::TextRun, invisible: bool) -> String {
        let size = ((run.size * self.place.scale * 100.0).round() as i64).clamp(100, 400_000);
        let mut properties = format!("lang=\"en-US\" sz=\"{size}\"");
        if run.bold {
            properties.push_str(" b=\"1\"");
        }
        if run.italic {
            properties.push_str(" i=\"1\"");
        }
        if run.superscript {
            properties.push_str(" baseline=\"30000\"");
        }
        properties.push_str(" dirty=\"0\"");
        // text over a scan stays as invisible as it was, but selectable
        let alpha = if invisible {
            "<a:alpha val=\"0\"/>"
        } else {
            ""
        };
        let family = xml_escape(office_family(&run.font_family));
        let link = match &run.link {
            Some(target) if !target.starts_with('#') => {
                let rel = self.relationship("hyperlink", xml_escape(target), true);
                format!("<a:hlinkClick r:id=\"{rel}\"/>")
            }
            _ => String::new(),
        };
        format!(
            "<a:r><a:rPr {properties}><a:solidFill><a:srgbClr val=\"{}\">{alpha}</a:srgbClr></a:solidFill><a:latin typeface=\"{family}\"/><a:cs typeface=\"{family}\"/>{link}</a:rPr><a:t>{}</a:t></a:r>",
            run.color.hex(),
            xml_escape(&run.text)
        )
    }

    /// the slide's xml and its relationships' xml.
    pub fn finish(self) -> (String, String) {
        let slide = format!(
            "{XML_HEADER}<p:sld {NAMESPACES}><p:cSld><p:spTree>{GROUP_START}{}{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>",
            self.background, self.shapes
        );
        let mut relationships = format!(
            "{XML_HEADER}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">"
        );
        for (index, relationship) in self.relationships.iter().enumerate() {
            relationships.push_str(&format!(
                "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{}\" Target=\"{}\"{}/>",
                index + 1,
                relationship.kind,
                relationship.target,
                if relationship.external { " TargetMode=\"External\"" } else { "" }
            ));
        }
        relationships.push_str("</Relationships>");
        (slide, relationships)
    }
}

/// drops spaces at the very start and end of a line's runs.
fn trim_ends(
    mut runs: Vec<crate::structure::model::TextRun>,
) -> Vec<crate::structure::model::TextRun> {
    if let Some(first) = runs.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    if let Some(last) = runs.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    runs.retain(|run| !run.text.is_empty());
    runs
}
