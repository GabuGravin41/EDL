/// Phase 1.2 Week 3: Constraint Propagation Tests
/// Validates voltage, current, thermal, and flow constraint propagation

#[cfg(test)]
mod constraint_propagation_tests {
    use crate::core::propagator::{
        ConstraintPropagator, DesignHierarchy, PropagationResult, PropagatedConstraint,
    };
    use crate::core::constraints::ConstraintValue;
    use crate::core::span::Span;
    use crate::syntax::constraint_elaborator::{ConstraintElaborator, ElaboratedConstraint, ResolvedEntityRef};
    use crate::syntax::constraints_ast::{ConstraintDecl, ConstraintTarget, ConstraintSeverity, ConstraintRule, ConstraintOp};

    fn create_elaborated_constraint(
        id: &str,
        entity_ref: ResolvedEntityRef,
        param: &str,
        op: ConstraintOp,
        value: f64,
        unit: &str,
    ) -> ElaboratedConstraint {
        ElaboratedConstraint {
            id: id.to_string(),
            severity: ConstraintSeverity::Mandatory,
            entity_ref,
            rules: vec![
                ConstraintRule {
                    parameter: param.to_string(),
                    operator: op,
                    value: ConstraintValue::Numeric(value, unit.to_string()),
                    tolerance: None,
                    span: Span::new("test.edl", 1, 1, 1, 10),
                }
            ],
            standard: None,
            description: None,
        }
    }

