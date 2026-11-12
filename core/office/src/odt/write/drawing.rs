//! drawings in an opendocument text: shapes as `draw:custom-shape` (or
//! `draw:line`), groups as `draw:g`, text boxes as a `draw:frame` with a
//! `draw:text-box`, and floating tables as a frame holding the table.
//!
//! an object read from an odt file and not changed since is written back
//! from its original xml. objects anchored in a paragraph are collected
//! and put into that paragraph (`waiting_drawings`), as the reader found
//! them; objects anchored to the page are written outside paragraphs.

use crate::model::*;
use crate::odt::graphics::{fill_attributes, stroke_attributes, text_area_attributes};
use crate::units::emu_to_inches;
use crate::xml::{escape_attribute, escape_text};

use super::OdtWriter;

/// how a drawing is attached: in a paragraph (anchor type "paragraph" or
/// "char"), to a page, or as a member of a group (no anchor of its own;
/// its position is absolute, `x` and `y` in emu).
#[derive(Clone, Copy)]
pub(super) enum Attach {
    Paragraph,
    Page(Option<u32>),
    Member {
        x: i64,
        y: i64,
        scale_x: f64,
        scale_y: f64,
    },
}

/// true for the blocks written as drawings.
pub(super) fn is_drawing(block: &Block) -> bool {
    matches!(block, Block::TextBox(_) | Block::Shape(_) | Block::Group(_))
        || matches!(block, Block::Table(table) if table.position.is_some())
}

/// opendocument's position relations for the model's anchors.
fn relation(relative_to: &str, vertical: bool) -> &'static str {
    match relative_to {
        "page" => "page",
        "margin" => "page-content",
        "character" => "char",
        "line" if vertical => "line",
        _ => "paragraph",
    }
}

fn wrap_attributes(wrap: Wrap) -> &'static str {
    match wrap {
        Wrap::Square => " style:wrap=\"parallel\"",
        Wrap::Tight | Wrap::Through => " style:wrap=\"dynamic\" style:wrap-contour=\"true\"",
        Wrap::TopAndBottom => " style:wrap=\"none\"",
        Wrap::BehindText => " style:wrap=\"run-through\" style:run-through=\"background\"",
        Wrap::InFrontOfText => " style:wrap=\"run-through\" style:run-through=\"foreground\"",
    }
}

/// the box of an object in its parent's coordinates, in emu.
struct Frame {
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    rotation: Option<f64>,
}

/// a member's box: its child coordinates scaled into the group's box.
fn member_frame(
    horizontal: &FloatPosition,
    vertical: &FloatPosition,
    size: (i64, i64),
    rotation: Option<f64>,
    attach: Attach,
) -> Frame {
    match attach {
        Attach::Member {
            x,
            y,
            scale_x,
            scale_y,
        } => Frame {
            x: x + (horizontal.emu() as f64 * scale_x).round() as i64,
            y: y + (vertical.emu() as f64 * scale_y).round() as i64,
            cx: (size.0 as f64 * scale_x).round() as i64,
            cy: (size.1 as f64 * scale_y).round() as i64,
            rotation,
        },
        _ => Frame {
            x: horizontal.emu(),
            y: vertical.emu(),
            cx: size.0,
            cy: size.1,
            rotation,
        },
    }
}

/// `svg:x`/`svg:y`, or a `draw:transform` for a turned object. the model
/// turns clockwise about the centre; opendocument turns counterclockwise
/// about the corner and then moves the corner.
fn place_attributes(frame: &Frame) -> String {
    let size = format!(
        " svg:width=\"{}\" svg:height=\"{}\"",
        emu_to_inches(frame.cx.max(1)),
        emu_to_inches(frame.cy.max(1))
    );
    let Some(degrees) = frame.rotation.filter(|r| r.abs() > 1e-9) else {
        return format!(
            " svg:x=\"{}\" svg:y=\"{}\"{size}",
            emu_to_inches(frame.x),
            emu_to_inches(frame.y)
        );
    };
    let angle = -degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let (half_w, half_h) = (frame.cx as f64 / 2.0, frame.cy as f64 / 2.0);
    let centre = (frame.x as f64 + half_w, frame.y as f64 + half_h);
    let corner = (
        centre.0 - (half_w * cos + half_h * sin),
        centre.1 - (-half_w * sin + half_h * cos),
    );
    format!(
        "{size} draw:transform=\"rotate ({angle}) translate ({} {})\"",
        emu_to_inches(corner.0.round() as i64),
        emu_to_inches(corner.1.round() as i64)
    )
}

