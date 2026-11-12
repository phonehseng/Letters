//! the worksheet elements around the cells, written from the model. each
//! function makes one element (or none when the model has nothing for
//! it). the worksheet writer compares the result with what the same
//! function made when the file was read and keeps the original xml when
//! they match, so untouched elements stay byte for byte.

use crate::address::{CellRef, Range};
use crate::model::*;
use crate::xml::{escape_text, Fragment, XmlWriter};

use super::style_parts::color_attributes;

fn extras<'a>(sheet: &'a Sheet, element: &str) -> Option<&'a crate::model::carry::ElementExtras> {
    sheet
        .carry
        .element_extras
        .iter()
        .find(|(name, _, _)| name == element)
}

/// an element from model attributes and children plus what was kept of it
fn with_extras(
    sheet: &Sheet,
    name: &str,
    mut attributes: Vec<(String, String)>,
    children: String,
) -> Fragment {
    let mut kept_children = String::new();
    if let Some((_, kept_attributes, kept)) = extras(sheet, name) {
        attributes.extend(kept_attributes.iter().cloned());
        kept_children = kept.clone();
    }
    let mut fragment = Fragment {
        name: name.to_string(),
        attributes,
        children: Vec::new(),
    };
    let all = children + &kept_children;
    if !all.is_empty() {
        fragment.children.push((String::new(), all));
    }
    fragment
}

fn pair(name: &str, value: impl ToString) -> (String, String) {
    (name.to_string(), value.to_string())
}

pub fn sheet_properties(sheet: &Sheet) -> Option<String> {
    let mut children = String::new();
    if let Some(color) = &sheet.view.tab_color {
        let mut writer = XmlWriter::new();
        writer.empty_owned("tabColor", &color_attributes(color));
        children.push_str(&writer.finish());
    }
    if sheet.print.fit_to.is_some() {
        children.push_str("<pageSetUpPr fitToPage=\"1\"/>");
    }
    let fragment = with_extras(sheet, "sheetPr", Vec::new(), children);
    let empty = fragment.attributes.is_empty() && fragment.children.is_empty();
    (!empty).then(|| fragment.to_xml())
}

pub fn sheet_views(sheet: &Sheet) -> Option<String> {
    let view = &sheet.view;
    let mut attributes = Vec::new();
    if view.selected {
        attributes.push(pair("tabSelected", 1));
    }
    if !view.show_grid_lines {
        attributes.push(pair("showGridLines", 0));
    }
    if view.show_formulas {
        attributes.push(pair("showFormulas", 1));
    }
    if view.right_to_left {
        attributes.push(pair("rightToLeft", 1));
    }
    if view.zoom != 100 {
        attributes.push(pair("zoomScale", view.zoom));
    }
    attributes.push(pair("workbookViewId", 0));
    let children = pane_and_selection(view);
    let fragment = with_extras(sheet, "sheetView", attributes, children);
    Some(format!("<sheetViews>{}</sheetViews>", fragment.to_xml()))
}

fn pane_and_selection(view: &SheetView) -> String {
    let mut out = String::new();
    let active = view.active_cell.unwrap_or_default().to_a1();
    match view.frozen.filter(|f| f.rows > 0 || f.cols > 0) {
        Some(frozen) => {
            let top_left = CellRef::new(frozen.rows, frozen.cols).to_a1();
            // the pane that scrolls is the one the active cell is in
            let pane = match (frozen.rows > 0, frozen.cols > 0) {
                (true, true) => "bottomRight",
                (true, false) => "bottomLeft",
                _ => "topRight",
            };
            let mut writer = XmlWriter::new();
            let mut attributes = Vec::new();
            if frozen.cols > 0 {
                attributes.push(pair("xSplit", frozen.cols));
            }
            if frozen.rows > 0 {
                attributes.push(pair("ySplit", frozen.rows));
            }
            attributes.push(pair("topLeftCell", top_left));
            attributes.push(pair("activePane", pane));
            attributes.push(pair("state", "frozen"));
            writer.empty_owned("pane", &attributes);
            writer.empty(
                "selection",
                &[("pane", pane), ("activeCell", &active), ("sqref", &active)],
            );
            out.push_str(&writer.finish());
        }
        None if view.active_cell.is_some() => {
            out.push_str(&format!(
                "<selection activeCell=\"{active}\" sqref=\"{active}\"/>"
            ));
        }
        None => {}
    }
    out
}

