//! cleaning a document up without changing how it looks: neighbouring
//! runs with the same formatting become one, empty runs and formatting
//! that only repeats the style go, exact sizes that say nothing the twips
//! do not are dropped, styles, lists and pictures nothing uses are
//! removed, and (when asked) a stack of one-line text boxes becomes one box
//! when the geometry proves the lines land where they were.
//!
//! drawings that still carry the xml they were read from are left alone:
//! they are written back byte for byte, which is tidier than anything a
//! rewrite could do.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidyOptions {
    pub merge_runs: bool,
    pub drop_redundant_formatting: bool,
    pub normalise_units: bool,
    pub remove_unused: bool,
    /// one text box for a stack of one-line boxes (off unless asked)
    pub merge_stacked_text_boxes: bool,
}

impl Default for TidyOptions {
    fn default() -> Self {
        TidyOptions {
            merge_runs: true,
            drop_redundant_formatting: true,
            normalise_units: true,
            remove_unused: true,
            merge_stacked_text_boxes: false,
        }
    }
}

/// what a tidy did, for the person and for the tests.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidyReport {
    pub runs_merged: usize,
    pub marks_dropped: usize,
    pub paragraph_properties_dropped: usize,
    pub units_normalised: usize,
    pub styles_removed: usize,
    pub lists_removed: usize,
    pub pictures_removed: usize,
    pub text_boxes_merged: usize,
}

/// a tidied copy of `document` and what changed.
pub fn tidy(document: &Document, options: &TidyOptions) -> (Document, TidyReport) {
    let mut tidied = document.clone();
    let mut report = TidyReport::default();
    if options.merge_stacked_text_boxes {
        report.text_boxes_merged = merge_stacked_boxes(&mut tidied.blocks);
    }
    let styles = tidied.clone();
    for_each_paragraph(&mut tidied, &mut |paragraph| {
        if options.drop_redundant_formatting {
            report.marks_dropped += drop_style_marks(&styles, paragraph);
            report.paragraph_properties_dropped += drop_style_properties(&styles, paragraph);
        }
        if options.merge_runs {
            let before = paragraph.content.len();
            paragraph.content = merge_adjacent_text(std::mem::take(&mut paragraph.content));
            report.runs_merged += before - paragraph.content.len();
        }
    });
    if options.normalise_units {
        report.units_normalised = normalise_units(&mut tidied.blocks);
    }
    if options.remove_unused {
        remove_unused(&mut tidied, &mut report);
    }
    (tidied, report)
}

/// calls `visit` on every paragraph the writer will rewrite: the body,
/// tables, text boxes, shapes and groups, headers, footers, notes and
/// comments. drawings that keep their original xml are skipped.
fn for_each_paragraph(document: &mut Document, visit: &mut dyn FnMut(&mut Paragraph)) {
    fn blocks(list: &mut [Block], visit: &mut dyn FnMut(&mut Paragraph)) {
        for block in list {
            match block {
                Block::Paragraph(paragraph) => visit(paragraph),
                Block::Heading(heading) => visit(&mut heading.paragraph),
                Block::Table(table) => {
                    for cell in table.rows.iter_mut().flat_map(|row| row.cells.iter_mut()) {
                        blocks(&mut cell.blocks, visit);
                    }
                }
                Block::TextBox(text_box) if text_box.original.is_none() => {
                    blocks(&mut text_box.blocks, visit)
                }
                Block::Shape(shape) if shape.original.is_none() => {
                    if let Some(text) = shape.text.as_mut() {
                        blocks(&mut text.blocks, visit);
                    }
                }
                _ => {}
            }
        }
    }
    blocks(&mut document.blocks, visit);
    for part in &mut document.headers_footers {
        blocks(&mut part.blocks, visit);
    }
    for note in &mut document.notes {
        blocks(&mut note.blocks, visit);
    }
    for comment in &mut document.comments {
        blocks(&mut comment.blocks, visit);
    }
}

