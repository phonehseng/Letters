//! Find and Replace across a deck: every place some words appear, in
//! slide text, table cells and speaker notes, in slide order; and the one
//! operation that replaces a single place (Replace All is the engine's own
//! `FindReplace`).

use letters_slides::edit::Operation;
use letters_slides::model::{GraphicKind, Object, Presentation, Slide};
use serde::{Deserialize, Serialize};

/// where some words were found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Place {
    Text {
        object: u32,
        paragraph: usize,
    },
    Cell {
        object: u32,
        row: usize,
        column: usize,
    },
    Notes,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub slide_id: String,
    pub slide_number: usize,
    pub place: Place,
    /// which match within that place, counting from 0
    pub occurrence: usize,
    /// the words around the match, for the list
    pub snippet: String,
}

/// how many characters of context a snippet shows on each side
const SNIPPET_CONTEXT: usize = 24;

/// every match in the deck, slide by slide.
pub fn find(presentation: &Presentation, words: &str, match_case: bool) -> Vec<Found> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    for (index, slide) in presentation.slides.iter().enumerate() {
        for (place, text) in places(slide) {
            for (occurrence, (start, end)) in
                matches(&text, words, match_case).into_iter().enumerate()
            {
                found.push(Found {
                    slide_id: slide.id.clone(),
                    slide_number: index + 1,
                    place: place.clone(),
                    occurrence,
                    snippet: snippet(&text, start, end),
                });
            }
        }
    }
    found
}

/// every searchable text of a slide with where it is.
fn places(slide: &Slide) -> Vec<(Place, String)> {
    let mut places = Vec::new();
    for object in &slide.objects {
        if let Some(body) = object.text() {
            for (paragraph, text) in body.paragraphs.iter().enumerate() {
                places.push((
                    Place::Text {
                        object: object.id(),
                        paragraph,
                    },
                    text.plain_text(),
                ));
            }
        }
        if let Object::Frame(frame) = object {
            if let GraphicKind::Table(table) = &frame.kind {
                for (row, cells) in table.rows.iter().enumerate() {
                    for (column, cell) in cells.cells.iter().enumerate() {
                        places.push((
                            Place::Cell {
                                object: frame.id,
                                row,
                                column,
                            },
                            cell.text.plain_text(),
                        ));
                    }
                }
            }
        }
    }
    if let Some(notes) = &slide.notes {
        places.push((Place::Notes, notes.plain_text()));
    }
    places
}

/// byte ranges of each match, left to right, not overlapping.
pub fn matches(text: &str, words: &str, match_case: bool) -> Vec<(usize, usize)> {
    if words.is_empty() {
        return Vec::new();
    }
    let wanted: Vec<char> = words.chars().map(|c| fold(c, match_case)).collect();
    let characters: Vec<(usize, char)> = text.char_indices().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index + wanted.len() <= characters.len() {
        let window = &characters[index..index + wanted.len()];
        let same = window
            .iter()
            .zip(&wanted)
            .all(|((_, c), w)| fold(*c, match_case) == *w);
        if same {
            let start = window[0].0;
            let end = characters
                .get(index + wanted.len())
                .map_or(text.len(), |(byte, _)| *byte);
            found.push((start, end));
            index += wanted.len();
        } else {
            index += 1;
        }
    }
    found
}

/// a character as compared: as it is, or its simple lower case.
fn fold(character: char, match_case: bool) -> char {
    if match_case {
        return character;
    }
    // one-to-one lower case keeps the character count, so ranges stay right
    let mut lower = character.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => character,
    }
}

fn snippet(text: &str, start: usize, end: usize) -> String {
    let before: String = text[..start]
        .chars()
        .rev()
        .take(SNIPPET_CONTEXT)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let after: String = text[end..].chars().take(SNIPPET_CONTEXT).collect();
    let lead = if before.len() < start { "…" } else { "" };
    let tail = if after.len() < text.len() - end {
        "…"
    } else {
        ""
    };
    format!("{lead}{before}{}{after}{tail}", &text[start..end]).replace('\n', " ")
}

