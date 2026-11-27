//! Concise, copy-paste-ready AI & engineer language guides (`edl guide <domain>`) and
//! sub-50ms layout introspection (`edl inspect <file> --pins --zones --nets`).

use crate::core::ir::{CanonicalIr, EntityGeometryIr};
use std::fmt::Write;

/// Return a compact, idiomatic EDL v3.5.0 reference guide for the requested domain
/// (`elec`, `arch`, `plumbing`, `mep_elec`, `sld`, or `all`).
pub fn domain_guide(domain: Option<&str>) -> String {
    match domain.map(|s| s.to_lowercase()).as_deref() {
        Some("elec") | Some("electronics") | Some("kicad") => GUIDE_ELEC.to_string(),
        Some("arch") | Some("architecture") | Some("plan") => GUIDE_ARCH.to_string(),
        Some("plumbing") | Some("mep.plumbing") => GUIDE_PLUMBING.to_string(),
        Some("mep_elec") | Some("mep.elec") | Some("building_electrical") => GUIDE_MEP_ELEC.to_string(),
        Some("sld") | Some("elec.sld") | Some("power_sld") => GUIDE_SLD.to_string(),
        _ => format!(
            "{GUIDE_OVERVIEW}\n\n{GUIDE_ELEC}\n\n{GUIDE_ARCH}\n\n{GUIDE_PLUMBING}\n\n{GUIDE_MEP_ELEC}\n\n{GUIDE_SLD}"
        ),
    }
}

/// Format exact computed layout coordinates (components, pin tips, zones, dividers, and routed nets)
/// in < 50ms so AI models and engineers can inspect & refine placement without waiting for PNG renders.
pub fn format_layout_inspection(
    ir: &CanonicalIr,
    show_pins: bool,
    show_zones: bool,
    show_nets: bool,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "EDL Layout Inspection: {} (domain: {}, sheets: {}, components: {}, nets: {})",
        ir.project.name,
        ir.project.domain,
        ir.views.len().max(1),
        ir.project.component_count,
        ir.project.net_count
    );

    if show_zones && !ir.schematic_groups.is_empty() {
        let _ = writeln!(out, "\n=== ZONES ({}) ===", ir.schematic_groups.len());
        for g in &ir.schematic_groups {
            let sh = g.sheet.as_deref().unwrap_or("Main");
            let col = g.color.as_deref().unwrap_or("blue");
            let _ = writeln!(
                out,
                "  [{sh}] {} ({col}, {}): rect=({:.2}, {:.2})..({:.2}, {:.2}) size={:.1}x{:.1}mm members={}",
                g.name,
                g.border.as_deref().unwrap_or("dashed"),
                g.bbox.min_x,
                g.bbox.min_y,
                g.bbox.max_x,
                g.bbox.max_y,
                g.bbox.width(),
                g.bbox.depth(),
                g.members.len()
            );
        }
    }

    if show_zones && !ir.schematic_dividers.is_empty() {
        let _ = writeln!(out, "\n=== DIVIDERS ({}) ===", ir.schematic_dividers.len());
        for dv in &ir.schematic_dividers {
            let sh = dv.sheet.as_deref().unwrap_or("Main");
            let _ = writeln!(
                out,
                "  [{sh}] {} ({}): ({:.2}, {:.2}) -> ({:.2}, {:.2})",
                dv.label.as_deref().unwrap_or("divider"),
                dv.color.as_deref().unwrap_or("red"),
                dv.from.x,
                dv.from.y,
                dv.to.x,
                dv.to.y
            );
        }
    }

    if show_pins {
        let _ = writeln!(out, "\n=== COMPONENTS & PIN TIPS ===");
        for ent in &ir.entities {
            if let EntityGeometryIr::Component {
                lib_id,
                orient_deg,
                mirror,
                symbol_origin,
                body_rect,
                pins,
                ..
            } = &ent.geometry
            {
                let sh = ent
                    .props
                    .get("sheet")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Main");
                let c = symbol_origin.unwrap_or_else(|| body_rect.center());
                let mir = mirror
                    .as_deref()
                    .map(|m| format!(" mirror:{m}"))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "  [{sh}] {} ({}) center=({:.2}, {:.2}) orient:{orient_deg}{mir} body=({:.2},{:.2})..({:.2},{:.2})",
                    ent.id,
                    lib_id.as_deref().unwrap_or("synth"),
                    c.x,
                    c.y,
                    body_rect.min_x,
                    body_rect.min_y,
                    body_rect.max_x,
                    body_rect.max_y
                );
                let pin_strs: Vec<String> = pins
                    .iter()
                    .map(|p| {
                        let code = p.pin_code.as_deref().unwrap_or("");
                        let tag = if code.is_empty() || code == p.name {
                            p.name.clone()
                        } else {
                            format!("{}({})", p.name, code)
                        };
                        format!(
                            "{tag}@({:.2},{:.2})[{:?}]",
                            p.tip_point.x, p.tip_point.y, p.side
                        )
                    })
                    .collect();
                let _ = writeln!(out, "      pins: {}", pin_strs.join(", "));
            }
        }
    }

    if show_nets && !ir.schematic_routes.is_empty() {
        let _ = writeln!(out, "\n=== ROUTED NETS ({}) ===", ir.schematic_routes.len());
        for rt in &ir.schematic_routes {
            let sh = rt.sheet.as_deref().unwrap_or("Main");
            let (mut x0, mut y0, mut x1, mut y1) =
                (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for s in &rt.segments {
                x0 = x0.min(s.p0.x).min(s.p1.x);
                y0 = y0.min(s.p0.y).min(s.p1.y);
                x1 = x1.max(s.p0.x).max(s.p1.x);
                y1 = y1.max(s.p0.y).max(s.p1.y);
            }
            for t in &rt.terminals {
                x0 = x0.min(t.pos.x);
                y0 = y0.min(t.pos.y);
                x1 = x1.max(t.pos.x);
                y1 = y1.max(t.pos.y);
            }
            let bbox_str = if x0.is_finite() {
                format!("bbox=({x0:.1},{y0:.1})..({x1:.1},{y1:.1})")
            } else {
                "unrouted".to_string()
            };
            let _ = writeln!(
                out,
                "  [{sh}] {} style={} segs={} juncs={} terms={} {bbox_str}",
                rt.net_id,
                rt.style,
                rt.segments.len(),
                rt.junctions.len(),
                rt.terminals.len()
            );
        }
    }

    if !ir.diagnostics.is_empty() {
        let _ = writeln!(out, "\n=== DIAGNOSTICS ({}) ===", ir.diagnostics.len());
        for d in &ir.diagnostics {
            let _ = writeln!(out, "  [{:?}] {}: {}", d.severity, d.code, d.message);
            if let Some(ref s) = d.suggestion {
                let _ = writeln!(out, "      fix: {s}");
            }
        }
    }

    out
}

