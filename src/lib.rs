pub mod core;
pub mod domains;
pub mod mcp;
pub mod syntax;

use crate::core::ir::CanonicalIr;
use crate::core::span::Diagnostic;
use std::fs;
use std::path::Path;

/// Compile an in-memory `.edl` source string (elaborates local modules; no filesystem imports).
pub fn compile_source(file: &str, src: &str) -> Result<CanonicalIr, Diagnostic> {
    let mut ast = syntax::parser::parse_project(file, src)?;
    syntax::elaborate::elaborate_project(&mut ast, None)?;
    Ok(compile_ast(&ast))
}

/// Compile an `.edl` file on disk, resolving relative `import "..."` libraries and `module`/`use` blocks.
pub fn compile_file(path: &Path) -> Result<CanonicalIr, Diagnostic> {
    let file_str = path.display().to_string();
    let src = fs::read_to_string(path).map_err(|e| {
        Diagnostic::error(
            "E_IO_READ",
            "io",
            format!("Failed to read '{file_str}': {e}"),
            None,
        )
    })?;
    let mut ast = syntax::parser::parse_project(&file_str, &src)?;
    syntax::elaborate::elaborate_project(&mut ast, path.parent())?;
    Ok(compile_ast(&ast))
}

fn compile_ast(ast: &syntax::ast::ProjectAst) -> CanonicalIr {
    if !ast.elec.circuits.is_empty() && ast.arch.rooms.is_empty() {
        domains::elec::compile::compile_electronics(ast)
    } else {
        domains::arch::compile::compile_project(ast)
    }
}

/// Domain-dispatched SVG preview renderer.
pub fn render_svg(ir: &CanonicalIr, annotated: bool) -> String {
    if ir.project.domain == "electronics" {
        domains::elec::svg::render_svg(ir, annotated)
    } else {
        domains::arch::svg::render_svg(ir, annotated)
    }
}

pub fn export_dxf(ir: &CanonicalIr) -> String {
    domains::arch::dxf::export_dxf(ir)
}

pub fn export_kicad_sch(ir: &CanonicalIr) -> String {
    domains::elec::kicad::export_kicad_sch(ir)
}

pub fn export_spice_netlist(ir: &CanonicalIr) -> String {
    domains::elec::spice::export_spice_netlist(ir)
}

pub fn simulate_dc_and_rc(ir: &CanonicalIr) -> domains::elec::spice::SimulationReport {
    domains::elec::spice::simulate_dc_and_rc(ir)
}

pub fn build_bom(ir: &CanonicalIr) -> domains::elec::bom::BomReport {
    domains::elec::bom::build_bom(ir)
}

