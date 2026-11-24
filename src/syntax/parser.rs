use crate::core::geom::Side;
use crate::core::ir::ConstraintStrength;
use crate::core::span::{Diagnostic, Span};
use crate::core::units::{Quantity, Unit, VoltageSpec};
use crate::domains::arch::ast::*;
use crate::domains::elec::ast::*;
use crate::syntax::ast::*;
use crate::syntax::lexer::{lex, Token, TokenKind};
use std::collections::BTreeMap;

pub fn parse_file(file: &str, src: &str) -> Result<FileAst, Diagnostic> {
    let tokens = lex(file, src)?;
    let mut p = Parser {
        file: file.to_string(),
        tokens,
        pos: 0,
        param_env: BTreeMap::new(),
    };
    p.parse_file()
}

pub fn parse_project(file: &str, src: &str) -> Result<ProjectAst, Diagnostic> {
    let f = parse_file(file, src)?;
    f.project.ok_or_else(|| {
        Diagnostic::error(
            "E_PARSE_NO_PROJECT",
            "syntax",
            format!("File '{file}' contains no 'project' declaration"),
            None,
        )
    })
}

pub fn parse_module_body(
    module: &ModuleDecl,
    param_env: BTreeMap<String, ArgValue>,
) -> Result<ModuleBody, Diagnostic> {
    let mut tokens = module.body_tokens.clone();
    tokens.push(Token {
        kind: TokenKind::Eof,
        line: module.span.end_line,
        col: module.span.end_col,
        end_line: module.span.end_line,
        end_col: module.span.end_col,
    });
    let mut p = Parser {
        file: module.file.clone(),
        tokens,
        pos: 0,
        param_env,
    };
    p.parse_module_body_inner()
}

struct Parser {
    file: String,
    tokens: Vec<Token>,
    pos: usize,
    param_env: BTreeMap<String, ArgValue>,
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

