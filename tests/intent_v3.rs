use edl::core::ir::{EntityGeometryIr, VerificationStatus};
use edl::domains::elec::kicad_import::compare_kicad_netlists;
use edl::domains::elec::kicad_lift::lift_kicad_project;
use std::fs;
use std::path::Path;

#[test]
fn v3_showcase_sensor_node_idioms_and_relative_layout_pass_real_kicad_erc() {
    if !Path::new("/usr/share/kicad/symbols/Device.kicad_sym").exists() {
        return; // Skip gracefully on CI machines without KiCad symbol libraries installed
    }
    let path = Path::new("examples/sensor_node_v3_intent.edl");
    let mut ir = edl::compile_file(path).expect("sensor_node_v3_intent.edl should compile cleanly");
    assert_eq!(ir.project.component_count, 22, "expected 5 explicit + 17 idiom-expanded passives");
    assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);

    // Verify `align_y J1.Pin_1 == U1.VI` produced exact horizontal pin alignment!
    let pin_y = |cid: &str, pname: &str| -> f64 {
        let ent = ir.entities.iter().find(|e| e.id == cid).unwrap();
        let EntityGeometryIr::Component { pins, .. } = &ent.geometry else {
            panic!("not a component");
        };
        pins.iter()
            .find(|p| p.name == pname || p.pin_code.as_deref() == Some(pname))
            .unwrap()
            .tip_point
            .y
    };
    assert!(
        (pin_y("J1", "Pin_1") - pin_y("U1", "VI")).abs() < 0.01,
        "J1.Pin_1 and U1.VI must share exact Y coordinate via align_y"
    );

    // Run real KiCad 9 CLI ERC and assert 0 error-level violations!
    edl::domains::elec::kicad_erc::run_and_ingest_kicad_erc(&mut ir);
    if let Some(ev) = ir.evidence.iter().find(|e| e.check_id == "K1_kicad_erc_external") {
        assert!(
            ev.status == VerificationStatus::ExactlyChecked
                || ev.status == VerificationStatus::Unverified,
            "KiCad 9 ERC failed: {:?}",
            ir.diagnostics
        );
    }
}

#[test]
fn v3_relative_offset_polar_array_and_cycle_detection() {
    let src = r#"
project "RelGeomTest" {
    units: mm
    frame Sheet: Frame2
    circuit Test {
        component R1: resistor { value: "1kohm" pin A: passive side: west pin B: passive side: east }
        component R2: resistor { value: "2kohm" pin A: passive side: west pin B: passive side: east }
        component R3: resistor { value: "3kohm" pin A: passive side: west pin B: passive side: east }
        component R4: resistor { value: "4kohm" pin A: passive side: west pin B: passive side: east }
        component R5: resistor { value: "5kohm" pin A: passive side: west pin B: passive side: east }
        net N1 { connect: [R1.A, R2.A, R3.A, R4.A, R5.A] }
        net GND { connect: [R1.B, R2.B, R3.B, R4.B, R5.B] }
    }
    schematic Main {
        sheet: A4 landscape
        routing { default: zone_local  ground_nets: symbol }
        group Bank { members: [R1, R2, R3, R4, R5] }
        place R2 offset: (20.32mm, 0mm) from R1
        place R3 at_angle: 45deg dist: 25.4mm from R1
        array [R4, R5] horizontal pitch: 15.24mm
    }
}
"#;
    let ir = edl::compile_source("rel_geom_test.edl", src).expect("should compile");
    assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
    let center = |cid: &str| {
        let ent = ir.entities.iter().find(|e| e.id == cid).unwrap();
        let EntityGeometryIr::Component { body_rect, .. } = &ent.geometry else {
            panic!();
        };
        body_rect.center()
    };
    let c1 = center("R1");
    let c2 = center("R2");
    let c3 = center("R3");
    let c4 = center("R4");
    let c5 = center("R5");
    assert!((c2.x - c1.x - 20.32).abs() <= 2.6, "offset dx mismatch: {:?}", (c1, c2));
    assert!(c3.x > c1.x && c3.y < c1.y, "45deg polar must place R3 northeast of R1");
    assert!((c5.x - c4.x - 15.24).abs() <= 0.1 && (c5.y - c4.y).abs() <= 0.1, "horizontal array pitch mismatch");

    // Cycle detection test: R1 left_of R2 AND R2 left_of R1 -> E_LAYOUT_CYCLE
    let cyclic = r#"
project "CycleTest" {
    units: mm
    frame Sheet: Frame2
    circuit C {
        component R1: resistor { value: "1kohm" pin A: passive side: west pin B: passive side: east }
        component R2: resistor { value: "1kohm" pin A: passive side: west pin B: passive side: east }
        net N1 { connect: [R1.A, R2.A] }
        net GND { connect: [R1.B, R2.B] }
    }
    schematic Main {
        sheet: A4 landscape
        group G { members: [R1, R2] }
        place R1 left_of R2
        place R2 left_of R1
    }
}
"#;
    let cir = edl::compile_source("cycle_test.edl", cyclic).expect("parses");
    assert!(
        cir.diagnostics.iter().any(|d| d.code == "E_LAYOUT_CYCLE"),
        "expected E_LAYOUT_CYCLE diagnostic, got: {:?}",
        cir.diagnostics
    );
}

