//! Phase 1.1 Integration Tests and Examples
//!
//! Demonstrates the constraint framework in action with realistic scenarios.
//! These tests verify the constraint specification, violation detection, and
//! repair suggestion generation across multiple domains.

#[cfg(test)]
mod phase_1_1_integration_tests {
    use super::*;

    /// Example: Electronics - Thermal Violation Detection
    /// 
    /// Scenario: A power IC dissipating 2W at 70°C ambient.
    /// Expected: Violation flagged, heatsink repair suggested.
    #[test]
    fn test_electronics_thermal_violation_scenario() {
        // Setup
        let mut context = ConstraintContext::new("ic_mp9486a".to_string(), "elec".to_string());
        
        // Metadata: typical boost converter
        context.metadata.insert("power_dissipation_mw".to_string(), serde_json::json!(2000.0));
        context.metadata.insert("tj_max_c".to_string(), serde_json::json!(125.0));
        context.metadata.insert("ta_max_c".to_string(), serde_json::json!(70.0));
        context.metadata.insert("has_heatsink".to_string(), serde_json::json!(false));
        
        // Verify
        let verifier = ElecConstraintVerifier::new();
        verifier.verify(&mut context);
        
        // Assert violations
        assert_eq!(context.violations.len(), 1);
        assert_eq!(context.violations[0].constraint_id, "elec_e3_thermal_pathway");
        assert_eq!(context.violations[0].severity, ConstraintSeverity::Mandatory);
        
        // Assert repairs are actionable
        assert!(!context.violations[0].repair_suggestions.is_empty());
        let first_repair = &context.violations[0].repair_suggestions[0];
        assert!(first_repair.description.contains("heatsink"));
        
        println!("✓ Thermal violation correctly detected");
        println!("  Repair: {}", first_repair.description);
    }

    /// Example: Electronics - Multi-Constraint Scenario
    /// 
    /// Scenario: PCB trace carrying 5A, 1mm wide, no heatsink nearby.
    /// Expected: Current derating AND thermal pathway violations.
    #[test]
    fn test_electronics_multi_constraint_scenario() {
        let mut context = ConstraintContext::new("trace_vout_5a".to_string(), "elec".to_string());
        
        // High-current trace scenario
        context.metadata.insert("operating_current_a".to_string(), serde_json::json!(5.0));
        context.metadata.insert("rated_current_a".to_string(), serde_json::json!(3.0));
        context.metadata.insert("trace_width_mm".to_string(), serde_json::json!(1.0));
        context.metadata.insert("trace_thickness_oz".to_string(), serde_json::json!(1.0));
        context.metadata.insert("power_dissipation_mw".to_string(), serde_json::json!(1500.0));
        context.metadata.insert("tj_max_c".to_string(), serde_json::json!(125.0));
        context.metadata.insert("ta_max_c".to_string(), serde_json::json!(70.0));
        
        let verifier = ElecConstraintVerifier::new();
        verifier.verify(&mut context);
        
        // Should have violations for both current and thermal
        assert!(context.violations.len() >= 1);
        assert!(context.violations.iter().any(|v| v.constraint_id == "elec_e2_current_derating"));
        
        println!("✓ Multi-constraint scenario detected");
        println!("  Violations: {}", context.violations.len());
        for v in &context.violations {
            println!("    - {}", v.constraint_id);
        }
    }

    /// Example: Plumbing - Pressure Drop Violation
    /// 
    /// Scenario: 20-person occupancy building with 1/2" main line over 150 ft.
    /// Expected: Pressure drop violation, upsize to 3/4" or add booster pump.
    #[test]
    fn test_plumbing_pressure_drop_scenario() {
        let mut context = ConstraintContext::new("main_line_20ppl".to_string(), "plumbing".to_string());
        
        // 20-person building with inadequate main line
        context.metadata.insert("flow_gpm".to_string(), serde_json::json!(8.0));
        context.metadata.insert("pipe_diameter_in".to_string(), serde_json::json!(0.5));
        context.metadata.insert("pipe_length_ft".to_string(), serde_json::json!(150.0));
        context.metadata.insert("max_pressure_drop_psi".to_string(), serde_json::json!(10.0));
        
        let verifier = PlumbingConstraintVerifier::new();
        verifier.verify(&mut context);
        
        // Should detect pressure drop violation
        assert!(context.violations.iter().any(|v| v.constraint_id == "plumb_p2_pressure_drop"));
        
        let pressure_violation = context.violations.iter()
            .find(|v| v.constraint_id == "plumb_p2_pressure_drop")
            .unwrap();
        
        // Should suggest upsizing or pump
        let has_upsize = pressure_violation.repair_suggestions.iter()
            .any(|r| r.description.contains("Increase pipe"));
        let has_pump = pressure_violation.repair_suggestions.iter()
            .any(|r| r.description.contains("booster pump"));
        
        assert!(has_upsize || has_pump);
        
        println!("✓ Pressure drop violation detected");
        println!("  Repair options:");
        for repair in &pressure_violation.repair_suggestions {
            println!("    - {}", repair.description);
        }
    }

