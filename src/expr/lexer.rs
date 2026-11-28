//! Lexer turns source string to tokens with byte spans
//!
//! Strict grammar rules:
//! ```text
//! IDENT := [a-z_][a-z0-9_]*
//! DURATION := a `units::Duration` literal (i.e. "45m, "1h30m")
//! TIME := a `units::TimeOfDay` literal (i.e. "07:30")
//! PUNCT := + - . , ( )
//! ```
//! Whitespace separates tokens and ignored after that.
//! Token starting with a digit gets scanned with `[0-9a-z:]`,
//! then gets classified as either `TIME` (if it has `:`) or
//! DURATION otherwise.

use std::fmt;
use std::ops::Range;
use thiserror::Error;

use crate::config::units::{Duration, TimeOfDay, UnitError};

pub type Span = Range<usize>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String),
    Duration(Duration),
    Time(TimeOfDay),
    Plus,
    Minus,
    Dot,
    Comma,
    LParen,
    RParen,
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Ident(s) => write!(f, "identifier `{s}`"),
            TokenKind::Duration(d) => write!(f, "duration `{d}`"),
            TokenKind::Time(t) => write!(f, "time `{t}`"),
            TokenKind::Plus => f.write_str("`+`"),
            TokenKind::Minus => f.write_str("`-`"),
            TokenKind::Dot => f.write_str("`.`"),
            TokenKind::Comma => f.write_str("`,`"),
            TokenKind::LParen => f.write_str("`(`"),
            TokenKind::RParen => f.write_str("`)`"),
            TokenKind::Eof => f.write_str("end of expression"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LexError {
    #[error("unexpected character {ch:?}")]
    UnexpectedChar { ch: char, span: Span },
    #[error("bad duration: {source}")]
    BadDuration { source: UnitError, span: Span},
    #[error("bad time: {source}")]
    BadTime { source: UnitError, span: Span },
}

impl LexError {
    pub fn span(&self) -> Span {
        match self {
            LexError::UnexpectedChar { span, .. }
            | LexError::BadDuration { span, .. }
            | LexError::BadTime { span, .. } => span.clone(),
        }
    }
}

/// Tokenizes all input short-circuits
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let b = bytes[i];
        let start = i;
        let punct = match b {
            b' ' | b'\t' => {
                i += 1;
                continue;
            }
            b'+' => Some(TokenKind::Plus),
            b'-' => Some(TokenKind::Minus),
            b'.' => Some(TokenKind::Dot),
            b',' => Some(TokenKind::Comma),
            b'(' => Some(TokenKind::LParen),
            b')' => Some(TokenKind::RParen),
            _ => None,
        };
        if let Some(kind) = punct {
            i += 1;
            out.push(Token { kind, span: start..i });
            continue;
        }
        if b.is_ascii_lowercase() || b == b'_' {
            while i < bytes.len() && is_ident_continue(bytes[i]) {
                i += 1;
            }
            out.push(Token {
                kind: TokenKind::Ident(src[start..i].to_owned()),
                span: start..i,
            });
            continue;
        }
        if b.is_ascii_digit() {
            while i < bytes.len() && is_literal_continue(bytes[i]) {
                i += 1;
            }
            let text = &src[start..i];
            let span = start..i;
            let kind = if text.contains(':') {
                text.parse()
                    .map(TokenKind::Time)
                    .map_err(|source| LexError::BadTime { source, span: span.clone() })?
            } else {
                text.parse()
                    .map(TokenKind::Duration)
                    .map_err(|source| LexError::BadDuration { source, span: span.clone() })?
            };
            out.push(Token { kind, span });
            continue;
        }

        let ch = src[start..].chars().next().expect("i < len, so a char exists");
        return Err(LexError::UnexpectedChar {
            ch,
            span: start..start + ch.len_utf8(),
        });
    }
    out.push(Token { kind: TokenKind::Eof, span: src.len()..src.len() });
    Ok(out)
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
}

fn is_literal_continue(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b':'
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenKind::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    fn id(s: &str) -> TokenKind {
        Ident(s.to_owned())
    }

    fn duration(s: &str) -> TokenKind {
        Duration(s.parse().unwrap())
    }

    #[test]
    fn real_expressions() {
        assert_eq!(
            kinds("first_commitment - prep.max - commute(mode).max"),
            [
                id("first_commitment"), Minus,
                id("prep"), Dot, id("max"), Minus,
                id("commute"), LParen, id("mode"), RParen, Dot, id("max"),
                Eof,
            ]
        );
        assert_eq!(
            kinds("wake_at - target"),
            [
                id("wake_at"), Minus,
                id("target"), Eof,
            ]
        );
        assert_eq!(kinds("bedtime"), [id("bedtime"), Eof]);
    }

    #[test]
    fn literals() {
        assert_eq!(kinds("07:30 + 1h30m"), [Time("07:30".parse().unwrap()), Plus, duration("1h30m"), Eof]);
        assert_eq!(kinds("f(a, 8h)"), [id("f"), LParen, id("a"), Comma, duration("8h"), RParen, Eof]);
    }

    #[test]
    fn whitespace_is_optional() {
        assert_eq!(kinds("a-b"), kinds("a - b"));
        assert_eq!(kinds("\ta\t + b "), [id("a"), Plus, id("b"), Eof]);
        assert_eq!(kinds(""), [Eof]);
        assert_eq!(kinds("   "), [Eof]);   
    }

    #[test]
    fn spans_are_byte_ranges() {
        let toks = lex("wake_at - 8h").unwrap();
        let spans: Vec<_> = toks.iter().map(|t| t.span.clone()).collect();
        assert_eq!(spans, [0..7, 8..9, 10..12, 12..12]);
        let src = "wake_at - 8h";
        assert_eq!(&src[toks[0].span.clone()], "wake_at");
        assert_eq!(&src[toks[2].span.clone()], "8h");
    }

    #[test]
    fn literal_errors_cover_entire_literal() {
        match lex("a - 7h30") {
            Err(LexError::BadDuration { span, .. }) => assert_eq!(span, 4..8),
            other => panic!("{other:?}"),
        }
        match lex("07:3 + a") {
            Err(LexError::BadTime { span, .. }) => assert_eq!(span, 0..4),
            other => panic!("{other:?}"),
        }
        assert!(matches!(lex("25:00"), Err(LexError::BadTime { .. })));
        assert!(matches!(lex("30m1h"), Err(LexError::BadDuration { .. })));
        assert!(matches!(lex("45"), Err(LexError::BadDuration { .. })));
    }

    #[test]
    fn unexpected_chars() {
        for (src,ch,at) in [
            ("a * b", '*', 2),
            ("Wake_at", 'W', 0),
            ("a\nb", '\n', 1),
            ("a – b", '–', 2),
            ("a\"", '"', 1),
        ] {
            match lex(src) {
                Err(LexError::UnexpectedChar { ch: c, span }) => {
                    assert_eq!(c, ch, "{src:?}");
                    assert_eq!(span, at..at + ch.len_utf8(), "{src:?}");
                }
                other => panic!("{src:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn ident_then_digits_stays_one_ident() {
        assert_eq!(kinds("arc2 + x_1"), [id("arc2"), Plus, id("x_1"), Eof]);
    }

    #[test]
    fn all_real_config_expressions_lex() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/life.yaml");
        let life = crate::config::life::Life::load(&path).unwrap().life;
        for e in [&life.sleep.wake_at, &life.sleep.bedtime, &life.friends.nights_out.end_by] {
            lex(&e.0).unwrap_or_else(|err| panic!("{:?}: {err:?}", e.0));
        }
    }
}

