//! the deck-level parts: presentation.xml with its slide and master
//! lists, its relationships, the content types, and dropping parts no
//! relationship reaches any more (a deleted slide's notes and pictures).

use std::collections::{HashSet, VecDeque};

use crate::xml::{self, Element};

use super::super::dml::{attr, WriteContext};
use super::super::rels::{self, rels_part_of, Rels};
use super::super::text::write_text_style;
use super::deck_piece_unchanged;
use super::fresh::PRESENTATION_TYPE;
use super::names::PartNames;
use super::package::{OutPackage, CONTENT_TYPES_PART, PRESENTATION_PART};
use super::parts::master_list_id;
use crate::error::SlidesError;
use crate::model::Presentation;
use crate::preserve::{patch_element, ChildEdit};

/// schema order of `p:presentation` children (ecma-376 part 1, 19.2.1.26)
const PRESENTATION_ORDER: [&str; 15] = [
    "p:sldMasterIdLst",
    "p:notesMasterIdLst",
    "p:handoutMasterIdLst",
    "p:sldIdLst",
    "p:sldSz",
    "p:notesSz",
    "p:smartTags",
    "p:embeddedFontLst",
    "p:custShowLst",
    "p:photoAlbum",
    "p:custDataLst",
    "p:kinsoku",
    "p:defaultTextStyle",
    "p:modifyVerifier",
    "p:extLst",
];

/// the extension uri powerpoint 2010 keeps sections under
const SECTIONS_URI: &str = "{521415D9-36F7-43E2-AB2F-B90AF26B5E84}";

/// the lowest slide id pptx allows (ecma-376 part 1, 19.2.1.33)
const FIRST_SLIDE_ID: u32 = 256;

pub fn write_presentation(
    presentation: &Presentation,
    names: &PartNames,
    package: &mut OutPackage,
) -> Result<(), SlidesError> {
    let text = package
        .text(PRESENTATION_PART)
        .ok_or_else(|| SlidesError::WriteFailed("the presentation part is missing".into()))?;
    let rels = presentation_rels(presentation, names, package);
    let document =
        xml::parse(&text).map_err(|detail| SlidesError::bad_xml(PRESENTATION_PART, detail))?;
    let slide_numbers = slide_numbers(presentation);
    let edits = presentation_edits(presentation, names, &rels, &document.root, &slide_numbers);
    let new_text = if edits.is_empty() {
        text
    } else {
        let borrowed: Vec<ChildEdit<'_>> = edits
            .iter()
            .map(|(names, xml)| ChildEdit {
                names: std::slice::from_ref(names),
                xml: xml.clone(),
            })
            .collect();
        let patched = patch_element(&text, &document.root, None, &borrowed, &PRESENTATION_ORDER);
        format!("{}{patched}", &text[..document.root.start])
    };
    package.set_text(PRESENTATION_PART, &new_text);
    package.set_rels(PRESENTATION_PART, &rels);
    package
        .content_types
        .set_override(PRESENTATION_PART, PRESENTATION_TYPE);
    Ok(())
}

/// the presentation's relationships: masters, slides, the notes master
/// and a theme, keeping every other relationship and every existing id.
fn presentation_rels(presentation: &Presentation, names: &PartNames, package: &OutPackage) -> Rels {
    let mut rels = package.rels_of(PRESENTATION_PART);
    let wanted_slides: HashSet<String> = names
        .slides
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let wanted_masters: HashSet<String> = names
        .masters
        .values()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    rels.items.retain(|rel| {
        let target = rel.resolved(PRESENTATION_PART).to_ascii_lowercase();
        match rel.short_kind() {
            rels::SLIDE => wanted_slides.contains(&target),
            rels::SLIDE_MASTER => wanted_masters.contains(&target),
            _ => true,
        }
    });
    for master in &presentation.masters {
        rels.ensure(
            PRESENTATION_PART,
            &names.masters[&master.id],
            rels::SLIDE_MASTER,
        );
    }
    for slide in &names.slides {
        rels.ensure(PRESENTATION_PART, slide, rels::SLIDE);
    }
    if let Some(notes_master) = package
        .parts
        .iter()
        .map(|(name, _)| name)
        .find(|name| name.starts_with("ppt/notesMasters/") && name.ends_with(".xml"))
    {
        rels.set_single(PRESENTATION_PART, notes_master, rels::NOTES_MASTER);
    }
    let first_theme = presentation
        .themes
        .first()
        .and_then(|t| names.themes.get(&t.id));
    if let (None, Some(theme)) = (rels.first_of(rels::THEME), first_theme) {
        rels.ensure(PRESENTATION_PART, theme, rels::THEME);
    }
    rels
}

/// a pptx slide id for every slide: its own when it has one that is not
/// taken, otherwise the next free number.
fn slide_numbers(presentation: &Presentation) -> Vec<u32> {
    let mut used: HashSet<u32> = HashSet::new();
    let mut numbers = Vec::new();
    let mut next = presentation
        .slides
        .iter()
        .filter_map(|s| s.number_id)
        .max()
        .map_or(FIRST_SLIDE_ID, |max| max + 1)
        .max(FIRST_SLIDE_ID);
    for slide in &presentation.slides {
        let number = match slide.number_id {
            Some(number) if number >= FIRST_SLIDE_ID && used.insert(number) => number,
            _ => {
                let number = next;
                next += 1;
                used.insert(number);
                number
            }
        };
        numbers.push(number);
    }
    numbers
}

