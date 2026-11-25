/// Phase 1.2 Week 4: Performance Optimization & Error Enhancement
///
/// Optimizes constraint checking performance and provides detailed, actionable error messages

use std::time::{Duration, Instant};
use std::collections::HashMap;

/// Performance metrics for constraint checking
#[derive(Clone, Debug)]
pub struct PerformanceMetrics {
    /// Total time for constraint checking (milliseconds)
    pub total_time_ms: f64,
    
    /// Time spent in parser (milliseconds)
    pub parser_time_ms: f64,
    
    /// Time spent in elaborator (milliseconds)
    pub elaborator_time_ms: f64,
    
    /// Time spent in constraint propagation (milliseconds)
    pub propagation_time_ms: f64,
    
    /// Time spent in verification (milliseconds)
    pub verification_time_ms: f64,
    
    /// Time spent in report generation (milliseconds)
    pub reporting_time_ms: f64,
    
    /// Number of nets processed
    pub nets_processed: usize,
    
    /// Number of constraints checked
    pub constraints_checked: usize,
    
    /// Number of violations found
    pub violations_found: usize,
    
    /// Performance rating (0.0-1.0, where 1.0 is optimal)
    pub performance_rating: f64,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        PerformanceMetrics {
            total_time_ms: 0.0,
            parser_time_ms: 0.0,
            elaborator_time_ms: 0.0,
            propagation_time_ms: 0.0,
            verification_time_ms: 0.0,
            reporting_time_ms: 0.0,
            nets_processed: 0,
            constraints_checked: 0,
            violations_found: 0,
            performance_rating: 1.0,
        }
    }
    
    pub fn is_acceptable(&self) -> bool {
        // Target: <100ms for typical designs
        self.total_time_ms < 100.0 && self.performance_rating > 0.8
    }
    
    pub fn calculate_rating(&mut self) {
        // Rating based on total time
        // 0-50ms: 1.0 (excellent)
        // 50-100ms: 0.9 (good)
        // 100-200ms: 0.7 (acceptable)
        // 200+ms: 0.5 (needs optimization)
        
        self.performance_rating = match self.total_time_ms {
            t if t <= 50.0 => 1.0,
            t if t <= 100.0 => 0.9,
            t if t <= 200.0 => 0.7,
            _ => 0.5,
        };
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance profiler for constraint checking
pub struct PerformanceProfiler {
    checkpoints: HashMap<String, Instant>,
    metrics: PerformanceMetrics,
}

impl PerformanceProfiler {
    pub fn new() -> Self {
        PerformanceProfiler {
            checkpoints: HashMap::new(),
            metrics: PerformanceMetrics::new(),
        }
    }
    
    /// Mark start of a phase
    pub fn start_phase(&mut self, phase_name: &str) {
        self.checkpoints.insert(phase_name.to_string(), Instant::now());
    }
    
    /// Mark end of a phase and record time
    pub fn end_phase(&mut self, phase_name: &str) -> f64 {
        let start = self.checkpoints
            .remove(phase_name)
            .unwrap_or_else(Instant::now);
        
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        
        match phase_name {
            "parser" => self.metrics.parser_time_ms = elapsed_ms,
            "elaborator" => self.metrics.elaborator_time_ms = elapsed_ms,
            "propagation" => self.metrics.propagation_time_ms = elapsed_ms,
            "verification" => self.metrics.verification_time_ms = elapsed_ms,
            "reporting" => self.metrics.reporting_time_ms = elapsed_ms,
            _ => {}
        }
        
        elapsed_ms
    }
    
    /// Get current metrics
    pub fn metrics(&mut self) -> PerformanceMetrics {
        self.metrics.total_time_ms = self.metrics.parser_time_ms
            + self.metrics.elaborator_time_ms
            + self.metrics.propagation_time_ms
            + self.metrics.verification_time_ms
            + self.metrics.reporting_time_ms;
        
        self.metrics.calculate_rating();
        self.metrics.clone()
    }
}

impl Default for PerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced error message with actionable guidance
#[derive(Clone, Debug)]
pub struct EnhancedErrorMessage {
    pub error_id: String,
    pub severity: ErrorSeverity,
    pub title: String,
    pub description: String,
    
    /// What went wrong
    pub what_happened: String,
    
    /// Why it happened
    pub why_it_happened: String,
    
    /// How to fix it
    pub how_to_fix: Vec<FixSuggestion>,
    
    /// Related documentation/examples
    pub see_also: Vec<String>,
    
    /// Context (line numbers, component names, etc.)
    pub context: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ErrorSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug)]
pub struct FixSuggestion {
    pub step: usize,
    pub action: String,
    pub details: String,
    pub example: Option<String>,
}

