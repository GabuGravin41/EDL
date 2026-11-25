use edl::core::ir::VerificationStatus;
use edl::{compile_source, export_dxf, render_svg};

#[test]
fn complex_stepped_villa_grid_solves_15_rooms_and_variable_wall_thicknesses() {
    let src = std::fs::read_to_string("examples/complex_villa_grid_stepped.edl").unwrap();
    let ir = compile_source("examples/complex_villa_grid_stepped.edl", &src).unwrap();

    assert!(
        !ir.project.has_errors,
        "unexpected villa errors: {:?}",
        ir.diagnostics
    );
    assert_eq!(ir.project.room_count, 15);
    assert_eq!(ir.project.remaining_dof.len(), 0);
    assert!(ir.derived_walls.iter().any(|w| (w.thickness_mm - 380.0).abs() < 1.0));
    assert!(ir.derived_walls.iter().any(|w| (w.thickness_mm - 120.0).abs() < 1.0));

    let svg = render_svg(&ir, false);
    assert!(svg.contains("id=\"structural-grid\""));
    assert!(svg.contains("-0.030"));
    assert!(svg.contains("±0.000"));

    let dxf = export_dxf(&ir);
    assert!(dxf.contains("GRID_AXES"));
    assert!(dxf.contains("GRID_BUBBLES"));
    assert!(dxf.contains("COLUMNS"));
}

#[test]
fn complex_multiunit_apartment_solves_27_rooms_with_dual_cores_and_22_columns() {
    let src = std::fs::read_to_string("examples/complex_apartment_multiunit.edl").unwrap();
    let ir = compile_source("examples/complex_apartment_multiunit.edl", &src).unwrap();

    assert!(
        !ir.project.has_errors,
        "unexpected apartment errors: {:?}",
        ir.diagnostics
    );
    assert_eq!(ir.project.room_count, 27);
    assert_eq!(ir.project.remaining_dof.len(), 0);
    for rule in [
        "T2_linear_layout_solve",
        "V1_no_room_overlap",
        "V2_opening_jamb_margin",
        "V3_no_door_swing_collision",
        "V4_all_rooms_reachable",
        "V5_no_fixture_collision",
        "V6_habitable_rooms_have_window",
        "V7_structural_column_clearance",
    ] {
        let ev = ir.evidence.iter().find(|e| e.check_id == rule).unwrap();
        assert_eq!(ev.status, VerificationStatus::ExactlyChecked, "rule {rule}");
    }

    let svg = render_svg(&ir, false);
    assert!(svg.contains("stair-tread"));
    assert!(svg.contains("elev-x"));
}

#[test]
fn complex_warehouse_48x24m_verifies_36_steel_columns_and_catches_column_clash() {
    let src = std::fs::read_to_string("examples/complex_warehouse_structural.edl").unwrap();
    let ir = compile_source("examples/complex_warehouse_structural.edl", &src).unwrap();

    assert!(
        !ir.project.has_errors,
        "unexpected warehouse errors: {:?}",
        ir.diagnostics
    );
    assert_eq!(ir.project.room_count, 9);
    let col_count = ir.entities.iter().filter(|e| e.kind == "arch.column").count();
    assert_eq!(col_count, 36);

    let svg = render_svg(&ir, false);
    assert!(svg.contains("col-steel"));
    assert!(svg.contains("1161.83"));

    // Negative test: push WinDispatch right into steel column H4 so V7 catches the structural clash!
    let bad = src.replace(
        "opening WinDispatch: window { host: exterior(DispatchOffice, north)  width: 3200mm  align: center }",
        "opening WinDispatch: window { host: exterior(DispatchOffice, north)  width: 3200mm  offset: 20mm from end }",
    );
    let bad_ir = compile_source("bad_col.edl", &bad).unwrap();
    assert!(bad_ir.project.has_errors);
    assert!(bad_ir
        .diagnostics
        .iter()
        .any(|d| d.code == "E_COLUMN_OPENING_CLASH"));
}
