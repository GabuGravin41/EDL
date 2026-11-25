/// Constraint parsing for EDL Phase 1.2
/// Parses declarative constraint syntax into ConstraintDecl AST nodes

use crate::core::span::{Diagnostic, Span};
use crate::syntax::constraints_ast::*;
use crate::syntax::lexer::{Token, TokenKind};
use crate::syntax::parser::Parser;

impl Parser {
    /// Parse a constraint declaration
    /// constraint <id> : <severity> { target: ... rules: { ... } }
    pub fn parse_constraint(&mut self) -> Result<ConstraintDecl, Diagnostic> {
        let start = self.expect_keyword("constraint")?;
        let (id, _) = self.expect_ident("constraint identifier")?;
        
        self.expect(TokenKind::Colon, "':' after constraint id")?;
        
        // Parse severity: mandatory | strong | weak
        let severity = self.parse_constraint_severity()?;
        
        self.expect(TokenKind::LBrace, "'{' to start constraint body")?;
        
        // Parse constraint body (target, rules, standard, description)
        let mut target = None;
        let mut rules = Vec::new();
        let mut standard = None;
        let mut description = None;
        
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let field_tok = self.peek().clone();
            let field_name = match &field_tok.kind {
                TokenKind::Target => "target",
                TokenKind::Rules => "rules",
                TokenKind::Standard => "standard",
                TokenKind::Description => "description",
                TokenKind::Ident(s) => s.as_str(),
                _ => {
                    return Err(self.err(
                        "E_PARSE_CONSTRAINT_FIELD",
                        format!("Expected constraint field (target, rules, standard, description), found {:?}", field_tok.kind),
                        &field_tok,
                    ))
                }
            };
            
            match field_name {
                "target" => {
                    self.bump(); // consume 'target'
                    self.expect(TokenKind::Colon, "':' after 'target'")?;
                    target = Some(self.parse_constraint_target()?);
                    if self.peek().kind == TokenKind::Comma {
                        self.bump();
                    }
                }
                "rules" => {
                    self.bump(); // consume 'rules'
                    self.expect(TokenKind::Colon, "':' after 'rules'")?;
                    rules = self.parse_constraint_rules()?;
                    if self.peek().kind == TokenKind::Comma {
                        self.bump();
                    }
                }
                "standard" => {
                    self.bump(); // consume 'standard'
                    self.expect(TokenKind::Colon, "':' after 'standard'")?;
                    let (std, _) = self.expect_string("standard name")?;
                    standard = Some(std);
                    if self.peek().kind == TokenKind::Comma {
                        self.bump();
                    }
                }
                "description" => {
                    self.bump(); // consume 'description'
                    self.expect(TokenKind::Colon, "':' after 'description'")?;
                    let (desc, _) = self.expect_string("description text")?;
                    description = Some(desc);
                    if self.peek().kind == TokenKind::Comma {
                        self.bump();
                    }
                }
                other => {
                    return Err(self.err(
                        "E_PARSE_CONSTRAINT_FIELD",
                        format!("Unknown constraint field '{other}'"),
                        &field_tok,
                    ))
                }
            }
        }
        
        self.expect(TokenKind::RBrace, "'}' to end constraint")?;
        
        // Validate required fields
        let target = target.ok_or_else(|| {
            self.err(
                "E_PARSE_CONSTRAINT_NO_TARGET",
                "Constraint missing required 'target' field",
                &start,
            )
        })?;
        
        if rules.is_empty() {
            return Err(self.err(
                "E_PARSE_CONSTRAINT_NO_RULES",
                "Constraint missing required 'rules' field",
                &start,
            ));
        }
        
