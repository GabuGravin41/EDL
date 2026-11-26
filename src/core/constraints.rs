//! Constraint Framework for EDL v4+
//!
//! Provides abstract constraint specification, propagation, and violation detection.
//! Supports domain-specific constraint types (electronics, MEP, architecture, etc).
//!
//! Core concepts:
//! - Constraint: A rule that must be satisfied (e.g., "voltage <= 5V")
//! - ConstraintValue: Typed value supporting numeric, enum, and composite constraints
//! - ConstraintContext: Tracks constraint values, violations, and repair suggestions
//! - Domain: Namespace for domain-specific constraint types

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use std::collections::BTreeMap;
use crate::core::span::Span;

/// Constraint severity level determines whether a constraint must, should, or may be satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintSeverity {
    /// Must satisfy (design is non-permittable or dangerous if violated)
    Mandatory,
    /// Should satisfy (best practice but not strictly required)
    Strong,
    /// May violate with justification (informational)
    Weak,
}

/// Constraint operator for numeric/comparable constraints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintOp {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Between,  // min <= value <= max
}

/// Typed constraint value supporting different types of constraints
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "value_type", rename_all = "snake_case")]
pub enum ConstraintValue {
    /// Floating-point numeric constraint (e.g., voltage, current, resistance)
    Numeric {
        value: f64,
        unit: String,  // "V", "A", "Ohm", etc.
    },
    /// Integer numeric constraint (e.g., wire gauge, pin count)
    Integer {
        value: i64,
    },
    /// Enumerated constraint (e.g., material = "copper" | "aluminum")
    Enum {
        value: String,
        allowed: Vec<String>,
    },
    /// Composite constraint combining multiple values
    Composite {
        name: String,
        fields: BTreeMap<String, ConstraintValue>,
    },
    /// Boolean constraint (e.g., must_have_heatsink = true)
    Boolean {
        value: bool,
    },
}

impl ConstraintValue {
    /// Get numeric value if this is a Numeric constraint
    pub fn as_numeric(&self) -> Option<f64> {
        match self {
            ConstraintValue::Numeric { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// Get integer value if this is an Integer constraint
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            ConstraintValue::Integer { value } => Some(*value),
            _ => None,
        }
    }

    /// Get enum value if this is an Enum constraint
    pub fn as_enum(&self) -> Option<&String> {
        match self {
            ConstraintValue::Enum { value, .. } => Some(value),
            _ => None,
        }
    }

    /// Get boolean value if this is a Boolean constraint
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            ConstraintValue::Boolean { value } => Some(*value),
            _ => None,
        }
    }
}

/// Single constraint rule (e.g., "voltage must be 3.3V ±5%")
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Constraint {
    /// Globally unique constraint identifier (e.g., "elec/voltage_derating_1")
    pub id: String,
    /// Domain namespace (e.g., "elec", "plumbing", "electrical")
    pub domain: String,
    /// Human-readable constraint description
    pub description: String,
    /// Constraint severity
    pub severity: ConstraintSeverity,
    /// Constraint operator
    pub operator: ConstraintOp,
    /// Expected/required constraint value
    pub expected: ConstraintValue,
    /// Actual measured/observed constraint value
    pub actual: Option<ConstraintValue>,
    /// Source standard or citation (e.g., "IPC-2152", "NEC Table 310.16")
    pub standard: Option<String>,
    /// Location in source code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// Verification result for a constraint (whether it's satisfied)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ConstraintStatus {
    /// Constraint satisfied
    Satisfied,
    /// Constraint violated
    Violated {
        reason: String,
    },
    /// Unable to verify (missing data)
    Unverified {
        reason: String,
    },
}

/// Constraint violation record with repair suggestions
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintViolation {
    pub constraint_id: String,
    pub constraint_description: String,
    pub domain: String,
    pub severity: ConstraintSeverity,
    pub violation_reason: String,
    /// Domain-specific repair suggestions (e.g., "change wire to 2 AWG")
    pub repair_suggestions: Vec<RepairAction>,
}

/// Actionable repair for a constraint violation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RepairAction {
    /// Brief repair description
    pub description: String,
    /// Whether this can be auto-applied
    pub auto_fixable: bool,
    /// Citation/justification (e.g., "IPC-2152: 1mm² at 5A exceeds 10°C rise")
    pub justification: Option<String>,
    /// Specific parameter changes if auto-fixable
    pub changes: BTreeMap<String, String>,
}

