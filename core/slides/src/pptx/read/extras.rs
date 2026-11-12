//! what a slide carries besides its shapes: its transition, the effects
//! of its animation timing, its notes page and its comments.

use std::collections::HashMap;

use crate::xml::{self, Element};

use super::Reader;
use crate::error::SlidesError;
use crate::model::*;
use crate::pptx::dml::{attr_i64, ReadContext};
use crate::pptx::rels::{self, Rels};
use crate::pptx::text::read_text_body;
use crate::preserve::part_source_of;

pub(super) fn read_transition(root: &Element, source: &str) -> Option<Transition> {
    // a transition is either `p:transition` or wrapped in mc:AlternateContent
    let element = root.elements().find(|child| {
        child.name == "p:transition"
            || (xml::local_name(&child.name) == "AlternateContent"
                && child_contains(child, "p:transition"))
    })?;
    let transition = if element.name == "p:transition" {
        element
    } else {
        find_named(element, "p:transition")?
    };
    let kind = transition
        .elements()
        .next()
        .map(|effect| xml::local_name(&effect.name).to_string())
        .unwrap_or_else(|| "none".into());
    let duration_ms = transition
        .attr("p14:dur")
        .or_else(|| transition.attr("dur"))
        .and_then(|d| d.parse().ok())
        .or_else(|| match transition.attr("spd") {
            Some("fast") => Some(500),
            Some("med") => Some(750),
            Some("slow") => Some(1000),
            _ => None,
        });
    Some(Transition {
        kind,
        duration_ms,
        xml: source[element.start..element.end].to_string(),
        format: SourceFormat::Pptx,
    })
}

pub(super) fn child_contains(element: &Element, name: &str) -> bool {
    find_named(element, name).is_some()
}

pub(super) fn find_named<'a>(element: &'a Element, name: &str) -> Option<&'a Element> {
    for child in element.elements() {
        if child.name == name {
            return Some(child);
        }
        if let Some(found) = find_named(child, name) {
            return Some(found);
        }
    }
    None
}

/// reads the effects of the timing tree in play order.
pub(super) fn read_animations(timing: &Element) -> Vec<Animation> {
    let mut animations = Vec::new();
    timing.walk(&mut |element| {
        if element.name != "p:cTn" {
            return;
        }
        let Some(class) = element.attr("presetClass") else {
            return;
        };
        let mut target = None;
        let mut duration = None;
        element.walk(&mut |inner| {
            if inner.name == "p:spTgt" && target.is_none() {
                target = inner.attr("spid").and_then(|id| id.parse().ok());
            }
            if inner.name == "p:cTn" && duration.is_none() && !std::ptr::eq(inner, element) {
                duration = inner.attr("dur").and_then(|d| d.parse().ok());
            }
        });
        let delay = element
            .child("p:stCondLst")
            .and_then(|list| list.child("p:cond"))
            .and_then(|cond| cond.attr("delay"))
            .and_then(|d| d.parse().ok());
        let Some(target_id) = target else {
            return;
        };
        animations.push(Animation {
            target_id,
            class: match class {
                "entr" => AnimationClass::Entrance,
                "emph" => AnimationClass::Emphasis,
                "exit" => AnimationClass::Exit,
                "path" => AnimationClass::Path,
                _ => AnimationClass::Other,
            },
            preset: element.attr("presetID").map(str::to_string),
            trigger: match element.attr("nodeType") {
                Some("withEffect") => AnimationTrigger::WithPrevious,
                Some("afterEffect") => AnimationTrigger::AfterPrevious,
                _ => AnimationTrigger::OnClick,
            },
            duration_ms: duration,
            delay_ms: delay,
        });
    });
    animations
}

pub(super) fn read_notes(
    reader: &Reader,
    slide_part: &str,
    slide_rels: &Rels,
) -> Result<Option<Notes>, SlidesError> {
    let Some(rel) = slide_rels.first_of(rels::NOTES_SLIDE) else {
        return Ok(None);
    };
    let part = rel.resolved(slide_part);
    let Some(document) = reader.xml(&part)? else {
        return Ok(None);
    };
    let notes_rels = reader.rels(&part);
    let context = ReadContext {
        part: &part,
        text: &document.source,
        rels: &notes_rels,
        media: &reader.media,
    };
    let body = find_notes_body(&document.root)
        .map(|body| read_text_body(body, &context))
        .unwrap_or_default();
    let mut notes = Notes {
        text: body,
        source: None,
    };
    notes.source = Some(part_source_of(
        SourceFormat::Pptx,
        &part,
        &document.source,
        &notes,
        Vec::new(),
    ));
    Ok(Some(notes))
}

/// the text body of the notes page's body placeholder.
pub fn find_notes_body(root: &Element) -> Option<&Element> {
    let tree = root.child("p:cSld")?.child("p:spTree")?;
    tree.children_named("p:sp")
        .find(|shape| {
            shape
                .child("p:nvSpPr")
                .and_then(|nv| nv.child("p:nvPr"))
                .and_then(|nv| nv.child("p:ph"))
                .is_some_and(|ph| {
                    PlaceholderKind::from_pptx(ph.attr("type").unwrap_or("obj"))
                        == PlaceholderKind::Body
                })
        })
        .and_then(|shape| shape.child("p:txBody"))
}

/// comments, legacy (`p:cmLst`) and modern (`p188:cmLst`), with authors.
pub(super) fn read_comments(
    reader: &Reader,
    slide_part: &str,
    slide_rels: &Rels,
) -> Vec<SlideComment> {
    let mut authors: HashMap<String, (String, Option<String>)> = HashMap::new();
    for author_part in ["ppt/commentAuthors.xml", "ppt/authors.xml"] {
        if let Ok(Some(document)) = reader.xml(author_part) {
            for author in document.root.elements() {
                if let Some(id) = author.attr("id") {
                    authors.insert(
                        id.to_string(),
                        (
                            author.attr("name").unwrap_or("").to_string(),
                            author.attr("initials").map(str::to_string),
                        ),
                    );
                }
            }
        }
    }
    let mut comments = Vec::new();
    for rel in slide_rels.all_of(rels::COMMENTS) {
        let part = rel.resolved(slide_part);
        let Ok(Some(document)) = reader.xml(&part) else {
            continue;
        };
        for comment in document.root.elements() {
            let author = comment
                .attr("authorId")
                .and_then(|id| authors.get(id))
                .cloned()
                .unwrap_or_default();
            let position = comment
                .elements()
                .find(|e| xml::local_name(&e.name) == "pos")
                .map(|pos| {
                    (
                        attr_i64(pos, "x").unwrap_or(0),
                        attr_i64(pos, "y").unwrap_or(0),
                    )
                });
            let text = comment
                .elements()
                .find(|e| matches!(xml::local_name(&e.name), "text" | "txBody"))
                .map(|e| e.deep_text())
                .unwrap_or_default();
            comments.push(SlideComment {
                author: author.0,
                initials: author.1,
                date: comment
                    .attr("dt")
                    .or_else(|| comment.attr("created"))
                    .map(str::to_string),
                text,
                position,
            });
        }
    }
    comments
}
