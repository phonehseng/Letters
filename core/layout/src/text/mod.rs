//! a paragraph's inline content as a flat row of shaped clusters, the
//! input of line breaking.
//!
//! a cluster is the smallest piece a line can hold: one character, or a
//! few that shaped together (a ligature), a space, a tab, a picture. each
//! cluster knows where it came from in the model (`offset`, see
//! `result::ModelPosition`) and whether a line may start before it.
//!
//! model offsets: within a paragraph, every character of a text inline is
//! one unit, and every tab, break, picture, note reference and field is
//! one unit too. bookmarks, comment markers and preserved xml take none.

pub mod breaks;
pub mod shape;

use std::collections::HashMap;

use letters_office::model::{BreakKind, Document, FieldKind, Image, Inline, Paragraph, Placement};

use crate::flavor::Flavor;
use crate::fonts::{FontId, FontLibrary, LineMetrics};
use crate::style::{run_style_from_props, Caps, ParaStyle, RunStyle, SMALL_CAPS_SCALE};
use crate::unsupported::{Unsupported, UnsupportedKind};
use breaks::{break_opportunities, Break};
use shape::{shape, ShapeOptions, ShapedGlyph};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterKind {
    Glyphs,
    /// an ordinary space: it hangs at a line end and stretches in
    /// justified text
    Space,
    Tab,
    LineBreak,
    PageBreak,
    ColumnBreak,
    /// an inline picture, by index into `ParagraphText::objects`
    Object(u32),
    /// where a floating object is anchored; takes no room in the line
    Anchor(u32),
}

/// one unbreakable piece of a line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cluster {
    pub kind: ClusterKind,
    /// model offset of the first unit this cluster covers
    pub offset: u32,
    /// how many model offset units it covers (0 for generated text)
    pub units: u16,
    /// index into `ParagraphText::styles`
    pub style: u16,
    pub font: FontId,
    /// the glyphs, as a range of `ParagraphText::glyphs`
    pub glyph_start: u32,
    pub glyph_count: u16,
    /// horizontal advance in points (tabs get theirs during line breaking)
    pub width: f64,
    pub break_before: Break,
    /// text the model does not hold: a list label, a note number
    pub generated: bool,
    /// the first character it stands for (as typed, before caps)
    pub ch: char,
    /// all the characters it stands for (a ligature has several), as a
    /// range of `ParagraphText::chars`
    pub char_start: u32,
    pub char_count: u16,
}

/// a picture in the line or anchored to it.
#[derive(Clone, Debug, PartialEq)]
pub struct InlineObject {
    pub width: f64,
    pub height: f64,
    pub media_id: String,
    pub placement: Placement,
    /// which inline of the paragraph it is
    pub inline_index: usize,
}

/// the values fields show; unknown values keep the file's last result.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FieldValues {
    pub page: Option<u32>,
    pub pages: Option<u32>,
}

/// a paragraph ready to break into lines.
#[derive(Clone, Debug, Default)]
pub struct ParagraphText {
    pub clusters: Vec<Cluster>,
    pub glyphs: Vec<ShapedGlyph>,
    pub styles: Vec<RunStyle>,
    /// line metrics of each style's font at its line-height size
    pub style_metrics: Vec<LineMetrics>,
    pub objects: Vec<InlineObject>,
    /// note ids referenced, with the cluster index of the reference
    pub note_references: Vec<(String, usize)>,
    /// the paragraph mark's line metrics (the height of an empty line)
    pub mark_metrics: LineMetrics,
    /// the number of model offset units in the paragraph
    pub length: u32,
    /// the cluster after the list label and its tab, if there is a label
    pub label_end: usize,
    /// how the label sits at the number position
    pub label_alignment: Option<letters_office::model::Alignment>,
    /// the characters of every cluster, for the text layer of the pdf
    pub chars: Vec<char>,
}

