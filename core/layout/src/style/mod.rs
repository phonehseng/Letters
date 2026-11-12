//! the formatting that really applies, resolved once per paragraph and run
//! and turned into points: what every later stage works from.
//!
//! style inheritance itself is the office crate's (`styles::resolved_*`);
//! this adds the defaults word assumes when a file says nothing, and the
//! properties the model keeps only as preserved xml.

pub mod preserved;

use letters_office::model::{
    Alignment, Document, LineRule, ListRef, Mark, Paragraph, ParagraphProps, RunProps, Script,
    TabKind, TabLeader,
};
use letters_office::styles;

use crate::flavor::Flavor;
use crate::units::{half_points_to_points, twips_to_points};

/// a font family word uses when neither the run, its styles nor the
/// document defaults name one (ecma-376 leaves it to the application;
/// word 16 shows times new roman 10 pt for an empty docDefaults).
pub const DEFAULT_FAMILY: &str = "Times New Roman";
pub const DEFAULT_SIZE: f64 = 10.0;

/// superscript and subscript text is drawn smaller and shifted. word
/// shrinks to 2/3 of the size and raises superscripts by 1/3 of it,
/// lowers subscripts by a sixth (measured on the footnote references of
/// the corpus, see the report); libreoffice's defaults are 58 % and 33 %.
pub fn script_scale(flavor: Flavor) -> (f64, f64, f64) {
    match flavor {
        Flavor::Word => (2.0 / 3.0, 1.0 / 3.0, 1.0 / 6.0),
        Flavor::LibreOffice => (0.58, 0.33, 0.08),
    }
}

/// small capitals are capitals at this share of the size (both oracles
/// synthesise them when the font has no small caps of its own).
pub const SMALL_CAPS_SCALE: f64 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caps {
    None,
    All,
    Small,
}

/// one run's formatting, ready for shaping.
#[derive(Clone, Debug, PartialEq)]
pub struct RunStyle {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// the size glyphs are shaped at, in points (after super/subscript)
    pub size: f64,
    /// the size the run asked for, which decides the line height
    pub nominal_size: f64,
    /// how far the baseline moves up (negative: down), in points
    pub baseline_shift: f64,
    /// the part of the shift that grows the line: raised or lowered text
    /// (w:position) does, superscript and subscript do not (a footnote
    /// reference leaves word's 12 pt double-spaced lines as they are,
    /// footnotes in the corpus)
    pub line_shift: f64,
    pub color: Option<[u8; 3]>,
    pub underline: Option<String>,
    pub strike: bool,
    pub double_strike: bool,
    pub caps: Caps,
    pub hidden: bool,
    /// extra space after every character (w:spacing), in points
    pub letter_spacing: f64,
    /// kerning applies at and above this size (w:kern, half points);
    /// none means the flavor's default
    pub kern_from: Option<f64>,
    pub highlight: Option<String>,
}

impl RunStyle {
    /// whether pair kerning applies to this run in this flavor.
    pub fn kerning(&self, flavor: Flavor) -> bool {
        match self.kern_from {
            Some(threshold) => self.nominal_size >= threshold,
            None => flavor.rules().kerning,
        }
    }
}

/// formatting a table style gives the paragraphs and text in its cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableStyleProps {
    pub paragraph: ParagraphProps,
    pub run: RunProps,
}

/// resolves the run formatting of text with `marks` in `paragraph`.
pub fn run_style(
    document: &Document,
    paragraph: &Paragraph,
    marks: &[Mark],
    flavor: Flavor,
) -> RunStyle {
    run_style_in(document, paragraph, marks, None, flavor)
}

/// the same inside a table: the table style's run formatting comes after
/// the document defaults and before the paragraph style (ecma-376 part 1,
/// 17.7.2, the style hierarchy).
pub fn run_style_in(
    document: &Document,
    paragraph: &Paragraph,
    marks: &[Mark],
    table: Option<&TableStyleProps>,
    flavor: Flavor,
) -> RunStyle {
    let props = match table {
        None => styles::resolved_run_props(document, paragraph.style_id.as_deref(), marks),
        Some(table) => resolve_run_props(document, paragraph, marks, &table.run),
    };
    run_style_from_props(&props, flavor)
}

