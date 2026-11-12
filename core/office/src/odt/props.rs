//! odt style properties (`style:paragraph-properties`,
//! `style:text-properties`...) to and from the model's props.
//!
//! odt keeps formatting in styles, also for direct formatting (as
//! "automatic" styles). attributes the model does not have are kept in
//! the props' `preserved` list as `name="value"` text, and elements as
//! xml, so they are written back into the style the writer makes.

use std::collections::HashMap;

use crate::model::{
    Alignment, Border, CellBorders, FontRef, LineRule, LineSpacing, ParagraphProps, RunProps,
    Script, TabKind, TabLeader, TabStop, VerticalAlign, Wrap,
};
use crate::units::{parse_length, points, twips_to_inches};
use crate::xml::{escape_attribute, Element, XmlDocument};

/// everything one odt style says, in model terms.
#[derive(Clone, Debug, Default)]
pub struct OdtStyle {
    pub name: String,
    pub display_name: Option<String>,
    pub family: String,
    pub parent: Option<String>,
    pub next: Option<String>,
    pub master_page: Option<String>,
    pub outline_level: Option<u8>,
    pub paragraph: ParagraphProps,
    pub run: RunProps,
    pub break_after_page: bool,
    pub cell_borders: Option<CellBorders>,
    pub background: Option<String>,
    pub vertical_align: Option<VerticalAlign>,
    pub width: Option<i32>,
    pub alignment: Option<Alignment>,
    pub wrap: Option<Wrap>,
}

/// reads a `style:style` (or `style:default-style`). `fonts` maps font
/// face names to their family names.
pub fn read_style(
    element: &Element,
    xml: &XmlDocument,
    fonts: &HashMap<String, String>,
) -> OdtStyle {
    let mut style = OdtStyle {
        name: element.attr("style:name").unwrap_or_default().to_string(),
        display_name: element.attr("style:display-name").map(str::to_string),
        family: element.attr("style:family").unwrap_or_default().to_string(),
        parent: element.attr("style:parent-style-name").map(str::to_string),
        next: element.attr("style:next-style-name").map(str::to_string),
        master_page: element
            .attr("style:master-page-name")
            .filter(|name| !name.is_empty())
            .map(str::to_string),
        outline_level: element
            .attr("style:default-outline-level")
            .and_then(|level| level.parse::<u8>().ok())
            .filter(|level| (1..=10).contains(level))
            .map(|level| level - 1),
        ..Default::default()
    };
    for child in element.elements() {
        match child.name.as_str() {
            "style:paragraph-properties" => read_paragraph_properties(child, xml, &mut style),
            "style:text-properties" => read_text_properties(child, fonts, &mut style.run),
            "style:table-cell-properties" => read_cell_properties(child, &mut style),
            "style:table-properties" => {
                style.width = child.attr("style:width").and_then(parse_length);
                style.alignment = child.attr("table:align").and_then(odt_alignment);
            }
            "style:table-column-properties" => {
                style.width = child.attr("style:column-width").and_then(parse_length);
            }
            "style:graphic-properties" => style.wrap = graphic_wrap(child),
            _ => {}
        }
    }
    style.paragraph.outline_level = style.outline_level;
    style
}

fn odt_alignment(value: &str) -> Option<Alignment> {
    match value {
        "start" | "left" => Some(Alignment::Left),
        "center" => Some(Alignment::Center),
        "end" | "right" => Some(Alignment::Right),
        "justify" => Some(Alignment::Justify),
        _ => None,
    }
}

/// attributes of paragraph properties the reader turns into the model.
const PARAGRAPH_KNOWN: &[&str] = &[
    "fo:text-align",
    "fo:margin-left",
    "fo:margin-right",
    "fo:text-indent",
    "fo:margin-top",
    "fo:margin-bottom",
    "fo:line-height",
    "style:line-height-at-least",
    "fo:keep-with-next",
    "fo:keep-together",
    "fo:break-before",
    "fo:break-after",
];