/// what building a paragraph's text needs besides the paragraph.
pub struct TextContext<'a> {
    pub document: &'a Document,
    pub flavor: Flavor,
    pub fonts: &'a mut FontLibrary,
    pub note_numbers: &'a HashMap<String, String>,
    pub fields: FieldValues,
    pub unsupported: &'a mut Vec<Unsupported>,
    /// where the paragraph is, for unsupported-feature reports
    pub location: String,
    /// the style of the table the paragraph is in, if any
    pub table: Option<&'a crate::style::TableStyleProps>,
}

/// builds the shaped, breakable text of a paragraph. `label` is the list
/// label when the paragraph is in a list.
pub fn build(
    context: &mut TextContext,
    paragraph: &Paragraph,
    para: &ParaStyle,
    label: Option<&crate::lists::Label>,
) -> ParagraphText {
    let mut builder = Builder::new(context.flavor);
    builder.out.mark_metrics = builder.metrics_of(context.fonts, &para.mark);
    if let Some(label) = label {
        let index = builder.style_index(context.fonts, &label.style);
        builder.push_text(context.fonts, &label.text, index, 0, true);
        builder.out.label_alignment = Some(label.alignment);
        match label.suffix {
            LabelSuffix::Tab => builder.push_simple(ClusterKind::Tab, index, 0, 0, true),
            LabelSuffix::Space => builder.push_text(context.fonts, " ", index, 0, true),
            LabelSuffix::Nothing => {}
        }
        builder.out.label_end = builder.out.clusters.len();
    }
    let mut offset = 0u32;
    for (inline_index, inline) in paragraph.content.iter().enumerate() {
        offset += builder.push_inline(context, paragraph, inline, inline_index, offset);
    }
    builder.out.length = offset;
    builder.finish()
}

/// what follows a list label (w:suff): a tab by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelSuffix {
    Tab,
    Space,
    Nothing,
}

struct Builder {
    flavor: Flavor,
    out: ParagraphText,
    style_ids: HashMap<String, u16>,
}

impl Builder {
    fn new(flavor: Flavor) -> Builder {
        Builder {
            flavor,
            out: ParagraphText::default(),
            style_ids: HashMap::new(),
        }
    }

    fn metrics_of(&self, fonts: &mut FontLibrary, style: &RunStyle) -> LineMetrics {
        match fonts.resolve(&style.family, style.bold, style.italic) {
            Some(id) => {
                let face = &fonts.font(id).face;
                crate::fonts::line_metrics(&face.metrics, style.nominal_size, self.flavor)
            }
            None => LineMetrics {
                ascent: style.nominal_size * 0.8,
                descent: style.nominal_size * 0.2,
            },
        }
    }

    fn style_index(&mut self, fonts: &mut FontLibrary, style: &RunStyle) -> u16 {
        let key = format!("{style:?}");
        if let Some(index) = self.style_ids.get(&key) {
            return *index;
        }
        let index = self.out.styles.len() as u16;
        // a raised or lowered run moves its share of the line with it. a
        // superscript or subscript counts with its own smaller size and
        // its shift: in word a footnote number in 10 pt text grows the
        // line by the little it reaches above the text (footnotes corpus)
        let script = (style.size - style.nominal_size).abs() > 1e-9;
        let (metrics, shift) = if script {
            let mut smaller = style.clone();
            smaller.nominal_size = style.size;
            (self.metrics_of(fonts, &smaller), style.baseline_shift)
        } else {
            (self.metrics_of(fonts, style), style.line_shift)
        };
        let shifted = LineMetrics {
            ascent: metrics.ascent + shift.max(0.0),
            descent: metrics.descent - shift.min(0.0),
        };
        self.out.styles.push(style.clone());
        self.out.style_metrics.push(shifted);
        self.style_ids.insert(key, index);
        index
    }

