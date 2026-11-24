use crate::ast::*;
use crate::span::{Diagnostic, Span};
use crate::units::{Quantity, Unit};

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    Ident(String),
    Str(String),
    Number(f64, String), // value + optional unit suffix
    LBrace,
    RBrace,
    LParen,
    RParen,
    Colon,
    Comma,
    Dot,
    Plus,
    Minus,
    Star,
    EqEq,
    Ge,
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    line: usize,
    col: usize,
    end_line: usize,
    end_col: usize,
}

impl Token {
    fn span(&self, file: &str) -> Span {
        Span::new(file, self.line, self.col, self.end_line, self.end_col)
    }
}

fn lex(file: &str, src: &str) -> Result<Vec<Token>, Diagnostic> {
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
        // Line comment //
        if ch == '/' && i + 1 < n && chars[i + 1] == '/' {
            while i < n && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }
        // Hash comment #
        if ch == '#' {
            while i < n && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }

        let start_line = line;
        let start_col = col;

        // String literal
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

        // Number + optional unit suffix (e.g. 4500mm, 1.5m, 90deg, 100)
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
            while i < n && chars[i].is_ascii_alphabetic() {
                unit_str.push(chars[i]);
                i += 1;
                col += 1;
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

        // Identifier
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

        // Single-char punctuation
        let kind = match ch {
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            ':' => TokenKind::Colon,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
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

pub fn parse_project(file: &str, src: &str) -> Result<ProjectAst, Diagnostic> {
    let tokens = lex(file, src)?;
    let mut p = Parser {
        file: file.to_string(),
        tokens,
        pos: 0,
    };
    p.parse_project()
}

struct Parser {
    file: String,
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_next(&self) -> &Token {
        &self.tokens[(self.pos + 1).min(self.tokens.len() - 1)]
    }

    fn bump(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn err(&self, code: &str, msg: impl Into<String>, tok: &Token) -> Diagnostic {
        Diagnostic::error(code, "syntax", msg, Some(tok.span(&self.file)))
    }

    fn expect(&mut self, expected: TokenKind, label: &str) -> Result<Token, Diagnostic> {
        let tok = self.peek().clone();
        if tok.kind == expected {
            Ok(self.bump())
        } else {
            Err(self.err(
                "E_PARSE_EXPECTED",
                format!("Expected {label}, found {:?}", tok.kind),
                &tok,
            ))
        }
    }

    fn expect_ident(&mut self, label: &str) -> Result<(String, Span), Diagnostic> {
        let tok = self.peek().clone();
        let sp = tok.span(&self.file);
        match tok.kind {
            TokenKind::Ident(s) => {
                self.bump();
                Ok((s, sp))
            }
            _ => Err(self.err(
                "E_PARSE_IDENT",
                format!("Expected {label}, found {:?}", tok.kind),
                &tok,
            )),
        }
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<Span, Diagnostic> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Ident(s) if s == kw => {
                self.bump();
                Ok(tok.span(&self.file))
            }
            _ => Err(self.err(
                "E_PARSE_KEYWORD",
                format!("Expected keyword '{kw}', found {:?}", tok.kind),
                &tok,
            )),
        }
    }

    fn parse_quantity(&mut self) -> Result<(Quantity, Span), Diagnostic> {
        let tok = self.peek().clone();
        let neg = if tok.kind == TokenKind::Minus {
            self.bump();
            true
        } else {
            false
        };
        let num_tok = self.peek().clone();
        match num_tok.kind {
            TokenKind::Number(val, ref unit_s) => {
                self.bump();
                let unit = Unit::parse(unit_s).ok_or_else(|| {
                    Diagnostic::error(
                        "E_UNIT_UNKNOWN",
                        "units",
                        format!("Unknown unit suffix '{unit_s}' (supported: mm, cm, m, in, ft, deg, rad)"),
                        Some(num_tok.span(&self.file)),
                    )
                })?;
                let signed = if neg { -val } else { val };
                let sp = if neg {
                    tok.span(&self.file).merge(&num_tok.span(&self.file))
                } else {
                    num_tok.span(&self.file)
                };
                Ok((Quantity::new(signed, unit), sp))
            }
            _ => Err(self.err(
                "E_PARSE_QUANTITY",
                format!("Expected numeric quantity (e.g. 4500mm), found {:?}", num_tok.kind),
                &num_tok,
            )),
        }
    }

    fn parse_project(&mut self) -> Result<ProjectAst, Diagnostic> {
        let start = self.expect_keyword("project")?;
        let name_tok = self.peek().clone();
        let name = match name_tok.kind {
            TokenKind::Str(s) => {
                self.bump();
                s
            }
            TokenKind::Ident(s) => {
                self.bump();
                s
            }
            _ => {
                return Err(self.err(
                    "E_PARSE_PROJECT_NAME",
                    "Expected quoted project name string",
                    &name_tok,
                ))
            }
        };
        self.expect(TokenKind::LBrace, "'{'")?;

        let mut default_unit = Unit::Mm;
        let mut frames = Vec::new();
        let mut parameters = Vec::new();
        let mut arch_defaults = ArchDefaults::default();
        let mut rooms = Vec::new();
        let mut layouts = Vec::new();
        let mut openings = Vec::new();
        let mut fixtures = Vec::new();
        let mut verifications = Vec::new();
        let mut views = Vec::new();

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let tok = self.peek().clone();
            let kw = match &tok.kind {
                TokenKind::Ident(s) => s.as_str(),
                _ => {
                    return Err(self.err(
                        "E_PARSE_DECL",
                        format!("Expected declaration inside project, found {:?}", tok.kind),
                        &tok,
                    ))
                }
            };
            match kw {
                "units" => {
                    self.bump();
                    self.expect(TokenKind::Colon, "':'")?;
                    let (u_str, u_span) = self.expect_ident("unit name (mm, cm, m, in, ft)")?;
                    default_unit = Unit::parse(&u_str).ok_or_else(|| {
                        Diagnostic::error(
                            "E_UNIT_UNKNOWN",
                            "units",
                            format!("Unknown project unit '{u_str}'"),
                            Some(u_span),
                        )
                    })?;
                }
                "frame" => frames.push(self.parse_frame()?),
                "parameter" => parameters.push(self.parse_param()?),
                "architecture" => arch_defaults = self.parse_arch_defaults()?,
                "room" => rooms.push(self.parse_room()?),
                "layout" => layouts.push(self.parse_layout()?),
                "opening" => openings.push(self.parse_opening()?),
                "fixture" => fixtures.push(self.parse_fixture()?),
                "verify" => verifications.extend(self.parse_verify()?),
                "view" => views.push(self.parse_view()?),
                other => {
                    return Err(self.err(
                        "E_PARSE_UNKNOWN_DECL",
                        format!("Unknown top-level declaration '{other}' inside project"),
                        &tok,
                    ))
                }
            }
        }

        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        if self.peek().kind != TokenKind::Eof {
            let extra = self.peek().clone();
            return Err(self.err(
                "E_PARSE_TRAILING",
                "Unexpected tokens after project closing '}'",
                &extra,
            ));
        }

        Ok(ProjectAst {
            name,
            span: start.merge(&end),
            default_unit,
            frames,
            parameters,
            arch_defaults,
            rooms,
            layouts,
            openings,
            fixtures,
            verifications,
            views,
        })
    }

    fn parse_param(&mut self) -> Result<ParamDecl, Diagnostic> {
        let start = self.expect_keyword("parameter")?;
        let (name, _) = self.expect_ident("parameter name")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (value, end) = self.parse_quantity()?;
        Ok(ParamDecl {
            name,
            value,
            span: start.merge(&end),
        })
    }

    fn parse_frame(&mut self) -> Result<FrameDecl, Diagnostic> {
        let start = self.expect_keyword("frame")?;
        let (name, _) = self.expect_ident("frame name")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (kind, end) = self.expect_ident("frame type (e.g. Frame2)")?;
        Ok(FrameDecl {
            name,
            kind,
            span: start.merge(&end),
        })
    }

    fn parse_arch_defaults(&mut self) -> Result<ArchDefaults, Diagnostic> {
        let start = self.expect_keyword("architecture")?;
        self.expect(TokenKind::Dot, "'.'")?;
        self.expect_keyword("defaults")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut defaults = ArchDefaults::default();
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (key, kspan) = self.expect_ident("architecture default property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match key.as_str() {
                "measurement_basis" => {
                    let (val, _) = self.expect_ident("measurement basis")?;
                    defaults.measurement_basis = val;
                }
                "exterior_wall" => {
                    let (q, _) = self.parse_quantity()?;
                    defaults.exterior_wall = q;
                }
                "interior_wall" => {
                    let (q, _) = self.parse_quantity()?;
                    defaults.interior_wall = q;
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown architecture.defaults property '{other}'"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        defaults.span = Some(start.merge(&end));
        Ok(defaults)
    }

    fn parse_room(&mut self) -> Result<RoomDecl, Diagnostic> {
        let start = self.expect_keyword("room")?;
        let (id, _) = self.expect_ident("room identifier")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut role = None;
        let mut clear_size = None;
        let mut anchor = None;

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (key, kspan) = self.expect_ident("room property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match key.as_str() {
                "role" => {
                    let (r_s, rspan) = self.expect_ident("room role (habitable/circulation/wet/service/outdoor)")?;
                    role = Some(match r_s.as_str() {
                        "habitable" => RoomRole::Habitable,
                        "circulation" => RoomRole::Circulation,
                        "wet" => RoomRole::Wet,
                        "service" => RoomRole::Service,
                        "outdoor" => RoomRole::Outdoor,
                        _ => {
                            return Err(Diagnostic::error(
                                "E_PARSE_ROLE",
                                "syntax",
                                format!("Unknown room role '{r_s}' (expected habitable, circulation, wet, service, or outdoor)"),
                                Some(rspan),
                            ))
                        }
                    });
                }
                "clear_size" => {
                    self.expect(TokenKind::LParen, "'('")?;
                    let (w, _) = self.parse_quantity()?;
                    self.expect(TokenKind::Comma, "','")?;
                    let (d, _) = self.parse_quantity()?;
                    self.expect(TokenKind::RParen, "')'")?;
                    clear_size = Some((w, d));
                }
                "anchor" => {
                    let (corner_s, cspan) = self.expect_ident("corner (southwest/southeast/northwest/northeast)")?;
                    let corner = match corner_s.as_str() {
                        "southwest" => Corner::Southwest,
                        "southeast" => Corner::Southeast,
                        "northwest" => Corner::Northwest,
                        "northeast" => Corner::Northeast,
                        _ => {
                            return Err(Diagnostic::error(
                                "E_PARSE_CORNER",
                                "syntax",
                                format!("Unknown corner '{corner_s}'"),
                                Some(cspan),
                            ))
                        }
                    };
                    self.expect_keyword("at")?;
                    let (frame, _) = self.expect_ident("frame name")?;
                    self.expect(TokenKind::Dot, "'.'")?;
                    let (target, tspan) = self.expect_ident("origin")?;
                    anchor = Some(AnchorSpec {
                        corner,
                        frame,
                        target,
                        span: kspan.merge(&tspan),
                    });
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown room property '{other}'"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(RoomDecl {
            id,
            role,
            clear_size,
            anchor,
            span: start.merge(&end),
        })
    }

    fn parse_layout(&mut self) -> Result<LayoutBlock, Diagnostic> {
        let start = self.expect_keyword("layout")?;
        let (frame, _) = self.expect_ident("frame name")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut constraints = Vec::new();
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (kw, cstart) = self.expect_ident("require or prefer")?;
            let strength = match kw.as_str() {
                "require" => ConstraintStrength::Require,
                "prefer" => ConstraintStrength::Prefer,
                _ => {
                    return Err(Diagnostic::error(
                        "E_PARSE_CONSTRAINT",
                        "syntax",
                        format!("Expected 'require' or 'prefer' inside layout, found '{kw}'"),
                        Some(cstart),
                    ))
                }
            };
            let (lhs, lhs_txt, _) = self.parse_linear_expr()?;
            self.expect(TokenKind::EqEq, "'=='")?;
            let (rhs, rhs_txt, cend) = self.parse_linear_expr()?;
            constraints.push(LinearConstraintAst {
                strength,
                lhs,
                rhs,
                raw_text: format!("{kw} {lhs_txt} == {rhs_txt}"),
                span: cstart.merge(&cend),
            });
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(LayoutBlock {
            frame,
            constraints,
            span: start.merge(&end),
        })
    }

    fn parse_linear_expr(&mut self) -> Result<(LinearExprAst, String, Span), Diagnostic> {
        let (mut expr, mut text, mut span) = self.parse_linear_term()?;
        loop {
            match self.peek().kind {
                TokenKind::Plus => {
                    self.bump();
                    let (rhs, rtxt, rspan) = self.parse_linear_term()?;
                    text = format!("{text} + {rtxt}");
                    span = span.merge(&rspan);
                    expr = LinearExprAst::Add(Box::new(expr), Box::new(rhs));
                }
                TokenKind::Minus => {
                    self.bump();
                    let (rhs, rtxt, rspan) = self.parse_linear_term()?;
                    text = format!("{text} - {rtxt}");
                    span = span.merge(&rspan);
                    expr = LinearExprAst::Sub(Box::new(expr), Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok((expr, text, span))
    }

    fn parse_linear_term(&mut self) -> Result<(LinearExprAst, String, Span), Diagnostic> {
        // Optional scalar factor: `2 * Kitchen.width`
        if let TokenKind::Number(val, ref u) = self.peek().kind {
            if u.is_empty() && self.peek_next().kind == TokenKind::Star {
                let ntok = self.bump();
                self.bump(); // '*'
                let (inner, itxt, ispan) = self.parse_linear_atom()?;
                let sp = ntok.span(&self.file).merge(&ispan);
                return Ok((LinearExprAst::Scale(val, Box::new(inner)), format!("{val} * {itxt}"), sp));
            }
        }
        self.parse_linear_atom()
    }

    fn parse_linear_atom(&mut self) -> Result<(LinearExprAst, String, Span), Diagnostic> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Ident(_) => {
                let (ident, rspan) = self.expect_ident("room or parameter name")?;
                if self.peek().kind != TokenKind::Dot {
                    return Ok((
                        LinearExprAst::Param {
                            name: ident.clone(),
                            span: rspan.clone(),
                        },
                        ident,
                        rspan,
                    ));
                }
                self.bump(); // '.'
                let room = ident;
                let (prop, pspan) = self.expect_ident("edge or dimension (west/east/south/north/width/depth)")?;
                let full_span = rspan.merge(&pspan);
                let txt = format!("{room}.{prop}");
                let expr = match prop.as_str() {
                    "west" => LinearExprAst::RoomEdge { room, side: Side::West, span: full_span.clone() },
                    "east" => LinearExprAst::RoomEdge { room, side: Side::East, span: full_span.clone() },
                    "south" => LinearExprAst::RoomEdge { room, side: Side::South, span: full_span.clone() },
                    "north" => LinearExprAst::RoomEdge { room, side: Side::North, span: full_span.clone() },
                    "width" => LinearExprAst::RoomDim { room, dim: RoomDimKind::Width, span: full_span.clone() },
                    "depth" => LinearExprAst::RoomDim { room, dim: RoomDimKind::Depth, span: full_span.clone() },
                    _ => {
                        return Err(Diagnostic::error(
                            "E_PARSE_EDGE",
                            "syntax",
                            format!("Unknown room attribute '{prop}' (expected west, east, south, north, width, or depth)"),
                            Some(pspan),
                        ))
                    }
                };
                Ok((expr, txt, full_span))
            }
            TokenKind::Number(_, _) | TokenKind::Minus => {
                let (q, qspan) = self.parse_quantity()?;
                let txt = format!("{}{}", q.raw_value, q.unit);
                Ok((LinearExprAst::Quantity(q, qspan.clone()), txt, qspan))
            }
            _ => Err(self.err(
                "E_PARSE_EXPR",
                format!("Expected room reference or quantity in constraint, found {:?}", tok.kind),
                &tok,
            )),
        }
    }

    fn parse_opening(&mut self) -> Result<OpeningDecl, Diagnostic> {
        let start = self.expect_keyword("opening")?;
        let (id, _) = self.expect_ident("opening id")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (kind_s, kspan) = self.expect_ident("door, window, or passage")?;
        let kind = match kind_s.as_str() {
            "door" => OpeningKind::Door,
            "window" => OpeningKind::Window,
            "passage" => OpeningKind::Passage,
            _ => {
                return Err(Diagnostic::error(
                    "E_PARSE_OPENING_KIND",
                    "syntax",
                    format!("Unknown opening kind '{kind_s}' (expected 'door', 'window', or 'passage')"),
                    Some(kspan),
                ))
            }
        };
        self.expect(TokenKind::LBrace, "'{'")?;

        let mut host = None;
        let mut width = None;
        let mut placement = None;
        let mut hinge = None;
        let mut swing_into = None;

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (key, pspan) = self.expect_ident("opening property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match key.as_str() {
                "host" => {
                    let (fn_name, fspan) = self.expect_ident("exterior or shared_wall")?;
                    self.expect(TokenKind::LParen, "'('")?;
                    match fn_name.as_str() {
                        "exterior" => {
                            let (room, _) = self.expect_ident("room id")?;
                            self.expect(TokenKind::Comma, "','")?;
                            let (side_s, sspan) = self.expect_ident("side (north/south/east/west)")?;
                            let side = parse_side(&side_s, sspan)?;
                            self.expect(TokenKind::RParen, "')'")?;
                            host = Some(WallHost::Exterior { room, side });
                        }
                        "shared_wall" => {
                            let (room_a, _) = self.expect_ident("first room id")?;
                            self.expect(TokenKind::Comma, "','")?;
                            let (room_b, _) = self.expect_ident("second room id")?;
                            self.expect(TokenKind::RParen, "')'")?;
                            host = Some(WallHost::SharedWall { room_a, room_b });
                        }
                        _ => {
                            return Err(Diagnostic::error(
                                "E_PARSE_HOST",
                                "syntax",
                                format!("Unknown wall host '{fn_name}' (expected exterior or shared_wall)"),
                                Some(fspan),
                            ))
                        }
                    }
                }
                "width" => {
                    let (q, _) = self.parse_quantity()?;
                    width = Some(q);
                }
                "align" => {
                    self.expect_keyword("center")?;
                    placement = Some(OpeningPlacement::Center);
                }
                "offset" => {
                    let (dist, _) = self.parse_quantity()?;
                    self.expect_keyword("from")?;
                    let (end_s, espan) = self.expect_ident("start or end")?;
                    let from = parse_wall_end(&end_s, espan)?;
                    placement = Some(OpeningPlacement::Offset { distance: dist, from });
                }
                "hinge" => {
                    let (end_s, espan) = self.expect_ident("start or end")?;
                    hinge = Some(parse_wall_end(&end_s, espan)?);
                }
                "swing" => {
                    self.expect_keyword("into")?;
                    let (room, _) = self.expect_ident("target room id")?;
                    swing_into = Some(room);
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown opening property '{other}'"),
                        Some(pspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        let full_span = start.merge(&end);

        let host = host.ok_or_else(|| {
            Diagnostic::error(
                "E_OPENING_MISSING_HOST",
                "semantics",
                format!("Opening '{id}' is missing required 'host' property"),
                Some(full_span.clone()),
            )
        })?;
        let width = width.ok_or_else(|| {
            Diagnostic::error(
                "E_OPENING_MISSING_WIDTH",
                "semantics",
                format!("Opening '{id}' is missing required 'width' property"),
                Some(full_span.clone()),
            )
        })?;
        let placement = placement.ok_or_else(|| {
            Diagnostic::error(
                "E_OPENING_MISSING_PLACEMENT",
                "semantics",
                format!("Opening '{id}' requires either 'align: center' or 'offset: <len> from start|end'"),
                Some(full_span.clone()),
            )
        })?;

        Ok(OpeningDecl {
            id,
            kind,
            host,
            width,
            placement,
            hinge,
            swing_into,
            span: full_span,
        })
    }

    fn parse_fixture(&mut self) -> Result<FixtureDecl, Diagnostic> {
        let start = self.expect_keyword("fixture")?;
        let (id, _) = self.expect_ident("fixture id")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (category, _) = self.expect_ident("fixture category (bed/sofa/counter/table/toilet/vanity/shower/wardrobe/desk)")?;
        self.expect(TokenKind::LBrace, "'{'")?;

        let mut room = None;
        let mut size = None;
        let mut against = None;
        let mut placement = None;
        let mut freestanding_center = false;
        let mut freestanding_at = None;
        let mut clearance_front = None;
        let mut clearance_all = None;

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (key, pspan) = self.expect_ident("fixture property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match key.as_str() {
                "room" => {
                    let (r, _) = self.expect_ident("room id")?;
                    room = Some(r);
                }
                "size" => {
                    self.expect(TokenKind::LParen, "'('")?;
                    let (w, _) = self.parse_quantity()?;
                    self.expect(TokenKind::Comma, "','")?;
                    let (d, _) = self.parse_quantity()?;
                    self.expect(TokenKind::RParen, "')'")?;
                    size = Some((w, d));
                }
                "against" => {
                    let (s_str, sspan) = self.expect_ident("wall side (north/south/east/west)")?;
                    against = Some(parse_side(&s_str, sspan)?);
                }
                "at" => {
                    if let TokenKind::Ident(ref s) = self.peek().kind {
                        if s == "center" {
                            self.bump();
                            freestanding_center = true;
                        } else {
                            return Err(Diagnostic::error(
                                "E_PARSE_PROP",
                                "syntax",
                                format!("Expected 'center' or '(x, y)' after 'at:', found '{s}'"),
                                Some(pspan),
                            ));
                        }
                    } else {
                        self.expect(TokenKind::LParen, "'('")?;
                        let (x, _) = self.parse_quantity()?;
                        self.expect(TokenKind::Comma, "','")?;
                        let (y, _) = self.parse_quantity()?;
                        self.expect(TokenKind::RParen, "')'")?;
                        freestanding_at = Some((x, y));
                    }
                }
                "align" => {
                    self.expect_keyword("center")?;
                    placement = Some(OpeningPlacement::Center);
                }
                "offset" => {
                    let (dist, _) = self.parse_quantity()?;
                    self.expect_keyword("from")?;
                    let (end_s, espan) = self.expect_ident("start or end")?;
                    let from = parse_wall_end(&end_s, espan)?;
                    placement = Some(OpeningPlacement::Offset { distance: dist, from });
                }
                "clearance_front" => {
                    let (q, _) = self.parse_quantity()?;
                    clearance_front = Some(q);
                }
                "clearance_all" => {
                    let (q, _) = self.parse_quantity()?;
                    clearance_all = Some(q);
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown fixture property '{other}'"),
                        Some(pspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        let full_span = start.merge(&end);

        let room = room.ok_or_else(|| {
            Diagnostic::error(
                "E_FIXTURE_MISSING_ROOM",
                "semantics",
                format!("Fixture '{id}' is missing required 'room' property"),
                Some(full_span.clone()),
            )
        })?;
        let size = size.ok_or_else(|| {
            Diagnostic::error(
                "E_FIXTURE_MISSING_SIZE",
                "semantics",
                format!("Fixture '{id}' is missing required 'size: (width, depth)' property"),
                Some(full_span.clone()),
            )
        })?;
        let position = if let Some(side) = against {
            FixturePos::AgainstWall {
                side,
                placement: placement.unwrap_or(OpeningPlacement::Center),
            }
        } else if let Some((x, y)) = freestanding_at {
            FixturePos::FreestandingAt { x, y }
        } else if freestanding_center || matches!(placement, Some(OpeningPlacement::Center)) {
            FixturePos::FreestandingCenter
        } else {
            return Err(Diagnostic::error(
                "E_FIXTURE_MISSING_POS",
                "semantics",
                format!("Fixture '{id}' requires either 'against: north|south|east|west' or 'at: center' / 'at: (x, y)'"),
                Some(full_span.clone()),
            ));
        };

        Ok(FixtureDecl {
            id,
            category,
            room,
            size,
            position,
            clearance_front,
            clearance_all,
            span: full_span,
        })
    }

    fn parse_verify(&mut self) -> Result<Vec<VerifyRule>, Diagnostic> {
        self.expect_keyword("verify")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut rules = Vec::new();
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let rstart = self.expect_keyword("require")?;
            let (rule_name, rspan) = self.expect_ident("verification rule name")?;
            match rule_name.as_str() {
                "no_room_overlap" => rules.push(VerifyRule::NoRoomOverlap {
                    span: rstart.merge(&rspan),
                }),
                "opening_jamb_margin" => {
                    self.expect(TokenKind::Ge, "'>='")?;
                    let (q, qspan) = self.parse_quantity()?;
                    rules.push(VerifyRule::OpeningJambMarginMin {
                        min: q,
                        span: rstart.merge(&qspan),
                    });
                }
                "no_door_swing_collision" => rules.push(VerifyRule::NoDoorSwingCollision {
                    span: rstart.merge(&rspan),
                }),
                "all_rooms_reachable" => rules.push(VerifyRule::AllRoomsReachable {
                    span: rstart.merge(&rspan),
                }),
                "no_fixture_collision" => rules.push(VerifyRule::NoFixtureCollision {
                    span: rstart.merge(&rspan),
                }),
                "habitable_rooms_have_window" => rules.push(VerifyRule::HabitableRoomsHaveWindow {
                    span: rstart.merge(&rspan),
                }),
                other => {
                    return Err(Diagnostic::error(
                        "E_VERIFY_UNKNOWN",
                        "semantics",
                        format!("Unknown verification rule '{other}'"),
                        Some(rspan),
                    ))
                }
            }
        }
        self.expect(TokenKind::RBrace, "'}'")?;
        Ok(rules)
    }

    fn parse_view(&mut self) -> Result<ViewDecl, Diagnostic> {
        let start = self.expect_keyword("view")?;
        let (id, _) = self.expect_ident("view id")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (kind, _) = self.expect_ident("view type (e.g. PlanView)")?;
        self.expect(TokenKind::LBrace, "'{'")?;

        let mut frame = String::new();
        let mut sheet = "A3 landscape".to_string();
        let mut scale = "1:50".to_string();
        let mut show_dimensions = true;
        let mut show_labels = true;

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (key, kspan) = self.expect_ident("view property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match key.as_str() {
                "frame" => {
                    let (f, _) = self.expect_ident("frame name")?;
                    frame = f;
                }
                "sheet" => {
                    let (size, _) = self.expect_ident("sheet size (e.g. A3)")?;
                    let orient = if let TokenKind::Ident(s) = &self.peek().kind {
                        if s == "landscape" || s == "portrait" {
                            let o = s.clone();
                            self.bump();
                            format!(" {o}")
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    };
                    sheet = format!("{size}{orient}");
                }
                "scale" => {
                    let (a, _) = self.parse_quantity()?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let (b, _) = self.parse_quantity()?;
                    scale = format!("{}:{}", a.raw_value as i64, b.raw_value as i64);
                }
                "show_dimensions" => {
                    let (b, _) = self.expect_ident("true or false")?;
                    show_dimensions = b == "true";
                }
                "show_labels" => {
                    let (b, _) = self.expect_ident("true or false")?;
                    show_labels = b == "true";
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown view property '{other}'"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(ViewDecl {
            id,
            kind,
            frame,
            sheet,
            scale,
            show_dimensions,
            show_labels,
            span: start.merge(&end),
        })
    }
}

fn parse_side(s: &str, span: Span) -> Result<Side, Diagnostic> {
    match s {
        "north" => Ok(Side::North),
        "south" => Ok(Side::South),
        "east" => Ok(Side::East),
        "west" => Ok(Side::West),
        _ => Err(Diagnostic::error(
            "E_PARSE_SIDE",
            "syntax",
            format!("Unknown side '{s}' (expected north, south, east, west)"),
            Some(span),
        )),
    }
}

fn parse_wall_end(s: &str, span: Span) -> Result<WallEnd, Diagnostic> {
    match s {
        "start" => Ok(WallEnd::Start),
        "end" => Ok(WallEnd::End),
        _ => Err(Diagnostic::error(
            "E_PARSE_END",
            "syntax",
            format!("Unknown wall endpoint '{s}' (expected start or end)"),
            Some(span),
        )),
    }
}
