//! each rule kind on a small sheet, priority and stop if true, and the
//! three rules of a tester's workbook (greater than 50, a two-colour
//! scale and data bars) built here cell for cell.

use super::*;
use crate::model::{Cell, ConditionalFormat, ConditionalRule, ErrorValue};

/// a sheet whose column A holds `values` from A1 down
fn book_with(values: &[Value]) -> Workbook {
    let mut book = Workbook::new();
    for (row, value) in values.iter().enumerate() {
        if !value.is_blank() {
            book.sheets[0]
                .cells
                .set(row as u32, 0, Cell::value(value.clone()));
        }
    }
    book
}

fn numbers(values: &[f64]) -> Vec<Value> {
    values.iter().map(|n| Value::Number(*n)).collect()
}

/// a red-on-pink format, as excel's "light red fill with dark red text"
fn red_format(book: &mut Workbook) -> u32 {
    book.styles
        .add_differential(Some("9C0006"), Some("FFC7CE"), false)
}

fn rule(kind: &str, format: Option<u32>, priority: i32) -> ConditionalRule {
    ConditionalRule {
        kind: kind.into(),
        format,
        priority,
        ..ConditionalRule::default()
    }
}

fn cell_is(operator: &str, formulas: &[&str], format: u32, priority: i32) -> ConditionalRule {
    ConditionalRule {
        operator: Some(operator.into()),
        formulas: formulas.iter().map(|f| f.to_string()).collect(),
        ..rule("cellIs", Some(format), priority)
    }
}

fn with_attributes(mut rule: ConditionalRule, attributes: &[(&str, &str)]) -> ConditionalRule {
    rule.attributes = attributes
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    rule
}

fn add(book: &mut Workbook, ranges: &str, rules: Vec<ConditionalRule>) {
    book.sheets[0].conditional_formats.push(ConditionalFormat {
        ranges: Range::parse_list(ranges),
        rules,
    });
}

/// the rows (one based, as a person reads them) that get a conditional
/// fill inside `view`
fn filled_rows(book: &Workbook, view: &str) -> Vec<u32> {
    looks_in(book, 0, Range::parse(view).unwrap())
        .into_iter()
        .filter(|(_, look)| look.fill.is_some())
        .map(|(at, _)| at.row + 1)
        .collect()
}

/// one rule of `kind` with the red format over A1:A{count}, and the rows it fills
fn rows_marked(values: &[Value], rule: ConditionalRule) -> Vec<u32> {
    let mut book = book_with(values);
    let red = red_format(&mut book);
    let rule = ConditionalRule {
        format: Some(red),
        ..rule
    };
    add(&mut book, &format!("A1:A{}", values.len()), vec![rule]);
    filled_rows(&book, "A1:A100")
}

fn look_at(book: &Workbook, cell: &str) -> Option<ConditionalLook> {
    let at = CellRef::parse(cell).unwrap();
    looks_in(book, 0, Range::cell(at))
        .into_iter()
        .find(|(found, _)| *found == at)
        .map(|(_, look)| look)
}

#[test]
fn nothing_is_sent_without_rules() {
    let book = book_with(&numbers(&[1.0, 2.0]));
    assert!(looks_in(&book, 0, Range::parse("A1:Z99").unwrap()).is_empty());
}

#[test]
fn greater_than_draws_the_formats_colours() {
    let mut book = book_with(&numbers(&[40.0, 50.0, 60.0]));
    let red = red_format(&mut book);
    add(
        &mut book,
        "A1:A5",
        vec![cell_is("greaterThan", &["50"], red, 1)],
    );
    assert_eq!(filled_rows(&book, "A1:A10"), vec![3]);
    let look = look_at(&book, "A3").unwrap();
    assert_eq!(look.fill.as_deref(), Some("#FFC7CE"));
    assert_eq!(look.color.as_deref(), Some("#9C0006"));
    assert_eq!(look.bold, None);
}

