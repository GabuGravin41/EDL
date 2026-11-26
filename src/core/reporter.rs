/// Phase 1.2 Week 4: Constraint Violation Reporter
/// Generates constraint violation reports in JSON and Markdown formats

use crate::core::constraints::ConstraintValue;
use crate::core::propagator::{PropagatedConstraint, PropagationResult};
use crate::core::constraints::ConstraintSeverity;
use crate::syntax::constraint_elaborator::ResolvedEntityRef;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// A constraint violation detected during verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintViolation {
    pub constraint_id: String,
    pub entity: String,
    pub severity: String,
    pub violation: String,
    pub expected: String,
    pub actual: String,
    pub repairs: Vec<RepairSuggestion>,
}

/// Suggested repair for a violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairSuggestion {
    pub description: String,
    pub auto_fixable: bool,
    pub justification: String,
    pub parameters: HashMap<String, String>,
}

/// Constraint verification report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintReport {
    pub project: String,
    pub timestamp: String,
    pub summary: ReportSummary,
    pub violations: Vec<ConstraintViolation>,
    pub details_by_domain: HashMap<String, DomainReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total_constraints: usize,
    pub total_violations: usize,
    pub mandatory_violations: usize,
    pub strong_violations: usize,
    pub weak_violations: usize,
    pub pass_rate_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainReport {
    pub domain: String,
    pub entity_count: usize,
    pub constraint_count: usize,
    pub violation_count: usize,
    pub violations: Vec<ConstraintViolation>,
}

/// Constraint reporter for generating formatted output
pub struct ConstraintReporter;

impl ConstraintReporter {
    /// Generate a constraint violation report
    pub fn generate_report(
        project_name: &str,
        propagations: &[PropagationResult],
        violations: Vec<ConstraintViolation>,
    ) -> ConstraintReport {
        let total_constraints = propagations.len();
        let mandatory_violations = violations.iter()
            .filter(|v| v.severity == "mandatory")
            .count();
        let strong_violations = violations.iter()
            .filter(|v| v.severity == "strong")
            .count();
        let weak_violations = violations.iter()
            .filter(|v| v.severity == "weak")
            .count();
        
        let total_violations = violations.len();
        let pass_rate = if total_constraints > 0 {
            ((total_constraints - total_violations) as f64 / total_constraints as f64) * 100.0
        } else {
            100.0
        };
        
        // Group violations by domain
        let mut domain_reports: HashMap<String, DomainReport> = HashMap::new();
        for violation in &violations {
            let domain = Self::extract_domain(&violation.entity);
            domain_reports
                .entry(domain.clone())
                .or_insert_with(|| DomainReport {
                    domain: domain.clone(),
                    entity_count: 0,
                    constraint_count: 0,
                    violation_count: 0,
                    violations: Vec::new(),
                })
                .violations
                .push(violation.clone());
            
            domain_reports.get_mut(&domain).unwrap().violation_count += 1;
        }
        
        ConstraintReport {
            project: project_name.to_string(),
            timestamp: chrono::Local::now().to_rfc3339(),
            summary: ReportSummary {
                total_constraints,
                total_violations,
                mandatory_violations,
                strong_violations,
                weak_violations,
                pass_rate_percent: pass_rate,
            },
            violations,
            details_by_domain: domain_reports,
        }
    }
    
    /// Generate JSON formatted report
    pub fn report_to_json(report: &ConstraintReport) -> Result<String, String> {
        serde_json::to_string_pretty(report)
            .map_err(|e| format!("JSON serialization failed: {}", e))
    }
    
