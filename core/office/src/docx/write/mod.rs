//! writing the model as a docx package.
//!
//! the writer produces the parts the model owns (the document body,
//! styles, numbering, notes, comments, headers, footers, pictures and the
//! core properties) and copies every carried part back unchanged. each
//! rewritten part keeps its original root attributes and the
//! relationships its preserved xml points at, under their original ids.

mod body;
mod drawing;
mod parts;
mod props;
mod table;

pub(crate) use body::foreign_text;

use std::collections::{BTreeSet, HashMap};

use crate::error::OfficeError;
use crate::model::{Block, Document, Inline, Mark, NoteKind, Relationship, SourceFormat};
use crate::package::{relative_target, write_zip, Part};
use crate::xml::{escape_attribute, namespace_uri};

use super::names::{self, REL_BASE, STORY_NAMESPACES};
use super::rels::{self, ContentTypes};

/// writes a document as a .docx file.
pub fn write_docx(document: &Document) -> Result<Vec<u8>, OfficeError> {
    let mut document = document.clone();
    document.normalize_sections();
    let mut writer = DocxWriter::new(&document);
    writer.write_all()?;
    writer.finish()
}

/// one rewritten part's relationships while its xml is produced.
pub(crate) struct StoryOut {
    pub part: String,
    pub rels: Vec<Relationship>,
    media_rels: HashMap<String, String>,
    link_rels: HashMap<String, String>,
    /// how many marked inlines of each comment are still to come
    comment_remaining: HashMap<String, usize>,
    comment_open: BTreeSet<String>,
    /// comments that have their own reference inline in this part
    comment_referenced: BTreeSet<String>,
}

impl StoryOut {
    fn new(part: &str, carried: &[Relationship], blocks: &[&[Block]]) -> StoryOut {
        let mut story = StoryOut {
            part: part.to_string(),
            rels: carried.to_vec(),
            media_rels: HashMap::new(),
            link_rels: HashMap::new(),
            comment_remaining: HashMap::new(),
            comment_open: BTreeSet::new(),
            comment_referenced: BTreeSet::new(),
        };
        for list in blocks {
            for_each_inline(list, &mut |inline| story.count_comment_marks(inline));
        }
        story
    }

    fn count_comment_marks(&mut self, inline: &Inline) {
        if let Inline::CommentReference { comment_id } = inline {
            self.comment_referenced.insert(comment_id.clone());
        }
        for mark in inline.marks() {
            if let Mark::Comment { id } = mark {
                *self.comment_remaining.entry(id.clone()).or_default() += 1;
            }
        }
    }

    /// adds a relationship and returns its id ("rId7"), never reusing an
    /// id a kept relationship already has.
    pub fn add_rel(&mut self, kind: &str, target: &str, external: bool) -> String {
        let mut number = self.rels.len() + 1;
        let mut id = format!("rId{number}");
        while self.rels.iter().any(|rel| rel.id == id) {
            number += 1;
            id = format!("rId{number}");
        }
        self.rels.push(Relationship {
            id: id.clone(),
            kind: format!("{REL_BASE}{kind}"),
            target: target.to_string(),
            external,
        });
        id
    }

    /// a relationship to another part, written relative to this one.
    pub fn add_part_rel(&mut self, kind: &str, part: &str) -> String {
        let target = relative_target(&self.part, part);
        self.add_rel(kind, &target, false)
    }

    pub fn link_rel(&mut self, href: &str) -> String {
        if let Some(id) = self.link_rels.get(href) {
            return id.clone();
        }
        let id = self.add_rel(names::KIND_HYPERLINK, href, true);
        self.link_rels.insert(href.to_string(), id.clone());
        id
    }

    pub fn media_rel(&mut self, media_id: &str, part: &str) -> String {
        if let Some(id) = self.media_rels.get(media_id) {
            return id.clone();
        }
        let id = self.add_part_rel(names::KIND_IMAGE, part);
        self.media_rels.insert(media_id.to_string(), id.clone());
        id
    }
}

