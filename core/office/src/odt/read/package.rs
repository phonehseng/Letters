//! meta.xml, the manifest, and keeping what the model does not read.

use super::*;

impl OdtReader<'_> {
    // meta and the rest -------------------------------------------------------

    pub(super) fn read_meta(&mut self) -> Result<(), OfficeError> {
        let Some(xml) = self.package.xml("meta.xml")? else {
            return Ok(());
        };
        let Some(meta) = xml.root.child("office:meta") else {
            return Ok(());
        };
        let mut extras = Vec::new();
        for element in meta.elements() {
            let metadata = &mut self.doc.metadata;
            let slot = match element.name.as_str() {
                "dc:title" => &mut metadata.title,
                "dc:subject" => &mut metadata.subject,
                "dc:description" => &mut metadata.description,
                "meta:keyword" => &mut metadata.keywords,
                "meta:initial-creator" => &mut metadata.author,
                "dc:creator" => &mut metadata.last_modified_by,
                "meta:creation-date" => &mut metadata.created,
                "dc:date" => &mut metadata.modified,
                "dc:language" => &mut metadata.language,
                _ => {
                    extras.push(xml.raw(element).to_string());
                    continue;
                }
            };
            *slot = Some(element.deep_text());
        }
        self.odt_carry().meta_extras = extras;
        Ok(())
    }

    pub(super) fn read_manifest(&mut self) -> Result<(), OfficeError> {
        let Some(xml) = self.package.xml("META-INF/manifest.xml")? else {
            return Ok(());
        };
        let entries: Vec<Attribute> = xml
            .root
            .children_named("manifest:file-entry")
            .filter_map(|entry| {
                Some(Attribute::new(
                    entry.attr("manifest:full-path")?,
                    entry.attr("manifest:media-type").unwrap_or(""),
                ))
            })
            .collect();
        self.odt_carry().manifest = entries;
        Ok(())
    }

    /// the writer makes automatic styles afresh for everything the model
    /// understands, so only the ones preserved xml still names are kept.
    /// that also stops styles from piling up save after save.
    pub(super) fn keep_only_referenced_automatic_styles(&mut self) {
        let mut names = BTreeSet::new();
        let mut collect = |xml: &str| collect_style_names(xml, &mut names);
        visit_preserved(&self.doc, &mut collect);
        let referenced = |raw: &String| parse_name(raw).is_some_and(|name| names.contains(&name));
        if let Some(odt) = self.doc.carry.odt.as_mut() {
            odt.content_automatic_styles.retain(referenced);
            odt.styles_automatic_styles.retain(referenced);
        }
    }

    pub(super) fn carry_unread_parts(&mut self) {
        let manifest = self
            .doc
            .carry
            .odt
            .as_ref()
            .map(|odt| odt.manifest.clone())
            .unwrap_or_default();
        for part in &self.package.parts {
            if self.consumed.contains(&part.name.to_ascii_lowercase()) {
                continue;
            }
            let content_type = manifest
                .iter()
                .find(|entry| entry.name == part.name)
                .map(|entry| entry.value.clone());
            self.doc.carry.parts.push(CarriedPart {
                name: part.name.clone(),
                content_type,
                bytes: part.bytes.clone(),
            });
        }
    }
}

/// every style name a piece of xml mentions (`...style-name="X"`).
pub(super) fn collect_style_names(xml: &str, names: &mut BTreeSet<String>) {
    let mut rest = xml;
    while let Some(index) = rest.find("style-name=\"") {
        let after = &rest[index + "style-name=\"".len()..];
        let end = after.find('"').unwrap_or(after.len());
        names.insert(after[..end].to_string());
        rest = &after[end..];
    }
}

pub(super) fn parse_name(raw: &str) -> Option<String> {
    crate::xml::parse(raw)
        .ok()?
        .root
        .attr("style:name")
        .map(str::to_string)
}

/// calls `visit` with every piece of preserved xml in the document.
pub(super) fn visit_preserved(document: &Document, visit: &mut dyn FnMut(&str)) {
    fn blocks(list: &[Block], visit: &mut dyn FnMut(&str)) {
        for block in list {
            match block {
                Block::Preserved { xml } => visit(xml),
                Block::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            blocks(&cell.blocks, visit);
                        }
                    }
                }
                // a drawing written back from its original xml needs the
                // automatic styles that xml names (graphic styles, and the
                // paragraph styles of its text)
                Block::TextBox(_) | Block::Shape(_) | Block::Group(_) => {
                    let original = match block {
                        Block::TextBox(object) => object.original.as_ref(),
                        Block::Shape(object) => object.original.as_ref(),
                        Block::Group(object) => object.original.as_ref(),
                        _ => None,
                    };
                    if let Some(original) = original {
                        visit(&original.xml);
                    }
                    let inner: Vec<Block> = block.drawing_blocks().into_iter().cloned().collect();
                    blocks(&inner, visit);
                }
                _ => {
                    for inline in block
                        .paragraph()
                        .map(|p| p.content.as_slice())
                        .unwrap_or(&[])
                    {
                        match inline {
                            Inline::Preserved { xml } | Inline::PreservedRun { xml, .. } => {
                                visit(xml)
                            }
                            Inline::Image(Image {
                                original: Some(original),
                                ..
                            }) => visit(&original.xml),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    blocks(&document.blocks, visit);
    for part in &document.headers_footers {
        blocks(&part.blocks, visit);
    }
    for note in &document.notes {
        blocks(&note.blocks, visit);
    }
    for comment in &document.comments {
        blocks(&comment.blocks, visit);
    }
    for preserved in &document.styles.preserved {
        visit(preserved);
    }
}
