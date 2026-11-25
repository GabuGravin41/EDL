/// Phase 1.2 Week 3: Repair Impact Analysis
///
/// Analyzes the impact of proposed repairs on the overall design,
/// including cascading effects and cost estimation

use std::collections::HashMap;

/// Repair impact analysis engine
pub struct RepairImpactAnalyzer {
    /// Design dependency graph
    dependencies: HashMap<String, Vec<String>>,
    
    /// Component costs (by part number)
    component_costs: HashMap<String, f64>,
    
    /// Net criticality scores (higher = more critical)
    net_criticality: HashMap<String, f64>,
}

impl RepairImpactAnalyzer {
    pub fn new() -> Self {
        RepairImpactAnalyzer {
            dependencies: HashMap::new(),
            component_costs: HashMap::new(),
            net_criticality: HashMap::new(),
        }
    }
    
    /// Register a dependency (net A depends on net B)
    pub fn register_dependency(&mut self, source: String, target: String) {
        self.dependencies
            .entry(source)
            .or_insert_with(Vec::new)
            .push(target);
    }
    
    /// Register component cost
    pub fn register_component_cost(&mut self, part_number: String, cost: f64) {
        self.component_costs.insert(part_number, cost);
    }
    
    /// Set net criticality (0.0 = not critical, 1.0 = essential)
    pub fn set_net_criticality(&mut self, net_name: String, criticality: f64) {
        self.net_criticality.insert(net_name, criticality.clamp(0.0, 1.0));
    }
    
    /// Analyze impact of changing a component
    pub fn analyze_component_change(
        &self,
        component_id: &str,
        affected_nets: &[String],
    ) -> Result<ImpactAnalysisResult, String> {
        let mut result = ImpactAnalysisResult::new();
        
        result.changed_component = component_id.to_string();
        result.directly_affected_nets = affected_nets.to_vec();
        result.direct_impact_count = affected_nets.len();
        
        // Find cascading impacts
        for net in affected_nets {
            if let Some(dependents) = self.dependencies.get(net) {
                for dependent in dependents {
                    result.cascading_nets.push(dependent.clone());
                }
            }
        }
        
        result.cascading_impact_count = result.cascading_nets.len();
        
        // Calculate criticality score
        let total_criticality: f64 = affected_nets
            .iter()
            .map(|n| self.net_criticality.get(n).copied().unwrap_or(0.0))
            .sum();
        
        result.total_criticality_impact = total_criticality;
        result.average_criticality = if !affected_nets.is_empty() {
            total_criticality / affected_nets.len() as f64
        } else {
            0.0
        };
        
        // Estimate risk (higher criticality = higher risk)
        result.risk_score = result.total_criticality_impact.clamp(0.0, 1.0);
        
        Ok(result)
    }
    
    /// Estimate cost of repair
    pub fn estimate_repair_cost(
        &self,
        repair_id: &str,
        old_components: &[String],
        new_components: &[String],
    ) -> Result<CostEstimate, String> {
        let mut estimate = CostEstimate::new(repair_id);
        
        // Remove cost of old components
        let old_cost: f64 = old_components
            .iter()
            .filter_map(|c| self.component_costs.get(c).copied())
            .sum();
        
        // Add cost of new components
        let new_cost: f64 = new_components
            .iter()
            .filter_map(|c| self.component_costs.get(c).copied())
            .sum();
        
        estimate.material_cost_delta = new_cost - old_cost;
        
        // Estimate labor cost (harder repairs = more expensive labor)
        estimate.labor_hours = self.estimate_labor_hours(repair_id);
        estimate.labor_cost_estimate = estimate.labor_hours * 85.0;  // $85/hour typical
        
        // Estimate re-layout effort (percentage of PCB layout cost)
        estimate.relayout_cost_estimate = self.estimate_relayout_cost(repair_id);
        
        // Total cost
        estimate.total_estimated_cost = estimate.material_cost_delta
            + estimate.labor_cost_estimate
            + estimate.relayout_cost_estimate;
        
        Ok(estimate)
    }
    
    /// Estimate labor hours for repair
    fn estimate_labor_hours(&self, repair_id: &str) -> f64 {
        // Rough estimates based on repair complexity
        // This is typically determined by test/analysis effort
        match repair_id {
            id if id.contains("resistor_change") => 0.5,
            id if id.contains("trace_width") => 2.0,
            id if id.contains("add_component") => 3.0,
            id if id.contains("restructure") => 8.0,
            _ => 1.0,
        }
    }
    
