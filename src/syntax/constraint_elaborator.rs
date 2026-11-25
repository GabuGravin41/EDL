/// Phase 1.2 Week 2: Constraint AST Elaboration
/// Resolves constraint targets to IR entities and injects constraints

use crate::core::span::{Diagnostic, Span};
use crate::syntax::ast::ProjectAst;
use crate::syntax::constraints_ast::*;
use std::collections::HashMap;

/// Elaboration result for a single constraint
#[derive(Debug, Clone)]
pub struct ElaboratedConstraint {
    pub id: String,
    pub severity: ConstraintSeverity,
    pub entity_ref: ResolvedEntityRef,
    pub rules: Vec<ConstraintRule>,
    pub standard: Option<String>,
    pub description: Option<String>,
}

/// Resolved entity reference (after elaboration)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResolvedEntityRef {
    /// Resolved net: "VCC" or "Module.VCC"
    Net(String),
    /// Resolved component: "IC1" or "Module.IC1"
    Component(String),
    /// Resolved pipe: "P1" or "Branch.P1"
    Pipe(String),
    /// Resolved fixture: "F1" or "Room.F1"
    Fixture(String),
    /// Resolved group: "GroupName" (for hierarchical constraints)
    Group(String),
}

/// Elaborates constraints from AST
/// - Resolves entity references
/// - Validates constraint targets exist
/// - Prepares for IR injection
pub struct ConstraintElaborator;

impl ConstraintElaborator {
    /// Elaborate all constraints in a project
    pub fn elaborate_constraints(
        project: &ProjectAst,
    ) -> Result<Vec<ElaboratedConstraint>, Diagnostic> {
        let mut elaborated = Vec::new();
        
        for constraint_decl in &project.constraints {
            let elaborated_constraint = Self::elaborate_one(project, constraint_decl)?;
            elaborated.push(elaborated_constraint);
        }
        
        Ok(elaborated)
    }
    
    /// Elaborate a single constraint declaration
    fn elaborate_one(
        project: &ProjectAst,
        constraint: &ConstraintDecl,
    ) -> Result<ElaboratedConstraint, Diagnostic> {
        // Resolve the target entity reference
        let entity_ref = Self::resolve_target(project, &constraint.target)?;
        
        // Validate that all rules have known parameters (Phase 1.3+)
        // For now, we accept any parameter name
        
        Ok(ElaboratedConstraint {
            id: constraint.id.clone(),
            severity: constraint.severity.clone(),
            entity_ref,
            rules: constraint.rules.clone(),
            standard: constraint.standard.clone(),
            description: constraint.description.clone(),
        })
    }
    
    /// Resolve a constraint target to an entity reference
    fn resolve_target(
        project: &ProjectAst,
        target: &ConstraintTarget,
    ) -> Result<ResolvedEntityRef, Diagnostic> {
        match target {
            ConstraintTarget::Net(name) => {
                // Verify net exists (optional for Phase 1.2, mandatory for Phase 1.3)
                let _found = project.elec.nets.iter().find(|n| n.name == *name);
                // For now, accept any net name (resolution deferred to compilation)
                Ok(ResolvedEntityRef::Net(name.clone()))
            }
            ConstraintTarget::Component(name) => {
                // Verify component exists (optional for Phase 1.2)
                let _found = project.elec.components.iter().find(|c| c.name == *name);
                Ok(ResolvedEntityRef::Component(name.clone()))
            }
            ConstraintTarget::Pipe(name) => {
                // Verify pipe exists (from MEP domain)
                // For now, accept any pipe name
                Ok(ResolvedEntityRef::Pipe(name.clone()))
            }
            ConstraintTarget::Fixture(name) => {
                // Verify fixture exists (from architecture domain)
                let _found = project.arch.fixtures.iter().find(|f| {
                    if let crate::domains::arch::ast::FixtureDef::Standard(std_fix) = &f.variant {
                        std_fix.name == *name
                    } else {
                        false
                    }
                });
                Ok(ResolvedEntityRef::Fixture(name.clone()))
            }
            ConstraintTarget::Group(name) => {
                // Verify group exists (hierarchical collection)
                // Groups are more abstract; validation deferred to compilation
                Ok(ResolvedEntityRef::Group(name.clone()))
            }
        }
    }
    
