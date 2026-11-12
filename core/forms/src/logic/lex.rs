//! splitting a script into tokens (numbers, text, words and symbols) for
//! the parser in `parse.rs`. comments are dropped here.

use super::script::ParseError;

/// scripts longer than this are not read at all.
pub const MAX_SOURCE_LENGTH: usize = 8192;

/// more tokens than this is not a simple script. it also bounds how deep
/// a chain like a + b + c ... can grow, which keeps running and dropping
/// the tree well inside the stack (measured: 250 terms run fine in a debug
/// build on a 2 mb test thread, 700 overflowed it).
const MAX_TOKENS: usize = 600;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Token {
    Number(f64),
    Text(String),
    Word(String),
    Symbol(&'static str),
}

/// the symbols both languages use, longest first so "<=" wins over "<".
const SYMBOLS: [&str; 26] = [
    "===", "!==", "==", "!=", "<=", ">=", "&&", "||", "+", "-", "*", "/", "%", "(", ")", "<", ">",
    "!", "?", ":", ",", ".", ";", "=", "{", "}",
];

/// reads characters one token at a time.
struct Lexer {
    chars: Vec<char>,
    at: usize,
}

pub(super) fn lex(source: &str) -> Result<Vec<Token>, ParseError> {
    if source.len() > MAX_SOURCE_LENGTH {
        return Err(ParseError("the script is too long".into()));
    }
    let mut lexer = Lexer {
        chars: source.chars().collect(),
        at: 0,
    };
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next_token()? {
        tokens.push(token);
        if tokens.len() > MAX_TOKENS {
            return Err(ParseError("the script is too long".into()));
        }
    }
    Ok(tokens)
}

impl Lexer {
    fn current(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn following(&self) -> Option<char> {
        self.chars.get(self.at + 1).copied()
    }

    /// the next token, or none at the end. whitespace and comments are skipped.
    fn next_token(&mut self) -> Result<Option<Token>, ParseError> {
        self.skip_space_and_comments();
        let Some(c) = self.current() else {
            return Ok(None);
        };
        let starts_number = c.is_ascii_digit()
            || (c == '.' && self.following().is_some_and(|d| d.is_ascii_digit()));
        let token = if starts_number {
            self.number()?
        } else if c == '"' || c == '\'' {
            self.text(c)?
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            self.word()
        } else {
            self.symbol(c)?
        };
        Ok(Some(token))
    }

    fn skip_space_and_comments(&mut self) {
        loop {
            match (self.current(), self.following()) {
                (Some(c), _) if c.is_whitespace() => self.at += 1,
                (Some('/'), Some('/')) => {
                    while self.current().is_some_and(|c| c != '\n' && c != '\r') {
                        self.at += 1;
                    }
                }
                (Some('/'), Some('*')) => {
                    self.at += 2;
                    while self.current().is_some()
                        && !(self.current() == Some('*') && self.following() == Some('/'))
                    {
                        self.at += 1;
                    }
                    self.at += 2;
                }
                _ => return,
            }
        }
    }

    /// digits with an optional decimal point and exponent (1.5e3).
    fn number(&mut self) -> Result<Token, ParseError> {
        let start = self.at;
        while self
            .current()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
        {
            self.at += 1;
        }
        if matches!(self.current(), Some('e' | 'E')) {
            let mut look = self.at + 1;
            if matches!(self.chars.get(look), Some('+' | '-')) {
                look += 1;
            }
            if self.chars.get(look).is_some_and(char::is_ascii_digit) {
                self.at = look;
                while self.current().is_some_and(|c| c.is_ascii_digit()) {
                    self.at += 1;
                }
            }
        }
        let text: String = self.chars[start..self.at].iter().collect();
        text.parse()
            .map(Token::Number)
            .map_err(|_| ParseError(format!("“{text}” isn’t a number")))
    }

    /// a quoted piece of text with backslash escapes.
    fn text(&mut self, quote: char) -> Result<Token, ParseError> {
        self.at += 1;
        let mut text = String::new();
        loop {
            let Some(next) = self.current() else {
                return Err(ParseError("a piece of text has no closing quote".into()));
            };
            self.at += 1;
            if next == quote {
                return Ok(Token::Text(text));
            }
            if next != '\\' {
                text.push(next);
                continue;
            }
            let Some(escaped) = self.current() else {
                return Err(ParseError("a piece of text ends in \\".into()));
            };
            self.at += 1;
            text.push(match escaped {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
        }
    }

    fn word(&mut self) -> Token {
        let start = self.at;
        while self
            .current()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            self.at += 1;
        }
        Token::Word(self.chars[start..self.at].iter().collect())
    }

    fn symbol(&mut self, c: char) -> Result<Token, ParseError> {
        let end = self.chars.len().min(self.at + 3);
        let rest: String = self.chars[self.at..end].iter().collect();
        let symbol = SYMBOLS
            .iter()
            .find(|symbol| rest.starts_with(**symbol))
            .ok_or_else(|| ParseError(format!("“{c}” isn’t understood")))?;
        self.at += symbol.chars().count();
        Ok(Token::Symbol(symbol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_escapes_are_handled() {
        let tokens = lex("/* a */ x = 'it\\'s' // end\n + 1.5e2").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Word("x".into()),
                Token::Symbol("="),
                Token::Text("it's".into()),
                Token::Symbol("+"),
                Token::Number(150.0),
            ]
        );
    }

    #[test]
    fn unknown_characters_are_refused() {
        assert!(lex("a # b").is_err());
        assert!(lex("\"open").is_err());
    }
}
