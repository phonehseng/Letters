//! breaking a paragraph into lines and placing every cluster on them.
//!
//! the method is word's: greedy first fit. words are added to a line while
//! they fit; spaces at the end of a line hang into the margin and do not
//! count; a word longer than a whole line is cut between characters. the
//! result keeps full precision: positions are fractional points.

pub mod tabs;
pub mod vertical;

use letters_office::model::{Alignment, TabKind, TabLeader};

use crate::flavor::Flavor;
use crate::fonts::LineMetrics;
use crate::style::ParaStyle;
use crate::text::breaks::Break;
use crate::text::{ClusterKind, ParagraphText};
use tabs::next_stop;
pub use vertical::{line_box, LineBox, LineContent};

/// a little room for rounding, so text that exactly fills a line fits
const FIT_TOLERANCE: f64 = 0.001;

/// one laid out line of a paragraph.
#[derive(Clone, Debug, PartialEq)]
pub struct LineLayout {
    /// the clusters on this line, as a range of `ParagraphText::clusters`
    pub start: usize,
    pub end: usize,
    /// x of the line's first cluster from the column's left edge
    pub x: f64,
    /// width of the content, without trailing spaces
    pub width: f64,
    /// the line's vertical box: height, baseline from its top
    pub vertical: LineBox,
    /// the line ends with a page or column break character
    pub forced_break: Option<ClusterKind>,
    /// tab leaders to draw: from x, to x (column coordinates), the leader
    /// and the tab cluster it belongs to
    pub leaders: Vec<(f64, f64, TabLeader, usize)>,
}

/// a whole paragraph's lines and the x of every cluster.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParagraphLayout {
    pub lines: Vec<LineLayout>,
    /// x of each cluster from the column's left edge (after alignment)
    pub cluster_x: Vec<f64>,
    /// the final advance of each cluster (tabs and stretched spaces differ
    /// from their shaped width)
    pub cluster_width: Vec<f64>,
}

impl ParagraphLayout {
    pub fn height(&self) -> f64 {
        self.lines.iter().map(|line| line.vertical.height).sum()
    }
}

/// the room a line has: its left edge and width from the column's left,
/// and how far it must move down first (to clear a floating object).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineSpace {
    pub left: f64,
    pub width: f64,
    pub move_down: f64,
}

/// lays a paragraph out in a column of `width` points.
pub fn layout_paragraph(
    text: &ParagraphText,
    style: &ParaStyle,
    width: f64,
    flavor: Flavor,
) -> ParagraphLayout {
    layout_paragraph_in(text, style, flavor, &mut |_, _| LineSpace {
        left: 0.0,
        width,
        move_down: 0.0,
    })
}

/// lays a paragraph out where the room of each line may differ (text
/// flowing around a floating picture). `space(line_index, top_of_line)`
/// gives the room; the top is the sum of the heights of the lines before.
pub fn layout_paragraph_in(
    text: &ParagraphText,
    style: &ParaStyle,
    flavor: Flavor,
    space: &mut dyn FnMut(usize, f64) -> LineSpace,
) -> ParagraphLayout {
    let count = text.clusters.len();
    let mut layout = ParagraphLayout {
        lines: Vec::new(),
        cluster_x: vec![0.0; count],
        cluster_width: text.clusters.iter().map(|c| c.width).collect(),
    };
    let mut start = 0;
    let mut top = 0.0;
    loop {
        let first = layout.lines.is_empty();
        let room = space(layout.lines.len(), top);
        let indent_left = style.left + if first { style.first_line } else { 0.0 };
        let line_left = room.left + indent_left;
        let line_right = room.left + room.width - style.right;
        let end = find_line_end(text, style, start, line_left, line_right, first, flavor);
        let mut line = place_line(
            text,
            style,
            &mut layout,
            start,
            end,
            line_left,
            line_right,
            first,
            flavor,
        );
        // a line pushed below a floating object keeps the gap above it
        line.vertical.height += room.move_down;
        line.vertical.fit_height += room.move_down;
        line.vertical.baseline += room.move_down;
        top += line.vertical.height;
        layout.lines.push(line);
        start = end;
        if start >= count {
            break;
        }
    }
    layout
}

