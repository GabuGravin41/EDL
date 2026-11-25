//! Electronics Domain Constraint Verifier for EDL v4+
//!
//! Implements domain-specific constraint verification for electronics schematics.
//! Covers: voltage derating (E1), current derating (E2), thermal pathways (E3),
//! EMI/SI isolation (E4).

use crate::core::constraints::*;
use std::collections::BTreeMap;

/// Electronics domain constraint verifier
pub struct ElecConstraintVerifier;

impl ElecConstraintVerifier {
    /// Create a new electronics constraint verifier
    pub fn new() -> Self {
        ElecConstraintVerifier
    }

    /// E1: Verify voltage derating (component rating >= operating + margin)
    fn verify_voltage_derating(context: &mut ConstraintContext) {
        // Look for voltage constraints in metadata
        let operating_v = context
            .metadata
            .get("operating_voltage_v")
            .and_then(|v| v.as_f64());
        let rated_v = context
            .metadata
            .get("rated_voltage_v")
            .and_then(|v| v.as_f64());
        let tolerance_pct = context
            .metadata
            .get("tolerance_pct")
            .and_then(|v| v.as_f64())
            .unwrap_or(5.0);

        if let (Some(operating), Some(rated)) = (operating_v, rated_v) {
            let min_rated = operating * (1.0 + tolerance_pct / 100.0);
            if rated < min_rated {
                let violation = ConstraintViolation {
                    constraint_id: "elec_e1_voltage_derating".to_string(),
                    constraint_description: "Voltage derating: component rating must exceed operating + margin".to_string(),
                    domain: "elec".to_string(),
                    severity: ConstraintSeverity::Mandatory,
                    violation_reason: format!(
                        "Component rated {}V but requires {}V (operating {}V + {}% margin)",
                        rated, min_rated, operating, tolerance_pct
                    ),
                    repair_suggestions: vec![
                        RepairAction {
                            description: format!(
                                "Replace with component rated {}V or higher",
                                min_rated.ceil()
                            ),
                            auto_fixable: false,
                            justification: Some(
                                "Component selection must be manual to ensure parametric accuracy"
                                    .to_string(),
                            ),
                            changes: BTreeMap::from([(
                                "rated_voltage_v".to_string(),
                                format!("{:.1}", min_rated.ceil()),
                            )]),
                        },
                    ],
                };
                context.record_violation(violation);
            }
        }
    }

    /// E2: Verify current derating (trace/component rating >= operating + margin)
    fn verify_current_derating(context: &mut ConstraintContext) {
        let operating_a = context
            .metadata
            .get("operating_current_a")
            .and_then(|v| v.as_f64());
        let rated_a = context
            .metadata
            .get("rated_current_a")
            .and_then(|v| v.as_f64());
        let margin_pct = context
            .metadata
            .get("current_margin_pct")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0);

        if let (Some(operating), Some(rated)) = (operating_a, rated_a) {
            let min_rated = operating * (1.0 + margin_pct / 100.0);
            if rated < min_rated {
                // IPC-2152 thermal derating calculation
                let trace_width_mm = context
                    .metadata
                    .get("trace_width_mm")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0);
                let trace_thickness_oz = context
                    .metadata
                    .get("trace_thickness_oz")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0);

                // Simplified IPC-2152: I = k * A^b where A is cross-section in mil^2
                let cross_section_mm2 = trace_width_mm * (trace_thickness_oz * 0.035);
                let ipc_capacity = cross_section_mm2 * 10.0; // Simplified formula

                let violations = vec![
                    RepairAction {
                        description: format!(
                            "Increase trace width to {}mm for {}A capacity (IPC-2152)",
                            (trace_width_mm * ipc_capacity / operating).ceil(),
                            operating
                        ),
                        auto_fixable: true,
                        justification: Some(
                            "IPC-2152 thermal derating of traces. Recommended 10% margin for safety."
                                .to_string(),
                        ),
                        changes: BTreeMap::from([(
                            "trace_width_mm".to_string(),
                            format!("{:.2}", (trace_width_mm * ipc_capacity / operating).ceil()),
                        )]),
                    },
                    RepairAction {
                        description: format!(
                            "Replace component with higher-rated variant ({}A minimum)",
                            min_rated.ceil()
                        ),
                        auto_fixable: false,
                        justification: Some(
                            "Component selection must verify package, thermal resistance, etc."
                                .to_string(),
                        ),
                        changes: BTreeMap::from([(
                            "rated_current_a".to_string(),
                            format!("{:.1}", min_rated.ceil()),
                        )]),
                    },
                ];