/// drops marks that set what the paragraph's style already gives. direct
/// formatting overrides the style with the same value, so nothing shows
/// differently. text with a character style is left alone: character
/// styles switch toggles on and off, which a plain comparison cannot see.
fn drop_style_marks(document: &Document, paragraph: &mut Paragraph) -> usize {
    let style = crate::styles::resolved_run_props(document, paragraph.style_id.as_deref(), &[]);
    let mut dropped = 0;
    for inline in &mut paragraph.content {
        let Some(marks) = inline.marks_mut() else {
            continue;
        };
        if marks
            .iter()
            .any(|m| matches!(m, Mark::CharacterStyle { .. } | Mark::Off { .. }))
        {
            continue;
        }
        let before = marks.len();
        marks.retain(|mark| !repeats_style(mark, &style));
        dropped += before - marks.len();
    }
    dropped
}

fn repeats_style(mark: &Mark, style: &RunProps) -> bool {
    match mark {
        Mark::Bold => style.bold == Some(true),
        Mark::Italic => style.italic == Some(true),
        Mark::FontSize { half_points } => style.size == Some(*half_points),
        Mark::Color { hex } => style
            .color
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case(hex)),
        Mark::FontFamily {
            name,
            theme: None,
            extra,
        } if extra.is_empty() => style.font.as_ref().is_some_and(|font| {
            font.theme.is_none() && font.name == *name && font.extra.is_empty()
        }),
        _ => false,
    }
}

/// drops paragraph properties that repeat the style's.
fn drop_style_properties(document: &Document, paragraph: &mut Paragraph) -> usize {
    let plain = Paragraph {
        style_id: paragraph.style_id.clone(),
        ..Paragraph::default()
    };
    let style = crate::styles::resolved_paragraph_props(document, &plain);
    let props = &mut paragraph.props;
    let mut dropped = 0;
    let mut clear = |own: bool| {
        if own {
            dropped += 1;
        }
        own
    };
    if clear(props.alignment.is_some() && props.alignment == style.alignment) {
        props.alignment = None;
    }
    if clear(props.spacing.before.is_some() && props.spacing.before == style.spacing.before) {
        props.spacing.before = None;
    }
    if clear(props.spacing.after.is_some() && props.spacing.after == style.spacing.after) {
        props.spacing.after = None;
    }
    if clear(props.spacing.line.is_some() && props.spacing.line == style.spacing.line) {
        props.spacing.line = None;
    }
    if clear(props.indent.left.is_some() && props.indent.left == style.indent.left) {
        props.indent.left = None;
    }
    if clear(props.indent.right.is_some() && props.indent.right == style.indent.right) {
        props.indent.right = None;
    }
    if clear(props.keep_with_next.is_some() && props.keep_with_next == style.keep_with_next) {
        props.keep_with_next = None;
    }
    dropped
}

/// exact emu values that say no more than the twips beside them.
fn normalise_units(blocks: &mut [Block]) -> usize {
    let mut count = 0;
    let mut position = |position: &mut FloatPosition| {
        if position.offset_emu == Some(crate::units::twips_to_emu(position.offset)) {
            position.offset_emu = None;
            count += 1;
        }
    };
    for block in blocks.iter_mut() {
        match block {
            Block::TextBox(text_box) if text_box.original.is_none() => {
                position(&mut text_box.horizontal);
                position(&mut text_box.vertical);
            }
            Block::Shape(shape) if shape.original.is_none() => {
                position(&mut shape.horizontal);
                position(&mut shape.vertical);
            }
            _ => {}
        }
    }
    let mut size = |emu: &mut Option<i64>, twips: Twips| {
        if *emu == Some(crate::units::twips_to_emu(twips)) {
            *emu = None;
            count += 1;
        }
    };
    for block in blocks.iter_mut() {
        match block {
            Block::TextBox(text_box) if text_box.original.is_none() => {
                size(&mut text_box.width_emu, text_box.width);
                size(&mut text_box.height_emu, text_box.height);
            }
            Block::Shape(shape) if shape.original.is_none() => {
                size(&mut shape.width_emu, shape.width);
                size(&mut shape.height_emu, shape.height);
            }
            _ => {}
        }
    }
    count
}

/// every string the document keeps as xml: preserved pieces and the
/// originals of drawings, which may name styles, lists and pictures.
fn kept_xml(document: &Document) -> String {
    let json = serde_json::to_string(&document.blocks).unwrap_or_default();
    let parts = serde_json::to_string(&document.headers_footers).unwrap_or_default();
    let notes = serde_json::to_string(&document.notes).unwrap_or_default();
    format!("{json}{parts}{notes}{}", document.styles.preserved.concat())
}

