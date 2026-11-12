//! reading a whole ods package: styles, every table, named ranges,
//! validations, document properties and frozen panes. parts the model
//! does not read (pictures, charts as embedded objects, thumbnails,
//! settings) are carried byte for byte.

use std::collections::HashMap;

use crate::address::Range;
use crate::error::SheetError;
use crate::model::carry::{CarriedPart, Section, SourceFormat};
use crate::model::*;
use crate::package::Package;
use crate::xml::{Event, Fragment, Reader};

use super::read_table::{read_table, TableContext};
use super::styles::StyleSheet;

/// parts rebuilt on save, so not carried
const REBUILT: [&str; 6] = [
    "mimetype",
    "content.xml",
    "styles.xml",
    "meta.xml",
    "settings.xml",
    "META-INF/manifest.xml",
];

pub fn read_ods(bytes: &[u8]) -> Result<Workbook, SheetError> {
    let package = Package::read(bytes)?;
    let content = package
        .text("content.xml")?
        .ok_or_else(|| SheetError::NotASpreadsheet("the content part is missing".into()))?;
    let mut book = Workbook::new();
    book.sheets.clear();
    book.carry.source = Some(SourceFormat::Ods);
    let mut styles = StyleSheet::default();
    if let Some(xml) = package.text("styles.xml")? {
        read_style_part(&xml, &mut styles);
        book.carry.ods_sections.push(Section {
            name: "styles.xml".into(),
            xml,
            generated: None,
        });
    }
    read_content(&content, &mut styles, &mut book)
        .map_err(|detail| SheetError::bad_xml("content.xml", detail))?;
    if let Some(xml) = package.text("settings.xml")? {
        read_settings(&xml, &mut book);
        book.carry.ods_sections.push(Section {
            name: "settings.xml".into(),
            xml,
            generated: None,
        });
    }
    if let Some(xml) = package.text("meta.xml")? {
        read_meta(&xml, &mut book.metadata);
    }
    if let Some(xml) = package.text("META-INF/manifest.xml")? {
        book.carry.ods_sections.push(Section {
            name: "manifest".into(),
            xml,
            generated: None,
        });
    }
    for part in &package.parts {
        if !REBUILT.contains(&part.name.as_str()) {
            book.carry.parts.push(CarriedPart {
                name: part.name.clone(),
                bytes: part.bytes.clone(),
            });
        }
    }
    if book.sheets.is_empty() {
        book.sheets.push(Sheet::new("Sheet1"));
    }
    Ok(book)
}

fn read_style_part(xml: &str, styles: &mut StyleSheet) {
    let mut reader = Reader::new(xml);
    while let Ok(Some(event)) = reader.next() {
        let Event::Start(tag) = event else { continue };
        if matches!(tag.local(), "styles" | "automatic-styles") {
            if let Ok(raw) = reader.skip_element(&tag) {
                styles.add_all(raw);
            }
        }
    }
}

fn read_content(xml: &str, styles: &mut StyleSheet, book: &mut Workbook) -> Result<(), String> {
    let mut reader = Reader::new(xml);
    let root = reader.root()?;
    book.carry.workbook_root_attributes = root.owned_attributes();
    let mut style_index = HashMap::new();
    let mut validations: HashMap<String, Vec<(usize, Range)>> = HashMap::new();
    let mut rules: Vec<(String, DataValidation)> = Vec::new();
    while let Some(event) = reader.next()? {
        let Event::Start(tag) = event else { continue };
        match tag.local() {
            "automatic-styles" => styles.add_all(reader.skip_element(&tag)?),
            "body" | "spreadsheet" => {}
            "table" => {
                let index = book.sheets.len();
                let mut context = TableContext {
                    styles,
                    style_index: &mut style_index,
                    table: &mut book.styles,
                    date_system: book.date_system,
                    validations: &mut validations,
                };
                let sheet = read_table(&mut reader, &tag, index, &mut context)?;
                book.sheets.push(sheet);
            }
            "named-expressions" => book
                .defined_names
                .extend(read_names(reader.skip_element(&tag)?)),
            "content-validations" => rules = read_validations(reader.skip_element(&tag)?),
            "calculation-settings" => {
                let raw = reader.skip_element(&tag)?;
                // the 1904 date system is a null date of 1904-01-01
                if raw.contains("table:date-value=\"1904") {
                    book.date_system = DateSystem::Excel1904;
                }
                book.carry.workbook_sections.push(Section {
                    name: "calculation-settings".into(),
                    xml: raw.to_string(),
                    generated: None,
                });
            }
            name => {
                let raw = reader.skip_element(&tag)?;
                book.carry.workbook_sections.push(Section {
                    name: name.to_string(),
                    xml: raw.to_string(),
                    generated: None,
                });
            }
        }
    }
    attach_validations(book, rules, validations);
    Ok(())
}