    /// Estimate re-layout cost
    fn estimate_relayout_cost(&self, repair_id: &str) -> f64 {
        // Rough estimates for PCB re-layout costs
        match repair_id {
            id if id.contains("trace_width") => 500.0,  // Minor routing changes
            id if id.contains("add_component") => 2000.0,  // Major re-layout
            id if id.contains("restructure") => 5000.0,  // Complete redesign
            _ => 0.0,
        }
    }
}

impl Default for RepairImpactAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of impact analysis for a component change
#[derive(Clone, Debug)]
pub struct ImpactAnalysisResult {
    pub changed_component: String,
    pub directly_affected_nets: Vec<String>,
    pub direct_impact_count: usize,
    pub cascading_nets: Vec<String>,
    pub cascading_impact_count: usize,
    pub total_criticality_impact: f64,
    pub average_criticality: f64,
    pub risk_score: f64,  // 0.0 (safe) to 1.0 (dangerous)
}

impl ImpactAnalysisResult {
    pub fn new() -> Self {
        ImpactAnalysisResult {
            changed_component: String::new(),
            directly_affected_nets: vec![],
            direct_impact_count: 0,
            cascading_nets: vec![],
            cascading_impact_count: 0,
            total_criticality_impact: 0.0,
            average_criticality: 0.0,
            risk_score: 0.0,
        }
    }
    
    pub fn is_safe(&self) -> bool {
        self.risk_score < 0.3
    }
    
    pub fn is_risky(&self) -> bool {
        self.risk_score > 0.7
    }
}

impl Default for ImpactAnalysisResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Cost estimate for a repair
#[derive(Clone, Debug)]
pub struct CostEstimate {
    pub repair_id: String,
    pub material_cost_delta: f64,
    pub labor_hours: f64,
    pub labor_cost_estimate: f64,
    pub relayout_cost_estimate: f64,
    pub total_estimated_cost: f64,
}

impl CostEstimate {
    pub fn new(repair_id: &str) -> Self {
        CostEstimate {
            repair_id: repair_id.to_string(),
            material_cost_delta: 0.0,
            labor_hours: 0.0,
            labor_cost_estimate: 0.0,
            relayout_cost_estimate: 0.0,
            total_estimated_cost: 0.0,
        }
    }
    
    pub fn is_expensive(&self) -> bool {
        self.total_estimated_cost > 1000.0
    }
}

/// Repair ranking engine
pub struct RepairRankingEngine {
    analyzer: RepairImpactAnalyzer,
}

impl RepairRankingEngine {
    pub fn new(analyzer: RepairImpactAnalyzer) -> Self {
        RepairRankingEngine { analyzer }
    }
    