#[test]
fn v3_semantic_lifter_stickhub_and_smps_compress_50x_and_roundtrip_100_percent_without_lockfile() {
    for (rel, min_comps, min_nets, min_ratio) in [
        ("stickhub/StickHub.kicad_sch", 90usize, 40usize, 50.0_f64),
        ("simulation/power_supplies/boost/smps-com.kicad_sch", 20, 14, 40.0),
    ] {
        let orig = Path::new("/usr/share/kicad/demos").join(rel);
        if !orig.is_file() {
            continue;
        }
        let stem = orig.file_stem().and_then(|s| s.to_str()).unwrap_or("demo");
        let out_dir = std::env::temp_dir().join(format!("edl_v3_lift_{}_{}", std::process::id(), stem));
        let _ = fs::remove_dir_all(&out_dir);

        let rep = lift_kicad_project(&orig, &out_dir).expect("lift_kicad_project");
        assert!(rep.layout_lock_path.is_file(), "separate .edl.layout lockfile must be created");
        let edl_text = fs::read_to_string(&rep.edl_path).unwrap();
        assert!(
            !edl_text.contains("at: ("),
            "lifted AI-native .edl must contain ZERO raw global coordinates!"
        );
        let ratio = rep.orig_kicad_lines as f64 / rep.intent_edl_lines as f64;
        assert!(
            ratio >= min_ratio,
            "expected >= {min_ratio}x compression on {rel}, got {ratio:.1}x ({} -> {} lines)",
            rep.orig_kicad_lines,
            rep.intent_edl_lines
        );

        // Compile ONLY the short coordinate-free .edl (WITHOUT the lockfile) and verify 100% netlist match
        // AND verify E11_sheet_frame_fit (ZERO components outside the red KiCad border frame!).
        let ir = edl::compile_file(&rep.edl_path).expect("compile lifted .edl");
        let fit_ev = ir
            .evidence
            .iter()
            .find(|e| e.check_id.starts_with("E11_sheet_frame_fit"))
            .expect("E11_sheet_frame_fit evidence record must be present");
        assert_eq!(
            fit_ev.status,
            VerificationStatus::ExactlyChecked,
            "Lifted {rel} overflowed sheet frame: {}",
            fit_ev.details
        );
        let rt_dir = out_dir.join("rt");
        fs::create_dir_all(&rt_dir).unwrap();
        let files = edl::export_kicad_project(&ir);
        for (fname, content) in &files {
            fs::write(rt_dir.join(fname), content).unwrap();
        }
        let rt_root = rt_dir.join(
            files
                .keys()
                .find(|k| k.ends_with(".kicad_sch"))
                .expect("at least one .kicad_sch file"),
        );
        let cmp = compare_kicad_netlists(&orig, &rt_root).expect("compare_kicad_netlists");
        assert!(
            cmp.exact_match,
            "Lifted intent netlist mismatch on {rel}: orig=({},{}) rt=({},{}) missing={:?} extra={:?}",
            cmp.orig_components,
            cmp.orig_nets,
            cmp.rt_components,
            cmp.rt_nets,
            &cmp.missing_pin_pairs[..cmp.missing_pin_pairs.len().min(3)],
            &cmp.extra_pin_pairs[..cmp.extra_pin_pairs.len().min(3)],
        );
        assert!(cmp.orig_components >= min_comps);
        assert!(cmp.orig_nets >= min_nets);
        let _ = fs::remove_dir_all(&out_dir);
    }
}