fn read_paragraph_properties(element: &Element, xml: &XmlDocument, style: &mut OdtStyle) {
    let props = &mut style.paragraph;
    for (name, value) in &element.attributes {
        if !PARAGRAPH_KNOWN.contains(&name.as_str()) {
            props
                .preserved
                .push(format!("{name}=\"{}\"", escape_attribute(value)));
        }
    }
    props.alignment = element.attr("fo:text-align").and_then(odt_alignment);
    props.indent.left = element.attr("fo:margin-left").and_then(parse_length);
    props.indent.right = element.attr("fo:margin-right").and_then(parse_length);
    if let Some(indent) = element.attr("fo:text-indent").and_then(parse_length) {
        if indent < 0 {
            props.indent.hanging = Some(-indent);
        } else {
            props.indent.first_line = Some(indent);
        }
    }
    props.spacing.before = element.attr("fo:margin-top").and_then(parse_length);
    props.spacing.after = element.attr("fo:margin-bottom").and_then(parse_length);
    props.spacing.line = line_spacing(element);
    props.keep_with_next = element.attr("fo:keep-with-next").map(|v| v == "always");
    props.keep_lines_together = element.attr("fo:keep-together").map(|v| v == "always");
    props.page_break_before = element.attr("fo:break-before").map(|v| v == "page");
    style.break_after_page = element.attr("fo:break-after") == Some("page");
    for child in element.elements() {
        if child.name == "style:tab-stops" {
            props.tabs = read_tab_stops(child, props.indent.left.unwrap_or(0));
        } else {
            props.preserved.push(xml.raw(child).to_string());
        }
    }
}

fn line_spacing(element: &Element) -> Option<LineSpacing> {
    if let Some(at_least) = element
        .attr("style:line-height-at-least")
        .and_then(parse_length)
    {
        return Some(LineSpacing {
            rule: LineRule::AtLeast,
            value: at_least,
        });
    }
    let value = element.attr("fo:line-height")?;
    if let Some(percent) = value.strip_suffix('%') {
        let percent: f64 = percent.trim().parse().ok()?;
        return Some(LineSpacing {
            rule: LineRule::Auto,
            value: (percent * 2.4).round() as i32,
        });
    }
    parse_length(value).map(|exact| LineSpacing {
        rule: LineRule::Exact,
        value: exact,
    })
}

/// odt tab positions count from the paragraph's left indent; the model
/// counts from the margin, as word does.
fn read_tab_stops(element: &Element, left_indent: i32) -> Vec<TabStop> {
    element
        .children_named("style:tab-stop")
        .filter_map(|stop| {
            let position = stop.attr("style:position").and_then(parse_length)? + left_indent;
            let kind = match stop.attr("style:type") {
                Some("center") => TabKind::Center,
                Some("right") => TabKind::Right,
                Some("char") => TabKind::Decimal,
                _ => TabKind::Left,
            };
            let leader = match (
                stop.attr("style:leader-text"),
                stop.attr("style:leader-style"),
            ) {
                (Some("."), _) | (_, Some("dotted")) => TabLeader::Dot,
                (Some("-"), _) | (_, Some("dash")) => TabLeader::Hyphen,
                (Some("_"), _) | (_, Some("solid")) => TabLeader::Underscore,
                _ => TabLeader::None,
            };
            Some(TabStop {
                position,
                kind,
                leader,
            })
        })
        .collect()
}

const TEXT_KNOWN: &[&str] = &[
    "fo:font-weight",
    "fo:font-style",
    "style:text-underline-style",
    "style:text-underline-type",
    "style:text-underline-width",
    "style:text-underline-color",
    "style:text-underline-mode",
    "style:text-line-through-style",
    "style:text-line-through-type",
    "style:text-position",
    "fo:font-variant",
    "fo:text-transform",
    "style:font-name",
    "fo:font-family",
    "fo:font-size",
    "fo:color",
    "fo:background-color",
    "fo:language",
    "fo:country",
    "text:display",
];