    /// adds one inline and returns how many model offset units it covers.
    fn push_inline(
        &mut self,
        context: &mut TextContext,
        paragraph: &Paragraph,
        inline: &Inline,
        inline_index: usize,
        offset: u32,
    ) -> u32 {
        if inline.is_deleted() {
            // the final view: deleted text takes no room (the units still
            // count, so offsets stay those of the model)
            return units_of(inline);
        }
        let style = crate::style::run_style_in(
            context.document,
            paragraph,
            inline.marks(),
            context.table,
            self.flavor,
        );
        if style.hidden {
            return units_of(inline);
        }
        let index = self.style_index(context.fonts, &style);
        match inline {
            Inline::Text { text, .. } => {
                self.push_text(context.fonts, text, index, offset, false);
                text.chars().count() as u32
            }
            Inline::Tab { .. } => {
                self.push_simple(ClusterKind::Tab, index, offset, 1, false);
                // leaders are drawn in the tab's own font
                if let Some(font) = context
                    .fonts
                    .resolve(&style.family, style.bold, style.italic)
                {
                    if let Some(last) = self.out.clusters.last_mut() {
                        last.font = font;
                    }
                }
                1
            }
            Inline::Break { kind, .. } => {
                let kind = match kind {
                    BreakKind::Line => ClusterKind::LineBreak,
                    BreakKind::Page => ClusterKind::PageBreak,
                    BreakKind::Column => ClusterKind::ColumnBreak,
                };
                self.push_simple(kind, index, offset, 1, false);
                1
            }
            Inline::Image(image) => {
                self.push_image(image, index, offset, inline_index);
                1
            }
            Inline::NoteReference { note_id, .. } => {
                let number = context
                    .note_numbers
                    .get(note_id)
                    .cloned()
                    .unwrap_or_else(|| "?".into());
                let first = self.out.clusters.len();
                self.push_generated_unit(context.fonts, &number, index, offset);
                self.out.note_references.push((note_id.clone(), first));
                1
            }
            Inline::Field(field) => {
                let shown = match field.kind {
                    FieldKind::PageNumber => context.fields.page.map(|n| n.to_string()),
                    FieldKind::PageCount => context.fields.pages.map(|n| n.to_string()),
                    _ => None,
                }
                .unwrap_or_else(|| field.result.clone());
                if field.kind == FieldKind::TableOfContents {
                    context.unsupported.push(Unsupported::new(
                        UnsupportedKind::TableOfContentsField,
                        &context.location,
                    ));
                }
                self.push_generated_unit(context.fonts, &shown, index, offset);
                1
            }
            Inline::PreservedRun { xml, .. } => {
                self.push_preserved_run(context, xml, index, offset);
                1
            }
            Inline::Preserved { xml } => {
                if let Some(kind) = UnsupportedKind::of_preserved_inline(xml) {
                    context
                        .unsupported
                        .push(Unsupported::new(kind, &context.location));
                }
                0
            }
            Inline::BookmarkStart { .. }
            | Inline::BookmarkEnd { .. }
            | Inline::CommentReference { .. } => 0,
        }
    }

    /// text shown for one model unit (a note number, a field result): the
    /// first cluster covers the unit, the rest are generated.
    fn push_generated_unit(
        &mut self,
        fonts: &mut FontLibrary,
        text: &str,
        style: u16,
        offset: u32,
    ) {
        let first = self.out.clusters.len();
        self.push_text(fonts, text, style, offset, true);
        if let Some(cluster) = self.out.clusters.get_mut(first) {
            cluster.units = 1;
            cluster.generated = false;
        } else {
            self.push_simple(ClusterKind::Glyphs, style, offset, 1, false);
        }
    }

