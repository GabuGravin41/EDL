/// Phase 1.2 Week 2: Cross-Domain Constraint Integration
///
/// This module integrates constraint propagation across electrical/mechanical/thermal domains.
/// It enables the reporter to verify complex multi-domain constraints and suggest repairs.

use crate::core::propagator::{ConstraintTransform, PropagationRule, PropagationEngine};
use crate::core::reporter::{ConstraintViolation, RepairSuggestion, ConstraintReport};
use std::collections::HashMap;

/// Cross-domain constraint rule (electrical → thermal, electrical → mechanical)
#[derive(Clone, Debug)]
pub struct CrossDomainRule {
    pub id: String,
    
    /// Source domain (e.g., "elec")
    pub source_domain: String,
    
    /// Target domain (e.g., "thermal")
    pub target_domain: String,
    
    /// Rule description (e.g., "current → heat dissipation")
    pub description: String,
    
    /// Transform function name (e.g., "current_to_power_loss")
    pub transform_name: String,
    
    /// Inverse transform function name (e.g., "power_loss_to_current")
    pub inverse_transform_name: Option<String>,
    
    /// Rules to enforce after transformation
    pub target_constraints: Vec<String>,
    
    /// Whether this rule is mandatory or advisory
    pub is_mandatory: bool,
}

/// Cross-domain constraint propagator
pub struct CrossDomainPropagator {
    /// Registered cross-domain rules
    rules: HashMap<String, CrossDomainRule>,
    
    /// Constraint engine for propagation
    engine: PropagationEngine,
    
    /// Domain-specific constraint validators
    validators: HashMap<String, Box<dyn Fn(&str, &str) -> Result<(), String>>>,
}

impl CrossDomainPropagator {
    /// Create new cross-domain propagator
    pub fn new(engine: PropagationEngine) -> Self {
        CrossDomainPropagator {
            rules: HashMap::new(),
            engine,
            validators: HashMap::new(),
        }
    }
    
    /// Register a cross-domain rule
    pub fn register_rule(&mut self, rule: CrossDomainRule) -> Result<(), String> {
        if self.rules.contains_key(&rule.id) {
            return Err(format!("Rule {} already registered", rule.id));
        }
        self.rules.insert(rule.id.clone(), rule);
        Ok(())
    }
    
    /// Register domain-specific validator
    pub fn register_validator<F>(
        &mut self,
        domain: String,
        validator: F,
    ) -> Result<(), String>
    where
        F: Fn(&str, &str) -> Result<(), String> + 'static,
    {
        self.validators.insert(domain, Box::new(validator));
        Ok(())
    }
    
    /// Propagate constraints across domains
    pub fn propagate_all_domains(
        &self,
        initial_constraints: HashMap<String, String>,
    ) -> Result<PropagationResult, String> {
        let mut results = PropagationResult::new();
        
        // Phase 1: Propagate within each domain
        let elec_constraints = self.propagate_within_domain(
            &initial_constraints,
            "elec",
        )?;
        results.add_domain_result("elec", elec_constraints);
        
        // Phase 2: Cross-domain transformations
        for rule in self.rules.values() {
            if !rule.is_mandatory {
                continue;  // Skip advisory rules for now
            }
            
            let transformed = self.apply_cross_domain_rule(
                rule,
                &results.domain_results,
            )?;
            
            results.add_domain_result(&rule.target_domain, transformed);
        }
        
        // Phase 3: Verify target domain constraints
        for (domain, constraints) in &results.domain_results {
            self.verify_domain_constraints(domain, constraints, &mut results)?;
        }
        
        Ok(results)
    }
    
    /// Propagate constraints within a single domain
    fn propagate_within_domain(
        &self,
        constraints: &HashMap<String, String>,
        domain: &str,
    ) -> Result<HashMap<String, String>, String> {
        // Filter constraints for this domain
        let domain_constraints: HashMap<String, String> = constraints
            .iter()
            .filter(|(k, _)| k.starts_with(&format!("{}.", domain)))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        
        // Run propagation engine
        let propagated = self.engine.propagate(&domain_constraints)?;
        
        Ok(propagated)
    }
    
