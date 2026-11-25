use edl::core::ir::VerificationStatus;
use edl::{
    compile_source, export_dxf, format_plumbing_schedule, render_svg_with_layers,
};

#[test]
fn villa_multidiscipline_verifies_all_19_rules_and_layer_selectable_rendering() {
    let src = std::fs::read_to_string("examples/villa_3br_arch_elec_plumbing.edl")
        .expect("villa benchmark exists");
    let ir = compile_source("examples/villa_3br_arch_elec_plumbing.edl", &src)
        .expect("compiles cleanly");

    assert!(
        !ir.project.has_errors,
        "unexpected multi-discipline errors: {:?}",
        ir.diagnostics
    );
    assert_eq!(ir.project.domain, "arch+mep.elec+mep.plumbing");
    assert_eq!(ir.project.room_count, 8);
    assert_eq!(ir.project.remaining_dof.len(), 0);
    assert!(
        ir.evidence.len() >= 19,
        "expected >= 19 evidence rules (7 arch + 6 elec + 6 plumbing), got {}",
        ir.evidence.len()
    );
    for rule in [
        "T2_linear_layout_solve",
        "V1_no_room_overlap",
        "V5_no_fixture_collision",
        "M1_breaker_load_capacity",
        "M2_voltage_drop_limit",
        "M6_wet_circuit_rcd",
        "P1_wet_fixture_supply",
        "P2_drainage_sizing_and_slope",
        "P3_trap_arm_distance",
        "P4_supply_velocity_and_sizing",
        "P5_isolation_valves",
        "P6_cross_discipline_clearance",
    ] {
        let ev = ir
            .evidence
            .iter()
            .find(|e| e.check_id == rule)
            .unwrap_or_else(|| panic!("missing evidence rule {rule}"));
        assert_eq!(
            ev.status,
            VerificationStatus::ExactlyChecked,
            "rule {rule} was not ExactlyChecked: {}",
            ev.details
        );
    }

    // Layer-selectable SVG rendering from ONE source of truth
    let arch_only = render_svg_with_layers(&ir, false, Some(&["arch".to_string()]));
    assert!(!arch_only.contains("id=\"mep-electrical\""));
    assert!(!arch_only.contains("id=\"overlay-mep.plumbing\""));

    let arch_elec = render_svg_with_layers(
        &ir,
        false,
        Some(&["arch".to_string(), "elec".to_string()]),
    );
    assert!(arch_elec.contains("id=\"mep-electrical\""));
    assert!(arch_elec.contains("id=\"electrical-legend\""));
    assert!(!arch_elec.contains("id=\"overlay-mep.plumbing\""));

    let arch_plumb = render_svg_with_layers(
        &ir,
        false,
        Some(&["arch".to_string(), "plumbing".to_string()]),
    );
    assert!(!arch_plumb.contains("id=\"mep-electrical\""));
    assert!(arch_plumb.contains("id=\"overlay-mep.plumbing\""));
    assert!(arch_plumb.contains("id=\"plumbing-legend\""));

    let all_mep = render_svg_with_layers(&ir, false, None);
    assert!(all_mep.contains("id=\"mep-electrical\""));
    assert!(all_mep.contains("id=\"overlay-mep.plumbing\""));

    // 19-layer AutoCAD R12 DXF contains architecture + electrical + plumbing layers
    let dxf = export_dxf(&ir);
    for layer in [
        "WALLS_EXT",
        "ELEC_PANEL",
        "ELEC_LIGHTING",
        "PLUMB_COLD",
        "PLUMB_HOT",
        "PLUMB_DRAIN",
        "PLUMB_EQUIP",
    ] {
        assert!(dxf.contains(layer), "missing DXF layer {layer}");
    }

    // Plumbing takeoff schedule CSV
    let csv = format_plumbing_schedule(&ir, true);
    assert!(csv.contains("COLD,15,"));
    assert!(csv.contains("HOT,15,"));
    assert!(csv.contains("SOIL,110,"));
    assert!(csv.contains("WASTE,50,"));
}

