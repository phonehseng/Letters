//! reading tokens into the syntax tree of `script.rs`, in either of the two
//! languages: the javascript subset form files use, and letters notation.
//! statements (javascript only) are in `statements.rs`.

use std::fmt::Write as _;

use super::lex::{lex, Token};
use super::script::{BinaryOp, Expr, Function, ParseError, Stmt, UnaryOp};

/// how deeply brackets and statements may nest. real form scripts nest a
/// few levels; this keeps a hostile script from exhausting the stack.
const MAX_DEPTH: usize = 48;

/// which of the two languages is being read.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Dialect {
    JavaScript,
    Notation,
}

pub(super) struct Parser {
    tokens: Vec<Token>,
    pub(super) at: usize,
    depth: usize,
    dialect: Dialect,
    /// variables declared with var so far
    pub(super) locals: Vec<String>,
}

/// reads a whole javascript script in the supported subset.
pub fn parse_script(source: &str) -> Result<Vec<Stmt>, ParseError> {
    let mut parser = Parser::new(lex(source)?, Dialect::JavaScript);
    parser.statements_until_end()
}

/// reads one expression in letters notation.
pub fn parse_notation(source: &str) -> Result<Expr, ParseError> {
    let mut parser = Parser::new(lex(source)?, Dialect::Notation);
    if parser.at_end() {
        return Err(ParseError("it is empty".into()));
    }
    let expr = parser.expression()?;
    if !parser.at_end() {
        return Err(ParseError(
            "there is more after the end of the formula".into(),
        ));
    }
    Ok(expr)
}

// moving through the tokens
impl Parser {
    fn new(tokens: Vec<Token>, dialect: Dialect) -> Self {
        Self {
            tokens,
            at: 0,
            depth: 0,
            dialect,
            locals: Vec::new(),
        }
    }

    pub(super) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    pub(super) fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.at + offset)
    }

    pub(super) fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    pub(super) fn is_symbol(&self, symbol: &str) -> bool {
        self.is_symbol_at(0, symbol)
    }

    pub(super) fn is_symbol_at(&self, offset: usize, symbol: &str) -> bool {
        matches!(self.peek_at(offset), Some(Token::Symbol(found)) if *found == symbol)
    }

    pub(super) fn is_word(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(found)) if found == word)
    }

    pub(super) fn eat_symbol(&mut self, symbol: &str) -> bool {
        let found = self.is_symbol(symbol);
        if found {
            self.at += 1;
        }
        found
    }

    fn eat_word_insensitive(&mut self, word: &str) -> bool {
        let found =
            matches!(self.peek(), Some(Token::Word(found)) if found.eq_ignore_ascii_case(word));
        if found {
            self.at += 1;
        }
        found
    }

    pub(super) fn expect_symbol(&mut self, symbol: &str) -> Result<(), ParseError> {
        if self.eat_symbol(symbol) {
            Ok(())
        } else {
            Err(ParseError(format!("“{symbol}” was expected")))
        }
    }

    pub(super) fn expect_word(&mut self, word: &str) -> Result<(), ParseError> {
        if self.is_word(word) {
            self.at += 1;
            Ok(())
        } else {
            Err(ParseError(format!("“{word}” was expected")))
        }
    }

    pub(super) fn enter(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ParseError("the script nests too deeply".into()));
        }
        Ok(())
    }

    pub(super) fn leave(&mut self) {
        self.depth -= 1;
    }

    pub(super) fn at_end(&self) -> bool {
        self.at >= self.tokens.len()
    }
}

// expressions, in both languages
impl Parser {
    pub(super) fn expression(&mut self) -> Result<Expr, ParseError> {
        self.enter()?;
        let condition = self.binary(1)?;
        let result = if self.eat_symbol("?") {
            let then = self.expression()?;
            self.expect_symbol(":")?;
            let otherwise = self.expression()?;
            Expr::Ternary(Box::new(condition), Box::new(then), Box::new(otherwise))
        } else {
            condition
        };
        self.leave();
        Ok(result)
    }