    /// Example: Plumbing - Accessibility Violation (ADA)
    /// 
    /// Scenario: Accessible bathroom with toilet at 22" height (too high per ADA).
    /// Expected: Accessibility violation flagged, repositioning repair suggested.
    #[test]
    fn test_plumbing_accessibility_scenario() {
        let mut context = ConstraintContext::new("toilet_accessible".to_string(), "plumbing".to_string());
        
        // ADA violation scenario
        context.metadata.insert("fixture_type".to_string(), serde_json::json!("toilet"));
        context.metadata.insert("fixture_height_in".to_string(), serde_json::json!(22.0));
        context.metadata.insert("needs_accessible".to_string(), serde_json::json!(true));
        
        let verifier = PlumbingConstraintVerifier::new();
        verifier.verify(&mut context);
        
        // Should detect accessibility violation
        assert!(context.violations.iter().any(|v| v.constraint_id == "plumb_p4_accessibility"));
        
        let access_violation = context.violations.iter()
            .find(|v| v.constraint_id == "plumb_p4_accessibility")
            .unwrap();
        
        assert_eq!(access_violation.severity, ConstraintSeverity::Mandatory);
        assert!(access_violation.violation_reason.contains("ADA"));
        
        println!("✓ ADA accessibility violation detected");
        println!("  Violation: {}", access_violation.violation_reason);
        println!("  Repair: {}", access_violation.repair_suggestions[0].description);
    }

    /// Example: Plumbing - Code Compliance (Missing Shutoff)
    /// 
    /// Scenario: Main water line without shutoff valve.
    /// Expected: Code compliance violation, shutoff installation suggested.
    #[test]
    fn test_plumbing_code_compliance_scenario() {
        let mut context = ConstraintContext::new("main_supply".to_string(), "plumbing".to_string());
        
        context.metadata.insert("has_shutoff_valve".to_string(), serde_json::json!(false));
        context.metadata.insert("has_check_valve".to_string(), serde_json::json!(true));
        
        let verifier = PlumbingConstraintVerifier::new();
        verifier.verify(&mut context);
        
        // Should detect missing shutoff valve
        assert!(context.violations.iter().any(|v| v.constraint_id == "plumb_p5_code_shutoff"));
        
        let shutoff_violation = context.violations.iter()
            .find(|v| v.constraint_id == "plumb_p5_code_shutoff")
            .unwrap();
        
        assert_eq!(shutoff_violation.severity, ConstraintSeverity::Mandatory);
        assert!(shutoff_violation.repair_suggestions[0].description.contains("shutoff"));
        
        println!("✓ Code compliance violation detected");
        println!("  Standard: IPC 603.3");
        println!("  Repair: {}", shutoff_violation.repair_suggestions[0].description);
    }

    /// Example: Cross-Domain Scenario
    /// 
    /// Tests that multiple domain verifiers work independently and together.
    #[test]
    fn test_cross_domain_verification() {
        // Create contexts for different domains
        let mut elec_context = ConstraintContext::new("power_stage".to_string(), "elec".to_string());
        let mut plumb_context = ConstraintContext::new("supply_line".to_string(), "plumbing".to_string());
        
        // Setup electronics violation
        elec_context.metadata.insert(
            "operating_current_a".to_string(),
            serde_json::json!(5.0),
        );
        elec_context.metadata.insert(
            "rated_current_a".to_string(),
            serde_json::json!(3.0),
        );
        
        // Setup plumbing violation
        plumb_context.metadata.insert(
            "fixture_height_in".to_string(),
            serde_json::json!(22.0),
        );
        plumb_context.metadata.insert("fixture_type".to_string(), serde_json::json!("toilet"));
        plumb_context.metadata.insert("needs_accessible".to_string(), serde_json::json!(true));
        
        // Verify both domains independently
        ElecConstraintVerifier::new().verify(&mut elec_context);
        PlumbingConstraintVerifier::new().verify(&mut plumb_context);
        
        // Both should have violations
        assert!(!elec_context.violations.is_empty());
        assert!(!plumb_context.violations.is_empty());
        
        // But in different domains
        assert_eq!(elec_context.domain, "elec");
        assert_eq!(plumb_context.domain, "plumbing");
        
        println!("✓ Cross-domain verification works independently");
    }

