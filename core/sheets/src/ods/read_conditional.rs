//! libreoffice's conditional formats (`<calcext:conditional-formats>`)
//! read into the model's rules, so the grid draws them as it draws
//! excel's. the element itself is still kept as xml for saving back to
//! ods; what is read here only feeds the screen (and an xlsx made from
//! the file).
//!
//! libreoffice writes each condition as text: "&gt;50", "between(1,10)",
//! "formula-is([.A1]&gt;[.B1])", "top-elements(10)", "duplicate"... with
//! openformula inside, relative to `calcext:base-cell-address`. a rule's
//! look is a named cell style ("Bad", "Good", "Accent"), turned here into
//! a differential format. colour scales and data bars become excel's
//! `<colorScale>` and `<dataBar>` (a 2007-style bar: libreoffice's axis
//! and solid bars have no place there).

use std::collections::HashMap;

use crate::address::{CellRef, Range};
use crate::formula::{parse_open_formula, refs::translate, to_string, Expr};
use crate::model::{CellStyle, ConditionalFormat, ConditionalRule, StyleTable};
use crate::xml::{escape_attribute, Fragment};

use super::styles::StyleSheet;

/// what reading a sheet's conditional formats needs from the file
pub struct ConditionalContext<'a> {
    pub styles: &'a StyleSheet,
    pub table: &'a mut StyleTable,
    /// differential formats made so far, by style name, so each style
    /// becomes one format however many rules use it
    pub made: HashMap<String, u32>,
    /// the next rule's priority: libreoffice has none, so the file's
    /// order is the priority, as it is in libreoffice
    pub priority: i32,
}

/// the rules of a `<calcext:conditional-formats>` element
pub fn read_conditional_formats(
    xml: &str,
    context: &mut ConditionalContext,
) -> Vec<ConditionalFormat> {
    let Some(formats) = Fragment::parse(xml) else {
        return Vec::new();
    };
    formats
        .children
        .iter()
        .filter(|(name, _)| name == "conditional-format")
        .filter_map(|(_, xml)| read_format(xml, context))
        .collect()
}

fn read_format(xml: &str, context: &mut ConditionalContext) -> Option<ConditionalFormat> {
    let format = Fragment::parse(xml)?;
    let ranges = target_ranges(format.attribute("calcext:target-range-address")?);
    let origin = ranges.first()?.start;
    let conditions = format
        .children
        .iter()
        .filter(|(name, _)| name == "condition")
        .count();
    let mut rules = Vec::new();
    for (name, child) in &format.children {
        let Some(element) = Fragment::parse(child) else {
            continue;
        };
        let rule = match name.as_str() {
            "condition" => read_condition(&element, origin, context).map(|mut rule| {
                // within one libreoffice format the first true condition
                // is the only one applied
                rule.stop_if_true = conditions > 1;
                rule
            }),
            "color-scale" => read_color_scale(&element, origin),
            "data-bar" => read_data_bar(&element, origin),
            // icon sets are not drawn, and date conditions are left out
            _ => None,
        };
        if let Some(mut rule) = rule {
            context.priority += 1;
            rule.priority = context.priority;
            rules.push(rule);
        }
    }
    (!rules.is_empty()).then_some(ConditionalFormat { ranges, rules })
}

/// "Sheet1.B1:Sheet1.B40 Sheet1.D1:Sheet1.D40", with quoted sheet names
/// that may hold spaces ('My marks'.A1)
fn target_ranges(address: &str) -> Vec<Range> {
    split_outside_quotes(address, ' ')
        .iter()
        .filter(|part| !part.is_empty())
        .filter_map(
            |part| match parse_open_formula(&format!("[{part}]")).ok()? {
                Expr::Ref { reference } => Some(reference.area.to_range()),
                _ => None,
            },
        )
        .collect()
}

