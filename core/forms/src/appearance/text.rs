//! drawing text answers: one line, comb cells or wrapped lines, sized to
//! fit when /DA asks for automatic size, aligned and clipped to the box.

use super::fonts::{AppearanceFont, DefaultAppearance};
use super::widget::{frame_content, Appearance, WidgetBox};
use crate::error::{FormError, FormResult};
use crate::fonts::FontMetrics;
use crate::geometry::number;
use crate::read::Alignment;
use crate::text::{
    encode_win_ansi, encode_win_ansi_lines, line_breaks_to_spaces, literal_string,
    normalize_line_breaks,
};

/// the font size used when a field asks for automatic size and the text
/// spans several lines.
pub(super) const AUTO_SIZE_MULTILINE_START: f32 = 12.0;
const AUTO_SIZE_MIN: f32 = 4.0;
const AUTO_SIZE_MAX: f32 = 24.0;
/// distance between lines as a share of the font size
pub(super) const LINE_SPACING: f32 = 1.15;

/// how a text answer should be laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub text: String,
    pub multiline: bool,
    /// the number of cells of a comb field
    pub comb_cells: Option<u32>,
    pub alignment: Alignment,
}

/// draws a text answer.
pub fn text_appearance(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    layout: &TextLayout,
) -> FormResult<Appearance> {
    // multiline fields keep the breaks people typed; one line fields (and
    // combs) show them as spaces, since they have nowhere to break
    let bytes = if layout.multiline && layout.comb_cells.is_none() {
        encode_win_ansi_lines(&normalize_line_breaks(&layout.text))
    } else {
        encode_win_ansi(&line_breaks_to_spaces(&layout.text))
    }
    .map_err(FormError::UnsupportedCharacters)?;
    let mut content = frame_content(widget, false);
    content.push_str("/Tx BMC\nq\n");
    let inset = widget.border_width.max(0.0) + 1.0;
    content.push_str(&format!(
        "{} {} {} {} re W n\n",
        number(inset),
        number(inset),
        number((widget.width - 2.0 * inset).max(0.0)),
        number((widget.height - 2.0 * inset).max(0.0))
    ));
    if !bytes.is_empty() {
        let text = if let Some(cells) = layout.comb_cells.filter(|cells| *cells > 0) {
            comb_text(widget, da, font, &bytes, cells)
        } else if layout.multiline {
            multiline_text(widget, da, font, &bytes, layout.alignment)
        } else {
            single_line_text(widget, da, font, &bytes, layout.alignment)
        };
        content.push_str(&text);
    }
    content.push_str("Q\nEMC\n");
    Ok(Appearance {
        content,
        fonts: vec![(font.resource_name.clone(), font.object.clone())],
    })
}

/// where text starts horizontally inside a box for an alignment.
fn aligned_x(widget: &WidgetBox, text_width: f32, alignment: Alignment) -> f32 {
    let padding = widget.border_width + 2.0;
    match alignment {
        Alignment::Left => padding,
        Alignment::Center => (widget.width - text_width) / 2.0,
        Alignment::Right => widget.width - padding - text_width,
    }
}

/// the baseline that centres one line of text of `size` in a box `height` tall.
fn centered_baseline(font: &AppearanceFont, size: f32, height: f32) -> f32 {
    let (ascent, descent) = font.metrics.standard.ascent_descent();
    let text_height = (ascent - descent) * size / 1000.0;
    (height - text_height) / 2.0 - descent * size / 1000.0
}

pub(super) fn text_start(font: &AppearanceFont, da: &DefaultAppearance, size: f32) -> String {
    format!(
        "BT\n/{} {} Tf\n{}\n",
        font.resource_name,
        number(size),
        da.color
    )
}

fn single_line_text(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    bytes: &[u8],
    alignment: Alignment,
) -> String {
    let available_width = widget.width - 2.0 * (widget.border_width + 2.0);
    let size = if da.size > 0.0 {
        da.size
    } else {
        let by_height = (widget.height - 2.0 * (widget.border_width + 1.0)) / LINE_SPACING;
        let unit_width = font.metrics.text_width(bytes, 1.0);
        let by_width = if unit_width > 0.0 {
            available_width / unit_width
        } else {
            by_height
        };
        by_height.min(by_width).clamp(AUTO_SIZE_MIN, AUTO_SIZE_MAX)
    };
    let width = font.metrics.text_width(bytes, size);
    let x = aligned_x(widget, width, alignment);
    let y = centered_baseline(font, size, widget.height);
    format!(
        "{}{} {} Td\n{} Tj\nET\n",
        text_start(font, da, size),
        number(x),
        number(y),
        literal_string(bytes)
    )
}

