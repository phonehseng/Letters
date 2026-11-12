//! paragraph (`w:pPr`) and run (`w:rPr`) properties.
//!
//! the rule everywhere: an element is read into the model only when every
//! attribute on it is understood. otherwise the whole element is kept as
//! preserved xml, so nothing the model cannot represent is thrown away.

use crate::model::{
    Alignment, Attribute, FontRef, LineRule, LineSpacing, ListRef, ParagraphProps, RunProps,
    Script, TabKind, TabLeader, TabStop, ThemeFonts, Twips,
};
use crate::units::parse_length;
use crate::xml::{Element, XmlDocument};

/// a word on/off property: present means on unless its value says off.
pub fn on_off(element: &Element) -> bool {
    !matches!(
        element.attr("w:val"),
        Some("0") | Some("false") | Some("off")
    )
}

/// a length attribute in twips (word may also write "1in" and the like).
pub fn twips(element: &Element, name: &str) -> Option<Twips> {
    element.attr(name).and_then(parse_length)
}

pub fn number(element: &Element, name: &str) -> Option<u32> {
    element
        .attr(name)
        .and_then(|value| value.trim().parse().ok())
}

/// true when every attribute of `element` is in `known`.
pub fn only_attributes(element: &Element, known: &[&str]) -> bool {
    element
        .attributes
        .iter()
        .all(|(name, _)| known.contains(&name.as_str()))
}

/// what a `w:pPr` holds, split into the parts the paragraph reader needs.
#[derive(Default)]
pub struct ParagraphPropsRead<'a> {
    pub style_id: Option<String>,
    /// includes the properties kept as xml, in `props.preserved`
    pub props: ParagraphProps,
    /// the section properties that end a section at this paragraph
    pub section: Option<&'a Element>,
}

pub fn read_paragraph_props<'a>(ppr: &'a Element, xml: &XmlDocument) -> ParagraphPropsRead<'a> {
    let mut read = ParagraphPropsRead::default();
    for child in ppr.elements() {
        let understood = match child.name.as_str() {
            "w:pStyle" => {
                read.style_id = child.attr("w:val").map(str::to_string);
                read.style_id.is_some()
            }
            "w:keepNext" => set(&mut read.props.keep_with_next, on_off(child)),
            "w:keepLines" => set(&mut read.props.keep_lines_together, on_off(child)),
            "w:pageBreakBefore" => set(&mut read.props.page_break_before, on_off(child)),
            "w:numPr" => read_list(child, &mut read.props),
            "w:tabs" => read_tabs(child, &mut read.props),
            "w:spacing" => read_spacing(child, &mut read.props),
            "w:ind" => read_indent(child, &mut read.props),
            "w:jc" => match child.attr("w:val").and_then(alignment) {
                Some(value) if only_attributes(child, &["w:val"]) => {
                    read.props.alignment = Some(value);
                    true
                }
                _ => false,
            },
            "w:outlineLvl" => match number(child, "w:val") {
                Some(level) if level < 10 => {
                    read.props.outline_level = Some(level as u8);
                    true
                }
                _ => false,
            },
            "w:sectPr" => {
                read.section = Some(child);
                true
            }
            _ => false,
        };
        if !understood {
            read.props.preserved.push(xml.raw(child).to_string());
        }
    }
    read
}

fn set(slot: &mut Option<bool>, value: bool) -> bool {
    *slot = Some(value);
    true
}

pub fn alignment(value: &str) -> Option<Alignment> {
    match value {
        "left" | "start" => Some(Alignment::Left),
        "center" => Some(Alignment::Center),
        "right" | "end" => Some(Alignment::Right),
        "both" | "justify" => Some(Alignment::Justify),
        _ => None,
    }
}

fn read_list(numpr: &Element, props: &mut ParagraphProps) -> bool {
    let level = numpr
        .child("w:ilvl")
        .and_then(|el| number(el, "w:val"))
        .unwrap_or(0);
    let Some(list_id) = numpr.child_val("w:numId") else {
        return false;
    };
    let known_children = numpr
        .elements()
        .all(|child| child.name == "w:ilvl" || child.name == "w:numId");
    if !known_children || level > 8 {
        return false;
    }
    props.list = Some(ListRef {
        list_id: list_id.to_string(),
        level: level as u8,
    });
    true
}

fn read_tabs(tabs: &Element, props: &mut ParagraphProps) -> bool {
    let mut stops = Vec::new();
    for tab in tabs.elements() {
        let kind = match tab.attr("w:val") {
            Some("left") | Some("start") => TabKind::Left,
            Some("center") => TabKind::Center,
            Some("right") | Some("end") => TabKind::Right,
            Some("decimal") => TabKind::Decimal,
            Some("bar") => TabKind::Bar,
            Some("clear") => TabKind::Clear,
            _ => return false,
        };
        let leader = match tab.attr("w:leader") {
            None | Some("none") => TabLeader::None,
            Some("dot") => TabLeader::Dot,
            Some("hyphen") => TabLeader::Hyphen,
            Some("underscore") => TabLeader::Underscore,
            Some("middleDot") => TabLeader::MiddleDot,
            Some("heavy") => TabLeader::Heavy,
            Some(_) => return false,
        };
        let Some(position) = twips(tab, "w:pos") else {
            return false;
        };
        stops.push(TabStop {
            position,
            kind,
            leader,
        });
    }
    props.tabs = stops;
    true
}