pub fn format_properties(sheet: &Sheet) -> Option<String> {
    let mut attributes = Vec::new();
    if let Some(width) = sheet.default_column_width {
        attributes.push(pair("defaultColWidth", width));
    }
    // excel requires defaultRowHeight on this element
    attributes.push(pair(
        "defaultRowHeight",
        sheet.default_row_height.unwrap_or(DEFAULT_ROW_HEIGHT),
    ));
    Some(with_extras(sheet, "sheetFormatPr", attributes, String::new()).to_xml())
}

pub fn columns(sheet: &Sheet) -> Option<String> {
    if sheet.columns.is_empty() {
        return None;
    }
    let mut writer = XmlWriter::new();
    writer.open("cols", &[]);
    for info in &sheet.columns {
        let mut attributes = vec![pair("min", info.first + 1), pair("max", info.last + 1)];
        // excel refuses a <col> without a width
        attributes.push(pair(
            "width",
            info.width
                .unwrap_or(sheet.default_column_width.unwrap_or(DEFAULT_COLUMN_WIDTH)),
        ));
        if let Some(style) = info.style {
            attributes.push(pair("style", style));
        }
        let flags = [
            ("hidden", info.hidden),
            ("bestFit", info.best_fit),
            ("customWidth", info.custom_width),
            ("collapsed", info.collapsed),
        ];
        for (name, on) in flags {
            if on {
                attributes.push(pair(name, 1));
            }
        }
        if let Some(level) = info.outline_level {
            attributes.push(pair("outlineLevel", level));
        }
        writer.empty_owned("col", &attributes);
    }
    writer.close("cols");
    Some(writer.finish())
}

pub fn merges(sheet: &Sheet) -> Option<String> {
    if sheet.merged.is_empty() {
        return None;
    }
    let mut writer = XmlWriter::new();
    writer.open("mergeCells", &[("count", &sheet.merged.len().to_string())]);
    for range in &sheet.merged {
        writer.empty("mergeCell", &[("ref", &range.to_a1())]);
    }
    writer.close("mergeCells");
    Some(writer.finish())
}

pub fn conditional_formats(sheet: &Sheet) -> Option<String> {
    if sheet.conditional_formats.is_empty() {
        return None;
    }
    let mut out = String::new();
    for format in &sheet.conditional_formats {
        out.push_str(&format!(
            "<conditionalFormatting sqref=\"{}\">",
            Range::list_to_a1(&format.ranges)
        ));
        for rule in &format.rules {
            out.push_str(&conditional_rule(rule));
        }
        out.push_str("</conditionalFormatting>");
    }
    Some(out)
}

fn conditional_rule(rule: &ConditionalRule) -> String {
    let mut attributes = vec![pair("type", &rule.kind)];
    if let Some(format) = rule.format {
        attributes.push(pair("dxfId", format));
    }
    attributes.push(pair("priority", rule.priority));
    if rule.stop_if_true {
        attributes.push(pair("stopIfTrue", 1));
    }
    if let Some(operator) = &rule.operator {
        attributes.push(pair("operator", operator));
    }
    if let Some(text) = &rule.text {
        attributes.push(pair("text", text));
    }
    attributes.extend(rule.attributes.iter().cloned());
    let mut writer = XmlWriter::new();
    writer.open_owned("cfRule", &attributes);
    for formula in &rule.formulas {
        writer.text_element("formula", &[], formula);
    }
    if let Some(details) = &rule.details_xml {
        writer.raw(details);
    }
    writer.close("cfRule");
    writer.finish()
}

