pub mod arch;
pub mod ast;
pub mod diff;
pub mod elec;
pub mod export_dxf;
pub mod export_kicad;
pub mod geom;
pub mod ir;
pub mod parser;
pub mod render_svg;
pub mod solver;
pub mod span;
pub mod units;

use ir::CanonicalIr;
use span::Diagnostic;

pub fn compile_source(file: &str, src: &str) -> Result<CanonicalIr, Diagnostic> {
    let ast = parser::parse_project(file, src)?;
    if !ast.circuits.is_empty() && ast.rooms.is_empty() {
        Ok(elec::compile_electronics(&ast))
    } else {
        Ok(arch::compile_project(&ast))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::VerificationStatus;

    #[test]
    fn studio_a_solves_and_verifies_cleanly() {
        let src = include_str!("../examples/studio_a.edl");
        let ir = compile_source("examples/studio_a.edl", src).expect("parse studio_a");
        assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
        assert_eq!(ir.project.room_count, 3);
        assert_eq!(ir.project.opening_count, 4);
        assert_eq!(ir.project.exterior_footprint_mm, Some([8020.0, 4400.0]));
        assert!(ir.project.remaining_dof.is_empty());

        // Check derived Bath geometry: (4620, 2520) -> (7620, 4000) => 3000 x 1480
        let bath = ir.entities.iter().find(|e| e.id == "Bath").unwrap();
        match bath.geometry {
            ir::EntityGeometryIr::Rect {
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

        // Check SVG & DXF contain analytical arcs
        let svg = render_svg::render_svg(&ir, true);
        assert!(svg.contains("<path id=\"door-arc-D_Entry\""));
        assert!(svg.contains(" A "));
        let dxf = export_dxf::export_dxf(&ir);
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
        // Hall 1500 + 120 + Living 4200 = 5820 (+ 400 ext = 6220)
        // Hall/Living 3600 + 120 + Bedroom 3000 = 6720 (+ 400 ext = 7120)
        assert_eq!(ir.project.exterior_footprint_mm, Some([6220.0, 7120.0]));

        let bath = ir.entities.iter().find(|e| e.id == "Bath").unwrap();
        match bath.geometry {
            ir::EntityGeometryIr::Rect {
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
        assert!(ir
            .diagnostics
            .iter()
            .any(|d| d.code == "E_VERIFY_JAMB_MARGIN"));
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
            // Bed placed directly in D_Bed1's swing arc!
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
        assert_eq!(ir.evidence.len(), 7); // T2 + V1..V6
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

        let svg = render_svg::render_svg(&ir, true);
        assert!(svg.contains("EDL Schematic: Power_Regulator_5V"));
        assert!(svg.contains("LM7805"));

        let kicad = export_kicad::export_kicad_sch(&ir);
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

        // Verify no net label sits on any segment of another net (prevents KiCad label shorts!)
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
                            r_i.net_id,
                            lp.x,
                            lp.y,
                            r_j.net_id,
                            seg
                        );
                    }
                }
            }
            // Also verify no segment endpoint of net i touches any segment of net j (prevents KiCad T-shorts!)
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
                                r_i.net_id,
                                ep.x,
                                ep.y,
                                r_j.net_id,
                                seg_j
                            );
                        }
                    }
                }
            }
        }
    }
}
