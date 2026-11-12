//! the parts a worksheet points to: notes (comments), tables, and
//! drawings with their charts and pictures. drawings and charts stay in
//! the package byte for byte; only a summary is read for the model.

use crate::address::Range;
use crate::model::*;
use crate::xml::{Event, Fragment, Reader};

use super::read_sections::element_text;

/// notes from a comments part
pub fn read_comments(xml: &str) -> Vec<Comment> {
    let mut reader = Reader::new(xml);
    let mut authors = Vec::new();
    let mut comments = Vec::new();
    let mut current: Option<Comment> = None;
    while let Ok(Some(event)) = reader.next() {
        let Event::Start(tag) = event else { continue };
        match tag.local() {
            "author" => authors.push(reader.element_text(&tag).unwrap_or_default()),
            "comment" => {
                let cell = tag
                    .attr("ref")
                    .and_then(|r| crate::address::CellRef::parse(&r));
                let author = tag.attr("authorId").and_then(|a| a.parse::<usize>().ok());
                current = cell.map(|cell| Comment {
                    row: cell.row,
                    col: cell.col,
                    author: author
                        .and_then(|a| authors.get(a).cloned())
                        .unwrap_or_default(),
                    text: String::new(),
                });
            }
            "text" => {
                let text = reader.element_text(&tag).unwrap_or_default();
                if let Some(mut comment) = current.take() {
                    comment.text = text;
                    comments.push(comment);
                }
            }
            _ => {}
        }
    }
    comments
}

/// a table part
pub fn read_table(xml: &str) -> Option<Table> {
    let fragment = Fragment::parse(xml)?;
    let number = |name: &str| fragment.attribute(name).and_then(|v| v.parse::<u32>().ok());
    let name = fragment
        .attribute("displayName")
        .or(fragment.attribute("name"))?
        .to_string();
    let mut table = Table {
        id: number("id").unwrap_or(1),
        name,
        range: fragment.attribute("ref").and_then(Range::parse)?,
        // a missing headerRowCount means one header row (ecma-376 18.5.1.2)
        header_row: number("headerRowCount") != Some(0),
        totals_row: number("totalsRowCount").unwrap_or(0) > 0,
        ..Table::default()
    };
    for (child_name, child) in &fragment.children {
        match child_name.as_str() {
            "tableColumns" => table.columns = read_table_columns(child),
            "autoFilter" => table.filter = super::read_sections::read_auto_filter(child),
            "tableStyleInfo" => read_table_style(child, &mut table),
            _ => {}
        }
    }
    Some(table)
}

fn read_table_columns(xml: &str) -> Vec<TableColumn> {
    let Some(fragment) = Fragment::parse(xml) else {
        return Vec::new();
    };
    let mut columns = Vec::new();
    for (_, child) in &fragment.children {
        let Some(column) = Fragment::parse(child) else {
            continue;
        };
        let formula = |name: &str| {
            column
                .children
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, xml)| element_text(xml))
        };
        columns.push(TableColumn {
            name: column.attribute("name").unwrap_or_default().to_string(),
            totals_function: column.attribute("totalsRowFunction").map(str::to_string),
            totals_label: column.attribute("totalsRowLabel").map(str::to_string),
            totals_formula: formula("totalsRowFormula"),
            calculated_formula: formula("calculatedColumnFormula"),
        });
    }
    columns
}

fn read_table_style(xml: &str, table: &mut Table) {
    let Some(style) = Fragment::parse(xml) else {
        return;
    };
    let flag = |name: &str| style.attribute(name) == Some("1");
    table.style = style.attribute("name").map(str::to_string);
    table.banded_rows = flag("showRowStripes");
    table.banded_columns = flag("showColumnStripes");
    table.first_column = flag("showFirstColumn");
    table.last_column = flag("showLastColumn");
}

/// one anchored object in a drawing part, before its chart is read
pub struct DrawnObject {
    pub anchor: Anchor,
    pub name: String,
    /// relationship id of a chart (c:chart r:id) or picture (a:blip r:embed)
    pub chart_id: Option<String>,
    pub picture_id: Option<String>,
}