pub fn read_text_properties(
    element: &Element,
    fonts: &HashMap<String, String>,
    run: &mut RunProps,
) {
    for (name, value) in &element.attributes {
        if !TEXT_KNOWN.contains(&name.as_str()) {
            run.preserved
                .push(format!("{name}=\"{}\"", escape_attribute(value)));
        }
    }
    let attr = |name: &str| element.attr(name);
    run.bold =
        attr("fo:font-weight").map(|v| v == "bold" || v.parse::<u32>().is_ok_and(|w| w >= 600));
    run.italic = attr("fo:font-style").map(|v| v == "italic" || v == "oblique");
    run.underline = attr("style:text-underline-style")
        .map(|style| underline(style, attr("style:text-underline-type")));
    match (
        attr("style:text-line-through-style"),
        attr("style:text-line-through-type"),
    ) {
        (Some("none"), _) => run.strike = Some(false),
        (Some(_), Some("double")) => run.double_strike = Some(true),
        (Some(_), _) => run.strike = Some(true),
        _ => {}
    }
    run.script = attr("style:text-position").and_then(text_position);
    run.small_caps = attr("fo:font-variant").map(|v| v == "small-caps");
    run.all_caps = attr("fo:text-transform").map(|v| v == "uppercase");
    let family = attr("style:font-name")
        .map(|name| fonts.get(name).cloned().unwrap_or_else(|| name.to_string()))
        .or_else(|| attr("fo:font-family").map(str::to_string));
    run.font = family.map(|name| FontRef {
        name: unquote(&name),
        theme: None,
        extra: Vec::new(),
    });
    match attr("fo:font-size") {
        Some(size) if size.ends_with("pt") => {
            run.size = parse_length(size).map(|twips| (twips / 10).max(1) as u32);
        }
        Some(other) => run.preserved.push(format!("fo:font-size=\"{other}\"")),
        None => {}
    }
    run.color = attr("fo:color").and_then(hex_colour);
    run.highlight = attr("fo:background-color").and_then(hex_colour);
    run.language = match (attr("fo:language"), attr("fo:country")) {
        (Some(language), Some(country)) if language != "none" && country != "none" => {
            Some(format!("{language}-{country}"))
        }
        (Some(language), _) if language != "none" => Some(language.to_string()),
        _ => None,
    };
    run.hidden = attr("text:display").map(|v| v == "none");
}

pub fn unquote(name: &str) -> String {
    name.trim()
        .trim_matches(|c| c == '\'' || c == '"')
        .to_string()
}

fn underline(style: &str, kind: Option<&str>) -> String {
    match (style, kind) {
        ("none", _) => "none".into(),
        (_, Some("double")) => "double".into(),
        ("wave", _) => "wave".into(),
        ("dotted", _) => "dotted".into(),
        ("dash", _) | ("long-dash", _) => "dash".into(),
        ("dot-dash", _) => "dotDash".into(),
        ("dot-dot-dash", _) => "dotDotDash".into(),
        _ => "single".into(),
    }
}

fn text_position(value: &str) -> Option<Script> {
    let first = value.split_whitespace().next()?;
    match first {
        "super" => Some(Script::Superscript),
        "sub" => Some(Script::Subscript),
        number => {
            let percent: f64 = number.trim_end_matches('%').parse().ok()?;
            Some(if percent > 0.0 {
                Script::Superscript
            } else if percent < 0.0 {
                Script::Subscript
            } else {
                Script::Baseline
            })
        }
    }
}

fn hex_colour(value: &str) -> Option<String> {
    let hex = value.strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| hex.to_ascii_uppercase())
}