        Ok(ConstraintDecl {
            id,
            severity,
            target,
            rules,
            standard,
            description,
            span: start.merge(&self.peek()),
        })
    }
    
    /// Parse constraint severity: mandatory | strong | weak
    fn parse_constraint_severity(&mut self) -> Result<ConstraintSeverity, Diagnostic> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Mandatory => {
                self.bump();
                Ok(ConstraintSeverity::Mandatory)
            }
            TokenKind::Strong => {
                self.bump();
                Ok(ConstraintSeverity::Strong)
            }
            TokenKind::Weak => {
                self.bump();
                Ok(ConstraintSeverity::Weak)
            }
            _ => Err(self.err(
                "E_PARSE_CONSTRAINT_SEVERITY",
                format!("Expected 'mandatory', 'strong', or 'weak', found {:?}", tok.kind),
                &tok,
            )),
        }
    }
    
    /// Parse constraint target: net:NAME | component:NAME | pipe:NAME | fixture:NAME | group:NAME
    fn parse_constraint_target(&mut self) -> Result<ConstraintTarget, Diagnostic> {
        let (entity_type, _) = self.expect_ident("entity type")?;
        self.expect(TokenKind::Colon, "':' after entity type")?;
        let (entity_name, _) = self.expect_ident("entity name")?;
        
        // Optional hierarchical reference: component:IC1/Pin1 or net:Module1/VCC
        let mut path = vec![entity_name];
        while self.peek().kind == TokenKind::Dot {
            self.bump();
            let (name, _) = self.expect_ident("hierarchy path component")?;
            path.push(name);
        }
        
        match entity_type.as_str() {
            "net" => Ok(ConstraintTarget::Net(path.join("."))),
            "component" => Ok(ConstraintTarget::Component(path.join("."))),
            "pipe" => Ok(ConstraintTarget::Pipe(path.join("."))),
            "fixture" => Ok(ConstraintTarget::Fixture(path.join("."))),
            "group" => Ok(ConstraintTarget::Group(path.join("."))),
            other => Err(self.err(
                "E_PARSE_CONSTRAINT_TARGET",
                format!("Unknown entity type '{other}' (expected net, component, pipe, fixture, or group)"),
                self.peek(),
            )),
        }
    }
    
    /// Parse constraint rules block: { param1: op value, param2: op value, ... }
    fn parse_constraint_rules(&mut self) -> Result<Vec<ConstraintRule>, Diagnostic> {
        let mut rules = Vec::new();
        
        self.expect(TokenKind::LBrace, "'{' to start rules")?;
        
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let rule = self.parse_constraint_rule()?;
            rules.push(rule);
            
            if self.peek().kind == TokenKind::Comma {
                self.bump();
            } else if self.peek().kind != TokenKind::RBrace {
                return Err(self.err(
                    "E_PARSE_CONSTRAINT_RULE",
                    "Expected ',' or '}' after constraint rule",
                    self.peek(),
                ));
            }
        }
        
        self.expect(TokenKind::RBrace, "'}' to end rules")?;
        
        Ok(rules)
    }
    
    /// Parse a single constraint rule: parameter: operator value [tolerance]
    /// Examples:
    ///   operating_voltage: == 3.3V
    ///   operating_voltage: 3.3V +- 5%
    ///   trace_width: >= 2.5mm
    ///   height: in [17in, 19in]
    fn parse_constraint_rule(&mut self) -> Result<ConstraintRule, Diagnostic> {
        let (param, _) = self.expect_ident("rule parameter")?;
        self.expect(TokenKind::Colon, "':' after parameter name")?;
        
        // Parse operator and value
        let (operator, value, tolerance) = self.parse_constraint_value()?;
        
        Ok(ConstraintRule {
            parameter: param,
            operator,
            value,
            tolerance,
            span: self.peek().span(&self.file),
        })
    }
    
    /// Parse constraint value with optional tolerance
    /// Formats:
    ///   == 3.3V
    ///   3.3V +- 5%
    ///   >= 2.5mm
    ///   in [17in, 19in]
    fn parse_constraint_value(&mut self) -> Result<(ConstraintOp, ConstraintValue, Option<ConstraintTolerance>), Diagnostic> {
        let tok = self.peek().clone();
        
        // Check for explicit operator (==, !=, <, <=, >, >=, in)
        let operator = if matches!(tok.kind, TokenKind::EqEq | TokenKind::Ne | TokenKind::Lt | TokenKind::Le | TokenKind::Gt | TokenKind::Ge) {
            let op = match &tok.kind {
                TokenKind::EqEq => ConstraintOp::Equal,
                TokenKind::Ne => ConstraintOp::NotEqual,
                TokenKind::Lt => ConstraintOp::LessThan,
                TokenKind::Le => ConstraintOp::LessThanOrEqual,
                TokenKind::Gt => ConstraintOp::GreaterThan,
                TokenKind::Ge => ConstraintOp::GreaterThanOrEqual,
                _ => unreachable!(),
            };
            self.bump();
            op
        } else if let TokenKind::Ident(s) = &tok.kind {
            if s == "in" {
                self.bump();
                ConstraintOp::In
            } else {
                // Implicit == operator
                ConstraintOp::Equal
            }
        } else {
            // Implicit == operator
            ConstraintOp::Equal
        };
        
        // Parse value
        let (value, tolerance) = if operator == ConstraintOp::In {
            // Range constraint: in [min, max]
            self.expect(TokenKind::LBracket, "'[' for range")?;
            let (min_num, min_unit) = self.expect_number("range minimum")?;
            self.expect(TokenKind::Comma, "',' in range")?;
            let (max_num, max_unit) = self.expect_number("range maximum")?;
            self.expect(TokenKind::RBracket, "']' to close range")?;
            
            let value = ConstraintValue::Range(
                ConstraintRange {
                    min: (min_num, min_unit),
                    max: (max_num, max_unit),
                },
            );
            (value, None)
        } else {
            let (num, unit) = self.expect_number("constraint value")?;
            let value = ConstraintValue::Numeric(num, unit.clone());
            
            // Check for tolerance: +- value
            let tolerance = if self.peek().kind == TokenKind::PlusMinus {
                self.bump();
                let (tol_num, tol_unit) = self.expect_number("tolerance value")?;
                Some(ConstraintTolerance {
                    value: tol_num,
                    unit: tol_unit,
                })
            } else {
                None
            };
            
            (value, tolerance)
        };
        
        Ok((operator, value, tolerance))
    }
    
    /// Expect and parse a number (with optional unit)
    fn expect_number(&mut self, label: &str) -> Result<(f64, String), Diagnostic> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Number(val, unit) => {
                self.bump();
                Ok((*val, unit.clone()))
            }
            _ => Err(self.err(
                "E_PARSE_NUMBER",
                format!("Expected number for {label}, found {:?}", tok.kind),
                &tok,
            )),
        }
    }
}
