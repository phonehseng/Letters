//! reading a pptx package into the model.

mod extras;
mod parts;

use std::collections::HashMap;

use crate::package::Package;
use crate::xml::{self, Element, XmlDocument};

use super::dml::{attr_i64, ReadContext};
use super::rels::{self, rels_part_of, ContentTypes, Rels};
use super::text::read_text_style;
use crate::error::SlidesError;
use crate::model::{
    CarriedPart, Carry, EmbeddedFont, MediaItem, Metadata, Presentation, SlideSection, SlideSize,
    SourceFormat,
};
pub use extras::find_notes_body;
pub use parts::{read_background, read_color_map, style_piece};
use parts::{read_master_tree, read_slide};

/// the start of every ole compound file: old .ppt files and encrypted pptx.
const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// a package being read: its parts, content types and parsed rels.
struct Reader {
    package: Package,
    media: HashMap<String, String>,
    media_items: Vec<MediaItem>,
}

impl Reader {
    fn text(&self, part: &str) -> Result<Option<String>, SlidesError> {
        match self.package.bytes(part) {
            None => Ok(None),
            Some(bytes) => xml::decode_text(bytes)
                .map(Some)
                .map_err(|detail| SlidesError::bad_xml(part, detail)),
        }
    }

    fn xml(&self, part: &str) -> Result<Option<XmlDocument>, SlidesError> {
        match self.text(part)? {
            None => Ok(None),
            Some(text) => xml::parse(&text)
                .map(Some)
                .map_err(|detail| SlidesError::bad_xml(part, detail)),
        }
    }

    fn rels(&self, part: &str) -> Rels {
        self.text(&rels_part_of(part))
            .ok()
            .flatten()
            .map(|text| Rels::parse(&text))
            .unwrap_or_default()
    }
}

pub fn read_pptx(bytes: &[u8]) -> Result<Presentation, SlidesError> {
    if bytes.starts_with(&OLE_MAGIC) {
        let text = String::from_utf8_lossy(bytes);
        return Err(
            if text.contains("EncryptionInfo") || utf16_contains(bytes, "EncryptionInfo") {
                SlidesError::Encrypted
            } else {
                SlidesError::LegacyFormat
            },
        );
    }
    let package = Package::read(bytes)?;
    let types = package
        .bytes("[Content_Types].xml")
        .and_then(|bytes| xml::decode_text(bytes).ok())
        .map(|text| ContentTypes::parse(&text))
        .ok_or_else(|| SlidesError::NotAPresentation("it has no content type list".into()))?;
    let mut reader = Reader {
        package,
        media: HashMap::new(),
        media_items: Vec::new(),
    };
    collect_media(&mut reader, &types);
    let root_rels = reader.rels("");
    let main = root_rels
        .first_of(rels::OFFICE_DOCUMENT)
        .map(|rel| rel.resolved(""))
        .unwrap_or_else(|| "ppt/presentation.xml".to_string());
    let is_presentation = types
        .of(&main)
        .is_some_and(|kind| kind.contains("presentationml") || kind.contains("ms-powerpoint"));
    if !is_presentation {
        return Err(SlidesError::NotAPresentation(
            "its main part is not a presentation".into(),
        ));
    }
    let document = reader
        .xml(&main)?
        .ok_or_else(|| SlidesError::Damaged(format!("{main} is missing")))?;
    let presentation = read_presentation(&reader, &main, &document)?;
    match unchanged_shadow(&reader, &root_rels, &presentation) {
        Some(original) => crate::odp::read_odp(original),
        None => Ok(presentation),
    }
}

/// the odp this pptx was converted from, when every slide is still
/// exactly as letters wrote it (see `crate::shadow`).
fn unchanged_shadow<'a>(
    reader: &'a Reader,
    root_rels: &Rels,
    presentation: &Presentation,
) -> Option<&'a [u8]> {
    use crate::shadow::SHADOW_RELATIONSHIP;
    let source = root_rels
        .items
        .iter()
        .find(|rel| rel.kind == SHADOW_RELATIONSHIP)?;
    let slides_kind = format!("{SHADOW_RELATIONSHIP}-slides");
    let recorded = root_rels.items.iter().find(|rel| rel.kind == slides_kind)?;
    let recorded =
        String::from_utf8_lossy(reader.package.bytes(&recorded.resolved(""))?).to_string();
    let texts = presentation
        .slides
        .iter()
        .filter_map(|slide| slide.source.as_ref().map(|s| s.xml.as_str()));
    let all_have_sources = presentation
        .slides
        .iter()
        .all(|slide| slide.source.is_some());
    (all_have_sources && crate::shadow::unchanged(&recorded, texts))
        .then(|| reader.package.bytes(&source.resolved("")))?
}

fn utf16_contains(bytes: &[u8], needle: &str) -> bool {
    let wide: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes
        .windows(wide.len())
        .any(|window| window == wide.as_slice())
}

/// pictures, videos, sounds and embedded fonts go into the media table.
fn collect_media(reader: &mut Reader, types: &ContentTypes) {
    for part in &reader.package.parts {
        let kind = types.of(&part.name).unwrap_or("application/octet-stream");
        let is_media = kind.starts_with("image/")
            || kind.starts_with("video/")
            || kind.starts_with("audio/")
            || kind.contains("fontdata")
            || kind.contains("obfuscatedFont")
            || part.name.starts_with("ppt/media/");
        if !is_media {
            continue;
        }
        let file = part
            .name
            .rsplit('/')
            .next()
            .unwrap_or(&part.name)
            .to_string();
        let mut id = file.clone();
        let mut counter = 2;
        while reader.media_items.iter().any(|item| item.id == id) {
            id = format!("{counter}-{file}");
            counter += 1;
        }
        reader.media.insert(part.name.clone(), id.clone());
        reader.media_items.push(MediaItem {
            id,
            content_type: kind.to_string(),
            source_path: Some(part.name.clone()),
            bytes: part.bytes.clone(),
        });
    }
}