#[test]
fn below_the_pass_mark_marks_empty_cells_too() {
    // a gradebook: marks under the pass mark in C1 go red. excel counts an
    // empty cell as 0, so it is below the mark as well
    let mut book = book_with(&[
        Value::Number(72.0),
        Value::Number(38.0),
        Value::Blank,
        Value::Number(50.0),
    ]);
    book.sheets[0]
        .cells
        .set(0, 2, Cell::value(Value::Number(50.0)));
    let red = red_format(&mut book);
    add(
        &mut book,
        "A1:A4",
        vec![cell_is("lessThan", &["$C$1"], red, 1)],
    );
    assert_eq!(filled_rows(&book, "A1:A10"), vec![2, 3]);
}

#[test]
fn every_comparison_works_like_excels() {
    let values = numbers(&[1.0, 5.0, 10.0, 15.0]);
    let marked =
        |operator: &str, formulas: &[&str]| rows_marked(&values, cell_is(operator, formulas, 0, 1));
    assert_eq!(marked("between", &["5", "10"]), vec![2, 3]);
    // the bounds may come in either order
    assert_eq!(marked("between", &["10", "5"]), vec![2, 3]);
    assert_eq!(marked("notBetween", &["5", "10"]), vec![1, 4]);
    assert_eq!(marked("equal", &["10"]), vec![3]);
    assert_eq!(marked("notEqual", &["10"]), vec![1, 2, 4]);
    assert_eq!(marked("greaterThanOrEqual", &["10"]), vec![3, 4]);
    assert_eq!(marked("lessThanOrEqual", &["5"]), vec![1, 2]);
    assert_eq!(marked("lessThan", &["5"]), vec![1]);
}

#[test]
fn text_comparisons_ignore_case_and_errors_never_match() {
    let values = vec![
        Value::text("Pass"),
        Value::text("fail"),
        Value::Error(ErrorValue::Div0),
    ];
    assert_eq!(
        rows_marked(&values, cell_is("equal", &["\"PASS\""], 0, 1)),
        vec![1]
    );
    assert_eq!(
        rows_marked(&values, cell_is("notEqual", &["\"pass\""], 0, 1)),
        vec![2]
    );
}

#[test]
fn operands_with_relative_references_move_with_the_cell() {
    // each mark in A is compared with the mark beside it in B
    let mut book = book_with(&numbers(&[5.0, 9.0, 3.0]));
    for (row, n) in [7.0, 2.0, 3.0].iter().enumerate() {
        book.sheets[0]
            .cells
            .set(row as u32, 1, Cell::value(Value::Number(*n)));
    }
    let red = red_format(&mut book);
    add(
        &mut book,
        "A1:A3",
        vec![cell_is("lessThan", &["B1"], red, 1)],
    );
    assert_eq!(filled_rows(&book, "A1:B5"), vec![1]);
}

#[test]
fn expressions_are_calculated_for_each_cell() {
    let values = numbers(&[10.0, 20.0, 30.0, 40.0]);
    let striped = ConditionalRule {
        formulas: vec!["MOD(ROW(),2)=0".into()],
        ..rule("expression", None, 1)
    };
    assert_eq!(rows_marked(&values, striped), vec![2, 4]);
    let relative = ConditionalRule {
        formulas: vec!["AND(A1>15,A1<35)".into()],
        ..rule("expression", None, 1)
    };
    assert_eq!(rows_marked(&values, relative), vec![2, 3]);
    let text_result = ConditionalRule {
        formulas: vec!["\"yes\"".into()],
        ..rule("expression", None, 1)
    };
    assert!(rows_marked(&values, text_result).is_empty());
}