    /// Generate Markdown formatted report
    pub fn report_to_markdown(report: &ConstraintReport) -> String {
        let mut md = String::new();
        
        md.push_str(&format!("# Constraint Verification Report\n\n"));
        md.push_str(&format!("**Project:** {}\n", report.project));
        md.push_str(&format!("**Generated:** {}\n\n", report.timestamp));
        
        // Summary section
        md.push_str("## Summary\n\n");
        md.push_str(&format!(
            "| Metric | Value |\n|--------|-------|\n"
        ));
        md.push_str(&format!(
            "| Total Constraints | {} |\n",
            report.summary.total_constraints
        ));
        md.push_str(&format!(
            "| Pass Rate | {:.1}% |\n",
            report.summary.pass_rate_percent
        ));
        md.push_str(&format!(
            "| Total Violations | {} |\n",
            report.summary.total_violations
        ));
        md.push_str(&format!(
            "| Mandatory Violations | {} |\n",
            report.summary.mandatory_violations
        ));
        md.push_str(&format!(
            "| Strong Violations | {} |\n",
            report.summary.strong_violations
        ));
        md.push_str(&format!(
            "| Weak Violations | {} |\n\n",
            report.summary.weak_violations
        ));
        
        // Violations by domain
        if !report.details_by_domain.is_empty() {
            md.push_str("## Violations by Domain\n\n");
            for (domain, domain_report) in &report.details_by_domain {
                md.push_str(&format!("### {}\n\n", domain));
                md.push_str(&format!(
                    "**Violations:** {} | **Entities Affected:** {}\n\n",
                    domain_report.violation_count,
                    domain_report.entity_count
                ));
                
                for violation in &domain_report.violations {
                    Self::format_violation_markdown(&mut md, violation);
                }
                
                md.push_str("\n");
            }
        }
        
        // Detailed violations
        if !report.violations.is_empty() {
            md.push_str("## All Violations\n\n");
            
            // Group by severity
            let mut by_severity: HashMap<String, Vec<_>> = HashMap::new();
            for violation in &report.violations {
                by_severity
                    .entry(violation.severity.clone())
                    .or_insert_with(Vec::new)
                    .push(violation);
            }
            
            // Mandatory first (highest priority)
            for sev in &["mandatory", "strong", "weak"] {
                if let Some(violations) = by_severity.get(&sev.to_string()) {
                    md.push_str(&format!("### {} Violations\n\n", sev.to_uppercase()));
                    for violation in violations {
                        Self::format_violation_markdown(&mut md, violation);
                        md.push_str("\n");
                    }
                }
            }
        }
        
        // Repair recommendations
        let auto_fixable: Vec<_> = report.violations.iter()
            .flat_map(|v| {
                v.repairs.iter().filter(|r| r.auto_fixable)
            })
            .collect();
        
        if !auto_fixable.is_empty() {
            md.push_str("## Recommended Auto-Fixes\n\n");
            for repair in auto_fixable {
                md.push_str(&format!("- **{}**\n", repair.description));
                md.push_str(&format!("  - Justification: {}\n", repair.justification));
            }
        }
        
        md
    }
    
    /// Format a single violation for Markdown
    fn format_violation_markdown(md: &mut String, violation: &ConstraintViolation) {
        md.push_str(&format!("#### {}\n", violation.constraint_id));
        md.push_str(&format!("- **Entity:** `{}`\n", violation.entity));
        md.push_str(&format!("- **Severity:** {}\n", violation.severity.to_uppercase()));
        md.push_str(&format!("- **Violation:** {}\n", violation.violation));
        md.push_str(&format!("- **Expected:** {}\n", violation.expected));
        md.push_str(&format!("- **Actual:** {}\n", violation.actual));
        
        if !violation.repairs.is_empty() {
            md.push_str("- **Repairs:**\n");
            for repair in &violation.repairs {
                md.push_str(&format!("  - {}\n", repair.description));
                if repair.auto_fixable {
                    md.push_str("    *(Auto-fixable)*\n");
                }
            }
        }
        md.push_str("\n");
    }
    
    /// Extract domain from entity reference
    fn extract_domain(entity: &str) -> String {
        // Simple heuristic: net→electrical, pipe→plumbing, fixture→architectural
        if entity.contains(':') {
            let parts: Vec<&str> = entity.split(':').collect();
            if parts.len() >= 1 {
                return match parts[0] {
                    "net" => "Electrical".to_string(),
                    "component" => "Electrical".to_string(),
                    "pipe" => "MEP/Plumbing".to_string(),
                    "fixture" => "Architectural".to_string(),
                    "group" => "Multi-Domain".to_string(),
                    _ => "Unknown".to_string(),
                };
            }
        }
        "Unknown".to_string()
    }
    
