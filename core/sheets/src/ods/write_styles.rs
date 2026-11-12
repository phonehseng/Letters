//! the automatic styles an ods file needs: one cell style per model style
//! (with its data style), one column style per width, one row style per
//! height and one table style per sheet. names start with "Ltr" so they
//! can never clash with styles kept from the file.

use std::collections::BTreeMap;

use crate::model::Workbook;

use super::styles::cell_style_xml;
use super::units::{column_width, row_height};

pub struct StyleNames {
    /// widths and heights in hundredths, so equal sizes share a style
    columns: BTreeMap<i64, String>,
    rows: BTreeMap<i64, String>,
    tables: usize,
}

fn key(size: f64) -> i64 {
    (size * 100.0).round() as i64
}

impl StyleNames {
    pub fn cell_style(&self, index: u32) -> String {
        format!("LtrCe{index}")
    }

    pub fn column_style(&self, width: f64) -> String {
        self.columns
            .get(&key(width))
            .cloned()
            .unwrap_or_else(|| "LtrCo0".into())
    }

    pub fn row_style(&self, height: f64) -> String {
        self.rows
            .get(&key(height))
            .cloned()
            .unwrap_or_else(|| "LtrRo0".into())
    }

    pub fn table_style(&self, index: usize) -> String {
        format!("LtrTa{}", index.min(self.tables))
    }
}

/// the names and the `<office:automatic-styles>` element
pub fn automatic_styles(book: &Workbook) -> (StyleNames, String) {
    let mut names = StyleNames {
        columns: BTreeMap::new(),
        rows: BTreeMap::new(),
        tables: book.sheets.len(),
    };
    let mut out = String::from("<office:automatic-styles>");
    for sheet in &book.sheets {
        let used_cols = sheet.cells.used_range().map(|r| r.end.col).unwrap_or(0);
        for col in 0..=used_cols.max(sheet.columns.iter().map(|c| c.last).max().unwrap_or(0)) {
            let width = sheet.column_width(col);
            let next = names.columns.len();
            names.columns.entry(key(width)).or_insert_with(|| {
                out.push_str(&format!(
                    "<style:style style:name=\"LtrCo{next}\" style:family=\"table-column\"><style:table-column-properties fo:break-before=\"auto\" style:column-width=\"{}\"/></style:style>",
                    column_width(width)
                ));
                format!("LtrCo{next}")
            });
        }
        let mut heights: Vec<f64> = sheet.rows.keys().map(|r| sheet.row_height(*r)).collect();
        heights.push(
            sheet
                .default_row_height
                .unwrap_or(crate::model::DEFAULT_ROW_HEIGHT),
        );
        for height in heights {
            let next = names.rows.len();
            let custom = (height - crate::model::DEFAULT_ROW_HEIGHT).abs() > 0.01;
            names.rows.entry(key(height)).or_insert_with(|| {
                out.push_str(&format!(
                    "<style:style style:name=\"LtrRo{next}\" style:family=\"table-row\"><style:table-row-properties style:row-height=\"{}\" fo:break-before=\"auto\" style:use-optimal-row-height=\"{}\"/></style:style>",
                    row_height(height),
                    !custom
                ));
                format!("LtrRo{next}")
            });
        }
    }
    for (index, sheet) in book.sheets.iter().enumerate() {
        let display = sheet.visibility == crate::model::Visibility::Visible;
        let tab = sheet
            .view
            .tab_color
            .as_ref()
            .and_then(|c| c.to_css(&book.styles.theme_colors))
            .map(|c| format!(" tableooo:tab-color=\"{}\"", c.to_ascii_lowercase()))
            .unwrap_or_default();
        out.push_str(&format!(
            "<style:style style:name=\"LtrTa{index}\" style:family=\"table\" style:master-page-name=\"Default\"><style:table-properties table:display=\"{display}\" style:writing-mode=\"lr-tb\"{tab}/></style:style>"
        ));
    }
    for (index, style) in book.styles.styles.iter().enumerate() {
        let data = (style.number_format != "General").then(|| format!("LtrN{index}"));
        if let Some(data) = &data {
            out.push_str(&crate::numfmt::odf::excel_to_odf(
                &style.number_format,
                data,
            ));
        }
        out.push_str(&cell_style_xml(
            &names.cell_style(index as u32),
            data.as_deref(),
            style,
            &book.styles.theme_colors,
        ));
    }
    out.push_str("</office:automatic-styles>");
    (names, out)
}
