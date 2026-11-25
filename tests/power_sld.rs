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