const GUIDE_OVERVIEW: &str = r#"# EDL v3.5.0 Quick Guide (Engineering Description Language)
CLI workflow:
  edl guide <elec|arch|plumbing|mep_elec|sld>  # Domain cheat sheet
  edl symbols search "<query>"                 # Search real KiCad 9 symbols
  edl symbols inspect "<Lib:Part>"             # Exact pin names, roles & default footprint
  edl build <file.edl>                         # Compile + run all physics/ERC/layout proofs
  edl inspect <file.edl> --pins --zones --nets # Exact placed coords & pin tips (< 50ms)
  edl export <file.edl> --kicad out.kicad_sch  # Single-sheet KiCad 9 schematic
  edl export <file.edl> --kicad-dir out_dir/   # Multi-sheet hierarchical KiCad 9 project
  edl export <file.edl> --dxf out.dxf          # Layered AutoCAD R12 DXF
  edl preview <file.edl> --layers arch,elec -o out.svg"#;

const GUIDE_ELEC: &str = r#"## Domain: Electronics Schematics (`elec` -> KiCad 9, 0 ERC errors/warnings)
Philosophy (Rust for Schematics): EDL gives you TOTAL freedom over sheet layout (`rect:`, `place ... center`),
while strictly enforcing non-negotiables: E1-E10 ERC/isolation/decoupling/I2C/flyback/BOM, E11 sheet/title-block fit,
E12 zero component overlap, E13/E14 80% DC voltage & thermal power derating, E15 barrier divider non-crossing, E16 real KiCad footprint check.

