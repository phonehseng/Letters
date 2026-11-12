//! writing the model as an opendocument presentation. a deck read from
//! odp keeps its styles.xml, settings, thumbnails and every page and
//! object that did not change byte for byte; edited pages are rebuilt
//! with new automatic styles. a deck from anywhere else gets fresh parts:
//! a master page per master, a page per slide, formatting resolved.

use std::collections::HashMap;

use super::units::write_length;
use super::write_shapes::ShapeWriter;
use super::write_styles::StyleRegistry;
use super::write_text::TextWriter;
use crate::color::ColorContext;
use crate::error::SlidesError;
use crate::inherit::Chain;
use crate::model::{ColorMap, Object, Presentation, Shape, Slide, SourceFormat};
use crate::package::{write_zip, Part};
use crate::preserve::reusable;
use crate::xml::{self, escape_attribute, Element};

pub const MIMETYPE: &str = "application/vnd.oasis.opendocument.presentation";

/// the namespaces every part this writer makes declares
const NAMESPACES: &str = " xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:presentation=\"urn:oasis:names:tc:opendocument:xmlns:presentation:1.0\" xmlns:smil=\"urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0\" xmlns:anim=\"urn:oasis:names:tc:opendocument:xmlns:animation:1.0\" office:version=\"1.3\"";

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

/// the deck as odp bytes, and what could only be written approximately.
pub fn write_odp(presentation: &Presentation) -> Result<(Vec<u8>, Vec<String>), SlidesError> {
    let from_odp = presentation.carry.format == Some(SourceFormat::Odp);
    let media = media_paths(presentation, from_odp);
    let mut writer = OdpWriter {
        presentation,
        media: &media,
        content_styles: StyleRegistry::default(),
        master_styles: StyleRegistry::default(),
        notes: Vec::new(),
    };
    let (content, styles) = if from_odp {
        writer.patched_parts()?
    } else {
        writer.fresh_parts()
    };
    let shadow = if from_odp {
        Vec::new()
    } else {
        shadow_parts(presentation, &content)?
    };
    let mut parts = vec![Part {
        name: "mimetype".into(),
        bytes: MIMETYPE.as_bytes().to_vec(),
    }];
    parts.push(Part {
        name: "content.xml".into(),
        bytes: content.into_bytes(),
    });
    parts.push(Part {
        name: "styles.xml".into(),
        bytes: styles.into_bytes(),
    });
    carry_other_parts(presentation, from_odp, &mut parts);
    parts.extend(shadow);
    for item in &presentation.media {
        if let Some(path) = media.get(&item.id) {
            parts.push(Part {
                name: path.clone(),
                bytes: item.bytes.clone(),
            });
        }
    }
    let manifest = manifest(&parts, presentation, &media, &document_version(&parts));
    parts.push(Part {
        name: "META-INF/manifest.xml".into(),
        bytes: manifest.into_bytes(),
    });
    let bytes = write_zip(&parts, &["mimetype"])?;
    Ok((bytes, writer.notes))
}

/// the deck in its own format, with a fingerprint of every page written,
/// so letters can read the odp back as the exact original (see `shadow`).
fn shadow_parts(presentation: &Presentation, content: &str) -> Result<Vec<Part>, SlidesError> {
    let document = xml::parse(content).map_err(SlidesError::WriteFailed)?;
    let mut pages = Vec::new();
    document.root.walk(&mut |element| {
        if element.name == "draw:page" {
            pages.push(&content[element.start..element.end]);
        }
    });
    let source = crate::pptx::write_pptx(presentation)?;
    Ok(vec![
        Part {
            name: crate::shadow::ODP_SHADOW.into(),
            bytes: source,
        },
        Part {
            name: crate::shadow::ODP_PAGES.into(),
            bytes: crate::shadow::fingerprints(pages.into_iter()).into_bytes(),
        },
    ])
}

/// where each media item goes: back to its path for an odp deck, into
/// Pictures/ otherwise.
fn media_paths(presentation: &Presentation, from_odp: bool) -> HashMap<String, String> {
    let mut paths = HashMap::new();
    let mut taken: Vec<String> = Vec::new();
    for item in &presentation.media {
        let path = match (&item.source_path, from_odp) {
            (Some(path), true) => path.clone(),
            _ => {
                let file = item.id.rsplit('/').next().unwrap_or(&item.id);
                let mut candidate = format!("Pictures/{file}");
                let mut number = 2;
                while taken.contains(&candidate) {
                    candidate = format!("Pictures/{number}-{file}");
                    number += 1;
                }
                candidate
            }
        };
        taken.push(path.clone());
        paths.insert(item.id.clone(), path);
    }
    paths
}

