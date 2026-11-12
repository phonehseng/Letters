//! reading an odt package (opendocument 1.3 text) into the model.

use std::collections::{BTreeSet, HashMap};

use crate::error::OfficeError;
use crate::model::*;
use crate::package::{detect_package, FileKind, Package};
use crate::units::parse_length;
use crate::xml::{Element, Node, XmlDocument};

use super::graphics::DrawResources;
use super::props::{read_style, unquote, OdtStyle};
use crate::anchors::{place_drawings, placeholder, take_drawings, FoundDrawing};

mod body;
mod drawing;
mod inlines;
mod package;
mod sections;
mod styles;
mod tables;

/// odt has no limit on repeated rows and cells; spreadsheets saved as
/// text can claim a million. anything past this is treated as one.
const MAX_REPEAT: usize = 1000;

/// reads a .odt (or .ott) file.
pub fn read_odt(bytes: &[u8]) -> Result<Document, OfficeError> {
    crate::package::detect_ole(bytes)?;
    let package = Package::read(bytes)?;
    if detect_package(&package)? != FileKind::Odt {
        return Err(OfficeError::NotAPackage(
            "it is a Word file, not an OpenDocument file".into(),
        ));
    }
    OdtReader::new(&package).read()
}

/// a tracked change from `text:tracked-changes`.
struct ChangeRegion {
    deletion: bool,
    change: Change,
    deleted: Vec<Inline>,
}

/// a master page: page setup plus the ids of its headers and footers.
#[derive(Clone)]
struct MasterPage {
    section: Section,
}

struct OdtReader<'p> {
    package: &'p Package,
    doc: Document,
    fonts: HashMap<String, String>,
    common: HashMap<String, OdtStyle>,
    /// automatic styles of content.xml and of styles.xml; headers and
    /// footers use the second
    content_auto: HashMap<String, OdtStyle>,
    styles_auto: HashMap<String, OdtStyle>,
    in_styles_part: bool,
    layouts: HashMap<String, Section>,
    masters: HashMap<String, MasterPage>,
    changes: HashMap<String, ChangeRegion>,
    open_insertions: Vec<Change>,
    open_comments: Vec<String>,
    /// list style -> instance of the last list using it
    last_instance: HashMap<String, String>,
    instances_by_xml_id: HashMap<String, String>,
    media_by_path: HashMap<String, String>,
    consumed: BTreeSet<String>,
    after_space: bool,
    counter: usize,
    /// graphic styles of `office:styles` and the default graphic style,
    /// which drawings inherit from
    graphic_common: HashMap<String, OdtStyle>,
    graphic_default: Option<OdtStyle>,
    /// named gradients, arrowheads, dashes and fill pictures
    resources: DrawResources,
    /// each linked text box's place in its chain, by frame name
    chains: HashMap<String, TextChain>,
    /// drawings read from the paragraph being read, until it places them
    pending_objects: Vec<FoundDrawing>,
}

impl<'p> OdtReader<'p> {
    fn new(package: &'p Package) -> OdtReader<'p> {
        let mut doc = Document::default();
        doc.carry.origin = Some(SourceFormat::Odt);
        let mimetype = package
            .bytes("mimetype")
            .map(|bytes| String::from_utf8_lossy(bytes).trim().to_string())
            .unwrap_or_else(|| "application/vnd.oasis.opendocument.text".into());
        doc.carry.odt = Some(OdtCarry {
            mimetype,
            ..Default::default()
        });
        let consumed = [
            "mimetype",
            "content.xml",
            "styles.xml",
            "meta.xml",
            "meta-inf/manifest.xml",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        OdtReader {
            package,
            doc,
            fonts: HashMap::new(),
            common: HashMap::new(),
            content_auto: HashMap::new(),
            styles_auto: HashMap::new(),
            in_styles_part: false,
            layouts: HashMap::new(),
            masters: HashMap::new(),
            changes: HashMap::new(),
            open_insertions: Vec::new(),
            open_comments: Vec::new(),
            last_instance: HashMap::new(),
            instances_by_xml_id: HashMap::new(),
            media_by_path: HashMap::new(),
            consumed,
            after_space: true,
            counter: 0,
            graphic_common: HashMap::new(),
            graphic_default: None,
            resources: DrawResources::default(),
            chains: HashMap::new(),
            pending_objects: Vec::new(),
        }
    }

    fn odt_carry(&mut self) -> &mut OdtCarry {
        self.doc.carry.odt.get_or_insert_with(OdtCarry::default)
    }

    fn read(mut self) -> Result<Document, OfficeError> {
        let styles = self.package.xml("styles.xml")?;
        let content = self
            .package
            .xml("content.xml")?
            .ok_or_else(|| OfficeError::Damaged("its content.xml is missing".into()))?;
        if let Some(styles) = &styles {
            self.read_styles_part(styles);
        }
        self.read_content_part(&content)?;
        self.read_meta()?;
        self.read_manifest()?;
        self.carry_unread_parts();
        self.keep_only_referenced_automatic_styles();
        self.doc.normalize_sections();
        crate::freeform::mark_canvas_sections(&mut self.doc);
        Ok(self.doc)
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}{}", self.counter)
    }
}

fn root_of(part: &str, xml: &XmlDocument) -> PartRoot {
    PartRoot {
        part: part.to_string(),
        attributes: xml
            .root_attributes
            .iter()
            .map(|(name, value)| Attribute::new(name, value))
            .collect(),
    }
}
