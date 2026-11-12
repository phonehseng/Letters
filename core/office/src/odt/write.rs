//! writing the model as an odt package.
//!
//! formatting goes into styles, as odt wants: named styles for the
//! model's styles, and automatic styles (made here, named "LP1", "LT1"...
//! so they cannot clash with the ones a file brought along) for direct
//! formatting. automatic styles read from an odt are written back as
//! they were, because preserved xml may use them.

use std::collections::{BTreeSet, HashMap};

use crate::error::OfficeError;
use crate::model::*;
use crate::package::{write_zip, Part};
use crate::tables::{grid, Slot};
use crate::units::twips_to_inches;
use crate::xml::{escape_attribute, escape_text, namespace_uri, parse, XmlWriter};

use super::graphics::DrawDefinitions;
use super::props::{cell_properties, paragraph_properties, text_properties, Breaks};

mod body;
mod drawing;
mod inlines;
mod package;
mod sections;
mod styles;
mod tables;

use drawing::is_drawing;
use sections::SectionState;

const MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

/// the prefixes every written part declares.
const PREFIXES: &[&str] = &[
    "office", "style", "text", "table", "draw", "fo", "xlink", "dc", "meta", "number", "svg",
    "loext",
];

/// writes a document as an .odt file.
pub fn write_odt(document: &Document) -> Result<Vec<u8>, OfficeError> {
    let mut document = document.clone();
    document.normalize_sections();
    let mut writer = OdtWriter::new(&document);
    writer.write()
}

/// which part an automatic style goes into.
#[derive(Clone, Copy, PartialEq)]
enum Pool {
    Content,
    Styles,
}

struct OdtWriter<'d> {
    doc: &'d Document,
    native: bool,
    pool: Pool,
    content_styles: Vec<(String, String)>,
    styles_styles: Vec<(String, String)>,
    /// automatic styles already made, by what they contain
    style_names: HashMap<String, String>,
    taken_names: BTreeSet<String>,
    counter: usize,
    fonts: Vec<String>,
    regions: Vec<String>,
    used_change_ids: BTreeSet<String>,
    comment_remaining: HashMap<String, usize>,
    comment_open: BTreeSet<String>,
    comment_ranged: BTreeSet<String>,
    bookmark_names: HashMap<String, String>,
    media_paths: HashMap<String, String>,
    note_numbers: (usize, usize),
    tables: usize,
    after_space: bool,
    seen_instances: BTreeSet<String>,
    seen_definitions: BTreeSet<String>,
    /// gradients, arrowheads, dashes and fill pictures the drawings need
    definitions: DrawDefinitions,
    /// drawings waiting for the paragraph that anchors them, each with the
    /// inline index it goes before
    waiting_drawings: Vec<(usize, String)>,
}

impl<'d> OdtWriter<'d> {
    fn new(doc: &'d Document) -> OdtWriter<'d> {
        let native = doc.carry.origin == Some(SourceFormat::Odt);
        let mut writer = OdtWriter {
            doc,
            native,
            pool: Pool::Content,
            content_styles: Vec::new(),
            styles_styles: Vec::new(),
            style_names: HashMap::new(),
            taken_names: BTreeSet::new(),
            counter: 0,
            fonts: Vec::new(),
            regions: Vec::new(),
            used_change_ids: BTreeSet::new(),
            comment_remaining: HashMap::new(),
            comment_open: BTreeSet::new(),
            comment_ranged: BTreeSet::new(),
            bookmark_names: HashMap::new(),
            media_paths: HashMap::new(),
            note_numbers: (0, 0),
            tables: 0,
            after_space: true,
            seen_instances: BTreeSet::new(),
            seen_definitions: BTreeSet::new(),
            definitions: DrawDefinitions::avoiding(defined_drawing_names(doc, native)),
            waiting_drawings: Vec::new(),
        };
        if let Some(odt) = writer.odt_carry() {
            for raw in odt
                .content_automatic_styles
                .iter()
                .chain(&odt.styles_automatic_styles)
            {
                if let Ok(parsed) = parse(raw) {
                    if let Some(name) = parsed.root.attr("style:name") {
                        writer.taken_names.insert(name.to_string());
                    }
                }
            }
        }
        for item in &doc.media {
            let path = item
                .source_path
                .clone()
                .filter(|path| native && path.starts_with("Pictures/"))
                .unwrap_or_else(|| format!("Pictures/{}", safe_name(&item.id)));
            writer.media_paths.insert(item.id.clone(), path);
        }
        writer.count_comment_marks();
        writer
    }

    /// the name an annotation is written under. ids from odt are kept;
    /// word's numbers are not valid names, so they get a prefix.
    fn comment_name(&self, id: &str) -> String {
        if self.native {
            id.to_string()
        } else {
            format!("c{id}")
        }
    }

    fn odt_carry(&self) -> Option<&'d OdtCarry> {
        if self.native {
            self.doc.carry.odt.as_ref()
        } else {
            None
        }
    }