/// splits at `separator` where it is outside quotes and brackets
fn split_outside_quotes(text: &str, separator: char) -> Vec<String> {
    let mut parts = vec![String::new()];
    let (mut quote, mut depth) = (None, 0i32);
    for c in text.chars() {
        match c {
            '\'' | '"' if quote == Some(c) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(c),
            '(' | '[' if quote.is_none() => depth += 1,
            ')' | ']' if quote.is_none() => depth -= 1,
            _ if c == separator && quote.is_none() && depth == 0 => {
                parts.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(last) = parts.last_mut() {
            last.push(c);
        }
    }
    parts
}

/// a cell reference to `cell` in openformula's place (moving with it)
fn base_cell(element: &Fragment) -> Option<CellRef> {
    let address = element.attribute("calcext:base-cell-address")?;
    target_ranges(address).first().map(|range| range.start)
}

/// openformula text as excel formula text, moved from the base cell the
/// file wrote it for to the range's top-left cell, which excel's rules
/// (and the engine) count from
fn excel_formula(open: &str, base: Option<CellRef>, origin: CellRef) -> Option<String> {
    let expr = parse_open_formula(open.trim()).ok()?;
    let expr = match base {
        Some(base) if base != origin => translate(
            &expr,
            i64::from(origin.row) - i64::from(base.row),
            i64::from(origin.col) - i64::from(base.col),
        ),
        _ => expr,
    };
    Some(to_string(&expr))
}

/// the text inside "name(" ... ")"
fn argument<'t>(value: &'t str, name: &str) -> Option<&'t str> {
    value
        .strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')
}

fn read_condition(
    element: &Fragment,
    origin: CellRef,
    context: &mut ConditionalContext,
) -> Option<ConditionalRule> {
    let value = element.attribute("calcext:value")?.trim();
    let base = base_cell(element);
    let formula = |text: &str| excel_formula(text, base, origin);
    let mut rule = ConditionalRule {
        kind: "cellIs".into(),
        format: element
            .attribute("calcext:apply-style-name")
            .map(|name| differential_for(name, context)),
        ..ConditionalRule::default()
    };
    let attribute = |key: &str, value: &str| (key.to_string(), value.to_string());
    if let Some(inner) = argument(value, "between").or_else(|| argument(value, "not-between")) {
        let bounds = split_outside_quotes(inner, ',');
        let bounds = if bounds.len() == 2 {
            bounds
        } else {
            split_outside_quotes(inner, ';')
        };
        let [low, high] = bounds.as_slice() else {
            return None;
        };
        let negated = value.starts_with("not-");
        rule.operator = Some(if negated { "notBetween" } else { "between" }.into());
        rule.formulas = vec![formula(low)?, formula(high)?];
    } else if let Some(inner) = argument(value, "formula-is") {
        rule.kind = "expression".into();
        rule.formulas = vec![formula(inner)?];
    } else if let Some((kind, inner)) = [
        ("containsText", "contains-text"),
        ("notContainsText", "not-contains-text"),
        ("beginsWith", "begins-with"),
        ("endsWith", "ends-with"),
    ]
    .iter()
    .find_map(|(kind, name)| argument(value, name).map(|inner| (*kind, inner)))
    {
        let text = match parse_open_formula(inner).ok()? {
            Expr::Text { value } => value,
            _ => return None,
        };
        rule.kind = kind.into();
        // excel names the operator after the kind, but for "notContains"
        let operator = if kind == "notContainsText" {
            "notContains"
        } else {
            kind
        };
        rule.operator = Some(operator.into());
        rule.formulas = vec![text_formula(kind, &text, origin)];
        rule.text = Some(text);
    } else if let Some((rank, percent, bottom)) = [
        ("top-elements", false, false),
        ("bottom-elements", false, true),
        ("top-percent", true, false),
        ("bottom-percent", true, true),
    ]
    .iter()
    .find_map(|(name, percent, bottom)| {
        argument(value, name).map(|inner| (inner.trim(), *percent, *bottom))
    }) {
        rule.kind = "top10".into();
        rule.attributes.push(attribute("rank", rank));
        if percent {
            rule.attributes.push(attribute("percent", "1"));
        }
        if bottom {
            rule.attributes.push(attribute("bottom", "1"));
        }
    } else {
        match value {
            "duplicate" => rule.kind = "duplicateValues".into(),
            "unique" => rule.kind = "uniqueValues".into(),
            "is-error" => rule.kind = "containsErrors".into(),
            "is-no-error" => rule.kind = "notContainsErrors".into(),
            "above-average" | "below-average" | "above-equal-average" | "below-equal-average" => {
                rule.kind = "aboveAverage".into();
                if value.starts_with("below") {
                    rule.attributes.push(attribute("aboveAverage", "0"));
                }
                if value.contains("equal") {
                    rule.attributes.push(attribute("equalAverage", "1"));
                }
            }
            _ => read_comparison(value, &mut rule, &formula)?,
        }
    }
    Some(rule)
}