    /// Check if a constraint target references a hierarchical entity
    pub fn is_hierarchical(entity_ref: &ResolvedEntityRef) -> bool {
        match entity_ref {
            ResolvedEntityRef::Net(name) | 
            ResolvedEntityRef::Component(name) |
            ResolvedEntityRef::Pipe(name) |
            ResolvedEntityRef::Fixture(name) |
            ResolvedEntityRef::Group(name) => name.contains('.'),
        }
    }
    
    /// Extract the base entity name (before the dot)
    pub fn base_name(entity_ref: &ResolvedEntityRef) -> String {
        let full_name = match entity_ref {
            ResolvedEntityRef::Net(n) | 
            ResolvedEntityRef::Component(n) |
            ResolvedEntityRef::Pipe(n) |
            ResolvedEntityRef::Fixture(n) |
            ResolvedEntityRef::Group(n) => n,
        };
        
        full_name.split('.').next().unwrap_or(full_name).to_string()
    }
    
    /// Extract the path components (for hierarchical references)
    pub fn path_components(entity_ref: &ResolvedEntityRef) -> Vec<String> {
        let full_name = match entity_ref {
            ResolvedEntityRef::Net(n) | 
            ResolvedEntityRef::Component(n) |
            ResolvedEntityRef::Pipe(n) |
            ResolvedEntityRef::Fixture(n) |
            ResolvedEntityRef::Group(n) => n,
        };
        
        full_name.split('.').map(|s| s.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::units::Unit;
    use crate::domains::arch::ast::ArchProjectData;
    use crate::domains::elec::ast::{ElecProjectData, NetDecl};
    use crate::domains::mep_elec::ast::MepElecProjectData;
    use crate::syntax::ext::ExtensionBlock;
    use crate::syntax::ast::ViewDecl;
    
    fn create_test_project() -> ProjectAst {
        ProjectAst {
            name: "Test".to_string(),
            span: Span::new("test.edl", 1, 1, 1, 1),
            default_unit: Unit::Mm,
            imports: vec![],
            modules: vec![],
            uses: vec![],
            frames: vec![],
            parameters: vec![],
            arch: ArchProjectData::default(),
            elec: ElecProjectData {
                nets: vec![
                    NetDecl {
                        name: "VCC".to_string(),
                        span: Span::new("test.edl", 1, 1, 1, 1),
                    },
                ],
                ..Default::default()
            },
            mep_elec: MepElecProjectData::default(),
            extensions: vec![],
            verifications: vec![],
            views: vec![],
            constraints: vec![],
        }
    }
    
    #[test]
    fn test_elaborate_net_target() {
        let project = create_test_project();
        let target = ConstraintTarget::Net("VCC".to_string());
        
        let result = ConstraintElaborator::resolve_target(&project, &target);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ResolvedEntityRef::Net("VCC".to_string()));
    }
    
    #[test]
    fn test_elaborate_component_target() {
        let project = create_test_project();
        let target = ConstraintTarget::Component("IC1".to_string());
        
        let result = ConstraintElaborator::resolve_target(&project, &target);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ResolvedEntityRef::Component("IC1".to_string()));
    }
    
    #[test]
    fn test_hierarchical_detection() {
        let net_flat = ResolvedEntityRef::Net("VCC".to_string());
        let net_hier = ResolvedEntityRef::Net("Module.VCC".to_string());
        
        assert!(!ConstraintElaborator::is_hierarchical(&net_flat));
        assert!(ConstraintElaborator::is_hierarchical(&net_hier));
    }
    
    #[test]
    fn test_base_name_extraction() {
        let net = ResolvedEntityRef::Net("Module.Sub.VCC".to_string());
        
        let base = ConstraintElaborator::base_name(&net);
        assert_eq!(base, "Module");
    }
    
    #[test]
    fn test_path_components() {
        let net = ResolvedEntityRef::Net("Module.Sub.VCC".to_string());
        
        let path = ConstraintElaborator::path_components(&net);
        assert_eq!(path, vec!["Module", "Sub", "VCC"]);
    }
}