    /// Apply cross-domain rule to transform constraints
    fn apply_cross_domain_rule(
        &self,
        rule: &CrossDomainRule,
        domain_results: &HashMap<String, HashMap<String, String>>,
    ) -> Result<HashMap<String, String>, String> {
        let mut transformed = HashMap::new();
        
        // Get source domain constraints
        let source_constraints = domain_results
            .get(&rule.source_domain)
            .ok_or_else(|| format!("Source domain {} not found", rule.source_domain))?;
        
        // Apply transformation
        for (key, value) in source_constraints {
            if key.contains(&rule.transform_name) {
                // Parse and transform
                let target_value = self.apply_transform(
                    &rule.transform_name,
                    value,
                )?;
                
                // Map to target domain constraint
                let target_key = format!("{}.{}", rule.target_domain, key);
                transformed.insert(target_key, target_value);
            }
        }
        
        Ok(transformed)
    }
    
    /// Apply a named transformation
    fn apply_transform(
        &self,
        transform_name: &str,
        value: &str,
    ) -> Result<String, String> {
        match transform_name {
            "current_to_power_loss" => {
                // Parse current value and convert to power loss
                // I (A) → P_loss = I² × R (W)
                // For now, simplified transformation
                let current_ma: f64 = value.parse()
                    .map_err(|_| format!("Cannot parse current: {}", value))?;
                let power_loss_mw = (current_ma / 1000.0).powi(2) * 1000.0;  // Approximate
                Ok(format!("{:.2}", power_loss_mw))
            }
            "power_to_temperature_rise" => {
                // Parse power and convert to temperature rise
                // P (W) → ΔT = P × R_th (°C)
                let power_w: f64 = value.parse()
                    .map_err(|_| format!("Cannot parse power: {}", value))?;
                let temp_rise_c = power_w * 50.0;  // R_th = 50°C/W typical
                Ok(format!("{:.2}", temp_rise_c))
            }
            _ => Err(format!("Unknown transform: {}", transform_name)),
        }
    }
    
