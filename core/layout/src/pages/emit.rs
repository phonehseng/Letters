//! turning one laid out line of a paragraph into result records: glyph
//! runs at page coordinates, decoration rules, leaders and carets.

use letters_office::model::TabLeader;

use crate::fonts::FontLibrary;
use crate::paragraph::{LineLayout, ParagraphLayout};
use crate::result::{GlyphRun, Line, PlacedImage, Rect, RectKind, Story};
use crate::text::{ClusterKind, ParagraphText};

/// where a line goes on its page.
pub struct LinePlacement<'a> {
    pub story: Story,
    pub block: &'a [u32],
    /// page x of the column's (or cell's) left edge
    pub left: f64,
    /// page y of the line's top
    pub top: f64,
}

/// what one line puts on its page.
pub struct Emitted {
    pub line: Line,
    pub rects: Vec<Rect>,
    pub images: Vec<PlacedImage>,
}

/// slant used for an italic the family does not have (word and
/// libreoffice both lean synthesised italics by about 12 degrees)
const SYNTHETIC_ITALIC_DEGREES: f64 = 12.0;

pub fn emit_line(
    text: &ParagraphText,
    layout: &ParagraphLayout,
    line: &LineLayout,
    place: &LinePlacement,
    fonts: &FontLibrary,
) -> Emitted {
    let baseline = place.top + line.vertical.baseline;
    let mut runs: Vec<GlyphRun> = Vec::new();
    let mut rects = Vec::new();
    let mut images = Vec::new();
    for index in line.start..line.end {
        let cluster = &text.clusters[index];
        let x = place.left + layout.cluster_x[index];
        let width = layout.cluster_width[index];
        let style = &text.styles[usize::from(cluster.style)];
        let y = baseline - style.baseline_shift;
        match cluster.kind {
            ClusterKind::Glyphs | ClusterKind::Space => {
                push_glyphs(&mut runs, text, index, x, y, fonts);
            }
            ClusterKind::Object(object) => {
                let object = &text.objects[object as usize];
                images.push(PlacedImage {
                    media_id: object.media_id.clone(),
                    x,
                    y: baseline - object.height,
                    width: object.width,
                    height: object.height,
                    story: place.story.clone(),
                    block: place.block.to_vec(),
                    offset: cluster.offset,
                    floating: false,
                });
            }
            _ => {}
        }
        decorate(&mut rects, text, index, x, width, y, fonts);
    }
    for (from, to, leader, tab) in &line.leaders {
        let (from, to) = (place.left + from, place.left + to);
        match leader_glyphs(text, *tab, *leader, from, to, baseline, fonts) {
            Some(run) => runs.push(run),
            None => rects.extend(leader_rect(*leader, from, to, baseline)),
        }
    }
    // runs in reading order (leaders are added after the text around them)
    runs.sort_by(|a, b| {
        a.x.first()
            .unwrap_or(&0.0)
            .total_cmp(b.x.first().unwrap_or(&0.0))
    });
    let (start, end, carets) = carets(text, layout, line, place.left);
    Emitted {
        line: Line {
            id: 0,
            story: place.story.clone(),
            block: place.block.to_vec(),
            start,
            end,
            x: place.left + line.x,
            width: line.width,
            top: place.top,
            height: line.vertical.height,
            baseline,
            ascent: line.vertical.ascent,
            descent: line.vertical.descent,
            carets,
            runs,
        },
        rects,
        images,
    }
}

