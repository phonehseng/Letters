//! the document as a whole: bookmarks, page labels, security settings,
//! scripts and media, and pages that are only a scanned picture.

use lopdf::{Document, Object};

use crate::numtree;
use crate::objects::{get, get_dict, get_name, resolve};
use crate::report::{FixKind, Location};
use crate::rules::fonts::pages_text;
use crate::rules::{Collector, Context};

/// a page is "only a picture" when pictures cover at least this share of it.
const SCANNED_COVERAGE: f32 = 0.5;

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    check_bookmarks(ctx, out);
    check_page_labels(ctx, out);
    check_security(ctx, out);
    check_scripts_and_media(ctx, out);
    check_scanned_pages(ctx, out);
}

fn check_bookmarks(ctx: &Context, out: &mut Collector) {
    if ctx.page_count() < ctx.options.bookmark_page_threshold {
        out.not_applicable(
            "no-bookmarks",
            format!(
                "Documents shorter than {} pages don’t need bookmarks.",
                ctx.options.bookmark_page_threshold
            ),
        );
        return;
    }
    let has_bookmarks = ctx
        .doc
        .catalog()
        .ok()
        .and_then(|catalog| get_dict(ctx.doc, catalog, b"Outlines"))
        .is_some_and(|outlines| get(ctx.doc, outlines, b"First").is_some());
    if has_bookmarks {
        return;
    }
    let has_headings = ctx
        .tree
        .as_ref()
        .is_some_and(|tree| tree.elements.iter().any(|e| e.is_heading().is_some()));
    out.report(
        "no-bookmarks",
        format!("This {}-page document has no bookmarks.", ctx.page_count()),
        Location::document(),
        has_headings.then_some(FixKind::AddBookmarksFromHeadings),
    );
}

fn check_page_labels(ctx: &Context, out: &mut Collector) {
    let Some(labels) = ctx
        .doc
        .catalog()
        .ok()
        .and_then(|catalog| get_dict(ctx.doc, catalog, b"PageLabels"))
    else {
        out.not_applicable("page-labels-invalid", "The document has no page labels.");
        return;
    };
    let entries = numtree::read(ctx.doc, labels);
    let mut problems = Vec::new();
    if entries.keys().next() != Some(&0) {
        problems.push("The labels don’t start at the first page.".to_owned());
    }
    if let Some(&last) = entries.keys().last() {
        if last >= ctx.page_count() as i64 || last < 0 {
            problems.push(format!(
                "A label range starts at page {}, past the end of the document.",
                last + 1
            ));
        }
    }
    for value in entries.values() {
        let Ok(range) = resolve(ctx.doc, value).as_dict() else {
            problems.push("A label range is damaged.".to_owned());
            continue;
        };
        if let Some(style) = get_name(ctx.doc, range, b"S") {
            if !matches!(style, b"D" | b"R" | b"r" | b"A" | b"a") {
                problems.push("A label range uses an unknown numbering style.".to_owned());
            }
        }
        if get(ctx.doc, range, b"St")
            .and_then(|s| s.as_i64().ok())
            .is_some_and(|start| start < 1)
        {
            problems.push("A label range starts counting below 1.".to_owned());
        }
    }
    problems.dedup();
    if !problems.is_empty() {
        out.report(
            "page-labels-invalid",
            problems.join(" "),
            Location::document(),
            None,
        );
    }
}

fn check_security(ctx: &Context, out: &mut Collector) {
    let Some(state) = &ctx.doc.encryption_state else {
        out.not_applicable(
            "security-blocks-assistive-tech",
            "The document has no security settings.",
        );
        return;
    };
    if !state
        .permissions()
        .contains(lopdf::Permissions::COPYABLE_FOR_ACCESSIBILITY)
    {
        out.report(
            "security-blocks-assistive-tech",
            "The permission “content copying for accessibility” is turned off.",
            Location::document(),
            None,
        );
    }
}

fn check_scripts_and_media(ctx: &Context, out: &mut Collector) {
    let mut found = Vec::new();
    if let Ok(catalog) = ctx.doc.catalog() {
        if get_dict(ctx.doc, catalog, b"Names")
            .is_some_and(|names| get(ctx.doc, names, b"JavaScript").is_some())
        {
            found.push("document scripts");
        }
        if get(ctx.doc, catalog, b"OpenAction").is_some_and(|action| is_script(ctx.doc, action)) {
            found.push("a script that runs when the document opens");
        }
        if catalog.has(b"AA") {
            found.push("document actions");
        }
    }
    for info in &ctx.pages {
        let Ok(page_dict) = ctx.doc.get_dictionary(info.id) else {
            continue;
        };
        if page_dict.has(b"AA") && !found.contains(&"page actions") {
            found.push("page actions");
        }
        for annotation in super::annotations::page_annotations(ctx.doc, info.id, &info.frame) {
            if matches!(
                annotation.subtype.as_str(),
                "Screen" | "Movie" | "Sound" | "RichMedia" | "3D"
            ) && !found.contains(&"video, sound or 3D content")
            {
                found.push("video, sound or 3D content");
            }
            if get(ctx.doc, annotation.dict, b"A").is_some_and(|action| is_script(ctx.doc, action))
                && !found.contains(&"links that run scripts")
            {
                found.push("links that run scripts");
            }
        }
    }
    if !found.is_empty() {
        out.report(
            "scripted-or-moving-content",
            format!("The document contains {}.", found.join(", ")),
            Location::document(),
            None,
        );
    }
}

fn is_script(doc: &Document, action: &Object) -> bool {
    resolve(doc, action)
        .as_dict()
        .ok()
        .and_then(|action| get_name(doc, action, b"S"))
        == Some(b"JavaScript")
}

fn check_scanned_pages(ctx: &Context, out: &mut Collector) {
    let mut scanned = Vec::new();
    for (page, content) in ctx.contents() {
        if !content.texts.is_empty() || content.images.is_empty() {
            continue;
        }
        let page_area = ctx.pages[page as usize].frame.visible_area();
        let covered: f32 = content.images.iter().map(|image| image.rect.area()).sum();
        if page_area > 0.0 && covered / page_area >= SCANNED_COVERAGE {
            scanned.push(page);
        }
    }
    if scanned.is_empty() {
        return;
    }
    out.report(
        "image-only-page",
        format!(
            "{} only a picture, with no text behind it.",
            capitalise(&pages_text(&scanned)) + if scanned.len() == 1 { " is" } else { " are" }
        ),
        Location::page(scanned[0], None),
        Some(FixKind::RecogniseText { pages: scanned }),
    );
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