#[test]
fn text_rules_look_for_their_text() {
    let values = vec![
        Value::text("Banana"),
        Value::text("apple pie"),
        Value::text("Pineapple"),
        Value::Number(1.5),
        Value::Error(ErrorValue::NA),
    ];
    let text_rule = |kind: &str, text: &str| ConditionalRule {
        text: Some(text.into()),
        ..rule(kind, None, 1)
    };
    assert_eq!(
        rows_marked(&values, text_rule("containsText", "APPLE")),
        vec![2, 3]
    );
    assert_eq!(
        rows_marked(&values, text_rule("notContainsText", "apple")),
        vec![1, 4, 5]
    );
    assert_eq!(
        rows_marked(&values, text_rule("beginsWith", "app")),
        vec![2]
    );
    assert_eq!(
        rows_marked(&values, text_rule("endsWith", "APPLE")),
        vec![3]
    );
    // a number is searched as its plain digits
    assert_eq!(
        rows_marked(&values, text_rule("containsText", ".5")),
        vec![4]
    );
}

#[test]
fn blank_and_error_rules() {
    let values = vec![
        Value::Number(1.0),
        Value::Blank,
        Value::text("   "),
        Value::Error(ErrorValue::Value),
    ];
    assert_eq!(
        rows_marked(&values, rule("containsBlanks", None, 1)),
        vec![2, 3]
    );
    assert_eq!(
        rows_marked(&values, rule("notContainsBlanks", None, 1)),
        vec![1, 4]
    );
    assert_eq!(
        rows_marked(&values, rule("containsErrors", None, 1)),
        vec![4]
    );
    assert_eq!(
        rows_marked(&values, rule("notContainsErrors", None, 1)),
        vec![1, 2, 3]
    );
}

#[test]
fn top_and_bottom_rules_count_from_their_end() {
    let values = vec![
        Value::Number(10.0),
        Value::Number(50.0),
        Value::Number(30.0),
        Value::Number(50.0),
        Value::text("n/a"),
        Value::Number(20.0),
    ];
    let top = |attributes: &[(&str, &str)]| {
        rows_marked(&values, with_attributes(rule("top10", None, 1), attributes))
    };
    // the top 1 is 50, and both 50s tie for it
    assert_eq!(top(&[("rank", "1")]), vec![2, 4]);
    assert_eq!(top(&[("rank", "3")]), vec![2, 3, 4]);
    assert_eq!(top(&[("rank", "2"), ("bottom", "1")]), vec![1, 6]);
    // 40% of five numbers is two
    assert_eq!(top(&[("rank", "40"), ("percent", "1")]), vec![2, 4]);
    // 10% of five rounds down to none, but at least one is marked
    assert_eq!(
        top(&[("rank", "10"), ("percent", "1"), ("bottom", "1")]),
        vec![1]
    );
}