/// meta, settings, thumbnails and the like from an odp deck; a fresh
/// meta.xml for a deck from elsewhere.
fn carry_other_parts(presentation: &Presentation, from_odp: bool, parts: &mut Vec<Part>) {
    const WRITTEN: [&str; 4] = [
        "mimetype",
        "content.xml",
        "styles.xml",
        "META-INF/manifest.xml",
    ];
    if from_odp {
        for part in &presentation.carry.parts {
            if !WRITTEN.contains(&part.name.as_str()) {
                parts.push(Part {
                    name: part.name.clone(),
                    bytes: part.bytes.clone(),
                });
            }
        }
        return;
    }
    let title = presentation
        .metadata
        .title
        .as_deref()
        .map(|t| format!("<dc:title>{}</dc:title>", xml::escape_text(t)))
        .unwrap_or_default();
    let meta = format!("{DECLARATION}<office:document-meta xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" office:version=\"1.3\"><office:meta><meta:generator>Letters</meta:generator>{title}</office:meta></office:document-meta>");
    parts.push(Part {
        name: "meta.xml".into(),
        bytes: meta.into_bytes(),
    });
}

/// the odf version content.xml declares; the manifest has to name the
/// same one or libreoffice refuses the file ("source file could not be
/// loaded", seen with its own 1.4 decks given a 1.3 manifest)
fn document_version(parts: &[Part]) -> String {
    let content = parts
        .iter()
        .find(|p| p.name == "content.xml")
        .map(|p| String::from_utf8_lossy(&p.bytes[..p.bytes.len().min(4096)]).to_string())
        .unwrap_or_default();
    let marker = "office:version=\"";
    content
        .find(marker)
        .and_then(|start| {
            let rest = &content[start + marker.len()..];
            rest.find('"').map(|end| rest[..end].to_string())
        })
        .unwrap_or_else(|| "1.3".to_string())
}

fn manifest(
    parts: &[Part],
    presentation: &Presentation,
    media: &HashMap<String, String>,
    version: &str,
) -> String {
    let mut out = format!("{DECLARATION}<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"{version}\"><manifest:file-entry manifest:full-path=\"/\" manifest:version=\"{version}\" manifest:media-type=\"{MIMETYPE}\"/>");
    for part in parts.iter().filter(|p| p.name != "mimetype") {
        let media_type = presentation
            .media
            .iter()
            .find(|item| media.get(&item.id) == Some(&part.name))
            .map(|item| item.content_type.as_str())
            .unwrap_or(if part.name.ends_with(".xml") {
                "text/xml"
            } else {
                ""
            });
        out.push_str(&format!(
            "<manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{media_type}\"/>",
            escape_attribute(&part.name)
        ));
    }
    out.push_str("</manifest:manifest>");
    out
}

struct OdpWriter<'a> {
    presentation: &'a Presentation,
    media: &'a HashMap<String, String>,
    content_styles: StyleRegistry,
    master_styles: StyleRegistry,
    notes: Vec<String>,
}

/// an odf style name for a master (names must be xml names; the master's
/// own name goes in `style:display-name`).
pub fn master_style_name(index: usize) -> String {
    format!("Master{}", index + 1)
}