/// adds a cluster's glyphs to the last run when font, size, colour and
/// baseline match, otherwise starts a new run.
fn push_glyphs(
    runs: &mut Vec<GlyphRun>,
    text: &ParagraphText,
    index: usize,
    x: f64,
    y: f64,
    fonts: &FontLibrary,
) {
    let cluster = &text.clusters[index];
    let style = &text.styles[usize::from(cluster.style)];
    let glyphs = &text.glyphs[cluster.glyph_start as usize
        ..(cluster.glyph_start as usize + usize::from(cluster.glyph_count))];
    let Some(first) = glyphs.first() else {
        return;
    };
    let size = if style.caps == crate::style::Caps::Small && cluster.ch.is_lowercase() {
        style.size * crate::style::SMALL_CAPS_SCALE
    } else {
        style.size
    };
    let font = first.font;
    // a tab ends the run: what follows starts a new one, so a leader drawn
    // in the tab's room sits between them in the pdf's reading order
    let after_tab = index > 0 && text.clusters[index - 1].kind == ClusterKind::Tab;
    let joins = !after_tab
        && runs.last().is_some_and(|run| {
            run.font == font && run.size == size && run.y == y && run.color == style.color
        });
    if !joins {
        let resolved = fonts.font(font);
        runs.push(GlyphRun {
            font,
            size,
            y,
            color: style.color,
            glyphs: Vec::new(),
            x: Vec::new(),
            text: String::new(),
            clusters: Vec::new(),
            start: cluster.offset,
            end: cluster.offset,
            skew: if resolved.synthetic_italic {
                SYNTHETIC_ITALIC_DEGREES
            } else {
                0.0
            },
            embolden: if resolved.synthetic_bold || resolved.face.stroke_for_weight {
                size / 30.0
            } else {
                0.0
            },
        });
    }
    let run = runs.last_mut().expect("just pushed");
    let char_index = run.text.chars().count() as u32;
    let shown = shown_text(text, index);
    let mut pen = x;
    for glyph in glyphs {
        run.glyphs.push(glyph.glyph);
        run.x.push(pen + glyph.x_offset);
        run.clusters.push(char_index);
        pen += glyph.advance;
    }
    run.text.push_str(&shown);
    run.end = run.end.max(cluster.offset + u32::from(cluster.units));
    run.start = run.start.min(cluster.offset);
}

/// the characters a cluster stands for, for the pdf's text layer.
fn shown_text(text: &ParagraphText, index: usize) -> String {
    let cluster = &text.clusters[index];
    let start = cluster.char_start as usize;
    let end = start + usize::from(cluster.char_count);
    match text.chars.get(start..end) {
        Some(chars) if !chars.is_empty() => chars.iter().collect(),
        _ => cluster.ch.to_string(),
    }
}

/// underline, strike-through and highlight for one cluster, from the
/// font's own decoration metrics.
fn decorate(
    rects: &mut Vec<Rect>,
    text: &ParagraphText,
    index: usize,
    x: f64,
    width: f64,
    y: f64,
    fonts: &FontLibrary,
) {
    let cluster = &text.clusters[index];
    let style = &text.styles[usize::from(cluster.style)];
    if style.underline.is_none()
        && !style.strike
        && !style.double_strike
        && style.highlight.is_none()
    {
        return;
    }
    let face = &fonts.font(cluster.font).face;
    let scale = face.scale(style.size);
    let metrics = face.metrics;
    let color = style.color.unwrap_or([0, 0, 0]);
    if let Some(highlight) = &style.highlight {
        let line = crate::fonts::line_metrics(&metrics, style.size, crate::flavor::Flavor::Word);
        rects.push(Rect {
            kind: RectKind::Highlight,
            x,
            y: y - line.ascent,
            width,
            height: line.height(),
            color: highlight_color(highlight),
            dotted: false,
        });
    }
    if let Some(underline) = &style.underline {
        let thickness = (metrics.underline_thickness * scale).max(0.25);
        rects.push(Rect {
            kind: RectKind::Underline,
            x,
            y: y - metrics.underline_position * scale - thickness / 2.0,
            width,
            height: thickness,
            color,
            dotted: underline.contains("dot") || underline.contains("dash"),
        });
    }
    if style.strike || style.double_strike {
        let thickness = (metrics.strikeout_thickness * scale).max(0.25);
        let middle = y - metrics.strikeout_position * scale;
        let offsets: &[f64] = if style.double_strike {
            &[-thickness, thickness]
        } else {
            &[0.0]
        };
        for offset in offsets {
            rects.push(Rect {
                kind: RectKind::Strike,
                x,
                y: middle + offset - thickness / 2.0,
                width,
                height: thickness,
                color,
                dotted: false,
            });
        }
    }
}

/// word's named highlighter colours.
fn highlight_color(name: &str) -> [u8; 3] {
    match name {
        "yellow" => [255, 255, 0],
        "green" => [0, 255, 0],
        "cyan" => [0, 255, 255],
        "magenta" => [255, 0, 255],
        "blue" => [0, 0, 255],
        "red" => [255, 0, 0],
        "darkBlue" => [0, 0, 128],
        "darkCyan" => [0, 128, 128],
        "darkGreen" => [0, 128, 0],
        "darkMagenta" => [128, 0, 128],
        "darkRed" => [128, 0, 0],
        "darkYellow" => [128, 128, 0],
        "darkGray" => [128, 128, 128],
        "lightGray" => [192, 192, 192],
        "black" => [0, 0, 0],
        other => crate::style::parse_hex_color(other).unwrap_or([255, 255, 0]),
    }
}