    fn count_comment_marks(&mut self) {
        let mut visit = |inline: &Inline| {
            for mark in inline.marks() {
                if let Mark::Comment { id } = mark {
                    *self.comment_remaining.entry(id.clone()).or_default() += 1;
                    self.comment_ranged.insert(id.clone());
                }
            }
        };
        crate::docx::for_each_inline(&self.doc.blocks, &mut visit);
    }

    /// a fresh automatic style name that no carried style uses.
    fn fresh_name(&mut self, prefix: &str) -> String {
        loop {
            self.counter += 1;
            let name = format!("{prefix}{}", self.counter);
            if !self.taken_names.contains(&name) {
                return name;
            }
        }
    }

    /// adds an automatic style (body is the xml without the name) or finds
    /// an identical one, and returns its name.
    fn automatic_style(
        &mut self,
        prefix: &str,
        family: &str,
        attributes: &str,
        body: &str,
    ) -> String {
        let key = format!(
            "{}|{family}|{attributes}|{body}",
            self.pool == Pool::Content
        );
        if let Some(name) = self.style_names.get(&key) {
            return name.clone();
        }
        let name = self.fresh_name(prefix);
        self.style_names.insert(key.clone(), name.clone());
        let xml = format!(
            "<style:style style:name=\"{name}\" style:family=\"{family}\"{attributes}>{body}</style:style>"
        );
        let pool = if self.pool == Pool::Content {
            &mut self.content_styles
        } else {
            &mut self.styles_styles
        };
        pool.push((key, xml));
        name
    }

    fn write(&mut self) -> Result<Vec<u8>, OfficeError> {
        let body = self.body_xml();
        let content = self.content_xml(&body);
        let styles = self.styles_xml();
        let meta = self.meta_xml();
        let mut parts = vec![
            Part {
                name: "mimetype".into(),
                bytes: self.mimetype().into_bytes(),
            },
            Part {
                name: "content.xml".into(),
                bytes: content.into_bytes(),
            },
            Part {
                name: "styles.xml".into(),
                bytes: styles.into_bytes(),
            },
            Part {
                name: "meta.xml".into(),
                bytes: meta.into_bytes(),
            },
        ];
        for item in &self.doc.media {
            parts.push(Part {
                name: self.media_paths[&item.id].clone(),
                bytes: item.bytes.clone(),
            });
        }
        if self.native {
            let written: BTreeSet<String> =
                parts.iter().map(|p| p.name.to_ascii_lowercase()).collect();
            for carried in &self.doc.carry.parts {
                if !written.contains(&carried.name.to_ascii_lowercase()) {
                    parts.push(Part {
                        name: carried.name.clone(),
                        bytes: carried.bytes.clone(),
                    });
                }
            }
        }
        let manifest = self.manifest_xml(&parts);
        parts.push(Part {
            name: "META-INF/manifest.xml".into(),
            bytes: manifest.into_bytes(),
        });
        write_zip(&parts, &["mimetype"])
    }

