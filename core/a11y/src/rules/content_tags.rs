//! in a tagged document every piece of content must be tagged or marked
//! as decoration (an artifact). this finds what is neither.

use crate::content::Mark;
use crate::geometry::Rect;
use crate::report::{FixKind, Location};
use crate::rules::{Collector, Context};

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    if !ctx.tagged {
        super::skip_untagged(out, &["untagged-content"]);
        return;
    }
    for (page, content) in ctx.contents() {
        let texts: Vec<&Rect> = content
            .texts
            .iter()
            .filter(|run| run.mark == Mark::Untagged)
            .map(|run| &run.rect)
            .collect();
        let shapes: Vec<&Rect> = content
            .paths
            .iter()
            .filter(|path| path.mark == Mark::Untagged)
            .map(|path| &path.rect)
            .collect();
        if texts.is_empty() && shapes.is_empty() {
            continue;
        }
        let mut parts = Vec::new();
        if !texts.is_empty() {
            parts.push(plural(texts.len(), "piece of text", "pieces of text"));
        }
        if !shapes.is_empty() {
            parts.push(plural(shapes.len(), "shape", "shapes"));
        }
        let rect = Rect::union_all(texts.iter().chain(shapes.iter()).copied());
        out.report(
            "untagged-content",
            format!("Page {} has {} with no tag.", page + 1, parts.join(" and ")),
            Location::page(page, rect),
            Some(FixKind::TagUntaggedContent { page }),
        );
    }
    let unreadable: Vec<u32> = (0..ctx.page_count())
        .filter(|&page| ctx.pages[page as usize].content.is_none())
        .collect();
    if !unreadable.is_empty() && unreadable.len() == ctx.pages.len() {
        out.not_checked(
            "untagged-content",
            "The pages’ drawing instructions couldn’t be read.",
        );
    }
}

pub(crate) fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}