/// removes styles, lists and pictures nothing refers to. a name that
/// appears anywhere in the document's model or kept xml counts as used,
/// so nothing a preserved piece needs can go.
fn remove_unused(document: &mut Document, report: &mut TidyReport) {
    let everything = kept_xml(document);
    let mentioned = |id: &str| {
        everything.contains(&format!("\"{id}\"")) || everything.contains(&format!("\\\"{id}\\\""))
    };
    let mut keep: BTreeSet<String> = BTreeSet::new();
    for style in &document.styles.styles {
        if style.is_default || mentioned(&style.id) {
            keep.insert(style.id.clone());
        }
    }
    // a kept style keeps the styles it is based on and is followed by
    let mut grew = true;
    while grew {
        grew = false;
        for style in &document.styles.styles {
            if !keep.contains(&style.id) {
                continue;
            }
            for linked in [&style.based_on, &style.next].into_iter().flatten() {
                grew |= keep.insert(linked.clone());
            }
        }
    }
    let before = document.styles.styles.len();
    document
        .styles
        .styles
        .retain(|style| keep.contains(&style.id));
    report.styles_removed = before - document.styles.styles.len();
    let styles_json = serde_json::to_string(&document.styles).unwrap_or_default();
    let list_used = |id: &str| mentioned(id) || styles_json.contains(&format!("\"{id}\""));
    let before = document.numbering.instances.len();
    document
        .numbering
        .instances
        .retain(|instance| list_used(&instance.id));
    let definitions: BTreeSet<String> = document
        .numbering
        .instances
        .iter()
        .map(|i| i.definition_id.clone())
        .collect();
    let before_definitions = document.numbering.definitions.len();
    document
        .numbering
        .definitions
        .retain(|definition| definitions.contains(&definition.id) || list_used(&definition.id));
    report.lists_removed = (before - document.numbering.instances.len())
        + (before_definitions - document.numbering.definitions.len());
    let before = document.media.len();
    document.media.retain(|item| mentioned(&item.id));
    report.pictures_removed = before - document.media.len();
}

/// merges runs of stacked one-line text boxes. returns how many boxes
/// were folded into an earlier one.
fn merge_stacked_boxes(blocks: &mut Vec<Block>) -> usize {
    let mut merged = 0;
    let mut index = 0;
    while index + 1 < blocks.len() {
        let stacks = matches!(
            (&blocks[index], &blocks[index + 1]),
            (Block::TextBox(first), Block::TextBox(next)) if stacks_under(first, next)
        );
        if !stacks {
            index += 1;
            continue;
        }
        let Block::TextBox(next) = blocks.remove(index + 1) else {
            continue;
        };
        let added_height = stackable_line(&next).unwrap_or(0);
        if let Block::TextBox(first) = &mut blocks[index] {
            first.blocks.extend(next.blocks);
            first.height += added_height;
            first.height_emu = None;
            first.original = None;
        }
        merged += 1;
    }
    merged
}
/// the exact line height of a box that holds one plain paragraph and
/// nothing that could make its lines land elsewhere, or `None`.
fn stackable_line(text_box: &TextBox) -> Option<Twips> {
    let plain = text_box.border.is_none()
        && text_box.fill.is_none()
        && text_box.rotation.is_none()
        && text_box.chain.is_none()
        && text_box.table_cell.is_none()
        && !text_box.auto_height
        && text_box.vertical_align.is_none();
    if !plain {
        return None;
    }
    let paragraphs: Vec<&Paragraph> = text_box
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .collect();
    if paragraphs.len() != text_box.blocks.len() || paragraphs.is_empty() {
        return None;
    }
    let mut total = 0;
    for paragraph in paragraphs {
        let spacing = &paragraph.props.spacing;
        let line = spacing.line.filter(|l| l.rule == LineRule::Exact)?;
        if spacing.before.unwrap_or(0) != 0
            || spacing.after.unwrap_or(0) != 0
            || paragraph.props.list.is_some()
        {
            return None;
        }
        total += line.value;
    }
    let margins = text_box.margins;
    (text_box.height == margins.top + total + margins.bottom).then_some(total)
}