impl EnhancedErrorMessage {
    pub fn new(error_id: &str, title: &str) -> Self {
        EnhancedErrorMessage {
            error_id: error_id.to_string(),
            severity: ErrorSeverity::Error,
            title: title.to_string(),
            description: String::new(),
            what_happened: String::new(),
            why_it_happened: String::new(),
            how_to_fix: vec![],
            see_also: vec![],
            context: HashMap::new(),
        }
    }
    
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }
    
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }
    
    pub fn with_what_happened(mut self, what: &str) -> Self {
        self.what_happened = what.to_string();
        self
    }
    
    pub fn with_why_it_happened(mut self, why: &str) -> Self {
        self.why_it_happened = why.to_string();
        self
    }
    
    pub fn add_fix_suggestion(mut self, suggestion: FixSuggestion) -> Self {
        self.how_to_fix.push(suggestion);
        self
    }
    
    pub fn add_context(mut self, key: String, value: String) -> Self {
        self.context.insert(key, value);
        self
    }
    
    pub fn format_for_terminal(&self) -> String {
        let mut output = String::new();
        
        // Header
        output.push_str(&format!("❌ {}\n", self.title));
        output.push_str(&format!("   Error ID: {}\n\n", self.error_id));
        
        // Description
        if !self.description.is_empty() {
            output.push_str(&format!("Description:\n  {}\n\n", self.description));
        }
        
        // What happened
        if !self.what_happened.is_empty() {
            output.push_str(&format!("What happened:\n  {}\n\n", self.what_happened));
        }
        
        // Why it happened
        if !self.why_it_happened.is_empty() {
            output.push_str(&format!("Why:\n  {}\n\n", self.why_it_happened));
        }
        
        // How to fix
        if !self.how_to_fix.is_empty() {
            output.push_str("How to fix:\n");
            for fix in &self.how_to_fix {
                output.push_str(&format!("  {}. {}\n", fix.step, fix.action));
                if !fix.details.is_empty() {
                    output.push_str(&format!("     → {}\n", fix.details));
                }
                if let Some(example) = &fix.example {
                    output.push_str(&format!("     Example: {}\n", example));
                }
            }
            output.push('\n');
        }
        
        // Context
        if !self.context.is_empty() {
            output.push_str("Context:\n");
            for (key, value) in &self.context {
                output.push_str(&format!("  {}: {}\n", key, value));
            }
        }
        
        output
    }
}

/// Error catalog for EDL compiler
pub struct ErrorCatalog {
    errors: HashMap<String, Box<dyn Fn() -> EnhancedErrorMessage>>,
}

impl ErrorCatalog {
    pub fn new() -> Self {
        ErrorCatalog {
            errors: HashMap::new(),
        }
    }
    
    /// Register error type
    pub fn register_error<F>(
        &mut self,
        error_id: &str,
        generator: F,
    ) -> Result<(), String>
    where
        F: Fn() -> EnhancedErrorMessage + 'static,
    {
        if self.errors.contains_key(error_id) {
            return Err(format!("Error {} already registered", error_id));
        }
        self.errors.insert(error_id.to_string(), Box::new(generator));
        Ok(())
    }
    
    /// Get error by ID
    pub fn get_error(&self, error_id: &str) -> Option<EnhancedErrorMessage> {
        self.errors.get(error_id).map(|gen| gen())
    }
    
    /// Generate catalog of common errors
    pub fn create_default_catalog() -> Self {
        let mut catalog = ErrorCatalog::new();
        
        // Register common EDL errors
        catalog.register_error("E001", || {
            EnhancedErrorMessage::new("E001", "Invalid hierarchical net syntax")
                .with_severity(ErrorSeverity::Error)
                .with_description("The hierarchical net syntax /Module/Net is incorrect")
                .with_what_happened("Parser encountered invalid characters in net name")
                .with_why_it_happened("Hierarchical nets must follow the pattern /Module/Net where Module and Net are valid identifiers")
                .add_fix_suggestion(FixSuggestion {
                    step: 1,
                    action: "Check net name format".to_string(),
                    details: "Ensure format is /ModuleName/NetName".to_string(),
                    example: Some("✓ /PowerModule/VOUT, ✗ /Power-Module/VOUT".to_string()),
                })
        }).ok();
        
        // E002: Undefined module
        catalog.register_error("E002", || {
            EnhancedErrorMessage::new("E002", "Module not found")
                .with_severity(ErrorSeverity::Error)
                .with_description("Hierarchical net references undefined module")
                .with_what_happened("Elaborator cannot resolve module referenced in /Module/Net")
                .with_why_it_happened("Module must be declared before using its nets")
                .add_fix_suggestion(FixSuggestion {
                    step: 1,
                    action: "Define the module".to_string(),
                    details: "Add 'module ModuleName;' before using /ModuleName/Net".to_string(),
                    example: Some("module PowerModule; net /PowerModule/VOUT;".to_string()),
                })
        }).ok();
        
        // E003: Voltage constraint violation
        catalog.register_error("E003", || {
            EnhancedErrorMessage::new("E003", "Voltage constraint violated")
                .with_severity(ErrorSeverity::Error)
                .with_description("Component voltage exceeds specification")
                .with_what_happened("Actual voltage exceeds maximum rated voltage")
                .with_why_it_happened("Insufficient voltage regulation or incorrect component selection")
                .add_fix_suggestion(FixSuggestion {
                    step: 1,
                    action: "Check component voltage rating".to_string(),
                    details: "Verify component datasheet for Vmax specification".to_string(),
                    example: None,
                })
                .add_fix_suggestion(FixSuggestion {
                    step: 2,
                    action: "Adjust voltage regulator output".to_string(),
                    details: "Lower the output voltage via feedback resistor adjustment".to_string(),
                    example: None,
                })
        }).ok();
        
        catalog
    }
}

