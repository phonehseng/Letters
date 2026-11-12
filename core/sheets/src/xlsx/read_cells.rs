//! the `<sheetData>` of a worksheet, read in one streaming pass: rows,
//! cells, values, formulas (shared formulas expanded to each cell) and
//! row settings. this is where a large file spends its time, so nothing
//! here builds a tree.

use std::collections::HashMap;

use crate::address::CellRef;
use crate::formula::{self, Expr};
use crate::model::{
    Cell, CellExtra, DateSystem, ErrorValue, Formula, FormulaKind, RowInfo, Sheet, Value,
};
use crate::xml::{Event, Reader, Tag};

/// one shared string: its plain text and, when it has formatted runs,
/// its original xml
#[derive(Clone, Debug, Default)]
pub struct SharedString {
    pub text: String,
    pub rich: Option<String>,
}

/// what reading cells needs to know about the rest of the workbook
pub struct CellContext<'a> {
    pub strings: &'a [SharedString],
    pub date_system: DateSystem,
    pub style_count: u32,
}

/// a shared formula's master: where it is and its parsed text
struct SharedMaster {
    at: CellRef,
    expr: Option<Expr>,
    text: String,
}

/// row attributes recomputed by excel and not worth keeping per row:
/// 200,000 copies of `x14ac:dyDescent` would cost megabytes
const DROPPED_ROW_ATTRIBUTES: [&str; 3] = ["spans", "x14ac:dyDescent", "dyDescent"];

pub fn read_sheet_data(
    reader: &mut Reader,
    sheet: &mut Sheet,
    context: &CellContext,
) -> Result<(), String> {
    let mut masters: HashMap<u32, SharedMaster> = HashMap::new();
    let mut next_row = 0u32;
    loop {
        match reader.next()? {
            Some(Event::Start(tag)) if tag.local() == "row" => {
                let row = tag
                    .attr("r")
                    .and_then(|r| r.parse::<u32>().ok())
                    .map(|r| r.saturating_sub(1))
                    .unwrap_or(next_row);
                read_row_info(&tag, row, sheet, context);
                read_row(reader, &tag, row, sheet, context, &mut masters)?;
                next_row = row + 1;
            }
            Some(Event::Start(tag)) => {
                reader.element_end(&tag)?;
            }
            Some(Event::End { .. }) | None => return Ok(()),
            Some(Event::Text(_)) => {}
        }
    }
}

fn read_row_info(tag: &Tag, row: u32, sheet: &mut Sheet, context: &CellContext) {
    let mut info = RowInfo::default();
    for (key, value) in tag.attributes() {
        match key {
            "r" => {}
            "ht" => info.height = value.parse().ok(),
            "customHeight" => info.custom_height = value == "1" || value == "true",
            "hidden" => info.hidden = value == "1" || value == "true",
            "s" => info.style = value.parse().ok().filter(|s| *s < context.style_count),
            "customFormat" => {}
            "outlineLevel" => info.outline_level = value.parse().ok(),
            "collapsed" => info.collapsed = value == "1" || value == "true",
            other if DROPPED_ROW_ATTRIBUTES.contains(&other) => {}
            other => info
                .attributes
                .push((other.to_string(), value.into_owned())),
        }
    }
    // a row style counts only when customFormat says so (ecma-376 18.3.1.73)
    let custom_format = tag
        .attr("customFormat")
        .is_some_and(|v| v == "1" || v == "true");
    if !custom_format {
        info.style = None;
    }
    if !info.is_default() {
        sheet.rows.insert(row, info);
    }
}

fn read_row(
    reader: &mut Reader,
    row_tag: &Tag,
    row: u32,
    sheet: &mut Sheet,
    context: &CellContext,
    masters: &mut HashMap<u32, SharedMaster>,
) -> Result<(), String> {
    if row_tag.self_closing {
        reader.next()?;
        return Ok(());
    }
    let mut next_col = 0u32;
    loop {
        match reader.next()? {
            Some(Event::Start(tag)) if tag.local() == "c" => {
                let at = tag
                    .attr("r")
                    .and_then(|r| CellRef::parse(&r))
                    .unwrap_or(CellRef::new(row, next_col));
                let cell = read_cell(reader, &tag, at, context, masters)?;
                next_col = at.col + 1;
                if !(cell.is_empty() && cell.style == 0 && cell.extra.is_none()) {
                    sheet.cells.set(at.row, at.col, cell);
                }
            }
            Some(Event::Start(tag)) => {
                reader.element_end(&tag)?;
            }
            Some(Event::End { .. }) | None => return Ok(()),
            Some(Event::Text(_)) => {}
        }
    }
}