#[test]
fn v3_1_rust_non_negotiables_catch_overlap_overflow_and_preserve_hierarchical_nets() {
    // 1. Explicit overlapping `place` is obeyed literally and rejected as a hard E_COMPONENT_OVERLAP error!
    let overlap_src = r#"
project "OverlapCheck" {
    units: mm
    frame Sheet: Frame2
    circuit C {
        component R1: resistor { value: "1kohm" pin A: passive side: west pin B: passive side: east }
        component R2: resistor { value: "2kohm" pin A: passive side: west pin B: passive side: east }
        net "/Power/VIN"  { connect: [R1.A, R2.A] }
        net "/Sensor/GND" { connect: [R1.B, R2.B] }
    }
    schematic Main {
        sheet: A4 landscape
        group G { members: [R1, R2] }
        place R2 offset: (0mm, 0mm) from R1
    }
}
"#;
    let ir = edl::compile_source("overlap_check.edl", overlap_src).expect("parses");
    assert!(ir.project.has_errors, "overlapping components must fail compilation");
    assert!(
        ir.diagnostics.iter().any(|d| d.code == "E_COMPONENT_OVERLAP"),
        "expected E_COMPONENT_OVERLAP, got: {:?}",
        ir.diagnostics
    );
    let e12 = ir
        .evidence
        .iter()
        .find(|e| e.check_id.starts_with("E12_no_component_overlap"))
        .expect("E12 evidence");
    assert_eq!(e12.status, VerificationStatus::Violated);
    assert!(!e12.repair_suggestions.is_empty());

    // 2. Verify `/Module/Net` hierarchical net paths are extracted cleanly without merging distinct modules!
    let r_pwr = ir.relations.iter().find(|r| r.id == "/Power/VIN").expect("/Power/VIN relation");
    let r_sns = ir.relations.iter().find(|r| r.id == "/Sensor/GND").expect("/Sensor/GND relation");
    assert_eq!(r_pwr.hierarchy, vec!["Power".to_string()]);
    assert_eq!(r_sns.hierarchy, vec!["Sensor".to_string()]);
}