```edl
project "Industrial_Controller" {
  units: mm
  frame Sheet: Frame2

  circuit Main {
    component J_PWR: "Connector:Screw_Terminal_01x02" { value: "24V_IN"  footprint: "TerminalBlock_Phoenix:TerminalBlock_Phoenix_MKDS-1,5-2-5.08_1x02_P5.08mm_Horizontal" }
    component F1:    "Device:Polyfuse" { value: "1.5A"  footprint: "Fuse:Fuse_1812_4532Metric" }
    component C1:    "Device:C" { value: 100nF  voltage_rating: 50V  footprint: "Capacitor_SMD:C_0603_1608Metric" }
    component U_REG: "Regulator_Linear:AMS1117-3.3" {
      value: "AMS1117-3.3"
      pin VI: power_in(4.5V..15V)
      pin VO: power_out(3.3V)
    }
    net "+24V"   { connect: [J_PWR.1, F1.1] power_flag: true } // Use standard KiCad power names: "+24V", "+5V", "+3V3", "GND"
    net IN_FUSED { connect: [F1.2, C1.1, U_REG.VI] }           // Non-power intermediate nets avoid fake power:* symbols
    net "+3V3"   { connect: [U_REG.VO] }
    net GND      { connect: [J_PWR.2, C1.2, U_REG.GND] }
  }

  // Multi-sheet: declare multiple `schematic SheetName { ... }` blocks; export with `--kicad-dir out/`!
  schematic Power {
    sheet: A2 landscape
    title: "Sheet 1: Power Supply"
    routing { default: zone_local  power: symbol  ground: symbol }
    group Stage1 {
      title: "STAGE 1: 24V INPUT & REGULATOR"
      rect: (17.78mm, 17.78mm, 314.96mm, 165.10mm) // Obeyed literally — NEVER re-packed!
      border: dashed
      color: blue                                  // blue | red | amber | green | slate
      members: [J_PWR, F1, C1, U_REG]
      note: "All caps >= 1.25x derated | Vout = 3.3V"
    }
    // Optional galvanic isolation barrier (E15 proves zero routed wires cross this line!)
    // divider { from: (310mm, 285mm) to: (310mm, 386mm) label: "ISOLATION BARRIER" color: red }

    // Free placement by exact KiCad symbol center (2.54mm grid) — NEVER shifted by EDL!
    place J_PWR center (35.56mm, 63.50mm) orient: 180
    place F1    center (60.96mm, 63.50mm) orient: 90
    place C1    center (86.36mm, 88.90mm)
    place U_REG center (127.00mm, 63.50mm)
  }

  verify {
    require all_required_pins_connected
    require no_power_ground_short
    require no_multiple_power_drivers
    require voltage_compatibility
    require no_single_pin_nets
    require ic_decoupling
    require bom_manufacturability
    // require galvanic_isolation(GND, ISO_GND)
    // require i2c_pullups
    // require inductive_flyback
  }
}
```
Pro tips:
- Prefer root non-`extends` KiCad 9 symbols (check with `edl symbols inspect`) for 0 `lib_symbol_mismatch` warnings.
- Vertical shunt caps on `GND`/`+3V3` in the same group (`|dy| <= 20.32mm`, `dx <= 90mm`) auto-merge onto ONE shared horizontal bus!
- Cross-group & cross-sheet signal nets automatically render as crisp red-outlined KiCad `global_label` flags.
- Run `edl inspect file.edl --pins --zones --nets` to check exact pin tips & net boxes in < 50ms."#;

const GUIDE_ARCH: &str = r#"## Domain: Architectural Floor Plans (`arch` -> SVG + 19-layer DXF)
```edl
project "Clinic" {
  units: mm
  frame Plan: Frame2
  architecture.defaults { measurement_basis: clear_interior  exterior_wall: 200mm  interior_wall: 120mm }
  room Lab  { role: service  clear_size: (4200mm, 3440mm)  anchor: southwest at Plan.origin }
  room Exam { role: habitable  clear_size: (4460mm, 4280mm) }
  layout Plan {
    require Exam.west == Lab.west
    require Exam.south == Lab.north + interior_wall
  }
  opening Door1: door { host: shared_wall(Lab, Exam)  width: 900mm  offset: 400mm from start  hinge: start  swing: into Exam }
  opening Win1: window { host: exterior(Exam, north)  width: 1500mm  offset: 1200mm from start }
  fixture Desk: desk { room: Exam  size: (1400mm, 700mm)  against: east  offset: 500mm from start  clearance_front: 800mm }
  verify { require no_room_overlap  require opening_jamb_margin >= 100mm  require no_door_swing_collision  require all_rooms_reachable  require no_fixture_collision  require habitable_rooms_have_window }
}
```"#;

