//! tables: grid, rows, cells, merges, borders and shading.
//!
//! word marks a vertical merge on every covered cell (`w:vMerge` without
//! "restart"). the model instead gives the first cell a row span and
//! leaves the covered cells out, so this reader tracks which cell started
//! a merge in each grid column.

use std::collections::HashMap;

use crate::model::{
    Attribute, Border, CellBorders, CellMargins, Distances, FloatPosition, HeightRule, Preserved,
    Table, TableCell, TablePosition, TableRow, TextDirection, VerticalAlign,
};
use crate::xml::Element;

use super::props::{alignment, number, on_off, only_attributes, twips};
use super::{DocxReader, Story};

impl DocxReader<'_> {
    pub(super) fn read_table(&mut self, element: &Element, story: &mut Story) -> Table {
        let mut table = Table::default();
        keep_attributes(element, &mut table.preserved);
        // grid column -> (row index, cell index) of the cell whose merge is open
        let mut open_merges: HashMap<usize, (usize, usize)> = HashMap::new();
        for child in element.elements() {
            match child.name.as_str() {
                "w:tblPr" => read_table_props(child, story, &mut table),
                "w:tblGrid" => {
                    table.column_widths = child
                        .children_named("w:gridCol")
                        .map(|col| twips(col, "w:w").unwrap_or(0))
                        .collect();
                }
                "w:tr" => {
                    let row_index = table.rows.len();
                    let row = self.read_row(child, story, row_index, &mut table, &mut open_merges);
                    table.rows.push(row);
                }
                _ => table.preserved.children.push(story.keep(child)),
            }
        }
        table
    }

    fn read_row(
        &mut self,
        element: &Element,
        story: &mut Story,
        row_index: usize,
        table: &mut Table,
        open_merges: &mut HashMap<usize, (usize, usize)>,
    ) -> TableRow {
        let mut row = TableRow::default();
        keep_attributes(element, &mut row.preserved);
        let mut column = 0usize;
        for child in element.elements() {
            match child.name.as_str() {
                "w:trPr" => column += read_row_props(child, story, &mut row),
                "w:tc" => {
                    let (cell, merge) = self.read_cell(child, story);
                    let span = cell.col_span.max(1) as usize;
                    match merge {
                        Merge::Continue => {
                            if let Some(&(start_row, start_cell)) = open_merges.get(&column) {
                                table.rows[start_row].cells[start_cell].row_span += 1;
                            } else {
                                // a continuation with nothing to continue:
                                // keep it as an ordinary cell
                                row.cells.push(cell);
                            }
                        }
                        Merge::Restart => {
                            open_merges.insert(column, (row_index, row.cells.len()));
                            row.cells.push(cell);
                        }
                        Merge::None => {
                            open_merges.remove(&column);
                            row.cells.push(cell);
                        }
                    }
                    column += span;
                }
                _ => row.preserved.children.push(story.keep(child)),
            }
        }
        row
    }

    fn read_cell(&mut self, element: &Element, story: &mut Story) -> (TableCell, Merge) {
        let mut cell = TableCell::default();
        let mut merge = Merge::None;
        keep_attributes(element, &mut cell.preserved);
        if let Some(tcpr) = element.child("w:tcPr") {
            merge = read_cell_props(tcpr, story, &mut cell);
        }
        let mut ignored_sections = Vec::new();
        cell.blocks = self.read_blocks(element, story, &mut ignored_sections);
        (cell, merge)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Merge {
    None,
    Restart,
    Continue,
}

fn keep_attributes(element: &Element, preserved: &mut Preserved) {
    preserved.attributes = element
        .attributes
        .iter()
        .map(|(name, value)| Attribute::new(name, value))
        .collect();
}

fn read_table_props(tblpr: &Element, story: &mut Story, table: &mut Table) {
    for child in tblpr.elements() {
        let understood = match child.name.as_str() {
            "w:tblStyle" => {
                table.style_id = child.attr("w:val").map(str::to_string);
                true
            }
            "w:tblW" if child.attr("w:type") == Some("dxa") => {
                table.width = twips(child, "w:w");
                table.width.is_some()
            }
            "w:jc" => {
                table.alignment = child.attr("w:val").and_then(alignment);
                table.alignment.is_some()
            }
            "w:tblBorders" => match read_borders(child) {
                Some(borders) => {
                    table.borders = Some(borders);
                    true
                }
                None => false,
            },
            "w:tblpPr" => match read_table_position(child) {
                Some(position) => {
                    table.position = Some(position);
                    true
                }
                None => false,
            },
            "w:tblLayout" if only_attributes(child, &["w:type"]) => {
                table.fixed_layout = match child.attr("w:type") {
                    Some("fixed") => Some(true),
                    Some("autofit") => Some(false),
                    _ => None,
                };
                table.fixed_layout.is_some()
            }
            "w:tblCellMar" => match read_cell_margins(child) {
                Some(margins) => {
                    table.cell_margins = Some(margins);
                    true
                }
                None => false,
            },
            "w:tblInd" if child.attr("w:type") == Some("dxa") => {
                table.indent = twips(child, "w:w");
                table.indent.is_some()
            }
            _ => false,
        };
        if !understood {
            table.preserved.children.push(story.keep(child));
        }
    }
    // the overlap rule belongs to the position, so it is read after it
    let overlap = table
        .preserved
        .children
        .iter()
        .position(|xml| xml.starts_with("<w:tblOverlap"));
    if let (Some(at), Some(position)) = (overlap, table.position.as_mut()) {
        position.overlap = !table.preserved.children[at].contains("never");
        table.preserved.children.remove(at);
    }
}

/// `w:tblpPr`: where a floating table sits. word measures horizontally
/// from the page, the margin or the column ("text") and vertically from
/// the page, the margin or the paragraph ("text").
fn read_table_position(element: &Element) -> Option<TablePosition> {
    const KNOWN: &[&str] = &[
        "w:leftFromText",
        "w:rightFromText",
        "w:topFromText",
        "w:bottomFromText",
        "w:vertAnchor",
        "w:horzAnchor",
        "w:tblpX",
        "w:tblpY",
        "w:tblpXSpec",
        "w:tblpYSpec",
    ];
    if !only_attributes(element, KNOWN) {
        return None;
    }
    let anchor = |name: &str, vertical: bool| match element.attr(name) {
        Some("page") => "page",
        Some("margin") => "margin",
        _ if vertical => "paragraph",
        _ => "column",
    };
    let position = |relative_to: &str, offset: &str, spec: &str| FloatPosition {
        relative_to: relative_to.to_string(),
        offset: twips(element, offset).unwrap_or(0),
        align: element.attr(spec).map(str::to_string),
        offset_emu: None,
    };
    let distance = |name: &str| twips(element, name).unwrap_or(0);
    Some(TablePosition {
        horizontal: position(anchor("w:horzAnchor", false), "w:tblpX", "w:tblpXSpec"),
        vertical: position(anchor("w:vertAnchor", true), "w:tblpY", "w:tblpYSpec"),
        distance: Distances {
            left: distance("w:leftFromText"),
            top: distance("w:topFromText"),
            right: distance("w:rightFromText"),
            bottom: distance("w:bottomFromText"),
        },
        overlap: true,
        anchor: None,
    })
}

/// `w:tblCellMar` or `w:tcMar`, when every side is a plain length.
fn read_cell_margins(element: &Element) -> Option<CellMargins> {
    let mut margins = CellMargins::default();
    for side in element.elements() {
        if side.attr("w:type") != Some("dxa") || !only_attributes(side, &["w:w", "w:type"]) {
            return None;
        }
        let value = twips(side, "w:w");
        match side.name.as_str() {
            "w:top" => margins.top = value,
            "w:bottom" => margins.bottom = value,
            "w:left" | "w:start" => margins.left = value,
            "w:right" | "w:end" => margins.right = value,
            _ => return None,
        }
    }
    Some(margins)
}

/// returns how many grid columns the row skips before its first cell.
fn read_row_props(trpr: &Element, story: &mut Story, row: &mut TableRow) -> usize {
    let mut skipped = 0;
    for child in trpr.elements() {
        let understood = match child.name.as_str() {
            "w:trHeight" if only_attributes(child, &["w:val", "w:hRule"]) => {
                row.height = twips(child, "w:val");
                row.height_rule = match child.attr("w:hRule") {
                    Some("exact") => Some(HeightRule::Exact),
                    Some("atLeast") => Some(HeightRule::AtLeast),
                    _ => None,
                };
                // "auto" ignores the height, which the model cannot say
                row.height.is_some() && child.attr("w:hRule") != Some("auto")
            }
            "w:tblHeader" => {
                row.header = on_off(child);
                true
            }
            "w:cantSplit" => {
                row.cant_split = on_off(child);
                true
            }
            "w:gridBefore" => {
                skipped = number(child, "w:val").unwrap_or(0) as usize;
                false
            }
            _ => false,
        };
        if !understood {
            row.preserved.children.push(story.keep(child));
        }
    }
    skipped
}

fn read_cell_props(tcpr: &Element, story: &mut Story, cell: &mut TableCell) -> Merge {
    let mut merge = Merge::None;
    for child in tcpr.elements() {
        let understood = match child.name.as_str() {
            "w:tcW" if child.attr("w:type") == Some("dxa") => {
                cell.width = twips(child, "w:w");
                cell.width.is_some()
            }
            "w:gridSpan" => {
                cell.col_span = number(child, "w:val").unwrap_or(1).max(1);
                true
            }
            "w:vMerge" => {
                merge = if child.attr("w:val") == Some("restart") {
                    Merge::Restart
                } else {
                    Merge::Continue
                };
                true
            }
            "w:tcBorders" => match read_borders(child) {
                Some(borders) => {
                    cell.borders = Some(borders);
                    true
                }
                None => false,
            },
            "w:shd" => match child.attr("w:fill") {
                Some(fill)
                    if only_attributes(child, &["w:val", "w:color", "w:fill"])
                        && child.attr("w:val") == Some("clear") =>
                {
                    cell.shading = Some(fill.to_string());
                    true
                }
                _ => false,
            },
            "w:tcMar" => match read_cell_margins(child) {
                Some(margins) => {
                    cell.margins = Some(margins);
                    true
                }
                None => false,
            },
            "w:textDirection" => {
                cell.text_direction = match child.attr("w:val") {
                    Some("lrTb") => Some(TextDirection::Horizontal),
                    Some("btLr") => Some(TextDirection::Upward),
                    Some("tbRl") => Some(TextDirection::Downward),
                    _ => None,
                };
                cell.text_direction.is_some()
            }
            "w:vAlign" => {
                cell.vertical_align = match child.attr("w:val") {
                    Some("top") => Some(VerticalAlign::Top),
                    Some("center") => Some(VerticalAlign::Center),
                    Some("bottom") => Some(VerticalAlign::Bottom),
                    _ => None,
                };
                cell.vertical_align.is_some()
            }
            _ => false,
        };
        if !understood {
            cell.preserved.children.push(story.keep(child));
        }
    }
    merge
}

/// reads `w:tblBorders` or `w:tcBorders`; `None` when any border uses
/// something the model does not have (theme colours, diagonal lines).
fn read_borders(element: &Element) -> Option<CellBorders> {
    let mut borders = CellBorders::default();
    for side in element.elements() {
        let border = read_border(side)?;
        let slot = match side.name.as_str() {
            "w:top" => &mut borders.top,
            "w:left" | "w:start" => &mut borders.left,
            "w:bottom" => &mut borders.bottom,
            "w:right" | "w:end" => &mut borders.right,
            "w:insideH" => &mut borders.inside_horizontal,
            "w:insideV" => &mut borders.inside_vertical,
            _ => return None,
        };
        *slot = Some(border);
    }
    Some(borders)
}

fn read_border(side: &Element) -> Option<Border> {
    if !only_attributes(side, &["w:val", "w:sz", "w:color", "w:space"]) {
        return None;
    }
    if side.attr("w:space").is_some_and(|space| space != "0") {
        return None;
    }
    Some(Border {
        style: side.attr("w:val")?.to_string(),
        size: number(side, "w:sz").unwrap_or(0),
        color: side
            .attr("w:color")
            .filter(|color| *color != "auto")
            .map(str::to_string),
    })
}
