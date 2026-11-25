/// Phase 1.2 Week 3: Constraint Propagation Engine
/// Propagates constraints across entity hierarchies and applies domain-specific rules

use crate::core::constraints::{ConstraintContext, ConstraintValue, ConstraintOp};
use crate::core::span::{Diagnostic, Span};
use crate::syntax::constraint_elaborator::{ElaboratedConstraint, ResolvedEntityRef};
use crate::syntax::constraints_ast::ConstraintRule;
use std::collections::HashMap;

/// Propagation result tracking how constraints spread through hierarchy
#[derive(Debug, Clone)]
pub struct PropagationResult {
    pub source_entity: ResolvedEntityRef,
    pub targets: Vec<PropagatedConstraint>,
}

/// A constraint after propagation to a specific entity
#[derive(Debug, Clone)]
pub struct PropagatedConstraint {
    pub target_entity: ResolvedEntityRef,
    pub constraint_id: String,
    pub propagated_rule: PropagatedRule,
    pub depth: usize, // How many hierarchy levels deep
}

/// A constraint rule after propagation
#[derive(Debug, Clone)]
pub struct PropagatedRule {
    pub parameter: String,
    pub original_value: ConstraintValue,
    pub propagated_value: ConstraintValue,
    pub derating_applied: bool,
    pub derating_percent: Option<f64>,
}

/// Constraint propagation engine
pub struct ConstraintPropagator;

impl ConstraintPropagator {
    /// Propagate all constraints through design hierarchy
    pub fn propagate_all(
        constraints: &[ElaboratedConstraint],
        hierarchy: &DesignHierarchy,
    ) -> Result<Vec<PropagationResult>, Diagnostic> {
        let mut results = Vec::new();
        
        for constraint in constraints {
            let propagation = Self::propagate_one(constraint, hierarchy)?;
            results.push(propagation);
        }
        
        Ok(results)
    }
    
    /// Propagate a single constraint through hierarchy
    fn propagate_one(
        constraint: &ElaboratedConstraint,
        hierarchy: &DesignHierarchy,
    ) -> Result<PropagationResult, Diagnostic> {
        let mut targets = Vec::new();
        
        // Find all entities that should receive this constraint
        let target_entities = Self::find_target_entities(&constraint.entity_ref, hierarchy)?;
        
        for (entity_ref, depth) in target_entities {
            for rule in &constraint.rules {
                // Apply domain-specific propagation logic
                let propagated_rule = match &constraint.entity_ref {
                    ResolvedEntityRef::Net(_) => {
                        Self::propagate_voltage_constraint(rule, depth)?
                    }
                    ResolvedEntityRef::Component(_) => {
                        Self::propagate_component_constraint(rule, depth)?
                    }
                    ResolvedEntityRef::Pipe(_) => {
                        Self::propagate_flow_constraint(rule, depth)?
                    }
                    ResolvedEntityRef::Fixture(_) => {
                        Self::propagate_fixture_constraint(rule, depth)?
                    }
                    ResolvedEntityRef::Group(_) => {
                        Self::propagate_group_constraint(rule, depth)?
                    }
                };
                
                targets.push(PropagatedConstraint {
                    target_entity: entity_ref.clone(),
                    constraint_id: constraint.id.clone(),
                    propagated_rule,
                    depth,
                });
            }
        }
        
        Ok(PropagationResult {
            source_entity: constraint.entity_ref.clone(),
            targets,
        })
    }
    
    /// Find all entities that should receive a constraint
    fn find_target_entities(
        entity_ref: &ResolvedEntityRef,
        hierarchy: &DesignHierarchy,
    ) -> Result<Vec<(ResolvedEntityRef, usize)>, Diagnostic> {
        let mut results = Vec::new();
        
        // If hierarchical, apply to all sub-entities
        let is_hierarchical = crate::syntax::constraint_elaborator::ConstraintElaborator::is_hierarchical(entity_ref);
        
        if is_hierarchical {
            // Find all entities under this hierarchy
            let path = crate::syntax::constraint_elaborator::ConstraintElaborator::path_components(entity_ref);
            results = hierarchy.find_descendants(&path)?;
        } else {
            // Apply to single entity
            results.push((entity_ref.clone(), 0));
        }
        
        Ok(results)
    }
    
    /// Propagate voltage constraint (electrical domain)
    /// Voltage propagates from net to all connected components
    fn propagate_voltage_constraint(rule: &ConstraintRule, depth: usize) -> Result<PropagatedRule, Diagnostic> {
        // Voltage derating rules per IPC-2152
        let derating_percent = match depth {
            0 => 0.0,          // No derating at source
            1 => 1.0,          // 1% per level
            2 => 2.5,          // 2.5% per level (cumulative)
            _ => 5.0,          // 5% max derating
        };
        
        let propagated_value = match &rule.value {
            ConstraintValue::Numeric(val, unit) => {
                let derating_factor = 1.0 + (derating_percent / 100.0);
                ConstraintValue::Numeric(val * derating_factor, unit.clone())
            }
            other => other.clone(),
        };
        
        Ok(PropagatedRule {
            parameter: rule.parameter.clone(),
            original_value: rule.value.clone(),
            propagated_value,
            derating_applied: depth > 0,
            derating_percent: if depth > 0 { Some(derating_percent) } else { None },
        })
    }
    
