//! turns an `Expr` back into formula text: excel syntax for the formula
//! bar and xlsx files, openformula for ods files, r1c1 on request.
//!
//! parentheses the person typed are kept as `Paren` nodes; trees built in
//! code get the parentheses precedence needs.

use crate::address::CellRef;

use super::ast::*;
use super::parser::Syntax;
pub use super::reference_text::{area_text, quote_sheet, sheet_text, structured_text};
use super::reference_text::{open_reference, r1c1_area};

/// excel syntax, as the formula bar shows it.
pub fn to_string(expr: &Expr) -> String {
    Printer::new(Syntax::Excel).print(expr)
}

/// excel syntax as stored in xlsx files: newer functions get the
/// "_xlfn." prefix excel expects, so older excel shows #NAME? instead of
/// misreading them.
pub fn to_file_formula(expr: &Expr) -> String {
    let mut printer = Printer::new(Syntax::Excel);
    printer.file_prefixes = true;
    printer.print(expr)
}

/// openformula as ods stores it, with the "of:=" prefix.
pub fn to_open_formula(expr: &Expr) -> String {
    format!("of:={}", Printer::new(Syntax::OpenFormula).print(expr))
}

/// r1c1 references relative to `at`.
pub fn to_r1c1(expr: &Expr, at: CellRef) -> String {
    let mut printer = Printer::new(Syntax::Excel);
    printer.r1c1 = Some(at);
    printer.print(expr)
}

struct Printer {
    syntax: Syntax,
    file_prefixes: bool,
    r1c1: Option<CellRef>,
}

impl Printer {
    fn new(syntax: Syntax) -> Printer {
        Printer {
            syntax,
            file_prefixes: false,
            r1c1: None,
        }
    }

    fn print(&self, expr: &Expr) -> String {
        let mut out = String::new();
        self.write(expr, &mut out);
        out
    }