#[test]
fn v3_2_unquoted_hierarchical_nets_cross_circuit_links_and_mna_derating_e13_e14() {
    // 1. Multi-stage industrial power distribution showcase compiles cleanly with E1-E5 + E11-E14!
    let mut ir = edl::compile_file(Path::new("examples/multisheet_power_distribution.edl"))
        .expect("multisheet_power_distribution.edl should compile");
    assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
    assert!(
        ir.relations.iter().any(|r| r.id == "/PrimaryStage/V3V3" && r.hierarchy == vec!["PrimaryStage"]),
        "expected canonical /PrimaryStage/V3V3 net with hierarchy, got: {:?}",
        ir.relations.iter().map(|r| (&r.id, &r.hierarchy)).collect::<Vec<_>>()
    );
    // Cross-hierarchy link merged SecondaryStage pins (C3.POS, U2.VDD) into /PrimaryStage/V3V3!
    let v3v3 = ir.relations.iter().find(|r| r.id == "/PrimaryStage/V3V3").unwrap();
    assert!(v3v3.connects.iter().any(|c| c == "U1.VOUT"));
    assert!(v3v3.connects.iter().any(|c| c == "U2.VDD"));
    assert!(v3v3.connects.iter().any(|c| c == "C3.POS"));

    // Verify E13 voltage derating & E14 power derating both passed ExactlyChecked
    for check in ["E13_voltage_derating", "E14_power_derating"] {
        let ev = ir.evidence.iter().find(|e| e.check_id == check).expect(check);
        assert_eq!(ev.status, VerificationStatus::ExactlyChecked, "{check}: {}", ev.details);
    }

    // Verify exported KiCad 9 schematic passes real `kicad-cli sch erc` (or Unverified if absent in CI)
    edl::domains::elec::kicad_erc::run_and_ingest_kicad_erc(&mut ir);
    if let Some(ev) = ir.evidence.iter().find(|e| e.check_id == "K1_kicad_erc_external") {
        assert!(
            ev.status == VerificationStatus::ExactlyChecked
                || ev.status == VerificationStatus::Unverified,
            "KiCad 9 ERC failed on hierarchical power distribution: {:?}",
            ir.diagnostics
        );
    }

    // 2. Dangling cross-hierarchy net reference `/Missing/Rail` triggers hard E_REF_UNKNOWN_NET
    let bad_link = r#"
project "BadHierLink" {
    units: mm
    circuit A {
        component R1: resistor { value: 1kohm pin A: passive side: west pin B: passive side: east }
        net /A/SIG { connect: [R1.A, /Missing/Rail] }
        net GND { connect: [R1.B] }
    }
}
"#;
    let bad_ir = edl::compile_source("bad_link.edl", bad_link).expect("parses");
    assert!(bad_ir.project.has_errors);
    assert!(
        bad_ir.diagnostics.iter().any(|d| d.code == "E_REF_UNKNOWN_NET"),
        "expected E_REF_UNKNOWN_NET, got: {:?}",
        bad_ir.diagnostics
    );

    // 3. MNA-backed E13 voltage derating & E14 power derating catch real hardware failures!
    let underrated = r#"
project "UnderratedParts" {
    units: mm
    frame Sheet: Frame2
    circuit Power {
        component J1: connector {
            value: "12V_IN"
            pin V12: power_out (12V) side: east
            pin GND: ground (0V) side: south
        }
        // 6.3V capacitor on a 12V rail -> E_VOLTAGE_DERATING (suggest >= 16V)
        component C_BAD: capacitor {
            value: 10uF
            voltage_rating: 6.3V
            pin POS: passive side: north
            pin NEG: passive side: south
        }
        // 400 ohm resistor across 12V -> P = 144 / 400 = 0.360W on a 125mW part -> E_POWER_DERATING (suggest >= 0.5W)
        component R_HOT: resistor {
            value: 400ohm
            power_rating: 125mW
            pin A: passive side: north
            pin B: passive side: south
        }
        net /Power/V12 { connect: [J1.V12, C_BAD.POS, R_HOT.A] power_flag: true }
        net /Power/GND { connect: [J1.GND, C_BAD.NEG, R_HOT.B] }
    }
    schematic Main {
        sheet: A4 landscape
        group G { members: [J1, C_BAD, R_HOT] }
    }
}
"#;
    let uir = edl::compile_source("underrated.edl", underrated).expect("parses");
    assert!(uir.project.has_errors);
    assert!(
        uir.diagnostics.iter().any(|d| d.code == "E_VOLTAGE_DERATING"),
        "expected E_VOLTAGE_DERATING, got: {:?}",
        uir.diagnostics
    );
    assert!(
        uir.diagnostics.iter().any(|d| d.code == "E_POWER_DERATING"),
        "expected E_POWER_DERATING, got: {:?}",
        uir.diagnostics
    );
    let e13 = uir.evidence.iter().find(|e| e.check_id == "E13_voltage_derating").unwrap();
    let e14 = uir.evidence.iter().find(|e| e.check_id == "E14_power_derating").unwrap();
    assert_eq!(e13.status, VerificationStatus::Violated);
    assert_eq!(e14.status, VerificationStatus::Violated);
    assert!(e13.repair_suggestions[0].description.contains("16.0V"));
    assert!(e14.repair_suggestions[0].description.contains("0.500W"));
}

