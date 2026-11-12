//! one-click number formats named in plain words, cell style presets,
//! and small starter workbooks (budget, grades, lab data, schedule,
//! invoice) that show tables, totals and formats without overwhelming.

use serde::Serialize;

use crate::address::Range;
use crate::model::{Cell, CellStyle, Color, Fill, Sheet, Table, TableColumn, Value, Workbook};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendlyFormat {
    pub name: &'static str,
    pub code: &'static str,
    /// 1234.5 (or a date) shown with it
    pub example: String,
}

/// (name, excel code, sample value)
const FORMATS: &[(&str, &str, f64)] = &[
    ("Number", "#,##0.00", 1234.5),
    ("1,234", "#,##0", 1234.5),
    ("Currency", "\"$\"#,##0.00", 1234.5),
    (
        "Accounting",
        "_(\"$\"* #,##0.00_);_(\"$\"* (#,##0.00);_(\"$\"* \"-\"??_);_(@_)",
        1234.5,
    ),
    ("Percent", "0%", 0.256),
    ("Percent with decimals", "0.0%", 0.256),
    ("1.2K", "#,##0.0,\"K\"", 1234.5),
    ("1.2M", "#,##0.0,,\"M\"", 1_234_500.0),
    ("Date short", "m/d/yyyy", 45352.0),
    ("Date long", "dddd, mmmm d, yyyy", 45352.0),
    ("Time", "h:mm AM/PM", 0.75),
    ("Duration", "[h]:mm", 1.5),
    ("Fraction", "# ?/?", 1.5),
    ("Scientific", "0.00E+00", 1234.5),
    ("Phone", "(000) 000-0000", 5_551_234_567.0),
    ("Text", "@", 1234.5),
];