#[test]
fn average_rules_compare_with_the_mean() {
    // mean 5, population deviation 2
    let values = numbers(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
    let average = |attributes: &[(&str, &str)]| {
        rows_marked(
            &values,
            with_attributes(rule("aboveAverage", None, 1), attributes),
        )
    };
    assert_eq!(average(&[]), vec![7, 8]);
    assert_eq!(average(&[("equalAverage", "1")]), vec![5, 6, 7, 8]);
    assert_eq!(average(&[("aboveAverage", "0")]), vec![1, 2, 3, 4]);
    assert_eq!(average(&[("stdDev", "1")]), vec![8]);
    assert_eq!(average(&[("aboveAverage", "0"), ("stdDev", "1")]), vec![1]);
}

#[test]
fn duplicate_and_unique_values() {
    let values = vec![
        Value::text("Ann"),
        Value::text("Ben"),
        Value::text("ann"),
        Value::Number(3.0),
        Value::Blank,
        Value::Number(3.0),
    ];
    assert_eq!(
        rows_marked(&values, rule("duplicateValues", None, 1)),
        vec![1, 3, 4, 6]
    );
    assert_eq!(rows_marked(&values, rule("uniqueValues", None, 1)), vec![2]);
}

#[test]
fn colour_scales_colour_every_number_between_their_ends() {
    let mut book = book_with(&[
        Value::Number(0.0),
        Value::Number(25.0),
        Value::Number(100.0),
        Value::text("skip"),
    ]);
    add(
        &mut book,
        "A1:A4",
        vec![ConditionalRule {
            details_xml: Some(r#"<colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FFFFFFFF"/><color rgb="FF000000"/></colorScale>"#.into()),
            ..rule("colorScale", None, 1)
        }],
    );
    assert_eq!(
        look_at(&book, "A1").unwrap().fill.as_deref(),
        Some("#FFFFFF")
    );
    // a quarter of the way from white to black: 255 * 0.75 = 191.25
    assert_eq!(
        look_at(&book, "A2").unwrap().fill.as_deref(),
        Some("#BFBFBF")
    );
    assert_eq!(
        look_at(&book, "A3").unwrap().fill.as_deref(),
        Some("#000000")
    );
    assert_eq!(look_at(&book, "A4"), None);
}

#[test]
fn data_bars_measure_each_number_against_the_range() {
    let mut book = book_with(&numbers(&[0.0, 50.0, 100.0]));
    add(
        &mut book,
        "A1:A3",
        vec![ConditionalRule {
            details_xml: Some(r#"<dataBar showValue="0"><cfvo type="num" val="0"/><cfvo type="num" val="200"/><color rgb="FF638EC6"/></dataBar>"#.into()),
            ..rule("dataBar", None, 1)
        }],
    );
    let fractions: Vec<f64> = ["A1", "A2", "A3"]
        .iter()
        .map(|cell| look_at(&book, cell).unwrap().bar.unwrap().fraction)
        .collect();
    // 10% at 0 up to 90% at 200: 50 is a quarter of the way (30%)
    assert_eq!(fractions, vec![0.1, 0.3, 0.5]);
    let bar = look_at(&book, "A1").unwrap().bar.unwrap();
    assert!(bar.hide_value);
    assert_eq!(bar.color, "#638EC6");
}

#[test]
fn earlier_rules_win_and_stop_if_true_ends_the_list() {
    let mut book = book_with(&numbers(&[80.0, 60.0, 20.0]));
    let red = red_format(&mut book);
    let green_bold = book
        .styles
        .add_differential(Some("006100"), Some("C6EFCE"), true);
    add(
        &mut book,
        "A1:A3",
        vec![
            // listed second but asked first: priority 1
            cell_is("greaterThan", &["50"], green_bold, 2),
            cell_is("greaterThan", &["70"], red, 1),
        ],
    );
    let top = look_at(&book, "A1").unwrap();
    // both match: red wins the fill and colour, and bold still comes
    // from the green rule, which sets what red leaves alone
    assert_eq!(top.fill.as_deref(), Some("#FFC7CE"));
    assert_eq!(top.bold, Some(true));
    assert_eq!(
        look_at(&book, "A2").unwrap().fill.as_deref(),
        Some("#C6EFCE")
    );

    book.sheets[0].conditional_formats[0].rules[1].stop_if_true = true;
    let stopped = look_at(&book, "A1").unwrap();
    assert_eq!(stopped.bold, None);
    assert_eq!(look_at(&book, "A2").unwrap().bold, Some(true));
}

#[test]
fn a_stop_if_true_rule_hides_a_data_bar_below_it() {
    let mut book = book_with(&numbers(&[1.0, 2.0, 3.0]));
    add(
        &mut book,
        "A1:A3",
        vec![
            ConditionalRule {
                stop_if_true: true,
                ..cell_is("equal", &["2"], 0, 1)
            },
            ConditionalRule {
                details_xml: Some(r#"<dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar>"#.into()),
                ..rule("dataBar", None, 2)
            },
        ],
    );
    assert!(look_at(&book, "A1").unwrap().bar.is_some());
    assert_eq!(look_at(&book, "A2"), None);
}

/// column j (B=1, C=2, D=3) of row r (one based) in the tester's cf.xlsx
fn tester_value(col: u32, row: u32) -> f64 {
    f64::from((13 * col + 7 * (row - 1)) % 100)
}

/// cf.xlsx (5.8 MB, so rebuilt here): B1:B40 greater than 50 in red on
/// pink, C1:C40 a red-to-green scale, D1:D40 blue data bars
fn tester_workbook() -> Workbook {
    let mut book = Workbook::new();
    for row in 1..=40 {
        for col in 1..=3 {
            book.sheets[0].cells.set(
                row - 1,
                col,
                Cell::value(Value::Number(tester_value(col, row))),
            );
        }
    }
    book.styles.differential.push(
        r#"<dxf><font><color rgb="FF9C0006"/></font><fill><patternFill><bgColor rgb="FFFFC7CE"/></patternFill></fill></dxf>"#.into(),
    );
    add(
        &mut book,
        "B1:B40",
        vec![cell_is("greaterThan", &["50"], 0, 1)],
    );
    add(
        &mut book,
        "C1:C40",
        vec![ConditionalRule {
            details_xml: Some(r#"<colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FFF8696B"/><color rgb="FF63BE7B"/></colorScale>"#.into()),
            ..rule("colorScale", None, 2)
        }],
    );
    add(
        &mut book,
        "D1:D40",
        vec![ConditionalRule {
            details_xml: Some(
                r#"<dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar>"#
                    .into(),
            ),
            ..rule("dataBar", None, 3)
        }],
    );
    book
}

#[test]
fn the_testers_three_rules_are_drawn() {
    let book = tester_workbook();
    let looks: HashMap<CellRef, ConditionalLook> =
        looks_in(&book, 0, Range::parse("A1:E40").unwrap())
            .into_iter()
            .collect();
    let at = |cell: &str| looks.get(&CellRef::parse(cell).unwrap());
    // B7 is 55, B6 is 48
    assert_eq!(at("B7").unwrap().fill.as_deref(), Some("#FFC7CE"));
    assert_eq!(at("B7").unwrap().color.as_deref(), Some("#9C0006"));
    assert!(at("B6").is_none());
    let over_fifty = (1..=40).filter(|row| tester_value(1, *row) > 50.0).count();
    assert_eq!(
        looks.keys().filter(|cell| cell.col == 1).count(),
        over_fifty
    );
    // C26 holds the column's least (1), C40 its most (99)
    assert_eq!(at("C26").unwrap().fill.as_deref(), Some("#F8696B"));
    assert_eq!(at("C40").unwrap().fill.as_deref(), Some("#63BE7B"));
    // C33 is 50: 49/98 of the way, so each channel sits halfway
    assert_eq!(at("C33").unwrap().fill.as_deref(), Some("#AE9473"));
    // D24 is the least (0), D38 the most (98), D1 is 39
    assert_eq!(at("D24").unwrap().bar.as_ref().unwrap().fraction, 0.1);
    assert_eq!(at("D38").unwrap().bar.as_ref().unwrap().fraction, 0.9);
    let d1 = at("D1").unwrap().bar.as_ref().unwrap();
    assert_eq!(
        d1.fraction,
        ((10.0 + 39.0 / 98.0 * 80.0) / 100.0 * 1000.0_f64).round() / 1000.0
    );
    assert!(d1.gradient);
    // nothing outside the three ranges
    assert!(looks
        .keys()
        .all(|cell| (1..=3).contains(&cell.col) && cell.row < 40));
}

#[test]
fn a_whole_column_rule_only_looks_at_the_view() {
    let mut book = Workbook::new();
    for row in 0..5000 {
        book.sheets[0]
            .cells
            .set(row, 0, Cell::value(Value::Number(f64::from(row))));
    }
    add(
        &mut book,
        "A:A",
        vec![with_attributes(rule("top10", None, 1), &[("rank", "10")])],
    );
    book.sheets[0].conditional_formats[0].rules[0].format = Some(0);
    book.styles
        .differential
        .push(r#"<dxf><font><b/></font></dxf>"#.into());
    let looks = looks_in(&book, 0, Range::parse("A4980:A5100").unwrap());
    let rows: Vec<u32> = looks.iter().map(|(at, _)| at.row).collect();
    assert_eq!(rows, (4990..5000).collect::<Vec<_>>());
}
