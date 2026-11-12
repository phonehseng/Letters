//! the formula cases the verification harness asks excel, libreoffice
//! and letters to calculate: every function family over awkward inputs
//! (negatives, fractions, huge and tiny numbers, text that looks like a
//! number, logical values, blanks and errors), the operators over every
//! pair of kinds, and dynamic array functions wrapped so they give one
//! value. a fixed `Data` sheet holds the ranges the cases read.

use letters_sheets::model::*;

use super::cases_more::{arrays, dates, financial, information, text};

pub struct Case {
    pub family: &'static str,
    pub formula: String,
}

pub fn case(family: &'static str, formula: impl Into<String>) -> Case {
    Case {
        family,
        formula: formula.into(),
    }
}

/// numbers that probe rounding, signs and extremes
const NUMBERS: [&str; 15] = [
    "0",
    "1",
    "-1",
    "2.5",
    "-2.5",
    "0.5",
    "3.14159",
    "100",
    "1E+20",
    "1E-20",
    "0.1",
    "123456.789",
    "-0.0001",
    "7",
    "2.675",
];

/// every kind of value an argument can be
pub const ANYTHING: [&str; 20] = [
    "0",
    "1",
    "-2.5",
    "0.1",
    "1E+300",
    "\"3\"",
    "\" 4 \"",
    "\"1,000\"",
    "\"50%\"",
    "\"abc\"",
    "\"\"",
    "TRUE",
    "FALSE",
    "Data!H1",
    "Data!C8",
    "Data!A1",
    "Data!B1",
    "\"2024-03-01\"",
    "\"10:30\"",
    "#N/A",
];

pub const TEXTS: [&str; 9] = [
    "\"Hello World\"",
    "\"  two   spaces  \"",
    "\"mIxEd caSe\"",
    "\"\"",
    "123",
    "TRUE",
    "\"données\"",
    "\"a*b?c\"",
    "\"o'neil-smith\"",
];

/// the shared data the cases read
pub fn data_sheet(book: &mut Workbook) {
    let mut sheet = Sheet::new("Data");
    let numbers = [3.0, 1.5, -2.0, 0.0, 7.25, 10.0, 4.0, 4.0, 100.0, 0.1];
    let words = [
        "apple", "Banana", "cherry", "apple", "date", "", "10", "Fig", "grape", "Apple",
    ];
    for (row, n) in numbers.iter().enumerate() {
        sheet
            .cells
            .set(row as u32, 0, Cell::value(Value::Number(*n)));
        let word = words[row];
        if !word.is_empty() {
            sheet
                .cells
                .set(row as u32, 1, Cell::value(Value::text(word)));
        }
    }
    let mixed = [
        Value::Number(1.0),
        Value::text("2"),
        Value::Bool(true),
        Value::Blank,
        Value::text("x"),
        Value::Number(3.5),
        Value::Bool(false),
        Value::Blank,
        Value::Number(0.0),
        Value::text("  spaced  "),
    ];
    for (row, value) in mixed.iter().enumerate() {
        if !value.is_blank() {
            sheet.cells.set(row as u32, 2, Cell::value(value.clone()));
        }
    }
    // C8 holds an error made by a formula
    sheet.cells.set(7, 2, Cell::formula("1/0"));
    let dates = [
        45352.0, 45351.0, 36526.0, 60.0, 61.0, 1.0, 45657.0, 44927.5, 2958465.0, 45292.0,
    ];
    for (row, serial) in dates.iter().enumerate() {
        sheet
            .cells
            .set(row as u32, 3, Cell::value(Value::Number(*serial)));
    }
    let keys = [
        (10.0, "ten"),
        (20.0, "twenty"),
        (30.0, "thirty"),
        (40.0, "forty"),
        (50.0, "fifty"),
    ];
    for (row, (key, name)) in keys.iter().enumerate() {
        sheet
            .cells
            .set(row as u32, 4, Cell::value(Value::Number(*key)));
        sheet
            .cells
            .set(row as u32, 5, Cell::value(Value::text(*name)));
        // the same table across rows 12 and 13 for HLOOKUP
        sheet
            .cells
            .set(11, row as u32, Cell::value(Value::Number(*key)));
        sheet
            .cells
            .set(12, row as u32, Cell::value(Value::text(*name)));
    }
    let flows = [-1000.0, 300.0, 400.0, 500.0, 200.0];
    for (row, flow) in flows.iter().enumerate() {
        sheet
            .cells
            .set(row as u32, 6, Cell::value(Value::Number(*flow)));
    }
    // descending keys for MATCH(..., -1)
    for (row, key) in [50.0, 40.0, 30.0, 20.0, 10.0].iter().enumerate() {
        sheet
            .cells
            .set(row as u32, 8, Cell::value(Value::Number(*key)));
    }
    book.sheets.push(sheet);
}

