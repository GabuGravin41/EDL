/// Phase 1.2 Week 3: MCP Tools for Constraint Analysis & Repair
///
/// Implements Model Context Protocol tools for IDE integration,
/// enabling Claude and other LLMs to analyze constraints and suggest repairs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MCP Tool: edl.analyze_constraints
/// Analyzes a design and returns all constraint violations with repairs
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalyzeConstraintsRequest {
    /// Design JSON or EDL source code
    pub design_input: String,
    
    /// Format: "json" or "edl"
    pub input_format: String,
    
    /// Domains to analyze (empty = all)
    pub domains: Vec<String>,
    
    /// Include repair suggestions?
    pub include_repairs: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalyzeConstraintsResponse {
    /// Overall status
    pub status: String,  // "ok" | "failed" | "partial"
    
    /// Violations found
    pub violations: Vec<ConstraintViolationReport>,
    
    /// Domain verification summary
    pub domain_summary: HashMap<String, DomainSummary>,
    
    /// Repair suggestions (if requested)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub repairs: Vec<RepairRecommendation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstraintViolationReport {
    pub id: String,
    pub domain: String,
    pub constraint_name: String,
    pub expected: String,
    pub actual: String,
    pub severity: String,  // "error" | "warning" | "info"
    pub affected_nets: Vec<String>,
    pub affected_components: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DomainSummary {
    pub domain: String,
    pub total_checks: usize,
    pub passed: usize,
    pub failed: usize,
    pub warnings: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepairRecommendation {
    pub id: String,
    pub violation_id: String,
    pub description: String,
    pub difficulty: String,  // "trivial" | "easy" | "medium" | "hard"
    pub impact_score: f64,   // 0.0 (low) to 1.0 (high)
    pub is_auto_fixable: bool,
    pub estimated_effort_minutes: u32,
    pub related_repairs: Vec<String>,
}

impl AnalyzeConstraintsResponse {
    pub fn new() -> Self {
        AnalyzeConstraintsResponse {
            status: "ok".to_string(),
            violations: vec![],
            domain_summary: HashMap::new(),
            repairs: vec![],
        }
    }
    
    pub fn add_violation(&mut self, violation: ConstraintViolationReport) {
        self.violations.push(violation);
    }
    
    pub fn add_domain_summary(&mut self, summary: DomainSummary) {
        self.domain_summary.insert(summary.domain.clone(), summary);
    }
    
    pub fn add_repair(&mut self, repair: RepairRecommendation) {
        self.repairs.push(repair);
    }
}

impl Default for AnalyzeConstraintsResponse {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP Tool: edl.suggest_repair
/// Get detailed repair steps for a specific violation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuggestRepairRequest {
    /// Violation ID to repair
    pub violation_id: String,
    
    /// Current design state (for impact analysis)
    pub design_input: String,
    
    /// Format: "json" or "edl"
    pub input_format: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuggestRepairResponse {
    pub violation_id: String,
    pub constraint_name: String,
    
    /// Repair options ranked by impact
    pub repair_options: Vec<RepairOption>,
    
    /// Root cause analysis
    pub root_causes: Vec<String>,
    
    /// Domain-specific impacts
    pub impacts: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepairOption {
    pub id: String,
    pub title: String,
    pub description: String,
    pub steps: Vec<RepairStep>,
    pub difficulty: String,
    pub estimated_time_minutes: u32,
    pub risk_level: String,  // "low" | "medium" | "high"
    pub validation_steps: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepairStep {
    pub sequence: usize,
    pub action: String,
    pub details: String,
    pub affected_items: Vec<String>,
}

/// MCP Tool: edl.verify_cross_domain
/// Verify cross-domain constraints between specified domains
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifyCrossDomainRequest {
    pub design_input: String,
    pub input_format: String,
    pub source_domain: String,
    pub target_domain: String,
    pub include_impact_analysis: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifyCrossDomainResponse {
    pub source_domain: String,
    pub target_domain: String,
    pub is_compatible: bool,
    pub violations: Vec<CrossDomainViolation>,
    pub transformation_chain: Vec<TransformationStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact_analysis: Option<ImpactAnalysis>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrossDomainViolation {
    pub id: String,
    pub description: String,
    pub source_constraint: String,
    pub target_constraint: String,
    pub violation_detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransformationStep {
    pub sequence: usize,
    pub from_domain: String,
    pub to_domain: String,
    pub transformation: String,
    pub formula: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub affected_modules: usize,
    pub affected_nets: usize,
    pub cascading_violations: usize,
    pub total_impact_score: f64,
}

/// MCP Tool Processor
pub struct McpToolProcessor {
    /// Registered tool handlers
    tools: HashMap<String, Box<dyn Fn(&str) -> Result<String, String>>>,
}

impl McpToolProcessor {
    pub fn new() -> Self {
        McpToolProcessor {
            tools: HashMap::new(),
        }
    }
    
    /// Register a tool handler
    pub fn register_tool<F>(
        &mut self,
        name: String,
        handler: F,
    ) -> Result<(), String>
    where
        F: Fn(&str) -> Result<String, String> + 'static,
    {
        if self.tools.contains_key(&name) {
            return Err(format!("Tool {} already registered", name));
        }
        self.tools.insert(name, Box::new(handler));
        Ok(())
    }
    
    /// Execute a tool
    pub fn execute_tool(&self, tool_name: &str, input: &str) -> Result<String, String> {
        let handler = self.tools
            .get(tool_name)
            .ok_or_else(|| format!("Tool {} not found", tool_name))?;
        
        handler(input)
    }
    
    /// List available tools
    pub fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "edl.analyze_constraints".to_string(),
                description: "Analyze design constraints and return violations with repair suggestions".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "design_input": {"type": "string"},
                        "input_format": {"type": "string", "enum": ["json", "edl"]},
                        "domains": {"type": "array", "items": {"type": "string"}},
                        "include_repairs": {"type": "boolean"}
                    }
                }),
            },
            ToolDefinition {
                name: "edl.suggest_repair".to_string(),
                description: "Get detailed repair steps for a specific constraint violation".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "violation_id": {"type": "string"},
                        "design_input": {"type": "string"},
                        "input_format": {"type": "string", "enum": ["json", "edl"]}
                    }
                }),
            },
            ToolDefinition {
                name: "edl.verify_cross_domain".to_string(),
                description: "Verify cross-domain constraints between specified domains".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "design_input": {"type": "string"},
                        "input_format": {"type": "string", "enum": ["json", "edl"]},
                        "source_domain": {"type": "string"},
                        "target_domain": {"type": "string"},
                        "include_impact_analysis": {"type": "boolean"}
                    }
                }),
            },
        ]
    }
}