/// "&gt;=50" and the other comparisons with one value
fn read_comparison(
    value: &str,
    rule: &mut ConditionalRule,
    formula: &dyn Fn(&str) -> Option<String>,
) -> Option<()> {
    for (symbol, operator) in [
        ("<=", "lessThanOrEqual"),
        (">=", "greaterThanOrEqual"),
        ("!=", "notEqual"),
        ("<", "lessThan"),
        (">", "greaterThan"),
        ("=", "equal"),
    ] {
        if let Some(rest) = value.strip_prefix(symbol) {
            rule.operator = Some(operator.into());
            rule.formulas = vec![formula(rest)?];
            return Some(());
        }
    }
    None
}

/// the formula excel keeps beside a text rule's text, so an xlsx made
/// from the file reads in excel as excel's own rule would
fn text_formula(kind: &str, text: &str, origin: CellRef) -> String {
    let cell = origin.to_a1();
    let quoted = format!("\"{}\"", text.replace('"', "\"\""));
    match kind {
        "containsText" => format!("NOT(ISERROR(SEARCH({quoted},{cell})))"),
        "notContainsText" => format!("ISERROR(SEARCH({quoted},{cell}))"),
        "beginsWith" => format!("LEFT({cell},LEN({quoted}))={quoted}"),
        _ => format!("RIGHT({cell},LEN({quoted}))={quoted}"),
    }
}

/// the differential format for a libreoffice cell style: its font colour,
/// bold, italic, underline, strike and background
fn differential_for(name: &str, context: &mut ConditionalContext) -> u32 {
    if let Some(index) = context.made.get(name) {
        return *index;
    }
    let style = context.styles.cell_style(name);
    context.table.differential.push(differential_xml(&style));
    let index = (context.table.differential.len() - 1) as u32;
    context.made.insert(name.to_string(), index);
    index
}

fn differential_xml(style: &CellStyle) -> String {
    let font = &style.font;
    let mut font_xml = String::new();
    for (on, element) in [
        (font.bold, "<b/>"),
        (font.italic, "<i/>"),
        (font.strike, "<strike/>"),
        (font.underline.is_some(), "<u/>"),
    ] {
        if on {
            font_xml.push_str(element);
        }
    }
    if let Some(rgb) = font.color.as_ref().and_then(|c| c.rgb.as_deref()) {
        font_xml.push_str(&format!("<color rgb=\"FF{}\"/>", escape_attribute(rgb)));
    }
    let mut xml = String::from("<dxf>");
    if !font_xml.is_empty() {
        xml.push_str(&format!("<font>{font_xml}</font>"));
    }
    if let Some(rgb) = style.fill.background().and_then(|c| c.rgb.as_deref()) {
        // excel's rules give a solid fill as the pattern's background colour
        xml.push_str(&format!(
            "<fill><patternFill><bgColor rgb=\"FF{}\"/></patternFill></fill>",
            escape_attribute(rgb)
        ));
    }
    xml.push_str("</dxf>");
    xml
}

/// "#f8696b" as excel's "FFF8696B"
fn excel_rgb(css: &str) -> Option<String> {
    let hex = css.trim().strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("FF{}", hex.to_ascii_uppercase()))
}

/// a scale or bar point as excel's `<cfvo>`
fn cfvo(entry: &Fragment, origin: CellRef) -> Option<String> {
    let value = entry.attribute("calcext:value").unwrap_or("0");
    let (kind, value) = match entry.attribute("calcext:type")? {
        "minimum" | "auto-minimum" => ("min", None),
        "maximum" | "auto-maximum" => ("max", None),
        "percentile" => ("percentile", Some(value.to_string())),
        "percent" => ("percent", Some(value.to_string())),
        "number" => ("num", Some(value.to_string())),
        "formula" => ("formula", Some(excel_formula(value, None, origin)?)),
        _ => return None,
    };
    Some(match value {
        Some(value) => format!(
            "<cfvo type=\"{kind}\" val=\"{}\"/>",
            escape_attribute(&value)
        ),
        None => format!("<cfvo type=\"{kind}\"/>"),
    })
}