impl OdtWriter<'_> {
    /// a drawing block's element, without the paragraph around it.
    pub(super) fn drawing_element(&mut self, block: &Block, attach: Attach) -> String {
        if self.native && !matches!(attach, Attach::Member { .. }) {
            let original = match block {
                Block::TextBox(object) => reusable_original(object, SourceFormat::Odt),
                Block::Shape(object) => reusable_original(object, SourceFormat::Odt),
                Block::Group(object) => reusable_original(object, SourceFormat::Odt),
                _ => None,
            };
            if let Some(xml) = original {
                return xml.to_string();
            }
        }
        match block {
            Block::Shape(shape) => self.shape_element(shape, attach),
            Block::Group(group) => self.group_element(group, attach),
            Block::TextBox(text_box) => self.text_box_element(text_box, attach),
            Block::Table(table) => self.floating_table_element(table, attach),
            _ => String::new(),
        }
    }

    /// the attributes that anchor a drawing and name its graphic style;
    /// `properties` are its fill, line and text area.
    fn anchored_attributes(
        &mut self,
        horizontal: &FloatPosition,
        vertical: &FloatPosition,
        wrap: Wrap,
        z: Option<i64>,
        attach: Attach,
        properties: &str,
    ) -> String {
        self.styled_attributes(horizontal, vertical, wrap, z, attach, properties, "")
    }

    /// `anchored_attributes` with more style children after the graphic
    /// properties (the text properties of a shape's text).
    #[allow(clippy::too_many_arguments)]
    fn styled_attributes(
        &mut self,
        horizontal: &FloatPosition,
        vertical: &FloatPosition,
        wrap: Wrap,
        z: Option<i64>,
        attach: Attach,
        properties: &str,
        children: &str,
    ) -> String {
        let mut body = String::from("<style:graphic-properties");
        body.push_str(properties);
        let anchor = match attach {
            Attach::Member { .. } => None,
            Attach::Page(number) => Some(("page", number)),
            Attach::Paragraph
                if horizontal.relative_to == "character" || vertical.relative_to == "line" =>
            {
                Some(("char", None))
            }
            Attach::Paragraph => Some(("paragraph", None)),
        };
        if anchor.is_some() {
            body.push_str(wrap_attributes(wrap));
            body.push_str(&format!(
                " style:horizontal-pos=\"from-left\" style:horizontal-rel=\"{}\" style:vertical-pos=\"from-top\" style:vertical-rel=\"{}\"",
                relation(&horizontal.relative_to, false),
                relation(&vertical.relative_to, true)
            ));
        }
        body.push_str("/>");
        body.push_str(children);
        let style = self.automatic_style("LGr", "graphic", "", &body);
        let mut attributes = format!(" draw:style-name=\"{style}\"");
        if let Some((kind, number)) = anchor {
            attributes.push_str(&format!(" text:anchor-type=\"{kind}\""));
            if let Some(number) = number {
                attributes.push_str(&format!(" text:anchor-page-number=\"{number}\""));
            }
            if let Some(z) = z {
                attributes.push_str(&format!(" draw:z-index=\"{}\"", z.max(0)));
            }
        }
        attributes
    }

    /// the graphic properties the model does not know, kept from an odt.
    fn kept_properties(&self, preserved: &DrawingPreserved) -> String {
        if !self.native {
            return String::new();
        }
        let Some(preserved) = preserved.in_format(SourceFormat::Odt) else {
            return String::new();
        };
        preserved
            .attributes
            .iter()
            .map(|a| format!(" {}=\"{}\"", a.name, escape_attribute(&a.value)))
            .collect()
    }

    fn shape_element(&mut self, shape: &Shape, attach: Attach) -> String {
        let media_paths = self.media_paths.clone();
        let picture = |id: &str| media_paths.get(id).cloned();
        let mut properties = fill_attributes(&shape.fill, &mut self.definitions, &picture);
        properties.push_str(&stroke_attributes(
            shape.stroke.as_ref(),
            &mut self.definitions,
        ));
        if let Some(text) = &shape.text {
            properties.push_str(&text_area_attributes(
                &text.margins,
                text.vertical_align,
                text.auto_fit,
            ));
        }
        properties.push_str(&self.kept_properties(&shape.preserved));
        // word shapes take their text colour from their style (often white)
        let text_colour = shape
            .text
            .as_ref()
            .and_then(|text| text.color.as_ref())
            .map(|hex| {
                format!(
                    "<style:text-properties fo:color=\"#{}\"/>",
                    hex.to_ascii_lowercase()
                )
            })
            .unwrap_or_default();
        let anchored = self.styled_attributes(
            &shape.horizontal,
            &shape.vertical,
            shape.wrap,
            shape.z_order,
            attach,
            &properties,
            &text_colour,
        );
        let frame = member_frame(
            &shape.horizontal,
            &shape.vertical,
            shape.size_emu(),
            shape.rotation,
            attach,
        );
        let name = shape
            .name
            .as_ref()
            .map(|n| format!(" draw:name=\"{}\"", escape_attribute(n)))
            .unwrap_or_default();
        let described = described(shape.title.as_deref(), shape.description.as_deref());
        if shape.is_line() {
            return self.line_element(shape, &frame, &anchored, &name, &described);
        }
        let text = self.drawing_text(shape.text.as_ref());
        let geometry = self.enhanced_geometry(shape);
        format!(
            "<draw:custom-shape{anchored}{name}{}>{described}{text}{geometry}</draw:custom-shape>",
            place_attributes(&frame)
        )
    }

    /// a straight line from one corner of its box to the other; the
    /// flips say which corners.
    fn line_element(
        &mut self,
        shape: &Shape,
        frame: &Frame,
        anchored: &str,
        name: &str,
        described: &str,
    ) -> String {
        let (mut x1, mut y1, mut x2, mut y2) =
            (frame.x, frame.y, frame.x + frame.cx, frame.y + frame.cy);
        if shape.flip_horizontal {
            std::mem::swap(&mut x1, &mut x2);
        }
        if shape.flip_vertical {
            std::mem::swap(&mut y1, &mut y2);
        }
        format!(
            "<draw:line{anchored}{name} svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\">{described}<text:p/></draw:line>",
            emu_to_inches(x1),
            emu_to_inches(y1),
            emu_to_inches(x2),
            emu_to_inches(y2)
        )
    }

    /// the outline: a named type for a preset, an enhanced path for a free
    /// outline. word's presets libreoffice has no own name for keep word's
    /// name, which only libreoffice reads (`shapes::nearest_odf_type`).
    fn enhanced_geometry(&mut self, shape: &Shape) -> String {
        let mirror = format!(
            " draw:mirror-horizontal=\"{}\" draw:mirror-vertical=\"{}\"",
            shape.flip_horizontal, shape.flip_vertical
        );
        match &shape.geometry {
            Geometry::Preset { name, adjust } => {
                let (kind, _) = crate::shapes::nearest_odf_type(name);
                let modifiers = if name == "roundRect" {
                    let corner = adjust
                        .first()
                        .map(|a| a.value)
                        .unwrap_or(crate::shapes::ROUND_RECT_DEFAULT);
                    format!(
                        " draw:modifiers=\"{}\"",
                        crate::shapes::round_rect_adjust_to_odf(corner)
                    )
                } else if kind.starts_with("ooxml-") && !adjust.is_empty() {
                    let values: Vec<String> = adjust.iter().map(|a| a.value.to_string()).collect();
                    format!(" draw:modifiers=\"{}\"", values.join(" "))
                } else {
                    String::new()
                };
                format!("<draw:enhanced-geometry svg:viewBox=\"0 0 21600 21600\" draw:type=\"{kind}\"{modifiers}{mirror}/>")
            }
            Geometry::Path { paths } => {
                let (path, width, height) = crate::odt::paths::to_enhanced(paths);
                format!(
                    "<draw:enhanced-geometry svg:viewBox=\"0 0 {} {}\" draw:type=\"non-primitive\" draw:enhanced-path=\"{}\"{mirror}/>",
                    width.round() as i64,
                    height.round() as i64,
                    escape_attribute(&path)
                )
            }
        }
    }

    /// a shape's text as paragraphs, or the empty paragraph a shape
    /// without text carries.
    fn drawing_text(&mut self, text: Option<&ShapeText>) -> String {
        let Some(text) = text else {
            return "<text:p/>".into();
        };
        let content = self.nested_blocks_xml(&text.blocks);
        if content.is_empty() {
            "<text:p/>".into()
        } else {
            content
        }
    }

    fn group_element(&mut self, group: &Group, attach: Attach) -> String {
        let anchored = self.anchored_attributes(
            &group.horizontal,
            &group.vertical,
            group.wrap,
            group.z_order,
            attach,
            "",
        );
        let frame = member_frame(
            &group.horizontal,
            &group.vertical,
            group.size_emu(),
            None,
            attach,
        );
        let scale_x = frame.cx as f64 / group.child_width.max(1) as f64;
        let scale_y = frame.cy as f64 / group.child_height.max(1) as f64;
        let member = Attach::Member {
            x: frame.x - (group.child_offset_x as f64 * scale_x).round() as i64,
            y: frame.y - (group.child_offset_y as f64 * scale_y).round() as i64,
            scale_x,
            scale_y,
        };
        let name = group
            .name
            .as_ref()
            .map(|n| format!(" draw:name=\"{}\"", escape_attribute(n)))
            .unwrap_or_default();
        let mut xml = format!("<draw:g{anchored}{name}>");
        xml.push_str(&described(
            group.title.as_deref(),
            group.description.as_deref(),
        ));
        for child in &group.children {
            let child = turned_member(child, group, scale_x, scale_y);
            xml.push_str(&match &child {
                GroupChild::Shape(shape) => self.shape_element(shape, member),
                // libreoffice does not draw a text frame inside a group
                GroupChild::TextBox(text_box) => self.shape_element(&text_box.as_shape(), member),
                GroupChild::Group(inner) => self.group_element(inner, member),
                GroupChild::Picture(image) => self.member_picture(image, member),
            });
        }
        xml.push_str("</draw:g>");
        xml
    }

    fn member_picture(&mut self, image: &Image, attach: Attach) -> String {
        let Some(path) = self.media_paths.get(&image.media_id).cloned() else {
            return String::new();
        };
        let Placement::Floating {
            horizontal,
            vertical,
            ..
        } = &image.placement
        else {
            return String::new();
        };
        let anchored = self.anchored_attributes(
            horizontal,
            vertical,
            Wrap::InFrontOfText,
            None,
            attach,
            " draw:fill=\"none\" draw:stroke=\"none\"",
        );
        let frame = member_frame(
            horizontal,
            vertical,
            image.size_emu(),
            image.rotation,
            attach,
        );
        format!(
            "<draw:frame{anchored}{}><draw:image xlink:href=\"{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>{}</draw:frame>",
            place_attributes(&frame),
            escape_attribute(&path),
            described(None, image.description.as_deref())
        )
    }

    /// a text box as a frame holding a `draw:text-box`; linked boxes name
    /// the next box of their chain.
    pub(super) fn text_box_element(&mut self, text_box: &TextBox, attach: Attach) -> String {
        let m = &text_box.margins;
        let mut properties = text_area_attributes(m, text_box.vertical_align, text_box.auto_height);
        match &text_box.fill {
            Some(hex) => properties.push_str(&format!(
                " draw:fill=\"solid\" draw:fill-color=\"#{0}\" fo:background-color=\"#{0}\"",
                hex.to_ascii_lowercase()
            )),
            None => properties.push_str(" draw:fill=\"none\" fo:background-color=\"transparent\""),
        }
        match &text_box.border {
            Some(border) if border.style != "none" && border.style != "nil" => {
                properties.push_str(&format!(
                    " fo:border=\"{} solid #{}\"",
                    crate::units::points(f64::from(border.size) / 8.0),
                    border
                        .color
                        .as_deref()
                        .unwrap_or("000000")
                        .to_ascii_lowercase()
                ))
            }
            _ => properties.push_str(" fo:border=\"none\""),
        }
        properties.push_str(&self.kept_properties(&text_box.preserved));
        let anchored = self.anchored_attributes(
            &text_box.horizontal,
            &text_box.vertical,
            text_box.wrap,
            text_box.z_order,
            attach,
            &properties,
        );
        let frame = member_frame(
            &text_box.horizontal,
            &text_box.vertical,
            text_box.size_emu(),
            text_box.rotation,
            attach,
        );
        let name = self.text_box_name(text_box);
        let mut place = place_attributes(&frame);
        let mut min_height = String::new();
        if text_box.auto_height {
            // a box that grows with its text has a smallest height only
            place = place.replace(
                &format!(
                    " svg:height=\"{}\"",
                    emu_to_inches(text_box.size_emu().1.max(1))
                ),
                "",
            );
            min_height = format!(
                " fo:min-height=\"{}\"",
                emu_to_inches(text_box.size_emu().1)
            );
        }
        let next = self
            .chain_next_name(text_box)
            .map(|next| format!(" draw:chain-next-name=\"{}\"", escape_attribute(&next)))
            .unwrap_or_default();
        let content = if text_box.holds_story() {
            let content = self.nested_blocks_xml(&text_box.blocks);
            if content.is_empty() {
                "<text:p/>".to_string()
            } else {
                content
            }
        } else {
            String::new()
        };
        format!(
            "<draw:frame{anchored} draw:name=\"{}\"{place}><draw:text-box{min_height}{next}>{content}</draw:text-box></draw:frame>",
            escape_attribute(&name)
        )
    }

    /// a text box's frame name. boxes of a chain need one so the box
    /// before can point at them.
    fn text_box_name(&mut self, text_box: &TextBox) -> String {
        if let Some(name) = &text_box.name {
            return name.clone();
        }
        match &text_box.chain {
            Some(chain) => format!("{} {}", chain.id, chain.sequence),
            None => {
                self.counter += 1;
                format!("Text Box {}", self.counter)
            }
        }
    }

    /// the frame name of the box after this one in its chain.
    fn chain_next_name(&self, text_box: &TextBox) -> Option<String> {
        let chain = text_box.chain.as_ref()?;
        let mut found = None;
        let mut visit = |block: &Block| {
            if let Block::TextBox(other) = block {
                if let Some(other_chain) = &other.chain {
                    if other_chain.id == chain.id && other_chain.sequence == chain.sequence + 1 {
                        found = Some(other.name.clone().unwrap_or_else(|| {
                            format!("{} {}", other_chain.id, other_chain.sequence)
                        }));
                    }
                }
            }
        };
        for block in &self.doc.blocks {
            visit(block);
        }
        found
    }

    /// a floating table: a frame without a line or fill, holding the table.
    fn floating_table_element(&mut self, table: &Table, attach: Attach) -> String {
        let Some(position) = &table.position else {
            return String::new();
        };
        let width = table
            .width
            .unwrap_or_else(|| table.column_widths.iter().sum());
        let distance = &position.distance;
        let properties = format!(
            " draw:fill=\"none\" fo:background-color=\"transparent\" fo:border=\"none\" fo:padding=\"0in\" fo:margin-left=\"{}\" fo:margin-right=\"{}\" fo:margin-top=\"{}\" fo:margin-bottom=\"{}\"",
            crate::units::twips_to_inches(distance.left),
            crate::units::twips_to_inches(distance.right),
            crate::units::twips_to_inches(distance.top),
            crate::units::twips_to_inches(distance.bottom)
        );
        let anchored = self.anchored_attributes(
            &position.horizontal,
            &position.vertical,
            Wrap::Square,
            None,
            attach,
            &properties,
        );
        let mut inner = table.clone();
        inner.position = None;
        let waiting = std::mem::take(&mut self.waiting_drawings);
        let content = self.table_xml(&inner, None);
        self.waiting_drawings = waiting;
        self.counter += 1;
        format!(
            "<draw:frame{anchored} draw:name=\"Table Frame {}\" svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\"><draw:text-box fo:min-height=\"0in\">{content}</draw:text-box></draw:frame>",
            self.counter,
            emu_to_inches(position.horizontal.emu()),
            emu_to_inches(position.vertical.emu()),
            crate::units::twips_to_inches(width)
        )
    }
}