/// what a `<c>` element holds before the value is typed
#[derive(Default)]
struct RawCell {
    kind: String,
    value: Option<String>,
    inline: Option<String>,
    formula: Option<(String, Vec<(String, String)>)>,
}

fn read_cell(
    reader: &mut Reader,
    tag: &Tag,
    at: CellRef,
    context: &CellContext,
    masters: &mut HashMap<u32, SharedMaster>,
) -> Result<Cell, String> {
    let mut cell = Cell::default();
    let mut raw = RawCell::default();
    let mut extra_attributes = Vec::new();
    for (key, value) in tag.attributes() {
        match key {
            "r" => {}
            "s" => {
                cell.style = value
                    .parse()
                    .ok()
                    .filter(|s| *s < context.style_count)
                    .unwrap_or(0)
            }
            "t" => raw.kind = value.into_owned(),
            other => extra_attributes.push((other.to_string(), value.into_owned())),
        }
    }
    if !tag.self_closing {
        read_cell_children(reader, &mut raw)?;
    } else {
        reader.next()?;
    }
    let mut rich = None;
    cell.value = typed_value(&raw, context, &mut rich);
    if let Some((text, attributes)) = raw.formula.take() {
        let (mut formula, formula_extra) = build_formula(text, attributes, at, masters);
        // excel 365 marks dynamic array formulas with cell metadata (cm);
        // the model says so in the formula itself and the writer adds it back
        if let Some(position) = extra_attributes.iter().position(|(key, _)| key == "cm") {
            if let Some(Formula {
                kind: kind @ FormulaKind::Array { .. },
                ..
            }) = formula.as_mut()
            {
                if let FormulaKind::Array { range } = *kind {
                    *kind = FormulaKind::Dynamic { spill: Some(range) };
                    extra_attributes.remove(position);
                }
            }
        }
        cell.formula = formula;
        if !formula_extra.is_empty() || !extra_attributes.is_empty() || rich.is_some() {
            cell.extra = Some(Box::new(CellExtra {
                rich_text: rich,
                attributes: extra_attributes,
                formula_attributes: formula_extra,
            }));
        }
    } else if !extra_attributes.is_empty() || rich.is_some() {
        cell.extra = Some(Box::new(CellExtra {
            rich_text: rich,
            attributes: extra_attributes,
            formula_attributes: Vec::new(),
        }));
    }
    Ok(cell)
}

fn read_cell_children(reader: &mut Reader, raw: &mut RawCell) -> Result<(), String> {
    loop {
        match reader.next()? {
            Some(Event::Start(child)) => match child.local() {
                "v" => raw.value = Some(reader.element_text(&child)?),
                "is" => raw.inline = Some(reader.element_text(&child)?),
                "f" => {
                    let attributes = child.owned_attributes();
                    let text = if child.self_closing {
                        reader.next()?;
                        String::new()
                    } else {
                        reader.element_text(&child)?
                    };
                    raw.formula = Some((text, attributes));
                }
                _ => {
                    reader.element_end(&child)?;
                }
            },
            Some(Event::End { .. }) | None => return Ok(()),
            Some(Event::Text(_)) => {}
        }
    }
}

