//! Built-in Building Power Single-Line Diagram Domain (`elec.sld`) — v1.6.0.

pub mod compile;
pub mod render;

use crate::core::ir::CanonicalIr;
use crate::domains::registry::{DomainEngine, DomainRole, ExportFormat, RuleInfo};
use crate::syntax::ast::ProjectAst;

pub struct PowerSldDomain;

impl DomainEngine for PowerSldDomain {
    fn id(&self) -> &'static str {
        "elec.sld"
    }
    fn description(&self) -> &'static str {
        "Building MV/LV power distribution single-line diagrams (SLD): transformers, generators, PV/BESS, bus couplers, IEC 60909 Ik3'' & N-1 contingency verification"
    }
    fn role(&self) -> DomainRole {
        DomainRole::Base
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["power_sld"]
    }
    fn activates(&self, ast: &ProjectAst) -> bool {
        ast.extensions.iter().any(|b| b.keyword == "power_sld")
    }
    fn compile(&self, ast: &ProjectAst, _ir: Option<CanonicalIr>) -> CanonicalIr {
        compile::compile_power_sld(ast)
    }
    fn render_svg(&self, ir: &CanonicalIr, _annotated: bool) -> Option<String> {
        (ir.project.domain == "elec.sld").then(|| render::render_sld_svg(ir))
    }
    fn exports(&self) -> Vec<ExportFormat> {
        vec![ExportFormat {
            name: "dxf",
            extension: "dxf",
            description: "Layered AutoCAD R12 DXF with SLD_BUSBAR, SLD_FEEDER, SLD_SYMBOLS, SLD_INTERLOCK, SLD_TEXT, SLD_METRICS",
        }]
    }
    fn export(&self, format: &str, ir: &CanonicalIr) -> Option<String> {
        (format == "dxf" && ir.project.domain == "elec.sld").then(|| render::export_sld_dxf(ir))
    }
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            RuleInfo { id: "S1_source_bus_and_n1_capacity", description: "Transformers, generators, busbars & N-1 single-transformer contingency within rated capacity" },
            RuleInfo { id: "S2_breaker_cable_coordination", description: "IEC 60364 Ib <= In <= Iz verified on every feeder and load branch" },
            RuleInfo { id: "S3_protection_selectivity", description: "Upstream vs. downstream breaker selectivity cascade verified" },
            RuleInfo { id: "S4_short_circuit_breaking_capacity", description: "IEC 60909 prospective 3-phase short-circuit Ik3'' <= breaker Icu everywhere" },
            RuleInfo { id: "S5_cumulative_voltage_drop", description: "End-to-end cumulative voltage drop from MV/LV transformer to every load within IEC limit" },
            RuleInfo { id: "S6_ats_coupler_interlock", description: "Formally verifies 2-of-3 ATS / mechanical interlock across split-bus dual sources" },
        ]
    }
}
