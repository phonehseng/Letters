//! links, comments and form fields: each needs a tag, a description a
//! screen reader can announce, and the page's tab order must follow the
//! tags.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, ObjectId};

use crate::geometry::{PageFrame, Rect};
use crate::objects::{
    get, get_array, get_dict, get_integer, get_name, get_text, number_box, resolve, ObjectRef,
};
use crate::report::{FixKind, Location};
use crate::rules::{Collector, Context};

/// one annotation on a page.
pub(crate) struct Annotation<'a> {
    pub id: Option<ObjectId>,
    pub dict: &'a Dictionary,
    pub subtype: String,
    pub rect: Option<Rect>,
    pub hidden: bool,
}

/// the annotations of a page, in the order the page lists them.
pub(crate) fn page_annotations<'a>(
    doc: &'a Document,
    page: ObjectId,
    frame: &PageFrame,
) -> Vec<Annotation<'a>> {
    let Ok(page_dict) = doc.get_dictionary(page) else {
        return Vec::new();
    };
    get_array(doc, page_dict, b"Annots")
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            let id = item.as_reference().ok();
            let dict = resolve(doc, item).as_dict().ok()?;
            let subtype = get_name(doc, dict, b"Subtype")
                .map(crate::objects::name_text)
                .unwrap_or_default();
            let rect = get(doc, dict, b"Rect")
                .and_then(number_box)
                .map(|b| frame.visible_box(b));
            // flag bit 2 hides the annotation
            let hidden = get_integer(doc, dict, b"F").unwrap_or(0) & 2 != 0;
            Some(Annotation {
                id,
                dict,
                subtype,
                rect,
                hidden,
            })
        })
        .collect()
}

/// annotations that are never content: pop-ups belong to their parent,
/// and printer marks are artifacts by definition.
fn is_ignored(annotation: &Annotation) -> bool {
    annotation.hidden
        || matches!(
            annotation.subtype.as_str(),
            "Popup" | "PrinterMark" | "TrapNet"
        )
}

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    let owners = ctx
        .tree
        .as_ref()
        .map(|tree| tree.object_owners())
        .unwrap_or_default();
    let mut seen_kinds: HashSet<&'static str> = HashSet::new();
    for (page, info) in ctx.pages.iter().enumerate() {
        let page = page as u32;
        let annotations = page_annotations(ctx.doc, info.id, &info.frame);
        let relevant: Vec<&Annotation> = annotations.iter().filter(|a| !is_ignored(a)).collect();
        if relevant.is_empty() {
            continue;
        }
        check_tab_order(ctx, info.id, page, out);
        for annotation in relevant {
            let location = Location {
                page: Some(page),
                rect: annotation.rect,
                annotation: annotation.id.map(ObjectRef::from),
                ..Location::default()
            };
            let owner_role = annotation
                .id
                .and_then(|id| owners.get(&id))
                .and_then(|&owner| {
                    ctx.tree
                        .as_ref()
                        .map(|tree| tree.elements[owner].role.clone())
                });
            match annotation.subtype.as_str() {
                "Link" => {
                    seen_kinds.insert("link");
                    check_link(ctx, annotation, owner_role.as_deref(), location, out);
                }
                "Widget" => {
                    seen_kinds.insert("field");
                    check_field(ctx, annotation, owner_role.as_deref(), location, out);
                }
                _ => {
                    seen_kinds.insert("annotation");
                    check_other(ctx, annotation, owner_role.as_deref(), location, out);
                }
            }
        }
    }
    let untagged_reason = "The document has no tags yet; see “The document has no tags”.";
    if !ctx.tagged {
        for id in [
            "link-not-tagged",
            "link-text-not-descriptive",
            "annotation-not-tagged",
            "field-not-tagged",
        ] {
            out.not_checked(id, untagged_reason);
        }
    }
    let none = |kind: &str| !seen_kinds.contains(kind);
    if none("link") {
        for id in [
            "link-not-tagged",
            "link-missing-description",
            "link-text-not-descriptive",
        ] {
            out.not_applicable(id, "The document has no links.");
        }
    }
    if none("field") {
        for id in ["field-missing-tooltip", "field-not-tagged"] {
            out.not_applicable(id, "The document has no form fields.");
        }
    }
    if none("annotation") {
        for id in ["annotation-not-tagged", "annotation-missing-contents"] {
            out.not_applicable(id, "The document has no comments or other annotations.");
        }
    }
    if seen_kinds.is_empty() {
        out.not_applicable(
            "tab-order-not-structure",
            "No page has links, comments or fields.",
        );
    }
}