/// calls `visit` on every inline of `blocks`, inside tables too.
pub(crate) fn for_each_inline(blocks: &[Block], visit: &mut dyn FnMut(&Inline)) {
    for block in blocks {
        match block {
            Block::Paragraph(_) | Block::Heading(_) => {
                if let Some(paragraph) = block.paragraph() {
                    paragraph.content.iter().for_each(&mut *visit);
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        for_each_inline(&cell.blocks, visit);
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) struct DocxWriter<'d> {
    doc: &'d Document,
    /// preserved xml is only valid in the format it came from
    native: bool,
    main_part: String,
    parts: Vec<Part>,
    types: ContentTypes,
    media_paths: HashMap<String, String>,
    header_paths: HashMap<String, String>,
    note_ids: HashMap<String, i64>,
    comment_ids: HashMap<String, String>,
    list_ids: HashMap<String, String>,
    definition_ids: HashMap<String, String>,
    used_change_ids: BTreeSet<String>,
    next_change_id: u64,
    next_drawing_id: u32,
    styles_present: BTreeSet<String>,
}

impl<'d> DocxWriter<'d> {
    fn new(doc: &'d Document) -> DocxWriter<'d> {
        let native = doc.carry.origin == Some(SourceFormat::Docx);
        let main_part = doc
            .carry
            .docx
            .as_ref()
            .filter(|_| native)
            .map(|docx| docx.main_part.clone())
            .unwrap_or_else(|| "word/document.xml".to_string());
        let mut writer = DocxWriter {
            doc,
            native,
            main_part,
            parts: Vec::new(),
            types: ContentTypes::default(),
            media_paths: HashMap::new(),
            header_paths: HashMap::new(),
            note_ids: HashMap::new(),
            comment_ids: HashMap::new(),
            list_ids: HashMap::new(),
            definition_ids: HashMap::new(),
            used_change_ids: BTreeSet::new(),
            next_change_id: 1,
            next_drawing_id: 1000,
            styles_present: doc.styles.styles.iter().map(|s| s.id.clone()).collect(),
        };
        writer.plan_ids();
        writer.plan_paths();
        writer
    }

    /// the carried docx details, only when they apply to this format.
    fn docx_carry(&self) -> Option<&'d crate::model::DocxCarry> {
        if self.native {
            self.doc.carry.docx.as_ref()
        } else {
            None
        }
    }

    fn carried_rels(&self, part: &str) -> Vec<Relationship> {
        if self.native {
            self.doc.carry.relationships(part).to_vec()
        } else {
            Vec::new()
        }
    }

    /// the opening tag of a rewritten part: its original root attributes
    /// when it has them, plus any namespace the writer needs.
    pub(crate) fn root_tag(&self, part: &str, name: &str) -> String {
        let mut attributes: Vec<(String, String)> = Vec::new();
        if self.native {
            if let Some(carried) = self.doc.carry.root_attributes(part) {
                for attribute in carried {
                    attributes.push((attribute.name.clone(), attribute.value.clone()));
                }
            }
        }
        for (key, uri) in STORY_NAMESPACES {
            if !attributes.iter().any(|(existing, _)| existing == key) {
                attributes.push((key.to_string(), uri.to_string()));
            }
        }
        let mut tag = format!("<{name}");
        for (key, value) in attributes {
            tag.push_str(&format!(" {key}=\"{}\"", escape_attribute(&value)));
        }
        tag.push('>');
        tag
    }

    /// word wants plain numbers for note, comment and list ids. ids that
    /// already are numbers are kept; others get the next free number.
    fn plan_ids(&mut self) {
        for kind in [NoteKind::Footnote, NoteKind::Endnote] {
            let notes: Vec<_> = self.doc.notes.iter().filter(|n| n.kind == kind).collect();
            let mut used: BTreeSet<i64> = BTreeSet::new();
            for note in &notes {
                if let Some(number) = trailing_number(&note.id) {
                    if used.insert(number) {
                        self.note_ids.insert(note.id.clone(), number);
                    }
                }
            }
            let mut next = 1;
            for note in &notes {
                if !self.note_ids.contains_key(&note.id) {
                    while used.contains(&next) {
                        next += 1;
                    }
                    used.insert(next);
                    self.note_ids.insert(note.id.clone(), next);
                }
            }
        }
        self.comment_ids = numbered(self.doc.comments.iter().map(|c| c.id.clone()), 0);
        self.definition_ids = numbered(
            self.doc.numbering.definitions.iter().map(|d| d.id.clone()),
            0,
        );
        self.list_ids = numbered(self.doc.numbering.instances.iter().map(|i| i.id.clone()), 1);
        self.list_ids.insert("0".into(), "0".into());
    }

    /// where each picture and each header or footer is saved.
    fn plan_paths(&mut self) {
        let mut taken: BTreeSet<String> = BTreeSet::new();
        if self.native {
            taken.extend(
                self.doc
                    .carry
                    .parts
                    .iter()
                    .map(|p| p.name.to_ascii_lowercase()),
            );
        }
        for item in &self.doc.media {
            let preferred = item
                .source_path
                .clone()
                .filter(|_| self.native)
                .unwrap_or_else(|| format!("word/media/{}", safe_file_name(&item.id)));
            let path = unique_path(&preferred, &mut taken);
            self.media_paths.insert(item.id.clone(), path);
        }
        let mut counters = (0, 0);
        for part in &self.doc.headers_footers {
            let carried = self
                .docx_carry()
                .and_then(|docx| docx.part_name(&format!("header:{}", part.id)))
                .map(str::to_string);
            let path = carried.unwrap_or_else(|| {
                let (word, counter) = match part.kind {
                    crate::model::HeaderFooterKind::Header => ("header", &mut counters.0),
                    crate::model::HeaderFooterKind::Footer => ("footer", &mut counters.1),
                };
                *counter += 1;
                format!("word/{word}{counter}.xml")
            });
            let path = unique_path(&path, &mut taken);
            self.header_paths.insert(part.id.clone(), path);
        }
    }

    /// a fresh id for a tracked change element. the model's id is kept
    /// the first time it is written.
    pub(crate) fn change_id(&mut self, wanted: &str) -> String {
        if wanted.parse::<u64>().is_ok() && self.used_change_ids.insert(wanted.to_string()) {
            return wanted.to_string();
        }
        loop {
            let candidate = (900_000 + self.next_change_id).to_string();
            self.next_change_id += 1;
            if self.used_change_ids.insert(candidate.clone()) {
                return candidate;
            }
        }
    }

    pub(crate) fn add_part(&mut self, name: &str, content_type: Option<&str>, bytes: Vec<u8>) {
        if let Some(content_type) = content_type {
            self.types.add_override(name, content_type);
        }
        self.parts
            .retain(|part| !part.name.eq_ignore_ascii_case(name));
        self.parts.push(Part {
            name: name.to_string(),
            bytes,
        });
    }

    /// writes a part's relationships file when it has any.
    pub(crate) fn add_rels(&mut self, story: &StoryOut) {
        if !story.rels.is_empty() {
            let path = rels::rels_path(&story.part);
            self.add_part(&path, None, rels::write(&story.rels).into_bytes());
        }
    }

    fn write_all(&mut self) -> Result<(), OfficeError> {
        self.write_main();
        self.write_headers_footers();
        self.write_media();
        self.write_core_and_app();
        self.write_carried_parts();
        self.write_package_rels();
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<u8>, OfficeError> {
        self.write_content_types();
        // the content type list and the package relationships go first,
        // as word itself writes them
        self.parts.sort_by_key(|part| match part.name.as_str() {
            "[Content_Types].xml" => 0,
            "_rels/.rels" => 1,
            _ => 2,
        });
        write_zip(&self.parts, &[])
    }

    fn write_carried_parts(&mut self) {
        if !self.native {
            return;
        }
        let written: BTreeSet<String> = self
            .parts
            .iter()
            .map(|part| part.name.to_ascii_lowercase())
            .collect();
        for part in &self.doc.carry.parts {
            if written.contains(&part.name.to_ascii_lowercase()) {
                continue;
            }
            if let Some(kind) = &part.content_type {
                let extension = part
                    .name
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let default = self
                    .docx_carry()
                    .map(|docx| {
                        docx.defaults
                            .iter()
                            .any(|d| d.extension == extension && &d.content_type == kind)
                    })
                    .unwrap_or(false);
                if !default {
                    self.types.add_override(&part.name, kind);
                }
            }
            self.parts.push(Part {
                name: part.name.clone(),
                bytes: part.bytes.clone(),
            });
        }
    }

    fn write_package_rels(&mut self) {
        let mut relationships = self
            .docx_carry()
            .map(|docx| docx.package_relationships.clone())
            .unwrap_or_default();
        let mut story = StoryOut::new("", &[], &[]);
        story.rels = std::mem::take(&mut relationships);
        let has = |story: &StoryOut, kind: &str| {
            story
                .rels
                .iter()
                .any(|rel| names::relationship_kind(&rel.kind) == kind)
        };
        if !has(&story, names::KIND_DOCUMENT) {
            let main = self.main_part.clone();
            story.add_rel(names::KIND_DOCUMENT, &main, false);
        }
        if !has(&story, names::KIND_CORE) {
            let id = story.add_rel(names::KIND_CORE, "docProps/core.xml", false);
            if let Some(rel) = story.rels.iter_mut().find(|rel| rel.id == id) {
                rel.kind = names::REL_CORE.to_string();
            }
        }
        if !has(&story, names::KIND_APP) {
            story.add_rel(names::KIND_APP, "docProps/app.xml", false);
        }
        self.add_part("_rels/.rels", None, rels::write(&story.rels).into_bytes());
    }

    fn write_content_types(&mut self) {
        let mut defaults = self
            .docx_carry()
            .map(|docx| docx.defaults.clone())
            .unwrap_or_default();
        if defaults.is_empty() {
            defaults.push(crate::model::ContentTypeDefault {
                extension: "rels".into(),
                content_type: names::CT_RELS.into(),
            });
            defaults.push(crate::model::ContentTypeDefault {
                extension: "xml".into(),
                content_type: "application/xml".into(),
            });
        }
        let mut types = std::mem::take(&mut self.types);
        let overrides = std::mem::take(&mut types.overrides);
        types.defaults = defaults;
        types.add_default("rels", names::CT_RELS);
        types.add_default("xml", "application/xml");
        types.overrides = overrides;
        self.parts.retain(|part| part.name != "[Content_Types].xml");
        self.parts.push(Part {
            name: "[Content_Types].xml".into(),
            bytes: types.write().into_bytes(),
        });
    }

    fn write_media(&mut self) {
        for item in &self.doc.media {
            let path = self.media_paths[&item.id].clone();
            let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            let default_known = self
                .docx_carry()
                .map(|docx| {
                    docx.defaults
                        .iter()
                        .any(|d| d.extension == extension && d.content_type == item.content_type)
                })
                .unwrap_or(false);
            // a part-specific override is always right, and leaves the
            // package's own list of defaults exactly as it was
            let content_type = (!default_known).then_some(item.content_type.as_str());
            self.add_part(&path, content_type, item.bytes.clone());
        }
    }

    pub(crate) fn has_style(&self, id: &str) -> bool {
        self.styles_present.contains(id)
    }
}

fn trailing_number(id: &str) -> Option<i64> {
    let digits = id.trim_start_matches(|c: char| !(c.is_ascii_digit() || c == '-'));
    digits.parse().ok()
}

/// keeps ids that are plain numbers and gives the others the next free
/// number, starting from `first`.
fn numbered(ids: impl Iterator<Item = String>, first: u64) -> HashMap<String, String> {
    let ids: Vec<String> = ids.collect();
    let mut used: BTreeSet<u64> = ids.iter().filter_map(|id| id.parse().ok()).collect();
    let mut map = HashMap::new();
    let mut next = first;
    for id in ids {
        if id.parse::<u64>().is_ok() {
            map.insert(id.clone(), id);
            continue;
        }
        while used.contains(&next) {
            next += 1;
        }
        used.insert(next);
        map.insert(id, next.to_string());
    }
    map
}

fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "image".into()
    } else {
        cleaned
    }
}

fn unique_path(preferred: &str, taken: &mut BTreeSet<String>) -> String {
    let mut path = preferred.to_string();
    let mut counter = 1;
    while taken.contains(&path.to_ascii_lowercase()) {
        counter += 1;
        path = match preferred.rsplit_once('/') {
            Some((folder, file)) => format!("{folder}/{counter}-{file}"),
            None => format!("{counter}-{preferred}"),
        };
    }
    taken.insert(path.to_ascii_lowercase());
    path
}

/// the uri behind the `w` prefix, for parts that are not stories.
pub(crate) fn w_namespace() -> &'static str {
    namespace_uri("w")
}
