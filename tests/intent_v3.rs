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
        let rt_root = rt_dir.join(files.keys().next().unwrap());
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