fn check_tab_order(ctx: &Context, page_id: ObjectId, page: u32, out: &mut Collector) {
    let tabs = ctx
        .doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|dict| get_name(ctx.doc, dict, b"Tabs"));
    if tabs != Some(b"S") {
        out.report(
            "tab-order-not-structure",
            format!(
                "Page {} has links, comments or fields but no /Tabs /S.",
                page + 1
            ),
            Location::page(page, None),
            Some(FixKind::SetTabOrder),
        );
    }
}

fn has_text(ctx: &Context, dict: &Dictionary, key: &[u8]) -> bool {
    get_text(ctx.doc, dict, key).is_some_and(|text| !text.trim().is_empty())
}

fn check_link(
    ctx: &Context,
    annotation: &Annotation,
    owner: Option<&str>,
    location: Location,
    out: &mut Collector,
) {
    let id = annotation.id.map(ObjectRef::from);
    if ctx.tagged && owner != Some("Link") {
        out.report(
            "link-not-tagged",
            "This link area is not inside a Link tag.",
            location.clone(),
            Some(FixKind::TagAnnotations),
        );
    }
    let text = link_text(ctx, annotation, location.page);
    let suggested = suggested_link_text(ctx, annotation, &text);
    if !has_text(ctx, annotation.dict, b"Contents") {
        out.report(
            "link-missing-description",
            match &text {
                Some(text) => format!(
                    "The link on “{}” has no /Contents.",
                    super::headings::short(text)
                ),
                None => "The link has no /Contents.".to_owned(),
            },
            location.clone(),
            id.map(|annotation| FixKind::AddLinkText {
                annotation,
                suggested: suggested.clone(),
            }),
        );
    }
    let described_well = get_text(ctx.doc, annotation.dict, b"Contents")
        .is_some_and(|contents| !is_vague_link_text(&contents));
    if ctx.tagged && !described_well {
        if let Some(text) = text.filter(|text| is_vague_link_text(text)) {
            out.report(
                "link-text-not-descriptive",
                format!("The link’s text is “{}”.", super::headings::short(&text)),
                location,
                id.map(|annotation| FixKind::AddLinkText {
                    annotation,
                    suggested,
                }),
            );
        }
    }
}

/// the words a link sits on: the text under its rectangle.
fn link_text(ctx: &Context, annotation: &Annotation, page: Option<u32>) -> Option<String> {
    let rect = annotation.rect?;
    let content = ctx.pages.get(page? as usize)?.content.as_ref()?;
    let words: Vec<&str> = content
        .texts
        .iter()
        .filter(|run| rect.contains_point(run.rect.center()) && !run.invisible)
        .map(|run| run.text.trim())
        .filter(|text| !text.is_empty())
        .collect();
    let text = words.join(" ");
    (!text.is_empty()).then_some(text)
}

/// a description to offer: the link's own words when they say something,
/// otherwise where it leads.
fn suggested_link_text(
    ctx: &Context,
    annotation: &Annotation,
    text: &Option<String>,
) -> Option<String> {
    if let Some(text) = text.as_ref().filter(|t| !is_vague_link_text(t)) {
        return Some(text.clone());
    }
    let action = get_dict(ctx.doc, annotation.dict, b"A")?;
    get_text(ctx.doc, action, b"URI").map(|uri| {
        let readable = uri
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("mailto:")
            .trim_end_matches('/');
        format!("Link to {readable}")
    })
}

