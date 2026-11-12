//! one `<table:table>` of an ods file: columns, rows and cells with their
//! repeats, values, formulas (openformula turned into excel syntax),
//! merged ranges, notes and links. elements the model does not read
//! (shapes, forms) are kept as their xml; conditional formats are read
//! for the screen and kept as their xml for saving.

use std::collections::HashMap;

use crate::address::{Range, MAX_COLS, MAX_ROWS};
use crate::model::carry::Section;
use crate::model::*;
use crate::xml::{Event, Reader, Tag};

use super::read_conditional::{read_conditional_formats, ConditionalContext};
use super::styles::StyleSheet;

/// what reading tables needs from the rest of the file
pub struct TableContext<'a> {
    pub styles: &'a StyleSheet,
    /// opendocument cell style name -> model style index
    pub style_index: &'a mut HashMap<String, u32>,
    pub table: &'a mut StyleTable,
    pub date_system: DateSystem,
    /// validation name -> cells using it
    pub validations: &'a mut HashMap<String, Vec<(usize, Range)>>,
}

impl TableContext<'_> {
    fn style(&mut self, name: Option<&str>) -> u32 {
        let Some(name) = name else { return 0 };
        if name == "Default" {
            return 0;
        }
        if let Some(index) = self.style_index.get(name) {
            return *index;
        }
        let index = self.table.intern(self.styles.cell_style(name));
        self.style_index.insert(name.to_string(), index);
        index
    }
}

/// a repeated empty run longer than this is not stored cell by cell
const MAX_STORED_REPEAT: u32 = 1000;

/// the cursor while reading one table
struct Position {
    row: u32,
    /// each column's default cell style
    column_styles: Vec<(u32, u32, Option<String>)>,
}

pub fn read_table(
    reader: &mut Reader,
    tag: &Tag,
    index: usize,
    context: &mut TableContext,
) -> Result<Sheet, String> {
    let mut sheet = Sheet::new(
        tag.attr("table:name")
            .map(|n| n.into_owned())
            .unwrap_or_else(|| format!("Sheet{}", index + 1)),
    );
    sheet.carry.source = Some(SourceFormat::Ods);
    sheet.carry.ods_attributes = tag.owned_attributes();
    if let Some(style) = tag.attr("table:style-name") {
        read_table_style(&style, context.styles, &mut sheet);
    }
    let mut position = Position {
        row: 0,
        column_styles: Vec::new(),
    };
    read_children(reader, &mut sheet, index, &mut position, context)?;
    Ok(sheet)
}

fn read_table_style(name: &str, styles: &StyleSheet, sheet: &mut Sheet) {
    if styles
        .property(name, "table-properties", "table:display")
        .as_deref()
        == Some("false")
    {
        sheet.visibility = Visibility::Hidden;
    }
    if let Some(color) = styles.property(name, "table-properties", "tableooo:tab-color") {
        sheet.view.tab_color = Some(Color::rgb(&color));
    }
}

/// table children, recursing into the groups rows and columns can sit in
fn read_children(
    reader: &mut Reader,
    sheet: &mut Sheet,
    index: usize,
    position: &mut Position,
    context: &mut TableContext,
) -> Result<(), String> {
    loop {
        let tag = match reader.next()? {
            Some(Event::Start(tag)) => tag,
            Some(Event::End { .. }) | None => return Ok(()),
            Some(Event::Text(_)) => continue,
        };
        match tag.local() {
            "table-column" => {
                read_column(&tag, sheet, position, context);
                reader.element_end(&tag)?;
            }
            "table-row" => read_row(reader, &tag, sheet, index, position, context)?,
            "table-columns"
            | "table-header-columns"
            | "table-column-group"
            | "table-rows"
            | "table-header-rows"
            | "table-row-group" => {
                if tag.self_closing {
                    reader.next()?;
                } else {
                    read_children(reader, sheet, index, position, context)?;
                }
            }
            name => {
                let raw = reader.skip_element(&tag)?;
                if name == "conditional-formats" {
                    // read for the screen; the xml below is what ods saving writes
                    let mut conditional = ConditionalContext {
                        styles: context.styles,
                        table: context.table,
                        made: HashMap::new(),
                        priority: 0,
                    };
                    let formats = read_conditional_formats(raw, &mut conditional);
                    sheet.conditional_formats.extend(formats);
                }
                sheet.carry.sections.push(Section {
                    name: name.to_string(),
                    xml: raw.to_string(),
                    generated: None,
                });
            }
        }
    }
}