fn read_cell_properties(element: &Element, style: &mut OdtStyle) {
    style.background = element.attr("fo:background-color").and_then(hex_colour);
    style.vertical_align = match element.attr("style:vertical-align") {
        Some("top") => Some(VerticalAlign::Top),
        Some("middle") => Some(VerticalAlign::Center),
        Some("bottom") => Some(VerticalAlign::Bottom),
        _ => None,
    };
    let all = element.attr("fo:border").and_then(read_border);
    let side = |name: &str| {
        element
            .attr(name)
            .and_then(read_border)
            .or_else(|| all.clone())
    };
    let borders = CellBorders {
        top: side("fo:border-top"),
        left: side("fo:border-left"),
        bottom: side("fo:border-bottom"),
        right: side("fo:border-right"),
        inside_horizontal: None,
        inside_vertical: None,
    };
    if borders != CellBorders::default() {
        style.cell_borders = Some(borders);
    }
}

/// "0.5pt solid #000000" as a border.
fn read_border(value: &str) -> Option<Border> {
    if value.trim() == "none" {
        return None;
    }
    let mut width = 4;
    let mut style = "single".to_string();
    let mut color = None;
    for part in value.split_whitespace() {
        if let Some(twips) = parse_length(part) {
            // twips to eighths of a point
            width = ((twips * 8) / 20).max(1) as u32;
        } else if let Some(hex) = hex_colour(part) {
            color = Some(hex);
        } else {
            style = match part {
                "double" => "double",
                "dotted" => "dotted",
                "dashed" => "dashed",
                _ => "single",
            }
            .to_string();
        }
    }
    Some(Border {
        style,
        size: width,
        color,
    })
}

fn graphic_wrap(element: &Element) -> Option<Wrap> {
    let wrap = element.attr("style:wrap")?;
    Some(match wrap {
        "none" => Wrap::TopAndBottom,
        "run-through" if element.attr("style:run-through") == Some("background") => {
            Wrap::BehindText
        }
        "run-through" => Wrap::InFrontOfText,
        "dynamic" | "parallel" | "left" | "right" | "biggest" => {
            if element.attr("style:wrap-contour") == Some("true") {
                Wrap::Tight
            } else {
                Wrap::Square
            }
        }
        _ => Wrap::Square,
    })
}

// writing -------------------------------------------------------------

/// `style:paragraph-properties` attributes and children for `props`.
/// `native` says whether preserved pieces came from an odt file.
pub fn paragraph_properties(
    props: &ParagraphProps,
    break_after_page: bool,
    native: bool,
) -> String {
    let mut attributes: Vec<(String, String)> = Vec::new();
    let mut push = |name: &str, value: String| attributes.push((name.to_string(), value));
    if let Some(alignment) = props.alignment {
        let value = match alignment {
            Alignment::Left => "start",
            Alignment::Center => "center",
            Alignment::Right => "end",
            Alignment::Justify => "justify",
        };
        push("fo:text-align", value.into());
    }
    if let Some(left) = props.indent.left {
        push("fo:margin-left", twips_to_inches(left));
    }
    if let Some(right) = props.indent.right {
        push("fo:margin-right", twips_to_inches(right));
    }
    match (props.indent.first_line, props.indent.hanging) {
        (_, Some(hanging)) => push("fo:text-indent", twips_to_inches(-hanging)),
        (Some(first), None) => push("fo:text-indent", twips_to_inches(first)),
        _ => {}
    }
    if let Some(before) = props.spacing.before {
        push("fo:margin-top", twips_to_inches(before));
    }
    if let Some(after) = props.spacing.after {
        push("fo:margin-bottom", twips_to_inches(after));
    }
    if let Some(line) = props.spacing.line {
        match line.rule {
            LineRule::Auto => push(
                "fo:line-height",
                format!("{}%", (f64::from(line.value) / 2.4).round()),
            ),
            LineRule::Exact => push("fo:line-height", twips_to_inches(line.value)),
            LineRule::AtLeast => push("style:line-height-at-least", twips_to_inches(line.value)),
        }
    }
    if let Some(keep) = props.keep_with_next {
        push(
            "fo:keep-with-next",
            if keep { "always" } else { "auto" }.into(),
        );
    }
    if let Some(keep) = props.keep_lines_together {
        push(
            "fo:keep-together",
            if keep { "always" } else { "auto" }.into(),
        );
    }
    if props.page_break_before == Some(true) {
        push("fo:break-before", "page".into());
    }
    if break_after_page {
        push("fo:break-after", "page".into());
    }
    let mut children = String::new();
    if !props.tabs.is_empty() {
        children.push_str(&tab_stops(props));
    }
    let mut raw_attributes = String::new();
    if native {
        for piece in &props.preserved {
            if piece.starts_with('<') {
                children.push_str(piece);
            } else {
                raw_attributes.push(' ');
                raw_attributes.push_str(piece);
            }
        }
    }
    element(
        "style:paragraph-properties",
        &attributes,
        &raw_attributes,
        &children,
    )
}