    /// a symbol (`w:sym`) is drawn; other unknown run content is reported.
    fn push_preserved_run(
        &mut self,
        context: &mut TextContext,
        xml: &str,
        style: u16,
        offset: u32,
    ) {
        let tag = crate::style::preserved::tags(xml)
            .into_iter()
            .find(|t| t.name == "w:sym");
        if let Some(symbol) = tag {
            let code = symbol
                .attr("w:char")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .and_then(char::from_u32);
            if let (Some(ch), Some(font)) = (code, symbol.attr("w:font")) {
                let mut sym_style = self.out.styles[usize::from(style)].clone();
                sym_style.family = font.to_string();
                let index = self.style_index(context.fonts, &sym_style);
                self.push_generated_unit(context.fonts, &ch.to_string(), index, offset);
                return;
            }
        }
        if let Some(kind) = UnsupportedKind::of_preserved_run(xml) {
            context
                .unsupported
                .push(Unsupported::new(kind, &context.location));
        }
        self.push_simple(ClusterKind::Glyphs, style, offset, 1, false);
    }

    fn push_image(&mut self, image: &Image, style: u16, offset: u32, inline_index: usize) {
        let object = InlineObject {
            width: crate::units::twips_to_points(image.width.max(0)),
            height: crate::units::twips_to_points(image.height.max(0)),
            media_id: image.media_id.clone(),
            placement: image.placement.clone(),
            inline_index,
        };
        let index = self.out.objects.len() as u32;
        let floating = matches!(image.placement, Placement::Floating { .. });
        let width = if floating { 0.0 } else { object.width };
        self.out.objects.push(object);
        let kind = if floating {
            ClusterKind::Anchor(index)
        } else {
            ClusterKind::Object(index)
        };
        self.push_simple(kind, style, offset, 1, false);
        if let Some(last) = self.out.clusters.last_mut() {
            last.width = width;
        }
    }

    fn push_simple(
        &mut self,
        kind: ClusterKind,
        style: u16,
        offset: u32,
        units: u16,
        generated: bool,
    ) {
        self.out.clusters.push(Cluster {
            kind,
            offset,
            units,
            style,
            font: 0,
            glyph_start: self.out.glyphs.len() as u32,
            glyph_count: 0,
            width: 0.0,
            break_before: Break::None,
            generated,
            ch: 'x',
            char_start: self.out.chars.len() as u32,
            char_count: 0,
        });
    }

    /// shapes text in one run style, splitting it where the font changes
    /// (fallback for missing characters) or small capitals change size.
    fn push_text(
        &mut self,
        fonts: &mut FontLibrary,
        text: &str,
        style: u16,
        offset: u32,
        generated: bool,
    ) {
        let run = self.out.styles[usize::from(style)].clone();
        let Some(primary) = fonts.resolve(&run.family, run.bold, run.italic) else {
            for (i, _) in text.chars().enumerate() {
                self.push_simple(
                    ClusterKind::Glyphs,
                    style,
                    offset + i as u32,
                    u16::from(!generated),
                    generated,
                );
            }
            return;
        };
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;
        while start < chars.len() {
            let (font, small) = self.font_for(fonts, primary, &run, chars[start]);
            let mut end = start + 1;
            while end < chars.len()
                && self.font_for(fonts, primary, &run, chars[end]) == (font, small)
            {
                end += 1;
            }
            self.push_shaped(
                fonts,
                &chars[start..end],
                &run,
                style,
                font,
                small,
                offset + start as u32,
                generated,
            );
            start = end;
        }
    }