impl Default for McpToolProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_analyze_constraints_request() {
        let req = AnalyzeConstraintsRequest {
            design_input: "{}".to_string(),
            input_format: "json".to_string(),
            domains: vec!["elec".to_string(), "thermal".to_string()],
            include_repairs: true,
        };
        
        assert_eq!(req.domains.len(), 2);
        assert!(req.include_repairs);
    }
    
    #[test]
    fn test_analyze_constraints_response_creation() {
        let mut resp = AnalyzeConstraintsResponse::new();
        assert_eq!(resp.status, "ok");
        assert!(resp.violations.is_empty());
        assert!(resp.repairs.is_empty());
    }
    
    #[test]
    fn test_add_violation_to_response() {
        let mut resp = AnalyzeConstraintsResponse::new();
        
        let violation = ConstraintViolationReport {
            id: "v1".to_string(),
            domain: "elec".to_string(),
            constraint_name: "voltage_drop".to_string(),
            expected: "< 2.0V".to_string(),
            actual: "2.5V".to_string(),
            severity: "error".to_string(),
            affected_nets: vec!["VBUS".to_string()],
            affected_components: vec!["R1".to_string()],
        };
        
        resp.add_violation(violation);
        assert_eq!(resp.violations.len(), 1);
    }
    
    #[test]
    fn test_domain_summary_creation() {
        let summary = DomainSummary {
            domain: "elec".to_string(),
            total_checks: 10,
            passed: 8,
            failed: 2,
            warnings: 0,
        };
        
        assert_eq!(summary.total_checks, 10);
        assert_eq!(summary.passed + summary.failed + summary.warnings, 10);
    }
    
    #[test]
    fn test_repair_recommendation() {
        let repair = RepairRecommendation {
            id: "r1".to_string(),
            violation_id: "v1".to_string(),
            description: "Increase resistor value".to_string(),
            difficulty: "easy".to_string(),
            impact_score: 0.3,
            is_auto_fixable: true,
            estimated_effort_minutes: 5,
            related_repairs: vec![],
        };
        
        assert!(repair.is_auto_fixable);
        assert!(repair.impact_score >= 0.0 && repair.impact_score <= 1.0);
    }
    
    #[test]
    fn test_suggest_repair_request() {
        let req = SuggestRepairRequest {
            violation_id: "v1".to_string(),
            design_input: "{}".to_string(),
            input_format: "json".to_string(),
        };
        
        assert_eq!(req.violation_id, "v1");
    }
    
    #[test]
    fn test_repair_option_creation() {
        let option = RepairOption {
            id: "opt1".to_string(),
            title: "Increase trace width".to_string(),
            description: "Wider PCB traces reduce resistance".to_string(),
            steps: vec![],
            difficulty: "medium".to_string(),
            estimated_time_minutes: 30,
            risk_level: "low".to_string(),
            validation_steps: vec![],
        };
        
        assert_eq!(option.difficulty, "medium");
        assert_eq!(option.risk_level, "low");
    }
    
    #[test]
    fn test_mcp_tool_processor_creation() {
        let processor = McpToolProcessor::new();
        assert!(processor.tools.is_empty());
    }
    
    #[test]
    fn test_list_tools() {
        let processor = McpToolProcessor::new();
        let tools = processor.list_tools();
        
        assert_eq!(tools.len(), 3);
        assert!(tools.iter().any(|t| t.name.contains("analyze_constraints")));
        assert!(tools.iter().any(|t| t.name.contains("suggest_repair")));
        assert!(tools.iter().any(|t| t.name.contains("verify_cross_domain")));
    }
    
    #[test]
    fn test_cross_domain_violation() {
        let violation = CrossDomainViolation {
            id: "xd1".to_string(),
            description: "Temperature exceeds thermal limit".to_string(),
            source_constraint: "elec.power_w".to_string(),
            target_constraint: "thermal.temp_c".to_string(),
            violation_detail: "95°C > 85°C limit".to_string(),
        };
        
        assert_eq!(violation.source_constraint, "elec.power_w");
        assert_eq!(violation.target_constraint, "thermal.temp_c");
    }
    
    #[test]
    fn test_transformation_step() {
        let step = TransformationStep {
            sequence: 1,
            from_domain: "elec".to_string(),
            to_domain: "thermal".to_string(),
            transformation: "power_loss".to_string(),
            formula: "P = I² × R".to_string(),
        };
        
        assert_eq!(step.sequence, 1);
        assert_eq!(step.from_domain, "elec");
        assert_eq!(step.to_domain, "thermal");
    }
}
