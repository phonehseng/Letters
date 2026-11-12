//! pdf to a spreadsheet (xlsx), written by hand as zip and xml.
//!
//! every table the structure analysis finds becomes real cells: numbers
//! as numbers in the format they were printed in, dates as dates, text as
//! text, merged cells where a cell spans others, and the column widths,
//! row heights, fonts, shading, borders and alignment of the pdf.
//!
//! why there is no picture of the page behind the cells: a spreadsheet
//! can't put a picture *behind* cells, only float it over them, where it
//! would hide the very cells it copies, get in the way of selecting,
//! sorting and typing, and print twice. so the sheet is made to look like
//! the table instead, by keeping its geometry: each column is as wide and
//! each row as tall as in the pdf, and on "a sheet per page" the text
//! around the tables keeps its place in the rows between them.

mod styles;
mod values;

use std::collections::HashMap;
use std::path::Path;

use letters_ocr::{CancelFlag, Progress};
use serde::{Deserialize, Serialize};

use crate::package::{xml_escape, PackageFile, XML_HEADER};
use crate::source::{PageSelection, PdfSource};
use crate::structure::fonts::office_family;
use crate::structure::model::{runs_text, Block, BlockContent, Table, TableCell, TextRun};
use crate::structure::{analyze_structure, StructureOptions};
use crate::ConvertError;
use styles::{CellStyle, FontStyle, StyleSheet};
use values::{parse_cell, CellValue};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SheetLayout {
    /// one sheet per page: its tables, with the text around them in place
    #[default]
    SheetPerPage,
    /// one sheet per table, nothing else
    SheetPerTable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct XlsxExportOptions {
    pub pages: PageSelection,
    pub layout: SheetLayout,
}

impl Default for XlsxExportOptions {
    fn default() -> Self {
        Self {
            pages: PageSelection::All,
            layout: SheetLayout::SheetPerPage,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XlsxReport {
    pub sheets: Vec<String>,
    /// how many tables became cells
    pub tables: usize,
    /// things worth telling the person, a sentence each
    pub notes: Vec<String>,
}

/// writes the chosen pages as a spreadsheet at `path` (replacing it: the
/// save panel already asked).
pub fn pdf_to_xlsx(
    source: &PdfSource,
    options: &XlsxExportOptions,
    path: &Path,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<XlsxReport, ConvertError> {
    let structure = analyze_structure(
        source,
        &StructureOptions {
            pages: options.pages.clone(),
            ..Default::default()
        },
        progress,
        cancel,
    )?;
    let mut book = Workbook::default();
    let mut report = XlsxReport::default();
    let tables: Vec<&Block> = structure
        .blocks
        .iter()
        .filter(|b| matches!(b.content, BlockContent::Table(_)))
        .collect();
    report.tables = tables.len();
    match options.layout {
        SheetLayout::SheetPerPage => {
            for page in &structure.pages {
                let blocks: Vec<&Block> = structure
                    .blocks
                    .iter()
                    .filter(|b| b.page == page.page)
                    .collect();
                let sheet = page_sheet(&mut book, &blocks);
                book.add_sheet(&format!("Page {}", page.page + 1), sheet);
            }
        }
        SheetLayout::SheetPerTable if tables.is_empty() => {
            let blocks: Vec<&Block> = structure.blocks.iter().collect();
            let sheet = page_sheet(&mut book, &blocks);
            book.add_sheet("Text", sheet);
            report.notes.push(
                "No tables were found, so the text is on one sheet, in the order it is read."
                    .into(),
            );
        }
        SheetLayout::SheetPerTable => {
            for (index, block) in tables.iter().enumerate() {
                let BlockContent::Table(table) = &block.content else {
                    continue;
                };
                let sheet = table_sheet(&mut book, table);
                book.add_sheet(
                    &format!("Table {} (page {})", index + 1, block.page + 1),
                    sheet,
                );
            }
        }
    }
    if cancel.is_cancelled() {
        return Err(ConvertError::Cancelled);
    }
    report.sheets = book.sheets.iter().map(|(name, _)| name.clone()).collect();
    book.write(path, structure.title.as_deref())?;
    Ok(report)
}

/// one sheet: column widths, rows, merged areas.
#[derive(Default)]
struct Sheet {
    /// points, left to right
    column_widths: Vec<f32>,
    rows: Vec<Row>,
    /// (first row, first column, last row, last column), zero based
    merges: Vec<(usize, usize, usize, usize)>,
}

#[derive(Default)]
struct Row {
    /// points; none for the default height
    height: Option<f32>,
    cells: Vec<Cell>,
}

struct Cell {
    column: usize,
    value: CellValue,
    style: usize,
}

#[derive(Default)]
struct Workbook {
    sheets: Vec<(String, Sheet)>,
    styles: Option<StyleSheet>,
    strings: Vec<String>,
    string_index: HashMap<String, usize>,
}

impl Workbook {
    fn styles(&mut self) -> &mut StyleSheet {
        self.styles.get_or_insert_with(StyleSheet::new)
    }

    fn add_sheet(&mut self, name: &str, sheet: Sheet) {
        let name = unique_sheet_name(name, &self.sheets);
        self.sheets.push((name, sheet));
    }

    fn string(&mut self, text: &str) -> usize {
        if let Some(index) = self.string_index.get(text) {
            return *index;
        }
        self.strings.push(text.to_string());
        self.string_index
            .insert(text.to_string(), self.strings.len() - 1);
        self.strings.len() - 1
    }
}

/// excel allows 31 characters and none of : \ / ? * [ ], and no two
/// sheets with the same name.
fn unique_sheet_name(name: &str, sheets: &[(String, Sheet)]) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']') {
                '-'
            } else {
                c
            }
        })
        .take(31)
        .collect();
    let taken = |candidate: &str| {
        sheets
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(candidate))
    };
    if !taken(&clean) {
        return clean;
    }
    (2..)
        .map(|n| {
            let suffix = format!(" ({n})");
            let base: String = clean.chars().take(31 - suffix.len()).collect();
            format!("{base}{suffix}")
        })
        .find(|candidate| !taken(candidate))
        .expect("some number is free")
}