/// a comb field puts each character in its own equal cell.
fn comb_text(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    bytes: &[u8],
    cells: u32,
) -> String {
    let cell_width = widget.width / cells as f32;
    let size = if da.size > 0.0 {
        da.size
    } else {
        ((widget.height - 2.0 * (widget.border_width + 1.0)) / LINE_SPACING)
            .min(cell_width * 1.4)
            .clamp(AUTO_SIZE_MIN, AUTO_SIZE_MAX)
    };
    let y = centered_baseline(font, size, widget.height);
    let mut out = text_start(font, da, size);
    let mut previous_x = 0.0;
    for (index, byte) in bytes.iter().take(cells as usize).enumerate() {
        let character_width = font.metrics.text_width(&[*byte], size);
        let x = index as f32 * cell_width + (cell_width - character_width) / 2.0;
        let (dx, dy) = if index == 0 {
            (x, y)
        } else {
            (x - previous_x, 0.0)
        };
        out.push_str(&format!(
            "{} {} Td {} Tj\n",
            number(dx),
            number(dy),
            literal_string(&[*byte])
        ));
        previous_x = x;
    }
    out.push_str("ET\n");
    out
}

fn multiline_text(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    bytes: &[u8],
    alignment: Alignment,
) -> String {
    let padding = widget.border_width + 2.0;
    let available_width = (widget.width - 2.0 * padding).max(1.0);
    let available_height = widget.height - 2.0 * padding;
    let size = if da.size > 0.0 {
        da.size
    } else {
        // shrink until the wrapped text fits the box, but never below the minimum
        let mut size = AUTO_SIZE_MULTILINE_START;
        while size > AUTO_SIZE_MIN {
            let lines = wrap_lines(bytes, &font.metrics, size, available_width);
            if lines.len() as f32 * size * LINE_SPACING <= available_height {
                break;
            }
            size -= 0.5;
        }
        size
    };
    let lines = wrap_lines(bytes, &font.metrics, size, available_width);
    let (ascent, _) = font.metrics.standard.ascent_descent();
    let leading = size * LINE_SPACING;
    let mut y = widget.height - padding - ascent * size / 1000.0;
    let mut out = text_start(font, da, size);
    let mut previous = (0.0, 0.0);
    for line in lines {
        let x = aligned_x(widget, font.metrics.text_width(&line, size), alignment);
        out.push_str(&format!(
            "{} {} Td {} Tj\n",
            number(x - previous.0),
            number(y - previous.1),
            literal_string(&line)
        ));
        previous = (x, y);
        y -= leading;
    }
    out.push_str("ET\n");
    out
}

/// breaks text into lines that fit `width`: at line breaks the person typed,
/// then between words, and inside a word only when the word alone is too long.
pub fn wrap_lines(bytes: &[u8], metrics: &FontMetrics, size: f32, width: f32) -> Vec<Vec<u8>> {
    let text: Vec<u8> = bytes
        .iter()
        .map(|&byte| if byte == b'\r' { b'\n' } else { byte })
        .collect();
    let mut lines = Vec::new();
    for paragraph in text.split(|&byte| byte == b'\n') {
        let mut line: Vec<u8> = Vec::new();
        for word in paragraph.split(|&byte| byte == b' ') {
            let candidate = if line.is_empty() {
                word.to_vec()
            } else {
                [line.as_slice(), b" ", word].concat()
            };
            if metrics.text_width(&candidate, size) <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // a word wider than the line is cut into pieces that fit
            for &byte in word {
                let mut longer = line.clone();
                longer.push(byte);
                if metrics.text_width(&longer, size) > width && !line.is_empty() {
                    lines.push(std::mem::replace(&mut line, vec![byte]));
                } else {
                    line = longer;
                }
            }
        }
        lines.push(line);
    }
    lines
}