#[test]
fn v33_multipin_relay_flyback_and_vssa_ground_verify_cleanly() {
    let src = r#"
project "RelayFlybackV33" {
    units: mm
    circuit Main {
        component J_ISO: "Connector:Conn_01x02_Pin" { value: "ISO_24V" }
        component K1: "Relay:Relay_SPDT" { value: "G5LE-1-DC24" }
        component D_FLY: "Device:D" { value: "1N4148W" }
        component Q1: "Transistor_FET:2N7002" { value: "2N7002" }
        net ISO_24V { connect: [J_ISO.1, K1.A1, D_FLY.K] power_flag: true }
        net COIL_SW { connect: [K1.A2, D_FLY.A, Q1.D] }
        net ISO_GND { connect: [J_ISO.2, Q1.S] }
        net GATE_DRV { connect: [Q1.G] }
    }
    schematic Main {
        sheet: A4 landscape
        group Relay { members: [J_ISO, K1, D_FLY, Q1] }
    }
    verify {
        require inductive_flyback
    }
}
"#;
    let ir = edl::compile_source("relay_v33.edl", src).expect("compiles cleanly");
    let e9 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "E9_inductive_flyback")
        .expect("E9 evidence");
    assert_eq!(
        e9.status,
        VerificationStatus::ExactlyChecked,
        "multi-pin Relay:Relay_SPDT with antiparallel D_FLY should pass E9: {}",
        e9.details
    );
    assert!(e9.details.contains("1 transistor-switched inductive"), "details: {}", e9.details);

    // Negative check: removing flyback diode D_FLY must fail E9 on multi-pin Relay_SPDT K1!
    let no_fly = src
        .replace("component D_FLY: \"Device:D\" { value: \"1N4148W\" }\n", "")
        .replace(", D_FLY.K", "")
        .replace(", D_FLY.A", "")
        .replace(", D_FLY", "");
    let bad_ir = edl::compile_source("relay_no_fly.edl", &no_fly).expect("parses");
    assert!(
        bad_ir.diagnostics.iter().any(|d| d.code == "E_ERC_MISSING_FLYBACK" && d.message.contains("K1")),
        "expected E_ERC_MISSING_FLYBACK mentioning K1, got: {:?}",
        bad_ir.diagnostics
    );
}

#[test]
fn v34_free_sheet_layout_explicit_rect_center_divider_rotated_labels_and_orient180_guard() {
    let src = r#"
project "FreeLayoutV34" {
    units: mm
    circuit Main {
        component J_PWR: "Connector:Screw_Terminal_01x02" { value: "24V_IN" }
        component F1: "Device:Polyfuse" { value: "1.5A" }
        component C1: "Device:C" { value: 100nF  voltage_rating: 50V }
        component C2: "Device:C" { value: 100nF  voltage_rating: 50V }
        component R1: "Device:R" { value: 10kohm }
        net "+24V" { connect: [J_PWR.1, F1.1] power_flag: true }
        net V24_FUSED { connect: [F1.2, C1.1, C2.1, R1.1] }
        net GND { connect: [J_PWR.2, C1.2, C2.2, R1.2] }
    }
    schematic Main {
        sheet: A3 landscape
        routing { default: zone_local  power: symbol  ground: symbol }
        group Stage1 {
            title: "STAGE 1: ISOLATED INPUT"
            rect: (17.78mm, 17.78mm, 228.6mm, 139.7mm)
            color: red
            border: dashed
            members: [J_PWR, F1, C1, C2, R1]
            note: "Free placement with zero auto-pack hijacking"
        }
        divider {
            from: (177.8mm, 25.4mm)
            to: (177.8mm, 127.0mm)
            label: "ISOLATION BARRIER"
            color: red
        }
        place J_PWR center (45.72mm, 63.5mm) orient: 180
        place F1    center (76.20mm, 63.5mm) orient: 90
        place C1    center (101.6mm, 83.82mm)
        place C2    center (121.92mm, 83.82mm)
        place R1    center (152.4mm, 83.82mm)
    }
}
"#;
    let ir = edl::compile_source("free_v34.edl", src).expect("compiles cleanly");
    assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
    // Explicit rect obeyed literally
    let g = ir.schematic_groups.iter().find(|g| g.name == "Stage1").unwrap();
    assert!((g.bbox.min_x - 17.78).abs() < 0.05);
    assert!((g.bbox.max_x - 228.6).abs() < 0.05);
    assert_eq!(g.color.as_deref(), Some("red"));
    assert_eq!(ir.schematic_dividers.len(), 1);

    // KiCad export emits horizontal labels (angle 90 for orient:90 passive F1), dashed red border, divider,
    // and passes real KiCad 9 ERC with 0 errors even on orient:180 J_PWR!
    let kicad = edl::export_kicad_sch(&ir);
    assert!(kicad.contains("ISOLATION BARRIER"));
    assert!(kicad.contains("(color 153 27 27 1)"));
    assert!(kicad.contains("(property \"Reference\" \"F1\" (at "));
    assert!(kicad.contains(" 90) (effects"), "F1 orient:90 should have horizontal property angle 90");

    if std::process::Command::new("kicad-cli").arg("--version").output().is_ok() {
        let dir = std::env::temp_dir().join("edl_v34_free_erc");
        let _ = std::fs::create_dir_all(&dir);
        let sch = dir.join("free_v34.kicad_sch");
        let rpt = dir.join("erc.rpt");
        std::fs::write(&sch, &kicad).unwrap();
        let st = std::process::Command::new("kicad-cli")
            .args(["sch", "erc", "--exit-code-violations", "--severity-error", "-o"])
            .arg(&rpt)
            .arg(&sch)
            .status()
            .unwrap();
        let txt = std::fs::read_to_string(&rpt).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(st.success(), "orient:180 J_PWR + free layout failed KiCad 9 ERC:\n{txt}");
    }
}

