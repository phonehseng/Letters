//! the outline view (each slide's title and bullet text), the outline as
//! plain text, and a new deck made from an outline (headings become
//! slides), which is how a document becomes a deck: the caller turns the
//! document's headings and paragraphs into `OutlineEntry`s.

use serde::{Deserialize, Serialize};

use crate::blank::{blank_presentation, new_slide, CONTENT_LAYOUT, TITLE_LAYOUT};
use crate::model::{Object, PlaceholderKind, Presentation, TextBody, TextParagraph};

/// one slide in the outline view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineSlide {
    pub slide_id: String,
    pub title: String,
    /// (level from 0, text) of every body paragraph
    pub items: Vec<(u8, String)>,
}

/// one line of an outline to build a deck from: level 0 is a slide
/// title, deeper levels are its bullets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineEntry {
    pub level: u8,
    pub text: String,
}

pub fn outline(presentation: &Presentation) -> Vec<OutlineSlide> {
    presentation
        .slides
        .iter()
        .map(|slide| {
            let mut title = String::new();
            let mut items = Vec::new();
            for object in &slide.objects {
                let (Some(placeholder), Some(text)) = (object.placeholder(), object.text()) else {
                    continue;
                };
                if placeholder.kind.is_title() {
                    title = text.plain_text().replace('\u{b}', " ");
                } else if !placeholder.kind.is_furniture() {
                    items.extend(
                        text.paragraphs
                            .iter()
                            .map(|p| (p.level(), p.plain_text()))
                            .filter(|(_, t)| !t.is_empty()),
                    );
                }
            }
            OutlineSlide {
                slide_id: slide.id.clone(),
                title,
                items,
            }
        })
        .collect()
}

/// the outline as text: a numbered title line per slide, its items
/// indented by a tab per level.
pub fn outline_text(presentation: &Presentation) -> String {
    let mut out = String::new();
    for (index, slide) in outline(presentation).iter().enumerate() {
        out.push_str(&format!("{}. {}\n", index + 1, slide.title));
        for (level, text) in &slide.items {
            out.push_str(&"\t".repeat(usize::from(*level) + 1));
            out.push_str(text);
            out.push('\n');
        }
    }
    out
}

/// a new deck with a slide per level-0 entry. the first slide uses the
/// title layout when it has no items (a document's title).
pub fn from_outline(entries: &[OutlineEntry]) -> Presentation {
    let mut deck = blank_presentation();
    let mut groups: Vec<(String, Vec<(u8, String)>)> = Vec::new();
    for entry in entries {
        match (entry.level, groups.last_mut()) {
            (0, _) | (_, None) => groups.push((entry.text.clone(), Vec::new())),
            (level, Some(group)) => group.1.push((level - 1, entry.text.clone())),
        }
    }
    for (index, (title, items)) in groups.into_iter().enumerate() {
        let layout = if index == 0 && items.is_empty() {
            TITLE_LAYOUT
        } else {
            CONTENT_LAYOUT
        };
        let mut slide = new_slide(&deck, &format!("slide-{}", index + 1), layout);
        fill_placeholders(&mut slide.objects, &title, &items);
        deck.slides.push(slide);
    }
    deck
}

fn fill_placeholders(objects: &mut [Object], title: &str, items: &[(u8, String)]) {
    for object in objects {
        let Some(kind) = object.placeholder().map(|p| p.kind) else {
            continue;
        };
        let Some(text) = object.text_mut() else {
            continue;
        };
        if kind.is_title() {
            *text = TextBody::from_text(title);
        } else if matches!(kind, PlaceholderKind::Body | PlaceholderKind::Object) {
            text.paragraphs = items
                .iter()
                .map(|(level, line)| {
                    let mut paragraph = TextParagraph::with_text(line);
                    paragraph.props.level = (*level > 0).then_some((*level).min(8));
                    paragraph
                })
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: u8, text: &str) -> OutlineEntry {
        OutlineEntry {
            level,
            text: text.into(),
        }
    }

    #[test]
    fn headings_become_slides_and_paragraphs_bullets() {
        let deck = from_outline(&[
            entry(0, "Cells"),
            entry(0, "Parts of a cell"),
            entry(1, "Nucleus"),
            entry(2, "Holds the DNA"),
            entry(1, "Membrane"),
        ]);
        assert_eq!(deck.slides.len(), 2);
        let view = outline(&deck);
        assert_eq!(view[0].title, "Cells");
        assert_eq!(view[1].title, "Parts of a cell");
        assert_eq!(
            view[1].items,
            vec![
                (0, "Nucleus".into()),
                (1, "Holds the DNA".into()),
                (0, "Membrane".into())
            ]
        );
        assert_eq!(
            outline_text(&deck),
            "1. Cells\n2. Parts of a cell\n\tNucleus\n\t\tHolds the DNA\n\tMembrane\n"
        );
    }
}