pub fn all_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    operators(&mut cases);
    math(&mut cases);
    statistics(&mut cases);
    conditional(&mut cases);
    logical(&mut cases);
    lookup(&mut cases);
    text(&mut cases);
    dates(&mut cases);
    financial(&mut cases);
    information(&mut cases);
    arrays(&mut cases);
    cases
}

fn operators(cases: &mut Vec<Case>) {
    for op in ["+", "-", "*", "/", "^", "&", "=", "<", ">="] {
        for a in ANYTHING {
            for b in ANYTHING.iter().step_by(2) {
                cases.push(case("operators", format!("{a}{op}{b}")));
            }
        }
    }
    for text in [
        "-2^2",
        "2^3^2",
        "-(2^2)",
        "50%*2",
        "1-0.9-0.1",
        "0.5-0.4-0.1",
        "1*(0.5-0.4-0.1)",
        "0.1+0.2=0.3",
        "1/3*3=1",
        "\"a\"<\"B\"",
        "TRUE>1",
        "\"1\"=1",
        "Data!H1=0",
        "Data!H1=\"\"",
        "Data!H1=FALSE",
        "10^15&\"\"",
        "1E-10&\"\"",
        "123456789012345678&\"\"",
        "1/3&\"\"",
        "0.0001&\"\"",
        "-0.000012345&\"\"",
        "1E-15&\"\"",
        "1E-19&\"\"",
        "1E-20&\"\"",
        "1E-21&\"\"",
        "1.5E-12&\"\"",
        "1E+19&\"\"",
        "1E+20&\"\"",
        "1.23456789E+20&\"\"",
        "2^70&\"\"",
        "1/7*1E-8&\"\"",
    ] {
        cases.push(case("operators", text));
    }
}

fn math(cases: &mut Vec<Case>) {
    let unary = [
        "ABS", "SIGN", "SQRT", "EXP", "LN", "LOG10", "INT", "SIN", "COS", "TAN", "ATAN", "ASIN",
        "ACOS", "SINH", "COSH", "TANH", "DEGREES", "RADIANS", "EVEN", "ODD", "FACT", "SQRTPI",
    ];
    for name in unary {
        for n in NUMBERS {
            cases.push(case("math", format!("{name}({n})")));
        }
        cases.push(case("math", format!("{name}(\"2\")")));
        cases.push(case("math", format!("{name}(\"x\")")));
        cases.push(case("math", format!("{name}(Data!H1)")));
    }
    for name in ["ROUND", "ROUNDUP", "ROUNDDOWN", "TRUNC"] {
        for n in NUMBERS {
            for digits in ["-2", "-1", "0", "1", "2", "3"] {
                cases.push(case("math", format!("{name}({n},{digits})")));
            }
        }
    }
    let pairs = [
        "7,2", "-7,2", "7,-2", "-7,-2", "5.5,0.5", "10,3", "1,0", "0,5", "2.5,1", "-2.5,2",
        "100,7", "6,4",
    ];
    for name in [
        "MOD",
        "POWER",
        "MROUND",
        "CEILING",
        "FLOOR",
        "QUOTIENT",
        "LOG",
        "ATAN2",
        "COMBIN",
        "PERMUT",
        "CEILING.MATH",
        "FLOOR.MATH",
    ] {
        for pair in pairs {
            cases.push(case("math", format!("{name}({pair})")));
        }
    }
    for args in [
        "Data!A1:A10",
        "Data!C1:C10",
        "Data!A1:A10,5",
        "1,\"2\",TRUE",
        "Data!B1:B10",
        "Data!A1:C10",
        "Data!A1:A10,\"x\"",
    ] {
        for name in ["SUM", "PRODUCT", "SUMSQ"] {
            cases.push(case("math", format!("{name}({args})")));
        }
    }
    cases.push(case("math", "SUMPRODUCT(Data!A1:A5,Data!E1:E5)"));
    cases.push(case("math", "SUMPRODUCT(Data!A1:A5,Data!B1:B5)"));
    cases.push(case("math", "SUMPRODUCT((Data!A1:A10>2)*Data!A1:A10)"));
    cases.push(case("math", "GCD(24,36,60)"));
    cases.push(case("math", "LCM(4,6,10)"));
    cases.push(case("math", "PI()"));
    cases.push(case("math", "RAND()<1"));
    cases.push(case("math", "RANDBETWEEN(5,5)"));
}

