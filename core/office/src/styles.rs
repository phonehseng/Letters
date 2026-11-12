//! working out what formatting actually applies to a paragraph, by
//! following its style, the styles that one is based on, and the
//! document defaults.

use crate::model::{Document, Mark, Paragraph, ParagraphProps, RunProps, Style, StyleKind};

/// a style based on itself (directly or in a circle) is a damaged file;
/// following at most this many steps makes resolution always finish.
const MAX_CHAIN: usize = 32;

/// the style and every style it is based on, nearest first.
pub fn style_chain<'a>(document: &'a Document, id: &str) -> Vec<&'a Style> {
    let mut chain: Vec<&Style> = Vec::new();
    let mut next = Some(id.to_string());
    while let Some(id) = next {
        if chain.len() >= MAX_CHAIN || chain.iter().any(|style| style.id == id) {
            break;
        }
        let Some(style) = document.style(&id) else {
            break;
        };
        chain.push(style);
        next = style.based_on.clone();
    }
    chain
}

/// the style a paragraph without a style id uses (word's "Normal").
pub fn default_paragraph_style(document: &Document) -> Option<&Style> {
    document
        .styles
        .styles
        .iter()
        .find(|style| style.kind == StyleKind::Paragraph && style.is_default)
}

/// the paragraph's style id, or the default style's when it has none.
pub fn effective_style_id(document: &Document, paragraph: &Paragraph) -> Option<String> {
    paragraph
        .style_id
        .clone()
        .or_else(|| default_paragraph_style(document).map(|style| style.id.clone()))
}

/// the paragraph formatting that really applies: defaults, then each
/// style from the root of the chain down, then the list level's indents
/// when the paragraph is in a list, then the paragraph's own.
pub fn resolved_paragraph_props(document: &Document, paragraph: &Paragraph) -> ParagraphProps {
    let mut props = document.styles.defaults.paragraph.clone();
    if let Some(id) = effective_style_id(document, paragraph) {
        for style in style_chain(document, &id).iter().rev() {
            props.merge(&style.paragraph);
        }
    }
    let list = paragraph.props.list.clone().or_else(|| props.list.clone());
    if let Some(list) = list {
        if let Some(level) = document.numbering.level(&list.list_id, list.level) {
            let mut level_props = level.paragraph.clone();
            // the level's own list reference would be circular; drop it
            level_props.list = None;
            props.merge(&level_props);
        }
    }
    props.merge(&paragraph.props);
    props
}

/// the run formatting that really applies to text with `marks` inside a
/// paragraph with `paragraph_style`.
pub fn resolved_run_props(
    document: &Document,
    paragraph_style: Option<&str>,
    marks: &[Mark],
) -> RunProps {
    let mut props = document.styles.defaults.run.clone();
    let paragraph_style = paragraph_style
        .map(str::to_string)
        .or_else(|| default_paragraph_style(document).map(|style| style.id.clone()));
    if let Some(id) = paragraph_style {
        for style in style_chain(document, &id).iter().rev() {
            props.merge(&style.run);
        }
    }
    let direct = RunProps::from_marks(marks);
    if let Some(id) = &direct.style_id {
        for style in style_chain(document, id).iter().rev() {
            props.merge(&style.run);
        }
    }
    props.merge(&direct);
    props.style_id = direct.style_id;
    props
}

/// the heading level (1 to 6) a style gives, by its outline level or,
/// failing that, by being named "heading 1" to "heading 6".
pub fn style_heading_level(document: &Document, id: &str) -> Option<u8> {
    for style in style_chain(document, id) {
        if let Some(level) = style.paragraph.outline_level {
            return level_from_outline(level);
        }
        if let Some(level) = level_from_name(&style.name) {
            return Some(level);
        }
    }
    None
}

/// the heading level of a paragraph, or `None` for body text.
pub fn heading_level(document: &Document, paragraph: &Paragraph) -> Option<u8> {
    if let Some(level) = paragraph.props.outline_level {
        return level_from_outline(level);
    }
    let id = paragraph.style_id.as_deref()?;
    style_heading_level(document, id)
}

/// word stores outline levels from 0; 9 means body text.
fn level_from_outline(level: u8) -> Option<u8> {
    (level < 6).then_some(level + 1)
}

fn level_from_name(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    let number = lower.strip_prefix("heading")?.trim();
    match number.parse::<u8>() {
        Ok(level) if (1..=6).contains(&level) => Some(level),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Style;

    fn document_with(styles: Vec<Style>) -> Document {
        let mut document = Document::new();
        document.styles.styles.extend(styles);
        document
    }

    #[test]
    fn a_circle_of_based_on_styles_still_finishes() {
        let mut a = Style::paragraph("A", "A");
        a.based_on = Some("B".into());
        let mut b = Style::paragraph("B", "B");
        b.based_on = Some("A".into());
        let document = document_with(vec![a, b]);
        assert_eq!(style_chain(&document, "A").len(), 2);
        assert_eq!(style_heading_level(&document, "A"), None);
    }

    #[test]
    fn heading_levels_by_outline_or_name() {
        let mut outline = Style::paragraph("Big", "Big");
        outline.paragraph.outline_level = Some(1);
        let named = Style::paragraph("H3", "heading 3");
        let mut child = Style::paragraph("Child", "Child");
        child.based_on = Some("H3".into());
        let document = document_with(vec![outline, named, child]);
        assert_eq!(style_heading_level(&document, "Big"), Some(2));
        assert_eq!(style_heading_level(&document, "H3"), Some(3));
        assert_eq!(style_heading_level(&document, "Child"), Some(3));
        assert_eq!(style_heading_level(&document, "Normal"), None);
    }
}