    fn write(&self, expr: &Expr, out: &mut String) {
        match expr {
            Expr::Number { value } => out.push_str(&number_text(*value)),
            Expr::Text { value } => {
                out.push('"');
                out.push_str(&value.replace('"', "\"\""));
                out.push('"');
            }
            Expr::Bool { value } => out.push_str(match (self.syntax, value) {
                (Syntax::OpenFormula, true) => "TRUE()",
                (Syntax::OpenFormula, false) => "FALSE()",
                (_, true) => "TRUE",
                (_, false) => "FALSE",
            }),
            Expr::Error { value } => out.push_str(value.as_str()),
            Expr::Ref { reference } => self.reference(reference, out),
            Expr::Name { name } => out.push_str(name),
            Expr::Structured { reference } => out.push_str(&structured_text(reference)),
            Expr::Function { name, args } => {
                out.push_str(&self.function_name(name));
                out.push('(');
                let separator = if self.syntax == Syntax::OpenFormula {
                    ';'
                } else {
                    ','
                };
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        out.push(separator);
                    }
                    self.write(arg, out);
                }
                out.push(')');
            }
            Expr::Unary { op, expr: inner } => self.unary(*op, inner, out),
            Expr::Binary { op, left, right } => self.binary(*op, left, right, out),
            Expr::Array { rows } => {
                let (col_sep, row_sep) = match self.syntax {
                    Syntax::Excel => (',', ';'),
                    Syntax::OpenFormula => (';', '|'),
                };
                out.push('{');
                for (r, row) in rows.iter().enumerate() {
                    if r > 0 {
                        out.push(row_sep);
                    }
                    for (c, item) in row.iter().enumerate() {
                        if c > 0 {
                            out.push(col_sep);
                        }
                        self.write(item, out);
                    }
                }
                out.push('}');
            }
            Expr::Paren { expr: inner }
                if matches!(
                    **inner,
                    Expr::Binary {
                        op: BinaryOp::Union,
                        ..
                    }
                ) =>
            {
                self.write(inner, out);
            }
            Expr::Paren { expr: inner } => {
                out.push('(');
                self.write(inner, out);
                out.push(')');
            }
            Expr::Missing => {}
        }
    }

    fn function_name(&self, name: &str) -> String {
        match self.syntax {
            Syntax::Excel if self.file_prefixes => match file_prefix(name) {
                Some(prefix) => format!("{prefix}{name}"),
                None => name.to_string(),
            },
            Syntax::OpenFormula if file_prefix(name).is_some() => format!("COM.MICROSOFT.{name}"),
            _ => name.to_string(),
        }
    }

    fn unary(&self, op: UnaryOp, inner: &Expr, out: &mut String) {
        let wrap = matches!(inner, Expr::Binary { op, .. } if op.precedence() < 8);
        let mut operand = String::new();
        if wrap {
            operand.push('(');
        }
        self.write(inner, &mut operand);
        if wrap {
            operand.push(')');
        }
        match op {
            UnaryOp::Minus => {
                out.push('-');
                out.push_str(&operand);
            }
            UnaryOp::Plus => {
                out.push('+');
                out.push_str(&operand);
            }
            UnaryOp::At => {
                out.push('@');
                out.push_str(&operand);
            }
            UnaryOp::Percent => {
                out.push_str(&operand);
                out.push('%');
            }
            UnaryOp::Spill => {
                out.push_str(&operand);
                out.push('#');
            }
        }
    }

    fn binary(&self, op: BinaryOp, left: &Expr, right: &Expr, out: &mut String) {
        let precedence = op.precedence();
        let needs = |child: &Expr, right_side: bool| match child {
            Expr::Binary { op: inner, .. } => {
                inner.precedence() < precedence
                    || (right_side && inner.precedence() == precedence && precedence < 8)
            }
            _ => false,
        };
        let wrap_union = op == BinaryOp::Union;
        if wrap_union {
            out.push('(');
        }
        self.child(left, needs(left, false), out);
        out.push_str(match (self.syntax, op) {
            (Syntax::OpenFormula, BinaryOp::Intersect) => "!",
            (Syntax::OpenFormula, BinaryOp::Union) => "~",
            _ => op.symbol(),
        });
        self.child(right, needs(right, true), out);
        if wrap_union {
            out.push(')');
        }
    }

    fn child(&self, expr: &Expr, wrap: bool, out: &mut String) {
        if wrap {
            out.push('(');
        }
        // a union written inside a paren already has its own
        if let (Expr::Paren { expr: inner }, false) = (expr, wrap) {
            if matches!(
                **inner,
                Expr::Binary {
                    op: BinaryOp::Union,
                    ..
                }
            ) {
                self.write(inner, out);
                return;
            }
        }
        self.write(expr, out);
        if wrap {
            out.push(')');
        }
    }

    fn reference(&self, reference: &Reference, out: &mut String) {
        match self.syntax {
            Syntax::OpenFormula => out.push_str(&open_reference(reference)),
            Syntax::Excel => {
                if let Some(sheet) = &reference.sheet {
                    out.push_str(&sheet_text(sheet));
                    out.push('!');
                }
                match self.r1c1 {
                    Some(at) => out.push_str(&r1c1_area(&reference.area, at)),
                    None => out.push_str(&area_text(&reference.area)),
                }
            }
        }
    }
}

