/// Phase 1.2 Week 2: Constraint Elaboration Tests
/// Validates AST elaboration and entity reference resolution

#[cfg(test)]
mod constraint_elaboration_tests {
    use crate::core::units::Unit;
    use crate::core::span::Span;
    use crate::syntax::constraint_elaborator::{ConstraintElaborator, ResolvedEntityRef};
    use crate::syntax::constraints_ast::{ConstraintDecl, ConstraintTarget, ConstraintSeverity, ConstraintRule, ConstraintOp, ConstraintValue};
    use crate::syntax::ast::ProjectAst;
    use crate::domains::arch::ast::ArchProjectData;
    use crate::domains::elec::ast::{ElecProjectData, NetDecl};
    use crate::domains::mep_elec::ast::MepElecProjectData;
    
    fn create_test_project() -> ProjectAst {
        ProjectAst {
            name: "TestProject".to_string(),
            span: Span::new("test.edl", 1, 1, 1, 100),
            default_unit: Unit::Mm,
            imports: vec![],
            modules: vec![],
            uses: vec![],
            frames: vec![],
            parameters: vec![],
            arch: ArchProjectData::default(),
            elec: ElecProjectData {
                nets: vec![
                    NetDecl { name: "VCC".to_string(), span: Span::new("test.edl", 10, 1, 10, 10) },
                    NetDecl { name: "GND".to_string(), span: Span::new("test.edl", 11, 1, 11, 10) },
                ],
                ..Default::default()
            },
            mep_elec: MepElecProjectData::default(),
            extensions: vec![],
            verifications: vec![],
            views: vec![],
            constraints: vec![],
        }
    }
    
    #[test]
    fn test_elaborate_simple_constraint() {
        let mut project = create_test_project();
        
        project.constraints.push(ConstraintDecl {
            id: "voltage_vcc".to_string(),
            severity: ConstraintSeverity::Mandatory,
            target: ConstraintTarget::Net("VCC".to_string()),
            rules: vec![
                ConstraintRule {
                    parameter: "operating_voltage".to_string(),
                    operator: ConstraintOp::Equal,
                    value: ConstraintValue::Numeric(3.3, "V".to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 15, 1, 15, 30),
                }
            ],
            standard: Some("IEC 60950-1".to_string()),
            description: Some("Ensure VCC at 3.3V".to_string()),
            span: Span::new("test.edl", 12, 1, 20, 1),
        });
        
        let result = ConstraintElaborator::elaborate_constraints(&project);
        assert!(result.is_ok());
        
        let elaborated = result.unwrap();
        assert_eq!(elaborated.len(), 1);
        
        let constraint = &elaborated[0];
        assert_eq!(constraint.id, "voltage_vcc");
        assert_eq!(constraint.entity_ref, ResolvedEntityRef::Net("VCC".to_string()));
        assert_eq!(constraint.rules.len(), 1);
    }
    
    #[test]
    fn test_elaborate_multiple_constraints() {
        let mut project = create_test_project();
        
        project.constraints.push(ConstraintDecl {
            id: "voltage_vcc".to_string(),
            severity: ConstraintSeverity::Mandatory,
            target: ConstraintTarget::Net("VCC".to_string()),
            rules: vec![
                ConstraintRule {
                    parameter: "operating_voltage".to_string(),
                    operator: ConstraintOp::Equal,
                    value: ConstraintValue::Numeric(3.3, "V".to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 15, 1, 15, 30),
                }
            ],
            standard: None,
            description: None,
            span: Span::new("test.edl", 12, 1, 20, 1),
        });
        
        project.constraints.push(ConstraintDecl {
            id: "voltage_gnd".to_string(),
            severity: ConstraintSeverity::Mandatory,
            target: ConstraintTarget::Net("GND".to_string()),
            rules: vec![
                ConstraintRule {
                    parameter: "reference_level".to_string(),
                    operator: ConstraintOp::Equal,
                    value: ConstraintValue::Numeric(0.0, "V".to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 22, 1, 22, 30),
                }
            ],
            standard: None,
            description: None,
            span: Span::new("test.edl", 21, 1, 25, 1),
        });
        
        let result = ConstraintElaborator::elaborate_constraints(&project);
        assert!(result.is_ok());
        
        let elaborated = result.unwrap();
        assert_eq!(elaborated.len(), 2);
    }
    