fn resolve_run_props(
    document: &Document,
    paragraph: &Paragraph,
    marks: &[Mark],
    table: &RunProps,
) -> RunProps {
    let mut props = document.styles.defaults.run.clone();
    props.merge(table);
    if let Some(id) = styles::effective_style_id(document, paragraph) {
        for style in styles::style_chain(document, &id).iter().rev() {
            props.merge(&style.run);
        }
    }
    let direct = RunProps::from_marks(marks);
    if let Some(id) = &direct.style_id {
        for style in styles::style_chain(document, id).iter().rev() {
            props.merge(&style.run);
        }
    }
    props.merge(&direct);
    props.style_id = direct.style_id;
    props
}

/// the id of a table's style: its own, or the document's default table
/// style ("Table Normal" in files word writes).
pub fn table_style_id(document: &Document, style_id: Option<&str>) -> Option<String> {
    style_id.map(str::to_string).or_else(|| {
        document
            .styles
            .styles
            .iter()
            .find(|style| style.kind == letters_office::model::StyleKind::Table && style.is_default)
            .map(|style| style.id.clone())
    })
}

/// a table style's paragraph and run formatting, with its base styles.
pub fn table_style_props(document: &Document, style_id: Option<&str>) -> TableStyleProps {
    let mut props = TableStyleProps::default();
    if let Some(id) = table_style_id(document, style_id) {
        for style in styles::style_chain(document, &id).iter().rev() {
            props.paragraph.merge(&style.paragraph);
            props.run.merge(&style.run);
        }
    }
    props
}

/// turns resolved run properties into a run style.
pub fn run_style_from_props(props: &RunProps, flavor: Flavor) -> RunStyle {
    let nominal_size = props
        .size
        .map(half_points_to_points)
        .unwrap_or(DEFAULT_SIZE);
    let (scale, raise, lower) = script_scale(flavor);
    let (size, baseline_shift) = match props.script {
        Some(Script::Superscript) => (nominal_size * scale, nominal_size * raise),
        Some(Script::Subscript) => (nominal_size * scale, -nominal_size * lower),
        _ => (nominal_size, 0.0),
    };
    let position = preserved::number(&props.preserved, "w:position")
        .map(|half_points| f64::from(half_points) / 2.0)
        .unwrap_or(0.0);
    let caps = if props.all_caps == Some(true) {
        Caps::All
    } else if props.small_caps == Some(true) {
        Caps::Small
    } else {
        Caps::None
    };
    RunStyle {
        family: props
            .font
            .as_ref()
            .map(|font| font.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| DEFAULT_FAMILY.to_string()),
        bold: props.bold == Some(true),
        italic: props.italic == Some(true),
        size,
        nominal_size,
        baseline_shift: baseline_shift + position,
        line_shift: position,
        color: props.color.as_deref().and_then(parse_hex_color),
        underline: props.underline.clone().filter(|style| style != "none"),
        strike: props.strike == Some(true),
        double_strike: props.double_strike == Some(true),
        caps,
        hidden: props.hidden == Some(true),
        letter_spacing: preserved::number(&props.preserved, "w:spacing")
            .map(twips_to_points)
            .unwrap_or(0.0),
        kern_from: preserved::number(&props.preserved, "w:kern")
            .map(|half_points| f64::from(half_points.max(0)) / 2.0),
        highlight: props.highlight.clone(),
    }
}

pub fn parse_hex_color(hex: &str) -> Option<[u8; 3]> {
    if hex.len() != 6 || hex.eq_ignore_ascii_case("auto") {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some([(value >> 16) as u8, (value >> 8) as u8, value as u8])
}

/// a tab stop in points from the text margin (where word measures them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tab {
    pub position: f64,
    pub kind: TabKind,
    pub leader: TabLeader,
}