    fn font_for(
        &self,
        fonts: &mut FontLibrary,
        primary: FontId,
        run: &RunStyle,
        ch: char,
    ) -> (FontId, bool) {
        let small = run.caps == Caps::Small && ch.is_lowercase();
        let shown = displayed_char(ch, run.caps);
        let face = &fonts.font(primary).face;
        if face.has_char(shown) || ch.is_whitespace() || ch.is_control() {
            return (primary, small);
        }
        (fonts.fallback(primary, shown).unwrap_or(primary), small)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_shaped(
        &mut self,
        fonts: &FontLibrary,
        chars: &[char],
        run: &RunStyle,
        style: u16,
        font: FontId,
        small: bool,
        offset: u32,
        generated: bool,
    ) {
        let shown: Vec<char> = chars
            .iter()
            .map(|&ch| displayed_char(ch, run.caps))
            .collect();
        let size = if small {
            run.size * SMALL_CAPS_SCALE
        } else {
            run.size
        };
        let options = ShapeOptions {
            size,
            advance_unit: self.flavor.rules().advance_unit,
            kerning: run.kerning(self.flavor),
            ligatures: self.flavor.rules().ligatures,
            right_to_left: false,
        };
        let face = &fonts.font(font).face;
        let glyphs = shape(face, font, &shown, options);
        // group glyphs by the character they start at: one cluster each
        let mut by_char: Vec<Vec<ShapedGlyph>> = vec![Vec::new(); chars.len()];
        for glyph in glyphs {
            let at = glyph.cluster.min(chars.len().saturating_sub(1));
            by_char[at].push(glyph);
        }
        let mut index = 0;
        while index < chars.len() {
            // characters with no glyph of their own joined the one before
            let mut next = index + 1;
            while next < chars.len() && by_char[next].is_empty() {
                next += 1;
            }
            let ch = chars[index];
            let kind = if ch == ' ' {
                ClusterKind::Space
            } else {
                ClusterKind::Glyphs
            };
            let width: f64 = by_char[index].iter().map(|g| g.advance).sum::<f64>()
                + run.letter_spacing * (next - index) as f64;
            let units = if generated { 0 } else { (next - index) as u16 };
            self.out.clusters.push(Cluster {
                kind,
                offset: offset + index as u32,
                units,
                style,
                font,
                glyph_start: self.out.glyphs.len() as u32,
                glyph_count: by_char[index].len() as u16,
                width,
                break_before: Break::None,
                generated,
                ch,
                char_start: self.out.chars.len() as u32,
                char_count: (next - index) as u16,
            });
            self.out.chars.extend_from_slice(&chars[index..next]);
            self.out.glyphs.extend(by_char[index].iter().copied());
            index = next;
        }
    }

    /// marks where lines may start, from the text the clusters stand for.
    fn finish(mut self) -> ParagraphText {
        let mut text = String::new();
        let mut first_char_of_cluster = Vec::with_capacity(self.out.clusters.len());
        for cluster in &self.out.clusters {
            first_char_of_cluster.push(text.chars().count());
            text.push(match cluster.kind {
                ClusterKind::Space => ' ',
                ClusterKind::Tab => '\t',
                ClusterKind::LineBreak | ClusterKind::PageBreak | ClusterKind::ColumnBreak => '\n',
                ClusterKind::Object(_) => '\u{fffc}',
                // anchors sit between characters without changing breaks
                ClusterKind::Anchor(_) => '\u{2060}',
                ClusterKind::Glyphs => cluster.ch,
            });
        }
        let opportunities = break_opportunities(&text);
        for (cluster, at) in self.out.clusters.iter_mut().zip(first_char_of_cluster) {
            cluster.break_before = opportunities.get(at).copied().unwrap_or(Break::None);
        }
        self.out
    }
}

/// the character drawn for `ch` under a caps setting.
fn displayed_char(ch: char, caps: Caps) -> char {
    match caps {
        Caps::None => ch,
        Caps::All | Caps::Small => ch.to_uppercase().next().unwrap_or(ch),
    }
}

/// how many model offset units an inline covers (see the module notes).
pub fn units_of(inline: &Inline) -> u32 {
    match inline {
        Inline::Text { text, .. } => text.chars().count() as u32,
        Inline::Tab { .. }
        | Inline::Break { .. }
        | Inline::Image(_)
        | Inline::NoteReference { .. }
        | Inline::Field(_)
        | Inline::PreservedRun { .. } => 1,
        Inline::BookmarkStart { .. }
        | Inline::BookmarkEnd { .. }
        | Inline::CommentReference { .. }
        | Inline::Preserved { .. } => 0,
    }
}

/// a run style for text that has no paragraph context (labels built from
/// a list level's run properties).
pub fn style_from_run_props(props: &letters_office::model::RunProps, flavor: Flavor) -> RunStyle {
    run_style_from_props(props, flavor)
}
