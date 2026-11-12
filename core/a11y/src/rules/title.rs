//! the document title, and whether readers show it.

use crate::objects::{get, get_bool, get_dict, get_text};
use crate::report::{FixKind, Location};
use crate::rules::{Collector, Context};
use crate::xmp;

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    let info_title = info_title(ctx);
    let xmp_title = xmp::read_packet(ctx.doc).as_deref().and_then(xmp::title);
    if xmp_title.is_none() {
        let details = match &info_title {
            Some(title) => format!(
                "The title “{title}” is only in the old document information, not in the XMP metadata PDF/UA reads."
            ),
            None => "Neither the document information nor the XMP metadata has a title.".to_owned(),
        };
        let suggested = info_title.clone().or_else(|| suggest_title(ctx));
        out.report(
            "missing-title",
            details,
            Location::document(),
            Some(FixKind::SetTitle { suggested }),
        );
    }
    let displayed = ctx
        .doc
        .catalog()
        .ok()
        .and_then(|catalog| get_dict(ctx.doc, catalog, b"ViewerPreferences"))
        .and_then(|preferences| get_bool(ctx.doc, preferences, b"DisplayDocTitle"))
        .unwrap_or(false);
    if !displayed {
        out.report(
            "title-not-displayed",
            "ViewerPreferences /DisplayDocTitle is not set to true.",
            Location::document(),
            Some(FixKind::ShowTitle),
        );
    }
}

fn info_title(ctx: &Context) -> Option<String> {
    let info = get(ctx.doc, &ctx.doc.trailer, b"Info")?.as_dict().ok()?;
    get_text(ctx.doc, info, b"Title")
        .map(|title| title.trim().to_owned())
        .filter(|title| !title.is_empty())
}

/// a title to offer: the first level 1 heading, or else the biggest text
/// on the first page.
pub(crate) fn suggest_title(ctx: &Context) -> Option<String> {
    if let Some(tree) = &ctx.tree {
        let heading = tree
            .preorder()
            .into_iter()
            .find(|&index| tree.elements[index].is_heading() == Some(1));
        if let Some(heading) = heading {
            let text = ctx.element_text(heading);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    let (_, content) = ctx.contents().next()?;
    let biggest = content
        .texts
        .iter()
        .filter(|run| !run.invisible && !run.text.trim().is_empty())
        .map(|run| run.font_size)
        .fold(0.0f32, f32::max);
    let text: Vec<&str> = content
        .texts
        .iter()
        .filter(|run| (run.font_size - biggest).abs() < 0.5 && !run.invisible)
        .map(|run| run.text.trim())
        .collect();
    let title = text
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty() && title.chars().count() <= 200).then_some(title)
}
