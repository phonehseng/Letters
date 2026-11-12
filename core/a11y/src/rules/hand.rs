//! the checks only a person can make. pdf/ua needs them as much as the
//! machine checks, so they are listed whenever they apply.

use crate::report::Location;
use crate::rules::{Collector, Context};

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    let has_text = ctx.contents().any(|(_, content)| !content.texts.is_empty());
    let has_images = ctx
        .contents()
        .any(|(_, content)| !content.images.is_empty());
    if let Some(tree) = ctx.tree.as_ref().filter(|_| ctx.tagged) {
        let described = tree
            .elements
            .iter()
            .filter(|e| {
                matches!(e.role.as_str(), "Figure" | "Formula") && e.description().is_some()
            })
            .count();
        if described > 0 {
            out.report(
                "check-alt-text-meaningful",
                format!("{described} pictures or formulas have alt text to read through."),
                Location::document(),
                None,
            );
        }
        if has_text {
            out.report(
                "check-reading-order-logical",
                "Step through each page in the reading order view.",
                Location::document(),
                None,
            );
            out.report(
                "check-language-changes",
                "Look for quotations and terms in other languages.",
                Location::document(),
                None,
            );
        }
        let headings = tree
            .elements
            .iter()
            .filter(|e| e.is_heading().is_some() || e.role == "H")
            .count();
        if headings > 0 {
            out.report(
                "check-headings-for-structure",
                format!("The document has {headings} headings to look through."),
                Location::document(),
                None,
            );
        }
    }
    let coloured_text = ctx.contents().any(|(_, content)| {
        content
            .texts
            .iter()
            .filter_map(|run| run.colour)
            .any(|c| c.r.abs_diff(c.g) > 24 || c.g.abs_diff(c.b) > 24 || c.r.abs_diff(c.b) > 24)
    });
    if coloured_text || has_images {
        out.report(
            "check-colour-not-only-meaning",
            "The document uses coloured text or pictures; check that colour isn’t the only way they carry meaning.",
            Location::document(),
            None,
        );
    }
}
