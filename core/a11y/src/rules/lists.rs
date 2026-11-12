//! lists: tags in the shape L › LI › Lbl/LBody, and paragraphs that look
//! like a list but aren't tagged as one.

use crate::rules::{Collector, Context};

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    let (true, Some(tree)) = (ctx.tagged, &ctx.tree) else {
        super::skip_untagged(out, &["list-structure-invalid", "list-not-tagged-as-list"]);
        return;
    };
    let mut reported = 0;
    for (index, element) in tree.elements.iter().enumerate() {
        let parent_role = element.parent.map(|p| tree.elements[p].role.as_str());
        let problem = match element.role.as_str() {
            "LI" if parent_role != Some("L") => Some("A list item (LI) isn’t inside a list (L)."),
            "Lbl" | "LBody" if parent_role != Some("LI") => {
                // a label is also allowed in a few other places, like a note
                // or a bibliography entry
                (!matches!(
                    parent_role,
                    Some("Note" | "BibEntry" | "TOCI" | "Reference")
                ) || element.role == "LBody")
                    .then_some("A list label or body isn’t inside a list item (LI).")
            }
            "L" => tree
                .children(index)
                .any(|c| !matches!(tree.elements[c].role.as_str(), "LI" | "L" | "Caption"))
                .then_some("A list (L) holds something other than list items."),
            "LI" => tree
                .children(index)
                .any(|c| !matches!(tree.elements[c].role.as_str(), "Lbl" | "LBody"))
                .then_some("A list item holds something other than a label and a body."),
            _ => None,
        };
        if let Some(problem) = problem {
            reported += 1;
            if reported <= 50 {
                out.report(
                    "list-structure-invalid",
                    problem,
                    ctx.element_location(index),
                    None,
                );
            }
        }
    }
    check_lists_in_disguise(ctx, out);
}

/// two or more paragraphs in a row that start like list items.
fn check_lists_in_disguise(ctx: &Context, out: &mut Collector) {
    let Some(tree) = &ctx.tree else {
        return;
    };
    let mut parents: Vec<Option<usize>> = vec![None];
    parents.extend((0..tree.elements.len()).map(Some));
    for parent in parents {
        // paragraphs already inside a list are the shape problem above
        if parent.is_some_and(|p| matches!(tree.elements[p].role.as_str(), "L" | "LI" | "LBody")) {
            continue;
        }
        let siblings: Vec<usize> = match parent {
            Some(p) => tree.children(p).collect(),
            None => tree.top.clone(),
        };
        let mut run: Vec<usize> = Vec::new();
        let flush = |run: &mut Vec<usize>, out: &mut Collector| {
            if run.len() >= 2 {
                out.report(
                    "list-not-tagged-as-list",
                    format!(
                        "{} paragraphs in a row start with a bullet or number.",
                        run.len()
                    ),
                    ctx.element_location(run[0]),
                    None,
                );
            }
            run.clear();
        };
        for sibling in siblings {
            let is_item = tree.elements[sibling].role == "P"
                && starts_like_list_item(&ctx.element_text(sibling));
            if is_item {
                run.push(sibling);
            } else {
                flush(&mut run, out);
            }
        }
        flush(&mut run, out);
    }
}

/// the label at the start of a list item: a bullet, or a number or
/// letter followed by "." or ")". returns its length in characters.
pub(crate) fn list_label(text: &str) -> Option<usize> {
    let text = text.trim_start();
    let mut chars = text.chars();
    let first = chars.next()?;
    if matches!(
        first,
        '•' | '◦'
            | '▪'
            | '▫'
            | '■'
            | '□'
            | '●'
            | '○'
            | '–'
            | '—'
            | '-'
            | '*'
            | '·'
            | '►'
            | '✓'
            | '\u{f0b7}'
    ) {
        let rest = chars.as_str();
        return (rest.starts_with(' ') || rest.starts_with('\t') || first != '-').then_some(1);
    }
    let label: String = text
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '(')
        .collect();
    let after = text[label.len()..].chars().next()?;
    let inner = label.trim_start_matches('(');
    let numbered =
        !inner.is_empty() && inner.len() <= 3 && inner.chars().all(|c| c.is_ascii_digit());
    let lettered = inner.len() == 1 && inner.chars().all(|c| c.is_ascii_lowercase());
    let roman = !inner.is_empty()
        && inner.len() <= 4
        && inner.chars().all(|c| matches!(c, 'i' | 'v' | 'x'));
    if (numbered || lettered || roman) && matches!(after, '.' | ')') {
        let next = text[label.len() + 1..].chars().next();
        if next.is_none_or(char::is_whitespace) {
            return Some(label.chars().count() + 1);
        }
    }
    None
}

pub(crate) fn starts_like_list_item(text: &str) -> bool {
    list_label(text).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bullets_and_numbers_are_list_labels() {
        assert_eq!(list_label("• Apples"), Some(1));
        assert_eq!(list_label("1. First"), Some(2));
        assert_eq!(list_label("12) Twelfth"), Some(3));
        assert_eq!(list_label("a) Option"), Some(2));
        assert_eq!(list_label("iv. Fourth"), Some(3));
    }

    #[test]
    fn ordinary_sentences_are_not() {
        assert_eq!(list_label("The year 2024. It rained."), None);
        assert_eq!(list_label("3.14 is pi"), None);
        assert_eq!(list_label("-5 degrees"), None);
        assert_eq!(list_label("Apples"), None);
    }
}
