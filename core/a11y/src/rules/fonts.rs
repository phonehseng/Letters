//! fonts: embedded in the file, and able to turn codes into characters.

use std::collections::BTreeMap;

use crate::fonts::UnicodeSource;
use crate::geometry::Rect;
use crate::report::Location;
use crate::rules::{Collector, Context};

/// what is known about one font across the whole document.
struct FontUse {
    name: String,
    embedded: bool,
    unicode: UnicodeSource,
    drawn: bool,
    pages: Vec<u32>,
    first_place: (u32, Rect),
}

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    // fonts are told apart by object; fonts written inline by name and page
    let mut fonts: BTreeMap<String, FontUse> = BTreeMap::new();
    for (page, content) in ctx.contents() {
        for run in &content.texts {
            let summary = &content.fonts[run.font];
            let key = match summary.id {
                Some((number, generation)) => format!("{number} {generation}"),
                None => format!("{} on page {page}", summary.name),
            };
            let entry = fonts.entry(key).or_insert_with(|| FontUse {
                name: summary.name.clone(),
                embedded: summary.embedded,
                unicode: summary.unicode,
                drawn: false,
                pages: Vec::new(),
                first_place: (page, run.rect),
            });
            entry.drawn |= !run.invisible;
            if entry.pages.last() != Some(&page) {
                entry.pages.push(page);
            }
        }
    }
    if fonts.is_empty() {
        out.not_applicable("font-not-embedded", "The document has no text.");
        out.not_applicable("text-not-unicode", "The document has no text.");
        return;
    }
    for font in fonts.values() {
        let (page, rect) = font.first_place;
        let location = Location::page(page, Some(rect));
        // invisible text (render mode 3) is exempt from embedding
        if font.drawn && !font.embedded {
            out.report(
                "font-not-embedded",
                format!(
                    "The font {} isn’t embedded. It is used on {}.",
                    font.name,
                    pages_text(&font.pages)
                ),
                location.clone(),
                None,
            );
        }
        if font.unicode == UnicodeSource::Missing {
            out.report(
                "text-not-unicode",
                format!(
                    "The font {} has no ToUnicode map and no standard encoding. It is used on {}.",
                    font.name,
                    pages_text(&font.pages)
                ),
                location,
                None,
            );
        }
    }
}

/// "page 3", "pages 1, 2 and 5", "12 pages", counting from 1.
pub(crate) fn pages_text(pages: &[u32]) -> String {
    match pages {
        [] => "no pages".to_owned(),
        [one] => format!("page {}", one + 1),
        _ if pages.len() > 6 => format!("{} pages", pages.len()),
        [rest @ .., last] => {
            let rest: Vec<String> = rest.iter().map(|p| (p + 1).to_string()).collect();
            format!("pages {} and {}", rest.join(", "), last + 1)
        }
    }
}