fn read_spacing(spacing: &Element, props: &mut ParagraphProps) -> bool {
    // the "lines" and "auto spacing" variants are left to preservation
    if !only_attributes(spacing, &["w:before", "w:after", "w:line", "w:lineRule"]) {
        return false;
    }
    props.spacing.before = twips(spacing, "w:before");
    props.spacing.after = twips(spacing, "w:after");
    if let Some(value) = spacing.attr("w:line").and_then(|v| v.trim().parse().ok()) {
        let rule = match spacing.attr("w:lineRule") {
            Some("exact") => LineRule::Exact,
            Some("atLeast") => LineRule::AtLeast,
            _ => LineRule::Auto,
        };
        props.spacing.line = Some(LineSpacing { rule, value });
    }
    true
}

fn read_indent(ind: &Element, props: &mut ParagraphProps) -> bool {
    if !only_attributes(
        ind,
        &[
            "w:left",
            "w:start",
            "w:right",
            "w:end",
            "w:firstLine",
            "w:hanging",
        ],
    ) {
        return false;
    }
    props.indent.left = twips(ind, "w:left").or_else(|| twips(ind, "w:start"));
    props.indent.right = twips(ind, "w:right").or_else(|| twips(ind, "w:end"));
    props.indent.first_line = twips(ind, "w:firstLine");
    props.indent.hanging = twips(ind, "w:hanging");
    true
}

/// reads a `w:rPr`. `theme` resolves fonts given as theme slots.
pub fn read_run_props(rpr: &Element, xml: &XmlDocument, theme: &ThemeFonts) -> RunProps {
    let mut props = RunProps::default();
    for child in rpr.elements() {
        if !read_run_property(child, theme, &mut props) {
            props.preserved.push(xml.raw(child).to_string());
        }
    }
    props
}

/// reads one run property into `props`; false when it is not understood.
fn read_run_property(child: &Element, theme: &ThemeFonts, props: &mut RunProps) -> bool {
    let toggle_ok = only_attributes(child, &["w:val"]);
    match child.name.as_str() {
        "w:rStyle" => {
            props.style_id = child.attr("w:val").map(str::to_string);
            props.style_id.is_some()
        }
        "w:rFonts" => read_fonts(child, theme, props),
        "w:b" if toggle_ok => set(&mut props.bold, on_off(child)),
        "w:i" if toggle_ok => set(&mut props.italic, on_off(child)),
        "w:caps" if toggle_ok => set(&mut props.all_caps, on_off(child)),
        "w:smallCaps" if toggle_ok => set(&mut props.small_caps, on_off(child)),
        "w:strike" if toggle_ok => set(&mut props.strike, on_off(child)),
        "w:dstrike" if toggle_ok => set(&mut props.double_strike, on_off(child)),
        "w:vanish" if toggle_ok => set(&mut props.hidden, on_off(child)),
        "w:color" if toggle_ok => match child.attr("w:val") {
            Some(hex) if is_hex_colour(hex) => {
                props.color = Some(hex.to_ascii_uppercase());
                true
            }
            _ => false,
        },
        "w:sz" if toggle_ok => {
            props.size = number(child, "w:val");
            props.size.is_some()
        }
        "w:highlight" if toggle_ok => {
            props.highlight = child.attr("w:val").map(str::to_string);
            props.highlight.is_some()
        }
        "w:u" if toggle_ok => {
            props.underline = Some(child.attr("w:val").unwrap_or("single").to_string());
            true
        }
        "w:vertAlign" if toggle_ok => {
            props.script = match child.attr("w:val") {
                Some("superscript") => Some(Script::Superscript),
                Some("subscript") => Some(Script::Subscript),
                Some("baseline") => Some(Script::Baseline),
                _ => None,
            };
            props.script.is_some()
        }
        "w:lang" if toggle_ok => {
            props.language = child.attr("w:val").map(str::to_string);
            props.language.is_some()
        }
        _ => false,
    }
}

fn read_fonts(fonts: &Element, theme: &ThemeFonts, props: &mut RunProps) -> bool {
    let slot = fonts
        .attr("w:asciiTheme")
        .or_else(|| fonts.attr("w:hAnsiTheme"))
        .map(str::to_string);
    let named = fonts.attr("w:ascii").or_else(|| fonts.attr("w:hAnsi"));
    let name = match (&slot, named) {
        (Some(slot), named) => theme.resolve(slot).or_else(|| named.map(str::to_string)),
        (None, Some(named)) => Some(named.to_string()),
        (None, None) => None,
    };
    let Some(name) = name else {
        return false;
    };
    // everything except the latin font and its theme slot rides along as is
    let latin = ["w:ascii", "w:hAnsi", "w:asciiTheme", "w:hAnsiTheme"];
    let extra = fonts
        .attributes
        .iter()
        .filter(|(key, _)| !latin.contains(&key.as_str()))
        .map(|(key, value)| Attribute::new(key, value))
        .collect();
    props.font = Some(FontRef {
        name,
        theme: slot,
        extra,
    });
    true
}

pub fn is_hex_colour(value: &str) -> bool {
    value.len() == 6 && value.chars().all(|c| c.is_ascii_hexdigit())
}