fn typed_value(raw: &RawCell, context: &CellContext, rich: &mut Option<String>) -> Value {
    let text = raw.value.as_deref().unwrap_or("");
    match raw.kind.as_str() {
        "s" => match text
            .trim()
            .parse::<usize>()
            .ok()
            .and_then(|i| context.strings.get(i))
        {
            Some(shared) => {
                rich.clone_from(&shared.rich);
                Value::Text(shared.text.clone())
            }
            None => Value::Blank,
        },
        "str" => Value::Text(text.to_string()),
        "inlineStr" => Value::Text(
            raw.inline
                .clone()
                .or_else(|| raw.value.clone())
                .unwrap_or_default(),
        ),
        "b" => Value::Bool(text.trim() == "1" || text.trim().eq_ignore_ascii_case("true")),
        "e" => Value::Error(ErrorValue::parse(text).unwrap_or(ErrorValue::Value)),
        "d" => crate::dates::serial_from_iso(text.trim(), context.date_system)
            .map(Value::Number)
            .unwrap_or_else(|| Value::Text(text.to_string())),
        _ if raw.value.is_none() => Value::Blank,
        _ => text
            .trim()
            .parse::<f64>()
            .map(Value::Number)
            .unwrap_or_else(|_| Value::Text(text.to_string())),
    }
}

/// excel writes newer functions as "_xlfn.XLOOKUP"; the model keeps the
/// name people type. other formulas keep their exact text.
pub fn canonical_formula_text(text: &str) -> String {
    if !text.contains("_xl") && !text.contains("_XL") {
        return text.to_string();
    }
    match formula::parse(text) {
        Ok(expr) => formula::to_string(&expr),
        Err(_) => text.to_string(),
    }
}

fn attribute<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// formula attributes the model reads; the rest is kept on the cell
const FORMULA_ATTRIBUTES: [&str; 3] = ["t", "ref", "si"];

fn build_formula(
    text: String,
    attributes: Vec<(String, String)>,
    at: CellRef,
    masters: &mut HashMap<u32, SharedMaster>,
) -> (Option<Formula>, Vec<(String, String)>) {
    let kind_name = attribute(&attributes, "t").unwrap_or("normal").to_string();
    let range = attribute(&attributes, "ref").and_then(crate::address::Range::parse);
    let group = attribute(&attributes, "si").and_then(|si| si.parse::<u32>().ok());
    if kind_name == "dataTable" {
        let formula = Formula {
            text,
            kind: FormulaKind::DataTable { attributes },
        };
        return (Some(formula), Vec::new());
    }
    let extra: Vec<(String, String)> = attributes
        .iter()
        .filter(|(key, _)| !FORMULA_ATTRIBUTES.contains(&key.as_str()))
        .cloned()
        .collect();
    let formula = match (kind_name.as_str(), group) {
        ("shared", Some(group)) => shared_formula(text, at, group, range, masters),
        ("array", _) => Some(Formula {
            text: canonical_formula_text(&text),
            kind: FormulaKind::Array {
                range: range.unwrap_or(crate::address::Range::cell(at)),
            },
        }),
        _ if text.is_empty() => None,
        _ => Some(Formula::legacy(canonical_formula_text(&text))),
    };
    (formula, extra)
}

/// a cell of a shared formula group: the master defines the text, every
/// other cell gets it translated to its own position
fn shared_formula(
    text: String,
    at: CellRef,
    group: u32,
    range: Option<crate::address::Range>,
    masters: &mut HashMap<u32, SharedMaster>,
) -> Option<Formula> {
    if !text.is_empty() {
        let canonical = canonical_formula_text(&text);
        masters.insert(
            group,
            SharedMaster {
                at,
                expr: formula::parse(&canonical).ok(),
                text: canonical.clone(),
            },
        );
        return Some(Formula {
            text: canonical,
            kind: FormulaKind::Shared { group, range },
        });
    }
    let master = masters.get(&group)?;
    let text = match &master.expr {
        Some(expr) => {
            let rows = i64::from(at.row) - i64::from(master.at.row);
            let cols = i64::from(at.col) - i64::from(master.at.col);
            formula::to_string(&formula::refs::translate(expr, rows, cols))
        }
        None => master.text.clone(),
    };
    Some(Formula {
        text,
        kind: FormulaKind::Shared { group, range: None },
    })
}