/// the operation that replaces one match with `replacement`.
pub fn replace_one(
    presentation: &Presentation,
    found: &Found,
    words: &str,
    replacement: &str,
    match_case: bool,
) -> Result<Operation, String> {
    let slide = presentation
        .slides
        .iter()
        .find(|slide| slide.id == found.slide_id)
        .ok_or("That slide is no longer in the presentation.")?;
    let replace_in = |text: &str| -> Result<String, String> {
        let (start, end) = *matches(text, words, match_case)
            .get(found.occurrence)
            .ok_or("Those words are no longer there. Search again.")?;
        Ok(format!("{}{replacement}{}", &text[..start], &text[end..]))
    };
    let slide_id = slide.id.clone();
    match &found.place {
        Place::Text { object, paragraph } => {
            let body = slide
                .objects
                .iter()
                .find(|o| o.id() == *object)
                .and_then(Object::text)
                .ok_or("That text is no longer on the slide.")?;
            let mut paragraphs: Vec<String> =
                body.paragraphs.iter().map(|p| p.plain_text()).collect();
            let target = paragraphs
                .get_mut(*paragraph)
                .ok_or("That paragraph is no longer there.")?;
            *target = replace_in(target)?;
            Ok(Operation::SetText {
                slide_id,
                object_id: *object,
                paragraphs,
            })
        }
        Place::Cell {
            object,
            row,
            column,
        } => {
            let text = places(slide)
                .into_iter()
                .find(|(place, _)| place == &found.place)
                .map(|(_, text)| text)
                .ok_or("That table cell is no longer there.")?;
            Ok(Operation::SetCellText {
                slide_id,
                object_id: *object,
                row: *row,
                column: *column,
                text: replace_in(&text)?,
            })
        }
        Place::Notes => {
            let notes = slide
                .notes
                .as_ref()
                .map(|n| n.plain_text())
                .unwrap_or_default();
            Ok(Operation::SetNotes {
                slide_id,
                text: replace_in(&notes)?,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::deck::SlideDeck;
    use crate::slides::test_decks::deck_path;

    #[test]
    fn matching_ignores_case_unless_asked() {
        assert_eq!(
            matches("Cats and cats", "cats", false),
            vec![(0, 4), (9, 13)]
        );
        assert_eq!(matches("Cats and cats", "cats", true), vec![(9, 13)]);
        assert_eq!(
            matches("ééé", "éé", false).len(),
            1,
            "matches don't overlap"
        );
        assert!(matches("abc", "", false).is_empty());
    }

    #[test]
    fn finds_words_on_every_slide_in_order() {
        let deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let first_title = crate::slides::view::deck_view(&deck.presentation).slides[0]
            .title
            .clone();
        let word = first_title
            .split_whitespace()
            .next()
            .unwrap()
            .to_lowercase();
        let found = find(&deck.presentation, &word, false);
        assert!(!found.is_empty());
        assert_eq!(found[0].slide_number, 1);
        assert!(found[0].snippet.to_lowercase().contains(&word));
    }

    #[test]
    fn replacing_one_match_changes_only_that_one() {
        let mut deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let title = crate::slides::view::deck_view(&deck.presentation).slides[0]
            .title
            .clone();
        let word = title.split_whitespace().next().unwrap().to_string();
        let found = find(&deck.presentation, &word, true);
        let operation = replace_one(&deck.presentation, &found[0], &word, "Changed", true).unwrap();
        deck.apply(&operation).unwrap();
        let now = crate::slides::view::deck_view(&deck.presentation).slides[0]
            .title
            .clone();
        assert!(now.starts_with("Changed"), "{now}");
        assert_eq!(find(&deck.presentation, &word, true).len(), found.len() - 1);
    }

    #[test]
    fn snippets_show_the_words_around_a_match() {
        let text = "The quick brown fox jumps over the lazy dog near the riverbank today";
        let (start, end) = matches(text, "lazy", false)[0];
        let shown = snippet(text, start, end);
        assert!(shown.contains("lazy") && shown.starts_with('…') && shown.ends_with('…'));
    }
}
