//! the operator levels, loosest first, in excel's order: comparisons,
//! "&", "+ -", "* /", "^", "%", unary minus, then the reference operators
//! intersection (" ") and range (":"). so -2^2 is 4, as in excel.

use super::super::ast::{BinaryOp, Expr, UnaryOp};
use super::{is_name_start, ParseError, Parser, Syntax};

fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

impl Parser {
    pub(super) fn comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.concat()?;
        loop {
            self.skip_spaces();
            // two-character operators are tried first so "<=" is not "<"
            let op = if self.eat("<>") {
                BinaryOp::NotEqual
            } else if self.eat("<=") {
                BinaryOp::LessEqual
            } else if self.eat(">=") {
                BinaryOp::GreaterEqual
            } else if self.eat("<") {
                BinaryOp::Less
            } else if self.eat(">") {
                BinaryOp::Greater
            } else if self.eat("=") {
                BinaryOp::Equal
            } else {
                return Ok(left);
            };
            let right = self.concat()?;
            left = binary(op, left, right);
        }
    }

    fn concat(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.additive()?;
        loop {
            self.skip_spaces();
            if !self.eat("&") {
                return Ok(left);
            }
            let right = self.additive()?;
            left = binary(BinaryOp::Concat, left, right);
        }
    }

    fn additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.multiplicative()?;
        loop {
            self.skip_spaces();
            let op = match self.peek() {
                Some('+') => BinaryOp::Add,
                Some('-') => BinaryOp::Subtract,
                _ => return Ok(left),
            };
            self.pos += 1;
            let right = self.multiplicative()?;
            left = binary(op, left, right);
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.power()?;
        loop {
            self.skip_spaces();
            let op = match self.peek() {
                Some('*') => BinaryOp::Multiply,
                Some('/') => BinaryOp::Divide,
                _ => return Ok(left),
            };
            self.pos += 1;
            let right = self.power()?;
            left = binary(op, left, right);
        }
    }

    /// "^" is left associative in excel: 2^3^2 is 64
    fn power(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.percent()?;
        loop {
            self.skip_spaces();
            if !self.eat("^") {
                return Ok(left);
            }
            let right = self.percent()?;
            left = binary(BinaryOp::Power, left, right);
        }
    }

    fn percent(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.unary()?;
        loop {
            let save = self.pos;
            self.skip_spaces();
            if !self.eat("%") {
                self.pos = save;
                return Ok(expr);
            }
            expr = Expr::Unary {
                op: UnaryOp::Percent,
                expr: Box::new(expr),
            };
        }
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        self.skip_spaces();
        let op = match self.peek() {
            Some('-') => UnaryOp::Minus,
            Some('+') => UnaryOp::Plus,
            _ => return self.intersection(),
        };
        self.pos += 1;
        self.nest()?;
        let inner = self.unary()?;
        self.depth -= 1;
        Ok(Expr::Unary {
            op,
            expr: Box::new(inner),
        })
    }

    /// "A1:B5 B2:C3" in excel, "A1:B5!B2:C3" in openformula
    fn intersection(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.range()?;
        loop {
            let save = self.pos;
            let spaced = self.skip_spaces();
            let found = match self.syntax {
                Syntax::Excel => spaced && self.starts_operand(),
                Syntax::OpenFormula => self.eat("!"),
            };
            if !found {
                self.pos = save;
                return Ok(left);
            }
            let right = self.range()?;
            left = binary(BinaryOp::Intersect, left, right);
        }
    }

    /// whether the text at the cursor can begin a reference, so a space
    /// before it is intersection rather than layout
    fn starts_operand(&self) -> bool {
        match self.peek() {
            Some(c) if is_name_start(c) || c == '$' || c == '\'' || c == '(' => true,
            Some(c) if c.is_ascii_digit() => {
                // only "3:5" (whole rows) starts a reference with a digit
                let digits = self.chars[self.pos..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit() || **c == '$')
                    .count();
                self.peek_at(digits) == Some(':')
            }
            _ => false,
        }
    }

    fn range(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.postfix()?;
        loop {
            let save = self.pos;
            self.skip_spaces();
            if !self.eat(":") {
                self.pos = save;
                return Ok(left);
            }
            self.skip_spaces();
            let right = self.postfix()?;
            left = join_range(left, right);
        }
    }

    /// "A1#": the whole spill range of a cell
    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let expr = self.primary()?;
        let can_spill = matches!(expr, Expr::Ref { .. } | Expr::Name { .. });
        if can_spill && self.peek() == Some('#') {
            self.pos += 1;
            return Ok(Expr::Unary {
                op: UnaryOp::Spill,
                expr: Box::new(expr),
            });
        }
        Ok(expr)
    }

    /// "(1+2)", and excel's union "(A1:A3,C1:C3)" which only exists
    /// inside parentheses (outside them the comma separates arguments)
    pub(super) fn parenthesised(&mut self) -> Result<Expr, ParseError> {
        self.pos += 1;
        self.nest()?;
        let mut expr = self.comparison()?;
        loop {
            self.skip_spaces();
            let union = match self.syntax {
                Syntax::Excel => self.eat(","),
                Syntax::OpenFormula => self.eat("~"),
            };
            if !union {
                break;
            }
            let right = self.comparison()?;
            expr = binary(BinaryOp::Union, expr, right);
        }
        self.skip_spaces();
        if !self.eat(")") {
            return Err(self.error("a \")\" is missing"));
        }
        self.depth -= 1;
        Ok(Expr::Paren {
            expr: Box::new(expr),
        })
    }
}

/// two plain references joined by ":" become one area when they are on
/// the same sheet, so "Sheet1!A1:B2" is one reference, not two
pub(super) fn join_range(left: Expr, right: Expr) -> Expr {
    use super::super::ast::Area;
    if let (Expr::Ref { reference: a }, Expr::Ref { reference: b }) = (&left, &right) {
        let same_sheet = b.sheet.is_none() || b.sheet == a.sheet;
        if let (true, Area::Cell { at: start }, Area::Cell { at: end }) =
            (same_sheet, a.area, b.area)
        {
            return Expr::reference(a.sheet.clone(), Area::Range { start, end });
        }
    }
    binary(BinaryOp::Range, left, right)
}