pub fn validations(sheet: &Sheet) -> Option<String> {
    if sheet.validations.is_empty() {
        return None;
    }
    let mut writer = XmlWriter::new();
    writer.open(
        "dataValidations",
        &[("count", &sheet.validations.len().to_string())],
    );
    for rule in &sheet.validations {
        writer.open_owned("dataValidation", &validation_attributes(rule));
        if let Some(formula) = &rule.formula1 {
            writer.text_element("formula1", &[], formula);
        }
        if let Some(formula) = &rule.formula2 {
            writer.text_element("formula2", &[], formula);
        }
        writer.close("dataValidation");
    }
    writer.close("dataValidations");
    Some(writer.finish())
}

fn validation_attributes(rule: &DataValidation) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    if rule.kind != "any" {
        attributes.push(pair("type", &rule.kind));
    }
    let texts = [
        ("errorStyle", &rule.error_style),
        ("operator", &rule.operator),
    ];
    for (name, value) in texts {
        if let Some(value) = value {
            attributes.push(pair(name, value));
        }
    }
    let flags = [
        ("allowBlank", rule.allow_blank),
        ("showDropDown", rule.hide_dropdown),
        ("showInputMessage", rule.show_input_message),
        ("showErrorMessage", rule.show_error_message),
    ];
    for (name, on) in flags {
        if on {
            attributes.push(pair(name, 1));
        }
    }
    let messages = [
        ("errorTitle", &rule.error_title),
        ("error", &rule.error),
        ("promptTitle", &rule.prompt_title),
        ("prompt", &rule.prompt),
    ];
    for (name, value) in messages {
        if let Some(value) = value {
            attributes.push(pair(name, value));
        }
    }
    attributes.push(pair("sqref", Range::list_to_a1(&rule.ranges)));
    attributes
}

/// hyperlinks; `ids` gives the relationship id of each external one
pub fn hyperlinks(sheet: &Sheet, ids: &[Option<String>]) -> Option<String> {
    if sheet.hyperlinks.is_empty() {
        return None;
    }
    let mut writer = XmlWriter::new();
    writer.open("hyperlinks", &[]);
    for (index, link) in sheet.hyperlinks.iter().enumerate() {
        let mut attributes = vec![pair("ref", link.range.to_a1())];
        if let Some(Some(id)) = ids.get(index) {
            attributes.push(pair("r:id", id));
        }
        let texts = [
            ("location", &link.location),
            ("tooltip", &link.tooltip),
            ("display", &link.display),
        ];
        for (name, value) in texts {
            if let Some(value) = value {
                attributes.push(pair(name, value));
            }
        }
        writer.empty_owned("hyperlink", &attributes);
    }
    writer.close("hyperlinks");
    Some(writer.finish())
}

pub fn auto_filter(filter: &AutoFilter) -> String {
    let mut out = format!("<autoFilter ref=\"{}\">", filter.range.to_a1());
    for (column, rule) in &filter.columns {
        out.push_str(&filter_column(*column, rule));
    }
    if let Some(sort) = &filter.sort {
        out.push_str(&sort_state(sort));
    }
    out.push_str("</autoFilter>");
    out
}

fn filter_column(column: u32, rule: &ColumnFilter) -> String {
    let inner = match rule {
        ColumnFilter::Other { xml } => return xml.clone(),
        ColumnFilter::Values { values, blank } => {
            let blank = if *blank { " blank=\"1\"" } else { "" };
            let items: String = values
                .iter()
                .map(|value| format!("<filter val=\"{}\"/>", crate::xml::escape_attribute(value)))
                .collect();
            format!("<filters{blank}>{items}</filters>")
        }
        ColumnFilter::Custom { conditions, and } => {
            let and = if *and { " and=\"1\"" } else { "" };
            let items: String = conditions
                .iter()
                .map(|(operator, value)| {
                    let operator = if operator == "equal" {
                        String::new()
                    } else {
                        format!(" operator=\"{operator}\"")
                    };
                    format!(
                        "<customFilter{operator} val=\"{}\"/>",
                        crate::xml::escape_attribute(value)
                    )
                })
                .collect();
            format!("<customFilters{and}>{items}</customFilters>")
        }
        ColumnFilter::Top {
            count,
            percent,
            bottom,
        } => format!(
            "<top10{}{} val=\"{count}\"/>",
            if *bottom { " top=\"0\"" } else { "" },
            if *percent { " percent=\"1\"" } else { "" }
        ),
    };
    format!("<filterColumn colId=\"{column}\">{inner}</filterColumn>")
}

