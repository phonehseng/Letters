//! parser tests: reading and printing back, precedence, file prefixes,
//! r1c1, openformula and broken input.

use super::*;
use crate::formula::ast::{BinaryOp, UnaryOp};
use crate::formula::print::to_string;

fn round_trip(text: &str) -> String {
    let expr = parse(text).unwrap_or_else(|error| panic!("{text}: {error}"));
    to_string(&expr)
}

#[test]
fn formulas_print_back_as_they_were_written() {
    for text in [
        "1+2*3",
        "SUM(A1:B3,5)",
        "-2^2",
        "Sheet2!$A$1+'My sheet'!B2:C3",
        "Sheet1:Sheet3!A1",
        "IF(A1>=10,\"big\",\"small\")",
        "{1,2;3,4}",
        "A:A",
        "$3:$5",
        "SUM((A1:B2,C3))",
        "A1:B5 B2:C3",
        "Table1[Price]",
        "Table1[[#Headers],[Price]:[Tax]]",
        "[@Price]*2",
        "A1#",
        "@A1:A5",
        "IF(A1,,2)",
        "10%",
        "\"say \"\"hi\"\"\"",
        "#REF!+1",
        "TRUE",
        "Rate*2",
        "XLOOKUP(A1,B:B,C:C)",
        "[1]Sheet1!A1",
        "LOG10(100)",
    ] {
        assert_eq!(round_trip(text), text, "{text}");
    }
}

#[test]
fn operator_precedence_is_excels() {
    let Expr::Binary {
        op: BinaryOp::Add,
        right,
        ..
    } = parse("1+2*3^2").unwrap()
    else {
        panic!("+ should be the loosest operator");
    };
    assert!(matches!(
        *right,
        Expr::Binary {
            op: BinaryOp::Multiply,
            ..
        }
    ));
    // unary minus binds tighter than ^, so -2^2 is 4 in excel
    let Expr::Binary {
        op: BinaryOp::Power,
        left,
        ..
    } = parse("-2^2").unwrap()
    else {
        panic!("^ should be outside the minus");
    };
    assert!(matches!(
        *left,
        Expr::Unary {
            op: UnaryOp::Minus,
            ..
        }
    ));
    // ^ is left associative: (2^3)^2
    let Expr::Binary {
        op: BinaryOp::Power,
        left,
        ..
    } = parse("2^3^2").unwrap()
    else {
        panic!("2^3^2 should be a power");
    };
    assert!(matches!(
        *left,
        Expr::Binary {
            op: BinaryOp::Power,
            ..
        }
    ));
    assert!(matches!(
        parse("1&2=\"12\"").unwrap(),
        Expr::Binary {
            op: BinaryOp::Equal,
            ..
        }
    ));
}

#[test]
fn numbers_keep_fifteen_significant_digits() {
    assert_eq!(
        literals::keep_fifteen_digits("123456789012345678"),
        123_456_789_012_345_000.0
    );
    assert_eq!(
        literals::keep_fifteen_digits("0.1234567890123456789"),
        0.123_456_789_012_345
    );
    assert_eq!(literals::keep_fifteen_digits("1.5E+3"), 1500.0);
}

#[test]
fn file_prefixes_are_removed() {
    assert_eq!(round_trip("_xlfn.XLOOKUP(1,A:A,B:B)"), "XLOOKUP(1,A:A,B:B)");
    assert_eq!(round_trip("_xlfn._xlws.SORT(A1:A3)"), "SORT(A1:A3)");
    assert_eq!(round_trip("=sum(a1)"), "SUM(A1)");
}

#[test]
fn r1c1_references_are_relative_to_their_cell() {
    let at = crate::address::CellRef::new(4, 4);
    let expr = parse_r1c1("R[-1]C+R1C1+SUM(R[-3]C[-2]:R[-1]C[-2])", at).unwrap();
    assert_eq!(to_string(&expr), "E4+$A$1+SUM(C2:C4)");
}

#[test]
fn open_formula_is_read_into_the_same_tree() {
    let expr = parse_open_formula("of:=SUM([.A1:.B3];[$Sheet2.C1];['My sheet'.D2:.D4])").unwrap();
    assert_eq!(to_string(&expr), "SUM(A1:B3,Sheet2!C1,'My sheet'!D2:D4)");
    let expr = parse_open_formula("of:=IF([.A1]>0;TRUE();{1;2|3;4})").unwrap();
    assert_eq!(to_string(&expr), "IF(A1>0,TRUE,{1,2;3,4})");
    let expr = parse_open_formula("of:=COM.MICROSOFT.CONCAT([.A1];\"x\")+SUM([.A:.A])").unwrap();
    assert_eq!(to_string(&expr), "CONCAT(A1,\"x\")+SUM(A:A)");
    let expr = parse_open_formula("of:=SUM([$Jan.A1:$Mar.B2])").unwrap();
    assert_eq!(to_string(&expr), "SUM(Jan:Mar!A1:B2)");
}

#[test]
fn bad_formulas_give_errors_not_panics() {
    for text in [
        "",
        "SUM(",
        "1+",
        "\"open",
        "{1,2;3}",
        "A1:",
        "'Sheet",
        "((((",
        "$",
        "Table1[[x]",
        "#WHAT",
    ] {
        assert!(parse(text).is_err(), "{text:?}");
    }
    let deep = "(".repeat(500) + "1" + &")".repeat(500);
    assert!(parse(&deep).is_err());
}