    fn binary_operator(&self) -> Option<BinaryOp> {
        match self.peek()? {
            Token::Symbol(symbol) => self.symbol_operator(symbol),
            // letters notation also says "and" and "or"
            Token::Word(word) if self.dialect == Dialect::Notation => {
                match word.to_ascii_lowercase().as_str() {
                    "and" => Some(BinaryOp::And),
                    "or" => Some(BinaryOp::Or),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn symbol_operator(&self, symbol: &str) -> Option<BinaryOp> {
        let notation = self.dialect == Dialect::Notation;
        Some(match symbol {
            "+" => BinaryOp::Add,
            "-" => BinaryOp::Subtract,
            "*" => BinaryOp::Multiply,
            "/" => BinaryOp::Divide,
            // in letters notation % is a percentage, not a remainder
            "%" if !notation => BinaryOp::Remainder,
            "<" => BinaryOp::Less,
            "<=" => BinaryOp::LessOrEqual,
            ">" => BinaryOp::Greater,
            ">=" => BinaryOp::GreaterOrEqual,
            "==" | "===" => BinaryOp::Equal,
            "=" if notation => BinaryOp::Equal,
            "!=" | "!==" => BinaryOp::NotEqual,
            "&&" => BinaryOp::And,
            "||" => BinaryOp::Or,
            _ => return None,
        })
    }

    /// precedence climbing: every operator here is left associative.
    fn binary(&mut self, minimum: u8) -> Result<Expr, ParseError> {
        let mut left = self.unary()?;
        while let Some(op) = self.binary_operator() {
            if op.precedence() < minimum {
                break;
            }
            self.at += 1;
            let right = self.binary(op.precedence() + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        self.enter()?;
        let result = if self.eat_symbol("-") {
            // a minus in front of a number is part of the number
            self.unary().map(|inner| match inner {
                Expr::Number(value) => Expr::Number(-value),
                other => Expr::Unary(UnaryOp::Negate, Box::new(other)),
            })
        } else if self.eat_symbol("+") {
            self.unary()
                .map(|inner| Expr::Unary(UnaryOp::Plus, Box::new(inner)))
        } else if self.eat_symbol("!")
            || (self.dialect == Dialect::Notation && self.eat_word_insensitive("not"))
        {
            self.unary()
                .map(|inner| Expr::Unary(UnaryOp::Not, Box::new(inner)))
        } else {
            self.postfix()
        };
        self.leave();
        result
    }

    /// a primary followed, in letters notation, by an optional percent sign.
    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let primary = self.primary()?;
        if self.dialect != Dialect::Notation || !self.eat_symbol("%") {
            return Ok(primary);
        }
        Ok(match primary {
            Expr::Number(value) => Expr::Number(value / 100.0),
            other => Expr::Binary(
                BinaryOp::Divide,
                Box::new(other),
                Box::new(Expr::Number(100.0)),
            ),
        })
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        match self.next() {
            Some(Token::Number(value)) => Ok(Expr::Number(value)),
            // in letters notation quotes name a field whose name has spaces
            Some(Token::Text(text)) if self.dialect == Dialect::Notation => Ok(Expr::Field(text)),
            Some(Token::Text(text)) => Ok(Expr::Text(text)),
            Some(Token::Symbol("(")) => {
                let inner = self.expression()?;
                self.expect_symbol(")")?;
                Ok(inner)
            }
            Some(Token::Word(word)) if self.dialect == Dialect::Notation => {
                self.notation_word(word)
            }
            Some(Token::Word(word)) => self.javascript_word(word),
            Some(Token::Symbol(symbol)) => {
                Err(ParseError(format!("“{symbol}” isn’t expected here")))
            }
            None => Err(ParseError("the script ends too early".into())),
        }
    }

    pub(super) fn arguments(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect_symbol("(")?;
        let mut arguments = Vec::new();
        if self.eat_symbol(")") {
            return Ok(arguments);
        }
        loop {
            arguments.push(self.expression()?);
            if self.eat_symbol(")") {
                return Ok(arguments);
            }
            self.expect_symbol(",")?;
        }
    }
}

// the words each language knows
impl Parser {
    fn javascript_word(&mut self, word: String) -> Result<Expr, ParseError> {
        match word.as_str() {
            "true" => Ok(Expr::Bool(true)),
            "false" => Ok(Expr::Bool(false)),
            "Number" | "parseFloat" => self.call(Function::ToNumber),
            "AFMakeNumber" => self.call(Function::MakeNumber),
            "Math" => self.math_call(),
            "event" => {
                self.expect_symbol(".")?;
                self.expect_word("value")?;
                Ok(Expr::ThisValue)
            }
            "this" => {
                self.expect_symbol(".")?;
                self.expect_word("getField")?;
                self.field_reference()
            }
            "getField" => self.field_reference(),
            _ if self.locals.contains(&word) => Ok(Expr::Local(word)),
            _ => Err(ParseError(format!(
                "“{word}” isn’t something Letters knows"
            ))),
        }
    }

    fn call(&mut self, function: Function) -> Result<Expr, ParseError> {
        let arguments = self.arguments()?;
        check_arity(function, arguments)
    }

    /// Math.round(...) and the other whitelisted Math functions.
    fn math_call(&mut self) -> Result<Expr, ParseError> {
        self.expect_symbol(".")?;
        let Some(Token::Word(name)) = self.next() else {
            return Err(ParseError("Math. needs a function name".into()));
        };
        let function = match name.as_str() {
            "round" => Function::Round,
            "floor" => Function::Floor,
            "ceil" => Function::Ceil,
            "min" => Function::Min,
            "max" => Function::Max,
            "abs" => Function::Abs,
            other => return Err(ParseError(format!("Math.{other} isn’t supported"))),
        };
        self.call(function)
    }

    /// ("name").value after getField: the only way a script may read a field.
    fn field_reference(&mut self) -> Result<Expr, ParseError> {
        self.expect_symbol("(")?;
        let Some(Token::Text(name)) = self.next() else {
            return Err(ParseError("getField needs a field name in quotes".into()));
        };
        self.expect_symbol(")")?;
        self.expect_symbol(".")?;
        let Some(Token::Word(property)) = self.next() else {
            return Err(ParseError("getField(...) needs .value".into()));
        };
        if property != "value" && property != "valueAsString" {
            return Err(ParseError(format!(".{property} isn’t supported")));
        }
        Ok(Expr::Field(name))
    }

    fn notation_word(&mut self, word: String) -> Result<Expr, ParseError> {
        if self.is_symbol("(") {
            let function = Function::from_notation(&word)
                .ok_or_else(|| ParseError(format!("“{word}” isn’t a function Letters knows")))?;
            return self.call(function);
        }
        if word.eq_ignore_ascii_case("value") {
            return Ok(Expr::ThisValue);
        }
        // field names may have dots in them: address.total, item.1
        let mut name = word;
        while self.is_symbol(".") {
            match self.peek_at(1).cloned() {
                Some(Token::Word(part)) => name.push_str(&format!(".{part}")),
                Some(Token::Number(number)) => {
                    let _ = write!(name, ".{number}");
                }
                _ => break,
            }
            self.at += 2;
        }
        Ok(Expr::Field(name))
    }
}

fn check_arity(function: Function, arguments: Vec<Expr>) -> Result<Expr, ParseError> {
    let fits = match function {
        Function::Min | Function::Max => !arguments.is_empty(),
        _ => arguments.len() == 1,
    };
    if fits {
        Ok(Expr::Call(function, arguments))
    } else {
        Err(ParseError(format!(
            "{} was given the wrong number of values",
            function.javascript_name()
        )))
    }
}
