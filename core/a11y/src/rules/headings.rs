//! headings: a long document should have them, and numbered levels must
//! start at 1 and never skip a level going down.

use crate::report::{FixKind, Location};
use crate::rules::{Collector, Context};

/// documents with more words than this are expected to have headings,
/// even on a single page.
const WORDS_NEEDING_HEADINGS: usize = 300;

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    let ids = [
        "no-headings",
        "first-heading-not-h1",
        "heading-level-skipped",
    ];
    let (true, Some(tree)) = (ctx.tagged, &ctx.tree) else {
        super::skip_untagged(out, &ids);
        return;
    };
    let headings: Vec<(usize, u8)> = tree
        .preorder()
        .into_iter()
        .filter_map(|index| Some((index, tree.elements[index].is_heading()?)))
        .collect();
    let has_plain_h = tree.elements.iter().any(|element| element.role == "H");
    if headings.is_empty() && !has_plain_h {
        if long_enough(ctx) {
            out.report(
                "no-headings",
                format!(
                    "This {}-page document has no heading tags.",
                    ctx.page_count()
                ),
                Location::document(),
                None,
            );
        } else {
            out.not_applicable(
                "no-headings",
                "The document is short enough not to need headings.",
            );
        }
        out.not_applicable(
            "first-heading-not-h1",
            "The document has no numbered headings.",
        );
        out.not_applicable(
            "heading-level-skipped",
            "The document has no numbered headings.",
        );
        return;
    }
    if let Some(&(first, level)) = headings.first() {
        if level != 1 {
            out.report(
                "first-heading-not-h1",
                format!(
                    "The first heading is an H{level}: “{}”.",
                    short(&ctx.element_text(first))
                ),
                ctx.element_location(first),
                Some(FixKind::FixHeadingLevels),
            );
        }
    }
    for pair in headings.windows(2) {
        let (_, previous) = pair[0];
        let (index, level) = pair[1];
        if level > previous + 1 {
            out.report(
                "heading-level-skipped",
                format!(
                    "An H{previous} is followed by an H{level}: “{}”.",
                    short(&ctx.element_text(index))
                ),
                ctx.element_location(index),
                Some(FixKind::FixHeadingLevels),
            );
        }
    }
}

fn long_enough(ctx: &Context) -> bool {
    if ctx.page_count() >= 2 {
        return true;
    }
    let words: usize = ctx
        .contents()
        .flat_map(|(_, content)| content.texts.iter())
        .map(|run| run.text.split_whitespace().count())
        .sum();
    words > WORDS_NEEDING_HEADINGS
}

/// the start of a text, for messages.
pub(crate) fn short(text: &str) -> String {
    let mut shortened: String = text.chars().take(60).collect();
    if text.chars().count() > 60 {
        shortened.push('…');
    }
    shortened
}