#[test]
fn plumbing_verifier_catches_missing_supply_undersized_drain_and_missing_valve() {
    let src = std::fs::read_to_string("examples/villa_3br_arch_elec_plumbing.edl").unwrap();

    // 1. Missing wet fixture connection (P1)
    let no_sink = src.replace(
        "connect Kitchen_Sink   { cold: ColdMain  cold_dia: 15mm  hot: WH1  hot_dia: 15mm  waste: SoilKitchen  drain_dia: 50mm  slope: \"1:40\" }",
        "",
    );
    let ir1 = compile_source("no_sink.edl", &no_sink).unwrap();
    assert!(ir1.project.has_errors);
    assert!(ir1.diagnostics.iter().any(|d| d.code == "E_PLUMB_MISSING_SUPPLY"));

    // 2. Undersized WC soil pipe + flat slope (P2)
    let bad_soil = src.replace(
        "soil: SoilShared   drain_dia: 110mm  slope: \"1:60\"",
        "soil: SoilShared   drain_dia: 50mm   slope: \"1:200\"",
    );
    let ir2 = compile_source("bad_soil.edl", &bad_soil).unwrap();
    assert!(ir2.project.has_errors);
    assert!(ir2.diagnostics.iter().any(|d| d.code == "E_PLUMB_UNDERSIZED_DRAIN"));
    assert!(ir2.diagnostics.iter().any(|d| d.code == "E_PLUMB_INVALID_SLOPE"));

    // 3. Missing main cold isolation valve (P5)
    let no_valve = src.replace(
        "valve V_Main: isolation_valve { system: cold  host: Utility.west  at: 2950mm  serves: \"Main Cold Supply\" }",
        "",
    );
    let ir3 = compile_source("no_valve.edl", &no_valve).unwrap();
    assert!(ir3.project.has_errors);
    assert!(ir3.diagnostics.iter().any(|d| d.code == "E_PLUMB_MISSING_VALVE"));
}

#[test]
fn cross_discipline_clash_check_catches_plumbing_riser_colliding_with_db_panel() {
    let src = std::fs::read_to_string("examples/villa_3br_arch_elec_plumbing.edl").unwrap();
    // Move ColdMain right onto Utility.east at 1800mm — directly on top of electrical panel DB1!
    let clash_src = src.replace(
        "riser ColdMain: cold_water { host: Utility.west  at: 2600mm  dia: 25mm }",
        "riser ColdMain: cold_water { host: Utility.east  at: 1800mm  dia: 25mm }",
    );
    let ir = compile_source("clash.edl", &clash_src).unwrap();

    assert!(ir.project.has_errors, "must catch plumbing<->DB1 clash!");
    assert!(
        ir.diagnostics
            .iter()
            .any(|d| d.code == "E_PLUMB_ELEC_CLASH"),
        "expected E_PLUMB_ELEC_CLASH, got: {:?}",
        ir.diagnostics
    );
    let p6 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "P6_cross_discipline_clearance")
        .unwrap();
    assert_eq!(p6.status, VerificationStatus::Violated);
}

#[test]
fn commercial_office_dual_panel_electrical_level01_passes_all_14_rules() {
    let src = std::fs::read_to_string("examples/commercial_office_electrical_level01.edl").unwrap();
    let ir = compile_source("examples/commercial_office_electrical_level01.edl", &src).unwrap();
    assert!(
        !ir.project.has_errors,
        "unexpected commercial electrical errors: {:?}",
        ir.diagnostics
    );
    assert_eq!(ir.project.room_count, 12);
    assert_eq!(ir.mep_circuits.len(), 8);
    assert!(ir.evidence.len() >= 14);
    let svg = render_svg_with_layers(&ir, false, None);
    assert!(svg.contains("PANEL SCHEDULE — LP1"));
    assert!(svg.contains("PANEL SCHEDULE — PP1"));
    assert!(svg.contains("EXIT"));
}