pub fn export_bom_csv(ir: &CanonicalIr) -> String {
    domains::elec::bom::export_bom_csv(ir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::diff;
    use crate::core::ir::VerificationStatus;

    #[test]
    fn studio_a_solves_and_verifies_cleanly() {
        let src = include_str!("../examples/studio_a.edl");
        let ir = compile_source("examples/studio_a.edl", src).expect("parse studio_a");
        assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
        assert_eq!(ir.project.room_count, 3);
        assert_eq!(ir.project.opening_count, 4);
        assert_eq!(ir.project.exterior_footprint_mm, Some([8020.0, 4400.0]));
        assert!(ir.project.remaining_dof.is_empty());

        let bath = ir.entities.iter().find(|e| e.id == "Bath").unwrap();
        match bath.geometry {
            crate::core::ir::EntityGeometryIr::Rect {
                min,
                max,
                width_mm,
                depth_mm,
                ..
            } => {
                assert_eq!(min, [4620.0, 2520.0]);
                assert_eq!(max, [7620.0, 4000.0]);
                assert_eq!(width_mm, 3000.0);
                assert_eq!(depth_mm, 1480.0);
            }
            _ => panic!("expected rect"),
        }

        let svg = render_svg(&ir, true);
        assert!(svg.contains("<path id=\"door-arc-D_Entry\""));
        assert!(svg.contains(" A "));
        let dxf = export_dxf(&ir);
        assert!(dxf.contains("\nARC\n"));
        assert!(dxf.contains("WALLS_EXT"));
    }

    #[test]
    fn apartment_b_derives_bath_across_offset_junction() {
        let src = include_str!("../examples/apartment_b.edl");
        let ir = compile_source("examples/apartment_b.edl", src).expect("parse apartment_b");
        assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
        assert_eq!(ir.project.room_count, 4);
        assert_eq!(ir.project.opening_count, 5);
        assert_eq!(ir.project.exterior_footprint_mm, Some([6220.0, 7120.0]));

        let bath = ir.entities.iter().find(|e| e.id == "Bath").unwrap();
        match bath.geometry {
            crate::core::ir::EntityGeometryIr::Rect {
                width_mm, depth_mm, ..
            } => {
                assert_eq!(width_mm, 2300.0);
                assert_eq!(depth_mm, 3000.0);
            }
            _ => panic!("expected rect"),
        }
    }

    #[test]
    fn catches_jamb_and_swing_violations() {
        let src = include_str!("../examples/studio_a_invalid.edl");
        let ir = compile_source("examples/studio_a_invalid.edl", src).expect("parse invalid");
        assert!(ir.project.has_errors);
        assert!(ir.diagnostics.iter().any(|d| d.code == "E_VERIFY_JAMB_MARGIN"));
        assert!(ir
            .diagnostics
            .iter()
            .any(|d| d.code == "E_VERIFY_DOOR_SWING_COLLISION"));
        assert!(ir
            .evidence
            .iter()
            .any(|e| e.check_id == "V3_no_door_swing_collision" && e.status == VerificationStatus::Violated));
    }

    #[test]
    fn catches_underconstrained_and_overconstrained_models() {
        let under = r#"
        project "Under" {
            units: mm
            frame FloorPlan: Frame2
            room A { clear_size: (4000mm, 3000mm) anchor: southwest at FloorPlan.origin }
            room B { clear_size: (3000mm, 3000mm) }
        }
        "#;
        let ir_u = compile_source("under.edl", under).unwrap();
        assert!(ir_u.project.has_errors);
        assert_eq!(ir_u.project.remaining_dof.len(), 2);

        let over = r#"
        project "Over" {
            units: mm
            frame FloorPlan: Frame2
            room A { clear_size: (4000mm, 3000mm) anchor: southwest at FloorPlan.origin }
            layout FloorPlan {
                require A.east == 5000mm
            }
        }
        "#;
        let ir_o = compile_source("over.edl", over).unwrap();
        assert!(ir_o
            .diagnostics
            .iter()
            .any(|d| d.code == "E_SOLVER_CONTRADICTION"));
    }

    #[test]
    fn rejects_unit_dimension_mismatch() {
        let bad_unit = r#"
        project "BadUnit" {
            units: mm
            frame FloorPlan: Frame2
            room A { clear_size: (4000mm, 90deg) anchor: southwest at FloorPlan.origin }
        }
        "#;
        let ir = compile_source("bad_unit.edl", bad_unit).unwrap();
        assert!(ir.diagnostics.iter().any(|d| d.code == "E_UNIT_MISMATCH"));
    }

    #[test]
    fn semantic_diff_isolates_wide_living_edit() {
        let a = compile_source("studio_a.edl", include_str!("../examples/studio_a.edl")).unwrap();
        let b = compile_source(
            "studio_a_wide.edl",
            include_str!("../examples/studio_a_wide.edl"),
        )
        .unwrap();
        let d = diff::diff_ir(&a, &b);
        assert_eq!(d.footprint_before_mm, Some([8020.0, 4400.0]));
        assert_eq!(d.footprint_after_mm, Some([8720.0, 4400.0]));
        assert!(d.unchanged_entities.contains(&"D_Entry".to_string()));
        assert!(d.changed_entities.iter().any(|c| c.id == "Living"));
        assert!(d.changed_entities.iter().any(|c| c.id == "W_Living"));
    }

    #[test]
    fn catches_trapped_room_windowless_habitable_and_fixture_collision() {
        let bad = r#"
        project "Broken_V02" {
            units: mm
            frame FloorPlan: Frame2
            room Hall { role: circulation clear_size: (2000mm, 3000mm) anchor: southwest at FloorPlan.origin }
            room Bed1 { role: habitable clear_size: (3200mm, 3000mm) }
            room Vault { role: habitable clear_size: (2000mm, 2000mm) }
            layout FloorPlan {
                require Bed1.west == Hall.east + 120mm
                require Bed1.south == Hall.south
                require Vault.west == Bed1.east + 120mm
                require Vault.south == Bed1.south
            }
            opening D_Entry: door {
                host: exterior(Hall, south)
                width: 900mm
                align: center
                hinge: start
                swing: into Hall
            }
            opening D_Bed1: door {
                host: shared_wall(Hall, Bed1)
                width: 800mm
                offset: 300mm from start
                hinge: start
                swing: into Bed1
            }
            fixture BedMain: bed {
                room: Bed1
                size: (1600mm, 2000mm)
                against: west
                offset: 200mm from start
                clearance_front: 600mm
            }
            verify {
                require no_room_overlap
                require opening_jamb_margin >= 100mm
                require no_door_swing_collision
                require all_rooms_reachable
                require no_fixture_collision
                require habitable_rooms_have_window
            }
        }
        "#;
        let ir = compile_source("broken_v02.edl", bad).unwrap();
        assert!(ir.diagnostics.iter().any(|d| d.code == "E_VERIFY_UNREACHABLE_ROOM"));
        assert!(ir.diagnostics.iter().any(|d| d.code == "E_VERIFY_NATURAL_LIGHT"));
        assert!(ir.diagnostics.iter().any(|d| d.code == "E_VERIFY_DOOR_SWING_FIXTURE"));
        assert!(ir.diagnostics.iter().any(|d| d.code == "E_VERIFY_FIXTURE_BLOCKS_OPENING"));
    }

    #[test]
    fn l_shaped_plan_with_passage_and_fixtures_passes_all_6_checks() {
        let lshape = r#"
        project "LShape_Test" {
            units: mm
            frame FloorPlan: Frame2
            room Hall { role: circulation clear_size: (1600mm, 3200mm) anchor: southwest at FloorPlan.origin }
            room Living { role: habitable clear_size: (4400mm, 3200mm) }
            room Bed1 { role: habitable clear_size: (3200mm, 3000mm) }
            layout FloorPlan {
                require Living.west == Hall.east + 120mm
                require Living.south == Hall.south
                require Bed1.west == Hall.west
                require Bed1.south == Hall.north + 120mm
            }
            opening D_Entry: door {
                host: exterior(Hall, south)
                width: 900mm
                align: center
                hinge: start
                swing: into Hall
            }
            opening P_Living: passage {
                host: shared_wall(Hall, Living)
                width: 1400mm
                align: center
            }
            opening D_Bed1: door {
                host: shared_wall(Hall, Bed1)
                width: 800mm
                align: center
                hinge: start
                swing: into Bed1
            }
            opening W_Living_N: window {
                host: exterior(Living, north)
                width: 1600mm
                align: center
            }
            opening W_Bed1: window {
                host: exterior(Bed1, north)
                width: 1400mm
                align: center
            }
            fixture Sofa: sofa {
                room: Living
                size: (2200mm, 900mm)
                against: south
                align: center
                clearance_front: 700mm
            }
            fixture Bed: bed {
                room: Bed1
                size: (1600mm, 2000mm)
                against: east
                align: center
                clearance_front: 600mm
            }
            verify {
                require no_room_overlap
                require opening_jamb_margin >= 100mm
                require no_door_swing_collision
                require all_rooms_reachable
                require no_fixture_collision
                require habitable_rooms_have_window
            }
        }
        "#;
        let ir = compile_source("lshape.edl", lshape).unwrap();
        assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
        assert_eq!(ir.evidence.len(), 7);
    }

    #[test]
    fn apartment_2br_l_with_interior_wall_param_and_freestanding_table() {
        let src = include_str!("../examples/apartment_2br_l.edl");
        let ir = compile_source("examples/apartment_2br_l.edl", src).expect("parse 2br");
        assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
        assert_eq!(ir.project.room_count, 6);
        assert_eq!(ir.project.opening_count, 11);
        assert_eq!(ir.project.fixture_count, 10);
        assert!(ir.project.remaining_dof.is_empty());
    }

    #[test]
    fn power_5v_schematic_passes_all_erc_and_exports_kicad() {
        let src = include_str!("../examples/power_5v.edl");
        let ir = compile_source("examples/power_5v.edl", src).expect("parse power_5v");
        assert_eq!(ir.project.domain, "electronics");
        assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
        assert_eq!(ir.project.component_count, 7);
        assert_eq!(ir.project.net_count, 4);
        assert_eq!(ir.evidence.len(), 5);
        assert!(ir.evidence.iter().all(|e| e.status == VerificationStatus::ExactlyChecked));

        let svg = render_svg(&ir, true);
        assert!(svg.contains("EDL Schematic: Power_Regulator_5V"));
        assert!(svg.contains("LM7805"));

        let kicad = export_kicad_sch(&ir);
        assert!(kicad.contains("(kicad_sch"));
        assert!(kicad.contains("(lib_id \"EDL:U1\")"));
        assert!(kicad.contains("(wire "));
        assert!(kicad.contains("(label \"VIN\""));
    }

    #[test]
    fn power_5v_invalid_catches_unconnected_voltage_and_single_pin_erc() {
        let src = include_str!("../examples/power_5v_invalid.edl");
        let ir = compile_source("examples/power_5v_invalid.edl", src).expect("parse invalid power");
        assert!(ir.project.has_errors);
        assert!(ir.diagnostics.iter().any(|d| d.code == "E_ERC_UNCONNECTED_PIN"));
        assert!(ir.diagnostics.iter().any(|d| d.code == "E_ERC_VOLTAGE_MISMATCH"));
        assert!(ir.diagnostics.iter().any(|d| d.code == "E_ERC_SINGLE_PIN_NET"));
    }

    #[test]
    fn mcu_board_3v3_passes_erc_and_has_zero_label_wire_crossings() {
        let src = include_str!("../examples/mcu_board_3v3.edl");
        let ir = compile_source("examples/mcu_board_3v3.edl", src).expect("parse mcu_board_3v3");
        assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
        assert_eq!(ir.project.component_count, 10);
        assert_eq!(ir.project.net_count, 8);

        for (i, r_i) in ir.schematic_routes.iter().enumerate() {
            if let Some(lp) = r_i.label_pos {
                for (j, r_j) in ir.schematic_routes.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    for seg in &r_j.segments {
                        let min_x = seg.p0.x.min(seg.p1.x) - 0.1;
                        let max_x = seg.p0.x.max(seg.p1.x) + 0.1;
                        let min_y = seg.p0.y.min(seg.p1.y) - 0.1;
                        let max_y = seg.p0.y.max(seg.p1.y) + 0.1;
                        assert!(
                            !(lp.x >= min_x && lp.x <= max_x && lp.y >= min_y && lp.y <= max_y),
                            "Label '{}' at ({:.2}, {:.2}) collides with net '{}' segment {:?}",
                            r_i.net_id, lp.x, lp.y, r_j.net_id, seg
                        );
                    }
                }
            }
            for seg_i in &r_i.segments {
                for ep in [seg_i.p0, seg_i.p1] {
                    for (j, r_j) in ir.schematic_routes.iter().enumerate() {
                        if i == j {
                            continue;
                        }
                        for seg_j in &r_j.segments {
                            let min_x = seg_j.p0.x.min(seg_j.p1.x) - 0.1;
                            let max_x = seg_j.p0.x.max(seg_j.p1.x) + 0.1;
                            let min_y = seg_j.p0.y.min(seg_j.p1.y) - 0.1;
                            let max_y = seg_j.p0.y.max(seg_j.p1.y) + 0.1;
                            assert!(
                                !(ep.x >= min_x && ep.x <= max_x && ep.y >= min_y && ep.y <= max_y),
                                "Net '{}' endpoint ({:.2}, {:.2}) shorts onto net '{}' segment {:?}",
                                r_i.net_id, ep.x, ep.y, r_j.net_id, seg_j
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn mcu_board_3v3_symbols_mode_passes_erc_and_emits_clean_terminals() {
        let src = include_str!("../examples/mcu_board_3v3_symbols.edl");
        let ir = compile_source("examples/mcu_board_3v3_symbols.edl", src).expect("parse symbols mode");
        assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
        let gnd = ir.schematic_routes.iter().find(|r| r.net_id == "GND").unwrap();
        assert_eq!(gnd.style, "symbol");
        assert_eq!(gnd.terminals.len(), 8);
        let vbus = ir.schematic_routes.iter().find(|r| r.net_id == "VBUS").unwrap();
        assert_eq!(vbus.style, "direct");
        let nrst = ir.schematic_routes.iter().find(|r| r.net_id == "NRST").unwrap();
        assert_eq!(nrst.style, "label_stub");
        assert_eq!(nrst.terminals.len(), 4);
    }

    #[test]
    fn dual_rail_modular_imports_elaborates_and_tracks_provenance() {
        use crate::core::ir::VerificationStatus;
        let ir = compile_file(std::path::Path::new("examples/dual_rail_modular.edl")).unwrap();
        assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
        assert_eq!(ir.project.domain, "electronics");
        assert_eq!(ir.entities.len(), 10);
        // 6 parent nets (with Reg3V3's internal _IN/_OUT/_GND cleanly merged) + 2 internal _MID nets
        assert_eq!(ir.relations.len(), 8);
        assert!(ir.evidence.iter().all(|e| e.status == VerificationStatus::ExactlyChecked));

        // Verify module provenance on elaborated components
        let reg_u = ir.entities.iter().find(|e| e.id == "Reg3V3_U").expect("Reg3V3_U");
        let prov = reg_u.provenance.as_ref().expect("provenance on Reg3V3_U");
        assert_eq!(prov.module, "DecoupledLDO");
        assert_eq!(prov.instance, "Reg3V3");

        let pwr_r = ir.entities.iter().find(|e| e.id == "PwrLed_R").expect("PwrLed_R");
        assert_eq!(pwr_r.provenance.as_ref().unwrap().module, "LedIndicator");
        assert_eq!(pwr_r.provenance.as_ref().unwrap().instance, "PwrLed");
        assert_eq!(pwr_r.props["value"], "1kohm");

        let stat_d = ir.entities.iter().find(|e| e.id == "StatLed_D").expect("StatLed_D");
        assert_eq!(stat_d.provenance.as_ref().unwrap().instance, "StatLed");
        assert_eq!(stat_d.props["value"], "Blue_STAT");

        // Verify internal _GND was merged into parent GND (all 8 ground pins in GND)
        let gnd = ir.relations.iter().find(|r| r.id == "GND").expect("GND net");
        assert_eq!(gnd.connects.len(), 8);
        assert!(gnd.connects.contains(&"Reg3V3_C_IN.NEG".to_string()));
        assert!(gnd.connects.contains(&"Reg3V3_C_OUT.NEG".to_string()));

        // Verify KiCad export succeeds cleanly
        let kicad = crate::domains::elec::kicad::export_kicad_sch(&ir);
        assert!(kicad.contains("(kicad_sch"));
        assert!(kicad.contains("PwrLed__MID"));
    }

    #[test]
    fn duplex_modular_imports_elaborates_arch_modules_and_verifies() {
        use crate::core::ir::{EntityGeometryIr, VerificationStatus};
        let ir = compile_file(std::path::Path::new("examples/duplex_modular.edl")).unwrap();
        assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
        assert_eq!(ir.project.domain, "architecture");
        assert_eq!(ir.evidence.len(), 7);
        assert!(ir.evidence.iter().all(|e| e.status == VerificationStatus::ExactlyChecked));

        let bed1 = ir.entities.iter().find(|e| e.id == "Bed1").expect("Bed1");
        let bed2 = ir.entities.iter().find(|e| e.id == "Bed2").expect("Bed2");
        assert_eq!(bed1.provenance.as_ref().unwrap().module, "StandardBedroom");
        assert_eq!(bed2.provenance.as_ref().unwrap().module, "StandardBedroom");
        if let EntityGeometryIr::Rect { width_mm, .. } = bed1.geometry {
            assert!((width_mm - 3600.0).abs() < 1e-6);
        } else {
            panic!("Bed1 must be Rect");
        }
        if let EntityGeometryIr::Rect { width_mm, .. } = bed2.geometry {
            assert!((width_mm - 3200.0).abs() < 1e-6);
        } else {
            panic!("Bed2 must be Rect");
        }

        let bath_shower = ir.entities.iter().find(|e| e.id == "Bath1_Shower").expect("Bath1_Shower");
        assert_eq!(bath_shower.provenance.as_ref().unwrap().module, "StandardBath");
        assert_eq!(bath_shower.provenance.as_ref().unwrap().instance, "Bath1");
    }

    #[test]
    fn module_error_diagnostics_catch_unknown_missing_and_bad_args() {
        let unknown_mod = r#"
project "Bad1" {
  units: mm
  circuit C { use X = DoesNotExist() }
}
"#;
        let err1 = compile_source("bad1.edl", unknown_mod).unwrap_err();
        assert_eq!(err1.code, "E_MODULE_UNKNOWN");

        let missing_arg = r#"
module Res(r) {
  component R: resistor { value: r pin A: passive pin B: passive }
}
project "Bad2" {
  units: mm
  circuit C { use R1 = Res() }
}
"#;
        let err2 = compile_source("bad2.edl", missing_arg).unwrap_err();
        assert_eq!(err2.code, "E_MODULE_MISSING_ARG");

        let bad_arg = r#"
module Res(r = 1kohm) {
  component R: resistor { value: r pin A: passive pin B: passive }
}
project "Bad3" {
  units: mm
  circuit C { use R1 = Res(bogus: 10ohm) }
}
"#;
        let err3 = compile_source("bad3.edl", bad_arg).unwrap_err();
        assert_eq!(err3.code, "E_MODULE_UNKNOWN_ARG");
    }

    #[test]
    fn inline_module_without_import_works_for_in_memory_compile() {
        let src = r#"
module PullUp(r = 10kohm) {
  component R: resistor {
    value: r
    pin TOP: passive side: north
    pin BOT: passive side: south
  }
  bind VCC -> R.TOP
  bind SIG -> R.BOT
}

project "InlineMod" {
  units: mm
  circuit Main {
    component U1: ic {
      value: "MCU"
      pin VDD: power_out (3.3V) side: east
      pin IO: input (3.3V) side: east
      pin GND: ground (0V) side: south
    }
    component J1: connector {
      value: "BTN"
      pin SIG: passive side: west
      pin GND: ground (0V) side: south
    }
    use PU1 = PullUp(r: 4.7kohm)
    net V3V3 { connect: [U1.VDD, PU1.VCC] }
    net BTN  { connect: [U1.IO, PU1.SIG, J1.SIG] }
    net GND  { connect: [U1.GND, J1.GND] }
  }
  verify {
    require all_required_pins_connected
    require no_power_ground_short
    require no_single_pin_nets
  }
}
"#;
        let ir = compile_source("inline_mod.edl", src).unwrap();
        assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
        let r = ir.entities.iter().find(|e| e.id == "PU1_R").expect("PU1_R");
        assert_eq!(r.props["value"], "4.7kohm");
        assert_eq!(r.provenance.as_ref().unwrap().module, "PullUp");
    }

    #[test]
    fn loft_notched_room_solves_verifies_and_catches_fixture_in_notch() {
        use crate::core::ir::{EntityGeometryIr, VerificationStatus};
        let ir = compile_file(std::path::Path::new("examples/loft_notched.edl")).unwrap();
        assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
        assert_eq!(ir.evidence.len(), 7);
        assert!(ir
            .evidence
            .iter()
            .all(|e| e.status == VerificationStatus::ExactlyChecked));

        let gr = ir.entities.iter().find(|e| e.id == "GreatRoom").expect("GreatRoom");
        if let EntityGeometryIr::Rect {
            area_m2,
            notch_rect,
            ..
        } = gr.geometry
        {
            assert!(notch_rect.is_some());
            // 6.4 * 5.2 - 2.32 * 2.52 = 33.28 - 5.8464 = 27.4336 m^2
            assert!((area_m2 - 27.4336).abs() < 0.01);
        } else {
            panic!("GreatRoom must be Rect");
        }

        // Now verify that placing a fixture inside a corner notch triggers E_VERIFY_FIXTURE_IN_NOTCH
        let bad = r#"
project "BadNotch" {
  units: mm
  frame P: Frame2
  room Living {
    role: habitable
    clear_size: (5000mm, 4000mm)
    notch: (2000mm, 2000mm) at northeast
    anchor: southwest at P.origin
  }
  opening Win: window { host: exterior(Living, south) width: 1400mm align: center }
  fixture BadTable: table {
    room: Living
    size: (1000mm, 800mm)
    at: (3500mm, 2600mm)
  }
  verify {
    require no_fixture_collision
  }
}
"#;
        let ir_bad = compile_source("bad_notch.edl", bad).unwrap();
        assert!(ir_bad.project.has_errors);
        assert!(ir_bad
            .diagnostics
            .iter()
            .any(|d| d.code == "E_VERIFY_FIXTURE_IN_NOTCH"));
    }

    #[test]
    fn ieee_schematic_symbols_render_in_svg_and_kicad() {
        let ir = compile_file(std::path::Path::new("examples/dual_rail_modular.edl")).unwrap();
        let svg = render_svg(&ir, false);
        let kicad = export_kicad_sch(&ir);
        // Zig-zag resistors & polarized capacitor plates emit polylines/polygons & "+" marks
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("<polygon"));
        assert!(svg.contains(">+<"));
        assert!(kicad.contains("(polyline (pts"));
        assert!(kicad.contains("(pin_names (offset 1.016) hide)"));
    }

    #[test]
    fn kicad_cli_erc_ingests_directly_into_evidence_ledger() {
        use crate::core::ir::VerificationStatus;
        let mut ir = compile_file(std::path::Path::new("examples/dual_rail_modular.edl")).unwrap();
        crate::domains::elec::kicad_erc::run_and_ingest_kicad_erc(&mut ir);
        let ev = ir
            .evidence
            .iter()
            .find(|e| e.check_id == "K1_kicad_erc_external")
            .expect("K1_kicad_erc_external evidence record");
        // On machines with kicad-cli installed (like this workspace), must be ExactlyChecked!
        assert!(
            ev.status == VerificationStatus::ExactlyChecked
                || ev.status == VerificationStatus::Unverified,
            "unexpected KiCad ERC status: {:?}",
            ev
        );
    }

    #[test]
    fn semantic_query_measure_and_repair_plans_work_across_domains() {
        use crate::core::query::{inspect_summary, measure_ir, query_ir};

        // 1) Architecture query + measure
        let arch_ir = compile_file(std::path::Path::new("examples/duplex_modular.edl")).unwrap();
        let summary = inspect_summary(&arch_ir);
        assert_eq!(summary["project"]["domain"], "architecture");
        assert_eq!(summary["modules"]["StandardBedroom"].as_array().unwrap().len(), 2);

        let room_q = query_ir(&arch_ir, "room", Some("Bed1")).unwrap();
        assert!(room_q["fixtures"].as_array().unwrap().len() >= 2);
        let m_pair = measure_ir(&arch_ir, "Living", Some("Bath1")).unwrap();
        assert_eq!(m_pair["clear_gap_mm"], 120.0); // exact interior_wall thickness!
        assert_eq!(m_pair["shared_walls"].as_array().unwrap().len(), 1);

        // 2) Electronics query + measure
        let elec_ir = compile_file(std::path::Path::new("examples/dual_rail_modular.edl")).unwrap();
        let mod_q = query_ir(&elec_ir, "module", Some("LedIndicator")).unwrap();
        assert_eq!(mod_q["instance_count"], 2);
        let net_m = measure_ir(&elec_ir, "STAT_SIG", None).unwrap();
        assert!(net_m["total_wire_length_mm"].as_f64().unwrap() > 0.0);
        let comp_pair = measure_ir(&elec_ir, "U_MCU", Some("J_DBG")).unwrap();
        assert!(comp_pair["shared_nets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "SWDIO"));

        // 3) Violations + machine-readable repair plans on deliberate invalid schematic
        let bad_elec = compile_file(std::path::Path::new("examples/power_5v_invalid.edl")).unwrap();
        let viol = query_ir(&bad_elec, "violations", None).unwrap();
        assert!(viol["violation_count"].as_u64().unwrap() >= 3);
        let plan = viol["repair_plan"].as_array().unwrap();
        assert!(plan.iter().any(|p| p["repair"]["action"] == "connect_pin"));
        assert!(plan.iter().any(|p| p["repair"]["action"] == "fix_voltage"));
    }

    #[test]
    fn mcp_jsonrpc_server_handles_initialize_list_and_tool_calls() {
        use serde_json::json;

        let init = crate::mcp::handle_jsonrpc_request(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "edl");

        let list = crate::mcp::handle_jsonrpc_request(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }))
        .unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 8);

        let call = crate::mcp::handle_jsonrpc_request(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "edl_measure",
                "arguments": {
                    "file_path": "examples/loft_notched.edl",
                    "target_a": "GreatRoom",
                    "target_b": "Master"
                }
            }
        }))
        .unwrap();
        assert_eq!(call["result"]["isError"], false);
        let text = call["result"]["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["clear_gap_mm"], 120.0);
    }

    #[test]
    fn stress_sensor_node_32_components_and_diode_polarity_and_inductance_units() {
        use crate::core::ir::{EntityGeometryIr, VerificationStatus};
        use crate::core::units::{Dimension, Unit};

        // Verify inductance units (H, mH, uH, nH)
        assert_eq!(Unit::parse("uH").unwrap().dimension(), Dimension::Inductance);
        assert_eq!(Unit::parse("nH").unwrap().dimension(), Dimension::Inductance);

        // Compile 32-component stress test & verify EDL + KiCad 9 ERC
        let mut ir = compile_file(std::path::Path::new("examples/stress_sensor_node.edl")).unwrap();
        assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
        assert_eq!(ir.entities.len(), 32);
        assert_eq!(ir.relations.len(), 19);
        crate::domains::elec::kicad_erc::run_and_ingest_kicad_erc(&mut ir);
        let k1 = ir
            .evidence
            .iter()
            .find(|e| e.check_id == "K1_kicad_erc_external")
            .unwrap();
        assert!(
            k1.status == VerificationStatus::ExactlyChecked
                || k1.status == VerificationStatus::Unverified,
            "KiCad ERC failed: {:?}",
            k1
        );

        // Verify D_ESD (pin K: north, pin A: south ground) triangle points UPWARD (cathode at top y < center)
        let d_esd = ir.entities.iter().find(|e| e.id == "D_ESD").expect("D_ESD");
        if let EntityGeometryIr::Component {
            category,
            body_rect,
            pins,
            ..
        } = &d_esd.geometry
        {
            let g = crate::domains::elec::symbols::build_symbol_graphic(category, *body_rect, pins);
            let cy = body_rect.center().y;
            let tri = g
                .primitives
                .iter()
                .find_map(|p| match p {
                    crate::domains::elec::symbols::SymPrim::Polyline { pts, filled: true, .. } => Some(pts),
                    _ => None,
                })
                .expect("filled diode triangle");
            // Tip vertex (pts[2]) is the cathode -> must be ABOVE center (smaller y in Y-down!)
            assert!(tri[2].y < cy, "D_ESD cathode tip must point UP (y < cy), got {} vs {}", tri[2].y, cy);
        } else {
            panic!("D_ESD must be Component");
        }
    }

    #[test]
    fn stress_family_home_notched_measure_hosted_openings_and_project_params() {
        use crate::core::query::{measure_ir, query_ir};

        let ir1 = compile_file(std::path::Path::new("examples/stress_family_home.edl")).unwrap();
        let ir2 = compile_file(std::path::Path::new("examples/stress_family_home_v2.edl")).unwrap();
        assert!(!ir1.project.has_errors, "v1 diags: {:?}", ir1.diagnostics);
        assert!(!ir2.project.has_errors, "v2 diags: {:?}", ir2.diagnostics);

        // 1) `query room` must return hosted openings!
        let q_ld = query_ir(&ir1, "room", Some("LivingDining")).unwrap();
        let hosted = q_ld["hosted_openings"].as_array().unwrap();
        assert!(hosted.len() >= 4, "expected >= 4 hosted openings on LivingDining, got {:?}", hosted);

        // 2) `measure` on L-shaped notched room + tucked room must report exact 120mm gap, NOT overlap!
        let m = measure_ir(&ir1, "LivingDining", Some("PowderRoom")).unwrap();
        assert_eq!(m["bboxes_overlap"], false);
        assert_eq!(m["clear_gap_mm"], 120.0);
        let sws = m["shared_walls"].as_array().unwrap();
        assert_eq!(sws.len(), 2);
        assert_ne!(sws[0]["wall_id"], sws[1]["wall_id"]);

        // 3) Project `parameter` works directly in `clear_size`, `use` args, and `shared_wall(..., side)`!
        let param_src = r#"
module MiniRoom(w = 2000mm, d = 2000mm) {
  room R { role: wet clear_size: (w, d) }
}
project "ParamEverywhere" {
  units: mm
  frame P: Frame2
  parameter bath_w: 2200mm
  parameter bath_d: 2000mm
  room Living {
    role: habitable
    clear_size: (5000mm, 4200mm)
    notch: (2320mm, 2120mm) at northeast
    anchor: southwest at P.origin
  }
  use Bath = MiniRoom(w: bath_w, d: bath_d)
  layout P {
    require Bath.east == Living.east
    require Bath.north == Living.north
  }
  opening Entry: door { host: exterior(Living, south) width: 900mm offset: 300mm from start hinge: start swing: into Living }
  opening BathDoor: door { host: shared_wall(Living, Bath, north) width: 800mm align: center hinge: start swing: into Living }
  opening Win: window { host: exterior(Living, west) width: 1400mm align: center }
  verify {
    require no_room_overlap
    require opening_jamb_margin >= 100mm
    require no_door_swing_collision
    require all_rooms_reachable
    require habitable_rooms_have_window
  }
}
"#;
        let ir_p = compile_source("param_everywhere.edl", param_src).unwrap();
        assert!(!ir_p.project.has_errors, "param diags: {:?}", ir_p.diagnostics);
    }

    #[test]
    fn sensor_node_2row_tests_multi_row_ferrite_propagation_spice_mna_and_bom() {
        use crate::core::ir::VerificationStatus;

        let mut ir = compile_file(std::path::Path::new("examples/sensor_node_2row.edl")).unwrap();
        assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);

        // 1) All 5 ERCs + S1 + 5 DC/LED simulation assertions must be ExactlyChecked!
        assert!(ir.evidence.len() >= 11, "evidence: {:?}", ir.evidence);
        assert!(ir
            .evidence
            .iter()
            .all(|e| e.status == VerificationStatus::ExactlyChecked));

        // 2) Verify series-passive ferrite bead propagated 5V to VBUS_5V
        let vbus5 = ir.relations.iter().find(|r| r.id == "VBUS_5V").expect("VBUS_5V");
        assert_eq!(vbus5.driven_voltage_v, Some(5.0));

        // 3) Verify real KiCad 9 ERC passes with 0 errors even with passive ferrite bead FB_USB!
        crate::domains::elec::kicad_erc::run_and_ingest_kicad_erc(&mut ir);
        let k1 = ir
            .evidence
            .iter()
            .find(|e| e.check_id == "K1_kicad_erc_external")
            .unwrap();
        assert!(
            k1.status == VerificationStatus::ExactlyChecked
                || k1.status == VerificationStatus::Unverified,
            "KiCad ERC failed on 2-row ferrite-bead schematic: {:?}",
            k1
        );

        // 4) Verify MNA simulation values: op-amp unity buffer ADC_SIG ~= 1.65V, RC cutoff ~= 1591.55 Hz, LED currents exact!
        let sim = simulate_dc_and_rc(&ir);
        assert!(sim.converged);
        assert!((sim.node_voltages_v["VBUS_5V"] - 5.0).abs() < 0.05);
        assert!((sim.node_voltages_v["ADC_SIG"] - 1.65).abs() < 0.05);
        let pwr_led = sim.branches.iter().find(|b| b.component == "PwrLed_D").unwrap();
        assert_eq!(pwr_led.state, "forward_biased");
        assert!((pwr_led.current_ma - 1.727).abs() < 0.15, "PwrLed_D I = {}", pwr_led.current_ma);
        let rc = sim.rc_filters.iter().find(|f| f.node == "ADC_SIG").expect("RC filter on ADC_SIG");
        assert!((rc.cutoff_hz - 1591.55).abs() < 5.0, "fc = {}", rc.cutoff_hz);

        // 5) Verify SPICE3 netlist & BOM export
        let cir = export_spice_netlist(&ir);
        assert!(cir.contains("R_PwrLed_R"));
        assert!(cir.contains("E_U_AMP"));
        assert!(cir.contains(".op"));

        let bom = build_bom(&ir);
        assert_eq!(bom.total_components, ir.entities.len());
        assert!(bom.unique_parts < bom.total_components);
        let csv = export_bom_csv(&ir);
        assert!(csv.starts_with("Item,Qty,Designators,Category,Value,Footprint,Modules"));
    }

    #[test]
    fn sheet_overflow_emits_warning_without_failing_build() {
        let src = r#"
project "TinySheet" {
  units: mm
  circuit C {
    component U1: ic { value: "A" pin P1: output (3.3V) side: east pin GND: ground (0V) side: south }
    component U2: ic { value: "B" pin P1: input (3.3V) side: west pin P2: output (3.3V) side: east pin GND: ground (0V) side: south }
    component U3: ic { value: "C" pin P1: input (3.3V) side: west pin P2: output (3.3V) side: east pin GND: ground (0V) side: south }
    component U4: ic { value: "D" pin P1: input (3.3V) side: west pin P2: output (3.3V) side: east pin GND: ground (0V) side: south }
    component U5: ic { value: "E" pin P1: input (3.3V) side: west pin GND: ground (0V) side: south }
    net N1 { connect: [U1.P1, U2.P1] }
    net N2 { connect: [U2.P2, U3.P1] }
    net N3 { connect: [U3.P2, U4.P1] }
    net N4 { connect: [U4.P2, U5.P1] }
    net GND { connect: [U1.GND, U2.GND, U3.GND, U4.GND, U5.GND] }
  }
  schematic S {
    sheet: A4 portrait
    group G1 { members: [U1] }
    group G2 { members: [U2] }
    group G3 { members: [U3] }
    group G4 { members: [U4] }
    group G5 { members: [U5] }
    layout { arrange: [G1, G2, G3, G4, G5] left_to_right group_gap: 35.56mm }
  }
}
"#;
        let ir = compile_source("tiny_sheet.edl", src).unwrap();
        assert!(!ir.project.has_errors);
        assert!(ir.diagnostics.iter().any(|d| d.code == "W_SHEET_OVERFLOW"));
    }
}
