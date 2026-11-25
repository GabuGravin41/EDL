/// Phase 1.2 Week 1: Elaborator Implementation for Hierarchical Nets
/// 
/// This module extends the elaborator to resolve hierarchical net references
/// across module boundaries

use std::collections::HashMap;

/// Context for elaborating hierarchical nets
#[derive(Clone, Debug)]
pub struct HierarchicalNetContext {
    /// Map of module names to their contained nets
    /// Key: module name (e.g., "PowerModule")
    /// Value: list of net names in that module
    module_nets: HashMap<String, Vec<String>>,
    
    /// Map of hierarchical net names to their resolved entity IDs
    /// Key: "/Module/Net"
    /// Value: "net_entity_12345" (assigned during elaboration)
    hierarchical_net_map: HashMap<String, String>,
    
    /// Current module being elaborated (for scope resolution)
    current_module: Option<String>,
    
    /// Parent module reference (for ../reference resolution)
    parent_module: Option<String>,
}

impl HierarchicalNetContext {
    /// Create new elaboration context
    pub fn new() -> Self {
        HierarchicalNetContext {
            module_nets: HashMap::new(),
            hierarchical_net_map: HashMap::new(),
            current_module: None,
            parent_module: None,
        }
    }
    
    /// Register a module's nets during first pass
    pub fn register_module_nets(
        &mut self,
        module_name: &str,
        net_names: Vec<String>,
    ) -> Result<(), ElaborationError> {
        if self.module_nets.contains_key(module_name) {
            return Err(ElaborationError::DuplicateModule(module_name.to_string()));
        }
        
        self.module_nets.insert(module_name.to_string(), net_names);
        Ok(())
    }
    
    /// Set the current module scope for elaboration
    pub fn enter_module(&mut self, module_name: &str) {
        self.parent_module = self.current_module.clone();
        self.current_module = Some(module_name.to_string());
    }
    
    /// Exit current module scope
    pub fn exit_module(&mut self) {
        self.current_module = self.parent_module.clone();
        self.parent_module = None;
    }
    
    /// Resolve a hierarchical net reference
    /// 
    /// Rules:
    /// 1. /Module/Net → Resolve to net in named module
    /// 2. ../Net → Resolve in parent module (future v3.2)
    /// 3. Net → Resolve in current module or globally
    /// 4. Must exist in module_nets registry
    pub fn resolve_net_reference(
        &self,
        reference: &str,
    ) -> Result<String, ElaborationError> {
        if reference.starts_with('/') {
            // Hierarchical reference: /Module/Net
            self.resolve_hierarchical_reference(reference)
        } else if reference.starts_with("..") {
            // Parent reference (future): ../Net
            Err(ElaborationError::ParentReferencesNotYetSupported(
                reference.to_string()
            ))
        } else {
            // Local or global reference
            self.resolve_local_reference(reference)
        }
    }
    
    /// Resolve /Module/Net reference
    fn resolve_hierarchical_reference(
        &self,
        reference: &str,
    ) -> Result<String, ElaborationError> {
        // Check if already cached
        if let Some(entity_id) = self.hierarchical_net_map.get(reference) {
            return Ok(entity_id.clone());
        }
        
        // Parse /Module/Net format
        let parts: Vec<&str> = reference
            .trim_start_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        
        if parts.len() != 2 {
            return Err(ElaborationError::InvalidNetReference(format!(
                "Hierarchical reference must be /Module/Net, got: {}",
                reference
            )));
        }
        
        let module_name = parts[0];
        let net_name = parts[1];
        
        // Verify module exists
        let nets = self.module_nets.get(module_name)
            .ok_or_else(|| ElaborationError::UnknownModule(module_name.to_string()))?;
        
        // Verify net exists in module
        if !nets.contains(&net_name.to_string()) {
            return Err(ElaborationError::UnknownNetInModule {
                module: module_name.to_string(),
                net: net_name.to_string(),
            });
        }
        
        // Generate entity ID: net_<module>_<net>_<uuid>
        let entity_id = format!(
            "net_{}_{}_{}",
            module_name.to_lowercase(),
            net_name.to_lowercase(),
            uuid::Uuid::new_v4().simple()
        );
        
        Ok(entity_id)
    }
    
    /// Resolve local or global net reference
    fn resolve_local_reference(
        &self,
        net_name: &str,
    ) -> Result<String, ElaborationError> {
        // First, try to find in current module
        if let Some(current) = &self.current_module {
            if let Some(nets) = self.module_nets.get(current) {
                if nets.contains(&net_name.to_string()) {
                    // Found in current module
                    return Ok(format!(
                        "net_{}_{}_{}", 
                        current.to_lowercase(),
                        net_name.to_lowercase(),
                        uuid::Uuid::new_v4().simple()
                    ));
                }
            }
        }
        
        // Try global scope (all modules or top-level nets)
        // For now, generate a flat net ID
        Ok(format!(
            "net_{}_{}",
            net_name.to_lowercase(),
            uuid::Uuid::new_v4().simple()
        ))
    }
}

#[derive(Clone, Debug)]
pub enum ElaborationError {
    DuplicateModule(String),
    UnknownModule(String),
    UnknownNetInModule { module: String, net: String },
    InvalidNetReference(String),
    ParentReferencesNotYetSupported(String),
}

// ============================================================================
// MODULE ELABORATOR
// ============================================================================