/// text that doesn't say where a link goes.
pub(crate) fn is_vague_link_text(text: &str) -> bool {
    let lower = text.trim().trim_end_matches(['.', '!', ':']).to_lowercase();
    const VAGUE: &[&str] = &[
        "click here",
        "here",
        "click",
        "link",
        "more",
        "read more",
        "learn more",
        "this",
        "this link",
        "go",
        "details",
        "more info",
        "info",
        "see here",
        "click this link",
    ];
    if VAGUE.contains(&lower.as_str()) {
        return true;
    }
    // a bare web address is read out letter by letter
    (lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www."))
        && !lower.contains(' ')
}

fn check_field(
    ctx: &Context,
    annotation: &Annotation,
    owner: Option<&str>,
    location: Location,
    out: &mut Collector,
) {
    if ctx.tagged && owner != Some("Form") {
        out.report(
            "field-not-tagged",
            "This form field is not inside a Form tag.",
            location.clone(),
            Some(FixKind::TagAnnotations),
        );
    }
    let (field_id, tooltip, name) = field_names(ctx.doc, annotation);
    if tooltip.is_none() {
        let field = field_id.or(annotation.id).map(ObjectRef::from);
        out.report(
            "field-missing-tooltip",
            match &name {
                Some(name) => format!("The field “{name}” has no tooltip (/TU)."),
                None => "A form field has no tooltip (/TU).".to_owned(),
            },
            location,
            field.map(|field| FixKind::SetFieldTooltip {
                field,
                suggested: name.as_deref().map(readable_field_name),
            }),
        );
    }
}

/// the field a widget belongs to, its tooltip and its internal name,
/// looking up through parent fields (a widget can be the kid of a field).
pub(crate) fn field_names(
    doc: &Document,
    annotation: &Annotation,
) -> (Option<ObjectId>, Option<String>, Option<String>) {
    let mut dict = annotation.dict;
    let mut id = annotation.id;
    let mut field_id = None;
    let mut tooltip = None;
    let mut name = None;
    for _ in 0..16 {
        if field_id.is_none() && dict.has(b"T") {
            field_id = id;
        }
        if tooltip.is_none() {
            tooltip = get_text(doc, dict, b"TU").filter(|t| !t.trim().is_empty());
        }
        if name.is_none() {
            name = get_text(doc, dict, b"T").filter(|t| !t.trim().is_empty());
        }
        let Some(parent) = dict.get(b"Parent").ok().and_then(|p| p.as_reference().ok()) else {
            break;
        };
        let Ok(parent_dict) = doc.get_dictionary(parent) else {
            break;
        };
        id = Some(parent);
        dict = parent_dict;
    }
    (field_id, tooltip, name)
}

/// "first_name" → "First name", a reasonable starting point for a tooltip.
fn readable_field_name(name: &str) -> String {
    let mut spaced = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        match c {
            '_' | '-' | '.' => spaced.push(' '),
            // a capital after a small letter starts a new word: firstName
            c if c.is_uppercase() && previous_lower => {
                spaced.push(' ');
                spaced.push(c);
            }
            c => spaced.push(c),
        }
        previous_lower = c.is_lowercase();
    }
    let words = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => name.to_owned(),
    }
}

fn check_other(
    ctx: &Context,
    annotation: &Annotation,
    owner: Option<&str>,
    location: Location,
    out: &mut Collector,
) {
    if ctx.tagged && owner.is_none() {
        out.report(
            "annotation-not-tagged",
            format!("A {} annotation is not in the tags.", annotation.subtype),
            location.clone(),
            Some(FixKind::TagAnnotations),
        );
    }
    if !has_text(ctx, annotation.dict, b"Contents") {
        out.report(
            "annotation-missing-contents",
            format!("A {} annotation has no /Contents.", annotation.subtype),
            location,
            annotation.id.map(|id| FixKind::SetAnnotationContents {
                annotation: ObjectRef::from(id),
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vague_link_text_is_caught() {
        for text in [
            "Click here",
            "here.",
            "Read more",
            "https://example.com/a/b",
            "www.example.com",
        ] {
            assert!(is_vague_link_text(text), "{text}");
        }
        for text in ["Library opening hours", "Assignment 2 brief (PDF)"] {
            assert!(!is_vague_link_text(text), "{text}");
        }
    }

    #[test]
    fn field_names_become_readable() {
        assert_eq!(readable_field_name("first_name"), "First name");
        assert_eq!(readable_field_name("studentID"), "Student ID");
        assert_eq!(readable_field_name("Email"), "Email");
    }
}
