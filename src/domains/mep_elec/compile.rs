use crate::core::geom::{Point2, QuarterDisk2, Rect2, Segment2, Side, EPS_MM};
use crate::core::ir::*;
use crate::core::span::{Diagnostic, Severity};
use crate::domains::arch::ast::{OpeningPlacement, WallEnd};
use crate::domains::arch::compile::decompose_room_rect;
use crate::domains::mep_elec::ast::*;
use crate::syntax::ast::{ProjectAst, VerifyRule};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const COPPER_RESISTIVITY_OHM_MM2_PER_M: f64 = 0.0175;

#[derive(Debug, Clone)]
struct ArchRoomInfo {
    rect: Rect2,
    notch: Option<Rect2>,
    subrects: Vec<Rect2>,
    role: String,
}

#[derive(Debug, Clone)]
struct ArchDoorInfo {
    cutout: Rect2,
    swing_arc: crate::core::geom::Arc2,
    quarter_disk: QuarterDisk2,
    is_horizontal: bool,
}

#[derive(Debug, Clone)]
struct ArchWetFixture {
    id: String,
    room: String,
    rect: Rect2,
}

#[derive(Debug, Clone)]
struct ResolvedDevice {
    id: String,
    device_kind: String, // "panel" | "light" | "switch" | "outlet"
    subtype: String,
    room: String,
    pos: Point2,
    facing: Option<Side>,
    height_mm: f64,
    load_w: f64,
    circuit: Option<String>,
    ip_rating: Option<String>,
    controls: Vec<String>,
    span: crate::core::span::Span,
}