    /// Rank repairs by feasibility and impact
    pub fn rank_repairs(
        &self,
        repairs: Vec<RepairCandidate>,
    ) -> Result<Vec<RankedRepair>, String> {
        let mut ranked: Vec<RankedRepair> = vec![];
        
        for repair in repairs {
            let impact = self.analyzer.analyze_component_change(
                &repair.component_id,
                &repair.affected_nets,
            )?;
            
            let cost = self.analyzer.estimate_repair_cost(
                &repair.id,
                &repair.old_components,
                &repair.new_components,
            )?;
            
            // Calculate ranking score (lower = better)
            // Factors: risk (avoid high), cost (avoid high), effort (avoid high)
            let risk_weight = impact.risk_score * 0.5;  // 50% risk
            let cost_weight = (cost.total_estimated_cost / 5000.0).clamp(0.0, 1.0) * 0.3;  // 30% cost
            let effort_weight = (cost.labor_hours / 8.0).clamp(0.0, 1.0) * 0.2;  // 20% effort
            
            let ranking_score = risk_weight + cost_weight + effort_weight;
            
            ranked.push(RankedRepair {
                repair,
                impact,
                cost,
                ranking_score,
            });
        }
        
        // Sort by ranking score (ascending = best first)
        ranked.sort_by(|a, b| {
            a.ranking_score
                .partial_cmp(&b.ranking_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        
        Ok(ranked)
    }
}

/// Repair candidate
#[derive(Clone, Debug)]
pub struct RepairCandidate {
    pub id: String,
    pub component_id: String,
    pub description: String,
    pub affected_nets: Vec<String>,
    pub old_components: Vec<String>,
    pub new_components: Vec<String>,
}

/// Ranked repair
#[derive(Clone, Debug)]
pub struct RankedRepair {
    pub repair: RepairCandidate,
    pub impact: ImpactAnalysisResult,
    pub cost: CostEstimate,
    pub ranking_score: f64,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_impact_analyzer_creation() {
        let analyzer = RepairImpactAnalyzer::new();
        assert!(analyzer.dependencies.is_empty());
        assert!(analyzer.component_costs.is_empty());
    }
    
    #[test]
    fn test_register_dependency() {
        let mut analyzer = RepairImpactAnalyzer::new();
        analyzer.register_dependency("VBUS".to_string(), "VCC".to_string());
        
        assert!(analyzer.dependencies.contains_key("VBUS"));
    }
    
    #[test]
    fn test_register_component_cost() {
        let mut analyzer = RepairImpactAnalyzer::new();
        analyzer.register_component_cost("TPS54160".to_string(), 2.50);
        
        assert_eq!(
            analyzer.component_costs.get("TPS54160"),
            Some(&2.50)
        );
    }
    
    #[test]
    fn test_set_net_criticality() {
        let mut analyzer = RepairImpactAnalyzer::new();
        analyzer.set_net_criticality("VBUS".to_string(), 0.95);
        
        assert_eq!(analyzer.net_criticality.get("VBUS"), Some(&0.95));
    }
    
    #[test]
    fn test_net_criticality_clamping() {
        let mut analyzer = RepairImpactAnalyzer::new();
        analyzer.set_net_criticality("TEST".to_string(), 1.5);  // > 1.0
        
        assert_eq!(analyzer.net_criticality.get("TEST"), Some(&1.0));
    }
    
    #[test]
    fn test_analyze_component_change() {
        let mut analyzer = RepairImpactAnalyzer::new();
        analyzer.set_net_criticality("VBUS".to_string(), 0.9);
        analyzer.set_net_criticality("VCC".to_string(), 0.5);
        
        let affected_nets = vec!["VBUS".to_string(), "VCC".to_string()];
        let result = analyzer.analyze_component_change("R1", &affected_nets).unwrap();
        
        assert_eq!(result.direct_impact_count, 2);
        assert!((result.total_criticality_impact - 1.4).abs() < 0.01);
    }
    
    #[test]
    fn test_estimate_repair_cost() {
        let mut analyzer = RepairImpactAnalyzer::new();
        analyzer.register_component_cost("R1_old".to_string(), 1.0);
        analyzer.register_component_cost("R1_new".to_string(), 2.0);
        
        let result = analyzer.estimate_repair_cost(
            "resistor_change",
            &["R1_old".to_string()],
            &["R1_new".to_string()],
        ).unwrap();
        
        assert!(result.material_cost_delta > 0.0);
        assert!(result.labor_cost_estimate > 0.0);
    }
    
    #[test]
    fn test_impact_analysis_result() {
        let result = ImpactAnalysisResult {
            changed_component: "R1".to_string(),
            directly_affected_nets: vec!["VBUS".to_string()],
            direct_impact_count: 1,
            cascading_nets: vec![],
            cascading_impact_count: 0,
            total_criticality_impact: 0.2,
            average_criticality: 0.2,
            risk_score: 0.2,
        };
        
        assert!(result.is_safe());
        assert!(!result.is_risky());
    }
    
    #[test]
    fn test_risk_scoring() {
        let safe_result = ImpactAnalysisResult {
            risk_score: 0.2,
            ..Default::default()
        };
        
        let risky_result = ImpactAnalysisResult {
            risk_score: 0.8,
            ..Default::default()
        };
        
        assert!(safe_result.is_safe());
        assert!(risky_result.is_risky());
    }
    
    #[test]
    fn test_cost_estimate() {
        let cost = CostEstimate::new("repair_1");
        cost.total_estimated_cost;  // Access for linting
        assert_eq!(cost.repair_id, "repair_1");
    }
    
    #[test]
    fn test_repair_ranking_engine() {
        let analyzer = RepairImpactAnalyzer::new();
        let engine = RepairRankingEngine::new(analyzer);
        
        let repairs = vec![];
        let ranked = engine.rank_repairs(repairs).unwrap();
        
        assert!(ranked.is_empty());
    }
}
