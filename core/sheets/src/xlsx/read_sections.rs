//! the worksheet elements around the cells that the model reads: views,
//! columns, merges, conditional formats, validations, links, filters and
//! print settings. each function reads one element's xml into the sheet.

use crate::address::{CellRef, Range};
use crate::model::*;
use crate::xml::Fragment;

use super::style_parts::color_from_attributes;

fn flag(fragment: &Fragment, name: &str) -> bool {
    matches!(fragment.attribute(name), Some("1") | Some("true"))
}

fn number<T: std::str::FromStr>(fragment: &Fragment, name: &str) -> Option<T> {
    fragment
        .attribute(name)
        .and_then(|value| value.parse().ok())
}

/// attributes other than `known`, for keeping
fn other_attributes(fragment: &Fragment, known: &[&str]) -> Vec<(String, String)> {
    fragment
        .attributes
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .cloned()
        .collect()
}

/// keeps what the model does not read of a partly read element
fn keep_extras(
    sheet: &mut Sheet,
    fragment: &Fragment,
    known_attributes: &[&str],
    known_children: &[&str],
) {
    let attributes = other_attributes(fragment, known_attributes);
    let children: String = fragment
        .children_except(known_children)
        .iter()
        .map(|(_, xml)| xml.as_str())
        .collect();
    let name = crate::xml::local_name(&fragment.name).to_string();
    sheet
        .carry
        .element_extras
        .retain(|(known, _, _)| *known != name);
    sheet
        .carry
        .element_extras
        .push((name, attributes, children));
}

pub fn read_sheet_properties(xml: &str, sheet: &mut Sheet) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    for (name, child) in &fragment.children {
        let Some(part) = Fragment::parse(child) else {
            continue;
        };
        match name.as_str() {
            "tabColor" => sheet.view.tab_color = Some(color_from_attributes(&part.attributes)),
            "pageSetUpPr" if flag(&part, "fitToPage") => {
                sheet.print.fit_to.get_or_insert((1, 1));
            }
            _ => {}
        }
    }
    keep_extras(sheet, &fragment, &[], &["tabColor", "pageSetUpPr"]);
}

pub fn read_sheet_views(xml: &str, sheet: &mut Sheet) {
    let Some(views) = Fragment::parse(xml) else {
        return;
    };
    let Some((_, first)) = views.children.iter().find(|(name, _)| name == "sheetView") else {
        return;
    };
    let Some(view) = Fragment::parse(first) else {
        return;
    };
    sheet.view.zoom = number(&view, "zoomScale").unwrap_or(100);
    sheet.view.show_grid_lines = view.attribute("showGridLines") != Some("0");
    sheet.view.show_formulas = flag(&view, "showFormulas");
    sheet.view.right_to_left = flag(&view, "rightToLeft");
    sheet.view.selected = flag(&view, "tabSelected");
    for (name, child) in &view.children {
        let Some(part) = Fragment::parse(child) else {
            continue;
        };
        match name.as_str() {
            "pane"
                if matches!(
                    part.attribute("state"),
                    Some("frozen") | Some("frozenSplit")
                ) =>
            {
                sheet.view.frozen = Some(FrozenPanes {
                    rows: number::<f64>(&part, "ySplit").unwrap_or(0.0) as u32,
                    cols: number::<f64>(&part, "xSplit").unwrap_or(0.0) as u32,
                });
            }
            "selection" if sheet.view.active_cell.is_none() || part.attribute("pane").is_some() => {
                sheet.view.active_cell = part.attribute("activeCell").and_then(CellRef::parse);
            }
            _ => {}
        }
    }
    let known = [
        "tabSelected",
        "zoomScale",
        "showGridLines",
        "showFormulas",
        "rightToLeft",
        "workbookViewId",
    ];
    keep_extras(sheet, &view, &known, &["pane", "selection"]);
}

pub fn read_format_properties(xml: &str, sheet: &mut Sheet) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    sheet.default_row_height = number(&fragment, "defaultRowHeight");
    sheet.default_column_width = number(&fragment, "defaultColWidth");
    keep_extras(
        sheet,
        &fragment,
        &["defaultRowHeight", "defaultColWidth"],
        &[],
    );
}

pub fn read_columns(xml: &str, sheet: &mut Sheet, style_count: u32) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    for (_, child) in &fragment.children {
        let Some(col) = Fragment::parse(child) else {
            continue;
        };
        let (Some(min), Some(max)) = (number::<u32>(&col, "min"), number::<u32>(&col, "max"))
        else {
            continue;
        };
        if min == 0 || max < min {
            continue;
        }
        sheet.columns.push(ColumnInfo {
            first: min - 1,
            last: (max - 1).min(crate::address::MAX_COLS - 1),
            width: number(&col, "width"),
            hidden: flag(&col, "hidden"),
            style: number::<u32>(&col, "style").filter(|s| *s != 0 && *s < style_count),
            custom_width: flag(&col, "customWidth"),
            outline_level: number(&col, "outlineLevel"),
            collapsed: flag(&col, "collapsed"),
            best_fit: flag(&col, "bestFit"),
        });
    }
}

