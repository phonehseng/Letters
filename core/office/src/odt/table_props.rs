//! opendocument properties for layout: row heights, cell padding and text
//! direction, a table's indent, and columns (of a page or of a
//! `text:section`), read into and written from the model.

use crate::model::{
    CellMargins, ColumnWidth, Columns, HeightRule, Section, TableCell, TableRow, TextDirection,
    Twips,
};
use crate::units::{parse_length, twips_to_inches};
use crate::xml::Element;

/// what a table, row, cell or section style says about layout.
#[derive(Clone, Debug, Default)]
pub struct TableLayout {
    pub row_height: Option<(Twips, HeightRule)>,
    pub padding: Option<CellMargins>,
    pub text_direction: Option<TextDirection>,
    pub margin_left: Option<Twips>,
    /// the `style:columns` of a section style, as read
    pub columns: Option<Element>,
}

pub fn read_table(element: &Element, layout: &mut TableLayout) {
    layout.margin_left = element.attr("fo:margin-left").and_then(parse_length);
}

/// `style:row-height` is exact, `style:min-row-height` a least height.
pub fn read_row(element: &Element, layout: &mut TableLayout) {
    if let Some(height) = element.attr("style:row-height").and_then(parse_length) {
        layout.row_height = Some((height, HeightRule::Exact));
    } else if let Some(height) = element.attr("style:min-row-height").and_then(parse_length) {
        layout.row_height = Some((height, HeightRule::AtLeast));
    }
}

pub fn read_cell(element: &Element, layout: &mut TableLayout) {
    let all = element.attr("fo:padding").and_then(parse_length);
    let side = |name: &str| element.attr(name).and_then(parse_length).or(all);
    let margins = CellMargins {
        top: side("fo:padding-top"),
        left: side("fo:padding-left"),
        bottom: side("fo:padding-bottom"),
        right: side("fo:padding-right"),
    };
    if !margins.is_empty() {
        layout.padding = Some(margins);
    }
    layout.text_direction = match element.attr("style:writing-mode") {
        Some("bt-lr") => Some(TextDirection::Upward),
        Some("tb-rl") => Some(TextDirection::Downward),
        Some("lr-tb") => Some(TextDirection::Horizontal),
        _ => None,
    };
}

pub fn read_section(element: &Element, layout: &mut TableLayout) {
    layout.columns = element.child("style:columns").cloned();
}

/// `style:columns` into the model's columns. unequal columns are given
/// as relative widths ("3000*") with their gaps as indents inside each
/// column, so `text_width` (the width the columns share) turns them into
/// twips.
pub fn read_columns(element: &Element, text_width: Twips) -> (Columns, Vec<ColumnWidth>) {
    let count = element
        .attr("fo:column-count")
        .and_then(|c| c.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1);
    let columns = Columns {
        count,
        space: element
            .attr("fo:column-gap")
            .and_then(parse_length)
            .unwrap_or(720),
        separator: element
            .child("style:column-sep")
            .is_some_and(|sep| sep.attr("style:style") != Some("none")),
    };
    let listed: Vec<(f64, Twips, Twips)> = element
        .children_named("style:column")
        .filter_map(|column| {
            let relative = column
                .attr("style:rel-width")?
                .trim_end_matches('*')
                .parse::<f64>()
                .ok()?;
            let start = column
                .attr("fo:start-indent")
                .and_then(parse_length)
                .unwrap_or(0);
            let end = column
                .attr("fo:end-indent")
                .and_then(parse_length)
                .unwrap_or(0);
            Some((relative, start, end))
        })
        .collect();
    let total: f64 = listed.iter().map(|(relative, _, _)| relative).sum();
    if listed.len() < 2 || total <= 0.0 {
        return (columns, Vec::new());
    }
    let mut widths = Vec::new();
    for (index, (relative, start, end)) in listed.iter().enumerate() {
        let share = (relative / total * f64::from(text_width)).round() as Twips;
        let next_start = listed.get(index + 1).map(|(_, s, _)| *s).unwrap_or(0);
        widths.push(ColumnWidth {
            width: (share - start - end).max(1),
            space: if index + 1 < listed.len() {
                end + next_start
            } else {
                0
            },
        });
    }
    let count = widths.len() as u32;
    // libreoffice lists every column even for equal ones; but its "equal"
    // columns share the width with their gaps inside them, so the middle
    // ones hold less text. only truly equal text widths and gaps are
    // stored as plain equal columns
    let first = widths[0];
    let gaps = &widths[..widths.len() - 1];
    let equal = widths.iter().all(|w| (w.width - first.width).abs() <= 1)
        && gaps.iter().all(|w| (w.space - first.space).abs() <= 1);
    if equal {
        let space = first.space;
        return (
            Columns {
                count,
                space,
                ..columns
            },
            Vec::new(),
        );
    }
    (Columns { count, ..columns }, widths)
}

