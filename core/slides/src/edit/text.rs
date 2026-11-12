//! text edits: an object's text, a slide's notes, and find and replace
//! across the deck (slides, tables and notes).

use super::{object_index, slide_mut, snapshot, Applied, Operation};
use crate::error::SlidesError;
use crate::model::{Notes, Object, Presentation, TextBody, TextParagraph, TextRun};

pub fn apply_text(
    presentation: &mut Presentation,
    operation: &Operation,
) -> Result<Applied, SlidesError> {
    match operation {
        Operation::SetText {
            slide_id,
            object_id,
            paragraphs,
        } => {
            let inverse = snapshot(presentation, slide_id)?;
            let slide = slide_mut(presentation, slide_id)?;
            let index = object_index(slide, *object_id)?;
            let Object::Shape(shape) = &mut slide.objects[index] else {
                return Err(SlidesError::InvalidEdit("only shapes hold text".into()));
            };
            let body = shape.text.get_or_insert_with(TextBody::default);
            set_paragraphs(body, paragraphs);
            Ok(Applied::slide(slide_id, inverse))
        }
        Operation::SetNotes { slide_id, text } => {
            let inverse = snapshot(presentation, slide_id)?;
            let slide = slide_mut(presentation, slide_id)?;
            let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
            let notes = slide.notes.get_or_insert_with(Notes::default);
            set_paragraphs(&mut notes.text, &lines);
            Ok(Applied::slide(slide_id, inverse))
        }
        Operation::FindReplace {
            find,
            replace,
            match_case,
        } => {
            let inverse = Operation::RestoreSlides {
                slides: presentation.slides.clone(),
                sections: presentation.sections.clone(),
            };
            let changed = find_replace(presentation, find, replace, *match_case);
            Ok(Applied {
                changed_slides: changed,
                all_slides: false,
                inverse,
            })
        }
        _ => Err(SlidesError::InvalidEdit("not a text edit".into())),
    }
}

/// new paragraph texts; each keeps the formatting of the paragraph that
/// was in its place (or of the last one), and an unchanged paragraph is
/// left alone so its xml stays as it was.
pub fn set_paragraphs(body: &mut TextBody, texts: &[String]) {
    let template = body.paragraphs.last().cloned().unwrap_or_default();
    let mut paragraphs = Vec::with_capacity(texts.len());
    for (index, text) in texts.iter().enumerate() {
        let existing = body.paragraphs.get(index).cloned();
        if let Some(existing) = &existing {
            if existing.plain_text() == *text {
                paragraphs.push(existing.clone());
                continue;
            }
        }
        paragraphs.push(retyped(existing.unwrap_or_else(|| template.clone()), text));
    }
    body.paragraphs = paragraphs;
}

/// a paragraph with new text in its first run's formatting.
fn retyped(mut paragraph: TextParagraph, text: &str) -> TextParagraph {
    let props = paragraph
        .runs
        .first()
        .map(|run| run.props().clone())
        .unwrap_or_default();
    paragraph.runs = if text.is_empty() {
        Vec::new()
    } else {
        vec![TextRun::Text {
            text: text.to_string(),
            props,
        }]
    };
    paragraph.source = None;
    paragraph
}

/// replaces text inside runs (a match may not span two runs, as in
/// powerpoint's own replace) and returns the slides that changed.
fn find_replace(
    presentation: &mut Presentation,
    find: &str,
    replace: &str,
    match_case: bool,
) -> Vec<String> {
    if find.is_empty() {
        return Vec::new();
    }
    let mut changed = Vec::new();
    for slide in &mut presentation.slides {
        let mut any = false;
        for object in &mut slide.objects {
            any |= replace_in_object(object, find, replace, match_case);
        }
        if let Some(notes) = &mut slide.notes {
            any |= replace_in_body(&mut notes.text, find, replace, match_case);
        }
        if any {
            changed.push(slide.id.clone());
        }
    }
    changed
}

fn replace_in_object(object: &mut Object, find: &str, replace: &str, match_case: bool) -> bool {
    match object {
        Object::Shape(shape) => shape
            .text
            .as_mut()
            .is_some_and(|body| replace_in_body(body, find, replace, match_case)),
        Object::Group(group) => {
            let mut any = false;
            for child in &mut group.objects {
                any |= replace_in_object(child, find, replace, match_case);
            }
            any
        }
        Object::Frame(frame) => match &mut frame.kind {
            crate::model::GraphicKind::Table(table) => {
                let mut any = false;
                for cell in table.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                    any |= replace_in_body(&mut cell.text, find, replace, match_case);
                }
                any
            }
            _ => false,
        },
        _ => false,
    }
}

fn replace_in_body(body: &mut TextBody, find: &str, replace: &str, match_case: bool) -> bool {
    let mut any = false;
    for run in body.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
        if let TextRun::Text { text, .. } = run {
            if let Some(new) = replaced(text, find, replace, match_case) {
                *text = new;
                any = true;
            }
        }
    }
    any
}

/// `text` with every match replaced, or `None` when there is none.
fn replaced(text: &str, find: &str, replace: &str, match_case: bool) -> Option<String> {
    if match_case {
        return text.contains(find).then(|| text.replace(find, replace));
    }
    let lower = text.to_lowercase();
    let needle = find.to_lowercase();
    if !lower.contains(&needle) || lower.len() != text.len() {
        // lower-casing changed byte lengths (rare scripts): match exactly
        return text.contains(find).then(|| text.replace(find, replace));
    }
    let mut out = String::new();
    let mut rest = 0;
    while let Some(found) = lower[rest..].find(&needle) {
        out.push_str(&text[rest..rest + found]);
        out.push_str(replace);
        rest += found + needle.len();
    }
    out.push_str(&text[rest..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_ignores_case_when_asked() {
        assert_eq!(
            replaced("Cell cell CELL", "cell", "unit", false).as_deref(),
            Some("unit unit unit")
        );
        assert_eq!(
            replaced("Cell cell", "cell", "unit", true).as_deref(),
            Some("Cell unit")
        );
        assert_eq!(replaced("nothing", "cell", "unit", false), None);
    }

    #[test]
    fn retyping_keeps_untouched_paragraphs() {
        let mut body = TextBody::from_text("first\nsecond");
        let kept = body.paragraphs[0].clone();
        set_paragraphs(
            &mut body,
            &[
                "first".to_string(),
                "changed".to_string(),
                "third".to_string(),
            ],
        );
        assert_eq!(body.paragraphs[0], kept);
        assert_eq!(body.plain_text(), "first\nchanged\nthird");
    }
}
