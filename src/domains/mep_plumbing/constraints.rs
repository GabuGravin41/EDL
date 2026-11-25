//! MEP Plumbing Domain Constraint Verifier for EDL v4+
//!
//! Implements domain-specific constraint verification for plumbing schematics.
//! Covers: flow demand sizing (P1), pressure drop (P2), pipe material (P3),
//! accessibility (P4), code compliance (P5).

use crate::core::constraints::*;
use std::collections::BTreeMap;

/// MEP Plumbing domain constraint verifier
pub struct PlumbingConstraintVerifier;

impl PlumbingConstraintVerifier {
    /// Create a new plumbing constraint verifier
    pub fn new() -> Self {
        PlumbingConstraintVerifier
    }

    /// P1: Verify flow demand sizing (IPC Table 422.1)
    fn verify_flow_demand(context: &mut ConstraintContext) {
        let occupancy = context
            .metadata
            .get("occupancy")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let required_demand_gpm = context
            .metadata
            .get("required_demand_gpm")
            .and_then(|v| v.as_f64());
        let calculated_demand_gpm = context
            .metadata
            .get("calculated_demand_gpm")
            .and_then(|v| v.as_f64());

        if let (Some(required), Some(calculated)) = (required_demand_gpm, calculated_demand_gpm) {
            if calculated < required {
                // IPC Table 422.1 demand factors
                let demand_per_fixture = 0.5; // Simplified
                let calculated_expected = occupancy as f64 * demand_per_fixture;

                let violation = ConstraintViolation {
                    constraint_id: "plumb_p1_flow_demand".to_string(),
                    constraint_description: "Flow demand: system must supply required GPM (IPC 422.1)"
                        .to_string(),
                    domain: "plumbing".to_string(),
                    severity: ConstraintSeverity::Mandatory,
                    violation_reason: format!(
                        "Design provides {:.1} GPM but {} occupancy requires {:.1} GPM",
                        calculated, occupancy, required
                    ),
                    repair_suggestions: vec![
                        RepairAction {
                            description: format!(
                                "Upsize main line to supply {:.1} GPM (IPC Table 422.1)",
                                required
                            ),
                            auto_fixable: false,
                            justification: Some(
                                format!("IPC 422.1: {} occupancy × {} GPM/fixture = {:.1} GPM required", 
                                    occupancy, demand_per_fixture, calculated_expected)
                            ),
                            changes: BTreeMap::from([(
                                "calculated_demand_gpm".to_string(),
                                format!("{:.1}", required),
                            )]),
                        },
                        RepairAction {
                            description: "Add booster pump to increase flow capacity".to_string(),
                            auto_fixable: false,
                            justification: Some(
                                "Booster pump can increase pressure and flow to meet demand"
                                    .to_string(),
                            ),
                            changes: BTreeMap::from([(
                                "has_booster_pump".to_string(),
                                "true".to_string(),
                            )]),
                        },
                    ],
                };
                context.record_violation(violation);
            }
        }
    }