/// a tab leader as characters, as word and libreoffice draw them: as many
/// dots (or hyphens, underscores) as fit between the text before the tab
/// and the text after it, ending at the text after it.
fn leader_glyphs(
    text: &ParagraphText,
    tab: usize,
    leader: TabLeader,
    from: f64,
    to: f64,
    baseline: f64,
    fonts: &FontLibrary,
) -> Option<GlyphRun> {
    let ch = match leader {
        TabLeader::Dot => '.',
        TabLeader::Hyphen => '-',
        TabLeader::Underscore => '_',
        TabLeader::MiddleDot => '\u{b7}',
        TabLeader::Heavy | TabLeader::None => return None,
    };
    let cluster = text.clusters.get(tab)?;
    let style = &text.styles[usize::from(cluster.style)];
    // the tab cluster carries its run's font (see text::build)
    let font = cluster.font;
    let face = &fonts.font(font).face;
    let (glyph, advance) = face.glyph(ch)?;
    let advance = f64::from(advance) * face.scale(style.size);
    if advance <= 0.0 {
        return None;
    }
    let count = ((to - from) / advance).floor().max(0.0) as usize;
    if count == 0 {
        return None;
    }
    let start = to - count as f64 * advance;
    Some(GlyphRun {
        font,
        size: style.size,
        y: baseline - style.baseline_shift,
        color: style.color,
        glyphs: vec![glyph; count],
        x: (0..count).map(|i| start + i as f64 * advance).collect(),
        text: std::iter::repeat_n(ch, count).collect(),
        clusters: (0..count as u32).collect(),
        start: cluster.offset,
        end: cluster.offset + u32::from(cluster.units),
        skew: 0.0,
        embolden: 0.0,
    })
}

/// a tab leader as a dotted (or solid) rule along the baseline.
fn leader_rect(leader: TabLeader, from: f64, to: f64, baseline: f64) -> Option<Rect> {
    let (dotted, lift, thickness) = match leader {
        TabLeader::None => return None,
        TabLeader::Dot | TabLeader::MiddleDot => (
            true,
            if leader == TabLeader::MiddleDot {
                3.0
            } else {
                0.0
            },
            0.6,
        ),
        TabLeader::Hyphen => (true, 3.0, 0.5),
        TabLeader::Underscore => (false, -1.0, 0.5),
        TabLeader::Heavy => (false, -1.0, 1.2),
    };
    // leave a little room on both sides, as the oracles do
    let (from, to) = (from + 1.5, to - 1.5);
    (to > from).then_some(Rect {
        kind: RectKind::Leader,
        x: from,
        y: baseline - lift - thickness,
        width: to - from,
        height: thickness,
        color: [0, 0, 0],
        dotted,
    })
}

/// the model range of a line and a caret x for every offset in it.
fn carets(
    text: &ParagraphText,
    layout: &ParagraphLayout,
    line: &LineLayout,
    left: f64,
) -> (u32, u32, Vec<f64>) {
    let model = || {
        text.clusters[line.start..line.end]
            .iter()
            .filter(|c| c.units > 0)
    };
    let start = model()
        .map(|c| c.offset)
        .min()
        .unwrap_or_else(|| first_offset_after(text, line.end));
    let mut end = model()
        .map(|c| c.offset + u32::from(c.units))
        .max()
        .unwrap_or(start);
    if line.end >= text.clusters.len() {
        end = end.max(text.length);
    }
    let mut carets = vec![f64::NAN; (end - start + 1) as usize];
    for index in line.start..line.end {
        let cluster = &text.clusters[index];
        if cluster.units == 0 {
            continue;
        }
        let x = left + layout.cluster_x[index];
        let width = layout.cluster_width[index];
        for unit in 0..u32::from(cluster.units) {
            let slot = (cluster.offset + unit - start) as usize;
            carets[slot] = x + width * f64::from(unit) / f64::from(cluster.units);
        }
        let after = (cluster.offset + u32::from(cluster.units) - start) as usize;
        if after < carets.len() {
            carets[after] = x + width;
        }
    }
    // offsets with no cluster (hidden or deleted text) take the caret before
    let mut previous = left + line.x;
    for caret in carets.iter_mut() {
        if caret.is_nan() {
            *caret = previous;
        }
        previous = *caret;
    }
    (start, end, carets)
}

fn first_offset_after(text: &ParagraphText, from: usize) -> u32 {
    text.clusters[from.min(text.clusters.len())..]
        .iter()
        .find(|c| c.units > 0)
        .map(|c| c.offset)
        .unwrap_or(text.length)
}