fn sort_state(sort: &SortState) -> String {
    let mut out = format!("<sortState ref=\"{}\">", sort.range.to_a1());
    for (key, descending) in &sort.keys {
        let descending = if *descending { " descending=\"1\"" } else { "" };
        out.push_str(&format!(
            "<sortCondition{descending} ref=\"{}\"/>",
            key.to_a1()
        ));
    }
    out.push_str("</sortState>");
    out
}

pub fn print_options(sheet: &Sheet) -> Option<String> {
    let print = &sheet.print;
    let flags = [
        ("horizontalCentered", print.center_horizontally),
        ("verticalCentered", print.center_vertically),
        ("headings", print.headings),
        ("gridLines", print.grid_lines),
    ];
    let attributes: Vec<(String, String)> = flags
        .iter()
        .filter(|(_, on)| *on)
        .map(|(name, _)| pair(name, 1))
        .collect();
    if attributes.is_empty() {
        return None;
    }
    let mut writer = XmlWriter::new();
    writer.empty_owned("printOptions", &attributes);
    Some(writer.finish())
}

pub fn margins(sheet: &Sheet) -> Option<String> {
    let m = sheet.print.margins.unwrap_or_default();
    Some(format!(
        "<pageMargins left=\"{}\" right=\"{}\" top=\"{}\" bottom=\"{}\" header=\"{}\" footer=\"{}\"/>",
        m.left, m.right, m.top, m.bottom, m.header, m.footer
    ))
}

pub fn page_setup(sheet: &Sheet) -> Option<String> {
    let print = &sheet.print;
    let mut attributes = Vec::new();
    if let Some(size) = print.paper_size {
        attributes.push(pair("paperSize", size));
    }
    if let Some(scale) = print.scale {
        attributes.push(pair("scale", scale));
    }
    if let Some((wide, tall)) = print.fit_to {
        attributes.push(pair("fitToWidth", wide));
        attributes.push(pair("fitToHeight", tall));
    }
    match print.orientation {
        Orientation::Portrait => attributes.push(pair("orientation", "portrait")),
        Orientation::Landscape => attributes.push(pair("orientation", "landscape")),
        Orientation::Default => {}
    }
    let fragment = with_extras(sheet, "pageSetup", attributes, String::new());
    (!fragment.attributes.is_empty()).then(|| fragment.to_xml())
}

pub fn header_footer(sheet: &Sheet) -> Option<String> {
    let mut children = String::new();
    if let Some(header) = &sheet.print.header {
        children.push_str(&format!("<oddHeader>{}</oddHeader>", escape_text(header)));
    }
    if let Some(footer) = &sheet.print.footer {
        children.push_str(&format!("<oddFooter>{}</oddFooter>", escape_text(footer)));
    }
    let fragment = with_extras(sheet, "headerFooter", Vec::new(), children);
    let empty = fragment.attributes.is_empty() && fragment.children.is_empty();
    (!empty).then(|| fragment.to_xml())
}

/// manual page breaks; `limit` is the last column (row breaks) or row
pub fn breaks(name: &str, positions: &[u32], limit: u32) -> Option<String> {
    if positions.is_empty() {
        return None;
    }
    let count = positions.len();
    let mut out = format!("<{name} count=\"{count}\" manualBreakCount=\"{count}\">");
    for id in positions {
        out.push_str(&format!("<brk id=\"{id}\" max=\"{limit}\" man=\"1\"/>"));
    }
    out.push_str(&format!("</{name}>"));
    Some(out)
}

/// "A1:D10" of everything on the sheet, as excel's dimension element wants
pub fn dimension(sheet: &Sheet) -> String {
    let used = sheet
        .cells
        .used_range()
        .map(|range| range.to_a1())
        .unwrap_or_else(|| "A1".into());
    format!("<dimension ref=\"{used}\"/>")
}
