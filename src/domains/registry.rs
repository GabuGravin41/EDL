//! Domain plugin architecture (v0.9).
//!
//! A *domain* is anything that can turn part of an EDL project into entities, relations, geometry and evidence in
//! the shared [`CanonicalIr`]: architecture, electronics, building electrical, and — via the same trait — plumbing,
//! HVAC, PCB, mechanical, structural...
//!
//! * **Base** engines create the IR (`arch`, `elec`). Exactly one base engine runs per project.
//! * **Overlay** engines enrich an existing IR in staged order (`mep.elec` layers on top of `arch`; a plumbing
//!   engine would do the same). Overlays read the solved base geometry through the IR, never through each other's AST.
//! * New top-level syntax needs **no parser change**: unknown `keyword Name { ... }` blocks are captured as
//!   [`ExtensionBlock`]s and handed to the engine that claims the keyword (see `syntax::ext`).
//! * Engines may contribute an SVG overlay drawn in *project model coordinates*, extra export formats and
//!   documented verification rules (shown by `edl domains` and the `edl_domains` MCP tool).

use crate::core::ir::{CanonicalIr, VerificationStatus};
use crate::core::span::{Diagnostic, Severity};
use crate::syntax::ast::ProjectAst;
use crate::syntax::ext::ExtensionBlock;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainRole {
    Base,
    Overlay,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleInfo {
    pub id: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportFormat {
    pub name: &'static str,
    pub extension: &'static str,
    pub description: &'static str,
}

/// Linear map from project model coordinates (mm) to SVG pixels: `px = ox + kx*x`, `py = oy + ky*y`.
/// (`ky` is negative for Y-up plan views.) Parsed from the `<!--EDL_VIEW ...-->` marker the base renderer emits.
#[derive(Debug, Clone, Copy)]
pub struct ViewTransform {
    pub ox: f64,
    pub oy: f64,
    pub kx: f64,
    pub ky: f64,
}

impl ViewTransform {
    pub fn map(&self, x_mm: f64, y_mm: f64) -> (f64, f64) {
        (self.ox + self.kx * x_mm, self.oy + self.ky * y_mm)
    }
    pub fn px_per_mm(&self) -> f64 {
        self.kx.abs()
    }
    fn parse(svg: &str) -> Option<Self> {
        let start = svg.find("<!--EDL_VIEW ")? + "<!--EDL_VIEW ".len();
        let end = start + svg[start..].find("-->")?;
        let mut t = ViewTransform { ox: 0.0, oy: 0.0, kx: 1.0, ky: 1.0 };
        for kv in svg[start..end].split_whitespace() {
            let (k, v) = kv.split_once('=')?;
            let v: f64 = v.parse().ok()?;
            match k {
                "ox" => t.ox = v,
                "oy" => t.oy = v,
                "kx" => t.kx = v,
                "ky" => t.ky = v,
                _ => {}
            }
        }
        Some(t)
    }
}

pub trait DomainEngine: Send + Sync {
    /// Stable id, e.g. `"arch"`, `"elec"`, `"mep.elec"`, `"plumbing"`.
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn role(&self) -> DomainRole;
    /// Top-level project keywords this engine owns *beyond* the built-in parser (extension blocks).
    fn keywords(&self) -> &'static [&'static str] {
        &[]
    }
    /// Does this project contain anything for this engine?
    fn activates(&self, ast: &ProjectAst) -> bool;
    /// Base: `ir` is `None` and a fresh IR must be returned. Overlay: `ir` is the solved base IR to enrich.
    fn compile(&self, ast: &ProjectAst, ir: Option<CanonicalIr>) -> CanonicalIr;
    /// Base engines: render the full preview when `ir.project.domain` is theirs.
    fn render_svg(&self, _ir: &CanonicalIr, _annotated: bool) -> Option<String> {
        None
    }
    /// Overlay engines: SVG fragment drawn in model coordinates through `view`, appended to the base preview.
    fn render_overlay(&self, _ir: &CanonicalIr, _view: &ViewTransform) -> Option<String> {
        None
    }
    fn exports(&self) -> Vec<ExportFormat> {
        Vec::new()
    }
    fn export(&self, _format: &str, _ir: &CanonicalIr) -> Option<String> {
        None
    }
    fn rules(&self) -> Vec<RuleInfo> {
        Vec::new()
    }
}

pub struct DomainRegistry {
    engines: Vec<Box<dyn DomainEngine>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DomainInfo {
    pub id: &'static str,
    pub description: &'static str,
    pub role: DomainRole,
    pub keywords: Vec<&'static str>,
    pub exports: Vec<ExportFormat>,
    pub rules: Vec<RuleInfo>,
}

impl DomainRegistry {
    pub fn empty() -> Self {
        DomainRegistry { engines: Vec::new() }
    }

    /// The domains that ship with EDL: `arch`, `elec`, `elec.industrial` (base) and `mep.elec`, `mep.plumbing` (overlays on `arch`).
    pub fn builtin() -> Self {
        let mut r = Self::empty();
        r.register(Box::new(ArchDomain));
        r.register(Box::new(ElecDomain));
        r.register(Box::new(super::industrial::IndustrialControlDomain));
        r.register(Box::new(MepElecDomain));
        r.register(Box::new(super::mep_plumbing::MepPlumbingDomain));
        r
    }

    /// Register a domain. Later registrations of the same id replace earlier ones (lets a user override a built-in).
    pub fn register(&mut self, engine: Box<dyn DomainEngine>) -> &mut Self {
        self.engines.retain(|e| e.id() != engine.id());
        self.engines.push(engine);
        self
    }

    pub fn info(&self) -> Vec<DomainInfo> {
        self.engines
            .iter()
            .map(|e| DomainInfo {
                id: e.id(),
                description: e.description(),
                role: e.role(),
                keywords: e.keywords().to_vec(),
                exports: e.exports(),
                rules: e.rules(),
            })
            .collect()
    }

    /// Compile a parsed + elaborated project: base engine first, then overlays in registration order, then
    /// report any extension block that no registered engine claims.
    pub fn compile_ast(&self, ast: &ProjectAst) -> CanonicalIr {
        let base = self
            .engines
            .iter()
            .rev() // later registrations win: plugin bases take priority over the broad built-in fallback
            .filter(|e| e.role() == DomainRole::Base)
            .find(|e| e.activates(ast))
            // Historical default: an empty/unknown project compiles as architecture.
            .or_else(|| self.engines.iter().find(|e| e.id() == "arch"))
            .or_else(|| self.engines.iter().find(|e| e.role() == DomainRole::Base));
        let mut ir = match base {
            Some(b) => b.compile(ast, None),
            None => panic!("DomainRegistry has no base engine registered"),
        };
        for e in self.engines.iter().filter(|e| e.role() == DomainRole::Overlay) {
            if e.activates(ast) {
                ir = e.compile(ast, Some(ir));
            }
        }
        self.report_unclaimed_blocks(ast, &mut ir);
        ir
    }

    fn report_unclaimed_blocks(&self, ast: &ProjectAst, ir: &mut CanonicalIr) {
        let claimed: Vec<&str> = self.engines.iter().flat_map(|e| e.keywords().iter().copied()).collect();
        for ExtensionBlock { keyword, span, .. } in &ast.extensions {
            if claimed.contains(&keyword.as_str()) {
                continue;
            }
            let mut d = Diagnostic::error(
                "E_UNKNOWN_DOMAIN_BLOCK",
                "syntax",
                format!("Unknown top-level declaration '{keyword}': no registered domain claims this keyword"),
                Some(span.clone()),
            );
            d.suggestion = Some(format!(
                "Built-ins: room, opening, fixture, layout, architecture.defaults, circuit, schematic, building_electrical, verify, view, module, use, import, parameter, frame. Registered extension keywords: [{}]",
                claimed.join(", ")
            ));
            ir.diagnostics.push(d);
            ir.project.has_errors = true;
        }
        if ir.diagnostics.iter().any(|d| d.severity == Severity::Error) {
            ir.project.has_errors = true;
        }
    }

    /// Render the preview: the base engine owning `ir.project.domain`, then every overlay's model-space fragment.
    pub fn render_svg(&self, ir: &CanonicalIr, annotated: bool) -> String {
        self.render_svg_with_layers(ir, annotated, None)
    }

    /// Render the preview with optional discipline layer selection (`arch`, `elec`/`mep.elec`, `plumbing`/`mep.plumbing`, or `all`).
    pub fn render_svg_with_layers(
        &self,
        ir: &CanonicalIr,
        annotated: bool,
        layers: Option<&[String]>,
    ) -> String {
        let want_all = layers
            .map(|ls| ls.is_empty() || ls.iter().any(|s| s.eq_ignore_ascii_case("all")))
            .unwrap_or(true);
        let want_elec = want_all
            || layers
                .map(|ls| {
                    ls.iter().any(|s| {
                        let low = s.to_ascii_lowercase();
                        low == "elec" || low == "mep.elec" || low == "electrical" || low == "mep"
                    })
                })
                .unwrap_or(true);

        let is_arch = ir.project.domain == "architecture"
            || ir.project.domain.starts_with("arch")
            || ir.project.domain.is_empty();

        let mut svg = if is_arch {
            super::arch::svg::render_svg_filtered(ir, annotated, want_elec)
        } else {
            self.engines
                .iter()
                .rev()
                .filter(|e| e.role() == DomainRole::Base)
                .find_map(|e| e.render_svg(ir, annotated))
                .unwrap_or_else(|| String::from("<svg xmlns=\"http://www.w3.org/2000/svg\"/>"))
        };
        if let Some(view) = ViewTransform::parse(&svg) {
            let frags: String = self
                .engines
                .iter()
                .filter(|e| e.role() == DomainRole::Overlay)
                .filter(|e| {
                    if want_all {
                        return true;
                    }
                    let id = e.id().to_ascii_lowercase();
                    layers.map(|ls| {
                        ls.iter().any(|s| {
                            let low = s.to_ascii_lowercase();
                            low == id
                                || (id == "mep.plumbing" && (low == "plumbing" || low == "mep"))
                                || (id == "mep.elec" && (low == "elec" || low == "electrical" || low == "mep"))
                        })
                    }).unwrap_or(true)
                })
                .filter_map(|e| {
                    e.render_overlay(ir, &view)
                        .map(|f| format!("  <g id=\"overlay-{}\">\n{}\n  </g>\n", e.id(), f))
                })
                .collect();
            if !frags.is_empty() {
                if let Some(pos) = svg.rfind("</svg>") {
                    svg.insert_str(pos, &frags);
                }
            }
        }
        svg
    }

    /// Export by format name (`dxf`, `kicad`, `spice`, `bom`, `schedule`, or any plugin-defined name).
    pub fn export(&self, format: &str, ir: &CanonicalIr) -> Result<String, String> {
        for e in &self.engines {
            if let Some(s) = e.export(format, ir) {
                return Ok(s);
            }
        }
        let known: Vec<&str> = self.engines.iter().flat_map(|e| e.exports()).map(|f| f.name).collect();
        Err(format!("Unknown export format '{format}' (available: {})", known.join(", ")))
    }
}

// ---------------------------------------------------------------------------------------------------------
// Built-in engines
// ---------------------------------------------------------------------------------------------------------

pub struct ArchDomain;
pub struct ElecDomain;
pub struct MepElecDomain;

impl DomainEngine for ArchDomain {
    fn id(&self) -> &'static str {
        "arch"
    }
    fn description(&self) -> &'static str {
        "2D architectural floor plans: constraint-solved rooms, openings, fixtures, notches, walls"
    }
    fn role(&self) -> DomainRole {
        DomainRole::Base
    }
    fn activates(&self, ast: &ProjectAst) -> bool {
        !ast.arch.rooms.is_empty() || ast.elec.circuits.is_empty()
    }
    fn compile(&self, ast: &ProjectAst, _ir: Option<CanonicalIr>) -> CanonicalIr {
        super::arch::compile::compile_project(ast)
    }
    fn render_svg(&self, ir: &CanonicalIr, annotated: bool) -> Option<String> {
        let is_arch = ir.project.domain == "architecture"
            || ir.project.domain.starts_with("arch")
            || ir.project.domain.is_empty();
        is_arch.then(|| super::arch::svg::render_svg(ir, annotated))
    }
    fn exports(&self) -> Vec<ExportFormat> {
        vec![ExportFormat {
            name: "dxf",
            extension: "dxf",
            description: "AutoCAD R12 ASCII DXF, layered, with EDL_ID anchors for `edl sync --from-dxf`",
        }]
    }
    fn export(&self, format: &str, ir: &CanonicalIr) -> Option<String> {
        let is_arch = ir.project.domain == "architecture"
            || ir.project.domain.starts_with("arch")
            || ir.project.domain.is_empty();
        (format == "dxf" && is_arch).then(|| super::arch::dxf::export_dxf(ir))
    }
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            RuleInfo { id: "T2_linear_layout_solve", description: "Rooms uniquely determined (0 remaining DOF)" },
            RuleInfo { id: "V1_no_room_overlap", description: "No two rooms overlap" },
            RuleInfo { id: "V2_opening_jamb_margin", description: "Openings keep minimum jamb margin" },
            RuleInfo { id: "V3_no_door_swing_collision", description: "Door swing arcs do not collide" },
            RuleInfo { id: "V4_all_rooms_reachable", description: "Every room reachable from an entry" },
            RuleInfo { id: "V5_no_fixture_collision", description: "Fixtures and their clearances do not collide" },
            RuleInfo { id: "V6_habitable_rooms_have_window", description: "Habitable rooms have natural light" },
        ]
    }
}