    #[test]
    fn test_propagate_voltage_no_hierarchy() {
        let constraint = create_elaborated_constraint(
            "voltage_vcc",
            ResolvedEntityRef::Net("VCC".to_string()),
            "operating_voltage",
            ConstraintOp::Equal,
            3.3,
            "V",
        );
        
        let hierarchy = DesignHierarchy::default();
        
        let result = ConstraintPropagator::propagate_all(&[constraint], &hierarchy).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].targets.len(), 1);
    }

    #[test]
    fn test_propagate_voltage_with_derating() {
        let constraint = create_elaborated_constraint(
            "voltage_vcc",
            ResolvedEntityRef::Net("VCC".to_string()),
            "operating_voltage",
            ConstraintOp::Equal,
            3.3,
            "V",
        );
        
        let hierarchy = DesignHierarchy::default();
        
        let result = ConstraintPropagator::propagate_all(&[constraint], &hierarchy).unwrap();
        let propagation = &result[0];
        
        // At depth 0, no derating
        assert_eq!(propagation.targets[0].propagated_rule.derating_applied, false);
    }

    #[test]
    fn test_propagate_multiple_constraints() {
        let vcc_constraint = create_elaborated_constraint(
            "voltage_vcc",
            ResolvedEntityRef::Net("VCC".to_string()),
            "operating_voltage",
            ConstraintOp::Equal,
            3.3,
            "V",
        );
        
        let gnd_constraint = create_elaborated_constraint(
            "voltage_gnd",
            ResolvedEntityRef::Net("GND".to_string()),
            "reference_level",
            ConstraintOp::Equal,
            0.0,
            "V",
        );
        
        let hierarchy = DesignHierarchy::default();
        
        let result = ConstraintPropagator::propagate_all(
            &[vcc_constraint, gnd_constraint],
            &hierarchy,
        ).unwrap();
        
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_propagate_thermal_constraint() {
        let thermal_constraint = create_elaborated_constraint(
            "thermal_ic1",
            ResolvedEntityRef::Component("IC1".to_string()),
            "max_temperature",
            ConstraintOp::LessThanOrEqual,
            85.0,
            "C",
        );
        
        let hierarchy = DesignHierarchy::default();
        
        let result = ConstraintPropagator::propagate_all(&[thermal_constraint], &hierarchy).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_propagate_flow_constraint() {
        let flow_constraint = create_elaborated_constraint(
            "flow_demand",
            ResolvedEntityRef::Pipe("P1".to_string()),
            "flow_rate",
            ConstraintOp::Equal,
            10.0,
            "GPM",
        );
        
        let hierarchy = DesignHierarchy::default();
        
        let result = ConstraintPropagator::propagate_all(&[flow_constraint], &hierarchy).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_propagate_fixture_constraint() {
        let fixture_constraint = create_elaborated_constraint(
            "fixture_access",
            ResolvedEntityRef::Fixture("F1".to_string()),
            "clearance",
            ConstraintOp::GreaterThanOrEqual,
            18.0,
            "in",
        );
        
        let hierarchy = DesignHierarchy::default();
        
        let result = ConstraintPropagator::propagate_all(&[fixture_constraint], &hierarchy).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_propagate_group_constraint() {
        let group_constraint = create_elaborated_constraint(
            "group_supply",
            ResolvedEntityRef::Group("SupplyNets".to_string()),
            "regulated",
            ConstraintOp::Equal,
            3.3,
            "V",
        );
        
        let hierarchy = DesignHierarchy::default();
        
        let result = ConstraintPropagator::propagate_all(&[group_constraint], &hierarchy).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_voltage_derating_increases_with_depth() {
        let rule = ConstraintRule {
            parameter: "voltage".to_string(),
            operator: ConstraintOp::Equal,
            value: ConstraintValue::Numeric(5.0, "V".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        // Depth 0: no derating
        let result0 = ConstraintPropagator::propagate_voltage_constraint(&rule, 0).unwrap();
        assert_eq!(result0.derating_percent, None);
        
        // Depth 1: 1% derating
        let result1 = ConstraintPropagator::propagate_voltage_constraint(&rule, 1).unwrap();
        assert_eq!(result1.derating_percent, Some(1.0));
        
        // Depth 2: 2.5% derating
        let result2 = ConstraintPropagator::propagate_voltage_constraint(&rule, 2).unwrap();
        assert_eq!(result2.derating_percent, Some(2.5));
        
        // Depth 3+: capped at 5%
        let result3 = ConstraintPropagator::propagate_voltage_constraint(&rule, 3).unwrap();
        assert_eq!(result3.derating_percent, Some(5.0));
    }

    #[test]
    fn test_thermal_derating_calculation() {
        let rule = ConstraintRule {
            parameter: "max_temp".to_string(),
            operator: ConstraintOp::LessThanOrEqual,
            value: ConstraintValue::Numeric(85.0, "C".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        // Thermal increases with depth
        let result0 = ConstraintPropagator::propagate_thermal_constraint(&rule, 0).unwrap();
        let result1 = ConstraintPropagator::propagate_thermal_constraint(&rule, 1).unwrap();
        let result2 = ConstraintPropagator::propagate_thermal_constraint(&rule, 2).unwrap();
        
        assert_eq!(result0.derating_percent, None);
        assert_eq!(result1.derating_percent, Some(10.0));
        assert_eq!(result2.derating_percent, Some(25.0));
    }

    #[test]
    fn test_flow_propagation_accumulation() {
        let rule = ConstraintRule {
            parameter: "demand".to_string(),
            operator: ConstraintOp::Equal,
            value: ConstraintValue::Numeric(5.0, "GPM".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        let result0 = ConstraintPropagator::propagate_flow_constraint(&rule, 0).unwrap();
        let result1 = ConstraintPropagator::propagate_flow_constraint(&rule, 1).unwrap();
        let result2 = ConstraintPropagator::propagate_flow_constraint(&rule, 2).unwrap();
        
        assert_eq!(result0.derating_percent, None);
        assert_eq!(result1.derating_percent, Some(3.0));
        assert_eq!(result2.derating_percent, Some(8.0));
    }

    #[test]
    fn test_propagated_value_calculation_voltage() {
        let rule = ConstraintRule {
            parameter: "voltage".to_string(),
            operator: ConstraintOp::Equal,
            value: ConstraintValue::Numeric(10.0, "V".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        let result = ConstraintPropagator::propagate_voltage_constraint(&rule, 1).unwrap();
        
        // With 1% derating, 10V * 1.01 = 10.1V
        match result.propagated_value {
            ConstraintValue::Numeric(val, _) => {
                assert!((val - 10.1).abs() < 0.001);
            }
            _ => panic!("Expected numeric value"),
        }
    }

    #[test]
    fn test_propagated_value_calculation_thermal() {
        let rule = ConstraintRule {
            parameter: "max_temp".to_string(),
            operator: ConstraintOp::LessThanOrEqual,
            value: ConstraintValue::Numeric(100.0, "C".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        let result = ConstraintPropagator::propagate_thermal_constraint(&rule, 1).unwrap();
        
        // With 10% derating, 100C * 1.10 = 110C
        match result.propagated_value {
            ConstraintValue::Numeric(val, _) => {
                assert!((val - 110.0).abs() < 0.001);
            }
            _ => panic!("Expected numeric value"),
        }
    }

    #[test]
    fn test_component_constraint_margin() {
        let rule = ConstraintRule {
            parameter: "rated_voltage".to_string(),
            operator: ConstraintOp::GreaterThanOrEqual,
            value: ConstraintValue::Numeric(5.0, "V".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        let result = ConstraintPropagator::propagate_component_constraint(&rule, 1).unwrap();
        
        // Component margins reduce the value (more conservative)
        // 5V * (1 - 0.02) = 4.9V
        match result.propagated_value {
            ConstraintValue::Numeric(val, _) => {
                assert!((val - 4.9).abs() < 0.001);
            }
            _ => panic!("Expected numeric value"),
        }
    }

    #[test]
    fn test_propagation_result_structure() {
        let constraint = create_elaborated_constraint(
            "test_constraint",
            ResolvedEntityRef::Net("TEST".to_string()),
            "param",
            ConstraintOp::Equal,
            1.0,
            "V",
        );
        
        let hierarchy = DesignHierarchy::default();
        let result = ConstraintPropagator::propagate_all(&[constraint], &hierarchy).unwrap();
        
        assert_eq!(result.len(), 1);
        let propagation = &result[0];
        
        assert_eq!(propagation.source_entity, ResolvedEntityRef::Net("TEST".to_string()));
        assert!(!propagation.targets.is_empty());
        
        let target = &propagation.targets[0];
        assert_eq!(target.constraint_id, "test_constraint");
    }

    #[test]
    fn test_hierarchy_entity_addition() {
        let mut hierarchy = DesignHierarchy::default();
        
        hierarchy.add_entity(
            ResolvedEntityRef::Net("VCC".to_string()),
            vec!["PowerModule".to_string(), "VCC".to_string()],
        );
        
        assert!(!hierarchy.entities.is_empty());
        assert!(hierarchy.entities.contains_key("PowerModule.VCC"));
    }

    #[test]
    fn test_hierarchy_descendant_finding() {
        let mut hierarchy = DesignHierarchy::default();
        
        hierarchy.add_entity(
            ResolvedEntityRef::Net("V1".to_string()),
            vec!["Module".to_string(), "Sub".to_string(), "V1".to_string()],
        );
        
        hierarchy.add_entity(
            ResolvedEntityRef::Net("V2".to_string()),
            vec!["Module".to_string(), "Sub".to_string(), "V2".to_string()],
        );
        
        let descendants = hierarchy.find_descendants(&["Module".to_string(), "Sub".to_string()]).unwrap();
        
        // Should find both V1 and V2
        assert_eq!(descendants.len(), 2);
    }

    #[test]
    fn test_constraint_propagation_all_domains() {
        let constraints = vec![
            create_elaborated_constraint(
                "voltage",
                ResolvedEntityRef::Net("VCC".to_string()),
                "voltage",
                ConstraintOp::Equal,
                3.3,
                "V",
            ),
            create_elaborated_constraint(
                "thermal",
                ResolvedEntityRef::Component("IC1".to_string()),
                "max_temp",
                ConstraintOp::LessThanOrEqual,
                85.0,
                "C",
            ),
            create_elaborated_constraint(
                "flow",
                ResolvedEntityRef::Pipe("P1".to_string()),
                "flow_rate",
                ConstraintOp::Equal,
                10.0,
                "GPM",
            ),
            create_elaborated_constraint(
                "fixture",
                ResolvedEntityRef::Fixture("F1".to_string()),
                "clearance",
                ConstraintOp::GreaterThanOrEqual,
                18.0,
                "in",
            ),
        ];
        
        let hierarchy = DesignHierarchy::default();
        let results = ConstraintPropagator::propagate_all(&constraints, &hierarchy).unwrap();
        
        assert_eq!(results.len(), 4);
    }
}
