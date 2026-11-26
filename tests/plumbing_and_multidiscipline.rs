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

#[test]
fn commercial_restroom_collector_headers_dfu_vents_and_cleanouts_verify_cleanly() {
    let src = std::fs::read_to_string("examples/commercial_restroom_plumbing.edl").unwrap();
    let ir = compile_source("examples/commercial_restroom_plumbing.edl", &src).unwrap();
    assert!(
        !ir.project.has_errors,
        "unexpected commercial plumbing errors: {:?}",
        ir.diagnostics
    );
    let p7 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "P7_dfu_header_capacity")
        .expect("P7 DFU evidence");
    assert_eq!(p7.status, VerificationStatus::ExactlyChecked);
    assert!(p7.details.contains("45 DFU"));

    let svg = render_svg_with_layers(&ir, false, None);
    assert!(svg.contains("24 DFU"));
    assert!(svg.contains("COP"));
    assert!(svg.contains("Vent Ø50"));

    // Negative DFU overload test: shrink SoilHeader_Center to 50mm (max 12 DFU < 24 DFU!)
    let bad = src.replace(
        "header SoilHeader_Center: soil_header  { from: COP_Soil_M   to: SoilStack_M  dia: 110mm  slope: \"1:100\" }",
        "header SoilHeader_Center: soil_header  { from: COP_Soil_M   to: SoilStack_M  dia: 50mm   slope: \"1:100\" }",
    );
    let bad_ir = compile_source("bad_dfu.edl", &bad).unwrap();
    assert!(bad_ir.project.has_errors);
    assert!(bad_ir
        .diagnostics
        .iter()
        .any(|d| d.code == "E_PLUMB_HEADER_DFU_OVERLOAD"));
}