pub fn read_merges(xml: &str, sheet: &mut Sheet) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    for (_, child) in &fragment.children {
        if let Some(range) =
            Fragment::parse(child).and_then(|m| m.attribute("ref").and_then(Range::parse))
        {
            sheet.merged.push(range);
        }
    }
}

/// the elements of a rule that are values rather than details
const RULE_ATTRIBUTES: [&str; 6] = [
    "type",
    "operator",
    "dxfId",
    "priority",
    "stopIfTrue",
    "text",
];

pub fn read_conditional_format(xml: &str, sheet: &mut Sheet) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    let ranges = Range::parse_list(fragment.attribute("sqref").unwrap_or(""));
    let mut format = ConditionalFormat {
        ranges,
        rules: Vec::new(),
    };
    for (name, child) in &fragment.children {
        if name != "cfRule" {
            continue;
        }
        let Some(rule) = Fragment::parse(child) else {
            continue;
        };
        let formulas = rule
            .children
            .iter()
            .filter(|(n, _)| n == "formula")
            .filter_map(|(_, xml)| element_text(xml))
            .collect();
        let details: String = rule
            .children_except(&["formula"])
            .iter()
            .map(|(_, xml)| xml.as_str())
            .collect();
        format.rules.push(ConditionalRule {
            kind: rule.attribute("type").unwrap_or("expression").to_string(),
            operator: rule.attribute("operator").map(str::to_string),
            formulas,
            format: number(&rule, "dxfId"),
            priority: number(&rule, "priority").unwrap_or(1),
            stop_if_true: flag(&rule, "stopIfTrue"),
            text: rule.attribute("text").map(str::to_string),
            attributes: other_attributes(&rule, &RULE_ATTRIBUTES),
            details_xml: (!details.is_empty()).then_some(details),
        });
    }
    sheet.conditional_formats.push(format);
}

/// the text inside a small element such as `<formula>A1&gt;3</formula>`
pub fn element_text(xml: &str) -> Option<String> {
    let mut reader = crate::xml::Reader::new(xml);
    let tag = reader.root().ok()?;
    if tag.self_closing {
        return Some(String::new());
    }
    reader.element_text(&tag).ok()
}

pub fn read_validations(xml: &str, sheet: &mut Sheet) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    for (_, child) in &fragment.children {
        let Some(rule) = Fragment::parse(child) else {
            continue;
        };
        let formula = |name: &str| {
            rule.children
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, xml)| element_text(xml))
        };
        let text = |name: &str| rule.attribute(name).map(str::to_string);
        sheet.validations.push(DataValidation {
            ranges: Range::parse_list(rule.attribute("sqref").unwrap_or("")),
            kind: rule.attribute("type").unwrap_or("any").to_string(),
            operator: text("operator"),
            formula1: formula("formula1"),
            formula2: formula("formula2"),
            allow_blank: flag(&rule, "allowBlank"),
            hide_dropdown: flag(&rule, "showDropDown"),
            show_input_message: flag(&rule, "showInputMessage"),
            show_error_message: flag(&rule, "showErrorMessage"),
            error_style: text("errorStyle"),
            error_title: text("errorTitle"),
            error: text("error"),
            prompt_title: text("promptTitle"),
            prompt: text("prompt"),
        });
    }
}

/// hyperlinks, with the relationship id each external one points through
pub fn read_hyperlinks(xml: &str, sheet: &mut Sheet) -> Vec<Option<String>> {
    let mut ids = Vec::new();
    let Some(fragment) = Fragment::parse(xml) else {
        return ids;
    };
    for (_, child) in &fragment.children {
        let Some(link) = Fragment::parse(child) else {
            continue;
        };
        let Some(range) = link.attribute("ref").and_then(Range::parse) else {
            continue;
        };
        let text = |name: &str| link.attribute(name).map(str::to_string);
        sheet.hyperlinks.push(Hyperlink {
            range,
            target: None,
            location: text("location"),
            tooltip: text("tooltip"),
            display: text("display"),
        });
        ids.push(text("r:id"));
    }
    ids
}

pub fn read_auto_filter(xml: &str) -> Option<AutoFilter> {
    let fragment = Fragment::parse(xml)?;
    let range = fragment.attribute("ref").and_then(Range::parse)?;
    let mut filter = AutoFilter {
        range,
        columns: Vec::new(),
        sort: None,
    };
    for (name, child) in &fragment.children {
        match name.as_str() {
            "filterColumn" => {
                if let Some(column) = read_filter_column(child) {
                    filter.columns.push(column);
                }
            }
            "sortState" => filter.sort = read_sort_state(child),
            _ => {}
        }
    }
    Some(filter)
}