/// a group member as drawn: opendocument groups have no transform of
/// their own, so a turned or mirrored group passes its turn and mirror to
/// each member, about the group's centre.
fn turned_member(child: &GroupChild, group: &Group, scale_x: f64, scale_y: f64) -> GroupChild {
    let turn = group.rotation.unwrap_or(0.0);
    if turn.abs() < 1e-9 && !group.flip_horizontal && !group.flip_vertical {
        return child.clone();
    }
    let mut child = child.clone();
    let (horizontal, vertical, size) = match &mut child {
        GroupChild::Shape(s) => {
            s.flip_horizontal ^= group.flip_horizontal;
            s.flip_vertical ^= group.flip_vertical;
            s.rotation = Some(s.rotation.unwrap_or(0.0) + turn).filter(|r| r.abs() > 1e-9);
            let size = s.size_emu();
            (&mut s.horizontal, &mut s.vertical, size)
        }
        GroupChild::TextBox(t) => {
            t.rotation = Some(t.rotation.unwrap_or(0.0) + turn).filter(|r| r.abs() > 1e-9);
            let size = t.size_emu();
            (&mut t.horizontal, &mut t.vertical, size)
        }
        GroupChild::Group(g) => {
            g.flip_horizontal ^= group.flip_horizontal;
            g.flip_vertical ^= group.flip_vertical;
            g.rotation = Some(g.rotation.unwrap_or(0.0) + turn).filter(|r| r.abs() > 1e-9);
            let size = g.size_emu();
            (&mut g.horizontal, &mut g.vertical, size)
        }
        GroupChild::Picture(i) => {
            i.rotation = Some(i.rotation.unwrap_or(0.0) + turn).filter(|r| r.abs() > 1e-9);
            let size = i.size_emu();
            match &mut i.placement {
                Placement::Floating {
                    horizontal,
                    vertical,
                    ..
                } => (horizontal, vertical, size),
                Placement::Inline => return child,
            }
        }
    };
    // the member's centre in the group's child coordinates, turned and
    // mirrored about the group's centre
    let centre_x = horizontal.emu() as f64 + size.0 as f64 / 2.0;
    let centre_y = vertical.emu() as f64 + size.1 as f64 / 2.0;
    let group_centre = (
        group.child_offset_x as f64 + group.child_width as f64 / 2.0,
        group.child_offset_y as f64 + group.child_height as f64 / 2.0,
    );
    let mut dx = (centre_x - group_centre.0) * scale_x;
    let mut dy = (centre_y - group_centre.1) * scale_y;
    if group.flip_horizontal {
        dx = -dx;
    }
    if group.flip_vertical {
        dy = -dy;
    }
    let (sin, cos) = turn.to_radians().sin_cos();
    let (tx, ty) = (dx * cos - dy * sin, dx * sin + dy * cos);
    let new_x = (group_centre.0 + tx / scale_x - size.0 as f64 / 2.0).round() as i64;
    let new_y = (group_centre.1 + ty / scale_y - size.1 as f64 / 2.0).round() as i64;
    *horizontal = FloatPosition {
        offset: crate::units::emu_to_twips(new_x),
        offset_emu: Some(new_x),
        ..horizontal.clone()
    };
    *vertical = FloatPosition {
        offset: crate::units::emu_to_twips(new_y),
        offset_emu: Some(new_y),
        ..vertical.clone()
    };
    child
}