fn repeat(tag: &Tag, name: &str) -> u32 {
    tag.attr(name)
        .and_then(|n| n.parse().ok())
        .unwrap_or(1)
        .max(1)
}

fn read_column(tag: &Tag, sheet: &mut Sheet, position: &mut Position, context: &mut TableContext) {
    let first = position
        .column_styles
        .last()
        .map(|(_, last, _)| last + 1)
        .unwrap_or(0);
    let count = repeat(tag, "table:number-columns-repeated")
        .min(MAX_COLS.saturating_sub(first))
        .max(1);
    let last = (first + count - 1).min(MAX_COLS - 1);
    position.column_styles.push((
        first,
        last,
        tag.attr("table:default-cell-style-name")
            .map(|s| s.into_owned()),
    ));
    let width = tag
        .attr("table:style-name")
        .and_then(|style| {
            context
                .styles
                .property(&style, "table-column-properties", "style:column-width")
        })
        .and_then(|w| super::units::points(&w))
        .map(super::units::characters);
    let hidden = tag.attr("table:visibility").is_some_and(|v| v != "visible");
    if width.is_some() || hidden {
        sheet.columns.push(ColumnInfo {
            first,
            last,
            width,
            hidden,
            custom_width: width.is_some(),
            ..ColumnInfo::default()
        });
    }
}

/// one parsed cell before it is stored (repeats are applied by the caller)
struct OdsCell {
    cell: Cell,
    spans: (u32, u32),
    validation: Option<String>,
    comment: Option<(String, String)>,
    link: Option<String>,
}

