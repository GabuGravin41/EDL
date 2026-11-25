/// Phase 1.2 Week 4: Constraint Reporter Tests
/// Validates JSON/Markdown report generation and repair suggestions

#[cfg(test)]
mod constraint_reporter_tests {
    use crate::core::reporter::{
        ConstraintViolation, RepairSuggestion, ConstraintReporter, ConstraintReport,
    };
    use std::collections::HashMap;

    fn create_test_violation(
        id: &str,
        entity: &str,
        severity: &str,
        violation: &str,
    ) -> ConstraintViolation {
        ConstraintViolation {
            constraint_id: id.to_string(),
            entity: entity.to_string(),
            severity: severity.to_string(),
            violation: violation.to_string(),
            expected: "3.3V".to_string(),
            actual: "3.5V".to_string(),
            repairs: vec![],
        }
    }

    #[test]
    fn test_generate_empty_report() {
        let violations = vec![];
        let report = ConstraintReporter::generate_report("TestProject", &[], violations);

        assert_eq!(report.project, "TestProject");
        assert_eq!(report.summary.total_constraints, 0);
        assert_eq!(report.summary.total_violations, 0);
        assert_eq!(report.summary.pass_rate_percent, 100.0);
    }

    #[test]
    fn test_generate_report_with_single_violation() {
        let violations = vec![
            create_test_violation("v1", "net:VCC", "mandatory", "Voltage exceeded"),
        ];

        let report = ConstraintReporter::generate_report("TestProject", &[], violations);

        assert_eq!(report.summary.total_violations, 1);
        assert_eq!(report.summary.mandatory_violations, 1);
        assert_eq!(report.summary.strong_violations, 0);
        assert_eq!(report.summary.weak_violations, 0);
    }

    #[test]
    fn test_generate_report_with_multiple_severities() {
        let violations = vec![
            create_test_violation("v1", "net:VCC", "mandatory", "Critical"),
            create_test_violation("v2", "net:GND", "strong", "Major"),
            create_test_violation("v3", "net:SIG", "weak", "Minor"),
        ];

        let report = ConstraintReporter::generate_report("TestProject", &[], violations);

        assert_eq!(report.summary.total_violations, 3);
        assert_eq!(report.summary.mandatory_violations, 1);
        assert_eq!(report.summary.strong_violations, 1);
        assert_eq!(report.summary.weak_violations, 1);
    }

    #[test]
    fn test_pass_rate_calculation() {
        let violations = vec![
            create_test_violation("v1", "net:VCC", "mandatory", "Test1"),
            create_test_violation("v2", "net:GND", "mandatory", "Test2"),
        ];

        let report = ConstraintReporter::generate_report("TestProject", &[], violations);

        assert_eq!(report.summary.pass_rate_percent, 0.0);
    }

    #[test]
    fn test_domain_grouping() {
        let violations = vec![
            create_test_violation("v1", "net:VCC", "mandatory", "Electrical"),
            create_test_violation("v2", "component:IC1", "mandatory", "Electrical"),
            create_test_violation("v3", "pipe:P1", "mandatory", "Plumbing"),
            create_test_violation("v4", "fixture:F1", "mandatory", "Architectural"),
        ];

        let report = ConstraintReporter::generate_report("TestProject", &[], violations);

        // Should have violations grouped by domain
        assert!(!report.details_by_domain.is_empty());
    }

    #[test]
    fn test_voltage_repair_low_difference() {
        let repair = ConstraintReporter::suggest_voltage_repair("IC1", 3.3, 3.31);

        assert!(repair.auto_fixable);
        assert!(repair.description.contains("Adjust"));
    }

    #[test]
    fn test_voltage_repair_medium_difference() {
        let repair = ConstraintReporter::suggest_voltage_repair("IC1", 3.3, 3.5);

        assert!(!repair.auto_fixable);
        assert!(repair.description.contains("Replace"));
    }

