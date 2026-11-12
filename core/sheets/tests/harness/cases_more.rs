//! the second half of the harness cases: text, dates and times,
//! financial, information and dynamic array functions.

use super::cases::{case, Case, ANYTHING, TEXTS};

pub fn text(cases: &mut Vec<Case>) {
    for t in TEXTS {
        for name in [
            "LEN", "UPPER", "LOWER", "PROPER", "TRIM", "CLEAN", "T", "CODE", "VALUE",
        ] {
            cases.push(case("text", format!("{name}({t})")));
        }
        for n in ["0", "1", "3", "20"] {
            cases.push(case("text", format!("LEFT({t},{n})")));
            cases.push(case("text", format!("RIGHT({t},{n})")));
            cases.push(case("text", format!("MID({t},2,{n})")));
            cases.push(case("text", format!("REPT({t},{n})")));
        }
        cases.push(case("text", format!("FIND(\"o\",{t})")));
        cases.push(case("text", format!("SEARCH(\"O\",{t})")));
        cases.push(case("text", format!("SEARCH(\"?l\",{t})")));
        cases.push(case("text", format!("SUBSTITUTE({t},\"l\",\"L\")")));
        cases.push(case("text", format!("SUBSTITUTE({t},\"l\",\"L\",2)")));
        cases.push(case("text", format!("REPLACE({t},2,3,\"#\")")));
        cases.push(case("text", format!("EXACT({t},\"Hello World\")")));
    }
    for value in [
        "1234.567",
        "-1234.567",
        "0",
        "0.5",
        "45352.75",
        "1.5",
        "\"abc\"",
        "TRUE",
    ] {
        for code in [
            "\"0.00\"",
            "\"#,##0\"",
            "\"0%\"",
            "\"0.0E+00\"",
            "\"$#,##0.00\"",
            "\"yyyy-mm-dd\"",
            "\"d mmm yyyy\"",
            "\"h:mm AM/PM\"",
            "\"[h]:mm\"",
            "\"# ?/?\"",
            "\"General\"",
            "\"@\"",
            "\"000\"",
            "\"#,##0,K\"",
            "\"dddd\"",
        ] {
            cases.push(case("text", format!("TEXT({value},{code})")));
        }
    }
    for text in [
        "CONCAT(\"a\",1,TRUE)",
        "CONCAT(Data!B1:B3)",
        "CONCATENATE(\"x\",Data!A2)",
        "TEXTJOIN(\"-\",TRUE,Data!B1:B10)",
        "TEXTJOIN(\", \",FALSE,Data!B4:B7)",
        "CHAR(65)",
        "CHAR(128)",
        "CHAR(0)",
        "FIXED(1234.567,1)",
        "FIXED(1234.567,-1,TRUE)",
        "DOLLAR(1234.567)",
        "DOLLAR(-5,0)",
        "VALUE(\"$1,234.50\")",
        "VALUE(\"(5)\")",
        "VALUE(\"3/1/2024\")",
        "VALUE(\"12:00\")",
        "LEN(1/3)",
        "\"x\"&1E+21",
        "FIND(\"\",\"abc\")",
        "FIND(\"z\",\"abc\")",
        "MID(\"abc\",0,1)",
    ] {
        cases.push(case("text", text));
    }
}

pub fn dates(cases: &mut Vec<Case>) {
    for parts in [
        "2024,3,1",
        "2024,13,1",
        "2024,0,1",
        "2024,2,30",
        "1900,2,29",
        "1900,3,1",
        "100,1,1",
        "2024,-1,15",
        "9999,12,31",
        "10000,1,1",
    ] {
        cases.push(case("date and time", format!("DATE({parts})")));
    }
    for serial in [
        "Data!D1",
        "Data!D2",
        "Data!D3",
        "Data!D4",
        "Data!D5",
        "Data!D6",
        "Data!D7",
        "Data!D8",
        "Data!D10",
        "0",
        "-1",
        "45352.999999",
    ] {
        for name in ["YEAR", "MONTH", "DAY", "HOUR", "MINUTE", "SECOND"] {
            cases.push(case("date and time", format!("{name}({serial})")));
        }
        for kind in ["1", "2", "3", "11", "16"] {
            cases.push(case("date and time", format!("WEEKDAY({serial},{kind})")));
        }
        for kind in ["1", "2", "21", "15"] {
            cases.push(case("date and time", format!("WEEKNUM({serial},{kind})")));
        }
        for months in ["1", "-1", "12", "0"] {
            cases.push(case("date and time", format!("EDATE({serial},{months})")));
            cases.push(case("date and time", format!("EOMONTH({serial},{months})")));
        }
    }
    for unit in [
        "\"Y\"", "\"M\"", "\"D\"", "\"MD\"", "\"YM\"", "\"YD\"", "\"X\"",
    ] {
        for (a, b) in [
            ("DATE(2020,1,31)", "DATE(2024,3,1)"),
            ("DATE(2024,2,29)", "DATE(2025,2,28)"),
            ("DATE(2024,5,15)", "DATE(2024,5,14)"),
        ] {
            cases.push(case("date and time", format!("DATEDIF({a},{b},{unit})")));
        }
    }
    for text in [
        "NETWORKDAYS(DATE(2024,1,1),DATE(2024,1,31))",
        "NETWORKDAYS(DATE(2024,1,31),DATE(2024,1,1))",
        "NETWORKDAYS(DATE(2024,1,1),DATE(2024,1,31),DATE(2024,1,15))",
        "WORKDAY(DATE(2024,1,5),1)",
        "WORKDAY(DATE(2024,1,5),-5)",
        "WORKDAY(DATE(2024,1,1),10,DATE(2024,1,8))",
        "DAYS(DATE(2024,3,1),DATE(2024,1,1))",
        "DAYS360(DATE(2024,1,31),DATE(2024,3,31))",
        "DAYS360(DATE(2024,2,29),DATE(2024,3,31),TRUE)",
        "YEARFRAC(DATE(2024,1,1),DATE(2024,7,1))",
        "YEARFRAC(DATE(2024,1,1),DATE(2025,7,1),1)",
        "YEARFRAC(DATE(2024,1,1),DATE(2024,7,1),3)",
        "TIME(25,30,0)",
        "TIME(12,-30,0)",
        "TIME(0,0,-1)",
        "DATEVALUE(\"2024-03-01\")",
        "DATEVALUE(\"1-Mar-2024\")",
        "TIMEVALUE(\"6:45 PM\")",
        "DATEVALUE(\"nonsense\")",
        "ISOWEEKNUM(DATE(2021,1,3))",
        "TODAY()=INT(NOW())",
        "DATE(2024,3,1)-DATE(2024,2,1)",
    ] {
        cases.push(case("date and time", text));
    }
}