/// `style:columns` for a section or page layout; empty for one column.
pub fn columns_xml(section: &Section) -> String {
    let layout = section.column_layout();
    if layout.len() < 2 {
        return String::new();
    }
    let separator = if section.columns.separator {
        "<style:column-sep style:width=\"0.0071in\" style:color=\"#000000\" style:height=\"100%\" style:vertical-align=\"top\"/>"
    } else {
        ""
    };
    let mut xml = format!(
        "<style:columns fo:column-count=\"{}\" fo:column-gap=\"{}\">{separator}",
        layout.len(),
        twips_to_inches(section.columns.space)
    );
    // each gap is split between the columns on its two sides
    let mut start = 0;
    for column in &layout {
        let end = column.space / 2;
        let share = start + column.width + end;
        xml.push_str(&format!(
            "<style:column style:rel-width=\"{share}*\" fo:start-indent=\"{}\" fo:end-indent=\"{}\"/>",
            twips_to_inches(start),
            twips_to_inches(end)
        ));
        start = column.space - end;
    }
    xml.push_str("</style:columns>");
    xml
}

/// `style:table-row-properties` for a row's height, when it has one.
pub fn row_properties(row: &TableRow) -> Option<String> {
    let height = row.height?;
    let attribute = match row.height_rule {
        Some(HeightRule::Exact) => "style:row-height",
        _ => "style:min-row-height",
    };
    Some(format!(
        "<style:table-row-properties {attribute}=\"{}\"/>",
        twips_to_inches(height)
    ))
}

/// the padding and text direction attributes of a cell's properties.
/// `defaults` are the table's cell margins, which opendocument has no
/// place for, so every cell carries them.
pub fn cell_attributes(cell: &TableCell, defaults: Option<&CellMargins>) -> String {
    let own = cell.margins.unwrap_or_default();
    let base = defaults.copied().unwrap_or_default();
    let mut xml = String::new();
    for (name, value) in [
        ("fo:padding-top", own.top.or(base.top)),
        ("fo:padding-left", own.left.or(base.left)),
        ("fo:padding-bottom", own.bottom.or(base.bottom)),
        ("fo:padding-right", own.right.or(base.right)),
    ] {
        if let Some(value) = value {
            xml.push_str(&format!(" {name}=\"{}\"", twips_to_inches(value)));
        }
    }
    let mode = match cell.text_direction {
        Some(TextDirection::Upward) => Some("bt-lr"),
        Some(TextDirection::Downward) => Some("tb-rl"),
        Some(TextDirection::Horizontal) => Some("lr-tb"),
        None => None,
    };
    if let Some(mode) = mode {
        xml.push_str(&format!(" style:writing-mode=\"{mode}\""));
    }
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    #[test]
    fn unequal_columns_come_back_through_relative_widths() {
        let column = |width, space| ColumnWidth { width, space };
        let mut section = Section {
            column_widths: vec![column(2000, 360), column(3600, 240), column(2600, 0)],
            ..Section::default()
        };
        section.columns.separator = true;
        let xml = columns_xml(&section).replace(
            "<style:columns",
            "<style:columns xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\"",
        );
        let element = parse(&xml).unwrap().root;
        let text_width = 2000 + 360 + 3600 + 240 + 2600;
        let (columns, widths) = read_columns(&element, text_width);
        assert!(columns.separator);
        assert_eq!(widths, section.column_widths);
    }
}
