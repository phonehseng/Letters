//! a sheet as an html table (for the clipboard and for pasting into
//! documents): shown text, merged cells as spans, and the basic look of
//! each cell (bold, italic, colours, alignment) as inline styles.

use crate::address::{CellRef, Range};
use crate::model::Workbook;
use crate::xml::escape_text;

fn cell_style(book: &Workbook, style: u32) -> String {
    let style = book.styles.get(style);
    let theme = &book.styles.theme_colors;
    let mut css = Vec::new();
    if style.font.bold {
        css.push("font-weight:bold".to_string());
    }
    if style.font.italic {
        css.push("font-style:italic".into());
    }
    if style.font.underline.is_some() {
        css.push("text-decoration:underline".into());
    }
    if let Some(color) = style.font.color.as_ref().and_then(|c| c.to_css(theme)) {
        css.push(format!("color:{color}"));
    }
    if let Some(background) = style.fill.background().and_then(|c| c.to_css(theme)) {
        css.push(format!("background:{background}"));
    }
    if let Some(align) = &style.alignment.horizontal {
        css.push(format!("text-align:{align}"));
    }
    css.join(";")
}

/// a range (or the used range) of a sheet as `<table>` html
pub fn to_html(book: &Workbook, sheet: usize, range: Option<Range>) -> String {
    let Some(target) = book.sheets.get(sheet) else {
        return String::new();
    };
    let Some(range) = range.or_else(|| target.cells.used_range()) else {
        return "<table></table>".into();
    };
    let mut out = String::from("<table>");
    for row in range.start.row..=range.end.row {
        if target.row_hidden(row) {
            continue;
        }
        out.push_str("<tr>");
        for col in range.start.col..=range.end.col {
            let at = CellRef::new(row, col);
            let merge = target.merge_at(row, col);
            if merge.is_some_and(|m| m.start != at) || target.column_hidden(col) {
                continue;
            }
            let mut attributes = String::new();
            if let Some(merge) = merge {
                attributes.push_str(&format!(
                    " colspan=\"{}\" rowspan=\"{}\"",
                    merge.cols(),
                    merge.rows()
                ));
            }
            let (text, style) = match target.cells.get(row, col) {
                Some(cell) => (crate::view::shown(book, cell), cell_style(book, cell.style)),
                None => (String::new(), String::new()),
            };
            if !style.is_empty() {
                attributes.push_str(&format!(" style=\"{style}\""));
            }
            out.push_str(&format!("<td{attributes}>{}</td>", escape_text(&text)));
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cell, Value};

    #[test]
    fn html_has_spans_and_styles() {
        let mut book = Workbook::new();
        let bold = book.styles.derive(0, |s| s.font.bold = true);
        book.sheets[0].cells.set(
            0,
            0,
            Cell {
                value: Value::text("a<b"),
                style: bold,
                ..Cell::default()
            },
        );
        book.sheets[0]
            .cells
            .set(1, 1, Cell::value(Value::Number(2.0)));
        book.sheets[0].merged.push(Range::parse("A1:B1").unwrap());
        let html = to_html(&book, 0, None);
        assert!(html.starts_with("<table><tr><td colspan=\"2\" rowspan=\"1\" style=\"font-weight:bold\">a&lt;b</td></tr>"));
        assert!(html.contains("<td>2</td>"));
    }
}