    fn expect_string(&mut self, label: &str) -> Result<(String, Span), Diagnostic> {
        let tok = self.peek().clone();
        let sp = tok.span(&self.file);
        match tok.kind {
            TokenKind::Str(s) => {
                self.bump();
                Ok((s, sp))
            }
            TokenKind::Ident(ref name) => {
                if let Some(ArgValue::String(val)) = self.param_env.get(name) {
                    let v = val.clone();
                    self.bump();
                    return Ok((v, sp));
                }
                Err(self.err(
                    "E_PARSE_STRING",
                    format!("Expected {label} string literal, found identifier '{name}'"),
                    &tok,
                ))
            }
            _ => Err(self.err(
                "E_PARSE_STRING",
                format!("Expected {label} string literal, found {:?}", tok.kind),
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
        let sp = num_tok.span(&self.file);
        match num_tok.kind {
            TokenKind::Number(val, ref unit_s) => {
                self.bump();
                let unit = Unit::parse(unit_s).ok_or_else(|| {
                    Diagnostic::error(
                        "E_UNIT_UNKNOWN",
                        "units",
                        format!("Unknown unit suffix '{unit_s}'"),
                        Some(sp.clone()),
                    )
                })?;
                let signed = if neg { -val } else { val };
                let full = if neg {
                    tok.span(&self.file).merge(&sp)
                } else {
                    sp
                };
                Ok((Quantity::new(signed, unit), full))
            }
            TokenKind::Ident(ref name) => {
                if let Some(ArgValue::Quantity(q)) = self.param_env.get(name) {
                    let mut qv = *q;
                    if neg {
                        qv.raw_value = -qv.raw_value;
                    }
                    self.bump();
                    return Ok((qv, sp));
                }
                Err(self.err(
                    "E_PARSE_QUANTITY",
                    format!("Expected numeric quantity, found identifier '{name}'"),
                    &num_tok,
                ))
            }
            _ => Err(self.err(
                "E_PARSE_QUANTITY",
                format!("Expected numeric quantity (e.g. 4500mm), found {:?}", num_tok.kind),
                &num_tok,
            )),
        }
    }

    fn parse_arg_value(&mut self) -> Result<(ArgValue, Span), Diagnostic> {
        let tok = self.peek().clone();
        let sp = tok.span(&self.file);
        match tok.kind {
            TokenKind::Str(s) => {
                self.bump();
                Ok((ArgValue::String(s), sp))
            }
            TokenKind::Number(_, _) | TokenKind::Minus => {
                let (q, qsp) = self.parse_quantity()?;
                Ok((ArgValue::Quantity(q), qsp))
            }
            TokenKind::Ident(s) => {
                self.bump();
                Ok((ArgValue::Ident(s), sp))
            }
            _ => Err(self.err(
                "E_PARSE_ARG",
                format!("Expected argument value (quantity, string, or identifier), found {:?}", tok.kind),
                &tok,
            )),
        }
    }

    fn parse_file(&mut self) -> Result<FileAst, Diagnostic> {
        let mut imports = Vec::new();
        let mut modules = Vec::new();
        let mut project = None;

        while self.peek().kind != TokenKind::Eof {
            let tok = self.peek().clone();
            let kw = match &tok.kind {
                TokenKind::Ident(s) => s.as_str(),
                _ => {
                    return Err(self.err(
                        "E_PARSE_TOPLEVEL",
                        format!("Expected 'import', 'module', or 'project', found {:?}", tok.kind),
                        &tok,
                    ))
                }
            };
            match kw {
                "import" => imports.push(self.parse_import()?),
                "module" => modules.push(self.parse_module()?),
                "project" => {
                    let mut proj = self.parse_project()?;
                    // Merge file-level imports & modules into project
                    let mut combined_imports = imports.clone();
                    combined_imports.extend(proj.imports);
                    proj.imports = combined_imports;
                    let mut combined_mods = modules.clone();
                    combined_mods.extend(proj.modules);
                    proj.modules = combined_mods;
                    project = Some(proj);
                }
                other => {
                    return Err(self.err(
                        "E_PARSE_TOPLEVEL",
                        format!("Unexpected top-level keyword '{other}' (expected import, module, or project)"),
                        &tok,
                    ))
                }
            }
        }

        Ok(FileAst {
            imports,
            modules,
            project,
        })
    }

    fn parse_import(&mut self) -> Result<ImportDecl, Diagnostic> {
        let start = self.expect_keyword("import")?;
        let (path, end) = self.expect_string("import path")?;
        Ok(ImportDecl {
            path,
            span: start.merge(&end),
        })
    }

    fn parse_module(&mut self) -> Result<ModuleDecl, Diagnostic> {
        let start = self.expect_keyword("module")?;
        let (name, _) = self.expect_ident("module name")?;
        let mut params = Vec::new();
        if self.peek().kind == TokenKind::LParen {
            self.bump();
            while self.peek().kind != TokenKind::RParen && self.peek().kind != TokenKind::Eof {
                let (pname, pstart) = self.expect_ident("module parameter name")?;
                // Optional type annotation `: length | resistance | string | ...` (ignored at syntax level)
                if self.peek().kind == TokenKind::Colon {
                    self.bump();
                    let _ = self.expect_ident("parameter type")?;
                }
                let mut default = None;
                let mut pend = pstart.clone();
                if self.peek().kind == TokenKind::Eq {
                    self.bump();
                    let (v, vsp) = self.parse_arg_value()?;
                    default = Some(v);
                    pend = vsp;
                }
                params.push(ModuleParamSpec {
                    name: pname,
                    default,
                    span: pstart.merge(&pend),
                });
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "')'")?;
        }
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut body_tokens = Vec::new();
        let mut depth = 1usize;
        let mut end_span = start.clone();
        while self.peek().kind != TokenKind::Eof {
            let tok = self.bump();
            if tok.kind == TokenKind::LBrace {
                depth += 1;
                body_tokens.push(tok);
            } else if tok.kind == TokenKind::RBrace {
                depth -= 1;
                if depth == 0 {
                    end_span = tok.span(&self.file);
                    break;
                }
                body_tokens.push(tok);
            } else {
                body_tokens.push(tok);
            }
        }
        if depth != 0 {
            return Err(Diagnostic::error(
                "E_PARSE_UNCLOSED_MODULE",
                "syntax",
                format!("Unclosed module '{name}' body"),
                Some(start),
            ));
        }

        let mdecl = ModuleDecl {
            name,
            file: self.file.clone(),
            params,
            body_tokens,
            span: start.merge(&end_span),
        };

        // Eager syntax check when all params have defaults
        if mdecl.params.iter().all(|p| p.default.is_some()) {
            let mut env = BTreeMap::new();
            for p in &mdecl.params {
                if let Some(ref d) = p.default {
                    env.insert(p.name.clone(), d.clone());
                }
            }
            let _ = parse_module_body(&mdecl, env)?;
        }

        Ok(mdecl)
    }

    fn parse_module_body_inner(&mut self) -> Result<ModuleBody, Diagnostic> {
        let mut body = ModuleBody::default();
        while self.peek().kind != TokenKind::Eof {
            let tok = self.peek().clone();
            let kw = match &tok.kind {
                TokenKind::Ident(s) => s.as_str(),
                _ => {
                    return Err(self.err(
                        "E_PARSE_MODULE_BODY",
                        format!("Unexpected token inside module: {:?}", tok.kind),
                        &tok,
                    ))
                }
            };
            match kw {
                "component" => body.components.push(self.parse_component()?),
                "net" => body.nets.push(self.parse_net()?),
                "bind" => body.port_binds.push(self.parse_port_bind()?),
                "room" => body.arch.rooms.push(self.parse_room()?),
                "layout" => body.arch.layouts.push(self.parse_layout()?),
                "opening" => body.arch.openings.push(self.parse_opening()?),
                "fixture" => body.arch.fixtures.push(self.parse_fixture()?),
                other => {
                    return Err(self.err(
                        "E_PARSE_MODULE_BODY",
                        format!("Unknown declaration '{other}' inside module"),
                        &tok,
                    ))
                }
            }
        }
        Ok(body)
    }

    fn parse_port_bind(&mut self) -> Result<PortBindDecl, Diagnostic> {
        let start = self.expect_keyword("bind")?;
        let (port, _) = self.expect_ident("module port name")?;
        self.expect(TokenKind::Arrow, "'->'")?;
        let (comp, cspan) = self.expect_ident("internal component id")?;
        self.expect(TokenKind::Dot, "'.'")?;
        let (pin, pspan) = self.expect_ident("internal pin name")?;
        Ok(PortBindDecl {
            port,
            target: PinRef {
                component: comp,
                pin,
                span: cspan.merge(&pspan),
            },
            span: start.merge(&pspan),
        })
    }

    fn parse_use(&mut self) -> Result<UseDecl, Diagnostic> {
        let start = self.expect_keyword("use")?;
        let (instance, _) = self.expect_ident("instance name")?;
        self.expect(TokenKind::Eq, "'='")?;
        let (module, mspan) = self.expect_ident("module name")?;
        let mut args = Vec::new();
        let mut end = mspan;
        if self.peek().kind == TokenKind::LParen {
            self.bump();
            while self.peek().kind != TokenKind::RParen && self.peek().kind != TokenKind::Eof {
                let (k, ksp) = self.expect_ident("argument name")?;
                self.expect(TokenKind::Colon, "':'")?;
                let (v, vsp) = self.parse_arg_value()?;
                args.push((k, v, ksp.merge(&vsp)));
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
            end = self.expect(TokenKind::RParen, "')'")?.span(&self.file);
        }
        Ok(UseDecl {
            instance,
            module,
            args,
            span: start.merge(&end),
        })
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
        let mut imports = Vec::new();
        let mut modules = Vec::new();
        let mut uses = Vec::new();
        let mut frames = Vec::new();
        let mut parameters = Vec::new();
        let mut arch = ArchProjectData::default();
        let mut elec = ElecProjectData::default();
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
                "import" => imports.push(self.parse_import()?),
                "module" => modules.push(self.parse_module()?),
                "use" => uses.push(self.parse_use()?),
                "units" => {
                    self.bump();
                    self.expect(TokenKind::Colon, "':'")?;
                    let (u_str, u_span) = self.expect_ident("unit name")?;
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
                "architecture" => arch.defaults = self.parse_arch_defaults()?,
                "room" => arch.rooms.push(self.parse_room()?),
                "layout" => arch.layouts.push(self.parse_layout()?),
                "opening" => arch.openings.push(self.parse_opening()?),
                "fixture" => arch.fixtures.push(self.parse_fixture()?),
                "circuit" => elec.circuits.push(self.parse_circuit()?),
                "schematic" => elec.schematics.push(self.parse_schematic()?),
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
        Ok(ProjectAst {
            name,
            span: start.merge(&end),
            default_unit,
            imports,
            modules,
            uses,
            frames,
            parameters,
            arch,
            elec,
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
                    let (r_s, rspan) = self.expect_ident("room role")?;
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
                                format!("Unknown room role '{r_s}'"),
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
                    let (corner_s, cspan) = self.expect_ident("corner")?;
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
            provenance: None,
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
        if let TokenKind::Number(val, ref u) = self.peek().kind {
            if u.is_empty() && self.peek_next().kind == TokenKind::Star {
                let ntok = self.bump();
                self.bump();
                let (inner, itxt, ispan) = self.parse_linear_atom()?;
                let sp = ntok.span(&self.file).merge(&ispan);
                return Ok((
                    LinearExprAst::Scale(val, Box::new(inner)),
                    format!("{val} * {itxt}"),
                    sp,
                ));
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
                    if let Some(ArgValue::Quantity(q)) = self.param_env.get(&ident) {
                        let txt = format!("{}{}", q.raw_value, q.unit);
                        return Ok((LinearExprAst::Quantity(*q, rspan.clone()), txt, rspan));
                    }
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
                let mut room = ident;
                let (mut prop, mut pspan) = self.expect_ident("edge, sub-room, or dimension")?;
                // Support qualified module room reference: `Inst.SubRoom.west`!
                if self.peek().kind == TokenKind::Dot {
                    self.bump();
                    room = format!("{room}_{prop}");
                    let (p2, sp2) = self.expect_ident("edge or dimension")?;
                    prop = p2;
                    pspan = sp2;
                }
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
                            format!("Unknown room attribute '{prop}'"),
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

    fn parse_room_ref(&mut self) -> Result<String, Diagnostic> {
        let (a, _) = self.expect_ident("room id")?;
        if self.peek().kind == TokenKind::Dot {
            self.bump();
            let (b, _) = self.expect_ident("module sub-room id")?;
            Ok(format!("{a}_{b}"))
        } else {
            Ok(a)
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
                    format!("Unknown opening kind '{kind_s}'"),
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
                            let room = self.parse_room_ref()?;
                            self.expect(TokenKind::Comma, "','")?;
                            let (side_s, sspan) = self.expect_ident("side")?;
                            let side = parse_side(&side_s, sspan)?;
                            self.expect(TokenKind::RParen, "')'")?;
                            host = Some(WallHost::Exterior { room, side });
                        }
                        "shared_wall" => {
                            let room_a = self.parse_room_ref()?;
                            self.expect(TokenKind::Comma, "','")?;
                            let room_b = self.parse_room_ref()?;
                            self.expect(TokenKind::RParen, "')'")?;
                            host = Some(WallHost::SharedWall { room_a, room_b });
                        }
                        _ => {
                            return Err(Diagnostic::error(
                                "E_PARSE_HOST",
                                "syntax",
                                format!("Unknown wall host '{fn_name}'"),
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
                    let room = self.parse_room_ref()?;
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
            provenance: None,
            span: full_span,
        })
    }

    fn parse_fixture(&mut self) -> Result<FixtureDecl, Diagnostic> {
        let start = self.expect_keyword("fixture")?;
        let (id, _) = self.expect_ident("fixture id")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (category, _) = self.expect_ident("fixture category")?;
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
                    room = Some(self.parse_room_ref()?);
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
                    let (s_str, sspan) = self.expect_ident("wall side")?;
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
                format!("Fixture '{id}' is missing required 'size' property"),
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
                format!("Fixture '{id}' requires 'against:' or 'at:'"),
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
            provenance: None,
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
                "no_room_overlap" => rules.push(VerifyRule::NoRoomOverlap { span: rstart.merge(&rspan) }),
                "opening_jamb_margin" => {
                    self.expect(TokenKind::Ge, "'>='")?;
                    let (q, qspan) = self.parse_quantity()?;
                    rules.push(VerifyRule::OpeningJambMarginMin {
                        min: q,
                        span: rstart.merge(&qspan),
                    });
                }
                "no_door_swing_collision" => rules.push(VerifyRule::NoDoorSwingCollision { span: rstart.merge(&rspan) }),
                "all_rooms_reachable" => rules.push(VerifyRule::AllRoomsReachable { span: rstart.merge(&rspan) }),
                "no_fixture_collision" => rules.push(VerifyRule::NoFixtureCollision { span: rstart.merge(&rspan) }),
                "habitable_rooms_have_window" => rules.push(VerifyRule::HabitableRoomsHaveWindow { span: rstart.merge(&rspan) }),
                "all_required_pins_connected" => rules.push(VerifyRule::AllRequiredPinsConnected { span: rstart.merge(&rspan) }),
                "no_power_ground_short" => rules.push(VerifyRule::NoPowerGroundShort { span: rstart.merge(&rspan) }),
                "no_multiple_power_drivers" => rules.push(VerifyRule::NoMultiplePowerDrivers { span: rstart.merge(&rspan) }),
                "voltage_compatibility" => rules.push(VerifyRule::VoltageCompatibility { span: rstart.merge(&rspan) }),
                "no_single_pin_nets" => rules.push(VerifyRule::NoSinglePinNets { span: rstart.merge(&rspan) }),
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
        let (kind, _) = self.expect_ident("view type")?;
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
                    let (size, _) = self.expect_ident("sheet size")?;
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

    fn parse_circuit(&mut self) -> Result<CircuitDecl, Diagnostic> {
        let start = self.expect_keyword("circuit")?;
        let (name, _) = self.expect_ident("circuit name")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut components = Vec::new();
        let mut nets = Vec::new();
        let mut uses = Vec::new();

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let tok = self.peek().clone();
            let kw = match &tok.kind {
                TokenKind::Ident(s) => s.as_str(),
                _ => {
                    return Err(self.err(
                        "E_PARSE_CIRCUIT",
                        format!("Expected 'component', 'net', or 'use' inside circuit, found {:?}", tok.kind),
                        &tok,
                    ))
                }
            };
            match kw {
                "component" => components.push(self.parse_component()?),
                "net" => nets.push(self.parse_net()?),
                "use" => uses.push(self.parse_use()?),
                other => {
                    return Err(self.err(
                        "E_PARSE_CIRCUIT",
                        format!("Unknown declaration '{other}' inside circuit"),
                        &tok,
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(CircuitDecl {
            name,
            components,
            nets,
            uses,
            span: start.merge(&end),
        })
    }

    fn parse_component(&mut self) -> Result<ComponentDecl, Diagnostic> {
        let start = self.expect_keyword("component")?;
        let (id, _) = self.expect_ident("component designator")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (category, _) = self.expect_ident("component category")?;
        self.expect(TokenKind::LBrace, "'{'")?;

        let mut value = String::new();
        let mut footprint = None;
        let mut pins = Vec::new();

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (key, kspan) = self.expect_ident("component property or 'pin'")?;
            match key.as_str() {
                "value" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    match self.peek().kind.clone() {
                        TokenKind::Str(_) => {
                            let (s, _) = self.expect_string("component value")?;
                            value = s;
                        }
                        TokenKind::Ident(ref pname) if self.param_env.contains_key(pname) => {
                            let v = self.param_env[pname].clone();
                            self.bump();
                            value = match v {
                                ArgValue::String(s) => s,
                                ArgValue::Quantity(q) => format!("{}{}", q.raw_value, q.unit),
                                ArgValue::Ident(i) => i,
                            };
                        }
                        _ => {
                            let (q, _) = self.parse_quantity()?;
                            value = format!("{}{}", q.raw_value, q.unit);
                        }
                    }
                }
                "footprint" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    if let TokenKind::Ident(ref pname) = self.peek().kind {
                        if let Some(ArgValue::String(s)) = self.param_env.get(pname) {
                            footprint = Some(s.clone());
                            self.bump();
                            continue;
                        }
                    }
                    let (s, _) = self.expect_string("footprint")?;
                    footprint = Some(s);
                }
                "pin" => {
                    let (pin_name, _) = self.expect_ident("pin name")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let (role_s, rspan) = self.expect_ident("pin role")?;
                    let role = match role_s.as_str() {
                        "power_out" => PinRole::PowerOut,
                        "power_in" => PinRole::PowerIn,
                        "ground" => PinRole::Ground,
                        "input" => PinRole::Input,
                        "output" => PinRole::Output,
                        "bidir" => PinRole::Bidir,
                        "passive" => PinRole::Passive,
                        "nc" => PinRole::Nc,
                        _ => {
                            return Err(Diagnostic::error(
                                "E_PARSE_PIN_ROLE",
                                "syntax",
                                format!("Unknown pin role '{role_s}'"),
                                Some(rspan),
                            ))
                        }
                    };
                    let mut voltage = None;
                    let mut pend = rspan.clone();
                    if self.peek().kind == TokenKind::LParen {
                        self.bump();
                        let (q1, q1span) = self.parse_quantity()?;
                        let v1 = q1.to_volts().map_err(|msg| {
                            Diagnostic::error("E_UNIT_MISMATCH", "units", format!("Pin '{id}.{pin_name}': {msg}"), Some(q1span))
                        })?;
                        if self.peek().kind == TokenKind::DotDot {
                            self.bump();
                            let (q2, q2span) = self.parse_quantity()?;
                            let v2 = q2.to_volts().map_err(|msg| {
                                Diagnostic::error("E_UNIT_MISMATCH", "units", format!("Pin '{id}.{pin_name}': {msg}"), Some(q2span))
                            })?;
                            voltage = Some(VoltageSpec::Range { min_v: v1, max_v: v2 });
                        } else {
                            voltage = Some(VoltageSpec::Exact { volts: v1 });
                        }
                        pend = self.expect(TokenKind::RParen, "')'")?.span(&self.file);
                    }
                    let mut side = None;
                    if let TokenKind::Ident(ref s) = self.peek().kind {
                        if s == "side" {
                            self.bump();
                            self.expect(TokenKind::Colon, "':'")?;
                            let (side_s, sspan) = self.expect_ident("pin side")?;
                            pend = sspan.clone();
                            side = Some(parse_side(&side_s, sspan)?);
                        }
                    }
                    pins.push(PinDecl {
                        name: pin_name,
                        role,
                        voltage,
                        side,
                        span: kspan.merge(&pend),
                    });
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown component property '{other}'"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        if value.is_empty() {
            value = category.clone();
        }
        Ok(ComponentDecl {
            id,
            category,
            value,
            footprint,
            pins,
            provenance: None,
            span: start.merge(&end),
        })
    }

    fn parse_net(&mut self) -> Result<NetDecl, Diagnostic> {
        let start = self.expect_keyword("net")?;
        let (name, _) = self.expect_ident("net name")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut connects = Vec::new();
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (key, kspan) = self.expect_ident("connect")?;
            if key != "connect" {
                return Err(Diagnostic::error(
                    "E_PARSE_NET",
                    "syntax",
                    format!("Expected 'connect' inside net '{name}', found '{key}'"),
                    Some(kspan),
                ));
            }
            self.expect(TokenKind::Colon, "':'")?;
            self.expect(TokenKind::LBracket, "'['")?;
            while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                let (comp, cspan) = self.expect_ident("component id")?;
                self.expect(TokenKind::Dot, "'.'")?;
                let (pin, pspan) = self.expect_ident("pin name")?;
                connects.push(PinRef {
                    component: comp,
                    pin,
                    span: cspan.merge(&pspan),
                });
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RBracket, "']'")?;
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(NetDecl {
            name,
            connects,
            span: start.merge(&end),
        })
    }

    fn parse_schematic(&mut self) -> Result<SchematicDecl, Diagnostic> {
        let start = self.expect_keyword("schematic")?;
        let (name, _) = self.expect_ident("schematic name")?;
        self.expect(TokenKind::LBrace, "'{'")?;

        let mut sheet = "A4 landscape".to_string();
        let mut groups = Vec::new();
        let mut layout = None;
        let mut routing = None;
        let mut placements = Vec::new();

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (kw, kspan) = self.expect_ident("sheet, group, layout, routing, or place")?;
            match kw.as_str() {
                "sheet" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (size, _) = self.expect_ident("sheet size")?;
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
                "group" => {
                    let (gname, _) = self.expect_ident("group name")?;
                    self.expect(TokenKind::LBrace, "'{'")?;
                    self.expect_keyword("members")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    self.expect(TokenKind::LBracket, "'['")?;
                    let mut members = Vec::new();
                    while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                        let m = self.parse_room_ref()?;
                        members.push(m);
                        if self.peek().kind == TokenKind::Comma {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBracket, "']'")?;
                    let gend = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
                    groups.push(SchematicGroupDecl {
                        name: gname,
                        members,
                        span: kspan.merge(&gend),
                    });
                }
                "layout" => {
                    self.expect(TokenKind::LBrace, "'{'")?;
                    let mut arrange = Vec::new();
                    let mut direction = "left_to_right".to_string();
                    let mut group_gap = None;
                    let mut member_gap = None;
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        let (lk, lspan) = self.expect_ident("schematic layout property")?;
                        self.expect(TokenKind::Colon, "':'")?;
                        match lk.as_str() {
                            "arrange" => {
                                self.expect(TokenKind::LBracket, "'['")?;
                                while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                                    let (g, _) = self.expect_ident("group name")?;
                                    arrange.push(g);
                                    if self.peek().kind == TokenKind::Comma {
                                        self.bump();
                                    } else {
                                        break;
                                    }
                                }
                                self.expect(TokenKind::RBracket, "']'")?;
                                if let TokenKind::Ident(ref dir) = self.peek().kind {
                                    if dir == "left_to_right" || dir == "top_to_bottom" {
                                        direction = dir.clone();
                                        self.bump();
                                    }
                                }
                            }
                            "group_gap" => {
                                let (q, _) = self.parse_quantity()?;
                                group_gap = Some(q);
                            }
                            "member_gap" => {
                                let (q, _) = self.parse_quantity()?;
                                member_gap = Some(q);
                            }
                            other => {
                                return Err(Diagnostic::error(
                                    "E_PARSE_PROP",
                                    "syntax",
                                    format!("Unknown schematic layout property '{other}'"),
                                    Some(lspan),
                                ))
                            }
                        }
                    }
                    let lend = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
                    layout = Some(SchematicLayoutDecl {
                        arrange,
                        direction,
                        group_gap,
                        member_gap,
                        span: kspan.merge(&lend),
                    });
                }
                "routing" => {
                    self.expect(TokenKind::LBrace, "'{'")?;
                    let mut rdecl = SchematicRoutingDecl::default();
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        let (rk, rspan) = self.expect_ident("default, power_nets, ground_nets, or net")?;
                        match rk.as_str() {
                            "default" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (st, sspan) = self.expect_ident("routing style")?;
                                rdecl.default_style = Some(parse_routing_style(&st, sspan)?);
                            }
                            "power_nets" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (st, sspan) = self.expect_ident("routing style")?;
                                rdecl.power_style = Some(parse_routing_style(&st, sspan)?);
                            }
                            "ground_nets" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (st, sspan) = self.expect_ident("routing style")?;
                                rdecl.ground_style = Some(parse_routing_style(&st, sspan)?);
                            }
                            "net" => {
                                let (net_name, _) = self.expect_ident("net name")?;
                                self.expect(TokenKind::Colon, "':'")?;
                                let (st, sspan) = self.expect_ident("routing style")?;
                                let style = parse_routing_style(&st, sspan.clone())?;
                                rdecl.net_overrides.push((net_name, style, rspan.merge(&sspan)));
                            }
                            other => {
                                return Err(Diagnostic::error(
                                    "E_PARSE_ROUTING",
                                    "syntax",
                                    format!("Unknown routing property '{other}'"),
                                    Some(rspan),
                                ))
                            }
                        }
                    }
                    self.expect(TokenKind::RBrace, "'}'")?;
                    routing = Some(rdecl);
                }
                "place" => {
                    let comp = self.parse_room_ref()?;
                    self.expect_keyword("at")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    self.expect(TokenKind::LParen, "'('")?;
                    let (x, _) = self.parse_quantity()?;
                    self.expect(TokenKind::Comma, "','")?;
                    let (y, _) = self.parse_quantity()?;
                    let pend = self.expect(TokenKind::RParen, "')'")?.span(&self.file);
                    placements.push(ExplicitPlacementDecl {
                        component: comp,
                        x,
                        y,
                        span: kspan.merge(&pend),
                    });
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_SCHEMATIC",
                        "syntax",
                        format!("Unknown declaration '{other}' inside schematic"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(SchematicDecl {
            name,
            sheet,
            groups,
            layout,
            routing,
            placements,
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
            format!("Unknown side '{s}'"),
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
            format!("Unknown wall endpoint '{s}'"),
            Some(span),
        )),
    }
}

fn parse_routing_style(s: &str, span: Span) -> Result<NetRoutingStyle, Diagnostic> {
    match s {
        "direct" => Ok(NetRoutingStyle::Direct),
        "bus" => Ok(NetRoutingStyle::Bus),
        "rail" => Ok(NetRoutingStyle::Rail),
        "symbol" => Ok(NetRoutingStyle::Symbol),
        "label_stub" => Ok(NetRoutingStyle::LabelStub),
        _ => Err(Diagnostic::error(
            "E_PARSE_ROUTING_STYLE",
            "syntax",
            format!("Unknown routing style '{s}' (expected direct, bus, rail, symbol, or label_stub)"),
            Some(span),
        )),
    }
}
