//! the core line edit: the chosen characters of a line change (replaced
//! or restyled) and the rest of the line keeps its glyphs and spacing,
//! moved along by however much wider or narrower the change is.

use super::*;

/// what an edit does to the chosen characters of a line.
#[derive(Clone, Copy)]
pub(super) enum Change<'a> {
    /// replace them with this text
    Text(&'a str),
    /// keep them, with a new look
    Style(&'a TextStyleChange),
}

pub(crate) fn glyphs_of(chars: &[LineChar]) -> impl Iterator<Item = usize> + '_ {
    chars.iter().filter_map(|char| match char {
        LineChar::Glyph(glyph) => Some(*glyph),
        LineChar::Gap => None,
    })
}

pub(super) fn text_length(model: &PageModel, chars: &[LineChar]) -> usize {
    chars
        .iter()
        .map(|char| model.char_text(char).chars().count())
        .sum()
}

/// turns a range of characters of the line's text into a range of the
/// line's glyphs and gaps. a ligature (one glyph, several characters)
/// is taken whole when the range touches it.
pub(super) fn char_range(
    model: &PageModel,
    line: &LineModel,
    range: Range<usize>,
) -> Result<Range<usize>, EditError> {
    let total = text_length(model, &line.chars);
    if range.start > range.end || range.end > total {
        return Err(EditError::OutOfRange);
    }
    let mut position = 0;
    let (mut start, mut end) = (line.chars.len(), line.chars.len());
    for (index, char) in line.chars.iter().enumerate() {
        let length = model.char_text(char).chars().count();
        if start == line.chars.len() && range.start < position + length.max(1) {
            start = index;
        }
        if range.end <= position {
            end = index;
            break;
        }
        position += length;
    }
    if range.start == range.end {
        end = start;
    }
    Ok(start..end.max(start))
}

/// the text matrix of a glyph taken all the way to the page.
pub(crate) fn to_page(model: &PageModel, glyph: usize) -> Matrix {
    let record = &model.run.glyphs[glyph];
    record.text_matrix.then(&model.run.shows[record.show].ctm)
}

/// where a glyph starts, measured in another glyph's text space.
pub(super) fn relative_x(model: &PageModel, glyph: usize, anchor: &Matrix) -> f32 {
    let inverse = anchor.invert().unwrap_or(Matrix::IDENTITY);
    to_page(model, glyph).then(&inverse).e
}

pub(super) fn font_ref(model: &PageModel, glyph: usize) -> Result<FontRef, EditError> {
    let record = &model.run.glyphs[glyph];
    let show = &model.run.shows[record.show];
    match record.font.and_then(|font| model.run.fonts[font].object) {
        Some(object) => Ok(FontRef::Object(object)),
        None if !show.before.font_name.is_empty() => Ok(FontRef::Named {
            source: show.source,
            name: show.before.font_name.clone(),
        }),
        None => Err(EditError::Damaged("the text has no font".into())),
    }
}

/// the anchor of an edit starting at line position `from`: the glyph to
/// write after, and the text-space position the edit starts at.
pub(super) fn anchor_for(model: &PageModel, line: &LineModel, from: usize) -> (usize, f32) {
    match line.chars.get(from) {
        Some(LineChar::Glyph(glyph)) => (*glyph, 0.0),
        _ => {
            // a gap or the end of the line: start where the glyph before ends
            let previous = glyphs_of(&line.chars[..from])
                .last()
                .expect("lines start with a glyph");
            (previous, model.run.glyphs[previous].advance)
        }
    }
}