fn read_color_scale(element: &Fragment, origin: CellRef) -> Option<ConditionalRule> {
    let mut points = String::new();
    let mut colours = String::new();
    for (name, xml) in &element.children {
        if name != "color-scale-entry" {
            continue;
        }
        let entry = Fragment::parse(xml)?;
        points.push_str(&cfvo(&entry, origin)?);
        let rgb = excel_rgb(entry.attribute("calcext:color")?)?;
        colours.push_str(&format!("<color rgb=\"{rgb}\"/>"));
    }
    if points.is_empty() {
        return None;
    }
    Some(ConditionalRule {
        kind: "colorScale".into(),
        details_xml: Some(format!("<colorScale>{points}{colours}</colorScale>")),
        ..ConditionalRule::default()
    })
}

fn read_data_bar(element: &Fragment, origin: CellRef) -> Option<ConditionalRule> {
    let points: Vec<String> = element
        .children
        .iter()
        .filter(|(name, _)| name == "formatting-entry")
        .filter_map(|(_, xml)| cfvo(&Fragment::parse(xml)?, origin))
        .collect();
    let [low, high] = points.as_slice() else {
        return None;
    };
    let colour = excel_rgb(
        element
            .attribute("calcext:positive-color")
            .unwrap_or("#638ec6"),
    )?;
    let mut attributes = String::new();
    for (open, excel) in [
        ("calcext:min-length", "minLength"),
        ("calcext:max-length", "maxLength"),
    ] {
        if let Some(length) = element.attribute(open).and_then(|v| v.parse::<f64>().ok()) {
            // excel's lengths are whole percentages
            attributes.push_str(&format!(
                " {excel}=\"{}\"",
                length.round().clamp(0.0, 100.0)
            ));
        }
    }
    if element.attribute("calcext:show-value") == Some("false") {
        attributes.push_str(" showValue=\"0\"");
    }
    Some(ConditionalRule {
        kind: "dataBar".into(),
        details_xml: Some(format!(
            "<dataBar{attributes}>{low}{high}<color rgb=\"{colour}\"/></dataBar>"
        )),
        ..ConditionalRule::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(xml: &str, styles_xml: &str) -> (Vec<ConditionalFormat>, StyleTable) {
        let mut styles = StyleSheet::default();
        styles.add_all(styles_xml);
        let mut table = StyleTable::default();
        let mut context = ConditionalContext {
            styles: &styles,
            table: &mut table,
            made: HashMap::new(),
            priority: 0,
        };
        let formats = read_conditional_formats(xml, &mut context);
        (formats, table)
    }

    const BAD_STYLE: &str = r##"<office:styles><style:style style:name="Bad" style:family="table-cell"><style:table-cell-properties fo:background-color="#ffc7ce"/><style:text-properties fo:color="#9c0006" fo:font-weight="bold"/></style:style></office:styles>"##;

    #[test]
    fn comparisons_and_their_styles_are_read() {
        let xml = r#"<calcext:conditional-formats><calcext:conditional-format calcext:target-range-address="Sheet1.B1:Sheet1.B40"><calcext:condition calcext:apply-style-name="Bad" calcext:value="&gt;50" calcext:base-cell-address="Sheet1.B1"/></calcext:conditional-format><calcext:conditional-format calcext:target-range-address="Sheet1.C2:Sheet1.C9 Sheet1.E2:Sheet1.E9"><calcext:condition calcext:apply-style-name="Bad" calcext:value="between(1,[.$H$1])" calcext:base-cell-address="Sheet1.C2"/><calcext:condition calcext:apply-style-name="Bad" calcext:value="formula-is([.C2]&lt;[.D2])" calcext:base-cell-address="Sheet1.C2"/></calcext:conditional-format></calcext:conditional-formats>"#;
        let (formats, table) = read(xml, BAD_STYLE);
        assert_eq!(formats.len(), 2);
        let greater = &formats[0].rules[0];
        assert_eq!(formats[0].ranges, vec![Range::parse("B1:B40").unwrap()]);
        assert_eq!(
            (greater.kind.as_str(), greater.operator.as_deref()),
            ("cellIs", Some("greaterThan"))
        );
        assert_eq!(greater.formulas, vec!["50"]);
        assert!(!greater.stop_if_true);
        assert_eq!(
            table.differential[0],
            r#"<dxf><font><b/><color rgb="FF9C0006"/></font><fill><patternFill><bgColor rgb="FFFFC7CE"/></patternFill></fill></dxf>"#
        );
        assert_eq!(formats[1].ranges.len(), 2);
        let between = &formats[1].rules[0];
        assert_eq!(between.formulas, vec!["1", "$H$1"]);
        assert!(
            between.stop_if_true,
            "the first true condition of a format wins"
        );
        let expression = &formats[1].rules[1];
        assert_eq!(expression.kind, "expression");
        assert_eq!(expression.formulas, vec!["C2<D2"]);
        // both rules use one style, so one format
        assert_eq!(table.differential.len(), 1);
        assert_eq!(
            formats
                .iter()
                .flat_map(|f| &f.rules)
                .map(|r| r.priority)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn formulas_move_from_the_base_cell_to_the_top_left() {
        let xml = r#"<calcext:conditional-formats><calcext:conditional-format calcext:target-range-address="Sheet1.A2:Sheet1.A9"><calcext:condition calcext:apply-style-name="Bad" calcext:value="formula-is([.A5]&gt;0)" calcext:base-cell-address="Sheet1.A5"/></calcext:conditional-format></calcext:conditional-formats>"#;
        let (formats, _) = read(xml, BAD_STYLE);
        assert_eq!(formats[0].rules[0].formulas, vec!["A2>0"]);
    }

    #[test]
    fn statistics_and_text_rules_are_read() {
        let condition = |value: &str| {
            let xml = format!(
                r#"<calcext:conditional-formats><calcext:conditional-format calcext:target-range-address="Sheet1.A1:Sheet1.A9"><calcext:condition calcext:apply-style-name="Bad" calcext:value="{value}" calcext:base-cell-address="Sheet1.A1"/></calcext:conditional-format></calcext:conditional-formats>"#
            );
            read(&xml, BAD_STYLE).0.remove(0).rules.remove(0)
        };
        let top = condition("top-percent(20)");
        assert_eq!(top.kind, "top10");
        assert_eq!(
            top.attributes,
            vec![
                ("rank".to_string(), "20".to_string()),
                ("percent".to_string(), "1".to_string())
            ]
        );
        let below = condition("below-equal-average");
        assert_eq!(below.kind, "aboveAverage");
        assert_eq!(below.attributes.len(), 2);
        assert_eq!(condition("duplicate").kind, "duplicateValues");
        assert_eq!(condition("is-error").kind, "containsErrors");
        let text = condition("begins-with(&quot;Pass&quot;)");
        assert_eq!(text.kind, "beginsWith");
        assert_eq!(text.text.as_deref(), Some("Pass"));
        assert_eq!(text.formulas, vec![r#"LEFT(A1,LEN("Pass"))="Pass""#]);
        assert_eq!(condition("!=0").operator.as_deref(), Some("notEqual"));
    }

    #[test]
    fn scales_and_bars_become_excels() {
        let xml = r##"<calcext:conditional-formats><calcext:conditional-format calcext:target-range-address="Sheet1.C1:Sheet1.C40"><calcext:color-scale><calcext:color-scale-entry calcext:value="0" calcext:type="minimum" calcext:color="#f8696b"/><calcext:color-scale-entry calcext:value="50" calcext:type="percentile" calcext:color="#ffeb84"/><calcext:color-scale-entry calcext:value="0" calcext:type="maximum" calcext:color="#63be7b"/></calcext:color-scale></calcext:conditional-format><calcext:conditional-format calcext:target-range-address="Sheet1.D1:Sheet1.D40"><calcext:data-bar calcext:positive-color="#638ec6" calcext:negative-color="#ff0000" calcext:gradient="true" calcext:min-length="10" calcext:max-length="90"><calcext:formatting-entry calcext:value="0" calcext:type="auto-minimum"/><calcext:formatting-entry calcext:value="0" calcext:type="auto-maximum"/></calcext:data-bar></calcext:conditional-format></calcext:conditional-formats>"##;
        let (formats, _) = read(xml, "");
        assert_eq!(
            formats[0].rules[0].details_xml.as_deref(),
            Some(
                r#"<colorScale><cfvo type="min"/><cfvo type="percentile" val="50"/><cfvo type="max"/><color rgb="FFF8696B"/><color rgb="FFFFEB84"/><color rgb="FF63BE7B"/></colorScale>"#
            )
        );
        assert_eq!(
            formats[1].rules[0].details_xml.as_deref(),
            Some(
                r#"<dataBar minLength="10" maxLength="90"><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar>"#
            )
        );
    }
}