/// (child name, new xml) for every presentation child that has to change.
fn presentation_edits(
    presentation: &Presentation,
    names: &PartNames,
    rels: &Rels,
    root: &Element,
    numbers: &[u32],
) -> Vec<(&'static str, Option<String>)> {
    let mut edits = Vec::new();
    let masters = master_list(presentation, names, rels);
    if list_differs(root, "p:sldMasterIdLst", "p:sldMasterId", &masters, rels) {
        edits.push(("p:sldMasterIdLst", Some(master_list_xml(&masters))));
    }
    if let (None, Some(rel)) = (
        root.child("p:notesMasterIdLst"),
        rels.first_of(rels::NOTES_MASTER),
    ) {
        let xml = format!(
            "<p:notesMasterIdLst><p:notesMasterId r:id=\"{}\"/></p:notesMasterIdLst>",
            rel.id
        );
        edits.push(("p:notesMasterIdLst", Some(xml)));
    }
    let slides = slide_list(names, rels, numbers);
    if list_differs(root, "p:sldIdLst", "p:sldId", &slides, rels) {
        edits.push(("p:sldIdLst", Some(slide_list_xml(&slides))));
    }
    deck_setting_edits(presentation, root, numbers, &mut edits);
    edits
}

fn deck_setting_edits(
    presentation: &Presentation,
    root: &Element,
    numbers: &[u32],
    edits: &mut Vec<(&'static str, Option<String>)>,
) {
    if !deck_piece_unchanged(presentation, "size") {
        let size = &presentation.slide_size;
        let kind = size
            .kind
            .as_ref()
            .map(|k| attr("type", k))
            .unwrap_or_default();
        let xml = format!(
            "<p:sldSz{}{}{kind}/>",
            attr("cx", size.width),
            attr("cy", size.height)
        );
        edits.push(("p:sldSz", Some(xml)));
    }
    if !deck_piece_unchanged(presentation, "defaultTextStyle") {
        let media = std::collections::HashMap::new();
        let mut context = WriteContext {
            part: PRESENTATION_PART.to_string(),
            rels: Rels::default(),
            media_parts: &media,
        };
        let xml = write_text_style(
            &presentation.default_text_style,
            "p:defaultTextStyle",
            &mut context,
        );
        edits.push(("p:defaultTextStyle", Some(xml)));
    }
    // sections name slides by number, so a changed slide list changes them too
    let numbers_changed = presentation
        .slides
        .iter()
        .zip(numbers)
        .any(|(s, n)| s.number_id != Some(*n));
    let had_sections = root.child("p:extLst").is_some_and(|list| {
        list.children_named("p:ext")
            .any(|ext| ext.attr("uri") == Some(SECTIONS_URI))
    });
    let has_sections = had_sections || !presentation.sections.is_empty();
    if has_sections && (!deck_piece_unchanged(presentation, "sections") || numbers_changed) {
        let sections = sections_xml(presentation, numbers);
        edits.push(("p:extLst", Some(extensions_with_sections(root, &sections))));
    }
}

/// (number, relationship id) of each master, in deck order.
fn master_list(presentation: &Presentation, names: &PartNames, rels: &Rels) -> Vec<(u64, String)> {
    presentation
        .masters
        .iter()
        .enumerate()
        .filter_map(|(index, master)| {
            let part = &names.masters[&master.id];
            let rel = rels
                .all_of(rels::SLIDE_MASTER)
                .find(|rel| rel.resolved(PRESENTATION_PART).eq_ignore_ascii_case(part))?;
            Some((master_list_id(index), rel.id.clone()))
        })
        .collect()
}

fn slide_list(names: &PartNames, rels: &Rels, numbers: &[u32]) -> Vec<(u64, String)> {
    names
        .slides
        .iter()
        .zip(numbers)
        .filter_map(|(part, number)| {
            let rel = rels
                .all_of(rels::SLIDE)
                .find(|rel| rel.resolved(PRESENTATION_PART).eq_ignore_ascii_case(part))?;
            Some((u64::from(*number), rel.id.clone()))
        })
        .collect()
}

/// true when an id list in the file names other targets, in another
/// order, than `wanted` (numbers of masters are not compared: the
/// original ones stay valid while the masters are the same).
fn list_differs(
    root: &Element,
    list: &str,
    entry: &str,
    wanted: &[(u64, String)],
    rels: &Rels,
) -> bool {
    let Some(element) = root.child(list) else {
        return true;
    };
    let current: Vec<(Option<u64>, String)> = element
        .children_named(entry)
        .map(|e| {
            (
                e.attr("id").and_then(|i| i.parse().ok()),
                e.attr("r:id").unwrap_or("").to_string(),
            )
        })
        .collect();
    let targets_differ = current.len() != wanted.len()
        || current.iter().zip(wanted).any(|((_, a), (_, b))| {
            let target = |id: &str| rels.get(id).map(|r| r.target.clone());
            target(a) != target(b) || a != b
        });
    let numbers_differ = list == "p:sldIdLst"
        && current
            .iter()
            .zip(wanted)
            .any(|((number, _), (wanted, _))| *number != Some(*wanted));
    targets_differ || numbers_differ
}

fn master_list_xml(masters: &[(u64, String)]) -> String {
    let entries: String = masters
        .iter()
        .map(|(id, rel)| format!("<p:sldMasterId{}{}/>", attr("id", id), attr("r:id", rel)))
        .collect();
    format!("<p:sldMasterIdLst>{entries}</p:sldMasterIdLst>")
}

fn slide_list_xml(slides: &[(u64, String)]) -> String {
    let entries: String = slides
        .iter()
        .map(|(id, rel)| format!("<p:sldId{}{}/>", attr("id", id), attr("r:id", rel)))
        .collect();
    format!("<p:sldIdLst>{entries}</p:sldIdLst>")
}

fn sections_xml(presentation: &Presentation, numbers: &[u32]) -> Option<String> {
    if presentation.sections.is_empty() {
        return None;
    }
    let number_of = |id: &str| {
        presentation
            .slide_index(id)
            .and_then(|index| numbers.get(index))
    };
    let mut out = String::from(
        "<p14:sectionLst xmlns:p14=\"http://schemas.microsoft.com/office/powerpoint/2010/main\">",
    );
    for (index, section) in presentation.sections.iter().enumerate() {
        let id = section.id.clone().unwrap_or_else(|| section_guid(index));
        out.push_str(&format!(
            "<p14:section{}{}><p14:sldIdLst>",
            attr("name", &section.name),
            attr("id", id)
        ));
        for number in section.slide_ids.iter().filter_map(|id| number_of(id)) {
            out.push_str(&format!("<p14:sldId{}/>", attr("id", number)));
        }
        out.push_str("</p14:sldIdLst></p14:section>");
    }
    out.push_str("</p14:sectionLst>");
    Some(out)
}

/// a stable guid for a section that has none.
fn section_guid(index: usize) -> String {
    format!("{{6C9E5B1A-0000-4000-8000-{index:012X}}}")
}

/// `p:extLst` with the sections extension replaced, added or removed.
fn extensions_with_sections(root: &Element, sections: &Option<String>) -> String {
    let wrapped = sections
        .as_ref()
        .map(|list| format!("<p:ext uri=\"{SECTIONS_URI}\">{list}</p:ext>"))
        .unwrap_or_default();
    let Some(list) = root.child("p:extLst") else {
        return format!("<p:extLst>{wrapped}</p:extLst>");
    };
    let others: String = list
        .children_named("p:ext")
        .filter(|ext| ext.attr("uri") != Some(SECTIONS_URI))
        .map(outer_xml)
        .collect();
    format!("<p:extLst>{wrapped}{others}</p:extLst>")
}

/// an element written back from the tree (used only for the few small
/// extension elements whose source text is not at hand here).
fn outer_xml(element: &Element) -> String {
    let mut out = format!("<{}", element.name);
    for (key, value) in &element.attributes {
        out.push_str(&attr(key, value));
    }
    out.push('>');
    for node in &element.children {
        match node {
            xml::Node::Element(child) => out.push_str(&outer_xml(child)),
            xml::Node::Text(text) => out.push_str(&xml::escape_text(text)),
        }
    }
    out.push_str(&format!("</{}>", element.name));
    out
}

/// drops content type overrides for parts that are gone and makes sure
/// every part has a type.
pub fn write_content_types(package: &mut OutPackage) {
    let present: HashSet<String> = package
        .parts
        .iter()
        .map(|(n, _)| n.to_ascii_lowercase())
        .collect();
    package
        .content_types
        .overrides
        .retain(|(name, _)| present.contains(&name.to_ascii_lowercase()));
    package.content_types.ensure_default(
        "rels",
        "application/vnd.openxmlformats-package.relationships+xml",
    );
    package
        .content_types
        .ensure_default("xml", "application/xml");
}

/// removes every part no relationship chain from the package root
/// reaches. this is what takes a deleted slide's notes, comments and
/// pictures out of the file.
pub fn drop_unreachable(package: &mut OutPackage) {
    let mut reached: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::from([String::new()]);
    while let Some(part) = queue.pop_front() {
        let rels_name = rels_part_of(&part);
        reached.insert(rels_name.to_ascii_lowercase());
        for rel in package
            .rels_of(&part)
            .items
            .iter()
            .filter(|rel| !rel.external)
        {
            let target = rel.resolved(&part);
            if reached.insert(target.to_ascii_lowercase()) {
                queue.push_back(target);
            }
        }
    }
    reached.insert(CONTENT_TYPES_PART.to_ascii_lowercase());
    package
        .parts
        .retain(|(name, _)| reached.contains(&name.to_ascii_lowercase()));
}