/// picks the fonts to write `text` in, starting with the font of `like`.
pub(crate) fn choose_fonts(
    model: &PageModel,
    draft: &mut Draft,
    like: usize,
    text: &str,
    style: Option<&TextStyleChange>,
    options: &TextEditOptions,
) -> Result<FontChoice, EditError> {
    if let Some(standard) = style.and_then(|style| style.font) {
        let missing = embed::not_in_standard_fonts(text);
        if !missing.is_empty() {
            return Err(EditError::UnsupportedCharacters(
                missing.into_iter().collect(),
            ));
        }
        let font = embed::standard_font(draft, standard);
        return Ok(FontChoice {
            by_char: text.chars().map(|c| (c, 0)).collect(),
            fonts: vec![font],
            notices: Vec::new(),
            to_unicode_additions: Default::default(),
        });
    }
    let record = &model.run.glyphs[like];
    let Some(font) = record.font.map(|font| &model.run.fonts[font]) else {
        return Err(EditError::Damaged("the text has no font".into()));
    };
    let choice = text_fonts::choose(draft, font, text, rules(options))?;
    if !choice.to_unicode_additions.is_empty() {
        if let Some(object) = font.object {
            embed::extend_to_unicode(draft, object, &choice.to_unicode_additions);
        }
    }
    Ok(choice)
}

/// the placed glyphs of new text, in the given fonts, from position `x`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn place_text(
    model: &PageModel,
    anchor_show: usize,
    text: &str,
    choice: &FontChoice,
    size: f32,
    rise: f32,
    color: Color,
    start_x: f32,
    line: usize,
) -> (Vec<Placed>, f32) {
    let state = &model.run.shows[anchor_show].before;
    let mut placed = Vec::new();
    let mut x = start_x;
    for character in text.chars() {
        let Some(font) = choice
            .by_char
            .get(&character)
            .map(|index| &choice.fonts[*index])
        else {
            // a space no font here can draw: a plain move a quarter wide
            x += advance(state, 250.0, size, false);
            continue;
        };
        let code = font
            .codes
            .get(&character)
            .expect("chosen fonts draw their characters");
        placed.push(Placed {
            x,
            line,
            font: FontRef::Object(font.object),
            size,
            rise,
            color,
            code: Some(code.code.clone()),
            width: code.width,
            is_word_space: code.is_word_space,
        });
        x += advance(state, code.width, size, code.is_word_space);
    }
    (placed, x)
}

/// the size, rise and colour new text takes from the glyph it continues.
pub(crate) fn look_of(model: &PageModel, glyph: usize) -> (f32, f32, Color) {
    let record = &model.run.glyphs[glyph];
    let state = &model.run.shows[record.show].before;
    (state.size, state.rise, record.color)
}

/// the core line edit: the line's characters `range` change (replaced
/// or restyled) and everything after them on the line is written again,
/// unchanged but moved by however much wider or narrower the change is.
#[allow(clippy::too_many_arguments)]
pub(super) fn edit_line(
    model: &PageModel,
    draft: &mut Draft,
    plan: &mut Plan,
    block: usize,
    line_index: usize,
    range: Range<usize>,
    change: Change,
    options: &TextEditOptions,
) -> Result<Outcome, EditError> {
    let line = &model.blocks[block].lines[line_index];
    let (anchor_glyph, start_x) = anchor_for(model, line, range.start);
    let anchor_show = model.run.glyphs[anchor_glyph].show;
    let anchor_matrix = model.run.glyphs[anchor_glyph].text_matrix;
    let anchor_page = to_page(model, anchor_glyph);
    let (size, rise, color) = look_of(model, anchor_glyph);

    let mut placed = Vec::new();
    let mut notices = Vec::new();
    let mut x = start_x;
    match change {
        Change::Text(text) if !text.is_empty() => {
            let choice = choose_fonts(model, draft, anchor_glyph, text, None, options)?;
            let (glyphs, end) =
                place_text(model, anchor_show, text, &choice, size, rise, color, x, 0);
            placed.extend(glyphs);
            notices = choice.notices;
            x = end;
        }
        Change::Text(_) => {}
        Change::Style(style) => {
            let restyled = restyle(
                model,
                draft,
                line,
                range.clone(),
                style,
                &anchor_page,
                x,
                options,
            )?;
            placed.extend(restyled.0);
            notices = restyled.1;
            x = restyled.2;
        }
    }

    // the rest of the line keeps its glyphs and spacing, moved along
    let tail = &line.chars[range.end..];
    if glyphs_of(tail).next().is_some() {
        let tail_start = match tail[0] {
            LineChar::Glyph(glyph) => relative_x(model, glyph, &anchor_page),
            LineChar::Gap => {
                let previous = glyphs_of(&line.chars[..range.end])
                    .last()
                    .expect("lines start with a glyph");
                relative_x(model, previous, &anchor_page) + model.run.glyphs[previous].advance
            }
        };
        let shift = x - tail_start;
        for glyph in glyphs_of(tail) {
            let record = &model.run.glyphs[glyph];
            let (size, rise, _) = look_of(model, glyph);
            placed.push(Placed {
                x: relative_x(model, glyph, &anchor_page) + shift,
                line: 0,
                font: font_ref(model, glyph)?,
                size,
                rise,
                color: record.color,
                code: Some(record.code.clone()),
                width: record.width,
                is_word_space: record.code == [32],
            });
        }
    }

    let removed: HashSet<usize> = glyphs_of(&line.chars[range.start..]).collect();
    text_write::blank(&model.run, plan, &removed);
    let anchor = Anchor {
        show: anchor_show,
        text_matrix: anchor_matrix,
        line_step: 0.0,
    };
    if !placed.is_empty() {
        text_write::write(draft.doc, &model.run, plan, &anchor, &placed)?;
    }
    Ok(Outcome {
        notices,
        overflow: None,
    })
}

