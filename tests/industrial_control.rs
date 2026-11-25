use edl::core::ir::VerificationStatus;
use edl::{compile_source, export_dxf, render_svg};

#[test]
fn motor_forward_reverse_star_delta_passes_all_iec60617_checks() {
    let src = std::fs::read_to_string("examples/motor_forward_reverse_star_delta.edl")
        .expect("benchmark C exists");
    let ir = compile_source("examples/motor_forward_reverse_star_delta.edl", &src)
        .expect("compiles cleanly");

    assert_eq!(ir.project.domain, "elec.industrial");
    assert!(
        !ir.project.has_errors,
        "unexpected industrial control diagnostics: {:?}",
        ir.diagnostics
    );
    assert_eq!(ir.project.component_count, 10);
    assert_eq!(ir.project.net_count, 4);

    let i1 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "I1_mutual_interlock")
        .expect("I1 evidence");
    assert_eq!(i1.status, VerificationStatus::FormallyChecked);

    let i2 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "I2_estop_overload_authority")
        .expect("I2 evidence");
    assert_eq!(i2.status, VerificationStatus::FormallyChecked);

    let i3 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "I3_coil_contact_cross_ref")
        .expect("I3 evidence");
    assert_eq!(i3.status, VerificationStatus::ExactlyChecked);

    let i4 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "I4_phase_reversal_integrity")
        .expect("I4 evidence");
    assert_eq!(i4.status, VerificationStatus::ExactlyChecked);

    // SVG contains IEC 60617 motor, power bus, coils, and cross-references
    let svg = render_svg(&ir, true);
    assert!(svg.contains("MAIN POWER CIRCUIT"));
    assert!(svg.contains("CONTROL CIRCUIT"));
    assert!(svg.contains("M1 3~"));
    assert!(svg.contains("PWR: M1 (L1-L2-L3)"));
    assert!(svg.contains("ALL SAFETY CHECKS PROVED"));

    // AutoCAD R12 DXF contains all 6 IEC layers
    let dxf = export_dxf(&ir);
    assert!(dxf.contains("IEC_POWER_BUS"));
    assert!(dxf.contains("IEC_CONTROL_WIRE"));
    assert!(dxf.contains("IEC_SYMBOLS"));
    assert!(dxf.contains("IEC_MECH_LINK"));
    assert!(dxf.contains("IEC_CROSSREF"));
    assert!(dxf.contains("EOF"));
}

#[test]
fn missing_contactor_interlock_is_caught_as_dead_short_hazard() {
    let src = std::fs::read_to_string("examples/motor_forward_reverse_star_delta.edl")
        .expect("benchmark C exists");
    // Remove RC.21-22 interlock from FWD rung — creating a 400V phase-to-phase dead short!
    let unsafe_src = src.replace("interlock_nc: [\"RC.21-22\"]", "interlock_nc: []");
    let ir = compile_source("unsafe_interlock.edl", &unsafe_src).expect("parses");

    assert!(ir.project.has_errors, "must catch missing NC interlock!");
    assert!(
        ir.diagnostics
            .iter()
            .any(|d| d.code == "E_IND_INTERLOCK_MISSING"),
        "expected E_IND_INTERLOCK_MISSING, got: {:?}",
        ir.diagnostics
    );
    let i1 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "I1_mutual_interlock")
        .unwrap();
    assert_eq!(i1.status, VerificationStatus::Violated);
}

#[test]
fn bypassed_estop_is_caught_by_i2_authority_proof() {
    let src = std::fs::read_to_string("examples/motor_forward_reverse_star_delta.edl")
        .expect("benchmark C exists");
    // Remove S0 from master_series — bypassing emergency stop!
    let unsafe_src = src.replace(
        "master_series: [\"F2.95-96\", \"S0.1-2\"]",
        "master_series: [\"F2.95-96\"]",
    );
    let ir = compile_source("unsafe_interlock.edl", &unsafe_src).expect("parses");

    assert!(ir.project.has_errors, "must catch bypassed E-Stop!");
    assert!(
        ir.diagnostics
            .iter()
            .any(|d| d.code == "E_IND_TRIP_BYPASSED"),
        "expected E_IND_TRIP_BYPASSED, got: {:?}",
        ir.diagnostics
    );
    let i2 = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "I2_estop_overload_authority")
        .unwrap();
    assert_eq!(i2.status, VerificationStatus::Violated);
}