fn statistics(cases: &mut Vec<Case>) {
    let ranges = [
        "Data!A1:A10",
        "Data!C1:C10",
        "Data!A1:A10,Data!E1:E5",
        "1,2,\"3\",TRUE",
        "Data!B1:B10",
        "Data!G1:G5",
    ];
    let names = [
        "AVERAGE",
        "COUNT",
        "COUNTA",
        "MIN",
        "MAX",
        "MEDIAN",
        "MODE.SNGL",
        "MODE",
        "STDEV.S",
        "STDEV.P",
        "STDEV",
        "STDEVP",
        "VAR.S",
        "VAR.P",
        "VAR",
        "VARP",
        "AVEDEV",
        "DEVSQ",
        "AVERAGEA",
        "MINA",
        "MAXA",
        "GEOMEAN",
    ];
    for name in names {
        for range in ranges {
            cases.push(case("statistics", format!("{name}({range})")));
        }
    }
    for range in ["Data!A1:A10", "Data!C1:C10", "Data!B1:C10", "Data!H1:H10"] {
        cases.push(case("statistics", format!("COUNTBLANK({range})")));
    }
    for k in ["1", "2", "3", "10", "11", "0"] {
        cases.push(case("statistics", format!("LARGE(Data!A1:A10,{k})")));
        cases.push(case("statistics", format!("SMALL(Data!A1:A10,{k})")));
    }
    for k in ["0", "0.1", "0.25", "0.5", "0.9", "1", "1.5"] {
        for name in ["PERCENTILE.INC", "PERCENTILE.EXC", "PERCENTILE"] {
            cases.push(case("statistics", format!("{name}(Data!A1:A10,{k})")));
        }
    }
    for q in ["0", "1", "2", "3", "4", "5"] {
        for name in ["QUARTILE.INC", "QUARTILE.EXC", "QUARTILE"] {
            cases.push(case("statistics", format!("{name}(Data!A1:A10,{q})")));
        }
    }
    for value in ["4", "10", "-2", "0.1", "99"] {
        for name in ["RANK.EQ", "RANK.AVG", "RANK"] {
            cases.push(case("statistics", format!("{name}({value},Data!A1:A10)")));
            cases.push(case("statistics", format!("{name}({value},Data!A1:A10,1)")));
        }
    }
    cases.push(case("statistics", "CORREL(Data!A1:A5,Data!E1:E5)"));
    cases.push(case("statistics", "CORREL(Data!A1:A10,Data!D1:D10)"));
    cases.push(case("statistics", "PEARSON(Data!A1:A5,Data!G1:G5)"));
}

fn conditional(cases: &mut Vec<Case>) {
    let criteria = [
        "\"apple\"",
        "\"a*\"",
        "\"?pple\"",
        "\"<>apple\"",
        "\"\"",
        "\">=c\"",
        "\"=\"",
        "\"<>\"",
        "\"10\"",
        "10",
        "\">5\"",
        "\"<0\"",
        "\"*\"",
        "TRUE",
        "\">=4\"",
        "\"=4\"",
        "\"~*\"",
        "Data!B1",
    ];
    for criterion in criteria {
        for range in ["Data!B1:B10", "Data!A1:A10", "Data!C1:C10"] {
            cases.push(case("conditional", format!("COUNTIF({range},{criterion})")));
        }
        cases.push(case(
            "conditional",
            format!("SUMIF(Data!B1:B10,{criterion},Data!A1:A10)"),
        ));
        cases.push(case(
            "conditional",
            format!("AVERAGEIF(Data!A1:A10,{criterion})"),
        ));
        cases.push(case(
            "conditional",
            format!("SUMIFS(Data!A1:A10,Data!B1:B10,{criterion},Data!A1:A10,\">0\")"),
        ));
        cases.push(case(
            "conditional",
            format!("COUNTIFS(Data!B1:B10,{criterion},Data!A1:A10,\"<=4\")"),
        ));
        cases.push(case(
            "conditional",
            format!("AVERAGEIFS(Data!A1:A10,Data!B1:B10,{criterion})"),
        ));
        cases.push(case(
            "conditional",
            format!("MAXIFS(Data!A1:A10,Data!B1:B10,{criterion})"),
        ));
        cases.push(case(
            "conditional",
            format!("MINIFS(Data!A1:A10,Data!B1:B10,{criterion})"),
        ));
    }
    cases.push(case("conditional", "SUMIF(Data!A1:A10,\">2\")"));
    cases.push(case("conditional", "SUMIF(Data!B1:B10,\"apple\",Data!A1)"));
    cases.push(case(
        "conditional",
        "COUNTIF(Data!D1:D10,\">\"&DATE(2024,1,1))",
    ));
}

