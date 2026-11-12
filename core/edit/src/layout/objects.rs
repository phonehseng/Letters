//! what the app is told about a grouped page: runs, lines and blocks with
//! their boxes and fonts, images, and paths.

use super::*;

impl PageModel {
    /// splits a line into runs of one font, size and colour; word gaps
    /// belong to the run before them.
    pub(super) fn run_ranges(&self, line: &LineModel) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        let mut current: Option<&GlyphRecord> = None;
        for (index, char) in line.chars.iter().enumerate() {
            let LineChar::Glyph(glyph) = char else {
                if let Some(last) = ranges.last_mut() {
                    last.1 = index + 1;
                }
                continue;
            };
            let glyph = &self.run.glyphs[*glyph];
            let same = current.is_some_and(|run| {
                run.font == glyph.font
                    && (run.size - glyph.size).abs() < 0.01
                    && run.color == glyph.color
                    && run.invisible == glyph.invisible
            });
            if same {
                ranges.last_mut().expect("a run is open").1 = index + 1;
            } else {
                ranges.push((index, index + 1));
                current = Some(glyph);
            }
        }
        ranges
    }

    pub(super) fn char_box(&self, line: &LineModel, index: usize) -> Rect {
        let space = &self.run.space;
        match line.chars[index] {
            LineChar::Glyph(glyph) => space
                .visible_rect_of(&self.run.glyphs[glyph].corners)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            LineChar::Gap => {
                // the gap runs from the glyph before to the glyph after
                let glyph_of = |char: &LineChar| match char {
                    LineChar::Glyph(glyph) => Some(*glyph),
                    LineChar::Gap => None,
                };
                let before = line.chars[..index].iter().rev().find_map(glyph_of);
                let after = line.chars[index + 1..].iter().find_map(glyph_of);
                let mut corners: Vec<Point> = Vec::new();
                if let Some(glyph) = before {
                    let box_ = self.run.glyphs[glyph].corners;
                    corners.extend([box_[1], box_[3]]);
                }
                if let Some(glyph) = after {
                    let box_ = self.run.glyphs[glyph].corners;
                    corners.extend([box_[0], box_[2]]);
                }
                space
                    .visible_rect_of(&corners)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
            }
        }
    }

    pub fn font_info(&self, font: Option<usize>) -> FontInfo {
        let Some(index) = font else {
            return FontInfo {
                id: "font:none".into(),
                name: "Unknown".into(),
                family: crate::fonts::pdf_font::FontFamily::SansSerif,
                bold: false,
                italic: false,
                embedded: false,
                subset: false,
                kind: crate::fonts::pdf_font::FontKind::Type1,
                editable_chars: String::new(),
            };
        };
        let font: &PdfFont = &self.run.fonts[index];
        FontInfo {
            id: match font.object {
                Some((number, generation)) => format!("font:{number}-{generation}"),
                None => format!("font:#{index}"),
            },
            name: font.name.clone(),
            family: font.family,
            bold: font.bold,
            italic: font.italic,
            embedded: font.embedded,
            subset: font.subset,
            kind: font.kind,
            editable_chars: if font.writable() {
                font.drawable_chars()
            } else {
                String::new()
            },
        }
    }

    pub(super) fn text_line(&self, block: usize, line_index: usize) -> TextLine {
        let line = &self.blocks[block].lines[line_index];
        let boxes: Vec<Rect> = (0..line.chars.len())
            .map(|index| self.char_box(line, index))
            .collect();
        let runs = self
            .run_ranges(line)
            .into_iter()
            .map(|(start, end)| {
                let LineChar::Glyph(first) = line.chars[start] else {
                    unreachable!("runs start with a glyph")
                };
                let glyph = &self.run.glyphs[first];
                let text: String = line.chars[start..end]
                    .iter()
                    .map(|char| self.char_text(char))
                    .collect();
                // ligatures give several characters one box; repeat it
                let char_boxes = line.chars[start..end]
                    .iter()
                    .zip(&boxes[start..end])
                    .flat_map(|(char, rect)| {
                        std::iter::repeat_n(*rect, self.char_text(char).chars().count())
                    })
                    .collect();
                TextRun {
                    id: self.key("run", first),
                    text,
                    font: self.font_info(glyph.font),
                    size: glyph.size,
                    color: glyph.color,
                    char_boxes,
                    invisible: glyph.invisible,
                }
            })
            .collect();
        let rect = boxes
            .iter()
            .skip(1)
            .fold(boxes[0], |all, rect| all.union(rect));
        TextLine {
            id: self.line_id(block, line_index),
            rect,
            text: self.line_text(line),
            baseline: [
                line.start,
                line.start.plus(line.direction.scaled(line.length)),
            ],
            runs,
        }
    }

    pub fn objects(&self) -> PageObjects {
        let text_blocks = self
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                let lines: Vec<TextLine> = (0..block.lines.len())
                    .map(|line| self.text_line(index, line))
                    .collect();
                let rect = lines
                    .iter()
                    .skip(1)
                    .fold(lines[0].rect, |all, line| all.union(&line.rect));
                let direction = block.lines[0].direction;
                TextBlock {
                    id: self.block_id(index),
                    rect,
                    text: lines
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    lines,
                    rotation: direction.y.atan2(direction.x).to_degrees(),
                    alignment: block.alignment,
                }
            })
            .collect();
        let paths: Vec<_> = (0..self.run.paths.len())
            .map(|path| self.path_object(path))
            .collect();
        PageObjects {
            page: self.page,
            width: self.run.space.width,
            height: self.run.space.height,
            text_blocks,
            images: (0..self.run.images.len())
                .map(|image| self.image_object(image))
                .collect(),
            path_groups: self.path_groups(&paths),
            paths,
        }
    }

    /// an image's corners on screen: top left, top right, bottom left,
    /// bottom right. image space puts the top of the picture at y = 1.
    pub fn image_quad(&self, image: usize) -> [Point; 4] {
        let ctm = self.run.images[image].ctm;
        [(0.0, 1.0), (1.0, 1.0), (0.0, 0.0), (1.0, 0.0)]
            .map(|(x, y)| self.run.space.visible_point(ctm.apply(Point::new(x, y))))
    }

    pub(super) fn image_object(&self, image: usize) -> ImageObject {
        let record = &self.run.images[image];
        let quad = self.image_quad(image);
        let doc_dict = record.object.and_then(|id| self.image_dicts.get(&id));
        let inline = record.inline.as_ref();
        let dict = doc_dict.or(inline);
        let number = |long: &[u8], short: &[u8]| {
            dict.and_then(|dict| dict.get(long).or_else(|_| dict.get(short)).ok())
                .and_then(|value| value.as_float().ok())
                .unwrap_or(0.0)
        };
        let color_space = dict
            .and_then(|dict| dict.get(b"ColorSpace").or_else(|_| dict.get(b"CS")).ok())
            .map(|space| match space {
                lopdf::Object::Name(name) => String::from_utf8_lossy(name).into_owned(),
                lopdf::Object::Array(items) => items
                    .first()
                    .and_then(|item| item.as_name().ok())
                    .map(|name| String::from_utf8_lossy(name).into_owned())
                    .unwrap_or_default(),
                _ => "Named".into(),
            })
            .unwrap_or_default();
        let filter = dict
            .and_then(|dict| dict.get(b"Filter").or_else(|_| dict.get(b"F")).ok())
            .and_then(|filter| match filter {
                lopdf::Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
                lopdf::Object::Array(items) => items
                    .last()
                    .and_then(|item| item.as_name().ok())
                    .map(|name| String::from_utf8_lossy(name).into_owned()),
                _ => None,
            });
        let mut object = ImageObject {
            id: self.image_id(image),
            rect: Rect::around(quad).expect("four corners"),
            quad,
            pixel_width: number(b"Width", b"W") as u32,
            pixel_height: number(b"Height", b"H") as u32,
            color_space,
            bits_per_component: number(b"BitsPerComponent", b"BPC") as u32,
            has_alpha: dict.is_some_and(|dict| dict.has(b"SMask") || dict.has(b"Mask")),
            inline: record.inline.is_some(),
            filter,
            crop_rect: None,
            pixels_per_inch: 0.0,
            opacity: 1.0,
        };
        self.image_extras(image, &mut object);
        object
    }
}