    /// the odf version the written parts declare: a native file's own,
    /// else 1.3.
    fn odf_version(&self) -> String {
        self.doc
            .carry
            .root_attributes("content.xml")
            .filter(|_| self.native)
            .and_then(|attributes| attributes.iter().find(|a| a.name == "office:version"))
            .map(|a| a.value.clone())
            .unwrap_or_else(|| "1.3".into())
    }

    fn mimetype(&self) -> String {
        self.odt_carry()
            .map(|odt| odt.mimetype.clone())
            .unwrap_or_else(|| MIMETYPE.into())
    }

    fn root(&self, part: &str, name: &str) -> String {
        let mut attributes: Vec<(String, String)> = Vec::new();
        if self.native {
            if let Some(carried) = self.doc.carry.root_attributes(part) {
                attributes.extend(carried.iter().map(|a| (a.name.clone(), a.value.clone())));
            }
        }
        for prefix in PREFIXES {
            let key = format!("xmlns:{prefix}");
            if !attributes.iter().any(|(existing, _)| *existing == key) {
                attributes.push((key, namespace_uri(prefix).to_string()));
            }
        }
        // every part says the same version (see `odf_version`)
        if !attributes.iter().any(|(key, _)| key == "office:version") {
            attributes.push(("office:version".into(), self.odf_version()));
        }
        let mut xml = XmlWriter::new();
        xml.open_owned(name, &attributes);
        xml.finish()
    }

    fn font_faces(&self) -> String {
        let mut xml = String::from("<office:font-face-decls>");
        let mut declared = BTreeSet::new();
        if let Some(odt) = self.odt_carry() {
            for raw in &odt.font_faces {
                if let Ok(parsed) = parse(raw) {
                    if let Some(name) = parsed.root.attr("style:name") {
                        declared.insert(name.to_string());
                    }
                }
                xml.push_str(raw);
            }
        }
        for font in &self.fonts {
            if declared.insert(font.clone()) {
                xml.push_str(&format!(
                    "<style:font-face style:name=\"{0}\" svg:font-family=\"'{0}'\"/>",
                    escape_attribute(font)
                ));
            }
        }
        xml.push_str("</office:font-face-decls>");
        xml
    }

    fn content_xml(&mut self, body: &str) -> String {
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&self.root("content.xml", "office:document-content"));
        xml.raw(&self.font_faces());
        xml.raw("<office:automatic-styles>");
        if let Some(odt) = self.odt_carry() {
            for raw in odt
                .content_automatic_styles
                .iter()
                .filter(|raw| keep_carried_style(raw))
            {
                xml.raw(raw);
            }
        }
        for (_, style) in &self.content_styles {
            xml.raw(style);
        }
        xml.raw("</office:automatic-styles><office:body><office:text>");
        if !self.regions.is_empty() {
            xml.raw("<text:tracked-changes>");
            for region in &self.regions {
                xml.raw(region);
            }
            xml.raw("</text:tracked-changes>");
        }
        if let Some(odt) = self.odt_carry() {
            for raw in &odt.body_prelude {
                xml.raw(raw);
            }
        }
        xml.raw(body);
        xml.raw("</office:text></office:body></office:document-content>");
        xml.finish()
    }
}

/// the drawing definitions (gradients, arrowheads...) a document's kept
/// styles already name, so new ones get other names.
fn defined_drawing_names(doc: &Document, native: bool) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if !native {
        return names;
    }
    for raw in &doc.styles.preserved {
        if let Ok(parsed) = parse(raw) {
            if let Some(name) = parsed.root.attr("draw:name") {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// carried automatic styles are kept, except list styles and page layouts:
/// the model owns those and the writer makes them afresh.
fn keep_carried_style(raw: &str) -> bool {
    let name = crate::docx::first_tag_name(raw);
    name != "text:list-style" && name != "style:page-layout"
}

/// style and id names must be xml names: letters, digits, '-', '_', '.'.
fn safe_name(name: &str) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty()
        || !cleaned
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
    {
        cleaned.insert(0, '_');
    }
    cleaned
}

/// word names its heading styles "heading 1"; odt applications look for
/// "Heading 1".
fn display_name(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("heading ") {
        return format!("Heading {rest}");
    }
    name.to_string()
}