    #[test]
    fn test_voltage_repair_high_difference() {
        let repair = ConstraintReporter::suggest_voltage_repair("IC1", 3.3, 5.0);

        assert!(!repair.auto_fixable);
        assert!(repair.description.contains("Redesign"));
    }

    #[test]
    fn test_voltage_repair_parameters() {
        let repair = ConstraintReporter::suggest_voltage_repair("IC1", 3.3, 3.5);

        assert!(repair.parameters.contains_key("entity"));
        assert!(repair.parameters.contains_key("current_voltage"));
        assert!(repair.parameters.contains_key("required_voltage"));
        assert!(repair.parameters.contains_key("difference_percent"));

        assert_eq!(repair.parameters.get("entity").unwrap(), "IC1");
    }

    #[test]
    fn test_thermal_repair_minor() {
        let repair = ConstraintReporter::suggest_thermal_repair("IC1", 85.0, 87.0);

        assert!(!repair.auto_fixable);
        assert!(repair.description.contains("airflow"));
    }

    #[test]
    fn test_thermal_repair_moderate() {
        let repair = ConstraintReporter::suggest_thermal_repair("IC1", 85.0, 95.0);

        assert!(!repair.auto_fixable);
        assert!(repair.description.contains("heatsink"));
    }

    #[test]
    fn test_thermal_repair_severe() {
        let repair = ConstraintReporter::suggest_thermal_repair("IC1", 85.0, 105.0);

        assert!(!repair.auto_fixable);
        assert!(
            repair.description.contains("lower-power")
                || repair.description.contains("active cooling")
        );
    }

    #[test]
    fn test_thermal_repair_parameters() {
        let repair = ConstraintReporter::suggest_thermal_repair("IC1", 85.0, 95.0);

        assert!(repair.parameters.contains_key("entity"));
        assert!(repair.parameters.contains_key("max_temperature"));
        assert!(repair.parameters.contains_key("actual_temperature"));
        assert!(repair.parameters.contains_key("overage"));
    }

    #[test]
    fn test_domain_extraction_net() {
        let domain = ConstraintReporter::extract_domain("net:VCC");
        assert_eq!(domain, "Electrical");
    }

    #[test]
    fn test_domain_extraction_component() {
        let domain = ConstraintReporter::extract_domain("component:IC1");
        assert_eq!(domain, "Electrical");
    }

    #[test]
    fn test_domain_extraction_pipe() {
        let domain = ConstraintReporter::extract_domain("pipe:P1");
        assert_eq!(domain, "MEP/Plumbing");
    }

    #[test]
    fn test_domain_extraction_fixture() {
        let domain = ConstraintReporter::extract_domain("fixture:F1");
        assert_eq!(domain, "Architectural");
    }

    #[test]
    fn test_domain_extraction_group() {
        let domain = ConstraintReporter::extract_domain("group:SupplyNets");
        assert_eq!(domain, "Multi-Domain");
    }

    #[test]
    fn test_markdown_generation_includes_summary() {
        let violations = vec![
            create_test_violation("v1", "net:VCC", "mandatory", "Test"),
        ];

        let report = ConstraintReporter::generate_report("MyProject", &[], violations);
        let markdown = ConstraintReporter::report_to_markdown(&report);

        assert!(markdown.contains("MyProject"));
        assert!(markdown.contains("Summary"));
        assert!(markdown.contains("Pass Rate"));
    }

    #[test]
    fn test_markdown_generation_includes_violations() {
        let violations = vec![
            create_test_violation("v1", "net:VCC", "mandatory", "Test violation"),
        ];

        let report = ConstraintReporter::generate_report("MyProject", &[], violations);
        let markdown = ConstraintReporter::report_to_markdown(&report);

        assert!(markdown.contains("Violation"));
        assert!(markdown.contains("Test violation"));
    }