/// Multi-domain compiler stage: layers Building Electrical (`mep.elec`) directly on top of
/// an already-solved Architectural Floor Plan inside the SAME Canonical IR!
pub fn compile_building_electrical(project: &ProjectAst, ir: &mut CanonicalIr) {
    if project.mep_elec.installations.is_empty() {
        return;
    }

    // Mark IR as multi-domain
    if ir.project.domain == "architecture" {
        ir.project.domain = "arch+mep.elec".to_string();
    } else {
        ir.project.domain = "mep.elec".to_string();
    }
    let default_frame = ir
        .frames
        .first()
        .map(|f| f.id.clone())
        .unwrap_or_else(|| "Plan".to_string());

    // 1. Extract solved architectural geometry from `ir`
    let mut rooms: BTreeMap<String, ArchRoomInfo> = BTreeMap::new();
    let mut doors: BTreeMap<String, ArchDoorInfo> = BTreeMap::new();
    let mut opening_cutouts: Vec<(String, Rect2)> = Vec::new();
    let mut wet_fixtures: Vec<ArchWetFixture> = Vec::new();

    for e in &ir.entities {
        match &e.geometry {
            EntityGeometryIr::Rect {
                min,
                max,
                notch_rect,
                ..
            } if e.kind == "arch.room" => {
                let rect = Rect2::new(min[0], min[1], max[0], max[1]);
                let subrects = decompose_room_rect(rect, *notch_rect);
                let role = e
                    .props
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                rooms.insert(
                    e.id.clone(),
                    ArchRoomInfo {
                        rect,
                        notch: *notch_rect,
                        subrects,
                        role,
                    },
                );
            }
            EntityGeometryIr::Door {
                cutout_rect,
                swing_arc,
                ..
            } => {
                opening_cutouts.push((e.id.clone(), *cutout_rect));
                doors.insert(
                    e.id.clone(),
                    ArchDoorInfo {
                        cutout: *cutout_rect,
                        swing_arc: *swing_arc,
                        quarter_disk: QuarterDisk2::from_arc(*swing_arc),
                        is_horizontal: cutout_rect.width() >= cutout_rect.depth(),
                    },
                );
            }
            EntityGeometryIr::Window { cutout_rect, .. }
            | EntityGeometryIr::Passage { cutout_rect, .. } => {
                opening_cutouts.push((e.id.clone(), *cutout_rect));
            }
            EntityGeometryIr::Fixture {
                category,
                body_rect,
                ..
            } => {
                let low = category.to_lowercase();
                if matches!(low.as_str(), "shower" | "bath" | "bathtub" | "tub") {
                    let room = e
                        .props
                        .get("room")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    wet_fixtures.push(ArchWetFixture {
                        id: e.id.clone(),
                        room,
                        rect: *body_rect,
                    });
                }
            }
            _ => {}
        }
    }

    let mut all_devices: Vec<ResolvedDevice> = Vec::new();
    let mut device_map: BTreeMap<String, usize> = BTreeMap::new();
    let mut panels_main_a: BTreeMap<String, (f64, crate::core::span::Span)> = BTreeMap::new();
    let mut all_circuits: Vec<MepCircuitIr> = Vec::new();
    let mut all_wires: Vec<MepWireRunIr> = Vec::new();
    let mut circuit_spans: BTreeMap<String, crate::core::span::Span> = BTreeMap::new();

    // 2. Resolve every installation block
    for inst in &project.mep_elec.installations {
        let supply_v = inst.supply_v.to_volts().unwrap_or(230.0);
        let ceiling_h_mm = inst.ceiling_height.to_mm().unwrap_or(2700.0);

        // 2a. Panels
        for p in &inst.panels {
            let main_a = p.main_breaker.to_amps().unwrap_or(63.0);
            let h_mm = p.height.to_mm().unwrap_or(1500.0);
            match resolve_mount(&p.mount, None, &rooms, &doors) {
                Ok((room, pos, facing)) => {
                    panels_main_a.insert(p.id.clone(), (main_a, p.span.clone()));
                    let idx = all_devices.len();
                    device_map.insert(p.id.clone(), idx);
                    all_devices.push(ResolvedDevice {
                        id: p.id.clone(),
                        device_kind: "panel".to_string(),
                        subtype: "main_db".to_string(),
                        room,
                        pos,
                        facing: Some(facing),
                        height_mm: h_mm,
                        load_w: 0.0,
                        circuit: None,
                        ip_rating: None,
                        controls: Vec::new(),
                        span: p.span.clone(),
                    });
                }
                Err(msg) => {
                    ir.diagnostics.push(
                        Diagnostic::error("E_MEP_MOUNT", "mep.elec", format!("Panel '{}': {msg}", p.id), Some(p.span.clone()))
                            .with_entities(vec![p.id.clone()]),
                    );
                }
            }
        }

        // 2b. Lights
        for l in &inst.lights {
            let load_w = l.load.to_watts().unwrap_or(18.0);
            let Some(rinfo) = rooms.get(&l.room) else {
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_REF_UNKNOWN_ROOM",
                        "mep.elec",
                        format!("Light '{}' references unknown room '{}'", l.id, l.room),
                        Some(l.span.clone()),
                    )
                    .with_entities(vec![l.id.clone()]),
                );
                continue;
            };
            let pos = match &l.position {
                RoomPointSpec::Center => {
                    // Place in largest clear sub-rectangle (handles L-shaped notched rooms cleanly!)
                    rinfo
                        .subrects
                        .iter()
                        .max_by(|a, b| a.area_m2().partial_cmp(&b.area_m2()).unwrap())
                        .copied()
                        .unwrap_or(rinfo.rect)
                        .center()
                }
                RoomPointSpec::OffsetXY { x, y } => {
                    let dx = x.to_mm().unwrap_or(0.0);
                    let dy = y.to_mm().unwrap_or(0.0);
                    Point2::new(rinfo.rect.min_x + dx, rinfo.rect.min_y + dy)
                }
            };
            let idx = all_devices.len();
            device_map.insert(l.id.clone(), idx);
            all_devices.push(ResolvedDevice {
                id: l.id.clone(),
                device_kind: "light".to_string(),
                subtype: l.subtype.clone(),
                room: l.room.clone(),
                pos,
                facing: None,
                height_mm: ceiling_h_mm,
                load_w,
                circuit: Some(l.circuit.clone()),
                ip_rating: l.ip_rating.clone(),
                controls: Vec::new(),
                span: l.span.clone(),
            });
        }

        // 2c. Switches
        for sw in &inst.switches {
            let h_mm = sw.height.to_mm().unwrap_or(1200.0);
            match resolve_mount(&sw.mount, Some(&sw.room), &rooms, &doors) {
                Ok((room, pos, facing)) => {
                    let idx = all_devices.len();
                    device_map.insert(sw.id.clone(), idx);
                    all_devices.push(ResolvedDevice {
                        id: sw.id.clone(),
                        device_kind: "switch".to_string(),
                        subtype: sw.subtype.clone(),
                        room,
                        pos,
                        facing: Some(facing),
                        height_mm: h_mm,
                        load_w: 0.0,
                        circuit: Some(sw.circuit.clone()),
                        ip_rating: sw.ip_rating.clone(),
                        controls: sw.controls.clone(),
                        span: sw.span.clone(),
                    });
                }
                Err(msg) => {
                    ir.diagnostics.push(
                        Diagnostic::error("E_MEP_MOUNT", "mep.elec", format!("Switch '{}': {msg}", sw.id), Some(sw.span.clone()))
                            .with_entities(vec![sw.id.clone()]),
                    );
                }
            }
        }

        // 2d. Outlets / Sockets
        for out in &inst.outlets {
            let h_mm = out.height.to_mm().unwrap_or(300.0);
            let load_w = out.load.to_watts().unwrap_or(300.0);
            match resolve_mount(&out.mount, None, &rooms, &doors) {
                Ok((room, pos, facing)) => {
                    let idx = all_devices.len();
                    device_map.insert(out.id.clone(), idx);
                    all_devices.push(ResolvedDevice {
                        id: out.id.clone(),
                        device_kind: "outlet".to_string(),
                        subtype: out.subtype.clone(),
                        room,
                        pos,
                        facing: Some(facing),
                        height_mm: h_mm,
                        load_w,
                        circuit: Some(out.circuit.clone()),
                        ip_rating: out.ip_rating.clone(),
                        controls: Vec::new(),
                        span: out.span.clone(),
                    });
                }
                Err(msg) => {
                    ir.diagnostics.push(
                        Diagnostic::error("E_MEP_MOUNT", "mep.elec", format!("Outlet '{}': {msg}", out.id), Some(out.span.clone()))
                            .with_entities(vec![out.id.clone()]),
                    );
                }
            }
        }

        // 2e. Build breaker circuits, route orthogonal daisy-chains, compute run lengths & voltage drops!
        for br in &inst.breakers {
            circuit_spans.insert(br.id.clone(), br.span.clone());
            let rating_a = br.rating.to_amps().unwrap_or(16.0);
            let cable_mm2 = br.cable.to_mm2().unwrap_or(2.5);
            let rcd_ma = br.rcd.and_then(|q| q.to_amps().ok()).map(|a| a * 1000.0);
            let max_vdrop = br.max_voltage_drop_pct.unwrap_or_else(|| {
                if br.kind == "lighting" {
                    3.0
                } else {
                    5.0
                }
            });

            let panel_dev = device_map.get(&br.from_panel).map(|&i| &all_devices[i]);
            if panel_dev.is_none() {
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_MEP_UNKNOWN_PANEL",
                        "mep.elec",
                        format!("Breaker '{}' references unknown panel '{}'", br.id, br.from_panel),
                        Some(br.span.clone()),
                    )
                    .with_entities(vec![br.id.clone(), br.from_panel.clone()]),
                );
            }

            let members: Vec<&ResolvedDevice> = all_devices
                .iter()
                .filter(|d| d.circuit.as_deref() == Some(br.id.as_str()))
                .collect();

            let total_load_w: f64 = members.iter().map(|d| d.load_w).sum();
            let design_current_a = if supply_v > 0.0 {
                total_load_w / supply_v
            } else {
                0.0
            };

            // Route nearest-neighbor Manhattan daisy-chain from DB panel across circuit load devices
            let mut total_run_mm = 0.0_f64;
            if let Some(panel) = panel_dev {
                // Vertical riser from DB up to ceiling distribution plane
                total_run_mm += (ceiling_h_mm - panel.height_mm).abs();

                let mut remaining: Vec<&ResolvedDevice> = members
                    .iter()
                    .copied()
                    .filter(|d| d.device_kind != "switch")
                    .collect();
                let mut cur_id = panel.id.clone();
                let mut cur_pos = panel.pos;
                let run_kind = if br.kind == "lighting" {
                    "lighting"
                } else {
                    "power"
                };

                while !remaining.is_empty() {
                    let (best_idx, _) = remaining
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            let da = (a.pos.x - cur_pos.x).abs() + (a.pos.y - cur_pos.y).abs();
                            let db = (b.pos.x - cur_pos.x).abs() + (b.pos.y - cur_pos.y).abs();
                            da.partial_cmp(&db).unwrap()
                        })
                        .unwrap();
                    let next = remaining.remove(best_idx);
                    let segs = ortho_l_route(cur_pos, next.pos);
                    let h_len: f64 = segs.iter().map(|s| s.length()).sum();
                    // Add vertical drop from ceiling down to device height (0 for ceiling lights!)
                    let v_drop = (ceiling_h_mm - next.height_mm).abs();
                    total_run_mm += h_len + v_drop;

                    all_wires.push(MepWireRunIr {
                        circuit_id: br.id.clone(),
                        run_kind: run_kind.to_string(),
                        from_id: cur_id,
                        to_id: next.id.clone(),
                        segments: segs,
                    });
                    cur_id = next.id.clone();
                    cur_pos = next.pos;
                }
            }

            let run_length_m = total_run_mm / 1000.0;
            // Exact single-phase copper voltage drop: dV% = (2 * rho * L * I) / (A * V) * 100
            let voltage_drop_pct = if cable_mm2 > 0.0 && supply_v > 0.0 {
                (2.0 * COPPER_RESISTIVITY_OHM_MM2_PER_M * run_length_m * design_current_a
                    / (cable_mm2 * supply_v))
                    * 100.0
            } else {
                0.0
            };

            all_circuits.push(MepCircuitIr {
                id: br.id.clone(),
                kind: br.kind.clone(),
                panel_id: br.from_panel.clone(),
                supply_v,
                rating_a,
                cable_mm2,
                rcd_ma,
                max_voltage_drop_pct: max_vdrop,
                connected_devices: members.iter().map(|d| d.id.clone()).collect(),
                total_load_w: round2(total_load_w),
                design_current_a: round2(design_current_a),
                run_length_m: round2(run_length_m),
                voltage_drop_pct: round2(voltage_drop_pct),
                status: "OK".to_string(),
            });
        }
    }

    // 2f. Emit switch-leg control lines from every switch to its controlled lights
    let mut controlled_lights: BTreeSet<String> = BTreeSet::new();
    for sw in all_devices.iter().filter(|d| d.device_kind == "switch") {
        for lid in &sw.controls {
            if let Some(&l_idx) = device_map.get(lid) {
                let light = &all_devices[l_idx];
                controlled_lights.insert(lid.clone());
                all_wires.push(MepWireRunIr {
                    circuit_id: sw.circuit.clone().unwrap_or_default(),
                    run_kind: "switch_leg".to_string(),
                    from_id: sw.id.clone(),
                    to_id: light.id.clone(),
                    segments: vec![Segment2::new(sw.pos, light.pos)],
                });
            } else {
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_MEP_UNKNOWN_LIGHT",
                        "mep.elec",
                        format!("Switch '{}' controls unknown light '{}'", sw.id, lid),
                        Some(sw.span.clone()),
                    )
                    .with_entities(vec![sw.id.clone(), lid.clone()]),
                );
            }
        }
    }

    // 3. Emit MepDevice entities into the shared Canonical IR
    for d in &all_devices {
        let mut props = BTreeMap::new();
        props.insert("room".to_string(), json!(d.room));
        props.insert("device_kind".to_string(), json!(d.device_kind));
        props.insert("subtype".to_string(), json!(d.subtype));
        props.insert("height_mm".to_string(), json!(round2(d.height_mm)));
        if d.load_w > 0.0 {
            props.insert("load_w".to_string(), json!(round2(d.load_w)));
        }
        if let Some(ref cid) = d.circuit {
            props.insert("circuit".to_string(), json!(cid));
        }
        if let Some(ref ip) = d.ip_rating {
            props.insert("ip".to_string(), json!(ip));
        }
        if !d.controls.is_empty() {
            props.insert("controls".to_string(), json!(d.controls));
        }
        ir.entities.push(EntityIr {
            id: d.id.clone(),
            kind: format!("mep.elec.{}", d.device_kind),
            frame: default_frame.clone(),
            span: Some(d.span.clone()),
            provenance: None,
            props,
            geometry: EntityGeometryIr::MepDevice {
                device_kind: d.device_kind.clone(),
                subtype: d.subtype.clone(),
                pos: d.pos,
                facing: d.facing,
                room: d.room.clone(),
                height_mm: round2(d.height_mm),
                load_w: round2(d.load_w),
                circuit: d.circuit.clone(),
            },
        });
    }

    // 4. Execute Building Electrical Verification Rules (M1..M6)
    // Run all declared MEP rules, or default to running all 6 IEC 60364 / NEC safety checks automatically!
    let mut run_m1 = false;
    let mut run_m2 = false;
    let mut run_m3 = false;
    let mut run_m4 = false;
    let mut run_m5 = false;
    let mut run_m6 = false;
    for r in &project.verifications {
        match r {
            VerifyRule::BreakerLoadOk { .. } => run_m1 = true,
            VerifyRule::VoltageDropOk { .. } => run_m2 = true,
            VerifyRule::WetZoneClearance { .. } => run_m3 = true,
            VerifyRule::SwitchAccessible { .. } => run_m4 = true,
            VerifyRule::ElectricalDeviceClear { .. } => run_m5 = true,
            VerifyRule::WetCircuitsRcdProtected { .. } => run_m6 = true,
            _ => {}
        }
    }
    if !(run_m1 || run_m2 || run_m3 || run_m4 || run_m5 || run_m6) {
        run_m1 = true;
        run_m2 = true;
        run_m3 = true;
        run_m4 = true;
        run_m5 = true;
        run_m6 = true;
    }

    // M1: Breaker Load Capacity (80% continuous load rule + Main DB capacity)
    if run_m1 {
        let mut viols = 0usize;
        let mut panel_loads_a: BTreeMap<String, f64> = BTreeMap::new();
        for c in &mut all_circuits {
            *panel_loads_a.entry(c.panel_id.clone()).or_insert(0.0) += c.design_current_a;
            let max_cont = c.rating_a * 0.80;
            if c.design_current_a > max_cont + 1e-4 {
                viols += 1;
                c.status = "OVERLOAD".to_string();
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_MEP_BREAKER_OVERLOAD",
                        "mep.elec",
                        format!(
                            "Circuit '{}' load {:.2}A ({:.0}W) exceeds 80% continuous limit ({:.1}A) of {:.0}A breaker",
                            c.id, c.design_current_a, c.total_load_w, max_cont, c.rating_a
                        ),
                        circuit_spans.get(&c.id).cloned(),
                    )
                    .with_entities(vec![c.id.clone()])
                    .with_measurement(c.design_current_a, round2(max_cont), "A")
                    .with_suggestion(format!(
                        "Split loads on '{}' across two circuits or increase breaker rating & cable gauge",
                        c.id
                    )),
                );
            }
        }
        for (pid, total_a) in &panel_loads_a {
            if let Some(&(main_a, ref pspan)) = panels_main_a.get(pid) {
                if *total_a > main_a + 1e-4 {
                    viols += 1;
                    ir.diagnostics.push(
                        Diagnostic::error(
                            "E_MEP_PANEL_OVERLOAD",
                            "mep.elec",
                            format!("Panel '{}' total demand {:.2}A exceeds main breaker {:.0}A", pid, total_a, main_a),
                            Some(pspan.clone()),
                        )
                        .with_entities(vec![pid.clone()])
                        .with_measurement(round2(*total_a), main_a, "A"),
                    );
                }
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "M1_breaker_load_capacity".to_string(),
            status: if viols == 0 {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "iec60364_circuit_load_80pct_v0.7".to_string(),
            tolerance_mm: None,
            subjects: all_circuits.iter().map(|c| c.id.clone()).collect(),
            details: if viols == 0 {
                format!("All {} circuit(s) within 80% continuous breaker rating", all_circuits.len())
            } else {
                format!("{viols} breaker/panel overload violation(s)")
            },
        });
    }

    // M2: Copper Voltage Drop Limit
    if run_m2 {
        let mut viols = 0usize;
        for c in &mut all_circuits {
            if c.voltage_drop_pct > c.max_voltage_drop_pct + 1e-4 {
                viols += 1;
                if c.status == "OK" {
                    c.status = "VDROP_EXCEEDED".to_string();
                }
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_MEP_VOLTAGE_DROP",
                        "mep.elec",
                        format!(
                            "Circuit '{}' voltage drop {:.2}% exceeds limit {:.1}% (run {:.1}m, {:.2}A on {:.1}mm2 Cu)",
                            c.id, c.voltage_drop_pct, c.max_voltage_drop_pct, c.run_length_m, c.design_current_a, c.cable_mm2
                        ),
                        circuit_spans.get(&c.id).cloned(),
                    )
                    .with_entities(vec![c.id.clone()])
                    .with_measurement(c.voltage_drop_pct, c.max_voltage_drop_pct, "%")
                    .with_suggestion(format!("Increase cable cross-section on '{}' (e.g. next standard mm2 size)", c.id)),
                );
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "M2_voltage_drop_limit".to_string(),
            status: if viols == 0 {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "iec60364_copper_voltage_drop_3d_run_v0.7".to_string(),
            tolerance_mm: None,
            subjects: all_circuits.iter().map(|c| c.id.clone()).collect(),
            details: if viols == 0 {
                let worst = all_circuits
                    .iter()
                    .map(|c| c.voltage_drop_pct)
                    .fold(0.0_f64, f64::max);
                format!("All {} circuit(s) within voltage drop limit (worst: {:.2}%)", all_circuits.len(), worst)
            } else {
                format!("{viols} voltage drop violation(s)")
            },
        });
    }

    // M3: IEC 60364-7-701 Bathroom Wet-Zone Clearance (600mm around shower/tub)
    if run_m3 {
        let mut viols = 0usize;
        for d in &all_devices {
            if d.device_kind == "light" && d.height_mm >= 2250.0 {
                continue; // Ceiling luminaires above Zone 2 (2.25m) are permitted
            }
            let ip_ok = d
                .ip_rating
                .as_deref()
                .map(|s| s.to_uppercase().contains("IP44") || s.to_uppercase().contains("IP65") || s.to_uppercase().contains("IPX4"))
                .unwrap_or(false)
                || d.subtype.eq_ignore_ascii_case("shaver");
            if ip_ok {
                continue;
            }
            for wf in &wet_fixtures {
                if wf.room != d.room {
                    continue;
                }
                let dist = point_to_rect_distance(d.pos, wf.rect);
                if dist < 600.0 - EPS_MM {
                    viols += 1;
                    ir.diagnostics.push(
                        Diagnostic::error(
                            "E_MEP_WET_ZONE",
                            "mep.elec",
                            format!(
                                "{} '{}' in '{}' is {:.0}mm from '{}' (IEC 60364 Zone 2 requires >= 600mm clearance)",
                                d.device_kind, d.id, d.room, dist, wf.id
                            ),
                            Some(d.span.clone()),
                        )
                        .with_entities(vec![d.id.clone(), wf.id.clone()])
                        .with_measurement(round2(dist), 600.0, "mm")
                        .with_suggestion(format!(
                            "Move '{}' at least {:.0}mm further from '{}', place switch outside '{}', or specify IP44/shaver rating",
                            d.id, 600.0 - dist, wf.id, d.room
                        )),
                    );
                }
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "M3_wet_zone_clearance".to_string(),
            status: if viols == 0 {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "iec60364_7_701_zone2_600mm_v0.7".to_string(),
            tolerance_mm: Some(EPS_MM),
            subjects: wet_fixtures.iter().map(|w| w.id.clone()).collect(),
            details: if viols == 0 {
                format!(
                    "All electrical devices satisfy >= 600mm wet-zone clearance across {} wet fixture(s)",
                    wet_fixtures.len()
                )
            } else {
                format!("{viols} IEC 60364 wet-zone clearance violation(s)")
            },
        });
    }

    // M4: Switch Accessibility (never behind door swing!) & Every Light Switched
    if run_m4 {
        let mut viols = 0usize;
        for d in &all_devices {
            if d.device_kind == "switch" {
                for (door_id, dinfo) in &doors {
                    // Check if switch point sits strictly inside a door's open swing quarter-disk
                    let sw_box = Rect2::new(d.pos.x - 30.0, d.pos.y - 30.0, d.pos.x + 30.0, d.pos.y + 30.0);
                    if dinfo.quarter_disk.overlaps_rect(sw_box) {
                        // Ignore if switch is on the latch side just touching the open circle's edge
                        let dist_to_hinge = d.pos.dist(dinfo.swing_arc.center);
                        if dist_to_hinge < dinfo.swing_arc.radius - 15.0 {
                            viols += 1;
                            ir.diagnostics.push(
                                Diagnostic::error(
                                    "E_MEP_SWITCH_BEHIND_DOOR",
                                    "mep.elec",
                                    format!("Switch '{}' falls inside the swing arc of door '{}'", d.id, door_id),
                                    Some(d.span.clone()),
                                )
                                .with_entities(vec![d.id.clone(), door_id.clone()])
                                .with_suggestion(format!(
                                    "Mount '{}' using `host: beside({}, latch_side, 150mm)` to keep it clear of the door leaf",
                                    d.id, door_id
                                )),
                            );
                        }
                    }
                }
            }
            let is_life_safety_unswitched = matches!(
                d.subtype.to_lowercase().as_str(),
                "exit" | "exit_sign" | "emergency" | "emergency_light"
            );
            if d.device_kind == "light" && !is_life_safety_unswitched && !controlled_lights.contains(&d.id) {
                viols += 1;
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_MEP_UNSWITCHED_LIGHT",
                        "mep.elec",
                        format!("Light '{}' in room '{}' has no switch controlling it", d.id, d.room),
                        Some(d.span.clone()),
                    )
                    .with_entities(vec![d.id.clone()])
                    .with_suggestion(format!("Add '{}' to a switch's `controls: [...]` list", d.id)),
                );
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "M4_switch_door_accessibility".to_string(),
            status: if viols == 0 {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "exact_door_arc_and_switch_graph_v0.7".to_string(),
            tolerance_mm: Some(EPS_MM),
            subjects: all_devices
                .iter()
                .filter(|d| d.device_kind == "switch" || d.device_kind == "light")
                .map(|d| d.id.clone())
                .collect(),
            details: if viols == 0 {
                "All switches clear of door swing arcs and all lights controlled by >= 1 switch".to_string()
            } else {
                format!("{viols} switch accessibility / unswitched light violation(s)")
            },
        });
    }

    // M5: Device Wall Clearance (never inside a door/window/passage cutout!)
    if run_m5 {
        let mut viols = 0usize;
        for d in &all_devices {
            if d.device_kind == "light" {
                continue;
            }
            let dev_box = Rect2::new(d.pos.x - 40.0, d.pos.y - 40.0, d.pos.x + 40.0, d.pos.y + 40.0);
            for (oid, orect) in &opening_cutouts {
                // Only flag windows if device height could conflict (standard outlets at 300mm sit safely below 900mm window sills!)
                let is_door_or_passage = doors.contains_key(oid)
                    || ir
                        .entities
                        .iter()
                        .any(|e| e.id == *oid && e.kind == "arch.passage");
                if !is_door_or_passage && d.height_mm < 850.0 {
                    continue;
                }
                let expanded = Rect2::new(
                    orect.min_x - 20.0,
                    orect.min_y - 20.0,
                    orect.max_x + 20.0,
                    orect.max_y + 20.0,
                );
                if dev_box.overlaps_strict(expanded) {
                    viols += 1;
                    ir.diagnostics.push(
                        Diagnostic::error(
                            "E_MEP_DEVICE_IN_OPENING",
                            "mep.elec",
                            format!("{} '{}' collides with opening '{}'", d.device_kind, d.id, oid),
                            Some(d.span.clone()),
                        )
                        .with_entities(vec![d.id.clone(), oid.clone()])
                        .with_suggestion(format!("Adjust offset of '{}' clear of opening '{}'", d.id, oid)),
                    );
                }
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "M5_device_wall_clearance".to_string(),
            status: if viols == 0 {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "exact_opening_cutout_clearance_v0.7".to_string(),
            tolerance_mm: Some(EPS_MM),
            subjects: all_devices.iter().map(|d| d.id.clone()).collect(),
            details: if viols == 0 {
                format!("All {} wall-mounted device(s) clear of door/passage/window openings", all_devices.len())
            } else {
                format!("{viols} device-in-opening collision(s)")
            },
        });
    }

    // M6: RCD / GFCI Protection on Socket, Appliance & Wet-Room Circuits (<= 30mA)
    if run_m6 {
        let mut viols = 0usize;
        for c in &mut all_circuits {
            let serves_wet_or_kitchen = c.connected_devices.iter().any(|did| {
                device_map.get(did).map(|&i| {
                    let r = &all_devices[i].room;
                    rooms
                        .get(r)
                        .map(|ri| ri.role == "wet" || ri.role == "service")
                        .unwrap_or(false)
                }).unwrap_or(false)
            });
            let needs_rcd = matches!(c.kind.as_str(), "socket" | "appliance") || serves_wet_or_kitchen;
            if needs_rcd {
                let rcd_ok = c.rcd_ma.map(|m| m > 0.0 && m <= 30.0 + 1e-4).unwrap_or(false);
                if !rcd_ok {
                    viols += 1;
                    if c.status == "OK" {
                        c.status = "RCD_MISSING".to_string();
                    }
                    ir.diagnostics.push(
                        Diagnostic::error(
                            "E_MEP_RCD_REQUIRED",
                            "mep.elec",
                            format!(
                                "Circuit '{}' ({}) requires <= 30mA RCD/GFCI protection (IEC 60364)",
                                c.id, c.kind
                            ),
                            circuit_spans.get(&c.id).cloned(),
                        )
                        .with_entities(vec![c.id.clone()])
                        .with_suggestion(format!("Add `rcd: 30mA` to breaker '{}'", c.id)),
                    );
                }
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "M6_wet_circuit_rcd".to_string(),
            status: if viols == 0 {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "iec60364_rcd_30ma_protection_v0.7".to_string(),
            tolerance_mm: None,
            subjects: all_circuits.iter().map(|c| c.id.clone()).collect(),
            details: if viols == 0 {
                "All socket, appliance, and wet-area circuits protected by <= 30mA RCD".to_string()
            } else {
                format!("{viols} circuit(s) missing <= 30mA RCD protection")
            },
        });
    }

    ir.mep_circuits = all_circuits;
    ir.mep_wires = all_wires;
    ir.project.has_errors = ir.diagnostics.iter().any(|d| d.severity == Severity::Error);
}

fn resolve_mount(
    mount: &WallMountSpec,
    explicit_room: Option<&str>,
    rooms: &BTreeMap<String, ArchRoomInfo>,
    doors: &BTreeMap<String, ArchDoorInfo>,
) -> Result<(String, Point2, Side), String> {
    match mount {
        WallMountSpec::RoomWall {
            room,
            side,
            placement,
        } => {
            let rinfo = rooms
                .get(room)
                .ok_or_else(|| format!("Unknown host room '{room}'"))?;
            let r = rinfo.rect;
            // Adjust wall span if a corner notch clips this cardinal wall
            let (seg_lo, seg_hi, fixed_coord, is_horiz, facing) = match side {
                Side::South => {
                    let (mut a, mut b) = (r.min_x, r.max_x);
                    if let Some(nr) = rinfo.notch {
                        if (nr.min_y - r.min_y).abs() <= EPS_MM {
                            if (nr.min_x - r.min_x).abs() <= EPS_MM {
                                a = nr.max_x;
                            } else {
                                b = nr.min_x;
                            }
                        }
                    }
                    (a, b, r.min_y, true, Side::North)
                }
                Side::North => {
                    let (mut a, mut b) = (r.min_x, r.max_x);
                    if let Some(nr) = rinfo.notch {
                        if (nr.max_y - r.max_y).abs() <= EPS_MM {
                            if (nr.min_x - r.min_x).abs() <= EPS_MM {
                                a = nr.max_x;
                            } else {
                                b = nr.min_x;
                            }
                        }
                    }
                    (a, b, r.max_y, true, Side::South)
                }
                Side::West => {
                    let (mut a, mut b) = (r.min_y, r.max_y);
                    if let Some(nr) = rinfo.notch {
                        if (nr.min_x - r.min_x).abs() <= EPS_MM {
                            if (nr.min_y - r.min_y).abs() <= EPS_MM {
                                a = nr.max_y;
                            } else {
                                b = nr.min_y;
                            }
                        }
                    }
                    (a, b, r.min_x, false, Side::East)
                }
                Side::East => {
                    let (mut a, mut b) = (r.min_y, r.max_y);
                    if let Some(nr) = rinfo.notch {
                        if (nr.max_x - r.max_x).abs() <= EPS_MM {
                            if (nr.min_y - r.min_y).abs() <= EPS_MM {
                                a = nr.max_y;
                            } else {
                                b = nr.min_y;
                            }
                        }
                    }
                    (a, b, r.max_x, false, Side::West)
                }
            };
            let coord = match placement {
                OpeningPlacement::Center => 0.5 * (seg_lo + seg_hi),
                OpeningPlacement::Offset { distance, from } => {
                    let d = distance.to_mm()?;
                    match from {
                        WallEnd::Start => seg_lo + d,
                        WallEnd::End => seg_hi - d,
                    }
                }
            };
            let pos = if is_horiz {
                Point2::new(coord, fixed_coord)
            } else {
                Point2::new(fixed_coord, coord)
            };
            Ok((room.clone(), pos, facing))
        }
        WallMountSpec::BesideDoor {
            door_id,
            latch_side,
            offset,
        } => {
            let dinfo = doors
                .get(door_id)
                .ok_or_else(|| format!("Unknown host door '{door_id}'"))?;
            let off_mm = offset.to_mm()?;
            let hinge = dinfo.swing_arc.center;
            let latch = dinfo.swing_arc.start_point();
            let (anchor, other) = if *latch_side {
                (latch, hinge)
            } else {
                (hinge, latch)
            };
            let dx = anchor.x - other.x;
            let dy = anchor.y - other.y;
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let ux = dx / len;
            let uy = dy / len;

            // Step `off_mm` past the latch (or hinge) jamb along the wall
            let mut px = anchor.x + ux * off_mm;
            let mut py = anchor.y + uy * off_mm;

            // Snap cleanly onto the target room's interior face along the door wall
            let target_room = explicit_room.unwrap_or("").to_string();
            let mut facing = Side::North;
            if let Some(rinfo) = rooms.get(&target_room) {
                let r = rinfo.rect;
                if dinfo.is_horizontal {
                    if (dinfo.cutout.min_y - r.max_y).abs() < 250.0 || (dinfo.cutout.max_y - r.max_y).abs() < 250.0 {
                        py = r.max_y;
                        facing = Side::South;
                    } else {
                        py = r.min_y;
                        facing = Side::North;
                    }
                    px = px.clamp(r.min_x + 80.0, r.max_x - 80.0);
                } else {
                    if (dinfo.cutout.min_x - r.max_x).abs() < 250.0 || (dinfo.cutout.max_x - r.max_x).abs() < 250.0 {
                        px = r.max_x;
                        facing = Side::West;
                    } else {
                        px = r.min_x;
                        facing = Side::East;
                    }
                    py = py.clamp(r.min_y + 80.0, r.max_y - 80.0);
                }
            }
            Ok((target_room, Point2::new(px, py), facing))
        }
    }
}

fn ortho_l_route(a: Point2, b: Point2) -> Vec<Segment2> {
    if (a.x - b.x).abs() <= EPS_MM || (a.y - b.y).abs() <= EPS_MM {
        vec![Segment2::new(a, b)]
    } else {
        let mid = Point2::new(b.x, a.y);
        vec![Segment2::new(a, mid), Segment2::new(mid, b)]
    }
}

fn point_to_rect_distance(p: Point2, r: Rect2) -> f64 {
    let dx = (r.min_x - p.x).max(p.x - r.max_x).max(0.0);
    let dy = (r.min_y - p.y).max(p.y - r.max_y).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