/// where the line starting at `start` ends: the furthest break
/// opportunity whose content still fits (greedy first fit).
fn find_line_end(
    text: &ParagraphText,
    style: &ParaStyle,
    start: usize,
    left: f64,
    right: f64,
    first: bool,
    flavor: Flavor,
) -> usize {
    let count = text.clusters.len();
    if start >= count {
        return count;
    }
    let mut best: Option<usize> = None;
    let mut candidate = start + 1;
    while candidate <= count {
        let at_opportunity =
            candidate == count || text.clusters[candidate].break_before != Break::None;
        if at_opportunity {
            let measured = measure(text, style, start, candidate, left, first);
            let limit = right + FIT_TOLERANCE + shrinkable(style, &measured, flavor);
            if measured.content_end > limit && best.is_some() {
                break;
            }
            if measured.content_end > limit {
                // the first word alone is too long: cut it between characters
                return cut_long_word(text, style, start, candidate, left, right, first);
            }
            best = Some(candidate);
            // a line or page break ends the line here
            if candidate < count && text.clusters[candidate].break_before == Break::Mandatory {
                break;
            }
            if ends_line(text, candidate - 1) {
                break;
            }
        }
        candidate += 1;
    }
    best.unwrap_or(count)
}

/// how much a justified line may be wider than its room: word squeezes
/// the spaces of justified lines by up to a share of their width rather
/// than move the last word down. measured on the corpus' justified times
/// new roman paragraphs: word fitted lines whose words were wider than the
/// room by up to a quarter of the line's space widths, never more (see the
/// report, "justified"). libreoffice never squeezes.
fn shrinkable(style: &ParaStyle, measured: &Measured, flavor: Flavor) -> f64 {
    if style.alignment != Alignment::Justify {
        return 0.0;
    }
    flavor.rules().justify_space_shrink * measured.space_width
}

fn ends_line(text: &ParagraphText, index: usize) -> bool {
    matches!(
        text.clusters[index].kind,
        ClusterKind::LineBreak | ClusterKind::PageBreak | ClusterKind::ColumnBreak
    )
}

/// as many clusters of an over-long word as fit, at least one.
fn cut_long_word(
    text: &ParagraphText,
    style: &ParaStyle,
    start: usize,
    end: usize,
    left: f64,
    right: f64,
    first: bool,
) -> usize {
    let mut fit = start + 1;
    for candidate in start + 2..=end {
        if measure(text, style, start, candidate, left, first).content_end > right + FIT_TOLERANCE {
            break;
        }
        fit = candidate;
    }
    fit
}

/// where clusters land when a line holds `start..end`.
struct Measured {
    /// pen at each cluster, from the column's left
    xs: Vec<f64>,
    /// final width of each cluster
    widths: Vec<f64>,
    /// the right edge of the content, trailing spaces left out
    content_end: f64,
    /// leaders: from, to, kind, and the tab's index within the line
    leaders: Vec<(f64, f64, TabLeader, usize)>,
    /// index (within the line) after the last tab, where justification starts
    after_last_tab: usize,
    /// the width of the spaces justification may stretch or squeeze
    space_width: f64,
}

/// a right, centre or decimal tab whose text is still being collected.
struct PendingTab {
    /// index within the line of the tab cluster
    tab: usize,
    start_pen: f64,
    stop: f64,
    kind: TabKind,
    leader: TabLeader,
    /// width of the text after the tab so far, and before its decimal point
    text_width: f64,
    before_decimal: Option<f64>,
}

impl PendingTab {
    /// where the tab's text would start, given the text collected so far.
    fn text_start(&self) -> f64 {
        let wanted = match self.kind {
            TabKind::Right => self.stop - self.text_width,
            TabKind::Center => self.stop - self.text_width / 2.0,
            TabKind::Decimal => self.stop - self.before_decimal.unwrap_or(self.text_width),
            _ => self.stop,
        };
        // text never moves left of where the tab started
        wanted.max(self.start_pen)
    }
}