// ------------------------------------------------------------ building

/// the x positions where columns start and end: every table's column
/// edges, close ones taken as one. with no tables, one column as wide as
/// the text.
fn column_edges(blocks: &[&Block]) -> Vec<f32> {
    let mut edges: Vec<f32> = Vec::new();
    for block in blocks {
        if let BlockContent::Table(table) = &block.content {
            let mut x = block.rect.x;
            edges.push(x);
            for width in &table.column_widths {
                x += width;
                edges.push(x);
            }
        }
    }
    if edges.is_empty() {
        let left = blocks
            .iter()
            .map(|b| b.rect.x)
            .fold(f32::INFINITY, f32::min);
        let right = blocks.iter().map(|b| b.rect.right()).fold(0.0, f32::max);
        if left.is_finite() && right > left {
            return vec![left, right];
        }
        return vec![0.0, 72.0];
    }
    edges.sort_by(f32::total_cmp);
    let mut merged: Vec<f32> = Vec::new();
    for edge in edges {
        if merged.last().is_none_or(|last| edge - last > 3.0) {
            merged.push(edge);
        }
    }
    merged
}

/// the column an x position falls in.
fn column_at(edges: &[f32], x: f32) -> usize {
    edges
        .iter()
        .rposition(|edge| *edge <= x + 3.0)
        .unwrap_or(0)
        .min(edges.len().saturating_sub(2))
}

/// the columns a span from x to right covers, given the edges.
fn columns_between(edges: &[f32], x: f32, right: f32) -> (usize, usize) {
    let first = column_at(edges, x);
    let last = edges
        .iter()
        .rposition(|edge| *edge < right - 3.0)
        .unwrap_or(first)
        .clamp(first, edges.len().saturating_sub(2));
    (first, last)
}

/// a page's blocks in order: tables cell by cell on a shared column grid,
/// other text one block per row, and spacer rows where the page has
/// space between blocks, so the sheet keeps the page's shape.
fn page_sheet(book: &mut Workbook, blocks: &[&Block]) -> Sheet {
    let edges = column_edges(blocks);
    let mut sheet = Sheet {
        column_widths: edges.windows(2).map(|w| w[1] - w[0]).collect(),
        ..Default::default()
    };
    let mut bottom: Option<(usize, f32)> = None;
    for block in blocks {
        if let Some((page, y)) = bottom {
            let gap = block.rect.y - y;
            if page == block.page && gap > 4.0 {
                sheet.rows.push(Row {
                    height: Some(gap.min(60.0)),
                    cells: Vec::new(),
                });
            }
        }
        match &block.content {
            BlockContent::Table(table) => {
                add_table(book, &mut sheet, table, &edges, block.rect.x);
            }
            _ => add_text_block(book, &mut sheet, block, &edges),
        }
        bottom = Some((block.page, block.rect.bottom()));
    }
    sheet
}