pub fn financial(cases: &mut Vec<Case>) {
    for text in [
        "PMT(0.08/12,10,10000)",
        "PMT(0.08/12,10,10000,0,1)",
        "PMT(0,12,1200)",
        "PMT(0.05,0,100)",
        "PV(0.08/12,240,500)",
        "PV(0.1,5,-100,1000,1)",
        "FV(0.06/12,10,-200,-500,1)",
        "FV(0,10,-100)",
        "NPER(0.12/12,-100,-1000,10000,1)",
        "NPER(0.01,-100,1000)",
        "NPER(0,-100,1000)",
        "RATE(48,-200,8000)",
        "RATE(10,-100,1000,0,1)",
        "RATE(12,100,1000)",
        "IPMT(0.1/12,1,36,8000)",
        "IPMT(0.1,3,3,8000)",
        "PPMT(0.1/12,1,24,2000)",
        "PPMT(0.08,10,10,200000,0,1)",
        "NPV(0.1,Data!G2:G5)",
        "NPV(0.1,-10000,3000,4200,6800)",
        "IRR(Data!G1:G5)",
        "IRR({-100,110})",
        "IRR({100,110})",
        "IRR(Data!G1:G5,0.5)",
    ] {
        cases.push(case("financial", text));
    }
}

pub fn information(cases: &mut Vec<Case>) {
    for a in ANYTHING {
        for name in [
            "ISNUMBER",
            "ISTEXT",
            "ISNONTEXT",
            "ISBLANK",
            "ISERROR",
            "ISERR",
            "ISNA",
            "ISLOGICAL",
            "TYPE",
            "N",
            "ERROR.TYPE",
        ] {
            cases.push(case("information", format!("{name}({a})")));
        }
    }
    for text in [
        "NA()",
        "ISREF(Data!A1)",
        "ISREF(1)",
        "ISFORMULA(Data!C8)",
        "ISFORMULA(Data!A1)",
        "TYPE({1,2})",
        "ISEVEN(-3.5)",
        "ISODD(\"x\")",
    ] {
        cases.push(case("information", text));
    }
}

pub fn arrays(cases: &mut Vec<Case>) {
    for text in [
        "SUM(SEQUENCE(10))",
        "SUM(SEQUENCE(3,4,10,-2))",
        "INDEX(SEQUENCE(5,1,0.5,0.5),3)",
        "INDEX(SORT(Data!A1:A10),1)",
        "INDEX(SORT(Data!A1:A10),10)",
        "INDEX(SORT(Data!A1:A10,1,-1),2)",
        "INDEX(SORT(Data!B1:B10),1)",
        "INDEX(SORT(Data!B1:B10),10)",
        "INDEX(SORT(Data!E1:F5,2),1,2)",
        "ROWS(UNIQUE(Data!B1:B10))",
        "ROWS(UNIQUE(Data!A1:A10))",
        "ROWS(UNIQUE(Data!A1:A10,FALSE,TRUE))",
        "TEXTJOIN(\",\",TRUE,UNIQUE(Data!B1:B10))",
        "SUM(FILTER(Data!A1:A10,Data!A1:A10>2))",
        "ROWS(FILTER(Data!B1:B10,Data!A1:A10>=4))",
        "FILTER(Data!A1:A10,Data!A1:A10>1000,\"none\")",
        "INDEX(FILTER(Data!F1:F5,Data!E1:E5>25),1)",
        "TEXTJOIN(\",\",,SORT(Data!E1:E5,,-1))",
        "SUM(Data!A1:A10*2)",
        "SUM(IF(Data!A1:A10>3,1,0))",
        "MAX(LEN(Data!B1:B10))",
    ] {
        cases.push(case("dynamic arrays", text));
    }
}
