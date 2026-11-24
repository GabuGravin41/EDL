pub mod core;
pub mod domains;
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
}