/// the objects of a drawing part, in order
pub fn read_drawing(xml: &str) -> Vec<DrawnObject> {
    let mut reader = Reader::new(xml);
    let mut objects = Vec::new();
    let mut current: Option<DrawnObject> = None;
    let mut corner: Option<&str> = None;
    let mut field: Option<&str> = None;
    while let Ok(Some(event)) = reader.next() {
        match event {
            Event::Start(tag) => match tag.local() {
                "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor" => {
                    current = Some(DrawnObject {
                        anchor: Anchor::default(),
                        name: String::new(),
                        chart_id: None,
                        picture_id: None,
                    });
                }
                "from" => corner = Some("from"),
                "to" => corner = Some("to"),
                "col" | "row" => field = Some(if tag.local() == "col" { "col" } else { "row" }),
                "cNvPr" => {
                    if let (Some(object), Some(name)) = (current.as_mut(), tag.attr("name")) {
                        object.name = name.into_owned();
                    }
                }
                "chart" => {
                    if let Some(object) = current.as_mut() {
                        object.chart_id = tag.attr("r:id").map(|id| id.into_owned());
                    }
                }
                "blip" => {
                    if let Some(object) = current.as_mut() {
                        object.picture_id = tag.attr("r:embed").map(|id| id.into_owned());
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if let (Some(object), Some(corner), Some(field)) = (current.as_mut(), corner, field)
                {
                    let value: u32 = text.trim().parse().unwrap_or(0);
                    match (corner, field) {
                        ("from", "col") => object.anchor.from_col = value,
                        ("from", "row") => object.anchor.from_row = value,
                        ("to", "col") => object.anchor.to_col = value,
                        _ => object.anchor.to_row = value,
                    }
                }
            }
            Event::End { local, .. } => match local {
                "col" | "row" => field = None,
                "from" | "to" => corner = None,
                "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor" => {
                    objects.extend(current.take())
                }
                _ => {}
            },
        }
    }
    objects
}

/// what a chart part shows: its kind, title, series ranges and legend
pub fn read_chart(xml: &str) -> Chart {
    let mut reader = Reader::new(xml);
    let mut chart = Chart {
        kind: ChartKind::Other,
        title: None,
        series: Vec::new(),
        legend: false,
    };
    let mut in_title = false;
    let mut title_text = String::new();
    // which part of a series the next formula belongs to
    let mut slot: Option<&str> = None;
    let mut in_formula = false;
    while let Ok(Some(event)) = reader.next() {
        match event {
            Event::Start(tag) => match tag.local() {
                "title" if chart.series.is_empty() => in_title = true,
                "legend" => chart.legend = true,
                "barDir" if tag.attr("val").as_deref() == Some("bar") => {
                    chart.kind = ChartKind::Bar
                }
                "ser" => chart.series.push(ChartSeries::default()),
                "tx" | "cat" | "val" | "xVal" | "yVal" => slot = Some(series_slot(tag.local())),
                "f" => in_formula = true,
                kind => {
                    if let Some(found) = chart_kind(kind) {
                        if chart.kind == ChartKind::Other {
                            chart.kind = found;
                        }
                    }
                }
            },
            Event::Text(text) => {
                if in_formula {
                    if let (Some(series), Some(slot)) = (chart.series.last_mut(), slot) {
                        let formula = Some(text.trim().to_string());
                        match slot {
                            "name" => series.name = formula,
                            "categories" => series.categories = formula,
                            _ => series.values = formula,
                        }
                    }
                } else if in_title {
                    title_text.push_str(&text);
                }
            }
            Event::End { local, .. } => match local {
                "f" => in_formula = false,
                "title" => in_title = false,
                "tx" | "cat" | "val" | "xVal" | "yVal" => slot = None,
                _ => {}
            },
        }
    }
    if !title_text.trim().is_empty() {
        chart.title = Some(title_text.trim().to_string());
    }
    chart
}

fn series_slot(element: &str) -> &'static str {
    match element {
        "tx" => "name",
        "cat" | "xVal" => "categories",
        _ => "values",
    }
}

fn chart_kind(element: &str) -> Option<ChartKind> {
    Some(match element {
        // a bar chart is a column chart until its barDir says "bar"
        "barChart" | "bar3DChart" => ChartKind::Column,
        "lineChart" | "line3DChart" => ChartKind::Line,
        "pieChart" | "pie3DChart" | "ofPieChart" => ChartKind::Pie,
        "doughnutChart" => ChartKind::Doughnut,
        "scatterChart" => ChartKind::Scatter,
        "areaChart" | "area3DChart" => ChartKind::Area,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_comments_tables_and_charts() {
        let comments = read_comments(
            r#"<comments><authors><author>Ann</author></authors><commentList><comment ref="B2" authorId="0"><text><r><t>Check </t></r><r><t>this</t></r></text></comment></commentList></comments>"#,
        );
        assert_eq!(comments[0].text, "Check this");
        assert_eq!(
            (
                comments[0].row,
                comments[0].col,
                comments[0].author.as_str()
            ),
            (1, 1, "Ann")
        );
        let table = read_table(r#"<table id="3" name="Table1" displayName="Sales" ref="A1:B4" totalsRowCount="1"><autoFilter ref="A1:B3"/><tableColumns count="2"><tableColumn id="1" name="Item" totalsRowLabel="Total"/><tableColumn id="2" name="Price" totalsRowFunction="sum"/></tableColumns><tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/></table>"#).unwrap();
        assert_eq!(table.name, "Sales");
        assert!(table.header_row && table.totals_row && table.banded_rows);
        assert_eq!(table.data_range().unwrap().to_a1(), "A2:B3");
        let chart = read_chart(
            r#"<c:chartSpace><c:chart><c:title><c:tx><c:rich><a:p><a:r><a:t>Sales</a:t></a:r></a:p></c:rich></c:tx></c:title><c:plotArea><c:barChart><c:barDir val="col"/><c:ser><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f></c:strRef></c:tx><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$4</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$2:$B$4</c:f></c:numRef></c:val></c:ser></c:barChart></c:plotArea><c:legend/></c:chart></c:chartSpace>"#,
        );
        assert_eq!(chart.kind, ChartKind::Column);
        assert_eq!(chart.title.as_deref(), Some("Sales"));
        assert_eq!(chart.series[0].values.as_deref(), Some("Sheet1!$B$2:$B$4"));
        assert!(chart.legend);
    }
}