/// "$Sheet1.$A$1:.$B$2" as excel's "Sheet1!$A$1:$B$2"
fn range_address(address: &str) -> Option<String> {
    let expr = crate::formula::parse_open_formula(&format!("[{}]", address.trim())).ok()?;
    Some(crate::formula::to_string(&expr))
}

fn read_names(xml: &str) -> Vec<DefinedName> {
    let Some(fragment) = Fragment::parse(xml) else {
        return Vec::new();
    };
    fragment
        .children
        .iter()
        .filter_map(|(kind, child)| {
            let item = Fragment::parse(child)?;
            let name = item.attribute("table:name")?.to_string();
            let formula = match kind.as_str() {
                "named-range" => range_address(item.attribute("table:cell-range-address")?)?,
                "named-expression" => {
                    let expression = item.attribute("table:expression")?;
                    crate::formula::to_string(&crate::formula::parse_open_formula(expression).ok()?)
                }
                _ => return None,
            };
            Some(DefinedName {
                name,
                formula,
                ..DefinedName::default()
            })
        })
        .collect()
}

/// validation rules by name; the cells using each are found while
/// reading the tables
fn read_validations(xml: &str) -> Vec<(String, DataValidation)> {
    let Some(fragment) = Fragment::parse(xml) else {
        return Vec::new();
    };
    fragment
        .children
        .iter()
        .filter_map(|(_, child)| {
            let item = Fragment::parse(child)?;
            let name = item.attribute("table:name")?.to_string();
            let condition = item.attribute("table:condition").unwrap_or("");
            let mut rule = validation_from_condition(condition);
            rule.allow_blank = item.attribute("table:allow-empty-cell") != Some("false");
            rule.hide_dropdown = item.attribute("table:display-list") == Some("none");
            rule.show_error_message = true;
            Some((name, rule))
        })
        .collect()
}

/// the conditions libreoffice writes: lists, numbers between, comparisons
pub fn validation_from_condition(condition: &str) -> DataValidation {
    let body = condition
        .trim_start_matches("of:")
        .trim_start_matches("oooc:");
    let mut rule = DataValidation {
        kind: "custom".into(),
        ..DataValidation::default()
    };
    if let Some(list) = body
        .strip_prefix("cell-content-is-in-list(")
        .and_then(|b| b.strip_suffix(')'))
    {
        rule.kind = "list".into();
        rule.formula1 = Some(if list.starts_with('"') {
            let items: Vec<String> = list
                .split(';')
                .map(|item| item.trim().trim_matches('"').to_string())
                .collect();
            format!("\"{}\"", items.join(","))
        } else {
            crate::formula::parse_open_formula(list)
                .map(|e| crate::formula::to_string(&e))
                .unwrap_or_else(|_| list.to_string())
        });
        return rule;
    }
    for (prefix, kind) in [
        ("cell-content-is-whole-number()", "whole"),
        ("cell-content-is-decimal-number()", "decimal"),
        ("cell-content-is-date()", "date"),
        ("cell-content-is-time()", "time"),
    ] {
        if let Some(rest) = body.strip_prefix(prefix) {
            rule.kind = kind.into();
            read_bounds(rest.trim_start_matches(" and ").trim(), &mut rule);
            return rule;
        }
    }
    if let Some(rest) = body.strip_prefix("cell-content-text-length()") {
        rule.kind = "textLength".into();
        read_comparison(rest, &mut rule);
        return rule;
    }
    rule.formula1 = Some(body.to_string());
    rule
}

fn excel_formula(open: &str) -> String {
    crate::formula::parse_open_formula(open)
        .map(|e| crate::formula::to_string(&e))
        .unwrap_or_else(|_| open.to_string())
}

fn read_bounds(rest: &str, rule: &mut DataValidation) {
    let between = rest
        .strip_prefix("cell-content-is-between(")
        .map(|r| ("between", r))
        .or_else(|| {
            rest.strip_prefix("cell-content-is-not-between(")
                .map(|r| ("notBetween", r))
        });
    if let Some((operator, inner)) = between {
        let inner = inner.trim_end_matches(')');
        if let Some((low, high)) = inner.split_once(';') {
            rule.operator = Some(operator.into());
            rule.formula1 = Some(excel_formula(low));
            rule.formula2 = Some(excel_formula(high));
        }
        return;
    }
    read_comparison(rest.trim_start_matches("cell-content()"), rule);
}

