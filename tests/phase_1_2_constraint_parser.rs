/// Phase 1.2 Constraint Parser Tests
/// Validates lexer extensions and parser for constraint declarations

#[cfg(test)]
mod constraint_parser_tests {
    use crate::syntax::lexer::{lex, TokenKind};
    use crate::syntax::parser::parse_project;

    #[test]
    fn test_lexer_constraint_keywords() {
        let src = "constraint target rules standard description mandatory strong weak";
        let tokens = lex("test.edl", src).expect("lexer failed");
        
        assert_eq!(tokens[0].kind, TokenKind::Constraint);
        assert_eq!(tokens[1].kind, TokenKind::Target);
        assert_eq!(tokens[2].kind, TokenKind::Rules);
        assert_eq!(tokens[3].kind, TokenKind::Standard);
        assert_eq!(tokens[4].kind, TokenKind::Description);
        assert_eq!(tokens[5].kind, TokenKind::Mandatory);
        assert_eq!(tokens[6].kind, TokenKind::Strong);
        assert_eq!(tokens[7].kind, TokenKind::Weak);
    }

    #[test]
    fn test_lexer_comparison_operators() {
        let src = "< <= > >= == !=";
        let tokens = lex("test.edl", src).expect("lexer failed");
        
        assert_eq!(tokens[0].kind, TokenKind::Lt);
        assert_eq!(tokens[1].kind, TokenKind::Le);
        assert_eq!(tokens[2].kind, TokenKind::Gt);
        assert_eq!(tokens[3].kind, TokenKind::Ge);
        assert_eq!(tokens[4].kind, TokenKind::EqEq);
        assert_eq!(tokens[5].kind, TokenKind::Ne);
    }

    #[test]
    fn test_lexer_tolerance_operator() {
        let src = "3.3V +- 5%";
        let tokens = lex("test.edl", src).expect("lexer failed");
        
        assert!(matches!(tokens[0].kind, TokenKind::Number(3.3, _)));
        assert_eq!(tokens[1].kind, TokenKind::PlusMinus);
        assert!(matches!(tokens[2].kind, TokenKind::Number(5.0, _)));
    }

    #[test]
    fn test_parser_simple_constraint() {
        let edl = r#"
            project "TestConstraint" {
                constraint voltage_vcc: mandatory {
                    target: net:VCC
                    rules: {
                        operating_voltage: == 3.3V
                    }
                }
            }
        "#;
        
        let result = parse_project("test.edl", edl);
        assert!(result.is_ok(), "Parser failed: {:?}", result.err());
        
        let proj = result.unwrap();
        assert!(!proj.constraints.is_empty());
        
        let constraint = &proj.constraints[0];
        assert_eq!(constraint.id, "voltage_vcc");
        assert_eq!(constraint.rules.len(), 1);
    }

    #[test]
    fn test_parser_constraint_with_tolerance() {
        let edl = r#"
            project "TestConstraint" {
                constraint voltage_vcc: mandatory {
                    target: net:VCC
                    rules: {
                        operating_voltage: 3.3V +- 5%
                    }
                }
            }
        "#;
        
        let result = parse_project("test.edl", edl);
        assert!(result.is_ok(), "Parser failed: {:?}", result.err());
        
        let proj = result.unwrap();
        let constraint = &proj.constraints[0];
        let rule = &constraint.rules[0];
        
        assert!(rule.tolerance.is_some());
        let tol = rule.tolerance.as_ref().unwrap();
        assert_eq!(tol.value, 5.0);
        assert_eq!(tol.unit, "%");
    }

    #[test]
    fn test_parser_constraint_with_range() {
        let edl = r#"
            project "TestConstraint" {
                constraint trace_width: strong {
                    target: net:VCC
                    rules: {
                        width: in [2.0mm, 5.0mm]
                    }
                }
            }
        "#;
        
        let result = parse_project("test.edl", edl);
        assert!(result.is_ok(), "Parser failed: {:?}", result.err());
        
        let proj = result.unwrap();
        let constraint = &proj.constraints[0];
        assert_eq!(constraint.id, "trace_width");
    }

