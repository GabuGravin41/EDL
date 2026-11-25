//! v1.1 tests: native KiCad 9 `.kicad_sym` loading (`extends`, multi-unit, orientation, multi-pin GND,
//! explicit `no_connect`), galvanic isolation verification, and live `kicad-cli sch erc`.

use edl::core::ir::{EntityGeometryIr, VerificationStatus};
use edl::domains::elec::kicad_erc::run_and_ingest_kicad_erc;
use edl::domains::elec::kicad_sym::{load_symbol, search_symbols};
use edl::{compile_source, export_kicad_sch, render_svg};

#[test]
fn kicad_sym_search_and_inspect_loads_real_libraries_when_installed() {
    if !std::path::Path::new("/usr/share/kicad/symbols/Device.kicad_sym").exists() {
        return;
    }
    let hits = search_symbols("BMP280", &[], 10);
    assert!(hits.iter().any(|h| h.lib_id == "Sensor_Pressure:BMP280"), "{hits:?}");

    let bmp = load_symbol("Sensor_Pressure:BMP280", &[]).unwrap();
    assert_eq!(bmp.pins.len(), 8);
    assert_eq!(bmp.pins.iter().filter(|p| p.name == "GND").count(), 2);

    // `extends` inheritance resolution (`BC547` -> `Q_NPN_CBE`)
    let bjt = load_symbol("Transistor_BJT:BC547", &[]).unwrap();
    assert_eq!(bjt.pins.len(), 3);
    assert!(bjt.primitives.len() >= 4, "must inherit Q_NPN_CBE vector primitives");

    // Multi-unit symbol (`LM358` -> `LM2904`, 3 units)
    let op = load_symbol("Amplifier_Operational:LM358", &[]).unwrap();
    assert_eq!(op.unit_count, 3);
}

#[test]
fn native_kicad_symbols_compile_route_render_and_pass_real_kicad_erc() {
    if !std::path::Path::new("/usr/share/kicad/symbols/Device.kicad_sym").exists() {
        return;
    }
    let src = r#"
project "KiCad_Sym_Native_Demo" {
    units: mm
    frame Sheet: Frame2

    circuit SensorAndDriver {
        component J1: connector {
            value: "PWR_I2C"
            pin V3V3: power_out (3.3V)
            pin GND:  ground (0V)
            pin SDA:  bidir
            pin SCL:  output
            pin BEEP: output
        }

        // Authentic KiCad 9 BMP280 (8 pins, 2x GND pins auto-wired by U1.GND!)
        component U1: "Sensor_Pressure:BMP280" {
            no_connect: [SDO]
        }

        // Authentic KiCad 9 Device:R rotated 90 deg (horizontal) + vertical!
        component R1: "Device:R" { value: 10kohm  orient: 90 }
        component C1: "Device:C" { value: 100nF }

        // Authentic KiCad 9 BC547 (resolves extends Q_NPN_CBE!) + base resistor + load resistor
        component R2: "Device:R" { value: 1kohm  orient: 90 }
        component Q1: "Transistor_BJT:BC547" { value: "BC547" }
        component R3: "Device:R" { value: 470ohm }

        // Multi-unit LM358 unit A (1 of 3 units) with `+` / `-` pin syntax
        component U2A: "Amplifier_Operational:LM358" {
            unit: A
            no_connect: [1]
        }

        net V3V3 { connect: [J1.V3V3, U1.VDD, U1.VDDIO, R1.1, C1.1, R3.1] }
        net GND  { connect: [J1.GND, U1.GND, C1.2, Q1.E, U2A.-] }
        net SDA  { connect: [J1.SDA, U1.SDI] }
        net SCL  { connect: [J1.SCL, U1.SCK, U2A.+] }
        net CSB  { connect: [R1.2, U1.CSB] }
        net BEEP { connect: [J1.BEEP, R2.1] }
        net BASE { connect: [R2.2, Q1.B] }
        net COL  { connect: [R3.2, Q1.C] }
    }

    schematic Main {
        sheet: A4 landscape
        group Input  { members: [J1, C1] }
        group Sensor { members: [U1, R1] }
        group Driver { members: [R2, Q1, R3] }
        group Amp    { members: [U2A] }

        layout {
            arrange: [Input, Sensor, Driver, Amp] left_to_right
            group_gap: 25.4mm
            member_gap: 17.78mm
        }
        routing {
            power: symbol
            ground: symbol
            default: label_stub
        }
    }

    verify {
        require all_required_pins_connected
        require no_power_ground_short
        require no_multiple_power_drivers
        require no_single_pin_nets
    }
}
"#;

    let mut ir = compile_source("kicad_sym_demo.edl", src).expect("compile native kicad symbols");
    assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
    assert_eq!(ir.entities.len(), 8);

    // Check BMP280 has authentic symbol primitives, both GND pins (1 & 7), and SDO marked no_connect
    let u1 = ir.entities.iter().find(|e| e.id == "U1").unwrap();
    if let EntityGeometryIr::Component {
        lib_id,
        symbol_prims,
        pins,
        ..
    } = &u1.geometry
    {
        assert_eq!(lib_id.as_deref(), Some("Sensor_Pressure:BMP280"));
        assert!(symbol_prims.as_ref().map(|v| !v.is_empty()).unwrap_or(false));
        assert_eq!(pins.len(), 8);
        assert!(pins.iter().any(|p| p.name == "SDO" && p.no_connect));
        assert_eq!(pins.iter().filter(|p| p.name == "GND").count(), 2);
    } else {
        panic!("expected Component");
    }

    // Export SVG & KiCad 9 and verify real `kicad-cli sch erc` passes with 0 errors!
    let svg = render_svg(&ir, false);
    assert!(svg.contains("#2563eb"), "SVG should render explicit blue X no-connect flags");
    let kicad = export_kicad_sch(&ir);
    assert!(kicad.contains("(no_connect (at "), "KiCad export must emit no_connect markers");

    run_and_ingest_kicad_erc(&mut ir);
    let k1 = ir.evidence.iter().find(|e| e.check_id == "K1_kicad_erc_external").unwrap();
    assert_eq!(
        k1.status,
        VerificationStatus::ExactlyChecked,
        "KiCad 9 ERC failed on native .kicad_sym circuit: {} | {:?}",
        k1.details,
        ir.diagnostics
    );
}

