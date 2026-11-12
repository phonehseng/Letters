//! pictures: every figure needs alt text that describes it, and every
//! picture on a page must be tagged or marked as decoration.

use std::collections::HashSet;

use crate::content::Mark;
use crate::objects::ObjectRef;
use crate::report::{ContentRef, FixKind, Location, Target};
use crate::rules::{Collector, Context};

/// types whose content is only understandable through a description.
const NEEDS_DESCRIPTION: &[&str] = &["Figure", "Formula"];

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    if !ctx.tagged {
        super::skip_untagged(
            out,
            &[
                "missing-alt-text",
                "alt-text-is-file-name",
                "image-not-tagged",
            ],
        );
        return;
    }
    let Some(tree) = &ctx.tree else {
        return;
    };
    let mut reported = HashSet::new();
    for (index, element) in tree.elements.iter().enumerate() {
        if !NEEDS_DESCRIPTION.contains(&element.role.as_str()) {
            continue;
        }
        reported.insert(index);
        check_description(ctx, out, index);
    }
    // a picture tagged inside something else (a P, say) also needs a
    // description from some tag around it
    let owners = tree.content_owners();
    for (page, content) in ctx.contents() {
        let page_id = ctx.pages[page as usize].id;
        for image in content.images.iter().filter(|image| !image.in_form) {
            match image.mark {
                Mark::Mcid(mcid) => {
                    let Some(&owner) = owners.get(&(page_id, mcid)) else {
                        continue;
                    };
                    let described = tree
                        .ancestors(owner)
                        .into_iter()
                        .any(|i| tree.elements[i].description().is_some() || reported.contains(&i));
                    if !described && reported.insert(owner) {
                        check_description(ctx, out, owner);
                    }
                }
                Mark::Untagged => {
                    let target = ContentRef {
                        page,
                        op_index: image.op_index,
                    };
                    out.report(
                        "image-not-tagged",
                        format!("A picture on page {} has no tag.", page + 1),
                        Location::page(page, Some(image.rect)),
                        Some(FixKind::SetAltText {
                            target: Target::Image { image: target },
                        }),
                    );
                }
                Mark::Artifact => {}
            }
        }
    }
}

fn check_description(ctx: &Context, out: &mut Collector, index: usize) {
    let Some(tree) = &ctx.tree else {
        return;
    };
    let element = &tree.elements[index];
    let fix = element.id.map(|id| FixKind::SetAltText {
        target: Target::Element {
            element: ObjectRef::from(id),
        },
    });
    match element.description() {
        None => {
            out.report(
                "missing-alt-text",
                format!("This {} tag has no /Alt text.", element.raw_type),
                ctx.element_location(index),
                fix,
            );
        }
        Some(alt) if looks_like_placeholder(alt) => {
            out.report(
                "alt-text-is-file-name",
                format!("The alt text is “{alt}”."),
                ctx.element_location(index),
                fix,
            );
        }
        Some(_) => {}
    }
}

/// alt text that is a file name or a word that only says "this is a picture".
pub(crate) fn looks_like_placeholder(alt: &str) -> bool {
    let lower = alt.trim().to_lowercase();
    const EXTENSIONS: &[&str] = &[
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".tif", ".tiff", ".svg", ".webp", ".emf", ".wmf",
        ".eps", ".pdf", ".heic",
    ];
    if EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) && !lower.contains(' ') {
        return true;
    }
    const PLACEHOLDERS: &[&str] = &[
        "image",
        "picture",
        "photo",
        "graphic",
        "figure",
        "img",
        "chart",
        "diagram",
        "logo",
        "untitled",
        "alt",
        "alt text",
        "description",
        "placeholder",
    ];
    // "picture 3", "image1", "img_2041", "DSC0042"
    let without_number = lower
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == '_' || c == '-' || c == ' ')
        .trim();
    if PLACEHOLDERS.contains(&without_number) {
        return true;
    }
    ["dsc", "img_", "img-", "screenshot", "scan"]
        .iter()
        .any(|prefix| {
            lower.starts_with(prefix)
                && lower.chars().any(|c| c.is_ascii_digit())
                && !lower.contains(' ')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_and_placeholders_are_caught() {
        for alt in [
            "photo.jpg",
            "IMG_2041.JPG",
            "image",
            "Picture 3",
            "image1",
            "DSC0042",
            "Figure",
        ] {
            assert!(looks_like_placeholder(alt), "{alt}");
        }
    }

    #[test]
    fn real_descriptions_pass() {
        for alt in [
            "Bar chart of rainfall by month, wettest in November",
            "A cat asleep on a keyboard",
            "Logo of the university",
        ] {
            assert!(!looks_like_placeholder(alt), "{alt}");
        }
    }
}
