//! inline content: text, spans, links, fields, notes, comments, tracked changes and pictures.

use super::*;

impl OdtReader<'_> {
    // inline content --------------------------------------------------------

    pub(super) fn marks_with_context(&self, base: &[Mark]) -> Vec<Mark> {
        let mut marks = base.to_vec();
        for change in &self.open_insertions {
            marks.push(Mark::Insertion(change.clone()));
        }
        for id in &self.open_comments {
            marks.push(Mark::Comment { id: id.clone() });
        }
        normalize_marks(&mut marks);
        marks
    }

    pub(super) fn push_text(&mut self, raw: &str, marks: &[Mark], out: &mut Vec<Inline>) {
        let mut text = String::new();
        for c in raw.chars() {
            let c = if matches!(c, '\t' | '\n' | '\r') {
                ' '
            } else {
                c
            };
            if c == ' ' {
                if self.after_space {
                    continue;
                }
                self.after_space = true;
            } else {
                self.after_space = false;
            }
            text.push(c);
        }
        if !text.is_empty() {
            out.push(Inline::Text {
                text,
                marks: self.marks_with_context(marks),
            });
        }
    }

    pub(super) fn read_inlines(
        &mut self,
        parent: &Element,
        xml: &XmlDocument,
        marks: &[Mark],
        out: &mut Vec<Inline>,
    ) {
        for node in &parent.children {
            match node {
                Node::Text(text) => self.push_text(text, marks, out),
                Node::Element(child) => self.read_inline(child, xml, marks, out),
            }
        }
    }

    pub(super) fn read_inline(
        &mut self,
        child: &Element,
        xml: &XmlDocument,
        marks: &[Mark],
        out: &mut Vec<Inline>,
    ) {
        if drawing::is_drawing_element(child) {
            if let Some(block) = self.read_drawing(child, xml) {
                // a placeholder marks where the drawing sat; the paragraph
                // places it (see `read_paragraph_with_drawings`)
                out.push(placeholder(self.pending_objects.len()));
                self.pending_objects.push((block, Vec::new()));
                self.after_space = false;
                return;
            }
        }
        match child.name.as_str() {
            "text:span" => {
                let mut span_marks = marks.to_vec();
                if let Some(name) = child.attr("text:style-name") {
                    span_marks.extend(self.span_marks(name));
                }
                self.read_inlines(child, xml, &span_marks, out);
            }
            "text:a" | "draw:a" => {
                let href = child.attr("xlink:href").unwrap_or_default();
                let target = match href.strip_prefix('#') {
                    Some(anchor) => LinkTarget {
                        anchor: Some(anchor.to_string()),
                        ..Default::default()
                    },
                    None => LinkTarget {
                        href: Some(href.to_string()),
                        ..Default::default()
                    },
                };
                let mut link_marks = marks.to_vec();
                link_marks.push(Mark::Link(target));
                self.read_inlines(child, xml, &link_marks, out);
            }
            "text:s" => {
                let count = child
                    .attr("text:c")
                    .and_then(|c| c.parse::<usize>().ok())
                    .unwrap_or(1)
                    .min(1000);
                out.push(Inline::Text {
                    text: " ".repeat(count),
                    marks: self.marks_with_context(marks),
                });
                self.after_space = true;
            }
            "text:tab" => {
                out.push(Inline::Tab {
                    marks: self.marks_with_context(marks),
                });
                self.after_space = false;
            }
            "text:line-break" => {
                out.push(Inline::Break {
                    kind: BreakKind::Line,
                    marks: self.marks_with_context(marks),
                });
                self.after_space = true;
            }
            "text:soft-page-break" | "text:note-citation" => {}
            "text:note" => self.read_note(child, xml, marks, out),
            "text:bookmark" => {
                let name = child.attr("text:name").unwrap_or_default().to_string();
                out.push(Inline::BookmarkStart {
                    id: name.clone(),
                    name: name.clone(),
                });
                out.push(Inline::BookmarkEnd { id: name });
            }
            "text:bookmark-start" => {
                let name = child.attr("text:name").unwrap_or_default().to_string();
                out.push(Inline::BookmarkStart {
                    id: name.clone(),
                    name,
                });
            }
            "text:bookmark-end" => {
                out.push(Inline::BookmarkEnd {
                    id: child.attr("text:name").unwrap_or_default().to_string(),
                });
            }
            "office:annotation" => self.read_annotation(child, xml, out),
            "office:annotation-end" => {
                if let Some(name) = child.attr("office:name") {
                    self.open_comments.retain(|open| open != name);
                    out.push(Inline::CommentReference {
                        comment_id: name.to_string(),
                    });
                }
            }
            "text:change-start" => {
                if let Some(region) = child
                    .attr("text:change-id")
                    .and_then(|id| self.changes.get(id))
                {
                    if !region.deletion {
                        let change = region.change.clone();
                        self.open_insertions.push(change);
                    }
                }
            }
            "text:change-end" => {
                if let Some(id) = child.attr("text:change-id") {
                    self.open_insertions.retain(|change| change.id != id);
                }
            }
            "text:change" => {
                if let Some(region) = child
                    .attr("text:change-id")
                    .and_then(|id| self.changes.get(id))
                {
                    out.extend(region.deleted.iter().cloned());
                }
            }
            "text:page-number" | "text:page-count" | "text:date" => {
                let (kind, instruction) = match child.name.as_str() {
                    "text:page-number" => (FieldKind::PageNumber, "PAGE"),
                    "text:page-count" => (FieldKind::PageCount, "NUMPAGES"),
                    _ => (FieldKind::Date, "DATE"),
                };
                out.push(Inline::Field(Field {
                    kind,
                    instruction: instruction.into(),
                    result: child.deep_text(),
                    marks: self.marks_with_context(marks),
                }));
                self.after_space = false;
            }
            "draw:frame" => {
                let inline = self.read_frame(child, xml, marks);
                out.push(inline);
                self.after_space = false;
            }
            _ => {
                out.push(Inline::Preserved {
                    xml: xml.raw(child).to_string(),
                });
                self.after_space = false;
            }
        }
    }

    pub(super) fn span_marks(&self, name: &str) -> Vec<Mark> {
        match self.style(name) {
            Some((style, true)) => {
                let mut run = style.run.clone();
                run.style_id = style.parent.clone();
                run.to_marks()
            }
            Some((style, false)) => vec![Mark::CharacterStyle {
                style_id: style.name.clone(),
            }],
            None => Vec::new(),
        }
    }

    pub(super) fn read_note(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        marks: &[Mark],
        out: &mut Vec<Inline>,
    ) {
        let kind = if element.attr("text:note-class") == Some("endnote") {
            NoteKind::Endnote
        } else {
            NoteKind::Footnote
        };
        let id = match element.attr("text:id") {
            Some(id) => id.to_string(),
            None => self.next_id("note"),
        };
        let saved_space = self.after_space;
        let blocks = element
            .child("text:note-body")
            .map(|body| self.read_blocks(body, xml))
            .unwrap_or_default();
        self.after_space = saved_space;
        self.doc.notes.push(Note {
            id: id.clone(),
            kind,
            special: None,
            blocks,
        });
        out.push(Inline::NoteReference {
            note_id: id,
            marks: self.marks_with_context(marks),
        });
    }

    pub(super) fn read_annotation(
        &mut self,
        element: &Element,
        xml: &XmlDocument,
        out: &mut Vec<Inline>,
    ) {
        let name = element.attr("office:name").map(str::to_string);
        let id = match &name {
            Some(name) => name.clone(),
            None => self.next_id("comment"),
        };
        let author = element
            .child("dc:creator")
            .map(Element::deep_text)
            .unwrap_or_default();
        let date = element.child("dc:date").map(Element::deep_text);
        let saved_space = self.after_space;
        let saved_comments = std::mem::take(&mut self.open_comments);
        let mut blocks = Vec::new();
        for child in element.elements() {
            if matches!(child.name.as_str(), "text:p" | "text:h" | "text:list") {
                self.read_block(child, xml, &mut blocks);
            }
        }
        self.open_comments = saved_comments;
        self.after_space = saved_space;
        self.doc.comments.push(Comment {
            id: id.clone(),
            author,
            initials: None,
            date,
            blocks,
        });
        match name {
            Some(_) => self.open_comments.push(id),
            None => out.push(Inline::CommentReference { comment_id: id }),
        }
    }

    pub(super) fn read_tracked_changes(&mut self, element: &Element, xml: &XmlDocument) {
        for region in element.children_named("text:changed-region") {
            let Some(id) = region.attr("text:id").or_else(|| region.attr("xml:id")) else {
                continue;
            };
            let Some(kind) = region.elements().next() else {
                continue;
            };
            let info = kind.child("office:change-info");
            let change = Change {
                id: id.to_string(),
                author: info
                    .and_then(|i| i.child("dc:creator"))
                    .map(Element::deep_text)
                    .unwrap_or_default(),
                date: info
                    .and_then(|i| i.child("dc:date"))
                    .map(Element::deep_text),
            };
            let deletion = kind.name == "text:deletion";
            let mut deleted = Vec::new();
            if deletion {
                let marks = vec![Mark::Deletion(change.clone())];
                for paragraph in kind
                    .children_named("text:p")
                    .chain(kind.children_named("text:h"))
                {
                    self.after_space = true;
                    if !deleted.is_empty() {
                        deleted.push(Inline::Break {
                            kind: BreakKind::Line,
                            marks: marks.clone(),
                        });
                    }
                    self.read_inlines(paragraph, xml, &marks, &mut deleted);
                }
            }
            if deletion || kind.name == "text:insertion" {
                self.changes.insert(
                    id.to_string(),
                    ChangeRegion {
                        deletion,
                        change,
                        deleted,
                    },
                );
            }
        }
    }

    pub(super) fn read_frame(
        &mut self,
        frame: &Element,
        xml: &XmlDocument,
        marks: &[Mark],
    ) -> Inline {
        match self.read_picture(frame, xml) {
            Some(mut image) => {
                image.marks = self.marks_with_context(marks);
                Inline::Image(image)
            }
            None => Inline::Preserved {
                xml: xml.raw(frame).to_string(),
            },
        }
    }

    pub(super) fn read_picture(&mut self, frame: &Element, xml: &XmlDocument) -> Option<Image> {
        let image_element = frame.child("draw:image")?;
        if frame
            .elements()
            .any(|child| child.name == "draw:text-box" || child.name == "draw:object")
        {
            return None;
        }
        let path = image_element
            .attr("xlink:href")?
            .trim_start_matches("./")
            .to_string();
        let media_id = self.media_for(&path)?;
        let width = frame.attr("svg:width").and_then(parse_length)?;
        let height = frame.attr("svg:height").and_then(parse_length)?;
        let placement = if frame.attr("text:anchor-type") == Some("as-char") {
            Placement::Inline
        } else {
            let (horizontal, vertical, wrap) = self.frame_position(frame);
            Placement::Floating {
                horizontal,
                vertical,
                wrap,
            }
        };
        let floating = placement != Placement::Inline;
        let mut image = Image {
            media_id,
            width,
            height,
            description: frame.child("svg:desc").map(Element::deep_text),
            name: frame.attr("draw:name").map(str::to_string),
            placement,
            z_order: frame
                .attr("draw:z-index")
                .and_then(|z| z.parse().ok())
                .filter(|_| floating),
            ..Default::default()
        };
        image.original = Some(OriginalDrawing {
            xml: xml.raw(frame).to_string(),
            as_read: image.geometry(),
        });
        Some(image)
    }

    /// where a floating frame sits and how text flows around it.
    pub(super) fn frame_position(&self, frame: &Element) -> (FloatPosition, FloatPosition, Wrap) {
        let anchor = frame.attr("text:anchor-type").unwrap_or("paragraph");
        let style = frame
            .attr("draw:style-name")
            .and_then(|name| self.style(name))
            .map(|(style, _)| style.clone())
            .unwrap_or_default();
        let wrap = style.wrap.unwrap_or(Wrap::Square);
        let default = if anchor == "page" {
            "page"
        } else {
            "paragraph"
        };
        let model = |rel: &Option<String>, vertical: bool| -> &'static str {
            match rel.as_deref().unwrap_or(default) {
                "page" | "page-start-margin" | "page-end-margin" => "page",
                "page-content" => "margin",
                "char" => "character",
                _ if vertical => "paragraph",
                _ => "column",
            }
        };
        let relative = (
            model(&style.horizontal_rel, false),
            model(&style.vertical_rel, true),
        );
        let offset = |name: &str| frame.attr(name).and_then(parse_length).unwrap_or(0);
        (
            FloatPosition::new(relative.0, offset("svg:x")),
            FloatPosition::new(relative.1, offset("svg:y")),
            wrap,
        )
    }

    pub(super) fn media_for(&mut self, path: &str) -> Option<String> {
        if let Some(id) = self.media_by_path.get(path) {
            return Some(id.clone());
        }
        let part = self.package.get(path)?;
        let file_name = path.rsplit('/').next().unwrap_or(path).to_string();
        let mut id = file_name.clone();
        let mut counter = 1;
        while self.doc.media_item(&id).is_some() {
            counter += 1;
            id = format!("{counter}-{file_name}");
        }
        self.doc.media.push(MediaItem {
            id: id.clone(),
            content_type: MediaItem::content_type_for(&file_name).to_string(),
            source_path: Some(part.name.clone()),
            bytes: part.bytes.clone(),
        });
        self.media_by_path.insert(path.to_string(), id.clone());
        self.consumed.insert(part.name.to_ascii_lowercase());
        Some(id)
    }
}
