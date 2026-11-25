use crate::core::span::{Diagnostic, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Str(String),
    Number(f64, String), // value + optional unit suffix
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Dot,
    DotDot,
    Plus,
    Minus,
    Star,
    Eq,
    EqEq,
    Ge,
    Arrow,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Token {
    pub fn span(&self, file: &str) -> Span {
        Span::new(file, self.line, self.col, self.end_line, self.end_col)
    }
}

pub fn lex(file: &str, src: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut line = 1usize;
    let mut col = 1usize;

    while i < n {
        let ch = chars[i];
        if ch == ' ' || ch == '\t' || ch == '\r' {
            i += 1;
            col += 1;
            continue;
        }
        if ch == '\n' {
            i += 1;
            line += 1;
            col = 1;
            continue;
        }
        if ch == '/' && i + 1 < n && chars[i + 1] == '/' {
            while i < n && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }
        if ch == '#' {
            while i < n && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }

        let start_line = line;
        let start_col = col;

        if ch == '"' {
            i += 1;
            col += 1;
            let mut s = String::new();
            let mut closed = false;
            while i < n {
                let c = chars[i];
                if c == '"' {
                    i += 1;
                    col += 1;
                    closed = true;
                    break;
                }
                if c == '\n' {
                    return Err(Diagnostic::error(
                        "E_LEX_STRING",
                        "syntax",
                        "Unterminated string literal",
                        Some(Span::new(file, start_line, start_col, line, col)),
                    ));
                }
                s.push(c);
                i += 1;
                col += 1;
            }
            if !closed {
                return Err(Diagnostic::error(
                    "E_LEX_STRING",
                    "syntax",
                    "Unterminated string literal at EOF",
                    Some(Span::new(file, start_line, start_col, line, col)),
                ));
            }
            tokens.push(Token {
                kind: TokenKind::Str(s),
                line: start_line,
                col: start_col,
                end_line: line,
                end_col: col,
            });
            continue;
        }

        if ch.is_ascii_digit() {
            let mut num_str = String::new();
            let mut seen_dot = false;
            while i < n {
                let c = chars[i];
                if c.is_ascii_digit() {
                    num_str.push(c);
                    i += 1;
                    col += 1;
                } else if c == '.' && !seen_dot && i + 1 < n && chars[i + 1].is_ascii_digit() {
                    seen_dot = true;
                    num_str.push(c);
                    i += 1;
                    col += 1;
                } else {
                    break;
                }
            }
            let mut unit_str = String::new();
            if i < n && chars[i] == '%' {
                unit_str.push('%');
                i += 1;
                col += 1;
            } else {
                while i < n && chars[i].is_ascii_alphabetic() {
                    unit_str.push(chars[i]);
                    i += 1;
                    col += 1;
                }
                // Support area units like `mm2`, `cm2`, `m2`
                if !unit_str.is_empty() && i < n && chars[i] == '2' {
                    unit_str.push('2');
                    i += 1;
                    col += 1;
                }
            }
            let val: f64 = num_str.parse().map_err(|_| {
                Diagnostic::error(
                    "E_LEX_NUMBER",
                    "syntax",
                    format!("Invalid numeric literal '{num_str}'"),
                    Some(Span::new(file, start_line, start_col, line, col)),
                )
            })?;
            tokens.push(Token {
                kind: TokenKind::Number(val, unit_str),
                line: start_line,
                col: start_col,
                end_line: line,
                end_col: col,
            });
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut ident = String::new();
            while i < n && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                ident.push(chars[i]);
                i += 1;
                col += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Ident(ident),
                line: start_line,
                col: start_col,
                end_line: line,
                end_col: col,
            });
            continue;
        }

        // Two-char operators
        if ch == '.' && i + 1 < n && chars[i + 1] == '.' {
            i += 2;
            col += 2;
            tokens.push(Token {
                kind: TokenKind::DotDot,
                line: start_line,
                col: start_col,
                end_line: line,
                end_col: col,
            });
            continue;
        }
        if ch == '-' && i + 1 < n && chars[i + 1] == '>' {
            i += 2;
            col += 2;
            tokens.push(Token {
                kind: TokenKind::Arrow,
                line: start_line,
                col: start_col,
                end_line: line,
                end_col: col,
            });
            continue;
        }
        if ch == '=' && i + 1 < n && chars[i + 1] == '=' {
            i += 2;
            col += 2;
            tokens.push(Token {
                kind: TokenKind::EqEq,
                line: start_line,
                col: start_col,
                end_line: line,
                end_col: col,
            });
            continue;
        }
        if ch == '>' && i + 1 < n && chars[i + 1] == '=' {
            i += 2;
            col += 2;
            tokens.push(Token {
                kind: TokenKind::Ge,
                line: start_line,
                col: start_col,
                end_line: line,
                end_col: col,
            });
            continue;
        }

        let kind = match ch {
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ':' => TokenKind::Colon,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '=' => TokenKind::Eq,
            _ => {
                return Err(Diagnostic::error(
                    "E_LEX_CHAR",
                    "syntax",
                    format!("Unexpected character '{ch}'"),
                    Some(Span::new(file, start_line, start_col, line, col + 1)),
                ));
            }
        };
        i += 1;
        col += 1;
        tokens.push(Token {
            kind,
            line: start_line,
            col: start_col,
            end_line: line,
            end_col: col,
        });
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        line,
        col,
        end_line: line,
        end_col: col,
    });
    Ok(tokens)
}