const GUIDE_PLUMBING: &str = r#"## Domain: Building Plumbing (`mep.plumbing` via `overlay` -> P1-P9 proofs + DXF + CSV)
Use `overlay "base_arch.edl"` so you NEVER duplicate architectural walls/rooms!
Enforces P1-P7 (supply, drainage slope, trap arm, velocity, valves, MEP clash, DFU headers) + P8 (`NO PLUMBING ZONE` in Elec/IT rooms) + P9 (zero pipe-vs-column clashes).
```edl
project "Clinic_Plumbing" {
  units: mm
  overlay "../b2_clinic_arch/clinic_arch.edl"
  building_plumbing ClinicPlumb {
    riser ColdMain: cold_riser { host: MechRoom.west  at: 400mm  dia: 32mm }
    riser SoilStack: soil_stack { host: PublicWC.west  at: 300mm  dia: 110mm }
    heater WH1: water_heater { host: MechRoom.south  at: 600mm  capacity_l: 300 }
    valve V_Main: isolation_valve { host: MechRoom.west  at: 700mm  serves: ColdMain  dia: 32mm }
    connect PublicWC_Toilet { cold: ColdMain  cold_dia: 15mm  soil: SoilStack  drain_dia: 110mm  slope: "1:60" }
  }
}
```"#;

const GUIDE_MEP_ELEC: &str = r#"## Domain: Building Electrical (`mep.elec` via `overlay` -> M1-M8 IEC 60364 proofs + DXF + CSV)
Use `overlay "base_arch.edl"`. Supports `phase: L1|L2|L3|3P`, IEC 60364-5-52 cable ampacity (`In <= Iz`), M7 3-phase balance (`<= 15%`), and M8 900x900mm DB working clearance.
```edl
project "Clinic_Electrical" {
  units: mm
  overlay "../b2_clinic_arch/clinic_arch.edl"
  building_electrical ClinicElec {
    supply: 230V 50Hz  ceiling_height: 2800mm
    panel DB_MAIN { host: wall(ElecITRoom, west)  offset: 1200mm from start  main_breaker: 100A  height: 1500mm }
    breaker L1_Core: lighting { from: DB_MAIN  rating: 10A  phase: L1  cable: 1.5mm2  rcd: 30mA  max_voltage_drop: 3% }
    breaker P1_Exam: socket   { from: DB_MAIN  rating: 16A  phase: L2  cable: 2.5mm2  rcd: 30mA  max_voltage_drop: 5% }
    light L_Exam1: ceiling { room: Exam1  at: center  load: 36W  circuit: L1_Core }
    outlet S_Exam1: duplex { host: wall(Exam1, north)  offset: 800mm from start  height: 300mm  load: 600W  circuit: P1_Exam }
  }
  verify { require breaker_load_ok  require voltage_drop_ok  require wet_zone_clearance  require switch_accessible  require electrical_device_clear  require wet_circuits_rcd_protected }
}
```"#;

const GUIDE_SLD: &str = r#"## Domain: Power Single-Line Diagrams (`elec.sld` -> S1-S6 IEC 60909 / 60364 proofs + SVG + DXF)
```edl
project "Hospital_SLD" {
  units: mm
  power_sld MainSwitchgear {
    standard: "IEC 60909 / IEC 60364 / IEC 61439"
    mv_intake RMU_11KV { utility_kv: 11  sk_mva: 250  vcb_a: 630  vcb_icu_ka: 25 }
    bus MSB_A { voltage_v: 400  system: "TN-S"  rating_a: 1600  icu_ka: 50  tier: main }
    bus MSB_B { voltage_v: 400  system: "TN-S"  rating_a: 1600  icu_ka: 50  tier: main }
    source TX1: transformer { rating_kva: 1000  uk_pct: 6.0  hv_kv: 11.0  lv_v: 400  vector: "Dyn11"  bus: MSB_A  incomer: Q_TX1  incomer_a: 1600  icu_ka: 50 }
    source TX2: transformer { rating_kva: 1000  uk_pct: 6.0  hv_kv: 11.0  lv_v: 400  vector: "Dyn11"  bus: MSB_B  incomer: Q_TX2  incomer_a: 1600  icu_ka: 50 }
    coupler Q_BT { from: MSB_A  to: MSB_B  rating_a: 1600  mode: ats_2_of_3 }
    load F1: motor { bus: MSB_A  power_kw: 200  pf: 0.85  breaker: QF1  kind: mccb  rating_a: 400  icu_ka: 50  cable: "Cu XLPE 3x240+120"  area_mm2: 240  parallel_runs: 1  length_m: 40 }
  }
}
```"#;