/// line spacing in the units layout uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineSpacingRule {
    /// a multiple of the font's line height (1.0 is single)
    Multiple(f64),
    /// exactly this height in points, whatever the fonts
    Exact(f64),
    /// at least this height in points
    AtLeast(f64),
}

/// one paragraph's formatting, resolved and in points.
#[derive(Clone, Debug, PartialEq)]
pub struct ParaStyle {
    pub style_id: Option<String>,
    pub alignment: Alignment,
    pub left: f64,
    pub right: f64,
    /// first line relative to `left`: positive indents, negative hangs
    pub first_line: f64,
    pub space_before: f64,
    pub space_after: f64,
    pub line: LineSpacingRule,
    pub keep_with_next: bool,
    pub keep_together: bool,
    pub page_break_before: bool,
    pub widow_control: bool,
    pub contextual_spacing: bool,
    pub tabs: Vec<Tab>,
    pub default_tab: f64,
    pub list: Option<ListRef>,
    /// the paragraph mark's own formatting: the height of an empty line
    pub mark: RunStyle,
    /// right-to-left paragraph (w:bidi): detected, laid out left to right
    pub right_to_left: bool,
}

/// resolves a paragraph's formatting.
pub fn para_style(document: &Document, paragraph: &Paragraph, flavor: Flavor) -> ParaStyle {
    para_style_in(document, paragraph, None, flavor)
}

/// the same inside a table, whose style comes between the document
/// defaults and the paragraph style (word's "table grid" style sets
/// spacing after 0 and single line spacing for its cells this way).
pub fn para_style_in(
    document: &Document,
    paragraph: &Paragraph,
    table: Option<&TableStyleProps>,
    flavor: Flavor,
) -> ParaStyle {
    let props = match table {
        None => styles::resolved_paragraph_props(document, paragraph),
        Some(table) => resolve_paragraph_props(document, paragraph, &table.paragraph),
    };
    para_style_from_props(document, paragraph, &props, flavor)
}

fn resolve_paragraph_props(
    document: &Document,
    paragraph: &Paragraph,
    table: &ParagraphProps,
) -> ParagraphProps {
    let mut props = document.styles.defaults.paragraph.clone();
    props.merge(table);
    if let Some(id) = styles::effective_style_id(document, paragraph) {
        for style in styles::style_chain(document, &id).iter().rev() {
            props.merge(&style.paragraph);
        }
    }
    let list = paragraph.props.list.clone().or_else(|| props.list.clone());
    if let Some(list) = list {
        if let Some(level) = document.numbering.level(&list.list_id, list.level) {
            let mut level_props = level.paragraph.clone();
            level_props.list = None;
            props.merge(&level_props);
        }
    }
    props.merge(&paragraph.props);
    props
}

fn para_style_from_props(
    document: &Document,
    paragraph: &Paragraph,
    props: &ParagraphProps,
    flavor: Flavor,
) -> ParaStyle {
    let rules = flavor.rules();
    let indent = &props.indent;
    let line = match props.spacing.line {
        Some(spacing) => match spacing.rule {
            LineRule::Auto => LineSpacingRule::Multiple(f64::from(spacing.value.max(1)) / 240.0),
            LineRule::Exact => LineSpacingRule::Exact(twips_to_points(spacing.value.abs())),
            LineRule::AtLeast => LineSpacingRule::AtLeast(twips_to_points(spacing.value.max(0))),
        },
        None => LineSpacingRule::Multiple(1.0),
    };
    let widow_control = preserved::find(&props.preserved, "w:widowControl")
        .map(|tag| tag.on())
        .unwrap_or(rules.widow_control_default);
    let contextual_spacing = preserved::find(&props.preserved, "w:contextualSpacing")
        .map(|tag| tag.on())
        .unwrap_or(false);
    let right_to_left = preserved::find(&props.preserved, "w:bidi")
        .map(|tag| tag.on())
        .unwrap_or(false);
    ParaStyle {
        style_id: styles::effective_style_id(document, paragraph),
        alignment: props.alignment.unwrap_or(Alignment::Left),
        left: twips_to_points(indent.left.unwrap_or(0)),
        right: twips_to_points(indent.right.unwrap_or(0)),
        first_line: twips_to_points(indent.first_line_offset()),
        space_before: twips_to_points(props.spacing.before.unwrap_or(0).max(0)),
        space_after: twips_to_points(props.spacing.after.unwrap_or(0).max(0)),
        line,
        keep_with_next: props.keep_with_next == Some(true),
        keep_together: props.keep_lines_together == Some(true),
        page_break_before: props.page_break_before == Some(true),
        widow_control,
        contextual_spacing,
        tabs: props
            .tabs
            .iter()
            .filter(|stop| stop.kind != TabKind::Clear)
            .map(|stop| Tab {
                position: twips_to_points(stop.position),
                kind: stop.kind,
                leader: stop.leader,
            })
            .collect(),
        default_tab: twips_to_points(document.settings.default_tab_stop.unwrap_or(720).max(1)),
        list: props.list.clone().filter(|list| list.list_id != "0"),
        mark: mark_style(document, paragraph, props, flavor),
        right_to_left,
    }
}

