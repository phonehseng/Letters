//! what the ruler above the page shows for a paragraph, as numbers for
//! drawing it and as plain sentences for people who would rather read.

use serde::Serialize;

use crate::model::{Document, TabKind, TabLeader, Twips};
use crate::styles::resolved_paragraph_props;

/// every position is in twips from the left edge of the page.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulerState {
    pub page_width: Twips,
    pub left_margin: Twips,
    pub right_margin: Twips,
    /// where the lines after the first start
    pub left_indent: Twips,
    /// where the first line starts
    pub first_line_start: Twips,
    /// where lines end
    pub right_indent: Twips,
    pub tab_stops: Vec<RulerTab>,
    /// distance between the automatic stops when no tab stop is set
    pub default_tab_interval: Twips,
    /// the same in sentences
    pub description: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulerTab {
    pub position: Twips,
    pub kind: TabKind,
    pub leader: TabLeader,
}

/// the ruler for the paragraph at `block_index` (or the section's
/// margins alone when the block is not a paragraph).
pub fn describe_ruler(document: &Document, block_index: usize) -> RulerState {
    let section = document.section_of(block_index);
    let page = &section.page;
    let left_margin = page.margins.left + page.margins.gutter;
    let right_edge = page.width - page.margins.right;
    let props = document
        .blocks
        .get(block_index)
        .and_then(|block| block.paragraph())
        .map(|paragraph| resolved_paragraph_props(document, paragraph))
        .unwrap_or_default();
    let left = props.indent.left.unwrap_or(0);
    let right = props.indent.right.unwrap_or(0);
    let offset = props.indent.first_line_offset();
    let tab_stops: Vec<RulerTab> = props
        .tabs
        .iter()
        .filter(|stop| stop.kind != TabKind::Clear)
        .map(|stop| RulerTab {
            position: left_margin + stop.position,
            kind: stop.kind,
            leader: stop.leader,
        })
        .collect();
    let mut state = RulerState {
        page_width: page.width,
        left_margin,
        right_margin: right_edge,
        left_indent: left_margin + left,
        first_line_start: left_margin + left + offset,
        right_indent: right_edge - right,
        tab_stops,
        default_tab_interval: document.settings.default_tab_stop.unwrap_or(720),
        description: Vec::new(),
    };
    state.description = describe(&state, left, right, offset);
    state
}

fn inches(twips: Twips) -> String {
    format!("{:.2} in", f64::from(twips) / 1440.0)
}

fn describe(state: &RulerState, left: Twips, right: Twips, offset: Twips) -> Vec<String> {
    let mut lines = vec![format!(
        "The page is {} wide. Text runs between the margins, {} from the left edge and {} from the right edge.",
        inches(state.page_width),
        inches(state.left_margin),
        inches(state.page_width - state.right_margin)
    )];
    lines.push(match left {
        0 => "Lines start at the left margin.".to_string(),
        l if l > 0 => format!("Lines start {} in from the left margin.", inches(l)),
        l => format!("Lines start {} out into the left margin.", inches(-l)),
    });
    lines.push(match offset {
        0 => "The first line starts where the others do.".to_string(),
        o if o > 0 => format!("The first line is indented {} further (a first-line indent).", inches(o)),
        o => format!(
            "The first line starts {} to the left of the others (a hanging indent, as in a reference list).",
            inches(-o)
        ),
    });
    if right != 0 {
        lines.push(format!(
            "Lines end {} before the right margin.",
            inches(right)
        ));
    }
    if state.tab_stops.is_empty() {
        lines.push(format!(
            "There are no tab stops: Tab moves to the next {} step.",
            inches(state.default_tab_interval)
        ));
    }
    for stop in &state.tab_stops {
        let kind = match stop.kind {
            TabKind::Left => "left-aligned",
            TabKind::Center => "centred",
            TabKind::Right => "right-aligned",
            TabKind::Decimal => "aligned on the decimal point",
            TabKind::Bar => "a vertical line",
            TabKind::Clear => "cleared",
        };
        let leader = match stop.leader {
            TabLeader::None => String::new(),
            TabLeader::Dot => ", with dots leading up to it".into(),
            TabLeader::Hyphen => ", with hyphens leading up to it".into(),
            TabLeader::Underscore => ", with a line leading up to it".into(),
            TabLeader::MiddleDot => ", with middle dots leading up to it".into(),
            TabLeader::Heavy => ", with a heavy line leading up to it".into(),
        };
        lines.push(format!(
            "Tab stop at {} from the left margin: {kind}{leader}.",
            inches(stop.position - state.left_margin)
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Paragraph, TabStop};

    #[test]
    fn hanging_indent_and_a_tab_in_plain_words() {
        let mut document = Document::new();
        let mut paragraph = Paragraph::with_text("Reference");
        crate::indent::set_hanging_indent(&mut paragraph, 720);
        paragraph.props.tabs.push(TabStop {
            position: 9360,
            kind: TabKind::Right,
            leader: TabLeader::Dot,
        });
        document.blocks = vec![Block::Paragraph(paragraph)];
        let ruler = describe_ruler(&document, 0);
        assert_eq!(ruler.left_margin, 1440);
        assert_eq!(ruler.left_indent, 2160);
        assert_eq!(ruler.first_line_start, 1440);
        assert_eq!(ruler.right_indent, 12240 - 1440);
        assert_eq!(ruler.tab_stops[0].position, 1440 + 9360);
        let text = ruler.description.join(" ");
        assert!(text.contains("hanging indent"), "{text}");
        assert!(
            text.contains("Tab stop at 6.50 in from the left margin: right-aligned, with dots"),
            "{text}"
        );
    }
}