    #[test]
    fn test_parser_constraint_with_metadata() {
        let edl = r#"
            project "TestConstraint" {
                constraint voltage_vcc: mandatory {
                    target: net:VCC
                    rules: {
                        operating_voltage: == 3.3V
                    }
                    standard: "IEC 60950-1"
                    description: "Ensure VCC doesn't exceed rated limits"
                }
            }
        "#;
        
        let result = parse_project("test.edl", edl);
        assert!(result.is_ok(), "Parser failed: {:?}", result.err());
        
        let proj = result.unwrap();
        let constraint = &proj.constraints[0];
        
        assert_eq!(constraint.standard, Some("IEC 60950-1".to_string()));
        assert_eq!(constraint.description, Some("Ensure VCC doesn't exceed rated limits".to_string()));
    }

    #[test]
    fn test_parser_multiple_rules() {
        let edl = r#"
            project "TestConstraint" {
                constraint elec_e1: mandatory {
                    target: component:IC1
                    rules: {
                        operating_voltage: >= 3.3V,
                        maximum_voltage: <= 3.6V,
                        power_dissipation: <= 2.5W
                    }
                }
            }
        "#;
        
        let result = parse_project("test.edl", edl);
        assert!(result.is_ok(), "Parser failed: {:?}", result.err());
        
        let proj = result.unwrap();
        let constraint = &proj.constraints[0];
        assert_eq!(constraint.rules.len(), 3);
        assert_eq!(constraint.rules[0].parameter, "operating_voltage");
        assert_eq!(constraint.rules[1].parameter, "maximum_voltage");
        assert_eq!(constraint.rules[2].parameter, "power_dissipation");
    }

    #[test]
    fn test_parser_multiple_constraints() {
        let edl = r#"
            project "TestConstraint" {
                constraint voltage_vcc: mandatory {
                    target: net:VCC
                    rules: {
                        operating_voltage: == 3.3V
                    }
                }
                
                constraint voltage_gnd: mandatory {
                    target: net:GND
                    rules: {
                        operating_voltage: == 0V
                    }
                }
            }
        "#;
        
        let result = parse_project("test.edl", edl);
        assert!(result.is_ok(), "Parser failed: {:?}", result.err());
        
        let proj = result.unwrap();
        assert_eq!(proj.constraints.len(), 2);
    }

    #[test]
    fn test_parser_hierarchical_entity_target() {
        let edl = r#"
            project "TestConstraint" {
                constraint trace_impedance: strong {
                    target: net:PowerModule.VOUT
                    rules: {
                        impedance: == 50.0Ohm
                    }
                }
            }
        "#;
        
        let result = parse_project("test.edl", edl);
        assert!(result.is_ok(), "Parser failed: {:?}", result.err());
    }

    #[test]
    fn test_parser_severity_levels() {
        let edl = r#"
            project "TestConstraint" {
                constraint rule1: mandatory {
                    target: net:V1
                    rules: { param: == 1.0V }
                }
                
                constraint rule2: strong {
                    target: net:V2
                    rules: { param: == 2.0V }
                }
                
                constraint rule3: weak {
                    target: net:V3
                    rules: { param: == 3.0V }
                }
            }
        "#;
        
        let result = parse_project("test.edl", edl);
        assert!(result.is_ok(), "Parser failed: {:?}", result.err());
        
        let proj = result.unwrap();
        assert_eq!(proj.constraints.len(), 3);
    }

    #[test]
    fn test_parser_comparison_operators() {
        let edl = r#"
            project "TestConstraint" {
                constraint rule1: mandatory {
                    target: net:V1
                    rules: {
                        param1: == 1.0V,
                        param2: != 2.0V,
                        param3: < 3.0V,
                        param4: <= 4.0V,
                        param5: > 5.0V,
                        param6: >= 6.0V
                    }
                }
            }
        "#;
        
        let result = parse_project("test.edl", edl);
        assert!(result.is_ok(), "Parser failed: {:?}", result.err());
        
        let proj = result.unwrap();
        let constraint = &proj.constraints[0];
        assert_eq!(constraint.rules.len(), 6);
    }
}