impl DomainEngine for ElecDomain {
    fn id(&self) -> &'static str {
        "elec"
    }
    fn description(&self) -> &'static str {
        "Electronics schematics: typed pins, routed nets, ERC, MNA DC simulation, BOM, KiCad/SPICE export"
    }
    fn role(&self) -> DomainRole {
        DomainRole::Base
    }
    fn activates(&self, ast: &ProjectAst) -> bool {
        !ast.elec.circuits.is_empty() && ast.arch.rooms.is_empty()
    }
    fn compile(&self, ast: &ProjectAst, _ir: Option<CanonicalIr>) -> CanonicalIr {
        super::elec::compile::compile_electronics(ast)
    }
    fn render_svg(&self, ir: &CanonicalIr, annotated: bool) -> Option<String> {
        (ir.project.domain == "electronics").then(|| super::elec::svg::render_svg(ir, annotated))
    }
    fn exports(&self) -> Vec<ExportFormat> {
        vec![
            ExportFormat { name: "kicad", extension: "kicad_sch", description: "KiCad 9 schematic with EDL_ID properties (ERC-clean)" },
            ExportFormat { name: "spice", extension: "cir", description: "SPICE3 netlist" },
            ExportFormat { name: "bom", extension: "csv", description: "Grouped bill of materials" },
        ]
    }
    fn export(&self, format: &str, ir: &CanonicalIr) -> Option<String> {
        match format {
            "kicad" => Some(super::elec::kicad::export_kicad_sch(ir)),
            "spice" => Some(super::elec::spice::export_spice_netlist(ir)),
            "bom" => Some(super::elec::bom::export_bom_csv(ir)),
            _ => None,
        }
    }
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            RuleInfo { id: "E1_all_required_pins_connected", description: "Required pins are connected" },
            RuleInfo { id: "E2_no_power_ground_short", description: "No power-to-ground shorts" },
            RuleInfo { id: "E3_no_multiple_power_drivers", description: "No conflicting power drivers" },
            RuleInfo { id: "E4_voltage_compatibility", description: "Rated pins within voltage limits" },
            RuleInfo { id: "E5_no_single_pin_nets", description: "Nets connect at least two pins" },
            RuleInfo { id: "K1_kicad_erc_external", description: "Real KiCad 9 ERC (opt-in `--kicad-erc`)" },
        ]
    }
}