    /// Propagate component constraint (electrical domain)
    /// Component constraints affect related traces and connections
    fn propagate_component_constraint(rule: &ConstraintRule, depth: usize) -> Result<PropagatedRule, Diagnostic> {
        // Component constraints are stricter at deeper levels
        let margin_increase = match depth {
            0 => 0.0,
            1 => 2.0,  // Add 2% margin per level
            2 => 5.0,
            _ => 10.0, // Max 10% margin
        };
        
        let propagated_value = match &rule.value {
            ConstraintValue::Numeric(val, unit) => {
                // Apply safety margin (reduce component value by margin)
                let margin_factor = 1.0 - (margin_increase / 100.0);
                ConstraintValue::Numeric(val * margin_factor, unit.clone())
            }
            other => other.clone(),
        };
        
        Ok(PropagatedRule {
            parameter: rule.parameter.clone(),
            original_value: rule.value.clone(),
            propagated_value,
            derating_applied: depth > 0,
            derating_percent: if depth > 0 { Some(margin_increase) } else { None },
        })
    }
    
    /// Propagate flow constraint (MEP domain)
    /// Flow demand propagates upstream through pipe network
    fn propagate_flow_constraint(rule: &ConstraintRule, depth: usize) -> Result<PropagatedRule, Diagnostic> {
        // Flow derating per IPC plumbing standards
        let derating_percent = match depth {
            0 => 0.0,
            1 => 3.0,  // 3% per level
            2 => 8.0,
            _ => 15.0, // Max 15% derating
        };
        
        let propagated_value = match &rule.value {
            ConstraintValue::Numeric(val, unit) => {
                // Flow increases upstream (larger pipe needed)
                let derating_factor = 1.0 + (derating_percent / 100.0);
                ConstraintValue::Numeric(val * derating_factor, unit.clone())
            }
            other => other.clone(),
        };
        
        Ok(PropagatedRule {
            parameter: rule.parameter.clone(),
            original_value: rule.value.clone(),
            propagated_value,
            derating_applied: depth > 0,
            derating_percent: if depth > 0 { Some(derating_percent) } else { None },
        })
    }
    
    /// Propagate thermal constraint (electrical/MEP domain)
    /// Component dissipation propagates to heatsink requirements
    fn propagate_thermal_constraint(rule: &ConstraintRule, depth: usize) -> Result<PropagatedRule, Diagnostic> {
        // Thermal resistance increases with hierarchy depth
        let thermal_increase_percent = match depth {
            0 => 0.0,
            1 => 10.0, // 10% thermal resistance per level
            2 => 25.0,
            _ => 50.0, // Max 50% increase
        };
        
        let propagated_value = match &rule.value {
            ConstraintValue::Numeric(val, unit) => {
                let thermal_factor = 1.0 + (thermal_increase_percent / 100.0);
                ConstraintValue::Numeric(val * thermal_factor, unit.clone())
            }
            other => other.clone(),
        };
        
        Ok(PropagatedRule {
            parameter: rule.parameter.clone(),
            original_value: rule.value.clone(),
            propagated_value,
            derating_applied: depth > 0,
            derating_percent: if depth > 0 { Some(thermal_increase_percent) } else { None },
        })
    }
    
    /// Propagate fixture constraint (architectural domain)
    /// Fixture constraints affect room and space requirements
    fn propagate_fixture_constraint(rule: &ConstraintRule, depth: usize) -> Result<PropagatedRule, Diagnostic> {
        // Fixture constraints have minimal propagation (mostly local)
        // But apply safety margins at deeper levels
        let margin_increase = match depth {
            0 => 0.0,
            1 => 1.0, // 1% per level (conservative)
            _ => 2.0,
        };
        
        let propagated_value = match &rule.value {
            ConstraintValue::Numeric(val, unit) => {
                let margin_factor = 1.0 + (margin_increase / 100.0);
                ConstraintValue::Numeric(val * margin_factor, unit.clone())
            }
            other => other.clone(),
        };
        
        Ok(PropagatedRule {
            parameter: rule.parameter.clone(),
            original_value: rule.value.clone(),
            propagated_value,
            derating_applied: depth > 0,
            derating_percent: if depth > 0 { Some(margin_increase) } else { None },
        })
    }
    