/// Elaborator for modules with hierarchical net support
pub struct ModuleElaborator {
    context: HierarchicalNetContext,
}

impl ModuleElaborator {
    pub fn new() -> Self {
        ModuleElaborator {
            context: HierarchicalNetContext::new(),
        }
    }
    
    /// First pass: register all modules and their nets
    pub fn register_module(
        &mut self,
        module_name: &str,
        net_declarations: Vec<&str>,
    ) -> Result<(), ElaborationError> {
        // Extract net names from declarations
        let net_names: Vec<String> = net_declarations
            .iter()
            .map(|decl| {
                // Remove "/Module/" prefix if hierarchical
                if decl.starts_with('/') {
                    decl.trim_start_matches('/').split('/').nth(1).unwrap_or(decl).to_string()
                } else {
                    decl.to_string()
                }
            })
            .collect();
        
        self.context.register_module_nets(module_name, net_names)?;
        Ok(())
    }
    
    /// Elaborate a module, resolving all hierarchical references
    pub fn elaborate_module(
        &mut self,
        module_name: &str,
        connections: Vec<&str>,
    ) -> Result<ElaboratedModule, ElaborationError> {
        self.context.enter_module(module_name);
        
        let mut elaborated_connections = Vec::new();
        
        for connection in connections {
            let resolved = self.context.resolve_net_reference(connection)?;
            elaborated_connections.push(ResolvedConnection {
                original: connection.to_string(),
                resolved_id: resolved,
            });
        }
        
        self.context.exit_module();
        
        Ok(ElaboratedModule {
            module_name: module_name.to_string(),
            connections: elaborated_connections,
        })
    }
}

impl Default for ModuleElaborator {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of elaborating a module
#[derive(Clone, Debug)]
pub struct ElaboratedModule {
    pub module_name: String,
    pub connections: Vec<ResolvedConnection>,
}

/// A connection reference resolved to its entity ID
#[derive(Clone, Debug)]
pub struct ResolvedConnection {
    pub original: String,
    pub resolved_id: String,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_module_registration() {
        let mut context = HierarchicalNetContext::new();
        
        context.register_module_nets(
            "PowerModule",
            vec!["VOUT".to_string(), "GND".to_string()],
        ).unwrap();
        
        assert!(context.module_nets.contains_key("PowerModule"));
    }
    
    #[test]
    fn test_hierarchical_net_resolution() {
        let mut context = HierarchicalNetContext::new();
        
        context.register_module_nets(
            "PowerModule",
            vec!["VOUT".to_string()],
        ).unwrap();
        
        let resolved = context.resolve_net_reference("/PowerModule/VOUT").unwrap();
        assert!(resolved.starts_with("net_powermodule_vout_"));
    }
    
    #[test]
    fn test_flat_net_resolution() {
        let context = HierarchicalNetContext::new();
        
        let resolved = context.resolve_net_reference("GND").unwrap();
        assert!(resolved.starts_with("net_gnd_"));
    }
    
    #[test]
    fn test_unknown_module_error() {
        let context = HierarchicalNetContext::new();
        
        let result = context.resolve_net_reference("/UnknownModule/Net");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_unknown_net_in_module_error() {
        let mut context = HierarchicalNetContext::new();
        
        context.register_module_nets(
            "PowerModule",
            vec!["VOUT".to_string()],
        ).unwrap();
        
        let result = context.resolve_net_reference("/PowerModule/UnknownNet");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_module_elaborator() {
        let mut elaborator = ModuleElaborator::new();
        
        elaborator.register_module(
            "PowerModule",
            vec!["/PowerModule/VOUT", "GND"],
        ).unwrap();
        
        elaborator.register_module(
            "MainBoard",
            vec!["VCC"],
        ).unwrap();
        
        // Elaborate MainBoard with connection to PowerModule
        let result = elaborator.elaborate_module(
            "MainBoard",
            vec!["VCC", "/PowerModule/VOUT"],
        ).unwrap();
        
        assert_eq!(result.module_name, "MainBoard");
        assert_eq!(result.connections.len(), 2);
    }
    
    #[test]
    fn test_cross_module_reference() {
        let mut elaborator = ModuleElaborator::new();
        
        elaborator.register_module(
            "PrimaryStage",
            vec!["/PrimaryStage/VOUT"],
        ).unwrap();
        
        elaborator.register_module(
            "SecondaryStage",
            vec!["/SecondaryStage/VIN"],
        ).unwrap();
        
        // Secondary stage connects to primary stage output
        let result = elaborator.elaborate_module(
            "SecondaryStage",
            vec!["/PrimaryStage/VOUT"],
        ).unwrap();
        
        // Verify cross-module connection resolved
        assert_eq!(result.connections[0].original, "/PrimaryStage/VOUT");
        assert!(result.connections[0].resolved_id.contains("primarystage_vout"));
    }
    
    #[test]
    fn test_net_isolation_between_modules() {
        let mut context = HierarchicalNetContext::new();
        
        context.register_module_nets(
            "PowerModule",
            vec!["GND".to_string()],
        ).unwrap();
        
        context.register_module_nets(
            "AudioModule",
            vec!["GND".to_string()],
        ).unwrap();
        
        let gnd_power = context.resolve_net_reference("/PowerModule/GND").unwrap();
        let gnd_audio = context.resolve_net_reference("/AudioModule/GND").unwrap();
        
        // Different modules = different resolved IDs
        assert_ne!(gnd_power, gnd_audio);
    }
}