#[test]
fn v35_e15_divider_crossing_e16_footprint_check_layout_inspect_and_guide() {
    // 1. Guide covers all 5 domains concisely
    for dom in ["elec", "arch", "plumbing", "mep_elec", "sld"] {
        let g = edl::guide::domain_guide(Some(dom));
        assert!(g.len() > 200 && g.len() < 8000, "guide {dom} length={}", g.len());
    }

    // 2. E15 catches a routed wire crossing an isolation barrier divider + E16 catches nonexistent KiCad footprint
    let bad_src = r#"
project "BarrierTestV35" {
    units: mm
    circuit Main {
        component R1: "Device:R" { value: 1kohm  footprint: "Resistor_SMD:R_0603_1608Metric" }
        component D1: "Device:D" { value: "BAT54" footprint: "Diode_SMD:D_SOT-23_NONEXISTENT_XYZ" }
        component J1: "Connector:Conn_01x02_Pin" { value: "PWR" }
        net "+5V" { connect: [J1.1, R1.1] power_flag: true }
        net CROSS { connect: [R1.2, D1.A] }
        net GND   { connect: [J1.2, D1.K] }
    }
    schematic Main {
        sheet: A3 landscape
        routing { default: zone_local  power: symbol  ground: symbol }
        group G {
            title: "STAGE 5: ISOLATED"
            rect: (20mm, 20mm, 220mm, 140mm)
            members: [J1, R1, D1]
        }
        divider {
            from: (114.3mm, 25.4mm)
            to: (114.3mm, 134.62mm)
            label: "ISOLATION BARRIER"
            color: red
        }
        place J1 center (50.8mm, 76.2mm) orient: 180
        place R1 center (88.9mm, 76.2mm) orient: 90
        place D1 center (139.7mm, 76.2mm) orient: 180
    }
}
"#;
    let ir = edl::compile_source("barrier_v35.edl", bad_src).expect("compiles");
    assert!(
        ir.diagnostics.iter().any(|d| d.code == "E_DIVIDER_WIRE_CROSSING"),
        "expected E_DIVIDER_WIRE_CROSSING for CROSS wire crossing divider at x=114.3mm, got: {:?}",
        ir.diagnostics
    );
    let e15 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "E15_divider_isolation_barrier_Main")
        .expect("E15 evidence");
    assert_eq!(e15.status, VerificationStatus::Violated);

    if std::path::Path::new("/usr/share/kicad/footprints/Diode_SMD.pretty").is_dir() {
        assert!(
            ir.diagnostics.iter().any(|d| d.code == "W_FOOTPRINT_NOT_FOUND" && d.message.contains("D_SOT-23_NONEXISTENT_XYZ")),
            "expected W_FOOTPRINT_NOT_FOUND for bogus footprint, got: {:?}",
            ir.diagnostics
        );
    }

    // 3. Layout inspection reports exact component centers, pin tips, zones & dividers
    let txt = edl::guide::format_layout_inspection(&ir, true, true, true);
    assert!(txt.contains("=== ZONES (1) ==="));
    assert!(txt.contains("=== DIVIDERS (1) ==="));
    assert!(txt.contains("ISOLATION BARRIER"));
    assert!(txt.contains("R1 (Device:R)"));
    assert!(txt.contains("CROSS"));
}