/// the shared string table (`xl/sharedStrings.xml`)
pub fn read_shared_strings(xml: &str) -> Result<Vec<SharedString>, String> {
    let mut reader = Reader::new(xml);
    reader.root()?;
    let mut strings = Vec::new();
    loop {
        match reader.next()? {
            Some(Event::Start(tag)) if tag.local() == "si" => {
                let source = reader.source();
                let start = tag.start;
                let text = reader.element_text(&tag)?;
                let end = source[start..].find("</si>").map(|i| start + i + 5);
                let raw = end.map(|end| &source[start..end]).unwrap_or("");
                let rich = raw.contains("<r>") || raw.contains("<r ") || raw.contains(":r>");
                strings.push(SharedString {
                    text,
                    rich: rich.then(|| raw.to_string()),
                });
            }
            Some(Event::Start(tag)) => {
                reader.element_end(&tag)?;
            }
            Some(Event::End { .. }) | None => return Ok(strings),
            Some(Event::Text(_)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(xml: &str, strings: &[SharedString]) -> Sheet {
        let mut sheet = Sheet::new("S");
        let mut reader = Reader::new(xml);
        reader.root().unwrap();
        let context = CellContext {
            strings,
            date_system: DateSystem::Excel1900,
            style_count: 4,
        };
        read_sheet_data(&mut reader, &mut sheet, &context).unwrap();
        sheet
    }

    #[test]
    fn reads_every_value_type() {
        let strings = vec![SharedString {
            text: "hello".into(),
            rich: None,
        }];
        let sheet = read(
            r#"<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>1.5</v></c><c r="C1" t="b"><v>1</v></c><c r="D1" t="e"><v>#N/A</v></c><c r="E1" t="inlineStr"><is><t>in</t></is></c><c r="F1" s="2"/></row><row r="3" ht="30" customHeight="1"><c r="A3" t="str"><f>"a"&amp;"b"</f><v>ab</v></c></row></sheetData>"#,
            &strings,
        );
        assert_eq!(sheet.cells.value(0, 0), &Value::text("hello"));
        assert_eq!(sheet.cells.value(0, 1), &Value::Number(1.5));
        assert_eq!(sheet.cells.value(0, 2), &Value::Bool(true));
        assert_eq!(sheet.cells.value(0, 3), &Value::Error(ErrorValue::NA));
        assert_eq!(sheet.cells.value(0, 4), &Value::text("in"));
        assert_eq!(sheet.cells.get(0, 5).unwrap().style, 2);
        assert_eq!(
            sheet
                .cells
                .get(2, 0)
                .unwrap()
                .formula
                .as_ref()
                .unwrap()
                .text,
            "\"a\"&\"b\""
        );
        assert_eq!(sheet.rows[&2].height, Some(30.0));
    }

    #[test]
    fn shared_formulas_are_expanded_to_each_cell() {
        let sheet = read(
            r#"<sheetData><row r="1"><c r="B1"><f t="shared" ref="B1:B3" si="0">A1*2+_xlfn.IFNA($C$1,0)</f><v>0</v></c></row><row r="2"><c r="B2"><f t="shared" si="0"/><v>0</v></c></row><row r="3"><c r="B3"><f t="shared" si="0"/></c></row></sheetData>"#,
            &[],
        );
        let text = |row| {
            sheet
                .cells
                .get(row, 1)
                .unwrap()
                .formula
                .as_ref()
                .unwrap()
                .text
                .clone()
        };
        assert_eq!(text(0), "A1*2+IFNA($C$1,0)");
        assert_eq!(text(1), "A2*2+IFNA($C$1,0)");
        assert_eq!(text(2), "A3*2+IFNA($C$1,0)");
    }

    #[test]
    fn rich_shared_strings_keep_their_runs() {
        let strings = read_shared_strings(
            r#"<sst xmlns="x" count="2"><si><t>plain</t></si><si><r><rPr><b/></rPr><t>bo</t></r><r><t>ld</t></r><rPh><t>x</t></rPh></si></sst>"#,
        )
        .unwrap();
        assert_eq!(strings[0].text, "plain");
        assert!(strings[0].rich.is_none());
        assert_eq!(strings[1].text, "bold");
        assert!(strings[1].rich.as_ref().unwrap().starts_with("<si><r>"));
    }
}
