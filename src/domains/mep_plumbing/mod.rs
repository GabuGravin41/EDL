//! Built-in Building Plumbing Overlay Domain (`mep.plumbing`) — v1.4.0.

pub mod compile;
pub mod render;

use crate::core::ir::CanonicalIr;
use crate::domains::registry::{DomainEngine, DomainRole, ExportFormat, RuleInfo, ViewTransform};
use crate::syntax::ast::ProjectAst;

pub struct MepPlumbingDomain;

impl DomainEngine for MepPlumbingDomain {
    fn id(&self) -> &'static str {
        "mep.plumbing"
    }
    fn description(&self) -> &'static str {
        "Building plumbing layered on architecture: CWS/HWS supply, DWV soil/waste, valves, EN 806/12056 & MEP clash checks"
    }
    fn role(&self) -> DomainRole {
        DomainRole::Overlay
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["building_plumbing"]
    }
    fn activates(&self, ast: &ProjectAst) -> bool {
        ast.extensions.iter().any(|b| b.keyword == "building_plumbing")
    }
    fn compile(&self, ast: &ProjectAst, ir: Option<CanonicalIr>) -> CanonicalIr {
        let mut ir = ir.expect("mep.plumbing is an overlay and requires a base IR");
        compile::compile_building_plumbing(ast, &mut ir);
        ir
    }
    fn render_overlay(&self, ir: &CanonicalIr, view: &ViewTransform) -> Option<String> {
        render::render_plumbing_overlay(ir, view)
    }
    fn exports(&self) -> Vec<ExportFormat> {
        vec![ExportFormat {
            name: "plumbing_schedule",
            extension: "csv",
            description: "Plumbing pipe & equipment takeoff schedule (CSV)",
        }]
    }
    fn export(&self, format: &str, ir: &CanonicalIr) -> Option<String> {
        (format == "plumbing_schedule").then(|| render::format_plumbing_schedule(ir, true))
    }
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            RuleInfo { id: "P1_wet_fixture_supply", description: "Every wet architectural fixture has verified cold/hot supply & drainage" },
            RuleInfo { id: "P2_drainage_sizing_and_slope", description: "Soil (>=100mm) and waste (>=40/50mm) pipes satisfy EN 12056 gravity slopes" },
            RuleInfo { id: "P3_trap_arm_distance", description: "Unvented drain trap arms stay within maximum siphonage limits" },
            RuleInfo { id: "P4_supply_velocity_and_sizing", description: "Supply water velocity <= 2.0 m/s to prevent noise & erosion" },
            RuleInfo { id: "P5_isolation_valves", description: "Shutoff isolation valves present on main cold supply and water heater" },
            RuleInfo { id: "P6_cross_discipline_clearance", description: "Zero wall/panel clashes between plumbing nodes and co-resident electrical devices" },
        ]
    }
}