    #[test]
    fn test_markdown_severity_ordering() {
        let violations = vec![
            create_test_violation("v1", "net:1", "weak", "Weak"),
            create_test_violation("v2", "net:2", "mandatory", "Mandatory"),
            create_test_violation("v3", "net:3", "strong", "Strong"),
        ];

        let report = ConstraintReporter::generate_report("Test", &[], violations);
        let markdown = ConstraintReporter::report_to_markdown(&report);

        // Mandatory should appear before Strong, before Weak
        let mandatory_pos = markdown.find("MANDATORY").unwrap_or(0);
        let strong_pos = markdown.find("STRONG").unwrap_or(1000);
        let weak_pos = markdown.find("WEAK").unwrap_or(2000);

        assert!(mandatory_pos < strong_pos);
        assert!(strong_pos < weak_pos);
    }

    #[test]
    fn test_markdown_includes_domain_sections() {
        let violations = vec![
            create_test_violation("v1", "net:VCC", "mandatory", "Electrical"),
            create_test_violation("v2", "pipe:P1", "mandatory", "Plumbing"),
        ];

        let report = ConstraintReporter::generate_report("Test", &[], violations);
        let markdown = ConstraintReporter::report_to_markdown(&report);

        assert!(markdown.contains("Electrical"));
        assert!(markdown.contains("MEP/Plumbing"));
    }

    #[test]
    fn test_json_generation() {
        let violations = vec![
            create_test_violation("v1", "net:VCC", "mandatory", "Test"),
        ];

        let report = ConstraintReporter::generate_report("Test", &[], violations);
        let json = ConstraintReporter::report_to_json(&report);

        assert!(json.is_ok());
        let json_str = json.unwrap();
        assert!(json_str.contains("\"constraint_id\""));
        assert!(json_str.contains("\"violations\""));
    }

    #[test]
    fn test_repair_suggestion_structure() {
        let repair = ConstraintReporter::suggest_voltage_repair("IC1", 3.3, 3.5);

        assert!(!repair.description.is_empty());
        assert!(!repair.justification.is_empty());
        assert!(!repair.parameters.is_empty());
    }

    #[test]
    fn test_multiple_violations_per_entity() {
        let violations = vec![
            ConstraintViolation {
                constraint_id: "v1".to_string(),
                entity: "net:VCC".to_string(),
                severity: "mandatory".to_string(),
                violation: "Voltage".to_string(),
                expected: "3.3V".to_string(),
                actual: "3.5V".to_string(),
                repairs: vec![ConstraintReporter::suggest_voltage_repair("VCC", 3.3, 3.5)],
            },
            ConstraintViolation {
                constraint_id: "v2".to_string(),
                entity: "net:VCC".to_string(),
                severity: "strong".to_string(),
                violation: "Current".to_string(),
                expected: "500mA".to_string(),
                actual: "600mA".to_string(),
                repairs: vec![],
            },
        ];

        let report = ConstraintReporter::generate_report("Test", &[], violations);

        assert_eq!(report.summary.total_violations, 2);
    }

    #[test]
    fn test_report_includes_auto_fixable_repairs() {
        let violations = vec![
            ConstraintViolation {
                constraint_id: "v1".to_string(),
                entity: "net:VCC".to_string(),
                severity: "mandatory".to_string(),
                violation: "Minor".to_string(),
                expected: "3.3V".to_string(),
                actual: "3.31V".to_string(),
                repairs: vec![ConstraintReporter::suggest_voltage_repair("VCC", 3.3, 3.31)],
            },
        ];

        let report = ConstraintReporter::generate_report("Test", &[], violations);
        let markdown = ConstraintReporter::report_to_markdown(&report);

        // Should mention auto-fixes if any
        if report.violations[0].repairs[0].auto_fixable {
            assert!(markdown.contains("Auto-Fix") || markdown.contains("auto-fixable"));
        }
    }

    #[test]
    fn test_empty_repairs_list() {
        let violation = create_test_violation("v1", "net:VCC", "mandatory", "Test");

        let report = ConstraintReporter::generate_report("Test", &[], vec![violation]);

        assert_eq!(report.violations[0].repairs.len(), 0);
    }
}