fn read_filter_column(xml: &str) -> Option<(u32, ColumnFilter)> {
    let fragment = Fragment::parse(xml)?;
    let column: u32 = number(&fragment, "colId")?;
    let other = ColumnFilter::Other {
        xml: xml.to_string(),
    };
    let Some((name, inner)) = fragment.children.first() else {
        return Some((column, other));
    };
    let inner = Fragment::parse(inner)?;
    let filter = match name.as_str() {
        "filters" => ColumnFilter::Values {
            values: inner
                .children
                .iter()
                .filter(|(n, _)| n == "filter")
                .filter_map(|(_, xml)| Fragment::parse(xml)?.attribute("val").map(str::to_string))
                .collect(),
            blank: flag(&inner, "blank"),
        },
        "customFilters" => ColumnFilter::Custom {
            conditions: inner
                .children
                .iter()
                .filter_map(|(_, xml)| {
                    let condition = Fragment::parse(xml)?;
                    let operator = condition
                        .attribute("operator")
                        .unwrap_or("equal")
                        .to_string();
                    Some((
                        operator,
                        condition.attribute("val").unwrap_or("").to_string(),
                    ))
                })
                .collect(),
            and: flag(&inner, "and"),
        },
        "top10" => ColumnFilter::Top {
            count: number(&inner, "val").unwrap_or(10.0),
            percent: flag(&inner, "percent"),
            bottom: inner.attribute("top") == Some("0"),
        },
        _ => other,
    };
    Some((column, filter))
}

fn read_sort_state(xml: &str) -> Option<SortState> {
    let fragment = Fragment::parse(xml)?;
    let range = fragment.attribute("ref").and_then(Range::parse)?;
    let keys = fragment
        .children
        .iter()
        .filter(|(name, _)| name == "sortCondition")
        .filter_map(|(_, xml)| {
            let condition = Fragment::parse(xml)?;
            let key = condition.attribute("ref").and_then(Range::parse)?;
            Some((key, flag(&condition, "descending")))
        })
        .collect();
    Some(SortState { range, keys })
}

pub fn read_print_options(xml: &str, sheet: &mut Sheet) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    sheet.print.center_horizontally = flag(&fragment, "horizontalCentered");
    sheet.print.center_vertically = flag(&fragment, "verticalCentered");
    sheet.print.grid_lines = flag(&fragment, "gridLines");
    sheet.print.headings = flag(&fragment, "headings");
}

pub fn read_margins(xml: &str, sheet: &mut Sheet) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    let defaults = Margins::default();
    sheet.print.margins = Some(Margins {
        left: number(&fragment, "left").unwrap_or(defaults.left),
        right: number(&fragment, "right").unwrap_or(defaults.right),
        top: number(&fragment, "top").unwrap_or(defaults.top),
        bottom: number(&fragment, "bottom").unwrap_or(defaults.bottom),
        header: number(&fragment, "header").unwrap_or(defaults.header),
        footer: number(&fragment, "footer").unwrap_or(defaults.footer),
    });
}

/// page setup attributes the model reads
pub const PAGE_SETUP_ATTRIBUTES: [&str; 5] = [
    "orientation",
    "paperSize",
    "scale",
    "fitToWidth",
    "fitToHeight",
];

pub fn read_page_setup(xml: &str, sheet: &mut Sheet) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    sheet.print.orientation = match fragment.attribute("orientation") {
        Some("portrait") => Orientation::Portrait,
        Some("landscape") => Orientation::Landscape,
        _ => Orientation::Default,
    };
    sheet.print.paper_size = number(&fragment, "paperSize");
    sheet.print.scale = number(&fragment, "scale");
    if let Some(fit) = sheet.print.fit_to.as_mut() {
        // a missing fitToWidth or fitToHeight means 1 (ecma-376 18.3.1.63)
        *fit = (
            number(&fragment, "fitToWidth").unwrap_or(1),
            number(&fragment, "fitToHeight").unwrap_or(1),
        );
    }
    keep_extras(sheet, &fragment, &PAGE_SETUP_ATTRIBUTES, &[]);
}

pub fn read_header_footer(xml: &str, sheet: &mut Sheet) {
    let Some(fragment) = Fragment::parse(xml) else {
        return;
    };
    for (name, child) in &fragment.children {
        let text = element_text(child);
        match name.as_str() {
            "oddHeader" => sheet.print.header = text,
            "oddFooter" => sheet.print.footer = text,
            _ => {}
        }
    }
    keep_extras(sheet, &fragment, &[], &["oddHeader", "oddFooter"]);
}

pub fn read_breaks(xml: &str) -> Vec<u32> {
    let Some(fragment) = Fragment::parse(xml) else {
        return Vec::new();
    };
    fragment
        .children
        .iter()
        .filter_map(|(_, child)| Fragment::parse(child)?.attribute("id")?.parse().ok())
        .collect()
}