fn read_comparison(rest: &str, rule: &mut DataValidation) {
    for (symbol, operator) in [
        ("<=", "lessThanOrEqual"),
        (">=", "greaterThanOrEqual"),
        ("!=", "notEqual"),
        ("<", "lessThan"),
        (">", "greaterThan"),
        ("=", "equal"),
    ] {
        if let Some(value) = rest.trim().strip_prefix(symbol) {
            rule.operator = Some(operator.into());
            rule.formula1 = Some(excel_formula(value));
            return;
        }
    }
}

fn attach_validations(
    book: &mut Workbook,
    rules: Vec<(String, DataValidation)>,
    used: HashMap<String, Vec<(usize, Range)>>,
) {
    for (name, rule) in rules {
        let Some(cells) = used.get(&name) else {
            continue;
        };
        let mut by_sheet: HashMap<usize, Vec<Range>> = HashMap::new();
        for (sheet, range) in cells {
            by_sheet.entry(*sheet).or_default().push(*range);
        }
        for (sheet, ranges) in by_sheet {
            if let Some(target) = book.sheets.get_mut(sheet) {
                target.validations.push(DataValidation {
                    ranges: super::merge_cells_into_ranges(ranges),
                    ..rule.clone()
                });
            }
        }
    }
}

/// frozen panes and the active sheet from settings.xml
fn read_settings(xml: &str, book: &mut Workbook) {
    let mut reader = Reader::new(xml);
    let mut table: Option<usize> = None;
    let mut item = String::new();
    let mut split: HashMap<String, f64> = HashMap::new();
    while let Ok(Some(event)) = reader.next() {
        match event {
            Event::Start(tag) if tag.local() == "config-item-map-entry" => {
                table = tag
                    .attr("config:name")
                    .and_then(|name| book.sheet_index(&name));
                split.clear();
            }
            Event::Start(tag) if tag.local() == "config-item" => {
                item = tag
                    .attr("config:name")
                    .map(|n| n.into_owned())
                    .unwrap_or_default()
            }
            Event::Text(text) => {
                if item == "ActiveTable" {
                    book.active_sheet = book.sheet_index(text.trim()).unwrap_or(0);
                }
                if let Ok(value) = text.trim().parse::<f64>() {
                    split.insert(item.clone(), value);
                }
            }
            Event::End {
                local: "config-item-map-entry",
                ..
            } => apply_split(book, table.take(), &split),
            Event::End {
                local: "config-item",
                ..
            } => item.clear(),
            _ => {}
        }
    }
}

/// split mode 2 means frozen (libreoffice's settings)
fn apply_split(book: &mut Workbook, table: Option<usize>, split: &HashMap<String, f64>) {
    let Some(sheet) = table.and_then(|t| book.sheets.get_mut(t)) else {
        return;
    };
    let frozen = |mode: &str, position: &str| -> u32 {
        if split.get(mode).copied() == Some(2.0) {
            split.get(position).copied().unwrap_or(0.0) as u32
        } else {
            0
        }
    };
    let cols = frozen("HorizontalSplitMode", "HorizontalSplitPosition");
    let rows = frozen("VerticalSplitMode", "VerticalSplitPosition");
    if rows > 0 || cols > 0 {
        sheet.view.frozen = Some(FrozenPanes { rows, cols });
    }
    if let Some(zoom) = split.get("ZoomValue") {
        sheet.view.zoom = *zoom as u32;
    }
}

fn read_meta(xml: &str, metadata: &mut Metadata) {
    let mut reader = Reader::new(xml);
    while let Ok(Some(event)) = reader.next() {
        let Event::Start(tag) = event else { continue };
        let slot = match tag.local() {
            "title" => &mut metadata.title,
            "subject" => &mut metadata.subject,
            "initial-creator" => &mut metadata.creator,
            "keyword" => &mut metadata.keywords,
            "description" => &mut metadata.description,
            "creator" => &mut metadata.last_modified_by,
            "creation-date" => &mut metadata.created,
            "date" => &mut metadata.modified,
            _ => continue,
        };
        *slot = reader.element_text(&tag).ok().filter(|t| !t.is_empty());
    }
}