    /// P2: Verify pressure drop (IPC Table 422.4, within budget)
    fn verify_pressure_drop(context: &mut ConstraintContext) {
        let flow_gpm = context
            .metadata
            .get("flow_gpm")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let pipe_diameter_in = context
            .metadata
            .get("pipe_diameter_in")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);
        let pipe_length_ft = context
            .metadata
            .get("pipe_length_ft")
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0);
        let max_pressure_drop_psi = context
            .metadata
            .get("max_pressure_drop_psi")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0);

        // Simplified Hazen-Williams equation: pressure drop ≈ flow^1.85 / diameter^4.87
        let velocity_fps = flow_gpm / (2.448 * pipe_diameter_in * pipe_diameter_in);
        let pressure_drop_per_100ft = (flow_gpm / 61.9) / (pipe_diameter_in.powf(1.63)); // Simplified
        let total_pressure_drop = (pressure_drop_per_100ft * pipe_length_ft) / 100.0;

        if total_pressure_drop > max_pressure_drop_psi {
            let violation = ConstraintViolation {
                constraint_id: "plumb_p2_pressure_drop".to_string(),
                constraint_description:
                    "Pressure drop: must not exceed budget (IPC Table 422.4)".to_string(),
                domain: "plumbing".to_string(),
                severity: ConstraintSeverity::Mandatory,
                violation_reason: format!(
                    "{:.1} GPM through {:.1}\" pipe over {} ft yields {:.1} PSI drop (max {} PSI)",
                    flow_gpm, pipe_diameter_in, pipe_length_ft, total_pressure_drop, max_pressure_drop_psi
                ),
                repair_suggestions: vec![
                    RepairAction {
                        description: format!(
                            "Increase pipe to {:.1}\" diameter (reduces drop to ~{:.1} PSI)",
                            pipe_diameter_in + 0.5,
                            total_pressure_drop * 0.4
                        ),
                        auto_fixable: false,
                        justification: Some(
                            "Pressure drop is proportional to 1/d^4.87; larger pipe exponentially reduces drop."
                                .to_string(),
                        ),
                        changes: BTreeMap::from([(
                            "pipe_diameter_in".to_string(),
                            format!("{:.1}", pipe_diameter_in + 0.5),
                        )]),
                    },
                    RepairAction {
                        description: "Add booster pump to overcome pressure drop".to_string(),
                        auto_fixable: false,
                        justification: Some(
                            format!("Booster pump adds {} PSI to maintain minimum 20 PSI at fixtures",
                                total_pressure_drop + 20.0)
                        ),
                        changes: BTreeMap::from([(
                            "pump_pressure_boost_psi".to_string(),
                            format!("{:.1}", total_pressure_drop + 20.0),
                        )]),
                    },
                ],
            };
            context.record_violation(violation);
        }
    }

    /// P3: Verify pipe material compatibility
    fn verify_material_compatibility(context: &mut ConstraintContext) {
        let fluid = context
            .metadata
            .get("fluid")
            .and_then(|v| v.as_str())
            .unwrap_or("water");
        let pipe_material = context
            .metadata
            .get("pipe_material")
            .and_then(|v| v.as_str())
            .unwrap_or("copper");

        // Simple material compatibility check
        let valid = match (fluid, pipe_material) {
            ("water", "copper") => true,
            ("water", "pvc") => true,
            ("water", "pex") => true,
            ("water", "galvanized") => true,
            ("hot_water", "copper") => true,
            ("hot_water", "pex") => true,
            ("hot_water", "pvc") => false, // PVC not suitable for hot water
            ("waste", "pvc") => true,
            ("waste", "cast_iron") => true,
            _ => false,
        };

        if !valid {
            let violation = ConstraintViolation {
                constraint_id: "plumb_p3_material_compat".to_string(),
                constraint_description: "Material compatibility: pipe must be suitable for fluid"
                    .to_string(),
                domain: "plumbing".to_string(),
                severity: ConstraintSeverity::Mandatory,
                violation_reason: format!(
                    "{} pipe is not suitable for {} service",
                    pipe_material, fluid
                ),
                repair_suggestions: vec![
                    RepairAction {
                        description: format!(
                            "Use copper or PEX pipe for {} service",
                            fluid
                        ),
                        auto_fixable: false,
                        justification: Some(format!(
                            "IPC 422: {} is compatible with {} but {} is not",
                            if fluid.contains("hot") { "copper/pex" } else { "copper/pvc/pex" },
                            fluid,
                            pipe_material
                        )),
                        changes: BTreeMap::from([(
                            "pipe_material".to_string(),
                            if fluid.contains("hot") {
                                "copper".to_string()
                            } else {
                                "pvc".to_string()
                            },
                        )]),
                    },
                ],
            };
            context.record_violation(violation);
        }
    }

    /// P4: Verify accessibility (ADA 2010 requirements)
    fn verify_accessibility(context: &mut ConstraintContext) {
        let fixture_height_in = context
            .metadata
            .get("fixture_height_in")
            .and_then(|v| v.as_f64());
        let fixture_type = context
            .metadata
            .get("fixture_type")
            .and_then(|v| v.as_str())
            .unwrap_or("sink");
        let needs_accessible = context
            .metadata
            .get("needs_accessible")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if needs_accessible {
            let (min_height, max_height) = match fixture_type {
                "sink" | "lavatory" => (32.0, 48.0),
                "toilet" => (17.0, 19.0),
                "grab_bar" => (33.0, 42.0),
                _ => return,
            };

            if let Some(height) = fixture_height_in {
                if height < min_height || height > max_height {
                    let violation = ConstraintViolation {
                        constraint_id: "plumb_p4_accessibility".to_string(),
                        constraint_description: "Accessibility: fixture must meet ADA height requirements"
                            .to_string(),
                        domain: "plumbing".to_string(),
                        severity: ConstraintSeverity::Mandatory,
                        violation_reason: format!(
                            "{} at {:.0}\" violates ADA (requires {:.0}-{:.0}\")",
                            fixture_type, height, min_height, max_height
                        ),
                        repair_suggestions: vec![
                            RepairAction {
                                description: format!(
                                    "Reposition {} to {:.0}\" height (ADA 2010 §{})",
                                    fixture_type,
                                    (min_height + max_height) / 2.0,
                                    if fixture_type == "toilet" { "604.1" } else { "603.3" }
                                ),
                                auto_fixable: false,
                                justification: Some(
                                    format!("ADA 2010 Standard for Accessible Design requires {} between {:.0}-{:.0}\"",
                                        fixture_type, min_height, max_height)
                                ),
                                changes: BTreeMap::from([(
                                    "fixture_height_in".to_string(),
                                    format!("{:.0}", (min_height + max_height) / 2.0),
                                )]),
                            },
                        ],
                    };
                    context.record_violation(violation);
                }
            }
        }
    }

    /// P5: Verify code compliance
    fn verify_code_compliance(context: &mut ConstraintContext) {
        let has_shutoff_valve = context
            .metadata
            .get("has_shutoff_valve")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let has_check_valve = context
            .metadata
            .get("has_check_valve")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !has_shutoff_valve {
            let violation = ConstraintViolation {
                constraint_id: "plumb_p5_code_shutoff".to_string(),
                constraint_description: "Code compliance: main line must have shutoff valve"
                    .to_string(),
                domain: "plumbing".to_string(),
                severity: ConstraintSeverity::Mandatory,
                violation_reason: "Main water line missing required shutoff valve (IPC 603.3)"
                    .to_string(),
                repair_suggestions: vec![
                    RepairAction {
                        description: "Add main shutoff valve after meter".to_string(),
                        auto_fixable: false,
                        justification: Some(
                            "IPC 603.3: Main supply must have accessible shutoff valve"
                                .to_string(),
                        ),
                        changes: BTreeMap::from([(
                            "has_shutoff_valve".to_string(),
                            "true".to_string(),
                        )]),
                    },
                ],
            };
            context.record_violation(violation);
        }

        if !has_check_valve {
            let violation = ConstraintViolation {
                constraint_id: "plumb_p5_code_backflow".to_string(),
                constraint_description: "Code compliance: system must have backflow prevention"
                    .to_string(),
                domain: "plumbing".to_string(),
                severity: ConstraintSeverity::Strong,
                violation_reason: "Backflow prevention valve missing (required in some jurisdictions)"
                    .to_string(),
                repair_suggestions: vec![
                    RepairAction {
                        description: "Add backflow preventer at main supply".to_string(),
                        auto_fixable: false,
                        justification: Some(
                            "Many codes require backflow prevention to protect public water supply"
                                .to_string(),
                        ),
                        changes: BTreeMap::from([(
                            "has_check_valve".to_string(),
                            "true".to_string(),
                        )]),
                    },
                ],
            };
            context.record_violation(violation);
        }
    }
}