/// one table on its own sheet: its own columns, and its caption above it.
fn table_sheet(book: &mut Workbook, table: &Table) -> Sheet {
    let mut edges = vec![0.0];
    for width in &table.column_widths {
        edges.push(edges.last().copied().unwrap_or(0.0) + width);
    }
    let mut sheet = Sheet {
        column_widths: table.column_widths.clone(),
        ..Default::default()
    };
    if let Some(caption) = &table.caption {
        let style = text_style(book, caption, false);
        let text = runs_text(caption);
        let value = CellValue::Text(text);
        sheet.rows.push(Row {
            height: None,
            cells: vec![Cell {
                column: 0,
                value,
                style,
            }],
        });
    }
    add_table(book, &mut sheet, table, &edges, 0.0);
    sheet
}

fn add_table(book: &mut Workbook, sheet: &mut Sheet, table: &Table, edges: &[f32], left: f32) {
    let first_row = sheet.rows.len();
    for height in &table.row_heights {
        sheet.rows.push(Row {
            height: Some(*height),
            cells: Vec::new(),
        });
    }
    // where each table column starts, for placing cells on the sheet grid
    let mut column_x = vec![left];
    for width in &table.column_widths {
        column_x.push(column_x.last().copied().unwrap_or(left) + width);
    }
    for cell in &table.cells {
        let x = column_x[cell.column];
        let right = column_x[(cell.column + cell.column_span).min(table.columns)];
        let (first_column, last_column) = columns_between(edges, x, right);
        let row = first_row + cell.row;
        let last_row = row + cell.row_span - 1;
        let style = cell_style(book, cell, right - x);
        let value = parse_cell(&cell.text());
        let number_style = match &value {
            CellValue::Number { format, .. } => {
                let mut styled = cell_style_parts(cell, right - x);
                styled.number_format = format.clone();
                book.styles().index(&styled)
            }
            _ => style,
        };
        sheet.rows[row].cells.push(Cell {
            column: first_column,
            value,
            style: number_style,
        });
        if last_row > row || last_column > first_column {
            sheet
                .merges
                .push((row, first_column, last_row, last_column));
            // the hidden cells of a merged area carry its borders, which
            // is how spreadsheets draw a merged cell's edges
            for covered_row in row..=last_row {
                for column in first_column..=last_column {
                    if (covered_row, column) != (row, first_column)
                        && covered_row < sheet.rows.len()
                    {
                        sheet.rows[covered_row].cells.push(Cell {
                            column,
                            value: CellValue::Empty,
                            style,
                        });
                    }
                }
            }
        }
    }
}

fn cell_style(book: &mut Workbook, cell: &TableCell, width: f32) -> usize {
    let style = cell_style_parts(cell, width);
    book.styles().index(&style)
}

fn cell_style_parts(cell: &TableCell, width: f32) -> CellStyle {
    let font = font_of(&cell.runs);
    // text that won't fit the column on one line wraps, as it did in the pdf
    let text = cell.text();
    let estimated_width = text.chars().count() as f32 * font.size * 0.5;
    CellStyle {
        wrap: estimated_width > width * 1.05,
        font,
        fill: cell.fill,
        borders: cell.borders,
        align: cell.align,
        number_format: None,
    }
}

/// the font of the first run with letters in it.
fn font_of(runs: &[TextRun]) -> FontStyle {
    match runs.iter().find(|run| !run.text.trim().is_empty()) {
        Some(run) => FontStyle {
            name: office_family(&run.font_family).to_string(),
            size: run.size,
            bold: run.bold,
            italic: run.italic,
            color: run.color,
        },
        None => FontStyle::default(),
    }
}

fn text_style(book: &mut Workbook, runs: &[TextRun], wrap: bool) -> usize {
    let style = CellStyle {
        font: font_of(runs),
        wrap,
        ..Default::default()
    };
    book.styles().index(&style)
}