#[test]
fn v36_tier1_kicad_hierarchy_custom_power_ergonomics_and_tier2_cd_arch_mep_dxf() {
    if !std::path::Path::new("/usr/share/kicad/symbols/Device.kicad_sym").exists() {
        return;
    }
    // 1. Tier 1: EE shorthand units (4.7k, 100R, 1M), quoted/numeric pin overrides (`pin "V-": ground`,
    //    `pin 2: ground`), custom isolated power library (`edl_power.kicad_sym` + `sym-lib-table`),
    //    adjacent-zone direct A* wiring, wired hierarchical root sheet, and 0/0 KiCad 9 CLI ERC!
    let src = r#"
project Tier1_V36_Test {
    circuit Main {
        component J1: conn {
            symbol: "Connector:Conn_01x02_Pin"
            value: "PWR_IN"
            footprint: "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical"
            pin 1: power_out(5V)
            pin 2: ground
        }
        component R1: resistor {
            symbol: "Device:R"
            value: 4.7k
            footprint: "Resistor_SMD:R_0603_1608Metric"
        }
        component R2: resistor {
            symbol: "Device:R"
            value: 100R
            footprint: "Resistor_SMD:R_0603_1608Metric"
        }
        component U1: ic {
            symbol: "Amplifier_Operational:LM321"
            value: "LM321"
            footprint: "Package_TO_SOT_SMD:SOT-23-5"
            pin "V-": ground
        }
        component R3: resistor {
            symbol: "Device:R"
            value: 1M
            footprint: "Resistor_SMD:R_0603_1608Metric"
        }
        component J2: conn {
            symbol: "Connector:Conn_01x02_Pin"
            value: "ISO_PORT"
            footprint: "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical"
            pin 1: power_out(24V)
            pin 2: ground
        }
        component R4: resistor {
            symbol: "Device:R"
            value: 4.7k
            footprint: "Resistor_SMD:R_0603_1608Metric"
        }

        net "+5V" { J1.1, R1.1, U1."V+" }
        net GND { J1.2, U1."V-", R2.2, R3.2 }
        net SIG_MID { R1.2, R2.1, U1."+" }
        net SIG_OUT { U1.1, U1."-", R3.1 }
        net ISO_24V { J2.1, R4.1 }
        net ISO_GND { J2.2, R4.2 }
    }

    schematic Sheet1_AFE {
        sheet: A4 landscape
        title: "Sheet 1: Analog Front End"
        routing { power: symbol  ground: symbol  signals: zone_local }
        group Input {
            rect: (25.4mm, 25.4mm, 114.3mm, 127.0mm)
            color: blue
            members: [J1, R1, R2]
        }
        group Buffer {
            rect: (127.0mm, 25.4mm, 215.9mm, 127.0mm)
            color: green
            members: [U1]
        }
        place J1 center (50.8mm, 76.2mm) orient: 180
        place R1 center (88.9mm, 63.5mm)
        place R2 center (88.9mm, 91.44mm)
        place U1 center (165.1mm, 76.2mm)
    }

    schematic Sheet2_Output {
        sheet: A4 landscape
        title: "Sheet 2: Output & Isolated Rail"
        routing { power: symbol  ground: symbol  signals: zone_local }
        group OutZone {
            rect: (25.4mm, 25.4mm, 114.3mm, 127.0mm)
            color: blue
            members: [R3]
        }
        group IsoZone {
            rect: (139.7mm, 25.4mm, 228.6mm, 127.0mm)
            color: red
            members: [J2, R4]
        }
        place R3 center (63.5mm, 76.2mm)
        place J2 center (165.1mm, 76.2mm) orient: 180
        place R4 center (203.2mm, 76.2mm)
    }
}
"#;
    let ir = edl::compile_source("tier1_v36.edl", src).expect("compiles cleanly");
    assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
    // Adjacent-zone signal SIG_MID should route directly via A* wire (no floating label_stub)!
    let sig_mid = ir.schematic_routes.iter().find(|r| r.net_id == "SIG_MID").expect("SIG_MID route");
    assert!(!sig_mid.segments.is_empty(), "SIG_MID should have direct orthogonal wire segments");
    assert!(sig_mid.terminals.is_empty(), "SIG_MID should route directly across adjacent zones without label stubs");

    // Project export creates wired hierarchical root sheet + custom edl_power.kicad_sym + sym-lib-table + .kicad_pro
    let files = edl::export_kicad_project(&ir);
    assert!(files.contains_key("edl_power.kicad_sym"));
    assert!(files.contains_key("sym-lib-table"));
    assert!(files.contains_key("Tier1_V36_Test.kicad_pro"));
    let root = files.get("Tier1_V36_Test.kicad_sch").expect("root sheet");
    assert!(root.contains("(pin \"SIG_OUT\" bidirectional"), "root sheet must have explicit SIG_OUT sheet pin");
    assert!(root.contains("(wire "), "root sheet must wire inter-sheet signals");
    assert!(root.contains("GLOBAL POWER & ISOLATION DOMAINS"));

    // Verify 0 KiCad 9 ERC errors AND 0 warnings under --severity-all!
    let tmp_dir = std::env::temp_dir().join("edl_v36_tier1_erc");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).unwrap();
    for (fname, content) in &files {
        std::fs::write(tmp_dir.join(fname), content).unwrap();
    }
    let erc_out = std::process::Command::new("kicad-cli")
        .args([
            "sch",
            "erc",
            "--exit-code-violations",
            "--severity-all",
            "-o",
            tmp_dir.join("erc.rpt").to_str().unwrap(),
            tmp_dir.join("Tier1_V36_Test.kicad_sch").to_str().unwrap(),
        ])
        .output();
    if let Ok(out) = erc_out {
        let rpt = std::fs::read_to_string(tmp_dir.join("erc.rpt")).unwrap_or_default();
        assert!(out.status.success(), "KiCad 9 ERC --severity-all failed:\n{rpt}");
    }
    let _ = std::fs::remove_dir_all(&tmp_dir);

    // 2. Tier 2: Construction-Document Arch/MEP SVG & Native DXF BLOCKS/INSERT + A-ANNO-DIMS
    let clinic_ir = edl::compile_file(std::path::Path::new("examples/villa_3br_arch_elec_plumbing.edl"))
        .expect("compile multi-discipline villa");
    assert!(!clinic_ir.project.has_errors);
    let arch_svg = edl::render_arch_svg_layers(&clinic_ir, false, false, false);
    assert!(arch_svg.contains("class=\"dim-tick\""), "45-deg architectural dim ticks");
    assert!(arch_svg.contains("class=\"tag-door-bg\""), "CAD door badges");
    assert!(arch_svg.contains("class=\"tag-win-bg\""), "CAD window badges");
    assert!(arch_svg.contains("id=\"cd-title-block\""), "boxed CD title block");
    assert!(arch_svg.contains("GRAPHIC SCALE (m)"), "calibrated scale bar");

    let all_svg = edl::render_arch_svg_layers(&clinic_ir, false, true, true);
    assert!(all_svg.contains("class=\"ckt-tag-bg\""), "inline electrical circuit tags");

    let dxf = edl::export_dxf(&clinic_ir);
    assert!(dxf.contains("SECTION\n2\nBLOCKS\n"), "DXF BLOCKS section");
    assert!(dxf.contains("EDL_LIGHT\n") && dxf.contains("EDL_PANEL\n"), "DXF MEP symbol blocks");
    assert!(dxf.contains("\nINSERT\n"), "DXF INSERT block references");
    assert!(dxf.contains("A-ANNO-DIMS\n"), "DXF A-ANNO-DIMS dimension layer");
}