fn measure(
    text: &ParagraphText,
    style: &ParaStyle,
    start: usize,
    end: usize,
    left: f64,
    first: bool,
) -> Measured {
    let mut xs = Vec::with_capacity(end - start);
    let mut widths = Vec::with_capacity(end - start);
    let mut leaders = Vec::new();
    let mut pen = left - label_offset(text, start, first);
    let mut pending: Option<PendingTab> = None;
    let mut after_last_tab = 0;
    for (local, cluster) in text.clusters[start..end].iter().enumerate() {
        if cluster.kind == ClusterKind::Tab {
            if let Some(tab) = pending.take() {
                pen = finish_tab(tab, &mut xs, &mut widths, &mut leaders, pen);
            }
            let stop = next_stop(style, pen, first);
            after_last_tab = local + 1;
            xs.push(pen);
            if stop.kind == TabKind::Left || stop.kind == TabKind::Bar {
                let width = (stop.position - pen).max(0.0);
                widths.push(width);
                if stop.leader != TabLeader::None {
                    leaders.push((pen, stop.position, stop.leader, local));
                }
                pen += width;
            } else {
                widths.push(0.0);
                pending = Some(PendingTab {
                    tab: local,
                    start_pen: pen,
                    stop: stop.position,
                    kind: stop.kind,
                    leader: stop.leader,
                    text_width: 0.0,
                    before_decimal: None,
                });
            }
            continue;
        }
        xs.push(pen);
        widths.push(cluster.width);
        pen += cluster.width;
        if let Some(tab) = pending.as_mut() {
            if tab.kind == TabKind::Decimal
                && tab.before_decimal.is_none()
                && (cluster.ch == '.' || cluster.ch == ',')
            {
                tab.before_decimal = Some(tab.text_width);
            }
            tab.text_width += cluster.width;
        }
    }
    if let Some(tab) = pending.take() {
        pen = finish_tab(tab, &mut xs, &mut widths, &mut leaders, pen);
    }
    // trailing spaces hang: the content ends at the last other cluster
    let mut content_end = left;
    for (local, cluster) in text.clusters[start..end].iter().enumerate().rev() {
        if cluster.kind != ClusterKind::Space && !is_break(cluster.kind) {
            content_end = xs[local] + widths[local];
            break;
        }
    }
    let _ = pen;
    let last_content = text.clusters[start..end]
        .iter()
        .rposition(|c| c.kind != ClusterKind::Space && !is_break(c.kind))
        .unwrap_or(0);
    let space_width = text.clusters[start..end]
        .iter()
        .enumerate()
        .filter(|(local, c)| {
            *local >= after_last_tab && *local < last_content && c.kind == ClusterKind::Space
        })
        .map(|(_, c)| c.width)
        .sum();
    Measured {
        xs,
        widths,
        content_end,
        leaders,
        after_last_tab,
        space_width,
    }
}

/// a right-aligned list label ends at the number position, a centred one
/// is centred on it (w:lvlJc); the label is the start of the first line.
fn label_offset(text: &ParagraphText, start: usize, first: bool) -> f64 {
    if !first || start != 0 || text.label_end == 0 {
        return 0.0;
    }
    let label_width: f64 = text.clusters[..text.label_end]
        .iter()
        .filter(|c| c.kind == ClusterKind::Glyphs)
        .map(|c| c.width)
        .sum();
    match text.label_alignment {
        Some(Alignment::Right) => label_width,
        Some(Alignment::Center) => label_width / 2.0,
        _ => 0.0,
    }
}

fn is_break(kind: ClusterKind) -> bool {
    matches!(
        kind,
        ClusterKind::LineBreak | ClusterKind::PageBreak | ClusterKind::ColumnBreak
    )
}

/// gives a pending tab its width now that its text is known, and moves
/// that text into place. returns the pen after it.
fn finish_tab(
    tab: PendingTab,
    xs: &mut [f64],
    widths: &mut [f64],
    leaders: &mut Vec<(f64, f64, TabLeader, usize)>,
    pen: f64,
) -> f64 {
    let text_start = tab.text_start();
    let shift = text_start - tab.start_pen;
    widths[tab.tab] = shift;
    for x in xs.iter_mut().skip(tab.tab + 1) {
        *x += shift;
    }
    if tab.leader != TabLeader::None && shift > 0.0 {
        leaders.push((tab.start_pen, text_start, tab.leader, tab.tab));
    }
    pen + shift
}

#[allow(clippy::too_many_arguments)]
fn place_line(
    text: &ParagraphText,
    style: &ParaStyle,
    layout: &mut ParagraphLayout,
    start: usize,
    end: usize,
    left: f64,
    right: f64,
    first: bool,
    flavor: Flavor,
) -> LineLayout {
    let mut measured = measure(text, style, start, end, left, first);
    let last_line = end >= text.clusters.len();
    let ends_with_break = end > start && is_break(text.clusters[end - 1].kind);
    let available = right - left;
    let used = measured.content_end - left;
    let slack = available - used;
    let shift = match style.alignment {
        Alignment::Center => slack.max(0.0) / 2.0,
        Alignment::Right => slack.max(0.0),
        _ => 0.0,
    };
    let justify = style.alignment == Alignment::Justify
        && !last_line
        && !(ends_with_break && !justifies_lines_ending_in_break(flavor, text, end));
    // a squeezed line (wider than its room) squeezes even when it is the
    // paragraph's last
    let squeezed = style.alignment == Alignment::Justify && slack < 0.0;
    if justify || squeezed {
        stretch_spaces(text, start, end, &mut measured, slack);
    }
    for (local, index) in (start..end).enumerate() {
        layout.cluster_x[index] = measured.xs[local] + shift;
        layout.cluster_width[index] = measured.widths[local];
    }
    let forced_break = (end > start)
        .then(|| text.clusters[end - 1].kind)
        .filter(|kind| matches!(kind, ClusterKind::PageBreak | ClusterKind::ColumnBreak));
    let content = line_content_metrics(text, start, end);
    LineLayout {
        start,
        end,
        x: left + shift,
        width: if justify || squeezed { available } else { used },
        vertical: line_box(&content, style, flavor),
        forced_break,
        leaders: measured
            .leaders
            .iter()
            .map(|(from, to, leader, local)| (from + shift, to + shift, *leader, start + local))
            .collect(),
    }
}