impl Default for PlumbingConstraintVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintVerifier for PlumbingConstraintVerifier {
    fn domain_name(&self) -> &'static str {
        "plumbing"
    }

    fn verify(&self, context: &mut ConstraintContext) {
        // Run all plumbing verifications
        Self::verify_flow_demand(context);
        Self::verify_pressure_drop(context);
        Self::verify_material_compatibility(context);
        Self::verify_accessibility(context);
        Self::verify_code_compliance(context);
    }

    fn suggest_repairs(&self, violation: &ConstraintViolation) -> Vec<RepairAction> {
        violation.repair_suggestions.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flow_demand_violation() {
        let mut context = ConstraintContext::new("main_line".to_string(), "plumbing".to_string());
        context.metadata.insert("occupancy".to_string(), serde_json::json!(20));
        context.metadata.insert(
            "required_demand_gpm".to_string(),
            serde_json::json!(10.0),
        );
        context.metadata.insert(
            "calculated_demand_gpm".to_string(),
            serde_json::json!(5.0),
        );

        let verifier = PlumbingConstraintVerifier::new();
        verifier.verify(&mut context);

        assert!(!context.violations.is_empty());
        assert_eq!(context.violations[0].constraint_id, "plumb_p1_flow_demand");
    }

    #[test]
    fn test_material_compatibility_violation() {
        let mut context = ConstraintContext::new("hot_line".to_string(), "plumbing".to_string());
        context.metadata.insert("fluid".to_string(), serde_json::json!("hot_water"));
        context.metadata.insert("pipe_material".to_string(), serde_json::json!("pvc"));

        let verifier = PlumbingConstraintVerifier::new();
        verifier.verify(&mut context);

        assert!(!context.violations.is_empty());
        assert_eq!(context.violations[0].constraint_id, "plumb_p3_material_compat");
    }

    #[test]
    fn test_accessibility_violation() {
        let mut context = ConstraintContext::new("toilet_1".to_string(), "plumbing".to_string());
        context.metadata.insert("fixture_type".to_string(), serde_json::json!("toilet"));
        context.metadata.insert("fixture_height_in".to_string(), serde_json::json!(22.0));
        context.metadata.insert("needs_accessible".to_string(), serde_json::json!(true));

        let verifier = PlumbingConstraintVerifier::new();
        verifier.verify(&mut context);

        assert!(!context.violations.is_empty());
        assert_eq!(context.violations[0].constraint_id, "plumb_p4_accessibility");
    }
}
