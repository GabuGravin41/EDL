use edl::core::ir::VerificationStatus;
use edl::{compile_source, export_dxf, render_svg};

#[test]
fn building_power_sld_2x1600kva_passes_all_6_iec_proofs_and_exports_svg_dxf() {
    let src = std::fs::read_to_string("examples/building_power_sld_2x1600kva.edl").unwrap();
    let ir = compile_source("examples/building_power_sld_2x1600kva.edl", &src).unwrap();

    assert_eq!(ir.project.domain, "elec.sld");
    assert!(
        !ir.project.has_errors,
        "unexpected SLD errors: {:?}",
        ir.diagnostics
    );

    for (rule, expected) in [
        ("S1_source_bus_and_n1_capacity", VerificationStatus::ExactlyChecked),
        ("S2_breaker_cable_coordination", VerificationStatus::ExactlyChecked),
        ("S3_protection_selectivity", VerificationStatus::ExactlyChecked),
        ("S4_short_circuit_breaking_capacity", VerificationStatus::ExactlyChecked),
        ("S5_cumulative_voltage_drop", VerificationStatus::ExactlyChecked),
        ("S6_ats_coupler_interlock", VerificationStatus::FormallyChecked),
    ] {
        let ev = ir
            .evidence
            .iter()
            .find(|e| e.check_id == rule)
            .unwrap_or_else(|| panic!("missing SLD rule {rule}"));
        assert_eq!(ev.status, expected, "rule {rule}: {}", ev.details);
    }

    let svg = render_svg(&ir, true);
    assert!(svg.contains("LVMD_A"));
    assert!(svg.contains("ATS 2-of-3"));
    assert!(svg.contains("N-1 Emergency"));
    assert!(svg.contains("ALL IEC 60909"));

    let dxf = export_dxf(&ir);
    assert!(dxf.contains("SLD_BUSBAR"));
    assert!(dxf.contains("SLD_INTERLOCK"));
    assert!(dxf.contains("SLD_METRICS"));
}

#[test]
fn sld_catches_arc_flash_breaking_capacity_undersized_cable_missing_ats_and_n1_overload() {
    let src = std::fs::read_to_string("examples/building_power_sld_2x1600kva.edl").unwrap();

    // 1. Arc-flash hazard: 10kA breaker on 38.4kA busbar (S4)
    let bad_icu = src.replace(
        "breaker: QF1  kind: mccb  rating_a: 630  icu_ka: 50",
        "breaker: QF1  kind: mccb  rating_a: 630  icu_ka: 10",
    );
    let ir1 = compile_source("bad_icu.edl", &bad_icu).unwrap();
    assert!(ir1.project.has_errors);
    assert!(ir1.diagnostics.iter().any(|d| d.code == "E_SLD_BREAKING_CAPACITY"));

    // 2. Undersized cable ampacity under 400A breaker (S2)
    let bad_cable = src.replace(
        "cable: \"Cu 3x240+120\"  area_mm2: 240  parallel_runs: 1  length_m: 40",
        "cable: \"Cu 3x16+10\"    area_mm2: 16   parallel_runs: 1  length_m: 40",
    );
    let ir2 = compile_source("bad_cable.edl", &bad_cable).unwrap();
    assert!(ir2.project.has_errors);
    assert!(ir2.diagnostics.iter().any(|d| d.code == "E_SLD_CABLE_UNDERSIZED"));

    // 3. Missing ATS interlock on dual-source bus coupler (S6)
    let bad_ats = src.replace("mode: ats_2_of_3", "mode: none");
    let ir3 = compile_source("bad_ats.edl", &bad_ats).unwrap();
    assert!(ir3.project.has_errors);
    assert!(ir3.diagnostics.iter().any(|d| d.code == "E_SLD_COUPLER_INTERLOCK_MISSING"));

    // 4. Hidden N-1 Contingency Overload: 800 kVA transformers pass normal working mode (393 & 496 kVA),
    //    but FAIL when one transformer trips and QF_Tie closes (883 kVA > 800 kVA)!
    let bad_n1 = src.replace("rating_kva: 1600", "rating_kva: 800");
    let ir4 = compile_source("bad_n1.edl", &bad_n1).unwrap();
    assert!(ir4.project.has_errors);
    assert!(ir4.diagnostics.iter().any(|d| d.code == "E_SLD_N1_CONTINGENCY_OVERLOAD"));
}

#[test]
fn sld_v33_mv_intake_computes_exact_iec60909_upstream_impedance_and_renders_rmu_and_sc_table() {
    let src = r#"
project "Hospital_MV_SLD" {
    units: mm
    power_sld HospitalSLD {
        standard: "IEC 60909 / IEC 60364"
        mv_intake RMU_11KV { utility_kv: 11  sk_mva: 250  vcb_a: 630  vcb_icu_ka: 25 }
        bus MSB_A { voltage_v: 400  system: "TN-S"  rating_a: 1600  icu_ka: 50  tier: main }
        bus MSB_B { voltage_v: 400  system: "TN-S"  rating_a: 1600  icu_ka: 50  tier: main }
        source TX1: transformer { rating_kva: 1000  hv_kv: 11.0  lv_v: 400  uk_pct: 6.0  vector: "Dyn11"  bus: MSB_A  incomer: Q_TX1  incomer_a: 1600  icu_ka: 50 }
        source TX2: transformer { rating_kva: 1000  hv_kv: 11.0  lv_v: 400  uk_pct: 6.0  vector: "Dyn11"  bus: MSB_B  incomer: Q_TX2  incomer_a: 1600  icu_ka: 50 }
        coupler Q_BT { from: MSB_A  to: MSB_B  rating_a: 1600  mode: ats_2_of_3 }
        load F1: motor { bus: MSB_A  power_kw: 200  pf: 0.85  breaker: QF1  kind: mccb  rating_a: 400  icu_ka: 50  cable: "Cu XLPE 3x240+120"  area_mm2: 240  parallel_runs: 1  length_m: 35 }
        load F2: panel { bus: MSB_B  power_kw: 180  pf: 0.90  breaker: QF2  kind: mccb  rating_a: 400  icu_ka: 50  cable: "Cu XLPE 3x240+120"  area_mm2: 240  parallel_runs: 1  length_m: 35 }
    }
}
"#;
    let ir = compile_source("hospital_mv.edl", src).expect("compiles cleanly");
    assert!(!ir.project.has_errors, "unexpected errors: {:?}", ir.diagnostics);
    let s4 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "S4_short_circuit_breaking_capacity")
        .expect("S4 evidence");
    assert_eq!(s4.status, VerificationStatus::ExactlyChecked);
    // With 250 MVA upstream grid + 1000 kVA uk=6% transformer, Ik3'' = ~23.6 kA (not infinite-bus 26.4 kA!)
    let svg = render_svg(&ir, true);
    assert!(svg.contains("UTILITY 11kV INTAKE"));
    assert!(svg.contains("RMU_11KV"));
    assert!(svg.contains("IEC 60909 SHORT-CIRCUIT"));
    assert!(svg.contains("23.6"));
    assert!(svg.contains("Outgoing Loads: 2"));
    assert!(!svg.contains("Feeders: 0"));
}