impl Default for ErrorCatalog {
    fn default() -> Self {
        Self::create_default_catalog()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_performance_metrics_creation() {
        let metrics = PerformanceMetrics::new();
        assert_eq!(metrics.total_time_ms, 0.0);
        assert_eq!(metrics.performance_rating, 1.0);
    }
    
    #[test]
    fn test_performance_rating_calculation() {
        let mut metrics = PerformanceMetrics::new();
        
        // Excellent (< 50ms)
        metrics.total_time_ms = 30.0;
        metrics.calculate_rating();
        assert_eq!(metrics.performance_rating, 1.0);
        
        // Good (50-100ms)
        metrics.total_time_ms = 75.0;
        metrics.calculate_rating();
        assert_eq!(metrics.performance_rating, 0.9);
        
        // Acceptable (100-200ms)
        metrics.total_time_ms = 150.0;
        metrics.calculate_rating();
        assert_eq!(metrics.performance_rating, 0.7);
        
        // Needs optimization (200+ms)
        metrics.total_time_ms = 300.0;
        metrics.calculate_rating();
        assert_eq!(metrics.performance_rating, 0.5);
    }
    
    #[test]
    fn test_metrics_acceptable() {
        let mut metrics = PerformanceMetrics::new();
        metrics.total_time_ms = 50.0;
        metrics.performance_rating = 0.9;
        
        assert!(metrics.is_acceptable());
    }
    
    #[test]
    fn test_performance_profiler() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start_phase("parser");
        // Simulate work
        std::thread::sleep(std::time::Duration::from_millis(5));
        profiler.end_phase("parser");
        
        let metrics = profiler.metrics();
        assert!(metrics.parser_time_ms > 0.0);
    }
    
    #[test]
    fn test_enhanced_error_message_creation() {
        let err = EnhancedErrorMessage::new("E001", "Test Error")
            .with_severity(ErrorSeverity::Error)
            .with_description("Test description");
        
        assert_eq!(err.error_id, "E001");
        assert_eq!(err.title, "Test Error");
        assert_eq!(err.severity, ErrorSeverity::Error);
    }
    
    #[test]
    fn test_fix_suggestion() {
        let suggestion = FixSuggestion {
            step: 1,
            action: "Do something".to_string(),
            details: "Details here".to_string(),
            example: Some("Example code".to_string()),
        };
        
        assert_eq!(suggestion.step, 1);
        assert!(suggestion.example.is_some());
    }
    
    #[test]
    fn test_error_message_formatting() {
        let err = EnhancedErrorMessage::new("E001", "Test Error")
            .with_description("Something went wrong")
            .with_what_happened("The parser failed");
        
        let formatted = err.format_for_terminal();
        assert!(formatted.contains("E001"));
        assert!(formatted.contains("Test Error"));
    }
    
    #[test]
    fn test_error_catalog_creation() {
        let catalog = ErrorCatalog::create_default_catalog();
        
        // Verify some errors are registered
        assert!(catalog.get_error("E001").is_some());
        assert!(catalog.get_error("E002").is_some());
        assert!(catalog.get_error("E003").is_some());
    }
    
    #[test]
    fn test_error_catalog_retrieval() {
        let catalog = ErrorCatalog::create_default_catalog();
        let err = catalog.get_error("E001").unwrap();
        
        assert_eq!(err.error_id, "E001");
        assert_eq!(err.severity, ErrorSeverity::Error);
    }
}