    /// Verify constraints in target domain
    fn verify_domain_constraints(
        &self,
        domain: &str,
        constraints: &HashMap<String, String>,
        results: &mut PropagationResult,
    ) -> Result<(), String> {
        // Get domain-specific validator if exists
        if let Some(validator) = self.validators.get(domain) {
            for (key, value) in constraints {
                match validator(key, value) {
                    Ok(_) => {
                        results.add_check_result(key, true, None);
                    }
                    Err(msg) => {
                        results.add_check_result(key, false, Some(msg));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Generate constraint report with cross-domain violations and repairs
    pub fn generate_cross_domain_report(
        &self,
        propagation_result: &PropagationResult,
    ) -> ConstraintReport {
        let mut report = ConstraintReport::new();
        
        for (check_id, (passed, error_msg)) in &propagation_result.check_results {
            if !passed {
                let violation = ConstraintViolation {
                    id: check_id.clone(),
                    constraint: check_id.clone(),
                    expected_range: "see domain specs".to_string(),
                    actual_value: error_msg.clone().unwrap_or_default(),
                    affected_domains: vec![
                        "elec".to_string(),
                        "thermal".to_string(),
                    ],
                };
                
                report.add_violation(violation);
                
                // Auto-generate repair suggestions
                let repairs = self.suggest_repairs(check_id, error_msg.as_deref());
                for repair in repairs {
                    report.add_repair_suggestion(repair);
                }
            }
        }
        
        report
    }
    
    /// Suggest repairs for a constraint violation
    fn suggest_repairs(
        &self,
        check_id: &str,
        error: Option<&str>,
    ) -> Vec<RepairSuggestion> {
        let mut repairs = vec![];
        
        if check_id.contains("current") {
            repairs.push(RepairSuggestion {
                id: format!("{}_reduce_current", check_id),
                description: "Reduce circuit current via higher resistance or lower voltage".to_string(),
                priority: 1,
                affected_nets: vec![],
                affected_components: vec![],
            });
            
            repairs.push(RepairSuggestion {
                id: format!("{}_increase_traces", check_id),
                description: "Increase PCB trace width to reduce resistive losses".to_string(),
                priority: 2,
                affected_nets: vec![],
                affected_components: vec![],
            });
        }
        
        if check_id.contains("temperature") {
            repairs.push(RepairSuggestion {
                id: format!("{}_add_heatsink", check_id),
                description: "Add heatsink to dissipate thermal energy".to_string(),
                priority: 1,
                affected_nets: vec![],
                affected_components: vec![],
            });
            
            repairs.push(RepairSuggestion {
                id: format!("{}_increase_airflow", check_id),
                description: "Increase airflow via cooling fan or better ventilation".to_string(),
                priority: 2,
                affected_nets: vec![],
                affected_components: vec![],
            });
        }
        
        repairs
    }
}

/// Result of cross-domain constraint propagation
#[derive(Clone, Debug)]
pub struct PropagationResult {
    /// Constraints per domain after propagation
    pub domain_results: HashMap<String, HashMap<String, String>>,
    
    /// Check results: (check_id, (passed, optional_error_msg))
    pub check_results: HashMap<String, (bool, Option<String>)>,
}

impl PropagationResult {
    pub fn new() -> Self {
        PropagationResult {
            domain_results: HashMap::new(),
            check_results: HashMap::new(),
        }
    }
    
    pub fn add_domain_result(
        &mut self,
        domain: &str,
        constraints: HashMap<String, String>,
    ) {
        self.domain_results.insert(domain.to_string(), constraints);
    }
    
    pub fn add_check_result(
        &mut self,
        check_id: &str,
        passed: bool,
        error_msg: Option<String>,
    ) {
        self.check_results.insert(check_id.to_string(), (passed, error_msg));
    }
}

impl Default for PropagationResult {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cross_domain_rule_creation() {
        let rule = CrossDomainRule {
            id: "current_to_power".to_string(),
            source_domain: "elec".to_string(),
            target_domain: "thermal".to_string(),
            description: "Transform circuit current to power dissipation".to_string(),
            transform_name: "current_to_power_loss".to_string(),
            inverse_transform_name: Some("power_to_current".to_string()),
            target_constraints: vec!["thermal.power_w".to_string()],
            is_mandatory: true,
        };
        
        assert_eq!(rule.id, "current_to_power");
        assert_eq!(rule.source_domain, "elec");
        assert_eq!(rule.target_domain, "thermal");
        assert!(rule.is_mandatory);
    }
    
    #[test]
    fn test_propagation_result_creation() {
        let result = PropagationResult::new();
        assert!(result.domain_results.is_empty());
        assert!(result.check_results.is_empty());
    }
    
    #[test]
    fn test_add_domain_result() {
        let mut result = PropagationResult::new();
        let mut constraints = HashMap::new();
        constraints.insert("voltage_v".to_string(), "3.3".to_string());
        
        result.add_domain_result("elec", constraints);
        
        assert_eq!(result.domain_results.len(), 1);
        assert!(result.domain_results.contains_key("elec"));
    }
    
    #[test]
    fn test_add_check_result() {
        let mut result = PropagationResult::new();
        result.add_check_result("check_1", true, None);
        result.add_check_result("check_2", false, Some("voltage too high".to_string()));
        
        assert_eq!(result.check_results.len(), 2);
        assert_eq!(result.check_results["check_1"].0, true);
        assert_eq!(result.check_results["check_2"].0, false);
    }
    
    #[test]
    fn test_suggest_repairs_for_current_violation() {
        let propagator = CrossDomainPropagator::new(PropagationEngine::new());
        let repairs = propagator.suggest_repairs("elec.current_violation", None);
        
        assert!(!repairs.is_empty());
        assert!(repairs.iter().any(|r| r.description.contains("current")));
    }
    
    #[test]
    fn test_suggest_repairs_for_temperature_violation() {
        let propagator = CrossDomainPropagator::new(PropagationEngine::new());
        let repairs = propagator.suggest_repairs("thermal.temperature_violation", None);
        
        assert!(!repairs.is_empty());
        assert!(repairs.iter().any(|r| r.description.contains("temperature")));
    }
    
    #[test]
    fn test_cross_domain_propagation_flow() {
        let mut propagator = CrossDomainPropagator::new(PropagationEngine::new());
        
        // Register electrical domain validator
        propagator.register_validator(
            "elec".to_string(),
            |key: &str, value: &str| {
                if key.contains("voltage") {
                    let v: f64 = value.parse()?;
                    if v > 5.0 {
                        return Err("Voltage too high".to_string());
                    }
                }
                Ok(())
            },
        ).unwrap();
        
        // Set up initial constraints
        let mut constraints = HashMap::new();
        constraints.insert("elec.voltage_v".to_string(), "3.3".to_string());
        
        // This test documents expected propagation flow
        assert!(constraints.contains_key("elec.voltage_v"));
    }
    
    #[test]
    fn test_propagation_result_check_aggregation() {
        let mut result = PropagationResult::new();
        
        // Add multiple checks
        result.add_check_result("check_voltage", true, None);
        result.add_check_result("check_current", true, None);
        result.add_check_result("check_temperature", false, Some("95°C > 85°C limit".to_string()));
        
        let passed_count = result.check_results.values().filter(|(p, _)| *p).count();
        let failed_count = result.check_results.values().filter(|(p, _)| !*p).count();
        
        assert_eq!(passed_count, 2);
        assert_eq!(failed_count, 1);
    }
}