#[test]
fn galvanic_isolation_rule_verifies_transformer_barrier_and_catches_breach() {
    let base = r#"
project "Isolated_SMPS_Check" {
    units: mm
    frame Sheet: Frame2
    circuit Power {
        component J_AC: connector {
            value: "AC_IN"
            pin HV: power_out (320V)
            pin GND_HOT: ground (0V)
        }
        component T1: transformer {
            value: "FLYBACK_T1"
            pin PRI_H: passive side: west
            pin PRI_L: passive side: west
            pin SEC_H: power_out (12V) side: east
            pin SEC_L: ground (0V) side: east
        }
        component R_LOAD: resistor {
            value: 100ohm
            pin A: passive
            pin B: passive
        }
        EXTRA_COMP
        net HV       { connect: [J_AC.HV, T1.PRI_H] }
        net GND_HOT  { connect: [J_AC.GND_HOT, T1.PRI_L EXTRA_HOT] }
        net V12_ISO  { connect: [T1.SEC_H, R_LOAD.A] }
        net GND_COLD { connect: [T1.SEC_L, R_LOAD.B EXTRA_COLD] }
    }
    verify {
        require all_required_pins_connected
        require no_power_ground_short
        require galvanic_isolation(GND_HOT, GND_COLD)
    }
}
"#;
    // 1. Intact isolation across transformer T1 -> PASS
    let clean = base
        .replace("EXTRA_COMP", "")
        .replace("EXTRA_HOT", "")
        .replace("EXTRA_COLD", "");
    let ir_ok = compile_source("iso_ok.edl", &clean).unwrap();
    assert!(!ir_ok.project.has_errors, "{:?}", ir_ok.diagnostics);
    let ev = ir_ok
        .evidence
        .iter()
        .find(|e| e.check_id == "E6_galvanic_isolation_GND_HOT_GND_COLD")
        .unwrap();
    assert_eq!(ev.status, VerificationStatus::ExactlyChecked);

    // 2. Accidental non-isolated resistor bridging GND_HOT <-> GND_COLD -> CAUGHT!
    let bad = base
        .replace(
            "EXTRA_COMP",
            "component R_SHORT: resistor { value: 1Mohm  pin A: passive  pin B: passive }",
        )
        .replace("EXTRA_HOT", ", R_SHORT.A")
        .replace("EXTRA_COLD", ", R_SHORT.B");
    let ir_bad = compile_source("iso_bad.edl", &bad).unwrap();
    assert!(ir_bad.project.has_errors);
    let diag = ir_bad
        .diagnostics
        .iter()
        .find(|d| d.code == "E_ERC_ISOLATION_BREACH")
        .expect("should report isolation breach");
    assert!(diag.message.contains("R_SHORT"), "{}", diag.message);
}