    /// Generate repair suggestion for voltage violation
    pub fn suggest_voltage_repair(
        entity: &str,
        expected: f64,
        actual: f64,
    ) -> RepairSuggestion {
        let diff = (expected - actual).abs();
        let percent_diff = (diff / actual) * 100.0;
        
        let description = if percent_diff < 5.0 {
            format!("Adjust {} to {}V (within 5% tolerance)", entity, expected)
        } else if percent_diff < 10.0 {
            format!("Replace {} with component rated for {}V", entity, expected)
        } else {
            format!(
                "Redesign circuit: {} cannot support {}V requirement ({}V rated)",
                entity, expected, actual
            )
        };
        
        let auto_fixable = percent_diff < 2.0;
        
        RepairSuggestion {
            description,
            auto_fixable,
            justification: format!(
                "Component rated {}V but requires {}V ({}% difference)",
                actual, expected, percent_diff
            ),
            parameters: {
                let mut params = HashMap::new();
                params.insert("entity".to_string(), entity.to_string());
                params.insert("current_voltage".to_string(), actual.to_string());
                params.insert("required_voltage".to_string(), expected.to_string());
                params.insert("difference_percent".to_string(), format!("{:.1}", percent_diff));
                params
            },
        }
    }
    
    /// Generate repair suggestion for thermal violation
    pub fn suggest_thermal_repair(
        entity: &str,
        max_temp: f64,
        actual_temp: f64,
    ) -> RepairSuggestion {
        let diff = actual_temp - max_temp;
        
        let description = if diff < 5.0 {
            format!(
                "Add airflow to {} (need {}°C, running {}°C)",
                entity, max_temp, actual_temp
            )
        } else if diff < 15.0 {
            format!("Add heatsink to {}", entity)
        } else {
            format!(
                "Replace {} with lower-power component or add active cooling",
                entity
            )
        };
        
        RepairSuggestion {
            description,
            auto_fixable: false,
            justification: format!(
                "Component running {}°C but max is {}°C ({}°C over limit)",
                actual_temp, max_temp, diff
            ),
            parameters: {
                let mut params = HashMap::new();
                params.insert("entity".to_string(), entity.to_string());
                params.insert("max_temperature".to_string(), max_temp.to_string());
                params.insert("actual_temperature".to_string(), actual_temp.to_string());
                params.insert("overage".to_string(), diff.to_string());
                params
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_report_summary() {
        let violations = vec![
            ConstraintViolation {
                constraint_id: "v1".to_string(),
                entity: "net:VCC".to_string(),
                severity: "mandatory".to_string(),
                violation: "Voltage exceeded".to_string(),
                expected: "3.3V".to_string(),
                actual: "3.5V".to_string(),
                repairs: vec![],
            },
        ];

        let report = ConstraintReporter::generate_report("Test", &[], violations);

        assert_eq!(report.summary.total_violations, 1);
        assert_eq!(report.summary.mandatory_violations, 1);
        assert!((report.summary.pass_rate_percent - 0.0).abs() < 0.1);
    }

    #[test]
    fn test_voltage_repair_suggestion() {
        let repair = ConstraintReporter::suggest_voltage_repair("IC1", 3.3, 3.5);

        assert!(!repair.description.is_empty());
        assert!(repair.parameters.contains_key("entity"));
    }

    #[test]
    fn test_thermal_repair_suggestion() {
        let repair = ConstraintReporter::suggest_thermal_repair("IC1", 85.0, 95.0);

        assert!(!repair.description.is_empty());
        assert_eq!(repair.auto_fixable, false);
    }

    #[test]
    fn test_domain_extraction() {
        assert_eq!(
            ConstraintReporter::extract_domain("net:VCC"),
            "Electrical"
        );
        assert_eq!(
            ConstraintReporter::extract_domain("pipe:P1"),
            "MEP/Plumbing"
        );
        assert_eq!(
            ConstraintReporter::extract_domain("fixture:F1"),
            "Architectural"
        );
    }

    #[test]
    fn test_markdown_generation() {
        let violations = vec![
            ConstraintViolation {
                constraint_id: "test".to_string(),
                entity: "net:VCC".to_string(),
                severity: "mandatory".to_string(),
                violation: "Test violation".to_string(),
                expected: "3.3V".to_string(),
                actual: "3.5V".to_string(),
                repairs: vec![],
            },
        ];

        let report = ConstraintReporter::generate_report("Test", &[], violations);
        let markdown = ConstraintReporter::report_to_markdown(&report);

        assert!(markdown.contains("Test"));
        assert!(markdown.contains("Violation"));
    }
}