/// a block that isn't a table: one row per paragraph or list item, in
/// the column under its left edge.
fn add_text_block(book: &mut Workbook, sheet: &mut Sheet, block: &Block, edges: &[f32]) {
    let column = column_at(edges, block.rect.x);
    let lines: Vec<(String, &[TextRun])> = match &block.content {
        BlockContent::Heading { runs, .. } | BlockContent::Paragraph { runs } => {
            vec![(runs_text(runs), runs)]
        }
        BlockContent::Footnote { marker, runs } => {
            vec![(format!("{marker} {}", runs_text(runs)), runs)]
        }
        BlockContent::List { items } => items
            .iter()
            .map(|item| {
                let indent = "    ".repeat(usize::from(item.level));
                (
                    format!("{indent}{} {}", item.marker, runs_text(&item.runs)),
                    item.runs.as_slice(),
                )
            })
            .collect(),
        BlockContent::Figure(figure) => match &figure.caption {
            Some(caption) => vec![(runs_text(caption), caption.as_slice())],
            None => Vec::new(),
        },
        BlockContent::Table(_) => Vec::new(),
    };
    for (text, runs) in lines {
        let style = text_style(book, runs, false);
        let size = font_of(runs).size;
        sheet.rows.push(Row {
            // a heading's row grows with its size, as in the pdf
            height: (size > 13.0).then_some(size * 1.3),
            cells: vec![Cell {
                column,
                value: CellValue::Text(text),
                style,
            }],
        });
    }
}

// ------------------------------------------------------------- writing

impl Workbook {
    fn write(mut self, path: &Path, title: Option<&str>) -> Result<(), ConvertError> {
        let mut package = PackageFile::create(path)?;
        let result = self.write_parts(&mut package, title);
        match result {
            Ok(()) => package.finish(),
            Err(error) => {
                package.abandon();
                Err(error)
            }
        }
    }

    fn write_parts(
        &mut self,
        package: &mut PackageFile,
        title: Option<&str>,
    ) -> Result<(), ConvertError> {
        let count = self.sheets.len();
        package
            .zip
            .add("[Content_Types].xml", content_types(count).as_bytes())?;
        package
            .zip
            .add("_rels/.rels", ROOT_RELATIONSHIPS.as_bytes())?;
        package
            .zip
            .add("docProps/core.xml", core_properties(title).as_bytes())?;
        package
            .zip
            .add("docProps/app.xml", APP_PROPERTIES.as_bytes())?;
        package
            .zip
            .add("xl/workbook.xml", self.workbook_xml().as_bytes())?;
        package.zip.add(
            "xl/_rels/workbook.xml.rels",
            workbook_relationships(count).as_bytes(),
        )?;
        let sheets = std::mem::take(&mut self.sheets);
        for (index, (_, sheet)) in sheets.iter().enumerate() {
            let xml = self.sheet_xml(sheet, index == 0);
            package.zip.add(
                &format!("xl/worksheets/sheet{}.xml", index + 1),
                xml.as_bytes(),
            )?;
        }
        self.sheets = sheets;
        let styles = self.styles().xml();
        package.zip.add("xl/styles.xml", styles.as_bytes())?;
        package
            .zip
            .add("xl/sharedStrings.xml", self.shared_strings_xml().as_bytes())?;
        Ok(())
    }

    fn workbook_xml(&self) -> String {
        let mut xml = format!(
            "{XML_HEADER}<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><bookViews><workbookView/></bookViews><sheets>"
        );
        for (index, (name, _)) in self.sheets.iter().enumerate() {
            xml.push_str(&format!(
                "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"/>",
                xml_escape(name),
                index + 1,
                index + 1
            ));
        }
        xml.push_str("</sheets></workbook>");
        xml
    }

    fn sheet_xml(&mut self, sheet: &Sheet, first: bool) -> String {
        let mut xml = format!(
            "{XML_HEADER}<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">"
        );
        xml.push_str(&format!(
            "<sheetViews><sheetView workbookViewId=\"0\"{}/></sheetViews><sheetFormatPr defaultRowHeight=\"15\"/>",
            if first { " tabSelected=\"1\"" } else { "" }
        ));
        if !sheet.column_widths.is_empty() {
            xml.push_str("<cols>");
            for (index, width) in sheet.column_widths.iter().enumerate() {
                xml.push_str(&format!(
                    "<col min=\"{n}\" max=\"{n}\" width=\"{}\" customWidth=\"1\"/>",
                    column_width(*width),
                    n = index + 1
                ));
            }
            xml.push_str("</cols>");
        }
        xml.push_str("<sheetData>");
        for (index, row) in sheet.rows.iter().enumerate() {
            xml.push_str(&format!("<row r=\"{}\"", index + 1));
            if let Some(height) = row.height {
                xml.push_str(&format!(
                    " ht=\"{}\" customHeight=\"1\"",
                    (height * 100.0).round() / 100.0
                ));
            }
            xml.push('>');
            let mut cells: Vec<&Cell> = row.cells.iter().collect();
            cells.sort_by_key(|c| c.column);
            cells.dedup_by_key(|c| c.column);
            for cell in cells {
                xml.push_str(&self.cell_xml(cell, index));
            }
            xml.push_str("</row>");
        }
        xml.push_str("</sheetData>");
        if !sheet.merges.is_empty() {
            xml.push_str(&format!("<mergeCells count=\"{}\">", sheet.merges.len()));
            for (r0, c0, r1, c1) in &sheet.merges {
                xml.push_str(&format!(
                    "<mergeCell ref=\"{}{}:{}{}\"/>",
                    column_name(*c0),
                    r0 + 1,
                    column_name(*c1),
                    r1 + 1
                ));
            }
            xml.push_str("</mergeCells>");
        }
        xml.push_str("<pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/></worksheet>");
        xml
    }