#[test]
fn benchmark_a_vectron_120_multisheet_passes_real_kicad_9_erc() {
    if !std::path::Path::new("/usr/share/kicad/symbols/Device.kicad_sym").exists() {
        return;
    }
    let mut ir = edl::compile_file(std::path::Path::new("examples/vectron_120_multisheet.edl"))
        .expect("compile Benchmark A");
    assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
    assert!(ir.project.component_count >= 120, "expected >= 120 components");
    assert_eq!(ir.views.len(), 4, "expected 4 hierarchical sheets");

    // Multi-sheet KiCad 9 project export emits root + 4 subsheets = 5 files
    let files = edl::export_kicad_project(&ir);
    assert_eq!(files.len(), 5, "files: {:?}", files.keys().collect::<Vec<_>>());
    assert!(files.values().any(|s| s.contains("(global_label \"SDA1\"")));
    // v2.0: native KiCad `(power)` symbols + zone `(rectangle ...)` frames directly in `.kicad_sch`
    assert!(files.values().any(|s| s.contains("(lib_id \"power:GND\")")));
    assert!(files.values().any(|s| s.contains("(lib_id \"power:V3V3_SENS\")")));
    assert!(files.values().any(|s| s.contains("(color 37 99 235 1)")));

    // v2.0 senior-engineer circuit rules E7-E10 all pass cleanly on Vectron!
    for rule_id in [
        "E7_ic_decoupling",
        "E8_i2c_pullups",
        "E9_inductive_flyback",
        "E10_bom_manufacturability",
    ] {
        let ev = ir.evidence.iter().find(|e| e.check_id == rule_id).expect(rule_id);
        assert_eq!(ev.status, VerificationStatus::ExactlyChecked, "{rule_id}: {}", ev.details);
    }

    // Real multi-sheet KiCad 9 CLI ERC across ALL 4 sheets -> 0 errors!
    run_and_ingest_kicad_erc(&mut ir);
    let k1 = ir.evidence.iter().find(|e| e.check_id == "K1_kicad_erc_external").unwrap();
    assert_eq!(k1.status, VerificationStatus::ExactlyChecked, "{}", k1.details);
}

#[test]
fn benchmark_b_isolated_flyback_smps_passes_galvanic_isolation_and_kicad_9_erc() {
    if !std::path::Path::new("/usr/share/kicad/symbols/Device.kicad_sym").exists() {
        return;
    }
    let mut ir = edl::compile_file(std::path::Path::new("examples/smps_flyback_isolated.edl"))
        .expect("compile Benchmark B");
    assert!(!ir.project.has_errors, "diagnostics: {:?}", ir.diagnostics);
    assert!(ir.project.component_count >= 50);

    let iso = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "E6_galvanic_isolation_GND_HOT_GND_COLD")
        .expect("galvanic isolation evidence");
    assert_eq!(iso.status, VerificationStatus::ExactlyChecked);
    let bom = ir
        .evidence
        .iter()
        .find(|e| e.check_id == "E10_bom_manufacturability")
        .expect("E10 BOM manufacturability evidence");
    assert_eq!(bom.status, VerificationStatus::ExactlyChecked, "{}", bom.details);

    let kicad = edl::export_kicad_sch(&ir);
    assert!(kicad.contains("(lib_id \"power:GND_HOT\")"));
    assert!(kicad.contains("(lib_id \"power:GND_COLD\")"));
    assert!(kicad.contains("(color 37 99 235 1)"));

    run_and_ingest_kicad_erc(&mut ir);
    let k1 = ir.evidence.iter().find(|e| e.check_id == "K1_kicad_erc_external").unwrap();
    assert_eq!(k1.status, VerificationStatus::ExactlyChecked, "{}", k1.details);
}