                let violation = ConstraintViolation {
                    constraint_id: "elec_e2_current_derating".to_string(),
                    constraint_description:
                        "Current derating: trace/component rating must exceed operating + margin"
                            .to_string(),
                    domain: "elec".to_string(),
                    severity: ConstraintSeverity::Mandatory,
                    violation_reason: format!(
                        "{}A operating current exceeds {}A rating ({}% margin required)",
                        operating, rated, margin_pct
                    ),
                    repair_suggestions: violations,
                };
                context.record_violation(violation);
            }
        }
    }

    /// E3: Verify thermal pathway (power dissipation has valid cooling)
    fn verify_thermal_pathway(context: &mut ConstraintContext) {
        let power_mw = context
            .metadata
            .get("power_dissipation_mw")
            .and_then(|v| v.as_f64());
        let tj_max_c = context
            .metadata
            .get("tj_max_c")
            .and_then(|v| v.as_f64())
            .unwrap_or(125.0);
        let ta_max_c = context
            .metadata
            .get("ta_max_c")
            .and_then(|v| v.as_f64())
            .unwrap_or(70.0);

        if let Some(power) = power_mw {
            if power < 1.0 {
                return; // Skip for low-power devices
            }

            let theta_ja_required = (tj_max_c - ta_max_c) / (power / 1000.0);
            let has_heatsink = context
                .metadata
                .get("has_heatsink")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let theta_ja_actual = if has_heatsink { 15.0 } else { 150.0 };

            if theta_ja_actual > theta_ja_required {
                let violation = ConstraintViolation {
                    constraint_id: "elec_e3_thermal_pathway".to_string(),
                    constraint_description: "Thermal pathway: device must not exceed Tj_max".to_string(),
                    domain: "elec".to_string(),
                    severity: ConstraintSeverity::Mandatory,
                    violation_reason: format!(
                        "{}mW dissipation requires θ_JA <= {:.1}°C/W but actual is {:.1}°C/W",
                        power, theta_ja_required, theta_ja_actual
                    ),
                    repair_suggestions: vec![
                        RepairAction {
                            description: format!(
                                "Add heatsink with thermal resistance <= {:.1}°C/W",
                                theta_ja_required
                            ),
                            auto_fixable: false,
                            justification: Some(
                                "Thermal resistance calculation: θ = ΔT / P = {} / {} = {:.1}°C/W"
                                    .to_string(),
                            ),
                            changes: BTreeMap::from([(
                                "has_heatsink".to_string(),
                                "true".to_string(),
                            )]),
                        },
                        RepairAction {
                            description: format!(
                                "Reduce operating current/power or select lower-power component"
                            ),
                            auto_fixable: false,
                            justification: Some(
                                "Lower power dissipation reduces thermal stress".to_string(),
                            ),
                            changes: BTreeMap::new(),
                        },
                    ],
                };
                context.record_violation(violation);
            }
        }
    }

    /// E4: Verify EMI/SI isolation (sensitive nets separated)
    fn verify_emi_isolation(context: &mut ConstraintContext) {
        let min_clearance_mm = context
            .metadata
            .get("min_clearance_mm")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0);
        let actual_clearance_mm = context
            .metadata
            .get("actual_clearance_mm")
            .and_then(|v| v.as_f64());
        let is_sensitive = context
            .metadata
            .get("is_sensitive_net")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let near_power = context
            .metadata
            .get("near_power_plane")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_sensitive && near_power {
            if let Some(clearance) = actual_clearance_mm {
                if clearance < min_clearance_mm {
                    let violation = ConstraintViolation {
                        constraint_id: "elec_e4_emi_isolation".to_string(),
                        constraint_description:
                            "EMI/SI isolation: sensitive nets must be separated from power"
                                .to_string(),
                        domain: "elec".to_string(),
                        severity: ConstraintSeverity::Strong,
                        violation_reason: format!(
                            "Sensitive net only {}mm from power plane (minimum {}mm required)",
                            clearance, min_clearance_mm
                        ),
                        repair_suggestions: vec![RepairAction {
                            description: format!(
                                "Route sensitive net at least {}mm away from power planes",
                                min_clearance_mm
                            ),
                            auto_fixable: false,
                            justification: Some(
                                "EMI immunity requires separation from switching noise sources."
                                    .to_string(),
                            ),
                            changes: BTreeMap::from([(
                                "actual_clearance_mm".to_string(),
                                format!("{:.1}", min_clearance_mm),
                            )]),
                        }],
                    };
                    context.record_violation(violation);
                }
            }
        }
    }
}