#[test]
fn v33_overlay_inherits_arch_deduplicates_verify_and_enforces_p8_p9_m7_m8() {
    let dir = std::env::temp_dir().join("edl_v33_overlay_test");
    let _ = std::fs::create_dir_all(&dir);
    let base_path = dir.join("base_arch.edl");
    let plumb_path = dir.join("overlay_plumb.edl");
    let elec_path = dir.join("overlay_elec.edl");

    std::fs::write(
        &base_path,
        r#"
project "OverlayBase" {
    units: mm
    frame Plan: Frame2
    architecture.defaults {
        measurement_basis: clear_interior
        exterior_wall: 200mm
        interior_wall: 120mm
    }
    room ElecITRoom { role: service  clear_size: (4000mm, 3600mm)  anchor: southwest at Plan.origin }
    room Bath       { role: wet      clear_size: (3600mm, 3600mm) }
    layout Plan {
        require Bath.west == ElecITRoom.east + interior_wall
        require Bath.south == ElecITRoom.south
    }
    opening MainDoor: door { host: exterior(ElecITRoom, south)  width: 900mm  offset: 1800mm from start  hinge: start  swing: into ElecITRoom }
    opening BathDoor: door { host: shared_wall(ElecITRoom, Bath) width: 900mm offset: 1500mm from start  hinge: start  swing: into Bath }
    fixture WC1: toilet    { room: Bath  against: north  offset: 600mm from start  size: (400mm, 650mm) }
    fixture Basin1: vanity { room: Bath  against: north  offset: 1600mm from start size: (550mm, 450mm) }
    verify {
        require no_room_overlap
        require all_rooms_reachable
    }
}
"#,
    )
    .unwrap();

    // 1. Plumbing overlay with a dry-room pipe clash (P8) -> must catch E_PLUMB_DRY_ROOM_CLASH
    std::fs::write(
        &plumb_path,
        r#"
project "OverlayPlumb" {
    units: mm
    overlay "base_arch.edl"
    building_plumbing MainPlumb {
        riser ColdMain: cold_riser { host: Bath.east  at: 400mm  dia: 25mm }
        riser SoilStack: soil_stack { host: Bath.east  at: 900mm  dia: 110mm }
        heater WH1: water_heater { host: ElecITRoom.north  at: 800mm  capacity_l: 100 }
        valve V1: isolation_valve { host: Bath.east  at: 500mm  serves: ColdMain  dia: 25mm }
        connect WC1    { cold: ColdMain  cold_dia: 15mm  soil: SoilStack  drain_dia: 110mm  slope: "1:60" }
        connect Basin1 { cold: ColdMain  cold_dia: 15mm  hot: WH1  hot_dia: 15mm  waste: SoilStack  drain_dia: 50mm  slope: "1:40" }
    }
    verify {
        require no_room_overlap
    }
}
"#,
    )
    .unwrap();
    let pir = edl::compile_file(&plumb_path).expect("plumbing overlay parses");
    assert_eq!(pir.project.room_count, 2);
    // Deduplicated verify rules: V1_no_room_overlap should appear exactly ONCE
    assert_eq!(
        pir.evidence.iter().filter(|e| e.check_id == "V1_no_room_overlap").count(),
        1
    );
    assert!(
        pir.diagnostics.iter().any(|d| d.code == "E_PLUMB_DRY_ROOM_CLASH"),
        "expected P8 dry room clash in ElecITRoom, got: {:?}",
        pir.diagnostics
    );
    let psvg = render_svg_with_layers(&pir, false, None);
    assert!(psvg.contains("NO PLUMBING ZONE"));

    // 2. Electrical overlay testing 3-phase balance (M7), undersized cable (M1), and DB clearance (M8)
    std::fs::write(
        &elec_path,
        r#"
project "OverlayElec" {
    units: mm
    overlay "base_arch.edl"
    building_electrical MainElec {
        supply: 230V 50Hz
        ceiling_height: 2700mm
        panel DB1 { host: wall(ElecITRoom, north)  offset: 600mm from start  main_breaker: 63A  height: 1500mm }
        panel DB2 { host: wall(ElecITRoom, north)  offset: 2600mm from start main_breaker: 40A  height: 1500mm }
        breaker C1: socket   { from: DB1  rating: 20A  phase: L1  cable: 1.5mm2  rcd: 30mA  max_voltage_drop: 5% }
        breaker C2: socket   { from: DB1  rating: 20A  phase: L1  cable: 2.5mm2  rcd: 30mA  max_voltage_drop: 5% }
        breaker C3: lighting { from: DB1  rating: 10A  phase: L2  cable: 1.5mm2  rcd: 30mA  max_voltage_drop: 3% }
        light L1: ceiling { room: ElecITRoom  at: center  load: 30W  circuit: C3 }
        light L2: ceiling { room: Bath        at: center  load: 30W  circuit: C3  ip: IP44 }
        outlet S1: duplex { host: wall(ElecITRoom, west)  offset: 1200mm from start  height: 300mm  load: 3200W  circuit: C1 }
        outlet S2: duplex { host: wall(Bath, east)        offset: 1200mm from start  height: 300mm  load: 3200W  circuit: C2 }
    }
    verify {
        require breaker_load_ok
        require voltage_drop_ok
        require electrical_device_clear
    }
}
"#,
    )
    .unwrap();
    let eir = edl::compile_file(&elec_path).expect("elec overlay parses");
    assert!(
        eir.diagnostics.iter().any(|d| d.code == "E_MEP_CABLE_UNDERSIZED"),
        "expected 20A on 1.5mm2 to fail ampacity check, got: {:?}",
        eir.diagnostics
    );
    assert!(
        eir.diagnostics.iter().any(|d| d.code == "E_MEP_PHASE_IMBALANCE"),
        "expected heavy L1 vs empty L3 to fail M7 3-phase balance, got: {:?}",
        eir.diagnostics
    );
    assert!(eir.evidence.iter().any(|e| e.check_id == "M8_panel_working_clearance"));
    let esvg = render_svg_with_layers(&eir, false, None);
    assert!(esvg.contains("id=\"mep-electrical\""));
    assert!(esvg.contains("DB1"));
    assert!(esvg.contains("db-clr-hatch"));

    let _ = std::fs::remove_dir_all(&dir);
}