/// `next` sits exactly where `first`'s next line would: same column,
/// width, margins, wrap and anchor, its text starting where `first`'s
/// text ends, sharing `first`'s anchor paragraph.
fn stacks_under(first: &TextBox, next: &TextBox) -> bool {
    let Some(line) = stackable_line(first) else {
        return false;
    };
    if stackable_line(next).is_none() {
        return false;
    }
    let same_frame = first.horizontal == next.horizontal
        && first.vertical.relative_to == next.vertical.relative_to
        && first.vertical.align.is_none()
        && next.vertical.align.is_none()
        && first.width == next.width
        && first.margins == next.margins
        && first.wrap == next.wrap;
    let shares_anchor = next
        .anchor
        .as_ref()
        .is_some_and(|a| a.with == AnchorWith::Previous)
        || (first.anchor.as_ref().map(|a| a.with) == Some(AnchorWith::Next)
            && next.anchor.as_ref().map(|a| a.with) == Some(AnchorWith::Next));
    same_frame && shares_anchor && next.vertical.offset == first.vertical.offset + line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_box(y: Twips, text: &str, previous: bool) -> TextBox {
        let mut paragraph = Paragraph::with_text(text);
        paragraph.props.spacing.line = Some(LineSpacing {
            rule: LineRule::Exact,
            value: 280,
        });
        let mut text_box = TextBox::new(1440, y, 4000, 280, vec![Block::Paragraph(paragraph)]);
        text_box.border = None;
        text_box.fill = None;
        text_box.margins = BoxMargins {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let with = if previous {
            AnchorWith::Previous
        } else {
            AnchorWith::Own
        };
        text_box.anchor = Some(ObjectAnchor::with(with));
        text_box
    }

    #[test]
    fn runs_merge_and_repeated_formatting_goes() {
        let mut document = Document::new();
        document.styles.styles[0].run.bold = Some(true);
        document.blocks = vec![Block::Paragraph(Paragraph {
            style_id: Some("Normal".into()),
            content: vec![
                Inline::styled("one ", vec![Mark::Bold]),
                Inline::styled("two", vec![]),
                Inline::styled("", vec![Mark::Italic]),
            ],
            ..Default::default()
        })];
        let (tidied, report) = tidy(&document, &TidyOptions::default());
        assert_eq!(
            tidied.blocks[0].paragraph().unwrap().content,
            vec![Inline::text("one two")]
        );
        assert_eq!((report.marks_dropped, report.runs_merged), (1, 2));
    }

    #[test]
    fn stacked_line_boxes_become_one_box_only_when_they_line_up() {
        let mut document = Document::new();
        document.blocks = vec![
            Block::TextBox(line_box(2000, "first", false)),
            Block::TextBox(line_box(2280, "second", true)),
            Block::TextBox(line_box(2600, "apart", true)),
        ];
        let options = TidyOptions {
            merge_stacked_text_boxes: true,
            ..TidyOptions::default()
        };
        let (tidied, report) = tidy(&document, &options);
        assert_eq!(report.text_boxes_merged, 1);
        let Block::TextBox(merged) = &tidied.blocks[0] else {
            panic!()
        };
        assert_eq!((merged.blocks.len(), merged.height), (2, 560));
        assert_eq!(
            tidied.blocks.len(),
            2,
            "the third box is 40 twips too low to join"
        );
    }

    #[test]
    fn unused_styles_lists_and_pictures_go_and_used_ones_stay() {
        let mut document = Document::new();
        document
            .styles
            .styles
            .push(Style::paragraph("Unused", "Unused"));
        let mut quote = Style::paragraph("Quote", "Quote");
        quote.based_on = Some("Base".into());
        document.styles.styles.push(quote);
        document
            .styles
            .styles
            .push(Style::paragraph("Base", "Base"));
        document.blocks = vec![Block::Paragraph(Paragraph {
            style_id: Some("Quote".into()),
            ..Paragraph::with_text("quoted")
        })];
        document.media.push(MediaItem {
            id: "unused.png".into(),
            content_type: "image/png".into(),
            source_path: None,
            bytes: vec![1, 2, 3],
        });
        let (tidied, report) = tidy(&document, &TidyOptions::default());
        let ids: Vec<&str> = tidied.styles.styles.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["Normal", "Quote", "Base"]);
        assert_eq!((report.styles_removed, report.pictures_removed), (1, 1));
    }
}