fn read_row(
    reader: &mut Reader,
    tag: &Tag,
    sheet: &mut Sheet,
    index: usize,
    position: &mut Position,
    context: &mut TableContext,
) -> Result<(), String> {
    let count = repeat(tag, "table:number-rows-repeated");
    let first = position.row;
    let mut info = RowInfo::default();
    if let Some(style) = tag.attr("table:style-name") {
        info.height = context
            .styles
            .property(&style, "table-row-properties", "style:row-height")
            .and_then(|h| super::units::points(&h));
        info.custom_height = context
            .styles
            .property(
                &style,
                "table-row-properties",
                "style:use-optimal-row-height",
            )
            .as_deref()
            == Some("false");
    }
    info.hidden = tag.attr("table:visibility").is_some_and(|v| v != "visible");
    let cells = if tag.self_closing {
        reader.next()?;
        Vec::new()
    } else {
        read_cells(reader, position, context)?
    };
    let has_content = cells
        .iter()
        .any(|(_, _, cell)| !cell.cell.is_empty() || cell.cell.style != 0);
    let stored_rows = if has_content || count <= MAX_STORED_REPEAT {
        count
    } else {
        1
    };
    for offset in 0..stored_rows.min(MAX_ROWS.saturating_sub(first)) {
        let row = first + offset;
        if info.height.is_some() || info.hidden {
            sheet.rows.insert(row, info.clone());
        }
        for (col, columns, cell) in &cells {
            place_cell(
                sheet,
                index,
                row,
                *col,
                *columns,
                cell,
                context,
                offset == 0,
            );
        }
    }
    position.row = first.saturating_add(count).min(MAX_ROWS);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn place_cell(
    sheet: &mut Sheet,
    index: usize,
    row: u32,
    col: u32,
    columns: u32,
    cell: &OdsCell,
    context: &mut TableContext,
    first_row: bool,
) {
    let stored = if cell.cell.is_empty() && columns > MAX_STORED_REPEAT {
        1
    } else {
        columns
    };
    for offset in 0..stored.min(MAX_COLS.saturating_sub(col)) {
        let at = col + offset;
        if !cell.cell.is_empty() || cell.cell.style != 0 {
            let mut stored_cell = cell.cell.clone();
            // a matrix formula's size was read without its position
            if let Some(Formula {
                kind: FormulaKind::Array { range },
                ..
            }) = stored_cell.formula.as_mut()
            {
                *range = Range::from_bounds(row, at, row + range.end.row, at + range.end.col);
            }
            sheet.cells.set(row, at, stored_cell);
        }
        if let Some(name) = &cell.validation {
            context
                .validations
                .entry(name.clone())
                .or_default()
                .push((index, Range::cell(crate::address::CellRef::new(row, at))));
        }
        if let Some((author, text)) = &cell.comment {
            sheet.comments.push(Comment {
                row,
                col: at,
                author: author.clone(),
                text: text.clone(),
            });
        }
        if let Some(target) = &cell.link {
            let range = Range::cell(crate::address::CellRef::new(row, at));
            let location = target.strip_prefix('#').map(str::to_string);
            let target = location.is_none().then(|| target.clone());
            sheet.hyperlinks.push(Hyperlink {
                range,
                target,
                location,
                ..Hyperlink::default()
            });
        }
    }
    let (span_cols, span_rows) = cell.spans;
    if first_row && (span_cols > 1 || span_rows > 1) {
        sheet.merged.push(Range::from_bounds(
            row,
            col,
            row + span_rows - 1,
            col + span_cols - 1,
        ));
    }
}

fn read_cells(
    reader: &mut Reader,
    position: &Position,
    context: &mut TableContext,
) -> Result<Vec<(u32, u32, OdsCell)>, String> {
    let mut cells = Vec::new();
    let mut col = 0u32;
    loop {
        let tag = match reader.next()? {
            Some(Event::Start(tag)) => tag,
            Some(Event::End { .. }) | None => return Ok(cells),
            Some(Event::Text(_)) => continue,
        };
        let count = repeat(&tag, "table:number-columns-repeated");
        let covered = tag.local() == "covered-table-cell";
        let default_style = position
            .column_styles
            .iter()
            .find(|(a, b, _)| col >= *a && col <= *b)
            .and_then(|(_, _, s)| s.clone());
        let parsed = read_cell(reader, &tag, default_style, context)?;
        if !covered || !parsed.cell.is_empty() {
            cells.push((col, count, parsed));
        }
        col = col.saturating_add(count).min(MAX_COLS);
    }
}

fn read_cell(
    reader: &mut Reader,
    tag: &Tag,
    default_style: Option<String>,
    context: &mut TableContext,
) -> Result<OdsCell, String> {
    let style_name = tag
        .attr("table:style-name")
        .map(|s| s.into_owned())
        .or(default_style);
    let mut cell = Cell {
        style: context.style(style_name.as_deref()),
        ..Cell::default()
    };
    let content = if tag.self_closing {
        reader.next()?;
        CellContent::default()
    } else {
        read_content(reader)?
    };
    cell.value = cell_value(tag, &content, context.date_system);
    if let Some(formula) = tag.attr("table:formula") {
        cell.formula = read_formula(&formula, tag);
    }
    let spans = (
        repeat(tag, "table:number-columns-spanned"),
        repeat(tag, "table:number-rows-spanned"),
    );
    Ok(OdsCell {
        cell,
        spans,
        validation: tag
            .attr("table:content-validation-name")
            .map(|v| v.into_owned()),
        comment: content.comment,
        link: content.link,
    })
}

fn read_formula(text: &str, tag: &Tag) -> Option<Formula> {
    let excel = match crate::formula::parse_open_formula(text) {
        Ok(expr) => crate::formula::to_string(&expr),
        // kept as written, so the cell is reported as not recalculated
        Err(_) => text
            .trim_start_matches("of:")
            .trim_start_matches('=')
            .to_string(),
    };
    let columns = repeat(tag, "table:number-matrix-columns-spanned");
    let rows = repeat(tag, "table:number-matrix-rows-spanned");
    let kind = if tag.attr("table:number-matrix-columns-spanned").is_some() {
        FormulaKind::Array {
            range: Range::from_bounds(0, 0, rows - 1, columns - 1),
        }
    } else {
        FormulaKind::Normal
    };
    Some(Formula { text: excel, kind })
}

/// the text, note and link inside a cell
#[derive(Default)]
struct CellContent {
    paragraphs: Vec<String>,
    comment: Option<(String, String)>,
    link: Option<String>,
}

fn read_content(reader: &mut Reader) -> Result<CellContent, String> {
    let mut content = CellContent::default();
    loop {
        let tag = match reader.next()? {
            Some(Event::Start(tag)) => tag,
            Some(Event::End { .. }) | None => return Ok(content),
            Some(Event::Text(_)) => continue,
        };
        match tag.local() {
            "p" => content
                .paragraphs
                .push(paragraph_text(reader, &tag, &mut content.link)?),
            "annotation" => content.comment = Some(annotation(reader)?),
            _ => {
                reader.element_end(&tag)?;
            }
        }
    }
}

/// a text:p's text: text:s is spaces, text:tab a tab, text:line-break a new line
fn paragraph_text(
    reader: &mut Reader,
    tag: &Tag,
    link: &mut Option<String>,
) -> Result<String, String> {
    let mut text = String::new();
    if tag.self_closing {
        reader.next()?;
        return Ok(text);
    }
    let mut depth = 1;
    while depth > 0 {
        match reader.next()? {
            Some(Event::Start(inner)) => {
                depth += 1;
                match inner.local() {
                    "s" => text.push_str(&" ".repeat(repeat(&inner, "text:c") as usize)),
                    "tab" => text.push('\t'),
                    "line-break" => text.push('\n'),
                    "a" => *link = inner.attr("xlink:href").map(|h| h.into_owned()),
                    _ => {}
                }
            }
            Some(Event::End { .. }) => depth -= 1,
            Some(Event::Text(part)) => text.push_str(&part),
            None => return Err("a paragraph is not closed".into()),
        }
    }
    Ok(text)
}

fn annotation(reader: &mut Reader) -> Result<(String, String), String> {
    let mut author = String::new();
    let mut lines = Vec::new();
    loop {
        let tag = match reader.next()? {
            Some(Event::Start(tag)) => tag,
            Some(Event::End { .. }) | None => return Ok((author, lines.join("\n"))),
            Some(Event::Text(_)) => continue,
        };
        match tag.local() {
            "creator" => author = reader.element_text(&tag)?,
            "p" => lines.push(paragraph_text(reader, &tag, &mut None)?),
            _ => {
                reader.element_end(&tag)?;
            }
        }
    }
}

/// "PT12H30M00S" (opendocument durations) as a fraction of a day
pub fn duration_days(text: &str) -> Option<f64> {
    let negative = text.starts_with('-');
    let body = text
        .trim_start_matches('-')
        .strip_prefix("PT")
        .or_else(|| text.trim_start_matches('-').strip_prefix('P'))?;
    let mut seconds = 0.0;
    let mut number = String::new();
    for c in body.chars() {
        match c {
            'D' => seconds += number.parse::<f64>().ok()? * 86_400.0,
            'H' => seconds += number.parse::<f64>().ok()? * 3600.0,
            'M' => seconds += number.parse::<f64>().ok()? * 60.0,
            'S' => seconds += number.parse::<f64>().ok()?,
            'T' => {}
            _ => {
                number.push(c);
                continue;
            }
        }
        number.clear();
    }
    let days = seconds / 86_400.0;
    Some(if negative { -days } else { days })
}

fn cell_value(tag: &Tag, content: &CellContent, system: DateSystem) -> Value {
    let text = content.paragraphs.join("\n");
    if tag.attr("calcext:value-type").as_deref() == Some("error") {
        return Value::Error(ErrorValue::parse(&text).unwrap_or(ErrorValue::Value));
    }
    let number = |name: &str| tag.attr(name).and_then(|v| v.parse::<f64>().ok());
    match tag.attr("office:value-type").as_deref() {
        Some("float") | Some("percentage") | Some("currency") => number("office:value")
            .map(Value::Number)
            .unwrap_or(Value::Blank),
        Some("date") => tag
            .attr("office:date-value")
            .and_then(|d| crate::dates::serial_from_iso(&d, system))
            .map(Value::Number)
            .unwrap_or(Value::Blank),
        Some("time") => tag
            .attr("office:time-value")
            .and_then(|t| duration_days(&t))
            .map(Value::Number)
            .unwrap_or(Value::Blank),
        Some("boolean") => Value::Bool(
            tag.attr("office:boolean-value")
                .is_some_and(|b| b == "true"),
        ),
        Some("string") => Value::Text(
            tag.attr("office:string-value")
                .map(|s| s.into_owned())
                .unwrap_or(text),
        ),
        _ if !text.is_empty() => Value::Text(text),
        _ => Value::Blank,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read() {
        assert_eq!(duration_days("PT12H00M00S"), Some(0.5));
        assert_eq!(duration_days("PT36H"), Some(1.5));
        assert_eq!(duration_days("P1DT6H"), Some(1.25));
    }
}