impl DomainEngine for MepElecDomain {
    fn id(&self) -> &'static str {
        "mep.elec"
    }
    fn description(&self) -> &'static str {
        "Building electrical layered on architecture: panels, circuits, lights, switches, sockets, IEC 60364 checks"
    }
    fn role(&self) -> DomainRole {
        DomainRole::Overlay
    }
    fn activates(&self, ast: &ProjectAst) -> bool {
        !ast.mep_elec.installations.is_empty()
    }
    fn compile(&self, ast: &ProjectAst, ir: Option<CanonicalIr>) -> CanonicalIr {
        let mut ir = ir.expect("mep.elec is an overlay and requires a base IR");
        super::mep_elec::compile::compile_building_electrical(ast, &mut ir);
        ir
    }
    fn exports(&self) -> Vec<ExportFormat> {
        vec![ExportFormat { name: "schedule", extension: "csv", description: "DB panel & circuit schedule" }]
    }
    fn export(&self, format: &str, ir: &CanonicalIr) -> Option<String> {
        (format == "schedule").then(|| super::mep_elec::schedule::export_panel_schedule_csv(ir))
    }
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            RuleInfo { id: "M1_breaker_load_capacity", description: "Circuit load within 80% of breaker rating" },
            RuleInfo { id: "M2_voltage_drop_limit", description: "Copper voltage drop within limit" },
            RuleInfo { id: "M3_wet_zone_clearance", description: "Devices clear of wet zones" },
            RuleInfo { id: "M4_switch_door_accessibility", description: "Switches clear of door swings; lights controlled" },
            RuleInfo { id: "M5_device_wall_clearance", description: "Devices clear of openings" },
            RuleInfo { id: "M6_wet_circuit_rcd", description: "Wet/socket circuits RCD protected" },
        ]
    }
}

/// Convenience for plugin authors: append an evidence record.
pub fn push_evidence(
    ir: &mut CanonicalIr,
    check_id: &str,
    status: VerificationStatus,
    subjects: Vec<String>,
    details: impl Into<String>,
) {
    ir.evidence.push(crate::core::ir::EvidenceRecord {
        check_id: check_id.to_string(),
        status,
        method: "domain_plugin".to_string(),
        tolerance_mm: None,
        subjects,
        details: details.into(),
    });
}
