//! the statements of a javascript form script: if/else, var, event.value
//! and event.rc assignments, and app.alert. letters notation has no
//! statements, only the expressions in `parse.rs`.

use super::lex::Token;
use super::parse::Parser;
use super::script::{Expr, ParseError, Stmt};

/// names a script may not use for its own variables, because they mean
/// something to form apps.
fn is_reserved(word: &str) -> bool {
    matches!(
        word,
        "event" | "this" | "app" | "Math" | "if" | "else" | "var" | "true" | "false" | "getField"
    )
}

impl Parser {
    pub(super) fn statements_until_end(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut out = Vec::new();
        while !self.at_end() {
            if self.eat_symbol(";") {
                continue;
            }
            out.extend(self.statement()?);
        }
        Ok(out)
    }

    fn block_or_statement(&mut self) -> Result<Vec<Stmt>, ParseError> {
        if !self.eat_symbol("{") {
            return self.statement();
        }
        self.enter()?;
        let mut out = Vec::new();
        while !self.eat_symbol("}") {
            if self.at_end() {
                return Err(ParseError("a “{” has no closing “}”".into()));
            }
            if self.eat_symbol(";") {
                continue;
            }
            out.extend(self.statement()?);
        }
        self.leave();
        Ok(out)
    }

    fn end_of_statement(&mut self) -> Result<(), ParseError> {
        if self.eat_symbol(";") || self.at_end() || self.is_symbol("}") {
            Ok(())
        } else {
            Err(ParseError("a statement doesn’t end where expected".into()))
        }
    }

    fn statement(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.enter()?;
        let result = self.one_statement();
        self.leave();
        result
    }

    fn one_statement(&mut self) -> Result<Vec<Stmt>, ParseError> {
        if self.is_word("if") {
            return self.if_statement();
        }
        if self.is_word("var") || self.is_word("let") || self.is_word("const") {
            return self.declaration();
        }
        if self.is_word("event") && self.is_symbol_at(1, ".") {
            return self.event_assignment();
        }
        if self.is_word("app") && self.is_symbol_at(1, ".") {
            return self.alert();
        }
        if let Some(Token::Word(name)) = self.peek().cloned() {
            if self.locals.contains(&name) && self.is_symbol_at(1, "=") {
                self.at += 2;
                let value = self.expression()?;
                self.end_of_statement()?;
                return Ok(vec![Stmt::Assign(name, value)]);
            }
        }
        Err(ParseError(
            "it does something other than set the value, check it, or show a message".into(),
        ))
    }

    fn if_statement(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.at += 1;
        self.expect_symbol("(")?;
        let condition = self.expression()?;
        self.expect_symbol(")")?;
        let then = self.block_or_statement()?;
        let otherwise = if self.is_word("else") {
            self.at += 1;
            self.block_or_statement()?
        } else {
            Vec::new()
        };
        Ok(vec![Stmt::If(condition, then, otherwise)])
    }

    /// var a = 1, b; (a variable without a value starts as empty text)
    fn declaration(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.at += 1;
        let mut out = Vec::new();
        loop {
            let Some(Token::Word(name)) = self.next() else {
                return Err(ParseError("a variable needs a name".into()));
            };
            if is_reserved(&name) {
                return Err(ParseError(format!("“{name}” can’t be a variable")));
            }
            let value = if self.eat_symbol("=") {
                self.expression()?
            } else {
                Expr::Text(String::new())
            };
            if !self.locals.contains(&name) {
                self.locals.push(name.clone());
            }
            out.push(Stmt::Assign(name, value));
            if !self.eat_symbol(",") {
                break;
            }
        }
        self.end_of_statement()?;
        Ok(out)
    }

    /// event.value = ... or event.rc = ...; nothing else of event may change.
    fn event_assignment(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let Some(Token::Word(property)) = self.peek_at(2).cloned() else {
            return Err(ParseError("event. needs a property".into()));
        };
        if !self.is_symbol_at(3, "=") {
            return Err(ParseError(format!(
                "event.{property} can’t be used like this"
            )));
        }
        self.at += 4;
        let value = self.expression()?;
        self.end_of_statement()?;
        match property.as_str() {
            "value" => Ok(vec![Stmt::SetValue(value)]),
            "rc" => Ok(vec![Stmt::SetAccepted(value)]),
            other => Err(ParseError(format!("event.{other} can’t be changed"))),
        }
    }

    /// app.alert("message", ...): later arguments only change how it looks.
    fn alert(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.at += 2;
        self.expect_word("alert")?;
        self.expect_symbol("(")?;
        let message = self.expression()?;
        while self.eat_symbol(",") {
            self.expression()?;
        }
        self.expect_symbol(")")?;
        self.end_of_statement()?;
        Ok(vec![Stmt::Alert(message)])
    }
}