/// `svg:title` and `svg:desc`, the alternative text.
fn described(title: Option<&str>, description: Option<&str>) -> String {
    let mut xml = String::new();
    if let Some(title) = title {
        xml.push_str(&format!("<svg:title>{}</svg:title>", escape_text(title)));
    }
    if let Some(description) = description {
        xml.push_str(&format!(
            "<svg:desc>{}</svg:desc>",
            escape_text(description)
        ));
    }
    xml
}

impl OdtWriter<'_> {
    /// writes the drawing at `index` (with the drawings after it that share
    /// its paragraph) and returns how many blocks it used. drawings for the
    /// next paragraph of text wait for it in `waiting_drawings`.
    pub(super) fn drawings_xml(
        &mut self,
        blocks: &[Block],
        index: usize,
        master: Option<&str>,
        out: &mut String,
    ) -> usize {
        let block = &blocks[index];
        let anchor = block.object_anchor().cloned().unwrap_or_default();
        if anchor.with == AnchorWith::Page {
            let element = self.drawing_element(block, Attach::Page(anchor.page_number.or(Some(1))));
            out.push_str(&element);
            return 1;
        }
        let element = self.drawing_element(block, Attach::Paragraph);
        if anchor.with == AnchorWith::Next && next_takes_drawings(blocks, index + 1) {
            self.waiting_drawings
                .push((anchor.inline_index.unwrap_or(0), element));
            return 1;
        }
        // an anchor paragraph of its own, shared with the drawings after it
        // that say so
        self.waiting_drawings.push((0, element));
        let mut used = 1;
        while let Some(next) = blocks.get(index + used) {
            let shares = is_drawing(next)
                && next.object_anchor().map(|a| a.with) == Some(AnchorWith::Previous);
            if !shares {
                break;
            }
            let element = self.drawing_element(next, Attach::Paragraph);
            self.waiting_drawings.push((0, element));
            used += 1;
        }
        let paragraph = anchor.paragraph.unwrap_or_default();
        out.push_str(&self.paragraph_xml(&paragraph, None, master, false));
        used
    }

    /// the blocks inside a drawing (its text, a text box's story). the
    /// drawings waiting for the next paragraph of the body wait on; the
    /// paragraphs in here are not that paragraph.
    fn nested_blocks_xml(&mut self, blocks: &[Block]) -> String {
        let waiting = std::mem::take(&mut self.waiting_drawings);
        let saved = self.after_space;
        let xml = self.blocks_xml(blocks);
        self.after_space = saved;
        self.waiting_drawings = waiting;
        xml
    }

    /// a paragraph's content with the drawings waiting for it put in at
    /// their places.
    pub(super) fn content_with_drawings(&mut self, content: &[Inline]) -> String {
        let mut drawings = std::mem::take(&mut self.waiting_drawings);
        drawings.sort_by_key(|(at, _)| *at);
        let mut xml = String::new();
        let mut start = 0;
        for (at, element) in drawings {
            let at = at.clamp(start, content.len());
            xml.push_str(&self.inlines_xml(&content[start..at]));
            xml.push_str(&element);
            start = at;
        }
        xml.push_str(&self.inlines_xml(&content[start..]));
        xml
    }
}

/// whether drawings for "the next paragraph" find one at `from`, looking
/// past other drawings that wait for it too.
fn next_takes_drawings(blocks: &[Block], from: usize) -> bool {
    for block in &blocks[from.min(blocks.len())..] {
        match block {
            Block::Paragraph(_) | Block::Heading(_) => return true,
            _ if is_drawing(block)
                && block.object_anchor().map(|a| a.with) == Some(AnchorWith::Next) => {}
            _ => return false,
        }
    }
    false
}