/// the prefix excel files need on functions added since excel 2007.
pub fn file_prefix(name: &str) -> Option<&'static str> {
    const XLWS: [&str; 3] = ["SORT", "FILTER", "SORTBY"];
    const XLFN: &[&str] = &[
        "XLOOKUP",
        "XMATCH",
        "UNIQUE",
        "SEQUENCE",
        "RANDARRAY",
        "LET",
        "LAMBDA",
        "IFS",
        "SWITCH",
        "MAXIFS",
        "MINIFS",
        "CONCAT",
        "TEXTJOIN",
        "XOR",
        "IFNA",
        "DAYS",
        "STDEV.S",
        "STDEV.P",
        "VAR.S",
        "VAR.P",
        "MODE.SNGL",
        "MODE.MULT",
        "PERCENTILE.INC",
        "PERCENTILE.EXC",
        "QUARTILE.INC",
        "QUARTILE.EXC",
        "RANK.EQ",
        "RANK.AVG",
        "CEILING.MATH",
        "FLOOR.MATH",
        "CEILING.PRECISE",
        "FLOOR.PRECISE",
        "ISOWEEKNUM",
        "NORM.DIST",
        "NORM.S.DIST",
        "NORM.INV",
        "NORM.S.INV",
        "T.TEST",
        "T.DIST",
        "CHISQ.TEST",
        "COVARIANCE.P",
        "COVARIANCE.S",
        "NUMBERVALUE",
        "SHEET",
        "SHEETS",
        "FORMULATEXT",
        "ISFORMULA",
        "UNICHAR",
        "UNICODE",
        "TEXTBEFORE",
        "TEXTAFTER",
        "TEXTSPLIT",
        "VSTACK",
        "HSTACK",
        "TAKE",
        "DROP",
        "CHOOSECOLS",
        "CHOOSEROWS",
        "TOCOL",
        "TOROW",
    ];
    if XLWS.contains(&name) {
        Some("_xlfn._xlws.")
    } else if XLFN.contains(&name) || name == "AGGREGATE" {
        Some("_xlfn.")
    } else {
        None
    }
}

/// numbers as excel writes them in formulas: shortest exact form, an
/// exponent only for very large or small values.
pub fn number_text(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let magnitude = value.abs();
    if (1e-9..1e15).contains(&magnitude) {
        let text = format!("{value}");
        text
    } else {
        let text = format!("{value:E}");
        // rust writes "1E21"; excel writes "1E+21"
        match text.split_once('E') {
            Some((mantissa, exponent)) if !exponent.starts_with('-') => {
                format!("{mantissa}E+{exponent}")
            }
            _ => text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::parse;

    #[test]
    fn numbers_print_like_excel() {
        assert_eq!(number_text(0.1), "0.1");
        assert_eq!(number_text(1e21), "1E+21");
        assert_eq!(number_text(1.5e-12), "1.5E-12");
        assert_eq!(number_text(-42.0), "-42");
    }

    #[test]
    fn sheet_names_are_quoted_when_needed() {
        assert_eq!(quote_sheet("Data"), "Data");
        assert_eq!(quote_sheet("My data"), "'My data'");
        assert_eq!(quote_sheet("A1"), "'A1'");
        assert_eq!(quote_sheet("Bob's"), "'Bob''s'");
        assert_eq!(quote_sheet("2024"), "'2024'");
    }

    #[test]
    fn file_formulas_get_prefixes() {
        let expr = parse("XLOOKUP(1,A:A,B:B)+SORT(C1:C3)+SUM(1)").unwrap();
        assert_eq!(
            to_file_formula(&expr),
            "_xlfn.XLOOKUP(1,A:A,B:B)+_xlfn._xlws.SORT(C1:C3)+SUM(1)"
        );
    }

    #[test]
    fn open_formula_output() {
        let expr = parse("SUM(A1:B2,Sheet2!C3,'My s'!D1)+TRUE").unwrap();
        assert_eq!(
            to_open_formula(&expr),
            "of:=SUM([.A1:.B2];[$Sheet2.C3];[$'My s'.D1])+TRUE()"
        );
    }

    #[test]
    fn built_trees_get_needed_parentheses() {
        let sum = Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(Expr::number(1.0)),
            right: Box::new(Expr::number(2.0)),
        };
        let product = Expr::Binary {
            op: BinaryOp::Multiply,
            left: Box::new(sum.clone()),
            right: Box::new(Expr::number(3.0)),
        };
        assert_eq!(to_string(&product), "(1+2)*3");
        let difference = Expr::Binary {
            op: BinaryOp::Subtract,
            left: Box::new(Expr::number(5.0)),
            right: Box::new(Expr::Binary {
                op: BinaryOp::Subtract,
                left: Box::new(Expr::number(2.0)),
                right: Box::new(Expr::number(1.0)),
            }),
        };
        assert_eq!(to_string(&difference), "5-(2-1)");
    }
}
