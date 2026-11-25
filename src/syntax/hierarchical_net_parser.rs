/// Phase 1.2 Week 1: Parser Implementation for Hierarchical Nets
/// 
/// This module extends the lexer and parser to support hierarchical net naming:
/// - Format: /Module/Net (e.g., /PowerModule/VOUT)
/// - Backward compatible with flat nets: GND, VBUS, etc.
/// - Prevents accidental net merging across module boundaries

use std::str::Chars;
use std::iter::Peekable;

/// Token types for EDL lexer (extend existing TokenType enum)
#[derive(Clone, Debug, PartialEq)]
pub enum TokenType {
    // Existing tokens...
    Identifier(String),
    
    // NEW: Slash for hierarchical names
    Slash,  // /
    
    // ... other tokens
}

/// Extended identifier rules for hierarchical net names
pub struct LexerContext {
    input: Peekable<Chars<'static>>,
    current_pos: usize,
    current_line: usize,
    current_col: usize,
}

impl LexerContext {
    /// Scan an identifier or hierarchical net name
    /// 
    /// Rules:
    /// - Starts with letter or underscore
    /// - Can be followed by: letters, digits, underscores
    /// - CAN be preceded by `/` for hierarchical names
    /// - Examples: GND, PowerModule, /PowerModule/VOUT
    pub fn scan_identifier(&mut self) -> Result<String, LexError> {
        let mut name = String::new();
        let mut is_hierarchical = false;
        
        // Check for leading / (hierarchical net)
        if self.peek() == Some('/') {
            is_hierarchical = true;
            self.advance(); // consume /
        }
        
        // First identifier part (before /, or entire name if flat)
        self.scan_identifier_part(&mut name)?;
        
        // Check for second / (hierarchical continuation)
        if is_hierarchical && self.peek() == Some('/') {
            self.advance(); // consume /
            name.push('/');
            
            // Second identifier part (after /)
            self.scan_identifier_part(&mut name)?;
            
            // Hierarchical nets must have exactly 2 parts: /Module/Net
            // Don't allow /Module/Net/Something (future v3.2)
            if self.peek() == Some('/') {
                return Err(LexError::InvalidHierarchicalName(
                    "Hierarchical nets support only /Module/Net format (2 levels)".to_string()
                ));
            }
        } else if is_hierarchical {
            // Started with / but no second part
            return Err(LexError::InvalidHierarchicalName(
                "Hierarchical net names must have format /Module/Net".to_string()
            ));
        }
        
        Ok(name)
    }
    
