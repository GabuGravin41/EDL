use crate::core::geom::Side;
use crate::core::ir::ConstraintStrength;
use crate::core::span::{Diagnostic, Span};
use crate::core::units::{Quantity, Unit, VoltageSpec};
use crate::domains::arch::ast::*;
use crate::domains::elec::ast::*;
use crate::domains::mep_elec::ast::*;
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
                if let Some(bound) = self.param_env.get(&s) {
                    Ok((bound.clone(), sp))
                } else {
                    Ok((ArgValue::Ident(s), sp))
                }
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
        let (pin, pspan) = match self.peek().kind.clone() {
            TokenKind::Ident(p) => {
                let sp = self.bump().span(&self.file);
                (p, sp)
            }
            TokenKind::Number(n, u) if u.is_empty() => {
                let sp = self.bump().span(&self.file);
                (format!("{}", n.round() as i64), sp)
            }
            TokenKind::Plus => {
                let sp = self.bump().span(&self.file);
                ("+".to_string(), sp)
            }
            TokenKind::Minus => {
                let sp = self.bump().span(&self.file);
                ("-".to_string(), sp)
            }
            _ => self.expect_ident("internal pin name or number")?,
        };
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
        let mut port_nets = Vec::new();
        let mut end = mspan;
        if self.peek().kind == TokenKind::LParen {
            self.bump();
            while self.peek().kind != TokenKind::RParen && self.peek().kind != TokenKind::Eof {
                let (k, ksp) = self.expect_ident("argument name")?;
                if matches!(self.peek().kind, TokenKind::Colon | TokenKind::Eq) {
                    self.bump();
                } else {
                    self.expect(TokenKind::Colon, "':' or '='")?;
                }
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
        // Optional trailing `{ PORT: NET, ... }` port-to-net wiring block
        if self.peek().kind == TokenKind::LBrace {
            self.bump();
            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                let (port_k, ksp) = self.expect_ident("port binding name")?;
                if matches!(self.peek().kind, TokenKind::Colon | TokenKind::Eq) {
                    self.bump();
                } else {
                    self.expect(TokenKind::Colon, "':' or '='")?;
                }
                let (net_v, vsp) = self.expect_ident("target net name")?;
                port_nets.push((port_k, net_v, ksp.merge(&vsp)));
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                }
            }
            end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        }
        Ok(UseDecl {
            instance,
            module,
            args,
            port_nets,
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
        let mut mep_elec = MepElecProjectData::default();
        let mut extensions = Vec::new();
        let mut verifications = Vec::new();
        let mut views = Vec::new();
        let mut constraints = Vec::new();

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let tok = self.peek().clone();
            let kw = match &tok.kind {
                TokenKind::Constraint => "constraint",
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
                "parameter" => {
                    let p = self.parse_param()?;
                    self.param_env
                        .insert(p.name.clone(), ArgValue::Quantity(p.value));
                    parameters.push(p);
                }
                "architecture" => arch.defaults = self.parse_arch_defaults()?,
                "room" => arch.rooms.push(self.parse_room()?),
                "layout" => arch.layouts.push(self.parse_layout()?),
                "opening" => arch.openings.push(self.parse_opening()?),
                "fixture" => arch.fixtures.push(self.parse_fixture()?),
                "symbol_lib" => {
                    self.bump();
                    if self.peek().kind == TokenKind::Colon {
                        self.bump();
                    }
                    let (path_s, _) = self.expect_string("symbol library path")?;
                    elec.symbol_libs.push(path_s);
                }
                "layout_lock" => {
                    self.bump();
                    if self.peek().kind == TokenKind::Colon {
                        self.bump();
                    }
                    let (path_s, _) = self.expect_string("layout lockfile path")?;
                    elec.layout_lock = Some(path_s);
                }
                "circuit" => elec.circuits.push(self.parse_circuit()?),
                "schematic" => elec.schematics.push(self.parse_schematic()?),
                "building_electrical" => mep_elec.installations.push(self.parse_building_electrical()?),
                "verify" => verifications.extend(self.parse_verify()?),
                "view" => views.push(self.parse_view()?),
                "constraint" => constraints.push(self.parse_constraint()?),
                _ => {
                    // Not a built-in keyword: capture as an extension block for a registered domain.
                    // Unclaimed keywords are reported at compile time as `E_UNKNOWN_DOMAIN_BLOCK`.
                    extensions.push(self.parse_extension_block()?);
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
            mep_elec,
            extensions,
            verifications,
            views,
            constraints,
        })
    }

    /// `keyword [header tokens...] { balanced body }` captured verbatim for a registered domain engine.
    fn parse_extension_block(&mut self) -> Result<crate::syntax::ext::ExtensionBlock, Diagnostic> {
        let kw_tok = self.bump();
        let TokenKind::Ident(keyword) = kw_tok.kind.clone() else {
            return Err(self.err("E_PARSE_DECL", "Expected declaration keyword", &kw_tok));
        };
        let mut header = Vec::new();
        while !matches!(self.peek().kind, TokenKind::LBrace | TokenKind::Eof | TokenKind::RBrace) {
            header.push(self.bump());
        }
        if self.peek().kind != TokenKind::LBrace {
            return Err(self.err(
                "E_PARSE_UNKNOWN_DECL",
                format!("Unknown top-level declaration '{keyword}' inside project (expected '{{' after it)"),
                &kw_tok,
            ));
        }
        self.bump();
        let mut body = Vec::new();
        let mut depth = 1usize;
        loop {
            let t = self.peek().clone();
            match t.kind {
                TokenKind::Eof => return Err(self.err("E_PARSE_UNTERMINATED", format!("Unterminated '{keyword}' block"), &kw_tok)),
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            body.push(self.bump());
        }
        let end = self.bump();
        Ok(crate::syntax::ext::ExtensionBlock {
            keyword,
            file: self.file.clone(),
            header,
            body,
            span: kw_tok.span(&self.file).merge(&end.span(&self.file)),
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
        let mut notch = None;

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
                "notch" => {
                    self.expect(TokenKind::LParen, "'('")?;
                    let (w, _) = self.parse_quantity()?;
                    self.expect(TokenKind::Comma, "','")?;
                    let (d, _) = self.parse_quantity()?;
                    self.expect(TokenKind::RParen, "')'")?;
                    self.expect_keyword("at")?;
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
                    notch = Some(RoomNotch {
                        width: w,
                        depth: d,
                        corner,
                        span: kspan.merge(&cspan),
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
            notch,
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
                            let side = if self.peek().kind == TokenKind::Comma {
                                self.bump();
                                let (side_s, sspan) = self.expect_ident("side (north/south/east/west)")?;
                                Some(parse_side(&side_s, sspan)?)
                            } else {
                                None
                            };
                            self.expect(TokenKind::RParen, "')'")?;
                            host = Some(WallHost::SharedWall { room_a, room_b, side });
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
                "galvanic_isolation" => {
                    self.expect(TokenKind::LParen, "'('")?;
                    let (net_a, _) = self.expect_ident("first isolated net")?;
                    self.expect(TokenKind::Comma, "','")?;
                    let (net_b, _) = self.expect_ident("second isolated net")?;
                    let qend = self.expect(TokenKind::RParen, "')'")?.span(&self.file);
                    rules.push(VerifyRule::GalvanicIsolation {
                        net_a,
                        net_b,
                        span: rstart.merge(&qend),
                    });
                }
                "ic_decoupling" => rules.push(VerifyRule::IcDecoupling { span: rstart.merge(&rspan) }),
                "i2c_pullups" => rules.push(VerifyRule::I2cPullups { span: rstart.merge(&rspan) }),
                "inductive_flyback" => rules.push(VerifyRule::InductiveFlyback { span: rstart.merge(&rspan) }),
                "bom_manufacturability" => rules.push(VerifyRule::BomManufacturability { span: rstart.merge(&rspan) }),
                "dc_voltage" => {
                    self.expect(TokenKind::LParen, "'('")?;
                    let (net, _) = self.expect_ident("net name")?;
                    self.expect(TokenKind::RParen, "')'")?;
                    self.expect_keyword("in")?;
                    let (min, _) = self.parse_quantity()?;
                    self.expect(TokenKind::DotDot, "'..'")?;
                    let (max, qend) = self.parse_quantity()?;
                    rules.push(VerifyRule::DcVoltageRange {
                        net,
                        min,
                        max,
                        span: rstart.merge(&qend),
                    });
                }
                "branch_current" | "led_current" => {
                    self.expect(TokenKind::LParen, "'('")?;
                    let comp = self.parse_room_ref()?;
                    self.expect(TokenKind::RParen, "')'")?;
                    self.expect_keyword("in")?;
                    let (min, _) = self.parse_quantity()?;
                    self.expect(TokenKind::DotDot, "'..'")?;
                    let (max, qend) = self.parse_quantity()?;
                    rules.push(VerifyRule::BranchCurrentRange {
                        component: comp,
                        min,
                        max,
                        span: rstart.merge(&qend),
                    });
                }
                "breaker_load_ok" => rules.push(VerifyRule::BreakerLoadOk { span: rstart.merge(&rspan) }),
                "voltage_drop_ok" => rules.push(VerifyRule::VoltageDropOk { span: rstart.merge(&rspan) }),
                "wet_zone_clearance" => rules.push(VerifyRule::WetZoneClearance { span: rstart.merge(&rspan) }),
                "switch_accessible" => rules.push(VerifyRule::SwitchAccessible { span: rstart.merge(&rspan) }),
                "electrical_device_clear" => rules.push(VerifyRule::ElectricalDeviceClear { span: rstart.merge(&rspan) }),
                "wet_circuits_rcd_protected" => rules.push(VerifyRule::WetCircuitsRcdProtected { span: rstart.merge(&rspan) }),
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
        let mut idioms = Vec::new();
        let mut uses = Vec::new();

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let tok = self.peek().clone();
            let kw = match &tok.kind {
                TokenKind::Ident(s) => s.as_str(),
                _ => {
                    return Err(self.err(
                        "E_PARSE_CIRCUIT",
                        format!("Expected 'component', 'net', 'use', or idiom inside circuit, found {:?}", tok.kind),
                        &tok,
                    ))
                }
            };
            match kw {
                "component" => components.push(self.parse_component()?),
                "net" => nets.push(self.parse_net()?),
                "use" => uses.push(self.parse_use()?),
                "decouple" => idioms.push(self.parse_idiom_decouple()?),
                "pullup" => idioms.push(self.parse_idiom_pull(true)?),
                "pulldown" => idioms.push(self.parse_idiom_pull(false)?),
                "divider" => idioms.push(self.parse_idiom_divider()?),
                "series" => idioms.push(self.parse_idiom_series()?),
                "rc_filter" => idioms.push(self.parse_idiom_rc_filter()?),
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
            idioms,
            uses,
            span: start.merge(&end),
        })
    }

    fn parse_pin_token(&mut self) -> Result<(String, Span), Diagnostic> {
        match self.peek().kind.clone() {
            TokenKind::Ident(p) | TokenKind::Str(p) => {
                let sp = self.bump().span(&self.file);
                Ok((p, sp))
            }
            TokenKind::Number(n, u) if u.is_empty() => {
                let sp = self.bump().span(&self.file);
                Ok((format!("{}", n.round() as i64), sp))
            }
            TokenKind::Plus => {
                let sp = self.bump().span(&self.file);
                Ok(("+".to_string(), sp))
            }
            TokenKind::Minus => {
                let sp = self.bump().span(&self.file);
                Ok(("-".to_string(), sp))
            }
            _ => self.expect_ident_or_str("pin name or number"),
        }
    }

    fn parse_pin_ref(&mut self) -> Result<PinRef, Diagnostic> {
        let (comp, cspan) = self.expect_ident_or_str("component id")?;
        self.expect(TokenKind::Dot, "'.'")?;
        let (pin, pspan) = self.parse_pin_token()?;
        Ok(PinRef {
            component: comp,
            pin,
            span: cspan.merge(&pspan),
        })
    }

    fn parse_anchor_ref(&mut self) -> Result<(AnchorRef, Span), Diagnostic> {
        let (comp, cspan) = self.expect_ident_or_str("anchor component id")?;
        if self.peek().kind == TokenKind::Dot {
            self.bump();
            let (pin, pspan) = self.parse_pin_token()?;
            Ok((AnchorRef { component: comp, pin: Some(pin) }, cspan.merge(&pspan)))
        } else {
            Ok((AnchorRef { component: comp, pin: None }, cspan))
        }
    }

    fn parse_chain_endpoint(&mut self) -> Result<(ChainEndpoint, Span), Diagnostic> {
        let (first, sp1) = self.expect_ident_or_str("net name or component.pin")?;
        if self.peek().kind == TokenKind::Dot {
            self.bump();
            let (pin, sp2) = self.parse_pin_token()?;
            let sp = sp1.merge(&sp2);
            Ok((
                ChainEndpoint::Pin(PinRef {
                    component: first,
                    pin,
                    span: sp.clone(),
                }),
                sp,
            ))
        } else {
            Ok((ChainEndpoint::Net(first), sp1))
        }
    }

    fn parse_idiom_value(&mut self) -> Result<(String, Span), Diagnostic> {
        match self.peek().kind.clone() {
            TokenKind::Str(s) => {
                let sp = self.bump().span(&self.file);
                Ok((s, sp))
            }
            TokenKind::Number(n, u) => {
                let sp = self.bump().span(&self.file);
                let ns = if (n - n.round()).abs() < 1e-9 {
                    format!("{}", n.round() as i64)
                } else {
                    format!("{n}")
                };
                Ok((format!("{ns}{u}"), sp))
            }
            TokenKind::Ident(s) => {
                let sp = self.bump().span(&self.file);
                Ok((s, sp))
            }
            _ => self.expect_string("component value"),
        }
    }

    fn parse_ref_val_pair(&mut self) -> Result<((String, String), Span), Diagnostic> {
        let (rid, sp1) = self.expect_ident_or_str("component designator")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (val, sp2) = self.parse_idiom_value()?;
        Ok(((rid, val), sp1.merge(&sp2)))
    }

    fn parse_idiom_decouple(&mut self) -> Result<CircuitIdiomDecl, Diagnostic> {
        let start = self.expect_keyword("decouple")?;
        let target = self.parse_pin_ref()?;
        self.expect_keyword("with")?;
        let mut caps = Vec::new();
        let mut end = if self.peek().kind == TokenKind::LBracket {
            self.bump();
            while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                let (pair, _) = self.parse_ref_val_pair()?;
                caps.push(pair);
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RBracket, "']'")?.span(&self.file)
        } else {
            let (pair, sp) = self.parse_ref_val_pair()?;
            caps.push(pair);
            sp
        };
        let mut ground = "GND".to_string();
        let mut footprint = None;
        loop {
            match &self.peek().kind {
                TokenKind::Ident(s) if s == "to" => {
                    self.bump();
                    if self.peek().kind == TokenKind::Colon {
                        self.bump();
                    }
                    let (g, sp) = self.expect_ident_or_str("ground net")?;
                    ground = g;
                    end = sp;
                }
                TokenKind::Ident(s) if s == "footprint" => {
                    self.bump();
                    self.expect(TokenKind::Colon, "':'")?;
                    let (fp, sp) = self.expect_string("footprint")?;
                    footprint = Some(fp);
                    end = sp;
                }
                _ => break,
            }
        }
        Ok(CircuitIdiomDecl::Decouple {
            target,
            caps,
            ground,
            footprint,
            span: start.merge(&end),
        })
    }

    fn parse_idiom_pull(&mut self, is_up: bool) -> Result<CircuitIdiomDecl, Diagnostic> {
        let start = self.expect_keyword(if is_up { "pullup" } else { "pulldown" })?;
        let target = self.parse_pin_ref()?;
        self.expect_keyword("with")?;
        let ((ref_id, value), mut end) = self.parse_ref_val_pair()?;
        let mut rail = if is_up { "V3V3".to_string() } else { "GND".to_string() };
        let mut footprint = None;
        loop {
            match &self.peek().kind {
                TokenKind::Ident(s) if s == "to" => {
                    self.bump();
                    if self.peek().kind == TokenKind::Colon {
                        self.bump();
                    }
                    let (r, sp) = self.expect_ident_or_str("rail net")?;
                    rail = r;
                    end = sp;
                }
                TokenKind::Ident(s) if s == "footprint" => {
                    self.bump();
                    self.expect(TokenKind::Colon, "':'")?;
                    let (fp, sp) = self.expect_string("footprint")?;
                    footprint = Some(fp);
                    end = sp;
                }
                _ => break,
            }
        }
        Ok(CircuitIdiomDecl::PullResistor {
            is_up,
            target,
            ref_id,
            value,
            rail,
            footprint,
            span: start.merge(&end),
        })
    }

    fn parse_idiom_divider(&mut self) -> Result<CircuitIdiomDecl, Diagnostic> {
        // `divider VIN -> R1: "10k" -> FB [at: U1.FB] -> R2: "3.3k" -> GND [footprint: "..."]`
        // OR `divider VIN -> R1: "10k" -> U1.FB -> R2: "3.3k" -> GND`
        let start = self.expect_keyword("divider")?;
        let (top_net, _) = self.expect_ident_or_str("top net")?;
        self.expect(TokenKind::Arrow, "'->'")?;
        let (r_top, _) = self.parse_ref_val_pair()?;
        self.expect(TokenKind::Arrow, "'->'")?;
        let (mut tap, _) = self.parse_chain_endpoint()?;
        let mut tap_net_name = None;
        if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "at") {
            self.bump();
            if self.peek().kind == TokenKind::Colon {
                self.bump();
            }
            if let ChainEndpoint::Net(ref n) = tap {
                tap_net_name = Some(n.clone());
            }
            tap = ChainEndpoint::Pin(self.parse_pin_ref()?);
        }
        self.expect(TokenKind::Arrow, "'->'")?;
        let (r_bot, _) = self.parse_ref_val_pair()?;
        self.expect(TokenKind::Arrow, "'->'")?;
        let (bot_net, mut end) = self.expect_ident_or_str("bottom net")?;
        let mut footprint = None;
        if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "footprint") {
            self.bump();
            self.expect(TokenKind::Colon, "':'")?;
            let (fp, sp) = self.expect_string("footprint")?;
            footprint = Some(fp);
            end = sp;
        }
        Ok(CircuitIdiomDecl::Divider {
            top_net,
            r_top,
            tap,
            tap_net_name,
            r_bot,
            bot_net,
            footprint,
            span: start.merge(&end),
        })
    }

    fn parse_idiom_series(&mut self) -> Result<CircuitIdiomDecl, Diagnostic> {
        // `series J1.D_P -> R1: "22" -> [L1: "10uH" ->] U1.USB_DP [footprint: "..."]`
        let start = self.expect_keyword("series")?;
        let (from, _) = self.parse_chain_endpoint()?;
        self.expect(TokenKind::Arrow, "'->'")?;
        let mut stages = Vec::new();
        let (to, mut end) = loop {
            // Peek ahead: if `<ident|str> ':'` -> stage component; else -> final endpoint!
            if self.peek_next().kind == TokenKind::Colon {
                let (pair, _) = self.parse_ref_val_pair()?;
                stages.push(pair);
                self.expect(TokenKind::Arrow, "'->'")?;
            } else {
                break self.parse_chain_endpoint()?;
            }
        };
        let mut footprint = None;
        if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "footprint") {
            self.bump();
            self.expect(TokenKind::Colon, "':'")?;
            let (fp, sp) = self.expect_string("footprint")?;
            footprint = Some(fp);
            end = sp;
        }
        Ok(CircuitIdiomDecl::SeriesChain {
            from,
            stages,
            to,
            footprint,
            span: start.merge(&end),
        })
    }

    fn parse_idiom_rc_filter(&mut self) -> Result<CircuitIdiomDecl, Diagnostic> {
        // `rc_filter VIN -> R1: "1k" -> FILT [at: U1.ADC] -> C1: "100nF" -> GND [footprint: "..."]`
        let start = self.expect_keyword("rc_filter")?;
        let (from, _) = self.parse_chain_endpoint()?;
        self.expect(TokenKind::Arrow, "'->'")?;
        let ((r_id, r_val), _) = self.parse_ref_val_pair()?;
        self.expect(TokenKind::Arrow, "'->'")?;
        let (mut tap, _) = self.parse_chain_endpoint()?;
        let mut tap_net_name = None;
        if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "at") {
            self.bump();
            if self.peek().kind == TokenKind::Colon {
                self.bump();
            }
            if let ChainEndpoint::Net(ref n) = tap {
                tap_net_name = Some(n.clone());
            }
            tap = ChainEndpoint::Pin(self.parse_pin_ref()?);
        }
        self.expect(TokenKind::Arrow, "'->'")?;
        let ((c_id, c_val), _) = self.parse_ref_val_pair()?;
        self.expect(TokenKind::Arrow, "'->'")?;
        let (ground, mut end) = self.expect_ident_or_str("ground net")?;
        let mut footprint = None;
        if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "footprint") {
            self.bump();
            self.expect(TokenKind::Colon, "':'")?;
            let (fp, sp) = self.expect_string("footprint")?;
            footprint = Some(fp);
            end = sp;
        }
        Ok(CircuitIdiomDecl::RcFilter {
            from,
            r_id,
            r_val,
            tap,
            tap_net_name,
            c_id,
            c_val,
            ground,
            footprint,
            span: start.merge(&end),
        })
    }

    fn expect_ident_or_str(&mut self, what: &str) -> Result<(String, Span), Diagnostic> {
        match self.peek().kind.clone() {
            TokenKind::Str(s) => {
                let sp = self.bump().span(&self.file);
                Ok((s, sp))
            }
            _ => self.expect_ident(what),
        }
    }

    fn parse_xy_pair(&mut self) -> Result<((Quantity, Quantity), Span), Diagnostic> {
        let lp = self.expect(TokenKind::LParen, "'('")?.span(&self.file);
        let (x, _) = self.parse_quantity()?;
        self.expect(TokenKind::Comma, "','")?;
        let (y, _) = self.parse_quantity()?;
        let rp = self.expect(TokenKind::RParen, "')'")?.span(&self.file);
        Ok(((x, y), lp.merge(&rp)))
    }

    fn parse_optional_orient_mirror(
        &mut self,
    ) -> Result<(Option<i32>, Option<crate::domains::elec::kicad_sym::SymMirror>, Option<Span>), Diagnostic> {
        let mut orient: Option<i32> = None;
        let mut mirror: Option<crate::domains::elec::kicad_sym::SymMirror> = None;
        let mut last_sp: Option<Span> = None;
        loop {
            match &self.peek().kind {
                TokenKind::Ident(s) if s == "orient" || s == "rotation" => {
                    self.bump();
                    self.expect(TokenKind::Colon, "':'")?;
                    let neg = if self.peek().kind == TokenKind::Minus {
                        self.bump();
                        true
                    } else {
                        false
                    };
                    let tok = self.peek().clone();
                    match tok.kind {
                        TokenKind::Number(n, ref u) if u.is_empty() || u == "deg" => {
                            let sp = self.bump().span(&self.file);
                            let signed = if neg { -n } else { n };
                            orient = Some((signed.round() as i32).rem_euclid(360));
                            last_sp = Some(sp);
                        }
                        _ => {
                            return Err(self.err("E_PARSE_ORIENT", "Expected orientation angle in degrees", &tok));
                        }
                    }
                }
                TokenKind::Ident(s) if s == "mirror" => {
                    self.bump();
                    self.expect(TokenKind::Colon, "':'")?;
                    let (m, msp) = self.expect_ident("mirror axis ('x' or 'y')")?;
                    mirror = match m.to_lowercase().as_str() {
                        "x" => Some(crate::domains::elec::kicad_sym::SymMirror::X),
                        "y" => Some(crate::domains::elec::kicad_sym::SymMirror::Y),
                        _ => {
                            return Err(Diagnostic::error(
                                "E_PARSE_MIRROR",
                                "syntax",
                                format!("Unknown mirror '{m}' (expected 'x' or 'y')"),
                                Some(msp),
                            ))
                        }
                    };
                    last_sp = Some(msp);
                }
                _ => break,
            }
        }
        Ok((orient, mirror, last_sp))
    }

    fn parse_component(&mut self) -> Result<ComponentDecl, Diagnostic> {
        let start = self.expect_keyword("component")?;
        let (id, _) = self.expect_ident_or_str("component designator")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (category, is_quoted_symbol) = match self.peek().kind.clone() {
            TokenKind::Str(s) => {
                self.bump();
                (s, true)
            }
            _ => (
                self.expect_ident("component category or quoted KiCad 'Library:Symbol'")?.0,
                false,
            ),
        };
        self.expect(TokenKind::LBrace, "'{'")?;

        let mut value = String::new();
        let mut footprint = None;
        let mut datasheet = None;
        let mut orient_deg: i32 = 0;
        let mut mirror: Option<crate::domains::elec::kicad_sym::SymMirror> = None;
        let mut unit: Option<usize> = None;
        let mut no_connect: Vec<String> = Vec::new();
        let mut isolated = false;
        let mut dnp = false;
        let mut in_bom = true;
        let mut on_board = true;
        let mut props: Vec<(String, String)> = Vec::new();
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
                "orient" | "rotation" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    match self.peek().kind.clone() {
                        TokenKind::Number(n, u) if u.is_empty() || u == "deg" => {
                            self.bump();
                            orient_deg = (n.round() as i32).rem_euclid(360);
                        }
                        TokenKind::Ident(ref d) => {
                            let dl = d.to_lowercase();
                            self.bump();
                            orient_deg = match dl.as_str() {
                                "horizontal" | "h" => 90,
                                "vertical" | "v" => 0,
                                _ => {
                                    return Err(Diagnostic::error(
                                        "E_PARSE_ORIENT",
                                        "syntax",
                                        format!("Unknown orient '{d}' (expected 0, 90, 180, 270, horizontal, vertical)"),
                                        Some(kspan),
                                    ))
                                }
                            };
                        }
                        _ => {
                            let tok = self.peek().clone();
                            return Err(self.err("E_PARSE_ORIENT", "Expected angle (0, 90, 180, 270) or horizontal/vertical", &tok));
                        }
                    }
                }
                "mirror" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (m, mspan) = self.expect_ident("mirror axis ('x' or 'y')")?;
                    mirror = match m.to_lowercase().as_str() {
                        "x" => Some(crate::domains::elec::kicad_sym::SymMirror::X),
                        "y" => Some(crate::domains::elec::kicad_sym::SymMirror::Y),
                        _ => {
                            return Err(Diagnostic::error(
                                "E_PARSE_MIRROR",
                                "syntax",
                                format!("Unknown mirror '{m}' (expected 'x' or 'y')"),
                                Some(mspan),
                            ))
                        }
                    };
                }
                "unit" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    match self.peek().kind.clone() {
                        TokenKind::Number(n, u) if u.is_empty() && n >= 1.0 => {
                            self.bump();
                            unit = Some(n.round() as usize);
                        }
                        TokenKind::Ident(ref s) if s.len() == 1 => {
                            let ch = s.chars().next().unwrap().to_ascii_uppercase();
                            self.bump();
                            if ('A'..='Z').contains(&ch) {
                                unit = Some((ch as u8 - b'A' + 1) as usize);
                            }
                        }
                        _ => {
                            let tok = self.peek().clone();
                            return Err(self.err("E_PARSE_UNIT", "Expected unit number (1..N) or letter (A..Z)", &tok));
                        }
                    }
                }
                "no_connect" | "nc" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    self.expect(TokenKind::LBracket, "'['")?;
                    while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                        let tok = self.peek().clone();
                        match tok.kind {
                            TokenKind::Ident(name) | TokenKind::Str(name) => {
                                self.bump();
                                no_connect.push(name);
                            }
                            TokenKind::Number(n, u) if u.is_empty() => {
                                self.bump();
                                no_connect.push(format!("{}", n.round() as i64));
                            }
                            _ => {
                                return Err(self.err("E_PARSE_NC", "Expected pin name or number inside no_connect list", &tok));
                            }
                        }
                        if self.peek().kind == TokenKind::Comma {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBracket, "']'")?;
                }
                "datasheet" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (s, _) = self.expect_string("datasheet URL or text")?;
                    datasheet = Some(s);
                }
                "dnp" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (b, _) = self.expect_ident("true or false")?;
                    dnp = b == "true";
                }
                "in_bom" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (b, _) = self.expect_ident("true or false")?;
                    in_bom = b == "true";
                }
                "on_board" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (b, _) = self.expect_ident("true or false")?;
                    on_board = b == "true";
                }
                "props" => {
                    self.expect(TokenKind::LBrace, "'{'")?;
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        let (pk, _) = self.expect_ident_or_str("property key")?;
                        self.expect(TokenKind::Colon, "':'")?;
                        let (pv, _) = self.expect_string("property value")?;
                        props.push((pk, pv));
                        if self.peek().kind == TokenKind::Comma {
                            self.bump();
                        }
                    }
                    self.expect(TokenKind::RBrace, "'}'")?;
                }
                "isolated" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (b, _) = self.expect_ident("true or false")?;
                    isolated = b == "true";
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
        if value.is_empty() && !is_quoted_symbol && !category.contains(':') {
            value = category.clone();
        }
        let cat_lower = category.to_lowercase();
        let is_iso = isolated || cat_lower.contains("transformer") || cat_lower.contains("optocoupler") || cat_lower.contains("isolator");
        Ok(ComponentDecl {
            id,
            category,
            is_quoted_symbol,
            value,
            footprint,
            datasheet,
            orient_deg,
            mirror,
            unit,
            no_connect,
            isolated: is_iso,
            dnp,
            in_bom,
            on_board,
            props,
            pins,
            provenance: None,
            span: start.merge(&end),
        })
    }

    fn parse_net(&mut self) -> Result<NetDecl, Diagnostic> {
        let start = self.expect_keyword("net")?;
        let (name, _) = self.expect_ident_or_str("net name")?;
        
        // Parse hierarchical net name: /Module/SubModule/NetName -> ["Module", "SubModule"] with name="NetName"
        let mut hierarchy = Vec::new();
        let mut net_name = name.clone();
        if name.starts_with('/') {
            let parts: Vec<&str> = name.split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() > 1 {
                // All but last are hierarchy
                hierarchy = parts[..parts.len() - 1].iter().map(|s| s.to_string()).collect();
                net_name = parts[parts.len() - 1].to_string();
            } else if parts.len() == 1 {
                net_name = parts[0].to_string();
            }
        }
        
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut connects = Vec::new();
        let mut power_flag = false;
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (key, kspan) = self.expect_ident("connect or power_flag")?;
            match key.as_str() {
                "connect" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    self.expect(TokenKind::LBracket, "'['")?;
                    while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                        let (comp, cspan) = self.expect_ident_or_str("component id")?;
                        self.expect(TokenKind::Dot, "'.'")?;
                        let (pin, pspan) = match self.peek().kind.clone() {
                            TokenKind::Ident(p) | TokenKind::Str(p) => {
                                let sp = self.bump().span(&self.file);
                                (p, sp)
                            }
                            TokenKind::Number(n, u) if u.is_empty() => {
                                let sp = self.bump().span(&self.file);
                                (format!("{}", n.round() as i64), sp)
                            }
                            TokenKind::Plus => {
                                let sp = self.bump().span(&self.file);
                                ("+".to_string(), sp)
                            }
                            TokenKind::Minus => {
                                let sp = self.bump().span(&self.file);
                                ("-".to_string(), sp)
                            }
                            _ => self.expect_ident_or_str("pin name or number")?,
                        };
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
                "power_flag" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (b, _) = self.expect_ident("true or false")?;
                    power_flag = b == "true";
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_NET",
                        "syntax",
                        format!("Expected 'connect' or 'power_flag' inside net '{name}', found '{other}'"),
                        Some(kspan),
                    ));
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(NetDecl {
            name: net_name,
            hierarchy,
            connects,
            power_flag,
            span: start.merge(&end),
        })
    }

    fn parse_schematic(&mut self) -> Result<SchematicDecl, Diagnostic> {
        let start = self.expect_keyword("schematic")?;
        let (name, _) = self.expect_ident_or_str("schematic name")?;
        self.expect(TokenKind::LBrace, "'{'")?;

        let mut sheet = "A4 landscape".to_string();
        let mut sheet_file = None;
        let mut title = None;
        let mut rev = None;
        let mut author = None;
        let mut groups = Vec::new();
        let mut layout = None;
        let mut routing = None;
        let mut placements = Vec::new();
        let mut relative_placements = Vec::new();
        let mut standalone_notes = Vec::new();
        let mut explicit_wires = Vec::new();
        let mut explicit_bus_entries = Vec::new();
        let mut explicit_junctions = Vec::new();
        let mut explicit_no_connects = Vec::new();
        let mut explicit_powers = Vec::new();
        let mut explicit_labels = Vec::new();
        let mut explicit_text_boxes = Vec::new();
        let mut explicit_graphics = Vec::new();
        let mut explicit_sheets = Vec::new();
        let mut bus_aliases = Vec::new();

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (kw, kspan) = self.expect_ident("schematic declaration")?;
            match kw.as_str() {
                "file" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    sheet_file = Some(self.expect_string("sheet filename")?.0);
                }
                "title" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    title = Some(self.expect_string("sheet title")?.0);
                }
                "rev" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    rev = Some(self.expect_string("sheet revision")?.0);
                }
                "author" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    author = Some(self.expect_string("sheet author")?.0);
                }
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
                "group" | "zone" => {
                    let is_zone = kw == "zone";
                    let gname = match self.peek().kind.clone() {
                        TokenKind::Str(s) => {
                            self.bump();
                            s
                        }
                        _ => self.expect_ident("group/zone name")?.0,
                    };
                    self.expect(TokenKind::LBrace, "'{'")?;
                    let mut gtitle: Option<String> = None;
                    let mut border = if is_zone { "solid" } else { "dashed" }.to_string();
                    let mut members: Vec<String> = Vec::new();
                    let mut notes: Vec<String> = Vec::new();
                    let mut tree_items: Vec<LayoutNode> = Vec::new();

                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        let (gk, gspan) = self.expect_ident("group/zone entry")?;
                        match gk.as_str() {
                            "title" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                gtitle = Some(self.expect_string("zone title")?.0);
                            }
                            "style" | "border" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (st, _) = self.expect_ident("solid or dashed")?;
                                border = st;
                            }
                            "note" => {
                                if self.peek().kind == TokenKind::Colon {
                                    self.bump();
                                }
                                notes.push(self.expect_string("zone note")?.0);
                            }
                            "members" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                self.expect(TokenKind::LBracket, "'['")?;
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
                            }
                            "hstack" => {
                                tree_items.push(self.parse_layout_node("hstack")?);
                            }
                            "vstack" => {
                                tree_items.push(self.parse_layout_node("vstack")?);
                            }
                            "subzone" => {
                                tree_items.push(self.parse_layout_node("subzone")?);
                            }
                            other => {
                                return Err(Diagnostic::error(
                                    "E_PARSE_GROUP",
                                    "syntax",
                                    format!("Unknown entry '{other}' inside group/zone '{gname}'"),
                                    Some(gspan),
                                ))
                            }
                        }
                    }
                    let gend = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
                    let layout_tree = if tree_items.is_empty() {
                        None
                    } else if tree_items.len() == 1 {
                        Some(tree_items.remove(0))
                    } else {
                        Some(LayoutNode::VStack(tree_items))
                    };
                    if let Some(ref lt) = layout_tree {
                        let mut collected = Vec::new();
                        lt.collect_components(&mut collected);
                        for c in collected {
                            if !members.contains(&c) {
                                members.push(c);
                            }
                        }
                    }
                    groups.push(SchematicGroupDecl {
                        name: gname,
                        title: gtitle,
                        border,
                        members,
                        layout_tree,
                        notes,
                        span: kspan.merge(&gend),
                    });
                }
                "layout" => {
                    self.expect(TokenKind::LBrace, "'{'")?;
                    let mut arrange = Vec::new();
                    let mut rows = Vec::new();
                    let mut direction = "left_to_right".to_string();
                    let mut pack = None;
                    let mut group_gap = None;
                    let mut member_gap = None;
                    let mut row_gap = None;
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        let (lk, lspan) = self.expect_ident("schematic layout property")?;
                        match lk.as_str() {
                            "pack" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (pm, _) = self.expect_ident("skyline or rows")?;
                                pack = Some(pm);
                            }
                            "arrange" => {
                                self.expect(TokenKind::Colon, "':'")?;
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
                            "row" => {
                                if self.peek().kind == TokenKind::Colon {
                                    self.bump();
                                }
                                self.expect(TokenKind::LBracket, "'['")?;
                                let mut r = Vec::new();
                                while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                                    let (g, _) = self.expect_ident("group name")?;
                                    r.push(g);
                                    if self.peek().kind == TokenKind::Comma {
                                        self.bump();
                                    } else {
                                        break;
                                    }
                                }
                                self.expect(TokenKind::RBracket, "']'")?;
                                rows.push(r);
                            }
                            "group_gap" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (q, _) = self.parse_quantity()?;
                                group_gap = Some(q);
                            }
                            "member_gap" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (q, _) = self.parse_quantity()?;
                                member_gap = Some(q);
                            }
                            "row_gap" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (q, _) = self.parse_quantity()?;
                                row_gap = Some(q);
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
                        rows,
                        direction,
                        pack,
                        group_gap,
                        member_gap,
                        row_gap,
                        span: kspan.merge(&lend),
                    });
                }
                "routing" => {
                    self.expect(TokenKind::LBrace, "'{'")?;
                    let mut rdecl = SchematicRoutingDecl::default();
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        let (rk, rspan) = self.expect_ident("default, power_nets, ground_nets, or net")?;
                        match rk.as_str() {
                            "mode" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (m, msp) = self.expect_ident("routing mode ('explicit' or 'auto')")?;
                                match m.as_str() {
                                    "explicit" => rdecl.explicit_mode = true,
                                    "auto" => rdecl.explicit_mode = false,
                                    _ => {
                                        return Err(Diagnostic::error(
                                            "E_PARSE_ROUTING",
                                            "syntax",
                                            format!("Unknown routing mode '{m}' (expected 'explicit' or 'auto')"),
                                            Some(msp),
                                        ))
                                    }
                                }
                            }
                            "default" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (st, sspan) = self.expect_ident("routing style")?;
                                rdecl.default_style = Some(parse_routing_style(&st, sspan)?);
                            }
                            "power_nets" | "power" => {
                                self.expect(TokenKind::Colon, "':'")?;
                                let (st, sspan) = self.expect_ident("routing style")?;
                                rdecl.power_style = Some(parse_routing_style(&st, sspan)?);
                            }
                            "ground_nets" | "ground" => {
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
                    let comp = match self.peek().kind.clone() {
                        TokenKind::Str(s) => {
                            self.bump();
                            s
                        }
                        _ => self.parse_room_ref()?,
                    };
                    // Check for AI-native relative placement keywords (`left_of`, `right_of`, `above`, `below`, `offset`, `at_angle`)!
                    if let TokenKind::Ident(ref rel_kw) = self.peek().kind.clone() {
                        if matches!(
                            rel_kw.as_str(),
                            "left_of" | "right_of" | "above" | "below" | "offset" | "at_angle"
                        ) {
                            let kw_s = rel_kw.clone();
                            self.bump();
                            let (kind, mut rend) = match kw_s.as_str() {
                                "left_of" | "right_of" | "above" | "below" => {
                                    let dir = match kw_s.as_str() {
                                        "left_of" => RelDir::LeftOf,
                                        "right_of" => RelDir::RightOf,
                                        "above" => RelDir::Above,
                                        _ => RelDir::Below,
                                    };
                                    let (anchor, mut sp) = self.parse_anchor_ref()?;
                                    let mut gap = None;
                                    let mut align_pin = None;
                                    loop {
                                        match &self.peek().kind {
                                            TokenKind::Ident(s) if s == "gap" || s == "by" => {
                                                self.bump();
                                                if self.peek().kind == TokenKind::Colon {
                                                    self.bump();
                                                }
                                                let (q, qsp) = self.parse_quantity()?;
                                                gap = Some(q);
                                                sp = sp.merge(&qsp);
                                            }
                                            TokenKind::Ident(s) if s == "align_pin" => {
                                                self.bump();
                                                self.expect(TokenKind::Colon, "':'")?;
                                                let (p, psp) = self.parse_pin_token()?;
                                                align_pin = Some(p);
                                                sp = sp.merge(&psp);
                                            }
                                            _ => break,
                                        }
                                    }
                                    (
                                        RelativePlacementKind::Directional {
                                            dir,
                                            anchor,
                                            gap,
                                            align_pin,
                                        },
                                        sp,
                                    )
                                }
                                "offset" => {
                                    self.expect(TokenKind::Colon, "':'")?;
                                    let ((dx, dy), _) = self.parse_xy_pair()?;
                                    self.expect_keyword("from")?;
                                    let (anchor, sp) = self.parse_anchor_ref()?;
                                    (RelativePlacementKind::Offset { dx, dy, anchor }, sp)
                                }
                                _ => {
                                    // `at_angle: 45deg dist: 15.24mm from P1`
                                    if self.peek().kind == TokenKind::Colon {
                                        self.bump();
                                    }
                                    let (ang_q, _) = self.parse_quantity()?;
                                    self.expect_keyword("dist")?;
                                    if self.peek().kind == TokenKind::Colon {
                                        self.bump();
                                    }
                                    let (dist, _) = self.parse_quantity()?;
                                    self.expect_keyword("from")?;
                                    let (anchor, sp) = self.parse_anchor_ref()?;
                                    (
                                        RelativePlacementKind::Polar {
                                            angle_deg: ang_q.raw_value,
                                            dist,
                                            anchor,
                                        },
                                        sp,
                                    )
                                }
                            };
                            let (orient_deg, mirror, extra_sp) = self.parse_optional_orient_mirror()?;
                            if let Some(sp) = extra_sp {
                                rend = rend.merge(&sp);
                            }
                            relative_placements.push(RelativePlacementDecl {
                                component: comp,
                                kind,
                                orient_deg,
                                mirror,
                                span: kspan.merge(&rend),
                            });
                            continue;
                        }
                    }
                    let mut punit = None;
                    if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "unit") {
                        self.bump();
                        self.expect(TokenKind::Colon, "':'")?;
                        if let TokenKind::Number(n, u) = self.peek().kind.clone() {
                            if u.is_empty() && n >= 1.0 {
                                self.bump();
                                punit = Some(n.round() as usize);
                            }
                        }
                    }
                    self.expect_keyword("at")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((x, y), mut pend) = self.parse_xy_pair()?;
                    let (orient_deg, mirror, extra_sp) = self.parse_optional_orient_mirror()?;
                    if let Some(sp) = extra_sp {
                        pend = pend.merge(&sp);
                    }
                    let mut instances = Vec::new();
                    let mut ref_at = None;
                    let mut val_at = None;
                    loop {
                        match &self.peek().kind {
                            TokenKind::Ident(s) if s == "ref_at" || s == "val_at" => {
                                let is_ref = s == "ref_at";
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                self.expect(TokenKind::LParen, "'('")?;
                                let (fx, _) = self.parse_quantity()?;
                                self.expect(TokenKind::Comma, "','")?;
                                let (fy, _) = self.parse_quantity()?;
                                let frot = if self.peek().kind == TokenKind::Comma {
                                    self.bump();
                                    let (fr, _) = self.parse_quantity()?;
                                    (fr.raw_value.round() as i32).rem_euclid(360)
                                } else {
                                    0
                                };
                                pend = self.expect(TokenKind::RParen, "')'")?.span(&self.file);
                                let tuple = (fx.to_mm().unwrap_or(0.0), fy.to_mm().unwrap_or(0.0), frot);
                                if is_ref {
                                    ref_at = Some(tuple);
                                } else {
                                    val_at = Some(tuple);
                                }
                            }
                            TokenKind::Ident(s) if s == "instances" => {
                                self.bump();
                                if self.peek().kind == TokenKind::Colon {
                                    self.bump();
                                }
                                self.expect(TokenKind::LBrace, "'{'")?;
                                while self.peek().kind != TokenKind::RBrace
                                    && self.peek().kind != TokenKind::Eof
                                {
                                    let (ipath, _) = self.expect_string("instance sheet path")?;
                                    self.expect(TokenKind::Colon, "':'")?;
                                    let (iref, _) = self.expect_ident_or_str("instance reference")?;
                                    instances.push((ipath, iref));
                                    if self.peek().kind == TokenKind::Comma {
                                        self.bump();
                                    }
                                }
                                pend = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
                            }
                            _ => break,
                        }
                    }
                    let mut fields = Vec::new();
                    if self.peek().kind == TokenKind::LBrace {
                        self.bump();
                        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                            self.expect_keyword("field")?;
                            let (fname, _) = self.expect_string("field name")?;
                            let fval = if let TokenKind::Str(s) = self.peek().kind.clone() {
                                self.bump();
                                Some(s)
                            } else {
                                None
                            };
                            let mut fhide = false;
                            let mut fx = x.to_mm().unwrap_or(0.0);
                            let mut fy = y.to_mm().unwrap_or(0.0);
                            let mut frot = 0i32;
                            let mut fsize = None;
                            let mut fjust = None;
                            loop {
                                match &self.peek().kind {
                                    TokenKind::Ident(s) if s == "hide" => {
                                        self.bump();
                                        fhide = true;
                                    }
                                    TokenKind::Ident(s) if s == "at" => {
                                        self.bump();
                                        self.expect(TokenKind::Colon, "':'")?;
                                        self.expect(TokenKind::LParen, "'('")?;
                                        let (qx, _) = self.parse_quantity()?;
                                        self.expect(TokenKind::Comma, "','")?;
                                        let (qy, _) = self.parse_quantity()?;
                                        if self.peek().kind == TokenKind::Comma {
                                            self.bump();
                                            let (qr, _) = self.parse_quantity()?;
                                            frot = (qr.raw_value.round() as i32).rem_euclid(360);
                                        }
                                        self.expect(TokenKind::RParen, "')'")?;
                                        fx = qx.to_mm().unwrap_or(fx);
                                        fy = qy.to_mm().unwrap_or(fy);
                                    }
                                    TokenKind::Ident(s) if s == "size" => {
                                        self.bump();
                                        self.expect(TokenKind::Colon, "':'")?;
                                        let (qs, _) = self.parse_quantity()?;
                                        fsize = Some(qs.to_mm().unwrap_or(qs.raw_value));
                                    }
                                    TokenKind::Ident(s) if s == "justify" => {
                                        self.bump();
                                        self.expect(TokenKind::Colon, "':'")?;
                                        fjust = Some(self.expect_ident_or_str("justify")?.0);
                                    }
                                    _ => break,
                                }
                            }
                            fields.push(ExplicitFieldStyleDecl {
                                name: fname,
                                value: fval,
                                hide: fhide,
                                x: fx,
                                y: fy,
                                orient_deg: frot,
                                font_size: fsize,
                                justify: fjust,
                            });
                        }
                        pend = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
                    }
                    placements.push(ExplicitPlacementDecl {
                        component: comp,
                        unit: punit,
                        x,
                        y,
                        orient_deg,
                        mirror,
                        instances,
                        ref_at,
                        val_at,
                        fields,
                        span: kspan.merge(&pend),
                    });
                }
                "align_y" | "align_x" => {
                    let horizontal = kw == "align_y";
                    let (lhs, _) = self.parse_anchor_ref()?;
                    self.expect(TokenKind::EqEq, "'=='")?;
                    let (rhs, rsp) = self.parse_anchor_ref()?;
                    relative_placements.push(RelativePlacementDecl {
                        component: lhs.component,
                        kind: RelativePlacementKind::AlignAxis {
                            horizontal,
                            my_pin: lhs.pin,
                            anchor: rhs,
                        },
                        orient_deg: None,
                        mirror: None,
                        span: kspan.merge(&rsp),
                    });
                }
                "array" => {
                    self.expect(TokenKind::LBracket, "'['")?;
                    let mut members = Vec::new();
                    while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                        let (m, _) = self.expect_ident_or_str("array component id")?;
                        members.push(m);
                        if self.peek().kind == TokenKind::Comma {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    let mut aend = self.expect(TokenKind::RBracket, "']'")?.span(&self.file);
                    let mut vertical = true;
                    if let TokenKind::Ident(ref d) = self.peek().kind.clone() {
                        if d == "vertical" || d == "horizontal" {
                            vertical = d == "vertical";
                            aend = self.bump().span(&self.file);
                        }
                    }
                    let mut pitch = None;
                    if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "pitch") {
                        self.bump();
                        if self.peek().kind == TokenKind::Colon {
                            self.bump();
                        }
                        let (q, qsp) = self.parse_quantity()?;
                        pitch = Some(q);
                        aend = aend.merge(&qsp);
                    }
                    let head = members.first().cloned().unwrap_or_default();
                    relative_placements.push(RelativePlacementDecl {
                        component: head,
                        kind: RelativePlacementKind::Array {
                            members,
                            vertical,
                            pitch,
                        },
                        orient_deg: None,
                        mirror: None,
                        span: kspan.merge(&aend),
                    });
                }
                "note" => {
                    let (txt, mut nend) = self.expect_string("schematic note text")?;
                    let mut near = None;
                    if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "near") {
                        self.bump();
                        if self.peek().kind == TokenKind::Colon {
                            self.bump();
                        }
                        let (tgt, tsp) = self.expect_ident_or_str("target group or component")?;
                        near = Some(tgt);
                        nend = nend.merge(&tsp);
                    }
                    standalone_notes.push(SchematicNoteDecl {
                        text: txt,
                        near,
                        span: kspan.merge(&nend),
                    });
                }
                "wire" | "bus" => {
                    let is_bus = kw == "bus";
                    let (p0, mut pend) = self.parse_xy_pair()?;
                    let mut pts = vec![p0];
                    while self.peek().kind == TokenKind::Arrow {
                        self.bump();
                        let (pn, psp) = self.parse_xy_pair()?;
                        pts.push(pn);
                        pend = psp;
                    }
                    explicit_wires.push(ExplicitWireDecl {
                        pts,
                        is_bus,
                        span: kspan.merge(&pend),
                    });
                }
                "bus_entry" => {
                    self.expect_keyword("at")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((x, y), _) = self.parse_xy_pair()?;
                    self.expect_keyword("size")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((dx, dy), pend) = self.parse_xy_pair()?;
                    explicit_bus_entries.push(ExplicitBusEntryDecl {
                        x,
                        y,
                        dx,
                        dy,
                        span: kspan.merge(&pend),
                    });
                }
                "junction" => {
                    self.expect_keyword("at")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((x, y), pend) = self.parse_xy_pair()?;
                    explicit_junctions.push(ExplicitPointDecl {
                        x,
                        y,
                        span: kspan.merge(&pend),
                    });
                }
                "no_connect" => {
                    self.expect_keyword("at")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((x, y), pend) = self.parse_xy_pair()?;
                    explicit_no_connects.push(ExplicitPointDecl {
                        x,
                        y,
                        span: kspan.merge(&pend),
                    });
                }
                "power" => {
                    let (lib_id, _) = self.expect_string("power lib_id")?;
                    let value = if let TokenKind::Str(s) = self.peek().kind.clone() {
                        self.bump();
                        s
                    } else {
                        lib_id.split_once(':').map(|(_, v)| v.to_string()).unwrap_or_else(|| lib_id.clone())
                    };
                    let mut reference = None;
                    if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "ref") {
                        self.bump();
                        self.expect(TokenKind::Colon, "':'")?;
                        reference = Some(self.expect_string("power reference")?.0);
                    }
                    self.expect_keyword("at")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((x, y), mut pend) = self.parse_xy_pair()?;
                    let (orient_opt, mirror, extra_sp) = self.parse_optional_orient_mirror()?;
                    if let Some(sp) = extra_sp {
                        pend = pend.merge(&sp);
                    }
                    let mut val_at = None;
                    let mut val_hide = false;
                    let mut val_justify = None;
                    loop {
                        match &self.peek().kind {
                            TokenKind::Ident(s) if s == "val_at" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                self.expect(TokenKind::LParen, "'('")?;
                                let (vx, _) = self.parse_quantity()?;
                                self.expect(TokenKind::Comma, "','")?;
                                let (vy, _) = self.parse_quantity()?;
                                let vrot = if self.peek().kind == TokenKind::Comma {
                                    self.bump();
                                    let (vr, _) = self.parse_quantity()?;
                                    (vr.raw_value.round() as i32).rem_euclid(360)
                                } else {
                                    0
                                };
                                pend = self.expect(TokenKind::RParen, "')'")?.span(&self.file);
                                val_at = Some((vx.to_mm().unwrap_or(0.0), vy.to_mm().unwrap_or(0.0), vrot));
                            }
                            TokenKind::Ident(s) if s == "val_hide" => {
                                let sp = self.bump().span(&self.file);
                                val_hide = true;
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "val_justify" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (j, sp) = self.expect_ident_or_str("val_justify")?;
                                val_justify = Some(j);
                                pend = pend.merge(&sp);
                            }
                            _ => break,
                        }
                    }
                    explicit_powers.push(ExplicitPowerDecl {
                        lib_id,
                        value,
                        reference,
                        x,
                        y,
                        orient_deg: orient_opt.unwrap_or(0),
                        mirror,
                        val_at,
                        val_hide,
                        val_justify,
                        span: kspan.merge(&pend),
                    });
                }
                "label" | "global_label" | "hierarchical_label" | "text" => {
                    let (txt, _) = self.expect_string("label text")?;
                    let mut shape = None;
                    let mut justify = None;
                    let mut font_size = None;
                    if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "shape") {
                        self.bump();
                        self.expect(TokenKind::Colon, "':'")?;
                        shape = Some(self.expect_ident_or_str("label shape")?.0);
                    }
                    self.expect_keyword("at")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((x, y), mut pend) = self.parse_xy_pair()?;
                    let (orient_opt, _mirror, extra_sp) = self.parse_optional_orient_mirror()?;
                    if let Some(sp) = extra_sp {
                        pend = pend.merge(&sp);
                    }
                    loop {
                        match &self.peek().kind {
                            TokenKind::Ident(s) if s == "shape" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (sh, sp) = self.expect_ident_or_str("label shape")?;
                                shape = Some(sh);
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "justify" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (j, sp) = self.expect_ident_or_str("justify")?;
                                justify = Some(j);
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "size" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (qs, sp) = self.parse_quantity()?;
                                font_size = Some(qs.to_mm().unwrap_or(qs.raw_value));
                                pend = pend.merge(&sp);
                            }
                            _ => break,
                        }
                    }
                    explicit_labels.push(ExplicitLabelDecl {
                        kind: kw,
                        text: txt,
                        shape,
                        x,
                        y,
                        orient_deg: orient_opt.unwrap_or(0),
                        justify,
                        font_size,
                        span: kspan.merge(&pend),
                    });
                }
                "text_box" => {
                    let (txt, _) = self.expect_string("text_box text")?;
                    self.expect_keyword("at")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((x, y), _) = self.parse_xy_pair()?;
                    self.expect_keyword("size")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((w, h), mut pend) = self.parse_xy_pair()?;
                    let (orient_opt, _, extra_sp) = self.parse_optional_orient_mirror()?;
                    if let Some(sp) = extra_sp {
                        pend = pend.merge(&sp);
                    }
                    let mut font_size = None;
                    let mut justify = None;
                    let mut stroke_type = None;
                    let mut stroke_width = None;
                    let mut stroke_color = None;
                    let mut fill_color = None;
                    loop {
                        match &self.peek().kind {
                            TokenKind::Ident(s) if s == "font" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (qs, sp) = self.parse_quantity()?;
                                font_size = Some(qs.to_mm().unwrap_or(qs.raw_value));
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "justify" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (j, sp) = self.expect_ident_or_str("justify")?;
                                justify = Some(j);
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "style" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (st, sp) = self.expect_ident_or_str("stroke style")?;
                                stroke_type = Some(st);
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "width" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (qw, sp) = self.parse_quantity()?;
                                stroke_width = Some(qw.to_mm().unwrap_or(qw.raw_value));
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "color" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (c, sp) = self.expect_string("stroke color")?;
                                stroke_color = Some(c);
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "fill" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (f, sp) = self.expect_string("fill color")?;
                                fill_color = Some(f);
                                pend = pend.merge(&sp);
                            }
                            _ => break,
                        }
                    }
                    explicit_text_boxes.push(ExplicitTextBoxDecl {
                        text: txt,
                        x,
                        y,
                        w,
                        h,
                        orient_deg: orient_opt.unwrap_or(0),
                        font_size,
                        justify,
                        stroke_type,
                        stroke_width,
                        stroke_color,
                        fill_color,
                        span: kspan.merge(&pend),
                    });
                }
                "graphic_line" | "graphic_rect" => {
                    let gkind = if kw == "graphic_rect" { "rectangle" } else { "polyline" }.to_string();
                    let (p0, mut pend) = self.parse_xy_pair()?;
                    let mut pts = vec![p0];
                    while self.peek().kind == TokenKind::Arrow
                        || matches!(&self.peek().kind, TokenKind::Ident(s) if s == "to")
                    {
                        self.bump();
                        let (pn, psp) = self.parse_xy_pair()?;
                        pts.push(pn);
                        pend = psp;
                    }
                    let mut stroke_type = None;
                    let mut stroke_width = None;
                    let mut stroke_color = None;
                    let mut fill_color = None;
                    loop {
                        match &self.peek().kind {
                            TokenKind::Ident(s) if s == "style" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (st, sp) = self.expect_ident_or_str("stroke style")?;
                                stroke_type = Some(st);
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "width" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (qw, sp) = self.parse_quantity()?;
                                stroke_width = Some(qw.to_mm().unwrap_or(qw.raw_value));
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "color" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (c, sp) = self.expect_string("stroke color")?;
                                stroke_color = Some(c);
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "fill" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (f, sp) = self.expect_string("fill color")?;
                                fill_color = Some(f);
                                pend = pend.merge(&sp);
                            }
                            _ => break,
                        }
                    }
                    explicit_graphics.push(ExplicitGraphicDecl {
                        kind: gkind,
                        pts,
                        stroke_type,
                        stroke_width,
                        stroke_color,
                        fill_color,
                        span: kspan.merge(&pend),
                    });
                }
                "bus_alias" => {
                    let (bname, _) = self.expect_string("bus alias name")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    self.expect(TokenKind::LBracket, "'['")?;
                    let mut members = Vec::new();
                    while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                        let (m, _) = self.expect_string("bus alias member")?;
                        members.push(m);
                        if self.peek().kind == TokenKind::Comma {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBracket, "']'")?;
                    bus_aliases.push((bname, members));
                }
                "sheet_block" => {
                    let (sname, _) = self.expect_string("sheet block name")?;
                    self.expect_keyword("file")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let (sfile, _) = self.expect_string("sheet file")?;
                    self.expect_keyword("at")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((x, y), _) = self.parse_xy_pair()?;
                    self.expect_keyword("size")?;
                    self.expect(TokenKind::Colon, "':'")?;
                    let ((w, h), mut pend) = self.parse_xy_pair()?;
                    let mut name_at = None;
                    let mut file_at = None;
                    let mut file_justify = None;
                    let mut fill_color = None;
                    loop {
                        match &self.peek().kind {
                            TokenKind::Ident(s) if s == "name_at" || s == "file_at" => {
                                let is_name = s == "name_at";
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                self.expect(TokenKind::LParen, "'('")?;
                                let (fx, _) = self.parse_quantity()?;
                                self.expect(TokenKind::Comma, "','")?;
                                let (fy, _) = self.parse_quantity()?;
                                let frot = if self.peek().kind == TokenKind::Comma {
                                    self.bump();
                                    let (fr, _) = self.parse_quantity()?;
                                    (fr.raw_value.round() as i32).rem_euclid(360)
                                } else {
                                    0
                                };
                                pend = self.expect(TokenKind::RParen, "')'")?.span(&self.file);
                                let t = (fx.to_mm().unwrap_or(0.0), fy.to_mm().unwrap_or(0.0), frot);
                                if is_name { name_at = Some(t); } else { file_at = Some(t); }
                            }
                            TokenKind::Ident(s) if s == "file_justify" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (j, sp) = self.expect_ident_or_str("file_justify")?;
                                file_justify = Some(j);
                                pend = pend.merge(&sp);
                            }
                            TokenKind::Ident(s) if s == "fill" => {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (f, sp) = self.expect_string("fill color")?;
                                fill_color = Some(f);
                                pend = pend.merge(&sp);
                            }
                            _ => break,
                        }
                    }
                    let mut spins = Vec::new();
                    if self.peek().kind == TokenKind::LBrace {
                        self.bump();
                        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                            let pstart = self.expect_keyword("pin")?;
                            let (pname, _) = self.expect_string("sheet pin name")?;
                            let (pshape, _) = self.expect_ident_or_str("sheet pin shape")?;
                            self.expect_keyword("at")?;
                            self.expect(TokenKind::Colon, "':'")?;
                            let ((px, py), mut ppend) = self.parse_xy_pair()?;
                            let (porient, _, esp) = self.parse_optional_orient_mirror()?;
                            if let Some(sp) = esp {
                                ppend = ppend.merge(&sp);
                            }
                            let mut pjust = None;
                            if matches!(&self.peek().kind, TokenKind::Ident(s) if s == "justify") {
                                self.bump();
                                self.expect(TokenKind::Colon, "':'")?;
                                let (j, sp) = self.expect_ident_or_str("justify")?;
                                pjust = Some(j);
                                ppend = ppend.merge(&sp);
                            }
                            spins.push(ExplicitSheetPinDecl {
                                name: pname,
                                shape: pshape,
                                x: px,
                                y: py,
                                orient_deg: porient.unwrap_or(0),
                                justify: pjust,
                                span: pstart.merge(&ppend),
                            });
                        }
                        pend = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
                    }
                    explicit_sheets.push(ExplicitSheetBlockDecl {
                        name: sname,
                        file: sfile,
                        x,
                        y,
                        w,
                        h,
                        name_at,
                        file_at,
                        file_justify,
                        fill_color,
                        pins: spins,
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
            sheet_file,
            title,
            rev,
            author,
            groups,
            layout,
            routing,
            placements,
            relative_placements,
            standalone_notes,
            explicit_wires,
            explicit_bus_entries,
            explicit_junctions,
            explicit_no_connects,
            explicit_powers,
            explicit_labels,
            explicit_text_boxes,
            explicit_graphics,
            explicit_sheets,
            bus_aliases,
            span: start.merge(&end),
        })
    }

    fn parse_layout_node(&mut self, kind: &str) -> Result<LayoutNode, Diagnostic> {
        let title = if kind == "subzone" {
            Some(match self.peek().kind.clone() {
                TokenKind::Str(s) => {
                    self.bump();
                    s
                }
                _ => self.expect_ident("subzone title")?.0,
            })
        } else {
            None
        };
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut items = Vec::new();
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            match self.peek().kind.clone() {
                TokenKind::Ident(ref k) if k == "hstack" || k == "vstack" || k == "subzone" => {
                    let subk = k.clone();
                    self.bump();
                    items.push(self.parse_layout_node(&subk)?);
                }
                TokenKind::Str(s) => {
                    self.bump();
                    items.push(LayoutNode::Comp(s));
                }
                _ => {
                    let comp_ref = self.parse_room_ref()?;
                    items.push(LayoutNode::Comp(comp_ref));
                }
            }
            if self.peek().kind == TokenKind::Comma {
                self.bump();
            }
        }
        self.expect(TokenKind::RBrace, "'}'")?;
        Ok(match kind {
            "hstack" => LayoutNode::HStack(items),
            "subzone" => LayoutNode::SubZone {
                title: title.unwrap_or_default(),
                items,
            },
            _ => LayoutNode::VStack(items),
        })
    }

    fn parse_building_electrical(&mut self) -> Result<BuildingElecDecl, Diagnostic> {
        let start = self.expect_keyword("building_electrical")?;
        let (name, _) = self.expect_ident("installation name")?;
        self.expect(TokenKind::LBrace, "'{'")?;

        let mut supply_v = Quantity::new(230.0, Unit::V);
        let mut frequency_hz = Quantity::new(50.0, Unit::Hz);
        let mut ceiling_height = Quantity::mm(2700.0);
        let mut panels = Vec::new();
        let mut breakers = Vec::new();
        let mut lights = Vec::new();
        let mut switches = Vec::new();
        let mut outlets = Vec::new();

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (kw, kspan) = self.expect_ident("building_electrical declaration")?;
            match kw.as_str() {
                "supply" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (v, _) = self.parse_quantity()?;
                    supply_v = v;
                    if matches!(self.peek().kind, TokenKind::Number(_, _)) {
                        let (f, _) = self.parse_quantity()?;
                        frequency_hz = f;
                    }
                }
                "ceiling_height" => {
                    self.expect(TokenKind::Colon, "':'")?;
                    let (h, _) = self.parse_quantity()?;
                    ceiling_height = h;
                }
                "panel" => panels.push(self.parse_mep_panel(kspan)?),
                "breaker" => breakers.push(self.parse_mep_breaker(kspan)?),
                "light" => lights.push(self.parse_mep_light(kspan)?),
                "switch" => switches.push(self.parse_mep_switch(kspan)?),
                "outlet" => outlets.push(self.parse_mep_outlet(kspan)?),
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_MEP",
                        "syntax",
                        format!("Unknown declaration '{other}' inside building_electrical"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(BuildingElecDecl {
            name,
            supply_v,
            frequency_hz,
            ceiling_height,
            panels,
            breakers,
            lights,
            switches,
            outlets,
            span: start.merge(&end),
        })
    }

    fn parse_mep_host_expr(&mut self) -> Result<(Option<(String, Side)>, Option<(String, bool, Quantity)>), Diagnostic> {
        let (fn_name, fspan) = self.expect_ident("wall or beside")?;
        self.expect(TokenKind::LParen, "'('")?;
        match fn_name.as_str() {
            "wall" => {
                let room = self.parse_room_ref()?;
                self.expect(TokenKind::Comma, "','")?;
                let (side_s, sspan) = self.expect_ident("wall side")?;
                let side = parse_side(&side_s, sspan)?;
                self.expect(TokenKind::RParen, "')'")?;
                Ok((Some((room, side)), None))
            }
            "beside" => {
                let door_id = self.parse_room_ref()?;
                let mut latch_side = true;
                let mut offset = Quantity::mm(150.0);
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                    let (s_kind, _) = self.expect_ident("latch_side or hinge_side")?;
                    latch_side = s_kind != "hinge_side";
                    if self.peek().kind == TokenKind::Comma {
                        self.bump();
                        let (q, _) = self.parse_quantity()?;
                        offset = q;
                    }
                }
                self.expect(TokenKind::RParen, "')'")?;
                Ok((None, Some((door_id, latch_side, offset))))
            }
            other => Err(Diagnostic::error(
                "E_PARSE_MEP_HOST",
                "syntax",
                format!("Unknown electrical host '{other}' (expected wall(...) or beside(...))"),
                Some(fspan),
            )),
        }
    }

    fn parse_mep_panel(&mut self, start: Span) -> Result<PanelDecl, Diagnostic> {
        let (id, _) = self.expect_ident("panel id")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut wall_host: Option<(String, Side)> = None;
        let mut beside_host: Option<(String, bool, Quantity)> = None;
        let mut placement = OpeningPlacement::Center;
        let mut main_breaker = Quantity::new(63.0, Unit::A);
        let mut height = Quantity::mm(1500.0);

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (k, kspan) = self.expect_ident("panel property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match k.as_str() {
                "host" => {
                    let (w, b) = self.parse_mep_host_expr()?;
                    wall_host = w;
                    beside_host = b;
                }
                "align" => {
                    self.expect_keyword("center")?;
                    placement = OpeningPlacement::Center;
                }
                "offset" => {
                    let (dist, _) = self.parse_quantity()?;
                    self.expect_keyword("from")?;
                    let (e_s, espan) = self.expect_ident("start or end")?;
                    placement = OpeningPlacement::Offset {
                        distance: dist,
                        from: parse_wall_end(&e_s, espan)?,
                    };
                }
                "main_breaker" => {
                    let (q, _) = self.parse_quantity()?;
                    main_breaker = q;
                }
                "height" => {
                    let (q, _) = self.parse_quantity()?;
                    height = q;
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown panel property '{other}'"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        let mount = if let Some((room, side)) = wall_host {
            WallMountSpec::RoomWall { room, side, placement }
        } else if let Some((door_id, latch_side, offset)) = beside_host {
            WallMountSpec::BesideDoor { door_id, latch_side, offset }
        } else {
            return Err(Diagnostic::error(
                "E_MEP_MISSING_HOST",
                "syntax",
                format!("Panel '{id}' requires a `host: wall(Room, side)` property"),
                Some(start.merge(&end)),
            ));
        };
        Ok(PanelDecl {
            id,
            mount,
            main_breaker,
            height,
            span: start.merge(&end),
        })
    }

    fn parse_mep_breaker(&mut self, start: Span) -> Result<BreakerDecl, Diagnostic> {
        let (id, _) = self.expect_ident("breaker circuit id")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (kind, _) = self.expect_ident("breaker kind (lighting, socket, appliance, dedicated)")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut from_panel = String::new();
        let mut rating = Quantity::new(16.0, Unit::A);
        let mut cable = Quantity::new(2.5, Unit::Mm2);
        let mut rcd = None;
        let mut max_voltage_drop_pct = None;

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (k, kspan) = self.expect_ident("breaker property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match k.as_str() {
                "from" => {
                    let (p, _) = self.expect_ident("panel id")?;
                    from_panel = p;
                }
                "rating" => {
                    let (q, _) = self.parse_quantity()?;
                    rating = q;
                }
                "cable" => {
                    let (q, _) = self.parse_quantity()?;
                    cable = q;
                }
                "rcd" => {
                    let (q, _) = self.parse_quantity()?;
                    rcd = Some(q);
                }
                "max_voltage_drop" => {
                    let (q, _) = self.parse_quantity()?;
                    max_voltage_drop_pct = Some(q.raw_value);
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown breaker property '{other}'"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(BreakerDecl {
            id,
            kind,
            from_panel,
            rating,
            cable,
            rcd,
            max_voltage_drop_pct,
            span: start.merge(&end),
        })
    }

    fn parse_mep_light(&mut self, start: Span) -> Result<LightDecl, Diagnostic> {
        let (id, _) = self.expect_ident("light id")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (subtype, _) = self.expect_ident("light type (ceiling, downlight, wall_sconce)")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut room = String::new();
        let mut position = RoomPointSpec::Center;
        let mut load = Quantity::new(18.0, Unit::W);
        let mut circuit = String::new();
        let mut ip_rating = None;

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (k, kspan) = self.expect_ident("light property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match k.as_str() {
                "room" => room = self.parse_room_ref()?,
                "at" => {
                    if self.peek().kind == TokenKind::LParen {
                        self.bump();
                        let (x, _) = self.parse_quantity()?;
                        self.expect(TokenKind::Comma, "','")?;
                        let (y, _) = self.parse_quantity()?;
                        self.expect(TokenKind::RParen, "')'")?;
                        position = RoomPointSpec::OffsetXY { x, y };
                    } else {
                        self.expect_keyword("center")?;
                        position = RoomPointSpec::Center;
                    }
                }
                "load" => {
                    let (q, _) = self.parse_quantity()?;
                    load = q;
                }
                "circuit" => {
                    let (c, _) = self.expect_ident("circuit id")?;
                    circuit = c;
                }
                "ip" => {
                    let (ip, _) = self.expect_ident("IP rating")?;
                    ip_rating = Some(ip);
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown light property '{other}'"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        Ok(LightDecl {
            id,
            subtype,
            room,
            position,
            load,
            circuit,
            ip_rating,
            span: start.merge(&end),
        })
    }

    fn parse_mep_switch(&mut self, start: Span) -> Result<SwitchDecl, Diagnostic> {
        let (id, _) = self.expect_ident("switch id")?;
        self.expect(TokenKind::Colon, "':'")?;
        let subtype = match self.peek().kind.clone() {
            // `1gang` / `2gang` lex as Number + unit suffix; accept them as switch kinds.
            TokenKind::Number(n, unit) if unit == "gang" || unit == "way" => {
                self.bump();
                format!("{}{}", n as i64, unit)
            }
            _ => self.expect_ident("switch type (1gang, 2gang, 2way, dimmer)")?.0,
        };
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut room = String::new();
        let mut wall_host: Option<(String, Side)> = None;
        let mut beside_host: Option<(String, bool, Quantity)> = None;
        let mut placement = OpeningPlacement::Center;
        let mut controls = Vec::new();
        let mut circuit = String::new();
        let mut height = Quantity::mm(1200.0);
        let mut ip_rating = None;

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (k, kspan) = self.expect_ident("switch property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match k.as_str() {
                "room" => room = self.parse_room_ref()?,
                "host" => {
                    let (w, b) = self.parse_mep_host_expr()?;
                    if let Some((ref r, _)) = w {
                        if room.is_empty() {
                            room = r.clone();
                        }
                    }
                    wall_host = w;
                    beside_host = b;
                }
                "align" => {
                    self.expect_keyword("center")?;
                    placement = OpeningPlacement::Center;
                }
                "offset" => {
                    let (dist, _) = self.parse_quantity()?;
                    self.expect_keyword("from")?;
                    let (e_s, espan) = self.expect_ident("start or end")?;
                    placement = OpeningPlacement::Offset {
                        distance: dist,
                        from: parse_wall_end(&e_s, espan)?,
                    };
                }
                "controls" => {
                    self.expect(TokenKind::LBracket, "'['")?;
                    while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                        let (lid, _) = self.expect_ident("light id")?;
                        controls.push(lid);
                        if self.peek().kind == TokenKind::Comma {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBracket, "']'")?;
                }
                "circuit" => {
                    let (c, _) = self.expect_ident("circuit id")?;
                    circuit = c;
                }
                "height" => {
                    let (q, _) = self.parse_quantity()?;
                    height = q;
                }
                "ip" => {
                    let (ip, _) = self.expect_ident("IP rating")?;
                    ip_rating = Some(ip);
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown switch property '{other}'"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        let mount = if let Some((r, side)) = wall_host {
            WallMountSpec::RoomWall { room: r, side, placement }
        } else if let Some((door_id, latch_side, offset)) = beside_host {
            WallMountSpec::BesideDoor { door_id, latch_side, offset }
        } else {
            return Err(Diagnostic::error(
                "E_MEP_MISSING_HOST",
                "syntax",
                format!("Switch '{id}' requires a `host: wall(...)` or `host: beside(...)` property"),
                Some(start.merge(&end)),
            ));
        };
        Ok(SwitchDecl {
            id,
            subtype,
            room,
            mount,
            controls,
            circuit,
            height,
            ip_rating,
            span: start.merge(&end),
        })
    }

    fn parse_mep_outlet(&mut self, start: Span) -> Result<OutletDecl, Diagnostic> {
        let (id, _) = self.expect_ident("outlet id")?;
        self.expect(TokenKind::Colon, "':'")?;
        let (subtype, _) = self.expect_ident("outlet type (duplex, single, cooker, shaver)")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut wall_host: Option<(String, Side)> = None;
        let mut beside_host: Option<(String, bool, Quantity)> = None;
        let mut placement = OpeningPlacement::Center;
        let mut height = Quantity::mm(300.0);
        let mut load = Quantity::new(300.0, Unit::W);
        let mut circuit = String::new();
        let mut ip_rating = None;

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let (k, kspan) = self.expect_ident("outlet property")?;
            self.expect(TokenKind::Colon, "':'")?;
            match k.as_str() {
                "host" => {
                    let (w, b) = self.parse_mep_host_expr()?;
                    wall_host = w;
                    beside_host = b;
                }
                "align" => {
                    self.expect_keyword("center")?;
                    placement = OpeningPlacement::Center;
                }
                "offset" => {
                    let (dist, _) = self.parse_quantity()?;
                    self.expect_keyword("from")?;
                    let (e_s, espan) = self.expect_ident("start or end")?;
                    placement = OpeningPlacement::Offset {
                        distance: dist,
                        from: parse_wall_end(&e_s, espan)?,
                    };
                }
                "height" => {
                    let (q, _) = self.parse_quantity()?;
                    height = q;
                }
                "load" => {
                    let (q, _) = self.parse_quantity()?;
                    load = q;
                }
                "circuit" => {
                    let (c, _) = self.expect_ident("circuit id")?;
                    circuit = c;
                }
                "ip" => {
                    let (ip, _) = self.expect_ident("IP rating")?;
                    ip_rating = Some(ip);
                }
                other => {
                    return Err(Diagnostic::error(
                        "E_PARSE_PROP",
                        "syntax",
                        format!("Unknown outlet property '{other}'"),
                        Some(kspan),
                    ))
                }
            }
        }
        let end = self.expect(TokenKind::RBrace, "'}'")?.span(&self.file);
        let mount = if let Some((room, side)) = wall_host {
            WallMountSpec::RoomWall { room, side, placement }
        } else if let Some((door_id, latch_side, offset)) = beside_host {
            WallMountSpec::BesideDoor { door_id, latch_side, offset }
        } else {
            return Err(Diagnostic::error(
                "E_MEP_MISSING_HOST",
                "syntax",
                format!("Outlet '{id}' requires a `host: wall(Room, side)` property"),
                Some(start.merge(&end)),
            ));
        };
        Ok(OutletDecl {
            id,
            subtype,
            mount,
            height,
            load,
            circuit,
            ip_rating,
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
        "zone_local" | "auto_zone" => Ok(NetRoutingStyle::ZoneLocal),
        _ => Err(Diagnostic::error(
            "E_PARSE_ROUTING_STYLE",
            "syntax",
            format!("Unknown routing style '{s}' (expected direct, bus, rail, symbol, or label_stub)"),
            Some(span),
        )),
    }
}
