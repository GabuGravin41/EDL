//! Industrial Electrical & 3-Phase Motor Control domain (`elec.industrial`, IEC 60617).

pub mod compile;
pub mod render;

use crate::core::ir::CanonicalIr;
use crate::domains::registry::{DomainEngine, DomainRole, ExportFormat, RuleInfo};
use crate::syntax::ast::ProjectAst;

pub struct IndustrialControlDomain;

impl DomainEngine for IndustrialControlDomain {
    fn id(&self) -> &'static str {
        "elec.industrial"
    }
    fn description(&self) -> &'static str {
        "IEC 60617 industrial electrical & 3-phase motor control ladders: split multi-unit contactors, cross-references, formal interlock proofs, AutoCAD DXF"
    }
    fn role(&self) -> DomainRole {
        DomainRole::Base
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["industrial_control"]
    }
    fn activates(&self, ast: &ProjectAst) -> bool {
        ast.extensions.iter().any(|b| b.keyword == "industrial_control")
    }
    fn compile(&self, ast: &ProjectAst, _ir: Option<CanonicalIr>) -> CanonicalIr {
        compile::compile_industrial(ast)
    }
    fn render_svg(&self, ir: &CanonicalIr, _annotated: bool) -> Option<String> {
        (ir.project.domain == "elec.industrial").then(|| render::render_industrial_svg(ir))
    }
    fn exports(&self) -> Vec<ExportFormat> {
        vec![ExportFormat {
            name: "dxf",
            extension: "dxf",
            description: "Layered AutoCAD R12 DXF with IEC_POWER_BUS, IEC_CONTROL_WIRE, IEC_SYMBOLS, IEC_MECH_LINK, IEC_TEXT, IEC_CROSSREF",
        }]
    }
    fn export(&self, format: &str, ir: &CanonicalIr) -> Option<String> {
        (format == "dxf" && ir.project.domain == "elec.industrial")
            .then(|| render::export_industrial_dxf(ir))
    }
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            RuleInfo { id: "I1_mutual_interlock", description: "Exhaustive Boolean proof that paired contactors (FWD/REV, Star/Delta) cannot energize simultaneously" },
            RuleInfo { id: "I2_estop_overload_authority", description: "Opening E-Stop or thermal overload de-energizes every motor coil" },
            RuleInfo { id: "I3_coil_contact_cross_ref", description: "All coil <-> power pole & auxiliary NO/NC contact references are valid and cross-referenced" },
            RuleInfo { id: "I4_phase_reversal_integrity", description: "Reversing contactor swaps exactly two phases (L1 <-> L3) relative to forward contactor" },
        ]
    }
}