    #[test]
    fn test_elaborate_component_target() {
        let mut project = create_test_project();
        
        project.constraints.push(ConstraintDecl {
            id: "comp_thermal".to_string(),
            severity: ConstraintSeverity::Strong,
            target: ConstraintTarget::Component("IC1".to_string()),
            rules: vec![
                ConstraintRule {
                    parameter: "max_temp".to_string(),
                    operator: ConstraintOp::LessThanOrEqual,
                    value: ConstraintValue::Numeric(85.0, "C".to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 30, 1, 30, 30),
                }
            ],
            standard: None,
            description: None,
            span: Span::new("test.edl", 29, 1, 32, 1),
        });
        
        let result = ConstraintElaborator::elaborate_constraints(&project);
        assert!(result.is_ok());
        
        let elaborated = result.unwrap();
        let constraint = &elaborated[0];
        assert_eq!(constraint.entity_ref, ResolvedEntityRef::Component("IC1".to_string()));
    }
    
    #[test]
    fn test_elaborate_hierarchical_target() {
        let mut project = create_test_project();
        
        project.constraints.push(ConstraintDecl {
            id: "hierarchical_net".to_string(),
            severity: ConstraintSeverity::Mandatory,
            target: ConstraintTarget::Net("PowerModule.VOUT".to_string()),
            rules: vec![
                ConstraintRule {
                    parameter: "output_voltage".to_string(),
                    operator: ConstraintOp::Equal,
                    value: ConstraintValue::Numeric(5.0, "V".to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 40, 1, 40, 30),
                }
            ],
            standard: None,
            description: None,
            span: Span::new("test.edl", 39, 1, 42, 1),
        });
        
        let result = ConstraintElaborator::elaborate_constraints(&project);
        assert!(result.is_ok());
        
        let elaborated = result.unwrap();
        let constraint = &elaborated[0];
        
        // Verify hierarchical detection
        assert!(ConstraintElaborator::is_hierarchical(&constraint.entity_ref));
        assert_eq!(ConstraintElaborator::base_name(&constraint.entity_ref), "PowerModule");
        
        let path = ConstraintElaborator::path_components(&constraint.entity_ref);
        assert_eq!(path, vec!["PowerModule", "VOUT"]);
    }
    
    #[test]
    fn test_elaborate_all_severity_levels() {
        let mut project = create_test_project();
        
        for (sev_name, sev) in &[
            ("mandatory", ConstraintSeverity::Mandatory),
            ("strong", ConstraintSeverity::Strong),
            ("weak", ConstraintSeverity::Weak),
        ] {
            project.constraints.push(ConstraintDecl {
                id: format!("rule_{}", sev_name),
                severity: sev.clone(),
                target: ConstraintTarget::Net("VCC".to_string()),
                rules: vec![
                    ConstraintRule {
                        parameter: "test_param".to_string(),
                        operator: ConstraintOp::Equal,
                        value: ConstraintValue::Numeric(1.0, "V".to_string()),
                        tolerance: None,
                        span: Span::new("test.edl", 50, 1, 50, 30),
                    }
                ],
                standard: None,
                description: None,
                span: Span::new("test.edl", 49, 1, 52, 1),
            });
        }
        
        let result = ConstraintElaborator::elaborate_constraints(&project);
        assert!(result.is_ok());
        
        let elaborated = result.unwrap();
        assert_eq!(elaborated.len(), 3);
        assert_eq!(elaborated[0].severity, ConstraintSeverity::Mandatory);
        assert_eq!(elaborated[1].severity, ConstraintSeverity::Strong);
        assert_eq!(elaborated[2].severity, ConstraintSeverity::Weak);
    }
    