fn read_presentation(
    reader: &Reader,
    main: &str,
    document: &XmlDocument,
) -> Result<Presentation, SlidesError> {
    let root = &document.root;
    let main_rels = reader.rels(main);
    let size = |name: &str| {
        root.child(name).map(|element| SlideSize {
            width: attr_i64(element, "cx").unwrap_or(9_144_000),
            height: attr_i64(element, "cy").unwrap_or(6_858_000),
            kind: element.attr("type").map(str::to_string),
        })
    };
    let context = ReadContext {
        part: main,
        text: &document.source,
        rels: &main_rels,
        media: &reader.media,
    };
    let mut presentation = Presentation {
        slide_size: size("p:sldSz").unwrap_or_default(),
        notes_size: size("p:notesSz"),
        default_text_style: root
            .child("p:defaultTextStyle")
            .map(|style| read_text_style(style, &context))
            .unwrap_or_default(),
        first_slide_number: root.attr("firstSlideNum").and_then(|n| n.parse().ok()),
        media: reader.media_items.clone(),
        metadata: read_metadata(reader),
        ..Default::default()
    };
    presentation.embedded_fonts = read_embedded_fonts(root, &context);

    // masters (with their themes and layouts) in the order the deck lists them
    let master_parts: Vec<String> = root
        .child("p:sldMasterIdLst")
        .map(|list| {
            list.children_named("p:sldMasterId")
                .filter_map(|entry| entry.attr("r:id"))
                .filter_map(|id| main_rels.get(id))
                .map(|rel| rel.resolved(main))
                .collect()
        })
        .unwrap_or_default();
    for part in &master_parts {
        read_master_tree(reader, part, &mut presentation)?;
    }

    let mut numbers: HashMap<u32, String> = HashMap::new();
    let mut part_names = Vec::new();
    if let Some(list) = root.child("p:sldIdLst") {
        for entry in list.children_named("p:sldId") {
            let Some(rel) = entry.attr("r:id").and_then(|id| main_rels.get(id)) else {
                continue;
            };
            let part = rel.resolved(main);
            let number = entry.attr("id").and_then(|n| n.parse().ok());
            let Some(slide) = read_slide(reader, &part, number, &mut presentation)? else {
                continue;
            };
            if let Some(number) = number {
                numbers.insert(number, slide.id.clone());
            }
            part_names.push((slide.id.clone(), part));
            presentation.slides.push(slide);
        }
    }
    presentation.sections = read_sections(root, &numbers);

    let media_parts: Vec<&str> = reader.media.keys().map(String::as_str).collect();
    presentation.carry = Carry {
        format: Some(SourceFormat::Pptx),
        parts: reader
            .package
            .parts
            .iter()
            .filter(|part| !media_parts.contains(&part.name.as_str()))
            .map(|part| CarriedPart {
                name: part.name.clone(),
                bytes: part.bytes.clone(),
            })
            .collect(),
        deck_pieces: Vec::new(),
        part_names,
    };
    presentation.carry.deck_pieces = crate::pptx::write::deck_pieces(&presentation);
    Ok(presentation)
}

fn read_metadata(reader: &Reader) -> Metadata {
    let Ok(Some(core)) = reader.xml("docProps/core.xml") else {
        return Metadata::default();
    };
    let text = |name: &str| {
        core.root
            .child(name)
            .map(|e| e.text())
            .filter(|t| !t.is_empty())
    };
    Metadata {
        title: text("dc:title"),
        author: text("dc:creator"),
    }
}

fn read_embedded_fonts(root: &Element, context: &ReadContext<'_>) -> Vec<EmbeddedFont> {
    let Some(list) = root.child("p:embeddedFontLst") else {
        return Vec::new();
    };
    list.children_named("p:embeddedFont")
        .filter_map(|font| {
            let face = |name: &str| {
                font.child(name)
                    .and_then(|e| e.attr("r:id"))
                    .and_then(|id| context.media_id(id))
            };
            Some(EmbeddedFont {
                typeface: font.child("p:font")?.attr("typeface")?.to_string(),
                regular: face("p:regular"),
                bold: face("p:bold"),
                italic: face("p:italic"),
                bold_italic: face("p:boldItalic"),
            })
        })
        .collect()
}

fn read_sections(root: &Element, numbers: &HashMap<u32, String>) -> Vec<SlideSection> {
    let mut sections = Vec::new();
    root.walk(&mut |element| {
        if xml::local_name(&element.name) != "section" {
            return;
        }
        let ids = element
            .elements()
            .filter(|e| xml::local_name(&e.name) == "sldIdLst")
            .flat_map(|list| list.elements())
            .filter_map(|entry| entry.attr("id").and_then(|n| n.parse::<u32>().ok()))
            .filter_map(|number| numbers.get(&number).cloned())
            .collect();
        sections.push(SlideSection {
            name: element.attr("name").unwrap_or("").to_string(),
            id: element.attr("id").map(str::to_string),
            slide_ids: ids,
        });
    });
    sections
}