    /// Scan a single identifier part (without slashes)
    /// 
    /// Valid characters:
    /// - First: a-z, A-Z, _
    /// - Rest: a-z, A-Z, 0-9, _
    fn scan_identifier_part(&mut self, name: &mut String) -> Result<(), LexError> {
        // First character must be letter or underscore
        match self.peek() {
            Some(c) if c.is_alphabetic() || c == '_' => {
                name.push(c);
                self.advance();
            }
            _ => return Err(LexError::InvalidIdentifier(
                "Identifier must start with letter or underscore".to_string()
            )),
        }
        
        // Subsequent characters: letters, digits, underscores
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                name.push(c);
                self.advance();
            } else {
                break;
            }
        }
        
        Ok(())
    }
    
    /// Peek at next character without consuming
    fn peek(&mut self) -> Option<char> {
        self.input.peek().copied()
    }
    
    /// Consume and return next character
    fn advance(&mut self) {
        if let Some(c) = self.input.next() {
            self.current_col += 1;
            if c == '\n' {
                self.current_line += 1;
                self.current_col = 0;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum LexError {
    InvalidIdentifier(String),
    InvalidHierarchicalName(String),
}

// ============================================================================
// AST EXTENSIONS
// ============================================================================

/// Hierarchical net name representation in AST
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HierarchicalNetName {
    /// Module path for hierarchical nets: Some("PowerModule") for /PowerModule/VOUT
    /// None for flat nets: GND, VBUS, etc.
    pub module_path: Option<String>,
    
    /// Net name: "VOUT" for /PowerModule/VOUT, or "GND" for flat net
    pub net_name: String,
    
    /// Full name: "/PowerModule/VOUT" or "GND"
    pub full_name: String,
}

impl HierarchicalNetName {
    /// Parse a net name, detecting if it's hierarchical
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        if !input.starts_with('/') {
            // Flat net name (legacy)
            return Ok(HierarchicalNetName {
                module_path: None,
                net_name: input.to_string(),
                full_name: input.to_string(),
            });
        }
        
        // Hierarchical net: /Module/Net
        let parts: Vec<&str> = input
            .trim_start_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        
        if parts.len() != 2 {
            return Err(ParseError::InvalidNetName(
                format!("Hierarchical nets must be /Module/Net, got: {}", input)
            ));
        }
        
        Ok(HierarchicalNetName {
            module_path: Some(parts[0].to_string()),
            net_name: parts[1].to_string(),
            full_name: input.to_string(),
        })
    }
    
    /// Check if this is a hierarchical net
    pub fn is_hierarchical(&self) -> bool {
        self.module_path.is_some()
    }
    
    /// Get the full scoped name: /Module/Net or just Net
    pub fn full_name(&self) -> &str {
        &self.full_name
    }
    
    /// Get just the net name part (without module scope)
    pub fn net_name(&self) -> &str {
        &self.net_name
    }
}

#[derive(Clone, Debug)]
pub enum ParseError {
    InvalidNetName(String),
}

// ============================================================================
// PARSER EXTENSIONS
// ============================================================================

/// Extended parser for net declarations with hierarchical names
pub struct NetParser;

impl NetParser {
    /// Parse a net declaration with hierarchical name support
    /// 
    /// Syntax examples:
    /// - net GND { ... }
    /// - net VBUS { ... }
    /// - net /PowerModule/VOUT { ... }
    /// - net /StepDown/VCORE { ... }
    pub fn parse_net_declaration(tokens: &[Token]) -> Result<NetDeclaration, ParseError> {
        let mut pos = 0;
        
        // Expect 'net' keyword
        if pos >= tokens.len() || tokens[pos].token_type != TokenType::Net {
            return Err(ParseError::InvalidNetName("Expected 'net' keyword".to_string()));
        }
        pos += 1;
        
        // Expect net name (identifier or hierarchical)
        let net_name = match &tokens.get(pos).map(|t| &t.token_type) {
            Some(TokenType::Identifier(name)) => {
                // Check if hierarchical
                // Note: Lexer should have already parsed /Module/Net as single identifier
                HierarchicalNetName::parse(name)?
            }
            _ => return Err(ParseError::InvalidNetName(
                "Expected net name after 'net' keyword".to_string()
            )),
        };
        pos += 1;
        
        // Expect opening brace
        if pos >= tokens.len() || tokens[pos].token_type != TokenType::OpenBrace {
            return Err(ParseError::InvalidNetName(
                "Expected '{' after net name".to_string()
            ));
        }
        pos += 1;
        
        // Parse net body (connections, attributes, etc.)
        let mut body_tokens = Vec::new();
        let mut brace_depth = 1;
        
        while pos < tokens.len() && brace_depth > 0 {
            match &tokens[pos].token_type {
                TokenType::OpenBrace => brace_depth += 1,
                TokenType::CloseBrace => brace_depth -= 1,
                _ => {}
            }
            
            if brace_depth > 0 {
                body_tokens.push(tokens[pos].clone());
            }
            pos += 1;
        }
        
        Ok(NetDeclaration {
            name: net_name,
            body: body_tokens,
            is_hierarchical: net_name.is_hierarchical(),
        })
    }
}

/// Representation of a parsed net declaration
#[derive(Clone, Debug)]
pub struct NetDeclaration {
    pub name: HierarchicalNetName,
    pub body: Vec<Token>,
    pub is_hierarchical: bool,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hierarchical_net_parsing() {
        let net = HierarchicalNetName::parse("/PowerModule/VOUT").unwrap();
        assert_eq!(net.module_path, Some("PowerModule".to_string()));
        assert_eq!(net.net_name, "VOUT");
        assert_eq!(net.full_name, "/PowerModule/VOUT");
        assert!(net.is_hierarchical());
    }
    
    #[test]
    fn test_flat_net_parsing() {
        let net = HierarchicalNetName::parse("GND").unwrap();
        assert_eq!(net.module_path, None);
        assert_eq!(net.net_name, "GND");
        assert_eq!(net.full_name, "GND");
        assert!(!net.is_hierarchical());
    }
    
    #[test]
    fn test_hierarchical_net_naming_patterns() {
        let patterns = vec![
            "/PowerModule/VOUT",
            "/StepDown/VCORE",
            "/GPIO_Bank1/Output",
            "/SPI_Master/MOSI",
            "/I2C_Primary/SCL",
        ];
        
        for pattern in patterns {
            let net = HierarchicalNetName::parse(pattern).unwrap();
            assert!(net.is_hierarchical());
            assert!(net.full_name.starts_with('/'));
        }
    }
    
    #[test]
    fn test_invalid_hierarchical_format() {
        // Too many levels
        let result = HierarchicalNetName::parse("/Level1/Level2/Level3");
        assert!(result.is_err());
        
        // No closing part
        let result = HierarchicalNetName::parse("/PowerModule/");
        assert!(result.is_err());
        
        // No opening part
        let result = HierarchicalNetName::parse("/");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_net_isolation_between_modules() {
        let net1 = HierarchicalNetName::parse("/PowerModule/GND").unwrap();
        let net2 = HierarchicalNetName::parse("/AudioModule/GND").unwrap();
        let net3 = HierarchicalNetName::parse("GND").unwrap();
        
        // Different modules = different nets
        assert_ne!(net1.full_name, net2.full_name);
        
        // Hierarchical != flat
        assert_ne!(net1.full_name, net3.full_name);
        
        // But they have the same net name part
        assert_eq!(net1.net_name, net2.net_name);
        assert_eq!(net2.net_name, net3.net_name);
    }
    
    #[test]
    fn test_backward_compatibility() {
        let flat_nets = vec!["GND", "VBUS", "VCORE", "VOUT", "Signal_A"];
        
        for net_name in flat_nets {
            let net = HierarchicalNetName::parse(net_name).unwrap();
            assert!(!net.is_hierarchical());
            assert_eq!(net.full_name, net_name);
        }
    }
}
