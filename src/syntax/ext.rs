//! Generic extension blocks: the language-level hook that lets a *new domain* add its own top-level
//! declarations (`plumbing Main { ... }`, `hvac Duct1 { ... }`, `pcb Board { ... }`) **without touching the lexer or the
//! core parser**.
//!
//! The core parser captures any unknown `keyword [header...] { body }` inside a `project` as an [`ExtensionBlock`]
//! (balanced braces, raw tokens, exact source spans). A registered [`crate::domains::registry::DomainEngine`] that
//! claims the keyword turns it into a typed [`ExtNode`] tree with [`ExtensionBlock::parse_tree`] and compiles it.
//!
//! Generic block grammar (deliberately the same shape as every built-in domain):
//!
//! ```text
//! body   := entry*
//! entry  := key ':' value                      // property
//!         | kind [Name] [':' Subtype] '{' body '}'   // child block
//! value  := number[unit] | Ident[.Ident]* | "string" | '[' value,* ']' | '(' value,* ')'
//! ```

use crate::core::span::{Diagnostic, Span};
use crate::syntax::lexer::{Token, TokenKind};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ExtensionBlock {
    pub keyword: String,
    pub file: String,
    /// Tokens between the keyword and the opening `{` (usually a name).
    pub header: Vec<Token>,
    /// Tokens strictly between the outer braces.
    pub body: Vec<Token>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtValue {
    Number { value: f64, unit: String },
    Ident(String),
    Str(String),
    List(Vec<ExtValue>),
    Tuple(Vec<ExtValue>),
}

impl ExtValue {
    /// Numeric value converted to millimetres when the unit is a length unit, otherwise the raw number.
    pub fn as_mm(&self) -> Option<f64> {
        match self {
            ExtValue::Number { value, unit } => Some(match unit.as_str() {
                "" | "mm" => *value,
                "cm" => value * 10.0,
                "m" => value * 1000.0,
                "in" => value * 25.4,
                "ft" => value * 304.8,
                _ => return None,
            }),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ExtValue::Number { value, .. } => Some(*value),
            _ => None,
        }
    }
    pub fn as_ident(&self) -> Option<&str> {
        match self {
            ExtValue::Ident(s) | ExtValue::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtNode {
    pub kind: String,
    pub name: Option<String>,
    pub subtype: Option<String>,
    pub props: BTreeMap<String, (ExtValue, Span)>,
    pub children: Vec<ExtNode>,
    pub span: Span,
}

impl ExtNode {
    pub fn prop(&self, key: &str) -> Option<&ExtValue> {
        self.props.get(key).map(|(v, _)| v)
    }
    pub fn mm(&self, key: &str) -> Option<f64> {
        self.prop(key).and_then(|v| v.as_mm())
    }
    pub fn num(&self, key: &str) -> Option<f64> {
        self.prop(key).and_then(|v| v.as_f64())
    }
    pub fn ident(&self, key: &str) -> Option<&str> {
        self.prop(key).and_then(|v| v.as_ident())
    }
    pub fn children_of<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a ExtNode> + 'a {
        self.children.iter().filter(move |c| c.kind == kind)
    }
}

impl ExtensionBlock {
    pub fn name(&self) -> Option<String> {
        self.header.iter().find_map(|t| match &t.kind {
            TokenKind::Ident(s) | TokenKind::Str(s) => Some(s.clone()),
            _ => None,
        })
    }

    /// Parse the body into a generic typed tree (root node has `kind == keyword`).
    pub fn parse_tree(&self) -> Result<ExtNode, Diagnostic> {
        let mut toks = self.body.clone();
        toks.push(Token {
            kind: TokenKind::Eof,
            line: self.span.end_line,
            col: self.span.end_col,
            end_line: self.span.end_line,
            end_col: self.span.end_col,
        });
        let mut p = TreeParser {
            file: self.file.clone(),
            toks,
            pos: 0,
        };
        let (props, children) = p.parse_body()?;
        Ok(ExtNode {
            kind: self.keyword.clone(),
            name: self.name(),
            subtype: None,
            props,
            children,
            span: self.span.clone(),
        })
    }
}

struct TreeParser {
    file: String,
    toks: Vec<Token>,
    pos: usize,
}

type Body = (BTreeMap<String, (ExtValue, Span)>, Vec<ExtNode>);

impl TreeParser {
    fn peek(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }
    fn peek_at(&self, n: usize) -> &Token {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)]
    }
    fn bump(&mut self) -> Token {
        let t = self.peek().clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn err(&self, msg: impl Into<String>, t: &Token) -> Diagnostic {
        Diagnostic::error("E_PARSE_EXT", "syntax", msg, Some(t.span(&self.file)))
    }

    fn parse_body(&mut self) -> Result<Body, Diagnostic> {
        let mut props = BTreeMap::new();
        let mut children = Vec::new();
        while !matches!(self.peek().kind, TokenKind::Eof | TokenKind::RBrace) {
            let first = self.bump();
            let TokenKind::Ident(key) = first.kind.clone() else {
                return Err(self.err(format!("Expected identifier, found {:?}", first.kind), &first));
            };
            // key: value   |   kind Name: Sub { ... }   |   kind Name { ... }   |   kind { ... }
            let mut name = None;
            if let TokenKind::Ident(n) = &self.peek().kind {
                name = Some(n.clone());
                self.bump();
            }
            let mut subtype = None;
            let mut value = None;
            if self.peek().kind == TokenKind::Colon {
                self.bump();
                let v = self.parse_value()?;
                if name.is_some() || matches!(self.peek().kind, TokenKind::LBrace) {
                    if let ExtValue::Ident(s) = &v {
                        subtype = Some(s.clone());
                    } else {
                        return Err(self.err("Expected a type name after ':' in block header", &first));
                    }
                } else {
                    value = Some(v);
                }
            }
            if self.peek().kind == TokenKind::LBrace {
                self.bump();
                let (p, c) = self.parse_body()?;
                let end = self.bump();
                if end.kind != TokenKind::RBrace {
                    return Err(self.err("Expected '}'", &end));
                }
                children.push(ExtNode {
                    kind: key,
                    name,
                    subtype,
                    props: p,
                    children: c,
                    span: first.span(&self.file).merge(&end.span(&self.file)),
                });
            } else if let Some(v) = value {
                if name.is_some() {
                    return Err(self.err(format!("Unexpected name '{}' before ':'", name.unwrap()), &first));
                }
                props.insert(key, (v, first.span(&self.file)));
            } else {
                return Err(self.err(format!("Expected ':' or '{{' after '{key}'"), &first));
            }
        }
        Ok((props, children))
    }

    fn parse_value(&mut self) -> Result<ExtValue, Diagnostic> {
        let t = self.bump();
        match t.kind.clone() {
            TokenKind::Number(v, u) => Ok(ExtValue::Number { value: v, unit: u }),
            TokenKind::Minus => match self.parse_value()? {
                ExtValue::Number { value, unit } => Ok(ExtValue::Number { value: -value, unit }),
                _ => Err(self.err("'-' must precede a number", &t)),
            },
            TokenKind::Str(s) => Ok(ExtValue::Str(s)),
            TokenKind::Ident(mut s) => {
                while self.peek().kind == TokenKind::Dot {
                    if let TokenKind::Ident(n) = &self.peek_at(1).kind {
                        s.push('.');
                        s.push_str(n);
                        self.bump();
                        self.bump();
                    } else {
                        break;
                    }
                }
                Ok(ExtValue::Ident(s))
            }
            TokenKind::LBracket | TokenKind::LParen => {
                let close = if t.kind == TokenKind::LBracket {
                    TokenKind::RBracket
                } else {
                    TokenKind::RParen
                };
                let mut items = Vec::new();
                while self.peek().kind != close && self.peek().kind != TokenKind::Eof {
                    items.push(self.parse_value()?);
                    if self.peek().kind == TokenKind::Comma {
                        self.bump();
                    }
                }
                let e = self.bump();
                if e.kind != close {
                    return Err(self.err("Unterminated list/tuple", &t));
                }
                Ok(if close == TokenKind::RBracket {
                    ExtValue::List(items)
                } else {
                    ExtValue::Tuple(items)
                })
            }
            other => Err(self.err(format!("Unexpected token {other:?} in value position"), &t)),
        }
    }
}