fn tab_stops(props: &ParagraphProps) -> String {
    let left = props.indent.left.unwrap_or(0);
    let mut xml = String::from("<style:tab-stops>");
    for stop in props.tabs.iter().filter(|stop| stop.kind != TabKind::Clear) {
        let kind = match stop.kind {
            TabKind::Center => "center",
            TabKind::Right => "right",
            TabKind::Decimal => "char",
            _ => "left",
        };
        let leader = match stop.leader {
            TabLeader::None => String::new(),
            TabLeader::Dot | TabLeader::MiddleDot => {
                " style:leader-style=\"dotted\" style:leader-text=\".\"".into()
            }
            TabLeader::Hyphen => " style:leader-style=\"dash\" style:leader-text=\"-\"".into(),
            TabLeader::Underscore | TabLeader::Heavy => {
                " style:leader-style=\"solid\" style:leader-text=\"_\"".into()
            }
        };
        let char = if stop.kind == TabKind::Decimal {
            " style:char=\".\""
        } else {
            ""
        };
        xml.push_str(&format!(
            "<style:tab-stop style:position=\"{}\" style:type=\"{kind}\"{char}{leader}/>",
            twips_to_inches(stop.position - left)
        ));
    }
    xml.push_str("</style:tab-stops>");
    xml
}

/// `style:text-properties` for `run`. `fonts` collects the font names
/// used, for the font face declarations.
pub fn text_properties(run: &RunProps, native: bool, fonts: &mut Vec<String>) -> String {
    let mut attributes: Vec<(String, String)> = Vec::new();
    let mut push = |name: &str, value: &str| attributes.push((name.to_string(), value.to_string()));
    if let Some(bold) = run.bold {
        push("fo:font-weight", if bold { "bold" } else { "normal" });
    }
    if let Some(italic) = run.italic {
        push("fo:font-style", if italic { "italic" } else { "normal" });
    }
    if let Some(underline) = &run.underline {
        let (style, kind) = match underline.as_str() {
            "none" => ("none", None),
            "double" => ("solid", Some("double")),
            "wave" => ("wave", None),
            "dotted" => ("dotted", None),
            "dash" => ("dash", None),
            "dotDash" => ("dot-dash", None),
            "dotDotDash" => ("dot-dot-dash", None),
            _ => ("solid", None),
        };
        push("style:text-underline-style", style);
        if style != "none" {
            push("style:text-underline-width", "auto");
            push("style:text-underline-color", "font-color");
        }
        if let Some(kind) = kind {
            push("style:text-underline-type", kind);
        }
    }
    if run.double_strike == Some(true) {
        push("style:text-line-through-style", "solid");
        push("style:text-line-through-type", "double");
    } else if let Some(strike) = run.strike {
        push(
            "style:text-line-through-style",
            if strike { "solid" } else { "none" },
        );
    }
    match run.script {
        Some(Script::Superscript) => push("style:text-position", "super 58%"),
        Some(Script::Subscript) => push("style:text-position", "sub 58%"),
        Some(Script::Baseline) => push("style:text-position", "0% 100%"),
        None => {}
    }
    if let Some(small) = run.small_caps {
        push(
            "fo:font-variant",
            if small { "small-caps" } else { "normal" },
        );
    }
    if let Some(caps) = run.all_caps {
        push("fo:text-transform", if caps { "uppercase" } else { "none" });
    }
    if let Some(font) = &run.font {
        if !fonts.contains(&font.name) {
            fonts.push(font.name.clone());
        }
        push("style:font-name", &font.name);
    }
    if let Some(size) = run.size {
        push("fo:font-size", &points(f64::from(size) / 2.0));
    }
    if let Some(color) = &run.color {
        push("fo:color", &format!("#{}", color.to_ascii_lowercase()));
    }
    if let Some(highlight) = &run.highlight {
        push("fo:background-color", &highlight_hex(highlight));
    }
    if let Some(language) = &run.language {
        let mut parts = language.splitn(2, '-');
        push("fo:language", parts.next().unwrap_or("none"));
        push("fo:country", parts.next().unwrap_or("none"));
    }
    if run.hidden == Some(true) {
        push("text:display", "none");
    }
    let mut raw = String::new();
    if native {
        for piece in run.preserved.iter().filter(|piece| !piece.starts_with('<')) {
            raw.push(' ');
            raw.push_str(piece);
        }
    }
    element("style:text-properties", &attributes, &raw, "")
}

