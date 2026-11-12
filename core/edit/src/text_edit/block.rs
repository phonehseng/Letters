//! re-setting a whole block's text over its lines: wrapped to its width
//! with its alignment and line spacing.

use super::*;

/// re-sets a block's text over its lines, wrapped to the block's width
/// (or line by line as given), keeping its first line's start, its line
/// spacing and its alignment.
pub(super) fn reset_block(
    model: &PageModel,
    draft: &mut Draft,
    plan: &mut Plan,
    block_index: usize,
    text: &str,
    options: &TextEditOptions,
) -> Result<Outcome, EditError> {
    let block = &model.blocks[block_index];
    let first_line = &block.lines[0];
    let anchor_glyph = PageModel::first_glyph(first_line);
    let anchor_show = model.run.glyphs[anchor_glyph].show;
    let anchor_matrix = model.run.glyphs[anchor_glyph].text_matrix;
    let anchor_page = to_page(model, anchor_glyph);
    let (size, rise, color) = look_of(model, anchor_glyph);

    // page units per text-space unit, along and across the line
    let along_scale = model
        .run
        .space
        .visible_vector(anchor_page.apply_vector(Point::new(1.0, 0.0)))
        .length()
        .max(1e-6);
    let across_scale = model
        .run
        .space
        .visible_vector(anchor_page.apply_vector(Point::new(0.0, 1.0)))
        .length()
        .max(1e-6);
    let leading = block.leading.unwrap_or(first_line.size * 1.2);
    let line_step = leading / across_scale;

    // the block's left and right edges, along the first line, in text space
    let along = |line: &LineModel| {
        line.start.minus(first_line.start).dot(first_line.direction) / along_scale
    };
    let left = block.lines.iter().map(along).fold(f32::MAX, f32::min);
    let right = block
        .lines
        .iter()
        .map(|line| along(line) + line.length / along_scale)
        .fold(f32::MIN, f32::max);
    let width = right - left;

    let choice = choose_fonts(model, draft, anchor_glyph, text, None, options)?;
    let state = &model.run.shows[anchor_show].before;
    let measure = |word: &str| -> f32 {
        word.chars()
            .map(|character| {
                match choice
                    .by_char
                    .get(&character)
                    .map(|index| &choice.fonts[*index])
                    .and_then(|font| font.codes.get(&character))
                {
                    Some(code) => advance(state, code.width, size, code.is_word_space),
                    None => advance(state, 250.0, size, false),
                }
            })
            .sum()
    };
    let lines = if options.reflow {
        wrap(text, width, &measure)
    } else {
        text.split('\n')
            .map(|line| (line.to_owned(), true))
            .collect()
    };

    let mut placed = Vec::new();
    for (number, (line_text, ends_paragraph)) in lines.iter().enumerate() {
        let natural = measure(line_text);
        let spaces = line_text.chars().filter(|c| *c == ' ').count();
        let (start, extra_per_space) = match block.alignment {
            TextAlignment::Left => (left, 0.0),
            TextAlignment::Right => (right - natural, 0.0),
            TextAlignment::Center => (left + (width - natural) / 2.0, 0.0),
            TextAlignment::Justified if !ends_paragraph && spaces > 0 => {
                (left, (width - natural).max(0.0) / spaces as f32)
            }
            TextAlignment::Justified => (left, 0.0),
        };
        let (mut glyphs, _) = place_text(
            model,
            anchor_show,
            line_text,
            &choice,
            size,
            rise,
            color,
            start,
            number,
        );
        if extra_per_space > 0.0 {
            // spread the spare width over the spaces: each glyph after a
            // space moves by one more share
            let mut shift = 0.0;
            let mut chars = line_text.chars().filter(|c| choice.by_char.contains_key(c));
            for glyph in &mut glyphs {
                glyph.x += shift;
                if chars.next() == Some(' ') {
                    shift += extra_per_space;
                }
            }
        }
        placed.extend(glyphs);
    }

    let removed: HashSet<usize> = block
        .lines
        .iter()
        .flat_map(|line| glyphs_of(&line.chars))
        .collect();
    text_write::blank(&model.run, plan, &removed);
    let anchor = Anchor {
        show: anchor_show,
        text_matrix: anchor_matrix,
        line_step,
    };
    if !placed.is_empty() {
        text_write::write(draft.doc, &model.run, plan, &anchor, &placed)?;
    }
    let overflow = (lines.len() > block.lines.len()).then(|| TextOverflow {
        lines_before: block.lines.len(),
        lines_after: lines.len(),
        extra_height: (lines.len() - block.lines.len()) as f32 * leading,
    });
    Ok(Outcome {
        notices: choice.notices,
        overflow,
    })
}

/// greedy line breaking: words move to the next line when they don't
/// fit; a new line in the text always starts a new line. each line says
/// whether it ends a paragraph (justified text leaves those ragged).
pub(crate) fn wrap(text: &str, width: f32, measure: &dyn Fn(&str) -> f32) -> Vec<(String, bool)> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split(' ').filter(|word| !word.is_empty()) {
            let candidate = if current.is_empty() {
                word.to_owned()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty() || measure(&candidate) <= width + 1e-3 {
                current = candidate;
            } else {
                lines.push((std::mem::take(&mut current), false));
                current = word.to_owned();
            }
        }
        lines.push((current, true));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greedy_wrapping_fills_lines() {
        let measure = |word: &str| word.chars().count() as f32;
        let lines = wrap("aa bb cc dd\nee", 5.0, &measure);
        let texts: Vec<&str> = lines.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(texts, ["aa bb", "cc dd", "ee"]);
        assert_eq!(
            lines.iter().map(|(_, end)| *end).collect::<Vec<_>>(),
            [false, true, true]
        );
    }
}