impl OdpWriter<'_> {
    fn colors_of(&self, chain: &Chain<'_>) -> (crate::model::ColorScheme, ColorMap) {
        let scheme = chain.theme.map(|t| t.colors.clone()).unwrap_or_default();
        let map = chain
            .master
            .map(|m| m.color_map.clone())
            .unwrap_or_default();
        (scheme, map)
    }

    /// a slide's objects written fresh.
    fn slide_objects(&mut self, slide: &Slide, objects: &[Object]) -> String {
        let chain = Chain::of_slide(self.presentation, slide);
        let (scheme, map) = self.colors_of(&chain);
        let colors = ColorContext {
            scheme: &scheme,
            map: &map,
            placeholder: None,
        };
        let mut writer = ShapeWriter {
            chain: &chain,
            colors: &colors,
            styles: &mut self.content_styles,
            media: self.media,
            notes: &mut self.notes,
        };
        writer.objects(objects)
    }

    fn master_name_of(&self, slide: &Slide) -> String {
        let (_, master) = self.presentation.chain_of(slide);
        let index = master
            .and_then(|m| {
                self.presentation
                    .masters
                    .iter()
                    .position(|other| other.id == m.id)
            })
            .unwrap_or(0);
        if self.presentation.carry.format == Some(SourceFormat::Odp) {
            return master
                .map(|m| m.id.clone())
                .unwrap_or_else(|| master_style_name(0));
        }
        master_style_name(index)
    }

    fn fresh_page(&mut self, slide: &Slide, number: usize) -> String {
        let objects = self.slide_objects(slide, &slide.objects);
        let page_style = self.page_style(slide);
        let notes = self.notes_xml(slide, number);
        format!(
            "<draw:page draw:name=\"{}\" draw:style-name=\"{page_style}\" draw:master-page-name=\"{}\">{objects}{notes}</draw:page>",
            escape_attribute(slide.name.as_deref().unwrap_or(&format!("page{number}"))),
            escape_attribute(&self.master_name_of(slide))
        )
    }

    /// the page's drawing-page style: its own background and visibility.
    fn page_style(&mut self, slide: &Slide) -> String {
        let chain = Chain::of_slide(self.presentation, slide);
        let (scheme, map) = self.colors_of(&chain);
        let colors = ColorContext {
            scheme: &scheme,
            map: &map,
            placeholder: None,
        };
        let fill = match slide.background.as_ref().and_then(|b| b.fill.as_ref()) {
            Some(crate::model::Fill::Solid { color }) => format!(" draw:fill=\"solid\" draw:fill-color=\"#{}\" presentation:background-visible=\"true\"", crate::color::resolve(color, &colors).to_hex()),
            _ => String::new(),
        };
        let hidden = if slide.hidden {
            " presentation:visibility=\"hidden\""
        } else {
            ""
        };
        self.content_styles.style(
            "drawing-page",
            None,
            &format!("<style:drawing-page-properties{fill}{hidden}/>"),
        )
    }

    fn notes_xml(&mut self, slide: &Slide, number: usize) -> String {
        let Some(notes) = &slide.notes else {
            return String::new();
        };
        let chain = Chain::of_slide(self.presentation, slide);
        let (scheme, map) = self.colors_of(&chain);
        let colors = ColorContext {
            scheme: &scheme,
            map: &map,
            placeholder: None,
        };
        let holder = Object::Shape(Shape {
            text: Some(notes.text.clone()),
            ..Default::default()
        });
        let mut writer = TextWriter {
            chain: &chain,
            colors: &colors,
            styles: &mut self.content_styles,
        };
        let text = writer.paragraphs(&holder, &notes.text);
        // powerpoint's notes page layout: the slide image above the text
        format!(
            "<presentation:notes><draw:page-thumbnail draw:layer=\"layout\" svg:width=\"{}\" svg:height=\"{}\" svg:x=\"{}\" svg:y=\"{}\" draw:page-number=\"{number}\" presentation:class=\"page\"/><draw:frame draw:layer=\"layout\" svg:width=\"{}\" svg:height=\"{}\" svg:x=\"{}\" svg:y=\"{}\" presentation:class=\"notes\"><draw:text-box>{text}</draw:text-box></draw:frame></presentation:notes>",
            write_length(5_486_400),
            write_length(3_086_100),
            write_length(685_800),
            write_length(1_143_000),
            write_length(5_486_400),
            write_length(3_600_450),
            write_length(685_800),
            write_length(4_400_550)
        )
    }

    fn fresh_parts(&mut self) -> (String, String) {
        let pages: String = self
            .presentation
            .slides
            .iter()
            .enumerate()
            .map(|(index, slide)| self.fresh_page(slide, index + 1))
            .collect();
        let styles = self.fresh_styles();
        let content = format!(
            "{DECLARATION}<office:document-content{NAMESPACES}><office:automatic-styles>{}</office:automatic-styles><office:body><office:presentation>{pages}</office:presentation></office:body></office:document-content>",
            self.content_styles.automatic
        );
        (content, styles)
    }

    fn fresh_styles(&mut self) -> String {
        let size = &self.presentation.slide_size;
        let mut masters = String::new();
        for (index, master) in self.presentation.masters.iter().enumerate() {
            let chain = Chain::of_master(self.presentation, master);
            let (scheme, map) = self.colors_of(&chain);
            let colors = ColorContext {
                scheme: &scheme,
                map: &map,
                placeholder: None,
            };
            let mut writer = ShapeWriter {
                chain: &chain,
                colors: &colors,
                styles: &mut self.master_styles,
                media: self.media,
                notes: &mut self.notes,
            };
            let objects = writer.objects(&master.objects);
            masters.push_str(&format!(
                "<style:master-page style:name=\"{}\" style:display-name=\"{}\" style:page-layout-name=\"LtPageLayout\">{objects}</style:master-page>",
                master_style_name(index),
                escape_attribute(&master.name)
            ));
        }
        format!(
            "{DECLARATION}<office:document-styles{NAMESPACES}><office:styles>{}{}</office:styles><office:automatic-styles><style:page-layout style:name=\"LtPageLayout\"><style:page-layout-properties fo:margin-top=\"0cm\" fo:margin-bottom=\"0cm\" fo:margin-left=\"0cm\" fo:margin-right=\"0cm\" fo:page-width=\"{}\" fo:page-height=\"{}\" style:print-orientation=\"landscape\"/></style:page-layout>{}</office:automatic-styles><office:master-styles>{masters}</office:master-styles></office:document-styles>",
            self.content_styles.named,
            self.master_styles.named,
            write_length(size.width),
            write_length(size.height),
            self.master_styles.automatic
        )
    }

    /// content.xml and styles.xml of a deck read from odp: unchanged pages
    /// as they were, edited pages rebuilt, new styles added.
    fn patched_parts(&mut self) -> Result<(String, String), SlidesError> {
        let carried = |name: &str| {
            self.presentation.carry.part(name).map(|p| {
                String::from_utf8_lossy(&p.bytes)
                    .trim_start_matches('\u{feff}')
                    .to_string()
            })
        };
        let content = carried("content.xml")
            .ok_or_else(|| SlidesError::WriteFailed("content.xml is missing".into()))?;
        let styles = carried("styles.xml").unwrap_or_default();
        let document =
            xml::parse(&content).map_err(|detail| SlidesError::bad_xml("content.xml", detail))?;
        let body = find(&document.root, "office:presentation")
            .ok_or_else(|| SlidesError::Damaged("content.xml has no presentation".into()))?;
        let pages = self.patched_pages();
        let others: String = body
            .elements()
            .filter(|e| e.name != "draw:page")
            .map(|e| content[e.start..e.end].to_string())
            .collect();
        let raw = &content[body.start..body.end];
        let opening = &raw[..crate::preserve::start_tag_end(raw)];
        let mut out = content.clone();
        out.replace_range(
            body.start..body.end,
            &format!("{opening}{pages}{others}</office:presentation>"),
        );
        let out = insert_before(
            &out,
            "</office:automatic-styles>",
            &self.content_styles.automatic,
            "<office:body>",
        );
        let styles = insert_before(&styles, "</office:styles>", &self.content_styles.named, "");
        Ok((out, styles))
    }

    fn patched_pages(&mut self) -> String {
        let mut out = String::new();
        for (index, slide) in self.presentation.slides.iter().enumerate() {
            let mut plain = slide.clone();
            plain.source = None;
            let source = slide
                .source
                .as_ref()
                .filter(|s| s.format == SourceFormat::Odp);
            if let Some(xml) = reusable(source, SourceFormat::Odp, &plain) {
                out.push_str(xml);
                continue;
            }
            match source.and_then(|s| xml::parse(&s.xml).ok().map(|d| (s.xml.clone(), d))) {
                Some((text, document)) => {
                    out.push_str(&self.rebuilt_page(slide, &text, &document.root, index + 1))
                }
                None => out.push_str(&self.fresh_page(slide, index + 1)),
            }
        }
        out
    }

    /// an edited page: its own opening tag, every unchanged object as it
    /// was, edited and new objects written fresh, the rest copied.
    fn rebuilt_page(&mut self, slide: &Slide, text: &str, page: &Element, number: usize) -> String {
        let mut out = text
            [page.start..page.start + crate::preserve::start_tag_end(&text[page.start..page.end])]
            .to_string();
        for object in &slide.objects {
            let mut plain = object.clone();
            plain.set_source(None);
            match reusable(object.source(), SourceFormat::Odp, &plain) {
                Some(xml) => out.push_str(xml),
                None => out.push_str(&self.slide_objects(slide, std::slice::from_ref(object))),
            }
        }
        let notes_unchanged = slide.notes.as_ref().is_some_and(|notes| {
            let mut plain = notes.clone();
            plain.source = None;
            reusable(notes.source.as_ref(), SourceFormat::Odp, &plain).is_some()
        });
        for child in page.elements().filter(|e| !is_object(e)) {
            if child.name == "presentation:notes" && !notes_unchanged {
                continue;
            }
            out.push_str(&text[child.start..child.end]);
        }
        if !notes_unchanged {
            out.push_str(&self.notes_xml(slide, number));
        }
        out.push_str("</draw:page>");
        out
    }
}

fn is_object(element: &Element) -> bool {
    element.name.starts_with("draw:") && element.name != "draw:page-thumbnail"
}

fn find<'a>(element: &'a Element, name: &str) -> Option<&'a Element> {
    if element.name == name {
        return Some(element);
    }
    element.elements().find_map(|child| find(child, name))
}

/// `addition` put just before `marker`; when the marker is missing, a
/// wrapping element is made before `fallback` (an empty fallback skips).
fn insert_before(text: &str, marker: &str, addition: &str, fallback: &str) -> String {
    if addition.is_empty() {
        return text.to_string();
    }
    if let Some(at) = text.find(marker) {
        let mut out = text.to_string();
        out.insert_str(at, addition);
        return out;
    }
    let element = marker.trim_start_matches("</").trim_end_matches('>');
    match (fallback.is_empty(), text.find(fallback)) {
        (false, Some(at)) => {
            let mut out = text.to_string();
            out.insert_str(at, &format!("<{element}>{addition}</{element}>"));
            out
        }
        _ => text.to_string(),
    }
}