pub fn friendly_formats() -> Vec<FriendlyFormat> {
    FORMATS
        .iter()
        .map(|(name, code, sample)| FriendlyFormat {
            name,
            code,
            example: crate::numfmt::format_number(
                *sample,
                code,
                &crate::numfmt::Locale::default(),
                crate::model::DateSystem::Excel1900,
            )
            .text,
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StylePreset {
    pub name: &'static str,
    pub style: CellStyle,
}

fn preset(name: &'static str, change: impl FnOnce(&mut CellStyle)) -> StylePreset {
    let mut style = CellStyle::default();
    change(&mut style);
    StylePreset { name, style }
}

/// the cell styles of excel's gallery that people use most
pub fn style_presets() -> Vec<StylePreset> {
    vec![
        preset("Title", |s| {
            s.font.bold = true;
            s.font.size = Some(18.0);
            s.font.color = Some(Color {
                theme: Some(3),
                ..Color::default()
            });
        }),
        preset("Heading", |s| {
            s.font.bold = true;
            s.font.size = Some(13.0);
            s.border.bottom = Some(crate::model::BorderEdge {
                style: "medium".into(),
                color: Some(Color {
                    theme: Some(4),
                    ..Color::default()
                }),
            });
        }),
        preset("Total", |s| {
            s.font.bold = true;
            s.border.top = Some(crate::model::BorderEdge {
                style: "thin".into(),
                color: None,
            });
            s.border.bottom = Some(crate::model::BorderEdge {
                style: "double".into(),
                color: None,
            });
        }),
        preset("Good", |s| {
            s.fill = Fill::solid(Color::rgb("C6EFCE"));
            s.font.color = Some(Color::rgb("006100"));
        }),
        preset("Bad", |s| {
            s.fill = Fill::solid(Color::rgb("FFC7CE"));
            s.font.color = Some(Color::rgb("9C0006"));
        }),
        preset("Neutral", |s| {
            s.fill = Fill::solid(Color::rgb("FFEB9C"));
            s.font.color = Some(Color::rgb("9C5700"));
        }),
        preset("Input", |s| {
            s.fill = Fill::solid(Color::rgb("FFCC99"));
            s.font.color = Some(Color::rgb("3F3F76"));
        }),
        preset("Note", |s| s.fill = Fill::solid(Color::rgb("FFFFCC"))),
    ]
}

fn put(sheet: &mut Sheet, address: &str, input: &str, style: u32) {
    let at = crate::address::CellRef::parse(address).expect("templates use valid addresses");
    let cell = match input.strip_prefix('=') {
        Some(formula) => Cell {
            formula: Some(crate::model::Formula::new(formula)),
            style,
            ..Cell::default()
        },
        None => match input.parse::<f64>() {
            Ok(n) => Cell {
                value: Value::Number(n),
                style,
                ..Cell::default()
            },
            Err(_) => Cell {
                value: Value::text(input),
                style,
                ..Cell::default()
            },
        },
    };
    sheet.cells.set(at.row, at.col, cell);
}

fn table(id: u32, name: &str, range: &str, columns: &[&str], totals: bool) -> Table {
    Table {
        id,
        name: name.into(),
        range: Range::parse(range).expect("templates use valid ranges"),
        header_row: true,
        totals_row: totals,
        columns: columns
            .iter()
            .map(|c| TableColumn {
                name: c.to_string(),
                ..TableColumn::default()
            })
            .collect(),
        style: Some("TableStyleMedium2".into()),
        banded_rows: true,
        ..Table::default()
    }
}

/// the starter workbooks by name: "budget", "grades", "lab", "schedule", "invoice"
pub fn template(name: &str) -> Option<Workbook> {
    let mut book = Workbook::new();
    let title = book.styles.derive(0, |s| {
        s.font.bold = true;
        s.font.size = Some(16.0);
    });
    let money = book
        .styles
        .derive(0, |s| s.number_format = "\"$\"#,##0.00".into());
    let bold_money = book.styles.derive(money, |s| s.font.bold = true);
    let date = book
        .styles
        .derive(0, |s| s.number_format = "ddd, mmm d".into());
    let percent = book.styles.derive(0, |s| s.number_format = "0%".into());
    let styles = TemplateStyles {
        title,
        money,
        bold_money,
        date,
        percent,
    };
    let sheet = &mut book.sheets[0];
    match name {
        "budget" => budget(sheet, &styles),
        "grades" => grades(sheet, &styles),
        "lab" => lab(sheet, &styles),
        "schedule" => schedule(sheet, &styles),
        "invoice" => invoice(sheet, &styles),
        _ => return None,
    }
    crate::calc::recalculate(&mut book);
    Some(book)
}

/// the styles every template uses
struct TemplateStyles {
    title: u32,
    money: u32,
    bold_money: u32,
    date: u32,
    percent: u32,
}

fn budget(sheet: &mut Sheet, styles: &TemplateStyles) {
    sheet.name = "Budget".into();
    put(sheet, "A1", "Monthly budget", styles.title);
    for (row, (item, planned, actual)) in [
        ("Rent", "800", "800"),
        ("Food", "300", "342.5"),
        ("Transport", "60", "48"),
        ("Books", "40", "55"),
        ("Fun", "80", "65"),
    ]
    .iter()
    .enumerate()
    {
        let r = row + 4;
        put(sheet, &format!("A{r}"), item, 0);
        put(sheet, &format!("B{r}"), planned, styles.money);
        put(sheet, &format!("C{r}"), actual, styles.money);
        put(
            sheet,
            &format!("D{r}"),
            &format!("=B{r}-C{r}"),
            styles.money,
        );
    }
    for (col, header) in ["A3", "B3", "C3", "D3"]
        .iter()
        .zip(["Item", "Planned", "Actual", "Left"])
    {
        put(sheet, col, header, 0);
    }
    put(sheet, "A9", "Total", 0);
    for col in ["B", "C", "D"] {
        put(
            sheet,
            &format!("{col}9"),
            &format!("=SUM({col}4:{col}8)"),
            styles.bold_money,
        );
    }
    sheet.tables.push(table(
        1,
        "Budget",
        "A3:D8",
        &["Item", "Planned", "Actual", "Left"],
        false,
    ));
}

fn grades(sheet: &mut Sheet, styles: &TemplateStyles) {
    sheet.name = "Grades".into();
    put(sheet, "A1", "Grade tracker", styles.title);
    let headers = ["Assignment", "Weight", "Score"];
    for (col, header) in ["A3", "B3", "C3"].iter().zip(headers) {
        put(sheet, col, header, 0);
    }
    for (row, (task, weight, score)) in [
        ("Essay", "0.2", "0.85"),
        ("Midterm", "0.3", "0.78"),
        ("Lab reports", "0.2", "0.92"),
        ("Final", "0.3", "0.8"),
    ]
    .iter()
    .enumerate()
    {
        let r = row + 4;
        put(sheet, &format!("A{r}"), task, 0);
        put(sheet, &format!("B{r}"), weight, styles.percent);
        put(sheet, &format!("C{r}"), score, styles.percent);
    }
    put(sheet, "A9", "Weighted grade", 0);
    put(
        sheet,
        "C9",
        "=SUMPRODUCT(B4:B7,C4:C7)/SUM(B4:B7)",
        styles.percent,
    );
    put(sheet, "A10", "Letter", 0);
    put(
        sheet,
        "C10",
        "=IFS(C9>=0.9,\"A\",C9>=0.8,\"B\",C9>=0.7,\"C\",C9>=0.6,\"D\",TRUE,\"F\")",
        0,
    );
    sheet
        .tables
        .push(table(1, "Grades", "A3:C7", &headers, false));
}

fn lab(sheet: &mut Sheet, styles: &TemplateStyles) {
    sheet.name = "Lab data".into();
    put(sheet, "A1", "Lab measurements", styles.title);
    for (col, header) in ["A3", "B3", "C3"]
        .iter()
        .zip(["Time (s)", "Distance (m)", "Speed (m/s)"])
    {
        put(sheet, col, header, 0);
    }
    for row in 0..8 {
        let r = row + 4;
        put(sheet, &format!("A{r}"), &(row as f64).to_string(), 0);
        put(
            sheet,
            &format!("B{r}"),
            &format!("{}", 4.9 * (row as f64).powi(2)),
            0,
        );
        put(
            sheet,
            &format!("C{r}"),
            &format!("=IF(A{r}=0,0,B{r}/A{r})"),
            0,
        );
    }
    put(sheet, "E3", "Average speed", 0);
    put(sheet, "E4", "=AVERAGE(C5:C11)", 0);
    sheet.drawings.push(crate::model::DrawingObject::Chart {
        anchor: crate::model::Anchor {
            from_row: 13,
            from_col: 0,
            to_row: 28,
            to_col: 6,
        },
        chart: crate::model::Chart {
            kind: crate::model::ChartKind::Scatter,
            title: Some("Distance over time".into()),
            series: vec![crate::model::ChartSeries {
                name: Some("'Lab data'!$B$3".into()),
                categories: Some("'Lab data'!$A$4:$A$11".into()),
                values: Some("'Lab data'!$B$4:$B$11".into()),
            }],
            legend: false,
        },
        part: None,
    });
}

fn schedule(sheet: &mut Sheet, styles: &TemplateStyles) {
    sheet.name = "Schedule".into();
    put(sheet, "A1", "Weekly schedule", styles.title);
    for (col, header) in ["A3", "B3", "C3", "D3"]
        .iter()
        .zip(["Day", "Class", "Room", "Hours"])
    {
        put(sheet, col, header, 0);
    }
    for (row, (class, room, hours)) in [
        ("Biology", "B204", "2"),
        ("Calculus", "M110", "1.5"),
        ("History", "H301", "1"),
        ("Chemistry lab", "C010", "3"),
        ("Writing", "L220", "1.5"),
    ]
    .iter()
    .enumerate()
    {
        let r = row + 4;
        put(
            sheet,
            &format!("A{r}"),
            &format!("{}", 45292 + row),
            styles.date,
        );
        put(sheet, &format!("B{r}"), class, 0);
        put(sheet, &format!("C{r}"), room, 0);
        put(sheet, &format!("D{r}"), hours, 0);
    }
    put(sheet, "C9", "Hours a week", 0);
    put(sheet, "D9", "=SUM(D4:D8)", 0);
    sheet.view.frozen = Some(crate::model::FrozenPanes { rows: 3, cols: 0 });
}

fn invoice(sheet: &mut Sheet, styles: &TemplateStyles) {
    sheet.name = "Invoice".into();
    put(sheet, "A1", "Invoice", styles.title);
    put(sheet, "A2", "Invoice date", 0);
    put(sheet, "B2", "=TODAY()", styles.date);
    for (col, header) in
        ["A4", "B4", "C4", "D4"]
            .iter()
            .zip(["Description", "Quantity", "Price", "Amount"])
    {
        put(sheet, col, header, 0);
    }
    for (row, (what, quantity, price)) in
        [("Tutoring hour", "3", "25"), ("Printed notes", "2", "4.5")]
            .iter()
            .enumerate()
    {
        let r = row + 5;
        put(sheet, &format!("A{r}"), what, 0);
        put(sheet, &format!("B{r}"), quantity, 0);
        put(sheet, &format!("C{r}"), price, styles.money);
        put(
            sheet,
            &format!("D{r}"),
            &format!("=B{r}*C{r}"),
            styles.money,
        );
    }
    put(sheet, "C8", "Total", 0);
    put(sheet, "D8", "=SUM(D5:D6)", styles.bold_money);
}

pub const TEMPLATES: [&str; 5] = ["budget", "grades", "lab", "schedule", "invoice"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_show_their_examples() {
        let formats = friendly_formats();
        let shown = |name: &str| {
            formats
                .iter()
                .find(|f| f.name == name)
                .unwrap()
                .example
                .clone()
        };
        assert_eq!(shown("Currency"), "$1,234.50");
        assert_eq!(shown("1.2K"), "1.2K");
        assert_eq!(shown("Date long"), "Friday, March 1, 2024");
        assert_eq!(shown("Phone"), "(555) 123-4567");
        assert_eq!(style_presets().len(), 8);
    }

    #[test]
    fn templates_calculate_and_save() {
        for name in TEMPLATES {
            let book = template(name).unwrap();
            let bytes = crate::xlsx::write_xlsx(&book).unwrap();
            assert!(crate::xlsx::read_xlsx(&bytes).is_ok(), "{name}");
        }
        let budget = template("budget").unwrap();
        assert_eq!(budget.value_at("Budget!C9"), Value::Number(1310.5));
        let grades = template("grades").unwrap();
        assert_eq!(grades.value_at("Grades!C10"), Value::text("B"));
    }
}
