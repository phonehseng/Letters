//! reads formula text into an `Expr` tree. the cursor and its small
//! helpers live here; each kind of thing the parser reads has its own
//! file: operators (precedence levels), literals, references, table
//! references and openformula references.
//!
//! two syntaxes are understood: excel's ("SUM(Sheet2!A1:B3,5)") and
//! opendocument's openformula ("SUM([$Sheet2.A1:.B3];5)"), and references
//! can be read in r1c1 style relative to a cell.

mod literals;
mod open_formula;
mod operators;
mod references;
mod structured;
#[cfg(test)]
mod tests;

use crate::address::CellRef;

use super::ast::Expr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    Excel,
    OpenFormula,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// character offset where reading stopped
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at character {})", self.message, self.position + 1)
    }
}

/// parses an excel formula (with or without the leading "=").
pub fn parse(text: &str) -> Result<Expr, ParseError> {
    Parser::new(text, Syntax::Excel, None).run()
}

/// parses an openformula text as ods stores it ("of:=SUM([.A1:.A3])").
pub fn parse_open_formula(text: &str) -> Result<Expr, ParseError> {
    let without_namespace = text
        .strip_prefix("of:")
        .or_else(|| text.strip_prefix("oooc:"));
    Parser::new(without_namespace.unwrap_or(text), Syntax::OpenFormula, None).run()
}

/// parses a formula written with r1c1 references, relative to `at`.
pub fn parse_r1c1(text: &str, at: CellRef) -> Result<Expr, ParseError> {
    Parser::new(text, Syntax::Excel, Some(at)).run()
}

pub use literals::canonical_function_name;

/// excel allows 64 levels of nested functions; the same limit keeps the
/// recursive parser and evaluator well inside the stack
const MAX_NESTING: usize = 64;

struct Parser {
    chars: Vec<char>,
    pos: usize,
    syntax: Syntax,
    /// the cell r1c1 references are relative to, when reading r1c1
    r1c1: Option<CellRef>,
    depth: usize,
}

/// characters that can start a name or function
fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '\\'
}

/// characters that can continue one (excel allows dots and question marks)
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '\\' || c == '?'
}

impl Parser {
    fn new(text: &str, syntax: Syntax, r1c1: Option<CellRef>) -> Parser {
        let text = text.trim();
        let text = text.strip_prefix('=').unwrap_or(text);
        Parser {
            chars: text.chars().collect(),
            pos: 0,
            syntax,
            r1c1,
            depth: 0,
        }
    }

    fn run(mut self) -> Result<Expr, ParseError> {
        self.skip_spaces();
        if self.chars.is_empty() {
            return Err(self.error("the formula is empty"));
        }
        let expr = self.comparison()?;
        self.skip_spaces();
        if let Some(c) = self.peek() {
            return Err(self.error(&format!("unexpected \"{c}\"")));
        }
        Ok(expr)
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            position: self.pos,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    /// skips layout; true when anything was skipped (a space between two
    /// references is excel's intersection operator)
    fn skip_spaces(&mut self) -> bool {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| matches!(c, ' ' | '\n' | '\r' | '\t'))
        {
            self.pos += 1;
        }
        self.pos > start
    }

    fn eat(&mut self, text: &str) -> bool {
        let wanted: Vec<char> = text.chars().collect();
        if self.chars[self.pos..].starts_with(&wanted) {
            self.pos += wanted.len();
            true
        } else {
            false
        }
    }

    fn eat_ignore_case(&mut self, text: &str) -> bool {
        let wanted: Vec<char> = text.chars().collect();
        let end = self.pos + wanted.len();
        if end > self.chars.len() {
            return false;
        }
        let matches = self.chars[self.pos..end]
            .iter()
            .zip(&wanted)
            .all(|(a, b)| a.eq_ignore_ascii_case(b));
        if matches {
            self.pos = end;
        }
        matches
    }

    /// the text from `start` to the cursor
    fn text_from(&self, start: usize) -> String {
        self.chars[start..self.pos].iter().collect()
    }

    fn name_text(&mut self) -> String {
        let start = self.pos;
        while self.peek().is_some_and(is_name_char) {
            self.pos += 1;
        }
        self.text_from(start)
    }

    fn arg_separator(&self) -> char {
        match self.syntax {
            Syntax::Excel => ',',
            Syntax::OpenFormula => ';',
        }
    }

    /// one level deeper; refuses formulas nested past excel's limit
    fn nest(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(self.error("the formula is nested too deeply"));
        }
        Ok(())
    }
}
