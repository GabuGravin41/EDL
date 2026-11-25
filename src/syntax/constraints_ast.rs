//! Phase 1.2: Constraint Parsing and AST Integration
//!
//! This module provides constraint parsing and AST integration for EDL v4+
//! Enables constraints to be declared in EDL files and compiled into the IR

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use crate::core::constraints::{ConstraintSeverity, ConstraintOp};
use crate::core::span::Span;

/// Constraint declaration AST node
/// Represents a constraint declared in EDL source
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintDecl {
    /// Unique constraint identifier
    pub id: String,
    /// Constraint severity (mandatory, strong, weak)
    pub severity: ConstraintSeverity,
    /// Domain namespace (elec, plumbing, electrical, arch)
    pub domain: String,
    /// Target entity (net, component, pipe, fixture, group)
    pub target: ConstraintTarget,
    /// Constraint rules
    pub rules: Vec<ConstraintRule>,
    /// Source standard or citation
    pub standard: Option<String>,
    /// Human-readable description
    pub description: Option<String>,
    /// Source code location
    pub span: Option<Span>,
}

/// Reference to a design entity that constraint applies to
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "target_type", rename_all = "snake_case")]
pub enum ConstraintTarget {
    /// Net constraint (e.g., "net:VCC")
    Net {
        name: String,
    },
    /// Component constraint (e.g., "component:IC1")
    Component {
        name: String,
    },
    /// Pipe constraint (e.g., "pipe:main_line")
    Pipe {
        name: String,
    },
    /// Fixture constraint (e.g., "fixture:toilet_1")
    Fixture {
        name: String,
    },
    /// Group constraint applied to multiple entities (e.g., "group:power_stage")
    Group {
        name: String,
    },
}

impl ConstraintTarget {
    /// Get the entity name regardless of type
    pub fn name(&self) -> &str {
        match self {
            ConstraintTarget::Net { name } => name,
            ConstraintTarget::Component { name } => name,
            ConstraintTarget::Pipe { name } => name,
            ConstraintTarget::Fixture { name } => name,
            ConstraintTarget::Group { name } => name,
        }
    }

    /// Get the target type as a string
    pub fn target_type(&self) -> &'static str {
        match self {
            ConstraintTarget::Net { .. } => "net",
            ConstraintTarget::Component { .. } => "component",
            ConstraintTarget::Pipe { .. } => "pipe",
            ConstraintTarget::Fixture { .. } => "fixture",
            ConstraintTarget::Group { .. } => "group",
        }
    }
}

/// Single constraint rule (e.g., "voltage: >= 3.3V")
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintRule {
    /// Parameter name (e.g., "voltage", "current", "flow")
    pub parameter: String,
    /// Comparison operator
    pub operator: ConstraintOp,
    /// Expected/required value
    pub value: String,  // Will be parsed as number, enum, etc.
    /// Unit (e.g., "V", "A", "PSI", "GPM")
    pub unit: Option<String>,
}

/// Constraint tolerance specification (e.g., 3.3V ±5%)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintTolerance {
    pub nominal: f64,
    pub tolerance_pct: f64,
    pub unit: String,
}

/// Range constraint (e.g., height in [17in, 19in])
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintRange {
    pub min: f64,
    pub max: f64,
    pub unit: String,
}

/// Parametric constraint using formulas
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ParametricConstraint {
    /// Formula or function name (e.g., "ipc_2152_thermal_derating")
    pub formula: String,
    /// Parameters to the formula
    pub params: Vec<String>,
}

/// Conditional constraint (applied only if condition is true)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConditionalConstraint {
    /// Condition expression (e.g., "power_dissipation > 1.0W")
    pub condition: String,
    /// Constraint to apply if condition is true
    pub then_constraint: Box<ConstraintDecl>,
}

/// Parsing errors
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ConstraintParseError {
    /// Missing required field
    MissingField { field: String, constraint_id: String },
    /// Unknown severity level
    UnknownSeverity { severity: String },
    /// Invalid operator
    InvalidOperator { operator: String },
    /// Invalid entity reference
    InvalidEntityReference { reference: String },
    /// Malformed numeric value
    InvalidNumericValue { value: String },
    /// Unknown unit
    UnknownUnit { unit: String },
    /// Span information
    Span(Option<Span>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_target_net() {
        let target = ConstraintTarget::Net {
            name: "VCC".to_string(),
        };
        assert_eq!(target.name(), "VCC");
        assert_eq!(target.target_type(), "net");
    }

    #[test]
    fn test_constraint_target_component() {
        let target = ConstraintTarget::Component {
            name: "IC1".to_string(),
        };
        assert_eq!(target.name(), "IC1");
        assert_eq!(target.target_type(), "component");
    }

    #[test]
    fn test_constraint_declaration() {
        let constraint = ConstraintDecl {
            id: "voltage_vcc".to_string(),
            severity: ConstraintSeverity::Mandatory,
            domain: "elec".to_string(),
            target: ConstraintTarget::Net {
                name: "VCC".to_string(),
            },
            rules: vec![ConstraintRule {
                parameter: "operating_voltage".to_string(),
                operator: ConstraintOp::GreaterThanOrEqual,
                value: "3.3".to_string(),
                unit: Some("V".to_string()),
            }],
            standard: Some("IEC 60950-1".to_string()),
            description: Some("VCC must be safe operating range".to_string()),
            span: None,
        };

        assert_eq!(constraint.id, "voltage_vcc");
        assert_eq!(constraint.domain, "elec");
        assert_eq!(constraint.rules.len(), 1);
    }

    #[test]
    fn test_constraint_tolerance() {
        let tolerance = ConstraintTolerance {
            nominal: 3.3,
            tolerance_pct: 5.0,
            unit: "V".to_string(),
        };

        let min = tolerance.nominal * (1.0 - tolerance.tolerance_pct / 100.0);
        let max = tolerance.nominal * (1.0 + tolerance.tolerance_pct / 100.0);

        assert!(min < 3.3 && 3.3 < max);
    }

    #[test]
    fn test_constraint_range() {
        let range = ConstraintRange {
            min: 17.0,
            max: 19.0,
            unit: "in".to_string(),
        };

        assert!(range.min <= 18.0 && 18.0 <= range.max);
        assert!(range.min > 16.0);
    }
}