/// Context tracking all constraints for a design entity
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintContext {
    /// Entity being constrained (e.g., "net_vout", "fixture_toilet_1")
    pub entity_id: String,
    /// Domain namespace
    pub domain: String,
    /// All constraints applicable to this entity
    pub constraints: Vec<Constraint>,
    /// Constraint violations found
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<ConstraintViolation>,
    /// Metadata about the entity
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl ConstraintContext {
    /// Create new constraint context for an entity
    pub fn new(entity_id: String, domain: String) -> Self {
        ConstraintContext {
            entity_id,
            domain,
            constraints: Vec::new(),
            violations: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Add a constraint to this context
    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Record a constraint violation
    pub fn record_violation(&mut self, violation: ConstraintViolation) {
        self.violations.push(violation);
    }

    /// Check if any mandatory constraints are violated
    pub fn has_mandatory_violations(&self) -> bool {
        self.violations.iter().any(|v| v.severity == ConstraintSeverity::Mandatory)
    }

    /// Get all violations for a specific domain
    pub fn violations_for_domain(&self, domain: &str) -> Vec<&ConstraintViolation> {
        self.violations.iter().filter(|v| v.domain == domain).collect()
    }
}

/// Domain-specific constraint verifier interface
pub trait ConstraintVerifier: Send + Sync {
    /// Unique domain identifier
    fn domain_name(&self) -> &'static str;

    /// Verify all constraints in a context, populate violations
    fn verify(&self, context: &mut ConstraintContext);

    /// Generate repair suggestions for a violation
    fn suggest_repairs(&self, violation: &ConstraintViolation) -> Vec<RepairAction>;
}

/// Registry of domain-specific constraint verifiers
pub struct ConstraintRegistry {
    verifiers: BTreeMap<String, Box<dyn ConstraintVerifier>>,
}

impl std::fmt::Debug for ConstraintRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConstraintRegistry")
            .field("domains", &self.domains())
            .finish()
    }
}

impl ConstraintRegistry {
    /// Create new empty registry
    pub fn new() -> Self {
        ConstraintRegistry {
            verifiers: BTreeMap::new(),
        }
    }

    /// Register a constraint verifier for a domain
    pub fn register(&mut self, verifier: Box<dyn ConstraintVerifier>) {
        self.verifiers.insert(verifier.domain_name().to_string(), verifier);
    }

    /// Get verifier for a domain
    pub fn get(&self, domain: &str) -> Option<&dyn ConstraintVerifier> {
        self.verifiers.get(domain).map(|v| v.as_ref())
    }

    /// List all registered domains
    pub fn domains(&self) -> Vec<&str> {
        self.verifiers.keys().map(|k| k.as_str()).collect()
    }

    /// Run all applicable verifiers on a context
    pub fn verify_all(&self, context: &mut ConstraintContext) {
        if let Some(verifier) = self.get(&context.domain) {
            verifier.verify(context);
        }
    }
}

impl Default for ConstraintRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_value_numeric() {
        let cv = ConstraintValue::Numeric {
            value: 3.3,
            unit: "V".to_string(),
        };
        assert_eq!(cv.as_numeric(), Some(3.3));
    }

    #[test]
    fn test_constraint_context_creation() {
        let ctx = ConstraintContext::new("net_vcc".to_string(), "elec".to_string());
        assert_eq!(ctx.entity_id, "net_vcc");
        assert_eq!(ctx.domain, "elec");
        assert!(ctx.constraints.is_empty());
        assert!(ctx.violations.is_empty());
    }

    #[test]
    fn test_constraint_context_add_constraint() {
        let mut ctx = ConstraintContext::new("net_vcc".to_string(), "elec".to_string());
        let constraint = Constraint {
            id: "vcc_voltage_1".to_string(),
            domain: "elec".to_string(),
            description: "VCC must be 3.3V ±5%".to_string(),
            severity: ConstraintSeverity::Mandatory,
            operator: ConstraintOp::Between,
            expected: ConstraintValue::Numeric {
                value: 3.3,
                unit: "V".to_string(),
            },
            actual: None,
            standard: Some("IPC-2152".to_string()),
            span: None,
        };
        ctx.add_constraint(constraint);
        assert_eq!(ctx.constraints.len(), 1);
    }

    #[test]
    fn test_constraint_registry() {
        let registry = ConstraintRegistry::new();
        assert!(registry.domains().is_empty());
    }
}