    #[test]
    fn test_elaborate_multiple_rules_per_constraint() {
        let mut project = create_test_project();
        
        project.constraints.push(ConstraintDecl {
            id: "multi_rule".to_string(),
            severity: ConstraintSeverity::Mandatory,
            target: ConstraintTarget::Component("IC1".to_string()),
            rules: vec![
                ConstraintRule {
                    parameter: "operating_voltage".to_string(),
                    operator: ConstraintOp::Equal,
                    value: ConstraintValue::Numeric(3.3, "V".to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 60, 1, 60, 30),
                },
                ConstraintRule {
                    parameter: "max_current".to_string(),
                    operator: ConstraintOp::LessThanOrEqual,
                    value: ConstraintValue::Numeric(500.0, "mA".to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 61, 1, 61, 30),
                },
                ConstraintRule {
                    parameter: "power_dissipation".to_string(),
                    operator: ConstraintOp::LessThanOrEqual,
                    value: ConstraintValue::Numeric(1.5, "W".to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 62, 1, 62, 30),
                },
            ],
            standard: None,
            description: None,
            span: Span::new("test.edl", 59, 1, 63, 1),
        });
        
        let result = ConstraintElaborator::elaborate_constraints(&project);
        assert!(result.is_ok());
        
        let elaborated = result.unwrap();
        assert_eq!(elaborated[0].rules.len(), 3);
        assert_eq!(elaborated[0].rules[0].parameter, "operating_voltage");
        assert_eq!(elaborated[0].rules[1].parameter, "max_current");
        assert_eq!(elaborated[0].rules[2].parameter, "power_dissipation");
    }
    
    #[test]
    fn test_elaborate_all_target_types() {
        let mut project = create_test_project();
        
        let targets = vec![
            ("net_target", ConstraintTarget::Net("VCC".to_string())),
            ("comp_target", ConstraintTarget::Component("IC1".to_string())),
            ("pipe_target", ConstraintTarget::Pipe("P1".to_string())),
            ("fixture_target", ConstraintTarget::Fixture("F1".to_string())),
            ("group_target", ConstraintTarget::Group("SupplyNet".to_string())),
        ];
        
        for (idx, (name, target)) in targets.iter().enumerate() {
            project.constraints.push(ConstraintDecl {
                id: name.to_string(),
                severity: ConstraintSeverity::Mandatory,
                target: target.clone(),
                rules: vec![
                    ConstraintRule {
                        parameter: "test".to_string(),
                        operator: ConstraintOp::Equal,
                        value: ConstraintValue::Numeric(1.0, "V".to_string()),
                        tolerance: None,
                        span: Span::new("test.edl", 70 + idx as usize, 1, 70 + idx as usize, 30),
                    }
                ],
                standard: None,
                description: None,
                span: Span::new("test.edl", 70 + idx as usize, 1, 72 + idx as usize, 1),
            });
        }
        
        let result = ConstraintElaborator::elaborate_constraints(&project);
        assert!(result.is_ok());
        
        let elaborated = result.unwrap();
        assert_eq!(elaborated.len(), 5);
        
        // Verify entity types
        assert!(matches!(elaborated[0].entity_ref, ResolvedEntityRef::Net(_)));
        assert!(matches!(elaborated[1].entity_ref, ResolvedEntityRef::Component(_)));
        assert!(matches!(elaborated[2].entity_ref, ResolvedEntityRef::Pipe(_)));
        assert!(matches!(elaborated[3].entity_ref, ResolvedEntityRef::Fixture(_)));
        assert!(matches!(elaborated[4].entity_ref, ResolvedEntityRef::Group(_)));
    }
    
    #[test]
    fn test_elaborate_constraint_metadata() {
        let mut project = create_test_project();
        
        project.constraints.push(ConstraintDecl {
            id: "with_metadata".to_string(),
            severity: ConstraintSeverity::Strong,
            target: ConstraintTarget::Net("VCC".to_string()),
            rules: vec![
                ConstraintRule {
                    parameter: "voltage".to_string(),
                    operator: ConstraintOp::Equal,
                    value: ConstraintValue::Numeric(3.3, "V".to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 80, 1, 80, 30),
                }
            ],
            standard: Some("IEC 60950-1 Section 2.4.1".to_string()),
            description: Some("Voltage supply must be regulated to 3.3V±5%".to_string()),
            span: Span::new("test.edl", 79, 1, 85, 1),
        });
        
        let result = ConstraintElaborator::elaborate_constraints(&project);
        assert!(result.is_ok());
        
        let elaborated = result.unwrap();
        let constraint = &elaborated[0];
        
        assert_eq!(constraint.standard, Some("IEC 60950-1 Section 2.4.1".to_string()));
        assert_eq!(constraint.description, Some("Voltage supply must be regulated to 3.3V±5%".to_string()));
    }
}
