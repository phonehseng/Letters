//! writing slides and their notes pages. a slide keeps the relationships
//! of the part it was read from (a duplicated slide those of its
//! original), so the relationship ids inside its xml stay valid.

use std::collections::HashMap;

use crate::xml;

use super::super::dml::WriteContext;
use super::super::read::find_notes_body;
use super::super::rels::{self, Rels};
use super::super::text::{write_paragraph, write_text_body};
use super::canvas::{
    fresh_canvas, patched_part, with_root_attributes, Canvas, NAMESPACES, XML_DECLARATION,
};
use super::fresh::{ensure_notes_master, fresh_notes};
use super::names::PartNames;
use super::package::OutPackage;
use crate::model::{Notes, Presentation, Slide, SourceFormat};
use crate::preserve::{reusable, start_tag_end};

pub const SLIDE_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
pub const NOTES_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml";

pub fn write_slides(
    presentation: &Presentation,
    names: &PartNames,
    media: &HashMap<String, String>,
    package: &mut OutPackage,
) {
    for (index, slide) in presentation.slides.iter().enumerate() {
        let part = names.slides[index].clone();
        let rels = slide_rels(presentation, slide, &part, names, index, package);
        let mut context = WriteContext {
            part: part.clone(),
            rels,
            media_parts: media,
        };
        let text = slide_text(slide, &mut context);
        package.set_text(&part, &text);
        package.set_rels(&part, &context.rels);
        package.content_types.set_override(&part, SLIDE_TYPE);
        if let (Some(notes), Some(notes_part)) = (&slide.notes, &names.notes[index]) {
            write_notes(presentation, notes, &part, notes_part, media, package);
        }
    }
}

/// the part whose relationships a slide's xml refers to.
fn base_part<'a>(presentation: &'a Presentation, slide: &'a Slide) -> Option<&'a str> {
    let from_source = slide
        .source
        .as_ref()
        .filter(|s| s.format == SourceFormat::Pptx)
        .and_then(|s| s.part.as_deref());
    from_source.or_else(|| presentation.carry.part_name_of(&slide.id))
}

fn slide_rels(
    presentation: &Presentation,
    slide: &Slide,
    part: &str,
    names: &PartNames,
    index: usize,
    package: &OutPackage,
) -> Rels {
    let base = base_part(presentation, slide);
    let mut rels = base.map(|base| package.rels_of(base)).unwrap_or_default();
    if base != Some(part) {
        // a duplicate must not share its original's comments
        rels.remove_kind(rels::COMMENTS);
    }
    if let Some(layout) = slide
        .layout_id
        .as_ref()
        .and_then(|id| names.layouts.get(id))
    {
        rels.set_single(part, layout, rels::SLIDE_LAYOUT);
    }
    match &names.notes[index] {
        Some(notes) => {
            rels.set_single(part, notes, rels::NOTES_SLIDE);
        }
        None => rels.remove_kind(rels::NOTES_SLIDE),
    }
    rels
}

fn slide_text(slide: &Slide, context: &mut WriteContext<'_>) -> String {
    let mut plain = slide.clone();
    plain.source = None;
    if let Some(text) = reusable(slide.source.as_ref(), SourceFormat::Pptx, &plain) {
        return text.to_string();
    }
    let canvas = Canvas {
        name: slide.name.as_deref(),
        background: slide.background.as_ref(),
        objects: &slide.objects,
    };
    let attributes = [
        ("show", slide.hidden.then(|| "0".to_string())),
        (
            "showMasterSp",
            (!slide.show_master_shapes).then(|| "0".to_string()),
        ),
    ];
    let original = slide
        .source
        .as_ref()
        .filter(|s| s.format == SourceFormat::Pptx)
        .and_then(|s| patched_part(&s.xml, &canvas, context));
    match original {
        Some(text) => with_root_attributes(&text, &attributes),
        None => fresh_slide(slide, &canvas, &attributes, context),
    }
}

fn fresh_slide(
    slide: &Slide,
    canvas: &Canvas<'_>,
    attributes: &[(&str, Option<String>)],
    context: &mut WriteContext<'_>,
) -> String {
    let root_attributes: String = attributes
        .iter()
        .filter_map(|(name, value)| value.as_ref().map(|v| format!(" {name}=\"{v}\"")))
        .collect();
    let color_map = match &slide.color_map {
        Some(map) => {
            let entries: String = map
                .entries
                .iter()
                .map(|(k, v)| format!(" {k}=\"{v}\""))
                .collect();
            format!("<p:clrMapOvr><a:overrideClrMapping{entries}/></p:clrMapOvr>")
        }
        None => "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>".to_string(),
    };
    let transition = slide
        .transition
        .as_ref()
        .filter(|t| t.format == SourceFormat::Pptx)
        .map(|t| t.xml.clone())
        .unwrap_or_default();
    format!(
        "{XML_DECLARATION}<p:sld{NAMESPACES}{root_attributes}>{}{color_map}{transition}</p:sld>",
        fresh_canvas(canvas, context)
    )
}

fn write_notes(
    presentation: &Presentation,
    notes: &Notes,
    slide_part: &str,
    notes_part: &str,
    media: &HashMap<String, String>,
    package: &mut OutPackage,
) {
    let source = notes
        .source
        .as_ref()
        .filter(|s| s.format == SourceFormat::Pptx);
    let base = source.and_then(|s| s.part.as_deref()).unwrap_or(notes_part);
    let mut rels = package.rels_of(base);
    rels.set_single(notes_part, slide_part, rels::SLIDE);
    let master = ensure_notes_master(presentation, package);
    rels.set_single(notes_part, &master, rels::NOTES_MASTER);
    let mut context = WriteContext {
        part: notes_part.to_string(),
        rels,
        media_parts: media,
    };
    let mut plain = notes.clone();
    plain.source = None;
    let text = match reusable(source, SourceFormat::Pptx, &plain) {
        Some(text) => text.to_string(),
        None => source
            .and_then(|s| patched_notes(&s.xml, notes, &mut context))
            .unwrap_or_else(|| fresh_notes(notes, &mut context)),
    };
    package.set_text(notes_part, &text);
    package.set_rels(notes_part, &context.rels);
    package.content_types.set_override(notes_part, NOTES_TYPE);
}

/// the original notes page with the body placeholder's paragraphs
/// rebuilt; its body and list style properties stay as they were.
fn patched_notes(text: &str, notes: &Notes, context: &mut WriteContext<'_>) -> Option<String> {
    let document = xml::parse(text).ok()?;
    let body = find_notes_body(&document.root)?;
    let raw = &text[body.start..body.end];
    let tag_end = start_tag_end(raw);
    if raw[..tag_end].ends_with("/>") {
        return None;
    }
    let mut new_body = raw[..tag_end].to_string();
    for name in ["a:bodyPr", "a:lstStyle"] {
        if let Some(element) = body.child(name) {
            new_body.push_str(&text[element.start..element.end]);
        }
    }
    for paragraph in &notes.text.paragraphs {
        new_body.push_str(&write_paragraph(paragraph, context));
    }
    if notes.text.paragraphs.is_empty() {
        new_body.push_str("<a:p><a:endParaRPr lang=\"en-US\" dirty=\"0\"/></a:p>");
    }
    new_body.push_str(&raw[raw.rfind("</").unwrap_or(raw.len())..]);
    let mut out = text.to_string();
    out.replace_range(body.start..body.end, &new_body);
    Some(out)
}

/// a notes text body written whole (for fresh notes pages).
pub fn notes_body_xml(notes: &Notes, context: &mut WriteContext<'_>) -> String {
    write_text_body(&notes.text, "p:txBody", context)
}
