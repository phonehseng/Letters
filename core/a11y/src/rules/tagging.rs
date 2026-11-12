//! is the document tagged at all, and is the tag tree sound.

use crate::objects::{get_bool, get_dict};
use crate::report::{FixKind, Location};
use crate::rules::{Collector, Context};

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    if !ctx.tagged {
        out.report(
            "not-tagged",
            "This document has no structure tree, so screen readers can’t tell its parts apart.",
            Location::document(),
            Some(FixKind::AddTags),
        );
        super::skip_untagged(out, &["tagged-flag-missing", "structure-tree-damaged"]);
        return;
    }
    let marked = ctx
        .doc
        .catalog()
        .ok()
        .and_then(|catalog| get_dict(ctx.doc, catalog, b"MarkInfo"))
        .and_then(|info| get_bool(ctx.doc, info, b"Marked"))
        .unwrap_or(false);
    if !marked {
        out.report(
            "tagged-flag-missing",
            "The tags are there, but MarkInfo /Marked is not set to true.",
            Location::document(),
            Some(FixKind::MarkAsTagged),
        );
    }
    let problems = ctx
        .tree
        .as_ref()
        .map(|t| t.problems.clone())
        .unwrap_or_default();
    if !problems.is_empty() {
        let mut details = problems
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        if problems.len() > 3 {
            details.push_str(&format!(" ({} problems in all.)", problems.len()));
        }
        out.report(
            "structure-tree-damaged",
            details,
            Location::document(),
            None,
        );
    }
}