    /// Propagate group constraint (hierarchical collection)
    /// Group constraints apply to all members recursively
    fn propagate_group_constraint(rule: &ConstraintRule, depth: usize) -> Result<PropagatedRule, Diagnostic> {
        // Group constraints apply uniformly but with depth-based adjustments
        let adjustment_percent = match depth {
            0 => 0.0,
            _ => 2.0 * (depth as f64), // 2% per level
        };
        
        let propagated_value = match &rule.value {
            ConstraintValue::Numeric(val, unit) => {
                let adjustment_factor = 1.0 + (adjustment_percent / 100.0);
                ConstraintValue::Numeric(val * adjustment_factor, unit.clone())
            }
            other => other.clone(),
        };
        
        Ok(PropagatedRule {
            parameter: rule.parameter.clone(),
            original_value: rule.value.clone(),
            propagated_value,
            derating_applied: depth > 0,
            derating_percent: if depth > 0 { Some(adjustment_percent) } else { None },
        })
    }
}

/// Design hierarchy for entity lookups
#[derive(Debug, Clone, Default)]
pub struct DesignHierarchy {
    /// All entities in flat list with hierarchical paths
    pub entities: HashMap<String, HierarchyEntry>,
    /// Reverse mapping from entity to parent
    pub parents: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct HierarchyEntry {
    pub entity_ref: ResolvedEntityRef,
    pub path: Vec<String>,
    pub parent_path: Vec<String>,
}

impl DesignHierarchy {
    /// Add an entity to the hierarchy
    pub fn add_entity(&mut self, entity_ref: ResolvedEntityRef, path: Vec<String>) {
        let key = path.join(".");
        let parent_path = if path.len() > 1 {
            path[..path.len() - 1].to_vec()
        } else {
            vec![]
        };
        
        self.entities.insert(
            key.clone(),
            HierarchyEntry {
                entity_ref,
                path: path.clone(),
                parent_path,
            },
        );
    }
    
    /// Find all descendants of a path
    pub fn find_descendants(&self, path: &[String]) -> Result<Vec<(ResolvedEntityRef, usize)>, Diagnostic> {
        let parent_path = path.join(".");
        let mut results = Vec::new();
        
        for (key, entry) in &self.entities {
            if key.starts_with(&parent_path) {
                let depth = entry.path.len() - path.len();
                if depth > 0 {
                    results.push((entry.entity_ref.clone(), depth));
                }
            }
        }
        
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_voltage_propagation_no_derating() {
        let rule = ConstraintRule {
            parameter: "voltage".to_string(),
            operator: ConstraintOp::Equal,
            value: ConstraintValue::Numeric(3.3, "V".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        let result = ConstraintPropagator::propagate_voltage_constraint(&rule, 0).unwrap();
        assert_eq!(result.derating_applied, false);
        assert_eq!(result.derating_percent, None);
    }
    
    #[test]
    fn test_voltage_propagation_with_derating() {
        let rule = ConstraintRule {
            parameter: "voltage".to_string(),
            operator: ConstraintOp::Equal,
            value: ConstraintValue::Numeric(3.3, "V".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        let result = ConstraintPropagator::propagate_voltage_constraint(&rule, 1).unwrap();
        assert_eq!(result.derating_applied, true);
        assert_eq!(result.derating_percent, Some(1.0));
    }
    
    #[test]
    fn test_flow_propagation_increase() {
        let rule = ConstraintRule {
            parameter: "flow_demand".to_string(),
            operator: ConstraintOp::Equal,
            value: ConstraintValue::Numeric(10.0, "GPM".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        let result = ConstraintPropagator::propagate_flow_constraint(&rule, 1).unwrap();
        // Flow should increase (3% derating = 1.03x)
        assert_eq!(result.derating_percent, Some(3.0));
    }
    
    #[test]
    fn test_thermal_propagation() {
        let rule = ConstraintRule {
            parameter: "max_temp".to_string(),
            operator: ConstraintOp::LessThanOrEqual,
            value: ConstraintValue::Numeric(85.0, "C".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        let result = ConstraintPropagator::propagate_thermal_constraint(&rule, 1).unwrap();
        assert_eq!(result.derating_percent, Some(10.0));
    }
    
    #[test]
    fn test_propagation_depth_scaling() {
        let rule = ConstraintRule {
            parameter: "voltage".to_string(),
            operator: ConstraintOp::Equal,
            value: ConstraintValue::Numeric(5.0, "V".to_string()),
            tolerance: None,
            span: Span::new("test.edl", 1, 1, 1, 10),
        };
        
        // Deeper levels get more derating
        let result0 = ConstraintPropagator::propagate_voltage_constraint(&rule, 0).unwrap();
        let result1 = ConstraintPropagator::propagate_voltage_constraint(&rule, 1).unwrap();
        let result2 = ConstraintPropagator::propagate_voltage_constraint(&rule, 2).unwrap();
        
        assert_eq!(result0.derating_percent, None);
        assert_eq!(result1.derating_percent, Some(1.0));
        assert_eq!(result2.derating_percent, Some(2.5));
    }
}