/// the paragraph mark's formatting: the style's run properties plus the
/// mark's own `w:rPr`, which the model keeps as preserved xml.
fn mark_style(
    document: &Document,
    paragraph: &Paragraph,
    props: &ParagraphProps,
    flavor: Flavor,
) -> RunStyle {
    let mut run = styles::resolved_run_props(document, paragraph.style_id.as_deref(), &[]);
    if let Some(size) = preserved::number(&props.preserved, "w:sz") {
        run.size = Some(size.max(1) as u32);
    }
    if let Some(fonts) = preserved::find(&props.preserved, "w:rFonts") {
        if let Some(name) = fonts.attr("w:ascii").or(fonts.attr("w:hAnsi")) {
            run.font = Some(letters_office::model::FontRef {
                name: name.to_string(),
                theme: None,
                extra: Vec::new(),
            });
        }
    }
    run_style_from_props(&run, flavor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Block, LineSpacing};

    #[test]
    fn defaults_fill_what_the_file_leaves_out() {
        let document = Document::new();
        let paragraph = Paragraph::with_text("x");
        let style = para_style(&document, &paragraph, Flavor::Word);
        assert_eq!(style.line, LineSpacingRule::Multiple(1.0));
        assert!(style.widow_control);
        assert_eq!(style.default_tab, 36.0);
        let run = run_style(&document, &paragraph, &[], Flavor::Word);
        assert_eq!(run.family, DEFAULT_FAMILY);
        assert_eq!(run.size, DEFAULT_SIZE);
    }

    #[test]
    fn preserved_properties_are_read() {
        let mut document = Document::new();
        let mut paragraph = Paragraph::with_text("x");
        paragraph.props.preserved = vec![
            r#"<w:widowControl w:val="0"/>"#.into(),
            "<w:contextualSpacing/>".into(),
            r#"<w:rPr><w:sz w:val="40"/></w:rPr>"#.into(),
        ];
        paragraph.props.spacing.line = Some(LineSpacing {
            rule: LineRule::Exact,
            value: 300,
        });
        document.blocks.push(Block::Paragraph(paragraph.clone()));
        let style = para_style(&document, &paragraph, Flavor::Word);
        assert!(!style.widow_control);
        assert!(style.contextual_spacing);
        assert_eq!(style.mark.size, 20.0);
        assert_eq!(style.line, LineSpacingRule::Exact(15.0));
    }

    #[test]
    fn superscript_shrinks_and_rises() {
        let props = RunProps {
            size: Some(24),
            script: Some(Script::Superscript),
            ..RunProps::default()
        };
        let style = run_style_from_props(&props, Flavor::Word);
        assert!((style.size - 8.0).abs() < 1e-9);
        assert!((style.baseline_shift - 4.0).abs() < 1e-9);
        assert_eq!(style.nominal_size, 12.0);
    }
}
