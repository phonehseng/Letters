//! reading a docx package into the model.
//!
//! the order matters a little: the theme first (styles name its fonts),
//! then styles (headings are recognised by style), numbering, notes and
//! comments, the body with its sections and headers, and last the
//! properties. whatever no step consumed is carried byte for byte.

mod body;
mod drawing;
mod meta;
mod notes;
mod props;
mod section;
mod shapes;
mod styles;
mod table;
mod vml;

use std::collections::{BTreeSet, HashMap};

use crate::error::OfficeError;
use crate::model::{
    Attribute, CarriedPart, Document, DocxCarry, PartRelationships, PartRoot, Relationship,
    SourceFormat, ThemeFonts,
};
use crate::package::{detect_package, FileKind, Package};
use crate::xml::{Element, XmlDocument};

use super::names::{self, relationship_kind, REWRITTEN_KINDS};
use super::rels::{self, ContentTypes};

/// reads a .docx (or .docm, .dotx) file.
pub fn read_docx(bytes: &[u8]) -> Result<Document, OfficeError> {
    crate::package::detect_ole(bytes)?;
    let package = Package::read(bytes)?;
    match detect_package(&package)? {
        FileKind::Docx => {}
        FileKind::Odt => {
            return Err(OfficeError::NotAPackage(
                "it is an OpenDocument file, not a Word file".into(),
            ))
        }
    }
    DocxReader::new(&package)?.read()
}

/// one part holding document content (the body, a header, the notes...)
/// while it is being read.
pub(crate) struct Story<'x> {
    pub part: String,
    pub xml: &'x XmlDocument,
    pub rels: Vec<Relationship>,
    /// relationship ids that preserved xml in this part points at
    pub referenced: BTreeSet<String>,
    /// comments whose range is open at this point of the part
    pub open_comments: Vec<String>,
    pub is_main: bool,
}

impl<'x> Story<'x> {
    fn new(part: &str, xml: &'x XmlDocument, rels: Vec<Relationship>) -> Story<'x> {
        Story {
            part: part.to_string(),
            xml,
            rels,
            referenced: BTreeSet::new(),
            open_comments: Vec::new(),
            is_main: false,
        }
    }

    /// keeps an element as original xml, noting the relationships it uses
    /// so they are kept too.
    pub fn keep(&mut self, element: &Element) -> String {
        element.walk(&mut |inner| {
            for (name, value) in &inner.attributes {
                if name.starts_with("r:") {
                    self.referenced.insert(value.clone());
                }
            }
        });
        self.xml.raw(element).to_string()
    }

    pub fn rel(&self, id: &str) -> Option<&Relationship> {
        self.rels.iter().find(|rel| rel.id == id)
    }
}

pub(crate) struct DocxReader<'p> {
    package: &'p Package,
    types: ContentTypes,
    doc: Document,
    theme: ThemeFonts,
    main_part: String,
    main_rels: Vec<Relationship>,
    /// parts the writer will produce again, so they are not carried
    consumed: BTreeSet<String>,
    media_by_path: HashMap<String, String>,
    headers_by_path: HashMap<String, String>,
    /// the theme's colours, which shapes refer to
    palette: crate::docx::drawingml::Palette,
    /// drawings read from the paragraph being read, each with its run's
    /// marks, until the paragraph places them
    pending_objects: Vec<(crate::model::Block, Vec<crate::model::Mark>)>,
}

impl<'p> DocxReader<'p> {
    fn new(package: &'p Package) -> Result<DocxReader<'p>, OfficeError> {
        let types = ContentTypes::read(package)?;
        let package_rels = rels::read(package, "")?;
        let main_part = rels::find(&package_rels, names::KIND_DOCUMENT)
            .map(|rel| rels::target_path("", rel))
            .or_else(|| {
                package
                    .get("word/document.xml")
                    .map(|part| part.name.clone())
            })
            .ok_or_else(|| OfficeError::Damaged("the main document part is missing".into()))?;
        let main_rels = rels::read(package, &main_part)?;
        let mut doc = Document::default();
        doc.carry.origin = Some(SourceFormat::Docx);
        doc.carry.docx = Some(DocxCarry {
            main_content_type: types
                .of(&main_part)
                .unwrap_or_else(|| names::CT_MAIN.to_string()),
            main_part: main_part.clone(),
            defaults: types.defaults.clone(),
            package_relationships: package_rels,
            ..Default::default()
        });
        let mut consumed = BTreeSet::new();
        for name in ["[content_types].xml", "_rels/.rels"] {
            consumed.insert(name.to_string());
        }
        Ok(DocxReader {
            package,
            types,
            doc,
            theme: ThemeFonts::default(),
            main_part,
            main_rels,
            consumed,
            media_by_path: HashMap::new(),
            headers_by_path: HashMap::new(),
            palette: crate::docx::drawingml::Palette::default(),
            pending_objects: Vec::new(),
        })
    }

    fn read(mut self) -> Result<Document, OfficeError> {
        self.read_theme()?;
        self.read_styles()?;
        self.read_numbering()?;
        self.read_settings()?;
        self.read_notes()?;
        self.read_comments()?;
        self.read_main()?;
        self.read_core_properties()?;
        self.carry_unread_parts();
        self.doc.normalize_sections();
        crate::freeform::mark_canvas_sections(&mut self.doc);
        Ok(self.doc)
    }

    /// the part a relationship of the main document points at, by kind.
    fn main_target(&self, kind: &str) -> Option<String> {
        rels::find(&self.main_rels, kind).map(|rel| rels::target_path(&self.main_part, rel))
    }

    fn consume(&mut self, part: &str) {
        self.consumed.insert(part.to_ascii_lowercase());
        self.consumed
            .insert(rels::rels_path(part).to_ascii_lowercase());
    }

    /// reads a part as xml, failing clearly when it is malformed.
    fn part_xml(&self, part: &str) -> Result<Option<XmlDocument>, OfficeError> {
        self.package.xml(part)
    }

    /// remembers a part's root attributes and the relationships its
    /// preserved xml needs, for the writer.
    fn finish_story(&mut self, story: Story<'_>, keep_all_other_kinds: bool) {
        let attributes = story
            .xml
            .root_attributes
            .iter()
            .map(|(name, value)| Attribute::new(name, value))
            .collect();
        self.doc.carry.part_roots.push(PartRoot {
            part: story.part.clone(),
            attributes,
        });
        let kept: Vec<Relationship> = story
            .rels
            .iter()
            .filter(|rel| {
                story.referenced.contains(&rel.id)
                    || (keep_all_other_kinds
                        && !REWRITTEN_KINDS.contains(&relationship_kind(&rel.kind)))
            })
            .cloned()
            .collect();
        self.doc.carry.part_relationships.push(PartRelationships {
            part: story.part,
            relationships: kept,
        });
    }

    fn record_part_name(&mut self, kind: &str, part: &str) {
        if let Some(docx) = self.doc.carry.docx.as_mut() {
            docx.part_names.push(Attribute::new(kind, part));
        }
    }

    /// every part nothing read is carried as it is, with its content type.
    fn carry_unread_parts(&mut self) {
        for part in &self.package.parts {
            if self.consumed.contains(&part.name.to_ascii_lowercase()) {
                continue;
            }
            self.doc.carry.parts.push(CarriedPart {
                name: part.name.clone(),
                content_type: self.types.of(&part.name),
                bytes: part.bytes.clone(),
            });
        }
    }
}
