//! the tree a formula is parsed into.

use serde::{Deserialize, Serialize};

use crate::address::{MAX_COLS, MAX_ROWS};
use crate::model::ErrorValue;

/// one corner of a reference, zero based, with its dollar signs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellPos {
    pub row: u32,
    pub col: u32,
    pub row_abs: bool,
    pub col_abs: bool,
}

impl CellPos {
    pub fn relative(row: u32, col: u32) -> CellPos {
        CellPos {
            row,
            col,
            row_abs: false,
            col_abs: false,
        }
    }
}

/// the cells a reference names, before a sheet is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Area {
    Cell {
        at: CellPos,
    },
    Range {
        start: CellPos,
        end: CellPos,
    },
    /// whole columns "A:C"
    Columns {
        first: u32,
        last: u32,
        first_abs: bool,
        last_abs: bool,
    },
    /// whole rows "3:5"
    Rows {
        first: u32,
        last: u32,
        first_abs: bool,
        last_abs: bool,
    },
}

impl Area {
    /// the rectangle as (first row, first col, last row, last col), both
    /// ends included and in order.
    pub fn bounds(&self) -> (u32, u32, u32, u32) {
        match *self {
            Area::Cell { at } => (at.row, at.col, at.row, at.col),
            Area::Range { start, end } => (
                start.row.min(end.row),
                start.col.min(end.col),
                start.row.max(end.row),
                start.col.max(end.col),
            ),
            Area::Columns { first, last, .. } => {
                (0, first.min(last), MAX_ROWS - 1, first.max(last))
            }
            Area::Rows { first, last, .. } => (first.min(last), 0, first.max(last), MAX_COLS - 1),
        }
    }

    pub fn to_range(&self) -> crate::address::Range {
        let (r1, c1, r2, c2) = self.bounds();
        crate::address::Range::from_bounds(r1, c1, r2, c2)
    }
}

/// which sheet a reference is on.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SheetRef {
    /// "Sheet2!A1"
    One { name: String },
    /// a 3-d reference "Sheet1:Sheet3!A1"
    Span { first: String, last: String },
    /// a reference into another file ("[1]Sheet1!A1"), kept as written
    External { workbook: String, name: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    /// none means the formula's own sheet
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<SheetRef>,
    pub area: Area,
}

/// the special parts of a structured reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableItem {
    All,
    Data,
    Headers,
    Totals,
    ThisRow,
}

/// "Table1[Price]", "Table1[[#Headers],[Price]:[Tax]]", "[@Price]".
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredRef {
    /// none inside the table itself ("[@Price]")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<TableItem>,
    /// a column, or a run of columns
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_column: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnaryOp {
    Minus,
    Plus,
    /// postfix "%"
    Percent,
    /// "@": implicit intersection
    At,
    /// postfix "#": the whole spill range of a cell
    Spill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Concat,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    /// ":" between two references
    Range,
    /// " " between two references: the cells both have
    Intersect,
    /// "," inside parentheses: both areas together
    Union,
}

impl BinaryOp {
    pub fn symbol(&self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Power => "^",
            BinaryOp::Concat => "&",
            BinaryOp::Equal => "=",
            BinaryOp::NotEqual => "<>",
            BinaryOp::Less => "<",
            BinaryOp::LessEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterEqual => ">=",
            BinaryOp::Range => ":",
            BinaryOp::Intersect => " ",
            BinaryOp::Union => ",",
        }
    }

    /// binding strength; higher binds tighter (excel's order).
    pub fn precedence(&self) -> u8 {
        match self {
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => 1,
            BinaryOp::Concat => 2,
            BinaryOp::Add | BinaryOp::Subtract => 3,
            BinaryOp::Multiply | BinaryOp::Divide => 4,
            BinaryOp::Power => 5,
            BinaryOp::Union => 8,
            BinaryOp::Intersect => 9,
            BinaryOp::Range => 10,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Expr {
    Number {
        value: f64,
    },
    Text {
        value: String,
    },
    Bool {
        value: bool,
    },
    Error {
        value: ErrorValue,
    },
    /// boxed, like the table reference below, so the tree's nodes stay
    /// small: a sheet holds hundreds of thousands of parsed formulas
    Ref {
        reference: Box<Reference>,
    },
    /// a defined name (or a table name on its own)
    Name {
        name: String,
    },
    Structured {
        reference: Box<StructuredRef>,
    },
    /// `name` is upper case and without "_xlfn." style prefixes
    Function {
        name: String,
        args: Vec<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// an array constant {1,2;3,4}: rows of items
    Array {
        rows: Vec<Vec<Expr>>,
    },
    /// parentheses the person wrote, kept so the formula prints back the same
    Paren {
        expr: Box<Expr>,
    },
    /// an argument left out: "IF(A1,,2)"
    Missing,
}

impl Expr {
    pub fn number(value: f64) -> Expr {
        Expr::Number { value }
    }

    pub fn text(value: &str) -> Expr {
        Expr::Text {
            value: value.to_string(),
        }
    }

    pub fn reference(sheet: Option<SheetRef>, area: Area) -> Expr {
        Expr::Ref {
            reference: Box::new(Reference { sheet, area }),
        }
    }

    pub fn function(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Function {
            name: name.to_string(),
            args,
        }
    }

    /// calls `visit` on this node and every node inside it.
    pub fn walk(&self, visit: &mut dyn FnMut(&Expr)) {
        visit(self);
        match self {
            Expr::Function { args, .. } => args.iter().for_each(|arg| arg.walk(visit)),
            Expr::Unary { expr, .. } | Expr::Paren { expr } => expr.walk(visit),
            Expr::Binary { left, right, .. } => {
                left.walk(visit);
                right.walk(visit);
            }
            Expr::Array { rows } => rows.iter().flatten().for_each(|item| item.walk(visit)),
            _ => {}
        }
    }

    /// changes nodes from the leaves up: `change` sees every node after
    /// its children were changed and may replace it.
    pub fn transform(self, change: &mut dyn FnMut(Expr) -> Expr) -> Expr {
        let rebuilt = match self {
            Expr::Function { name, args } => Expr::Function {
                name,
                args: args.into_iter().map(|arg| arg.transform(change)).collect(),
            },
            Expr::Unary { op, expr } => Expr::Unary {
                op,
                expr: Box::new(expr.transform(change)),
            },
            Expr::Paren { expr } => Expr::Paren {
                expr: Box::new(expr.transform(change)),
            },
            Expr::Binary { op, left, right } => Expr::Binary {
                op,
                left: Box::new(left.transform(change)),
                right: Box::new(right.transform(change)),
            },
            Expr::Array { rows } => Expr::Array {
                rows: rows
                    .into_iter()
                    .map(|row| row.into_iter().map(|item| item.transform(change)).collect())
                    .collect(),
            },
            other => other,
        };
        change(rebuilt)
    }

    /// every function name used, upper case.
    pub fn function_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        self.walk(&mut |node| {
            if let Expr::Function { name, .. } = node {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        });
        names
    }
}