#[test]
fn senior_engineer_erc_rules_e7_to_e10_catch_real_hardware_bugs() {
    let buggy = r#"
project "BuggyHardwareCheck" {
    units: mm
    frame Sheet: Frame2

    circuit Board {
        component J1: connector {
            value: "PWR_12V"
            pin V12: power_out (12V)
            pin GND: ground (0V)
        }
        // Active IC missing decoupling cap on V12 (E7 violation!)
        component U1: ic {
            value: "MCU_NO_CAP"
            pin VDD:  power_in (10V..14V)
            pin GND:  ground (0V)
            pin SDA:  bidir
            pin SCL:  output
            pin CTRL: output
        }
        // Under-rated 12V capacitor on a 12V rail (< 1.25x derating -> E10 violation!)
        component C_BAD: capacitor {
            value: "100uF_12V"
            pin A: passive
            pin B: passive
        }
        // Over-dissipated 0402 resistor: 12V across 100ohm = 1440mW >> 62.5mW (E10 violation!)
        component R_HOT: resistor {
            value: 100ohm
            footprint: "Resistor_SMD:R_0402_1005Metric"
            pin A: passive
            pin B: passive
        }
        // Transistor switching an inductive relay with NO flyback clamp diode (E9 violation!)
        component Q1: npn {
            value: "2N2222"
            pin B: passive
            pin C: passive
            pin E: passive
        }
        component K1: relay {
            value: "12V_COIL"
            pin A: passive
            pin B: passive
        }
        component J_I2C: connector {
            value: "I2C_PORT"
            pin SDA: bidir
            pin SCL: input
        }

        // Intentional bugs:
        // - V12_IC has NO decoupling cap (C_BAD is on V12_RAW, not V12_IC)
        // - SDA and SCL have NO pull-up resistors (E8)
        // - K1 switched by Q1 has NO flyback diode (E9)
        // - C_BAD is only 12V rated on 12V rail; R_HOT dissipates 1.44W in 0402 (E10)
        component F1: fuse { value: "1A"  pin A: passive  pin B: passive }
        net V12_RAW { connect: [J1.V12, C_BAD.A, R_HOT.A, K1.A, F1.A] }
        net V12_IC  { connect: [F1.B, U1.VDD] }
        net GND     { connect: [J1.GND, U1.GND, C_BAD.B, R_HOT.B, Q1.E] }
        net I2C_SDA { connect: [U1.SDA, J_I2C.SDA] }
        net I2C_SCL { connect: [U1.SCL, J_I2C.SCL] }
        net RELAY_SW { connect: [Q1.C, K1.B] }
        net BASE_DRV { connect: [U1.CTRL, Q1.B] }
    }

    verify {
        require all_required_pins_connected
        require ic_decoupling
        require i2c_pullups
        require inductive_flyback
        require bom_manufacturability
    }
}
"#;
    let ir = edl::compile_source("buggy_hw.edl", buggy).expect("compile buggy hw");
    assert!(ir.project.has_errors);

    let codes: Vec<&str> = ir.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert!(codes.contains(&"E_ERC_MISSING_DECOUPLING"), "codes: {codes:?}");
    assert!(codes.contains(&"E_ERC_MISSING_I2C_PULLUP"), "codes: {codes:?}");
    assert!(codes.contains(&"E_ERC_MISSING_FLYBACK"), "codes: {codes:?}");
    assert!(codes.contains(&"E_ERC_CAP_VOLTAGE_MARGIN"), "codes: {codes:?}");
    assert!(codes.contains(&"E_ERC_RESISTOR_POWER"), "codes: {codes:?}");

    for rule_id in [
        "E7_ic_decoupling",
        "E8_i2c_pullups",
        "E9_inductive_flyback",
        "E10_bom_manufacturability",
    ] {
        let ev = ir.evidence.iter().find(|e| e.check_id == rule_id).expect(rule_id);
        assert_eq!(ev.status, VerificationStatus::Violated, "{rule_id}: {}", ev.details);
    }
}