    fn cell_xml(&mut self, cell: &Cell, row: usize) -> String {
        let reference = format!("{}{}", column_name(cell.column), row + 1);
        match &cell.value {
            CellValue::Empty => format!("<c r=\"{reference}\" s=\"{}\"/>", cell.style),
            CellValue::Text(text) => {
                let index = self.string(text);
                format!(
                    "<c r=\"{reference}\" s=\"{}\" t=\"s\"><v>{index}</v></c>",
                    cell.style
                )
            }
            CellValue::Number { value, .. } => {
                format!(
                    "<c r=\"{reference}\" s=\"{}\"><v>{value}</v></c>",
                    cell.style
                )
            }
        }
    }

    fn shared_strings_xml(&self) -> String {
        let mut xml = format!(
            "{XML_HEADER}<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{n}\" uniqueCount=\"{n}\">",
            n = self.strings.len()
        );
        for text in &self.strings {
            xml.push_str(&format!(
                "<si><t xml:space=\"preserve\">{}</t></si>",
                xml_escape(text)
            ));
        }
        xml.push_str("</sst>");
        xml
    }
}

/// "A" for 0, "Z" for 25, "AA" for 26.
pub(crate) fn column_name(mut index: usize) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).expect("letters")
}

/// a column width in excel's unit: characters of the default font's
/// widest digit (7 pixels for calibri 11 at 96 pixels per inch), plus
/// 5 pixels of padding, as the file format describes.
pub(crate) fn column_width(points: f32) -> f32 {
    let pixels = points * 96.0 / 72.0;
    let characters = ((pixels - 5.0) / 7.0 * 100.0 + 0.5).trunc() / 100.0;
    characters.max(0.5)
}

fn content_types(sheets: usize) -> String {
    let mut xml = format!(
        "{XML_HEADER}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>"
    );
    for index in 1..=sheets {
        xml.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        ));
    }
    xml.push_str("<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/><Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/></Types>");
    xml
}

const ROOT_RELATIONSHIPS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/></Relationships>";

const APP_PROPERTIES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Letters</Application></Properties>";

pub(crate) fn core_properties(title: Option<&str>) -> String {
    format!(
        "{XML_HEADER}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>Letters</dc:creator></cp:coreProperties>",
        xml_escape(title.unwrap_or(""))
    )
}

fn workbook_relationships(sheets: usize) -> String {
    let mut xml = format!(
        "{XML_HEADER}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">"
    );
    for index in 1..=sheets {
        xml.push_str(&format!(
            "<Relationship Id=\"rId{index}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{index}.xml\"/>"
        ));
    }
    xml.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/><Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/></Relationships>",
        sheets + 1,
        sheets + 2
    ));
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_named_like_excel() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
    }

    #[test]
    fn widths_follow_excels_rule() {
        // 64 pixels (48 points) is excel's default column, 8.43 characters
        assert_eq!(column_width(48.0), 8.43);
        assert_eq!(column_width(1.0), 0.5);
    }

    #[test]
    fn sheet_names_are_cleaned_and_unique() {
        let sheets = vec![("Page 1".to_string(), Sheet::default())];
        assert_eq!(unique_sheet_name("Page 1", &sheets), "Page 1 (2)");
        assert_eq!(unique_sheet_name("a/b:c", &sheets), "a-b-c");
        assert_eq!(unique_sheet_name(&"x".repeat(40), &sheets).len(), 31);
    }

    #[test]
    fn page_grids_share_column_edges() {
        let edges = vec![72.0, 192.0, 282.0, 372.0];
        assert_eq!(column_at(&edges, 72.0), 0);
        assert_eq!(column_at(&edges, 200.0), 1);
        assert_eq!(column_at(&edges, 500.0), 2);
        assert_eq!(columns_between(&edges, 72.0, 372.0), (0, 2));
        assert_eq!(columns_between(&edges, 192.0, 282.0), (1, 1));
    }
}