fn logical(cases: &mut Vec<Case>) {
    for a in ANYTHING {
        cases.push(case("logical", format!("IF({a},\"yes\",\"no\")")));
        cases.push(case("logical", format!("NOT({a})")));
        cases.push(case("logical", format!("AND({a},TRUE)")));
        cases.push(case("logical", format!("OR({a},FALSE)")));
        cases.push(case("logical", format!("XOR({a},TRUE)")));
        cases.push(case("logical", format!("IFERROR({a},\"oops\")")));
        cases.push(case("logical", format!("IFNA({a},\"none\")")));
    }
    for text in [
        "IF(TRUE,,1)",
        "IF(FALSE,1)",
        "IF(1,2)",
        "IFS(1>2,\"a\",2>1,\"b\")",
        "IFS(FALSE,1)",
        "AND(Data!A1:A10)",
        "OR(Data!B1:B10)",
        "AND(Data!H1:H5)",
        "SWITCH(2,1,\"one\",2,\"two\",\"other\")",
        "SWITCH(9,1,\"one\",\"other\")",
        "SWITCH(\"b\",\"a\",1,\"B\",2)",
        "SWITCH(3,1,\"one\")",
        "IF(Data!C8,1,2)",
        "IFERROR(1/0,)",
        "TRUE()",
        "FALSE()",
    ] {
        cases.push(case("logical", text));
    }
}

fn lookup(cases: &mut Vec<Case>) {
    for key in ["5", "10", "25", "50", "60", "\"x\"", "\"20\"", "30"] {
        cases.push(case("lookup", format!("VLOOKUP({key},Data!E1:F5,2)")));
        cases.push(case("lookup", format!("VLOOKUP({key},Data!E1:F5,2,FALSE)")));
        cases.push(case(
            "lookup",
            format!("HLOOKUP({key},Data!A12:E13,2,TRUE)"),
        ));
        cases.push(case("lookup", format!("MATCH({key},Data!E1:E5)")));
        cases.push(case("lookup", format!("MATCH({key},Data!E1:E5,0)")));
        cases.push(case("lookup", format!("MATCH({key},Data!I1:I5,-1)")));
        cases.push(case(
            "lookup",
            format!("LOOKUP({key},Data!E1:E5,Data!F1:F5)"),
        ));
        for mode in ["0", "-1", "1"] {
            cases.push(case(
                "lookup",
                format!("XLOOKUP({key},Data!E1:E5,Data!F1:F5,\"none\",{mode})"),
            ));
            cases.push(case("lookup", format!("XMATCH({key},Data!E1:E5,{mode})")));
        }
    }
    for text in [
        "VLOOKUP(\"b*\",Data!B1:C10,2,FALSE)",
        "VLOOKUP(\"APPLE\",Data!B1:C10,2,FALSE)",
        "VLOOKUP(10,Data!E1:F5,3)",
        "VLOOKUP(10,Data!E1:F5,0)",
        "MATCH(\"?ig\",Data!B1:B10,0)",
        "XLOOKUP(\"a*\",Data!B1:B10,Data!A1:A10,,2)",
        "XLOOKUP(\"apple\",Data!B1:B10,Data!A1:A10,,0,-1)",
        "XLOOKUP(35,Data!E1:E5,Data!F1:F5,,1,2)",
        "INDEX(Data!A1:C10,3,2)",
        "INDEX(Data!A1:A10,4)",
        "INDEX(Data!A12:E12,3)",
        "INDEX(Data!A1:C10,11,1)",
        "SUM(INDEX(Data!A1:C10,0,1))",
        "SUM(Data!A1:INDEX(Data!A1:A10,3))",
        "CHOOSE(2,\"a\",\"b\",\"c\")",
        "CHOOSE(4,1,2,3)",
        "SUM(OFFSET(Data!A1,1,0,3,1))",
        "OFFSET(Data!A1,2,1)",
        "OFFSET(Data!A1,-1,0)",
        "INDIRECT(\"Data!A5\")",
        "SUM(INDIRECT(\"Data!A1:A3\"))",
        "INDIRECT(\"R2C1\",FALSE)",
        "INDIRECT(\"nonsense\")",
        "ROW(Data!C7)",
        "COLUMN(Data!C7)",
        "ROWS(Data!A1:C10)",
        "COLUMNS(Data!A1:C10)",
        "ROWS({1,2;3,4})",
        "SUM(TRANSPOSE(Data!A1:A3))",
        "INDEX(TRANSPOSE(Data!A1:A3),1,2)",
    ] {
        cases.push(case("lookup", text));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn there_are_thousands_of_cases() {
        assert!(super::all_cases().len() > 3000);
    }
}