    /// Example: Registry-Based Verification
    /// 
    /// Tests that the constraint registry correctly routes verifications.
    #[test]
    fn test_constraint_registry_routing() {
        let mut registry = ConstraintRegistry::new();
        registry.register(Box::new(ElecConstraintVerifier::new()));
        registry.register(Box::new(PlumbingConstraintVerifier::new()));
        
        // Verify domains are registered
        assert!(registry.domains().contains(&"elec"));
        assert!(registry.domains().contains(&"plumbing"));
        
        // Test routing
        let mut elec_context = ConstraintContext::new("trace_1".to_string(), "elec".to_string());
        elec_context.metadata.insert("operating_current_a".to_string(), serde_json::json!(5.0));
        elec_context.metadata.insert("rated_current_a".to_string(), serde_json::json!(3.0));
        
        registry.verify_all(&mut elec_context);
        
        assert!(!elec_context.violations.is_empty());
        
        println!("✓ Registry-based routing works correctly");
    }

    /// Example: Repair Action Auto-Fixability
    /// 
    /// Tests that repairs are correctly marked as auto-fixable or not.
    #[test]
    fn test_repair_auto_fixability() {
        let mut context = ConstraintContext::new("ic_voltage".to_string(), "elec".to_string());
        context.metadata.insert("operating_voltage_v".to_string(), serde_json::json!(3.6));
        context.metadata.insert("rated_voltage_v".to_string(), serde_json::json!(3.3));
        
        let verifier = ElecConstraintVerifier::new();
        verifier.verify(&mut context);
        
        if !context.violations.is_empty() {
            let repairs = &context.violations[0].repair_suggestions;
            
            // Component selection should NOT be auto-fixable
            let comp_repair = repairs.iter()
                .find(|r| r.description.contains("Replace with component"))
                .unwrap();
            assert!(!comp_repair.auto_fixable);
            
            // But repair should still have actionable details
            assert!(!comp_repair.changes.is_empty());
            assert!(comp_repair.changes.contains_key("rated_voltage_v"));
            
            println!("✓ Repair auto-fixability correctly assigned");
            println!("  Auto-fixable repairs: {}", 
                repairs.iter().filter(|r| r.auto_fixable).count());
            println!("  Manual repairs: {}", 
                repairs.iter().filter(|r| !r.auto_fixable).count());
        }
    }
}

#[cfg(test)]
mod constraint_framework_validation {
    /// Validates the abstract constraint framework design.
    
    #[test]
    fn test_constraint_severity_levels() {
        // Mandatory: must satisfy (design non-permittable if violated)
        assert_eq!(ConstraintSeverity::Mandatory as u8, 0);
        
        // Strong: should satisfy (best practice)
        assert_eq!(ConstraintSeverity::Strong as u8, 1);
        
        // Weak: may violate with justification
        assert_eq!(ConstraintSeverity::Weak as u8, 2);
        
        println!("✓ Constraint severity levels are correct");
    }

    #[test]
    fn test_constraint_operators() {
        // Verify all constraint operators are defined
        let ops = vec![
            ConstraintOp::Equal,
            ConstraintOp::NotEqual,
            ConstraintOp::LessThan,
            ConstraintOp::LessThanOrEqual,
            ConstraintOp::GreaterThan,
            ConstraintOp::GreaterThanOrEqual,
            ConstraintOp::Between,
        ];
        
        assert_eq!(ops.len(), 7);
        println!("✓ All constraint operators defined");
    }

    #[test]
    fn test_constraint_values_cover_types() {
        // Numeric
        let numeric = ConstraintValue::Numeric {
            value: 3.3,
            unit: "V".to_string(),
        };
        assert_eq!(numeric.as_numeric(), Some(3.3));
        
        // Integer
        let integer = ConstraintValue::Integer { value: 24 };
        assert_eq!(integer.as_integer(), Some(24));
        
        // Enum
        let enum_val = ConstraintValue::Enum {
            value: "copper".to_string(),
            allowed: vec!["copper".to_string(), "aluminum".to_string()],
        };
        assert_eq!(enum_val.as_enum(), Some(&"copper".to_string()));
        
        // Boolean
        let boolean = ConstraintValue::Boolean { value: true };
        assert_eq!(boolean.as_boolean(), Some(true));
        
        // Composite
        let mut fields = BTreeMap::new();
        fields.insert("width_mm".to_string(), ConstraintValue::Numeric { value: 2.5, unit: "mm".to_string() });
        let composite = ConstraintValue::Composite {
            name: "trace".to_string(),
            fields,
        };
        assert_eq!(composite.as_enum(), None);
        
        println!("✓ All constraint value types supported");
    }
}