impl Default for ElecConstraintVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintVerifier for ElecConstraintVerifier {
    fn domain_name(&self) -> &'static str {
        "elec"
    }

    fn verify(&self, context: &mut ConstraintContext) {
        // Run all electronics verifications
        Self::verify_voltage_derating(context);
        Self::verify_current_derating(context);
        Self::verify_thermal_pathway(context);
        Self::verify_emi_isolation(context);
    }

    fn suggest_repairs(&self, violation: &ConstraintViolation) -> Vec<RepairAction> {
        violation.repair_suggestions.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voltage_derating_violation() {
        let mut context = ConstraintContext::new("component_1".to_string(), "elec".to_string());
        context.metadata.insert(
            "operating_voltage_v".to_string(),
            serde_json::json!(3.6),
        );
        context.metadata.insert(
            "rated_voltage_v".to_string(),
            serde_json::json!(3.3),
        );
        context.metadata.insert(
            "tolerance_pct".to_string(),
            serde_json::json!(5.0),
        );

        let verifier = ElecConstraintVerifier::new();
        verifier.verify(&mut context);

        assert!(!context.violations.is_empty());
        assert_eq!(context.violations[0].constraint_id, "elec_e1_voltage_derating");
    }

    #[test]
    fn test_current_derating_violation() {
        let mut context = ConstraintContext::new("trace_1".to_string(), "elec".to_string());
        context.metadata.insert(
            "operating_current_a".to_string(),
            serde_json::json!(5.0),
        );
        context.metadata.insert(
            "rated_current_a".to_string(),
            serde_json::json!(3.0),
        );
        context.metadata.insert(
            "trace_width_mm".to_string(),
            serde_json::json!(1.0),
        );

        let verifier = ElecConstraintVerifier::new();
        verifier.verify(&mut context);

        assert!(!context.violations.is_empty());
        assert_eq!(context.violations[0].constraint_id, "elec_e2_current_derating");
    }

    #[test]
    fn test_thermal_violation() {
        let mut context = ConstraintContext::new("ic_1".to_string(), "elec".to_string());
        context.metadata.insert(
            "power_dissipation_mw".to_string(),
            serde_json::json!(2000.0),
        );
        context.metadata.insert("tj_max_c".to_string(), serde_json::json!(125.0));
        context.metadata.insert("ta_max_c".to_string(), serde_json::json!(70.0));
        context.metadata.insert("has_heatsink".to_string(), serde_json::json!(false));

        let verifier = ElecConstraintVerifier::new();
        verifier.verify(&mut context);

        assert!(!context.violations.is_empty());
        assert_eq!(context.violations[0].constraint_id, "elec_e3_thermal_pathway");
    }

    #[test]
    fn test_no_violations_when_compliant() {
        let mut context = ConstraintContext::new("trace_ok".to_string(), "elec".to_string());
        context.metadata.insert(
            "operating_current_a".to_string(),
            serde_json::json!(1.0),
        );
        context.metadata.insert(
            "rated_current_a".to_string(),
            serde_json::json!(5.0),
        );

        let verifier = ElecConstraintVerifier::new();
        verifier.verify(&mut context);

        assert!(context.violations.is_empty());
    }
}
