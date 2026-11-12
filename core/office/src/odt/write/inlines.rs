//! inline content: text, spans, links, tracked changes, comments, notes and pictures.

use super::*;

impl OdtWriter<'_> {
    // inline content ----------------------------------------------------------

    pub(super) fn inlines_xml(&mut self, inlines: &[Inline]) -> String {
        let mut out = String::new();
        let mut link: Option<LinkTarget> = None;
        let mut insertion: Option<String> = None;
        let mut index = 0;
        while index < inlines.len() {
            let inline = &inlines[index];
            let this_link = inline.marks().iter().find_map(|m| match m {
                Mark::Link(target) => Some(target.clone()),
                _ => None,
            });
            if this_link != link {
                self.close_insertion(&mut insertion, &mut out);
                if link.is_some() {
                    out.push_str("</text:a>");
                }
                if let Some(target) = &this_link {
                    let href = match (&target.href, &target.anchor) {
                        (Some(href), _) => href.clone(),
                        (None, Some(anchor)) => format!("#{anchor}"),
                        _ => String::new(),
                    };
                    out.push_str(&format!(
                        "<text:a xlink:type=\"simple\" xlink:href=\"{}\">",
                        escape_attribute(&href)
                    ));
                }
                link = this_link;
            }
            // a run of deleted content becomes one change mark
            if inline.is_deleted() {
                self.close_insertion(&mut insertion, &mut out);
                let end = index
                    + inlines[index..]
                        .iter()
                        .take_while(|i| i.is_deleted())
                        .count();
                out.push_str(&self.deletion(&inlines[index..end]));
                index = end;
                continue;
            }
            let inserted = inline.marks().iter().find_map(|m| match m {
                Mark::Insertion(change) => Some(change.clone()),
                _ => None,
            });
            match (&insertion, &inserted) {
                (Some(_), None) => self.close_insertion(&mut insertion, &mut out),
                (None, Some(change)) => insertion = Some(self.open_insertion(change, &mut out)),
                _ => {}
            }
            self.open_comments(inline, &mut out);
            out.push_str(&self.inline_xml(inline));
            self.close_comments(inline, &mut out);
            index += 1;
        }
        self.close_insertion(&mut insertion, &mut out);
        if link.is_some() {
            out.push_str("</text:a>");
        }
        out
    }

    pub(super) fn change_info(change: &Change) -> String {
        format!(
            "<office:change-info><dc:creator>{}</dc:creator><dc:date>{}</dc:date></office:change-info>",
            escape_text(&change.author),
            escape_text(change.date.as_deref().unwrap_or("1970-01-01T00:00:00"))
        )
    }

    pub(super) fn region_id(&mut self, change: &Change) -> String {
        let wanted = safe_name(&change.id);
        if self.native && !change.id.is_empty() && self.used_change_ids.insert(wanted.clone()) {
            return wanted;
        }
        loop {
            self.counter += 1;
            let id = format!("ct{}", self.counter);
            if self.used_change_ids.insert(id.clone()) {
                return id;
            }
        }
    }

    pub(super) fn open_insertion(&mut self, change: &Change, out: &mut String) -> String {
        let id = self.region_id(change);
        self.regions.push(format!(
            "<text:changed-region xml:id=\"{id}\" text:id=\"{id}\"><text:insertion>{}</text:insertion></text:changed-region>",
            Self::change_info(change)
        ));
        out.push_str(&format!("<text:change-start text:change-id=\"{id}\"/>"));
        id
    }

    pub(super) fn close_insertion(&mut self, insertion: &mut Option<String>, out: &mut String) {
        if let Some(id) = insertion.take() {
            out.push_str(&format!("<text:change-end text:change-id=\"{id}\"/>"));
        }
    }

    /// deleted text lives in the change region; the text itself only gets
    /// a marker where it was.
    pub(super) fn deletion(&mut self, inlines: &[Inline]) -> String {
        let change = inlines[0]
            .marks()
            .iter()
            .find_map(|m| match m {
                Mark::Deletion(change) => Some(change.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let id = self.region_id(&change);
        let saved = self.after_space;
        self.after_space = true;
        let mut text = String::new();
        for inline in inlines {
            let mut visible = inline.clone();
            if let Some(marks) = visible.marks_mut() {
                marks.retain(|m| {
                    !matches!(m, Mark::Deletion(_) | Mark::Comment { .. } | Mark::Link(_))
                });
            }
            text.push_str(&self.inline_xml(&visible));
        }
        self.after_space = saved;
        self.regions.push(format!(
            "<text:changed-region xml:id=\"{id}\" text:id=\"{id}\"><text:deletion>{}<text:p>{text}</text:p></text:deletion></text:changed-region>",
            Self::change_info(&change)
        ));
        format!("<text:change text:change-id=\"{id}\"/>")
    }

    pub(super) fn open_comments(&mut self, inline: &Inline, out: &mut String) {
        for mark in inline.marks() {
            let Mark::Comment { id } = mark else { continue };
            if self.comment_open.insert(id.clone()) {
                if let Some(comment) = self.doc.comment(id) {
                    out.push_str(&self.annotation(comment, true));
                }
            }
        }
    }

    pub(super) fn close_comments(&mut self, inline: &Inline, out: &mut String) {
        for mark in inline.marks() {
            let Mark::Comment { id } = mark else { continue };
            let remaining = self.comment_remaining.entry(id.clone()).or_default();
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 && self.comment_open.remove(id) && self.doc.comment(id).is_some() {
                out.push_str(&format!(
                    "<office:annotation-end office:name=\"{}\"/>",
                    escape_attribute(&self.comment_name(id))
                ));
            }
        }
    }

    pub(super) fn annotation(&mut self, comment: &Comment, ranged: bool) -> String {
        let saved = self.after_space;
        let name = if ranged {
            format!(
                " office:name=\"{}\"",
                escape_attribute(&self.comment_name(&comment.id))
            )
        } else {
            String::new()
        };
        let mut xml = format!(
            "<office:annotation{name}><dc:creator>{}</dc:creator>",
            escape_text(&comment.author)
        );
        if let Some(date) = &comment.date {
            xml.push_str(&format!("<dc:date>{}</dc:date>", escape_text(date)));
        }
        let blocks = self.blocks_xml(&comment.blocks);
        xml.push_str(if blocks.is_empty() {
            "<text:p/>"
        } else {
            &blocks
        });
        xml.push_str("</office:annotation>");
        self.after_space = saved;
        xml
    }

    pub(super) fn span(&mut self, marks: &[Mark], content: String) -> String {
        let run = RunProps::from_marks(marks);
        let mut plain = run.clone();
        plain.style_id = None;
        if !self.native {
            plain.preserved.clear();
        }
        let name = if plain.is_empty() {
            run.style_id.as_deref().map(safe_name)
        } else {
            let attributes = run
                .style_id
                .as_deref()
                .map(|id| {
                    format!(
                        " style:parent-style-name=\"{}\"",
                        escape_attribute(&safe_name(id))
                    )
                })
                .unwrap_or_default();
            let body = text_properties(&plain, self.native, &mut self.fonts);
            Some(self.automatic_style("LT", "text", &attributes, &body))
        };
        match name {
            Some(name) => format!(
                "<text:span text:style-name=\"{}\">{content}</text:span>",
                escape_attribute(&name)
            ),
            None => content,
        }
    }

    pub(super) fn inline_xml(&mut self, inline: &Inline) -> String {
        let marks = inline.marks().to_vec();
        match inline {
            Inline::Text { text, .. } => {
                let content = self.encode_text(text);
                self.span(&marks, content)
            }
            Inline::Tab { .. } => {
                self.after_space = false;
                self.span(&marks, "<text:tab/>".into())
            }
            Inline::Break { .. } => {
                self.after_space = true;
                "<text:line-break/>".into()
            }
            Inline::Image(image) => {
                self.after_space = false;
                self.frame_xml(image)
            }
            Inline::NoteReference { note_id, .. } => self.note_xml(note_id, &marks),
            Inline::Field(field) => {
                self.after_space = false;
                let result = escape_text(&field.result);
                let content = match field.kind {
                    FieldKind::PageNumber => format!("<text:page-number text:select-page=\"current\">{result}</text:page-number>"),
                    FieldKind::PageCount => format!("<text:page-count>{result}</text:page-count>"),
                    FieldKind::Date => format!("<text:date>{result}</text:date>"),
                    // a table of contents is a block in odt; inline, only its text can go
                    FieldKind::TableOfContents => result,
                };
                self.span(&marks, content)
            }
            Inline::BookmarkStart { id, name } => {
                self.bookmark_names.insert(id.clone(), name.clone());
                format!(
                    "<text:bookmark-start text:name=\"{}\"/>",
                    escape_attribute(name)
                )
            }
            Inline::BookmarkEnd { id } => {
                let name = self
                    .bookmark_names
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.clone());
                format!(
                    "<text:bookmark-end text:name=\"{}\"/>",
                    escape_attribute(&name)
                )
            }
            Inline::CommentReference { comment_id } => {
                if self.comment_ranged.contains(comment_id) {
                    return String::new();
                }
                match self.doc.comment(comment_id) {
                    Some(comment) => self.annotation(comment, false),
                    None => String::new(),
                }
            }
            Inline::PreservedRun { xml, .. } => {
                let content = self.encode_text(&crate::docx::foreign_text(xml));
                self.span(&marks, content)
            }
            Inline::Preserved { xml } => {
                if self.native {
                    self.after_space = false;
                    xml.clone()
                } else {
                    self.encode_text(&crate::docx::foreign_text(xml))
                }
            }
        }
    }

    /// text with odt's white space rules: runs of spaces as `text:s`,
    /// tabs and line breaks as elements.
    pub(super) fn encode_text(&mut self, text: &str) -> String {
        let mut out = String::new();
        let mut spaces = 0;
        let flush = |spaces: &mut usize, out: &mut String| {
            match *spaces {
                0 => {}
                1 => out.push_str("<text:s/>"),
                n => out.push_str(&format!("<text:s text:c=\"{n}\"/>")),
            }
            *spaces = 0;
        };
        for c in text.chars() {
            match c {
                ' ' if self.after_space => spaces += 1,
                ' ' => {
                    out.push(' ');
                    self.after_space = true;
                }
                '\t' => {
                    flush(&mut spaces, &mut out);
                    out.push_str("<text:tab/>");
                    self.after_space = false;
                }
                '\n' => {
                    flush(&mut spaces, &mut out);
                    out.push_str("<text:line-break/>");
                    self.after_space = true;
                }
                other => {
                    flush(&mut spaces, &mut out);
                    out.push_str(&escape_text(&other.to_string()));
                    self.after_space = false;
                }
            }
        }
        flush(&mut spaces, &mut out);
        out
    }

    pub(super) fn note_xml(&mut self, note_id: &str, marks: &[Mark]) -> String {
        let Some(note) = self.doc.note(note_id) else {
            return String::new();
        };
        let (class, number) = match note.kind {
            NoteKind::Footnote => {
                self.note_numbers.0 += 1;
                ("footnote", self.note_numbers.0)
            }
            NoteKind::Endnote => {
                self.note_numbers.1 += 1;
                ("endnote", self.note_numbers.1)
            }
        };
        let saved = self.after_space;
        let body = self.blocks_xml(&note.blocks);
        self.after_space = saved;
        let body = if body.is_empty() {
            "<text:p/>".to_string()
        } else {
            body
        };
        let xml = format!(
            "<text:note text:id=\"{}\" text:note-class=\"{class}\"><text:note-citation>{number}</text:note-citation><text:note-body>{body}</text:note-body></text:note>",
            escape_attribute(&safe_name(note_id))
        );
        let mut marks = marks.to_vec();
        marks.retain(|m| !matches!(m, Mark::Superscript | Mark::CharacterStyle { .. }));
        self.span(&marks, xml)
    }

    /// the attributes of a floating frame: anchored to its paragraph, with
    /// the position measured from the page, the margins or the paragraph
    /// through the frame's graphic style.
    pub(super) fn floating_attributes(
        &mut self,
        horizontal: &FloatPosition,
        vertical: &FloatPosition,
        wrap: Wrap,
        z_order: Option<i64>,
        extra_properties: &str,
    ) -> String {
        let (wrap_value, through) = match wrap {
            Wrap::Square => ("parallel", ""),
            Wrap::Tight | Wrap::Through => ("dynamic", " style:wrap-contour=\"true\""),
            Wrap::TopAndBottom => ("none", ""),
            Wrap::BehindText => ("run-through", " style:run-through=\"background\""),
            Wrap::InFrontOfText => ("run-through", " style:run-through=\"foreground\""),
        };
        let relation = |position: &FloatPosition| match position.relative_to.as_str() {
            "page" => "page",
            "margin" => "page-content",
            "character" => "char",
            _ => "paragraph",
        };
        let body = format!(
            "<style:graphic-properties style:wrap=\"{wrap_value}\"{through} style:horizontal-pos=\"from-left\" \
             style:horizontal-rel=\"{}\" style:vertical-pos=\"from-top\" style:vertical-rel=\"{}\"{extra_properties}/>",
            relation(horizontal),
            relation(vertical)
        );
        let style = self.automatic_style("LFr", "graphic", "", &body);
        let z = z_order
            .map(|z| format!(" draw:z-index=\"{}\"", z.max(0)))
            .unwrap_or_default();
        format!(
            " draw:style-name=\"{style}\" text:anchor-type=\"paragraph\" svg:x=\"{}\" svg:y=\"{}\"{z}",
            twips_to_inches(horizontal.offset),
            twips_to_inches(vertical.offset)
        )
    }

    pub(super) fn frame_xml(&mut self, image: &Image) -> String {
        if let (true, Some(original)) = (self.native, &image.original) {
            if original.as_read == image.geometry() {
                return original.xml.clone();
            }
        }
        let Some(path) = self.media_paths.get(&image.media_id).cloned() else {
            return String::new();
        };
        let content_type = self
            .doc
            .media_item(&image.media_id)
            .map(|m| m.content_type.clone())
            .unwrap_or_default();
        self.counter += 1;
        let name = image
            .name
            .clone()
            .unwrap_or_else(|| format!("Image{}", self.counter));
        let mut attributes = format!(
            " draw:name=\"{}\" svg:width=\"{}\" svg:height=\"{}\"",
            escape_attribute(&name),
            twips_to_inches(image.width),
            twips_to_inches(image.height)
        );
        match &image.placement {
            Placement::Inline => attributes.push_str(" text:anchor-type=\"as-char\""),
            Placement::Floating {
                horizontal,
                vertical,
                wrap,
            } => {
                let floating =
                    self.floating_attributes(horizontal, vertical, *wrap, image.z_order, "");
                attributes.push_str(&floating);
            }
        }
        let description = image
            .description
            .as_ref()
            .map(|d| format!("<svg:desc>{}</svg:desc>", escape_text(d)))
            .unwrap_or_default();
        format!(
            "<draw:frame{attributes}><draw:image xlink:href=\"{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\" draw:mime-type=\"{}\"/>{description}</draw:frame>",
            escape_attribute(&path),
            escape_attribute(&content_type)
        )
    }
}