/// the restyled glyphs of a range, and where the pen ends.
#[allow(clippy::too_many_arguments)]
pub(super) fn restyle(
    model: &PageModel,
    draft: &mut Draft,
    line: &LineModel,
    range: Range<usize>,
    style: &TextStyleChange,
    anchor_page: &Matrix,
    start_x: f32,
    options: &TextEditOptions,
) -> Result<(Vec<Placed>, Vec<FontNotice>, f32), EditError> {
    let chars = &line.chars[range.clone()];
    let Some(first) = glyphs_of(chars).next() else {
        return Ok((Vec::new(), Vec::new(), start_x));
    };
    let anchor_show = model.run.glyphs[first].show;
    let (original_size, rise, original_color) = look_of(model, first);
    let visible_size = model.run.glyphs[first].size.max(1e-6);
    let size = style.size.map_or(original_size, |wanted| {
        original_size * wanted / visible_size
    });
    let color = style.color.unwrap_or(original_color);
    if style.font.is_some() {
        let text: String = chars.iter().map(|char| model.char_text(char)).collect();
        let choice = choose_fonts(model, draft, first, &text, Some(style), options)?;
        let (placed, end) = place_text(
            model,
            anchor_show,
            &text,
            &choice,
            size,
            rise,
            color,
            start_x,
            0,
        );
        return Ok((placed, choice.notices, end));
    }
    // same fonts: each glyph keeps its code; positions follow the new size
    let state = &model.run.shows[anchor_show].before;
    let mut placed = Vec::new();
    let mut x = start_x;
    let mut previous_end: Option<f32> = None;
    for char in chars {
        match char {
            LineChar::Glyph(glyph) => {
                let record = &model.run.glyphs[*glyph];
                let glyph_size = look_of(model, *glyph).0 * size / original_size.max(1e-6);
                placed.push(Placed {
                    x,
                    line: 0,
                    font: font_ref(model, *glyph)?,
                    size: glyph_size,
                    rise,
                    color: style.color.unwrap_or(record.color),
                    code: Some(record.code.clone()),
                    width: record.width,
                    is_word_space: record.code == [32],
                });
                x += advance(state, record.width, glyph_size, record.code == [32]);
                previous_end = Some(relative_x(model, *glyph, anchor_page) + record.advance);
            }
            LineChar::Gap => {
                // a gap keeps its width, scaled with the text
                let next = glyphs_of(&line.chars[range.start..]).find(|glyph| {
                    previous_end.is_some_and(|end| relative_x(model, *glyph, anchor_page) > end)
                });
                let gap = match (previous_end, next) {
                    (Some(end), Some(next)) => relative_x(model, next, anchor_page) - end,
                    _ => advance(state, 250.0, original_size, false),
                };
                x += gap * size / original_size.max(1e-6);
            }
        }
    }
    Ok((placed, Vec::new(), x))
}