/// word's highlighter colours by name, as odt hex colours.
fn highlight_hex(color: &str) -> String {
    let hex = match color {
        "yellow" => "ffff00",
        "green" => "00ff00",
        "cyan" => "00ffff",
        "magenta" => "ff00ff",
        "blue" => "0000ff",
        "red" => "ff0000",
        "darkBlue" => "000080",
        "darkCyan" => "008080",
        "darkGreen" => "008000",
        "darkMagenta" => "800080",
        "darkRed" => "800000",
        "darkYellow" => "808000",
        "darkGray" => "808080",
        "lightGray" => "c0c0c0",
        "black" => "000000",
        "white" => "ffffff",
        other if other.len() == 6 => return format!("#{}", other.to_ascii_lowercase()),
        _ => "ffff00",
    };
    format!("#{hex}")
}

pub fn cell_properties(
    borders: Option<&CellBorders>,
    background: Option<&str>,
    align: Option<VerticalAlign>,
) -> String {
    let mut attributes: Vec<(String, String)> = Vec::new();
    if let Some(color) = background {
        attributes.push((
            "fo:background-color".into(),
            format!("#{}", color.to_ascii_lowercase()),
        ));
    }
    if let Some(align) = align {
        let value = match align {
            VerticalAlign::Top => "top",
            VerticalAlign::Center => "middle",
            VerticalAlign::Bottom => "bottom",
        };
        attributes.push(("style:vertical-align".into(), value.into()));
    }
    if let Some(borders) = borders {
        for (name, border) in [
            ("fo:border-top", &borders.top),
            ("fo:border-left", &borders.left),
            ("fo:border-bottom", &borders.bottom),
            ("fo:border-right", &borders.right),
        ] {
            let value = match border {
                Some(border) if border.style != "none" && border.style != "nil" => format!(
                    "{} {} #{}",
                    points(f64::from(border.size) / 8.0),
                    match border.style.as_str() {
                        "double" => "double",
                        "dotted" => "dotted",
                        "dashed" => "dashed",
                        _ => "solid",
                    },
                    border
                        .color
                        .as_deref()
                        .unwrap_or("000000")
                        .to_ascii_lowercase()
                ),
                _ => "none".into(),
            };
            attributes.push((name.into(), value));
        }
    }
    element("style:table-cell-properties", &attributes, "", "")
}

/// `<name a="1" raw>children</name>`, or `<name a="1" raw/>`.
fn element(name: &str, attributes: &[(String, String)], raw: &str, children: &str) -> String {
    let mut xml = format!("<{name}");
    for (key, value) in attributes {
        xml.push_str(&format!(" {key}=\"{}\"", escape_attribute(value)));
    }
    xml.push_str(raw);
    if children.is_empty() {
        xml.push_str("/>");
    } else {
        xml.push_str(&format!(">{children}</{name}>"));
    }
    xml
}