/// word stretches a justified line that ends with a manual line break
/// (unless a compatibility option says otherwise); libreoffice does not.
fn justifies_lines_ending_in_break(flavor: Flavor, text: &ParagraphText, end: usize) -> bool {
    flavor == Flavor::Word && text.clusters[end - 1].kind == ClusterKind::LineBreak
}

/// spreads `slack` over the spaces after the last tab, trailing spaces
/// excluded, as both word and libreoffice justify.
fn stretch_spaces(
    text: &ParagraphText,
    start: usize,
    end: usize,
    measured: &mut Measured,
    slack: f64,
) {
    let mut last_content = None;
    for local in (0..end - start).rev() {
        let kind = text.clusters[start + local].kind;
        if kind != ClusterKind::Space && !is_break(kind) {
            last_content = Some(local);
            break;
        }
    }
    let Some(last_content) = last_content else {
        return;
    };
    let spaces: Vec<usize> = (measured.after_last_tab..last_content)
        .filter(|&local| text.clusters[start + local].kind == ClusterKind::Space)
        .collect();
    if spaces.is_empty() || slack == 0.0 {
        return;
    }
    let extra = slack / spaces.len() as f64;
    let mut added = 0.0;
    let mut next_space = spaces.iter().peekable();
    for local in 0..measured.xs.len() {
        measured.xs[local] += added;
        if next_space.peek() == Some(&&local) {
            measured.widths[local] += extra;
            added += extra;
            next_space.next();
        }
    }
    measured.content_end += added;
}

/// the tallest ascent and descent of the line's content, with and
/// without its inline pictures.
fn line_content_metrics(text: &ParagraphText, start: usize, end: usize) -> LineContent {
    let mut metrics: Option<LineMetrics> = None;
    let mut text_only: Option<LineMetrics> = None;
    let mut scripts: Option<LineMetrics> = None;
    for cluster in &text.clusters[start..end] {
        let mut own = text.style_metrics[usize::from(cluster.style)];
        if matches!(cluster.kind, ClusterKind::Anchor(_)) {
            continue;
        }
        let style = &text.styles[usize::from(cluster.style)];
        if (style.size - style.nominal_size).abs() > 1e-9 {
            scripts = Some(scripts.map_or(own, |current| current.max(own)));
            continue;
        }
        text_only = Some(text_only.map_or(own, |current| current.max(own)));
        if let ClusterKind::Object(index) = cluster.kind {
            // an inline picture stands on the baseline
            own.ascent = own.ascent.max(text.objects[index as usize].height);
        }
        metrics = Some(match metrics {
            Some(current) => current.max(own),
            None => own,
        });
    }
    let has_content = end > start && !text.clusters[start..end].iter().all(|c| is_break(c.kind));
    // a line holding pictures and no text stands on the pictures alone:
    // word gives it no descent (real_lab_report: a figure paragraph 3.8 pt
    // shorter than with the font's descent below the picture)
    let only_objects = text.clusters[start..end]
        .iter()
        .any(|c| matches!(c.kind, ClusterKind::Object(_)))
        && text.clusters[start..end].iter().all(|c| {
            matches!(c.kind, ClusterKind::Object(_) | ClusterKind::Anchor(_)) || is_break(c.kind)
        });
    if only_objects {
        if let Some(found) = metrics.as_mut() {
            found.descent = 0.0;
        }
    }
    if metrics.is_none() && has_content {
        // a line of nothing but superscripts stands on their metrics
        metrics = scripts.take();
    }
    let text_only = text_only.or(metrics).unwrap_or(text.mark_metrics);
    // an empty line (or one holding only a break) takes the paragraph
    // mark's height. a line with text does not grow for a larger mark:
    // word's probes put the last line of a 9 pt paragraph with an 11 pt
    // mark at the same pitch as its other lines (font probes, word 16)
    match metrics {
        Some(found) if has_content => LineContent {
            content: found,
            text: text_only,
            scripts,
        },
        _ => LineContent {
            content: text.mark_metrics,
            text: text.mark_metrics,
            scripts: None,
        },
    }
}

#[cfg(test)]
mod tests;
