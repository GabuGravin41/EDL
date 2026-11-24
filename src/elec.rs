use crate::ast::*;
use crate::geom::{Point2, Rect2, Segment2, EPS_MM};
use crate::ir::*;
use crate::span::{Diagnostic, Severity};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// KiCad standard 100 mil grid in millimeters.
pub const GRID_MM: f64 = 2.54;
const PIN_LEN_MM: f64 = 5.08; // 2 grid steps (200 mil)
const PIN_STEP_MM: f64 = 5.08;

fn snap_grid(v: f64) -> f64 {
    (v / GRID_MM).round() * GRID_MM
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

pub fn compile_electronics(project: &ProjectAst) -> CanonicalIr {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut evidence: Vec<EvidenceRecord> = Vec::new();

    let default_frame = project
        .frames
        .first()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "Sheet".to_string());

    let frames: Vec<FrameIr> = project
        .frames
        .iter()
        .map(|f| FrameIr {
            id: f.name.clone(),
            kind: f.kind.clone(),
            dim: 2,
            handedness: "screen_y_down".to_string(),
        })
        .collect();

    // Collect all components & nets across circuit blocks
    let mut comp_map: BTreeMap<String, &ComponentDecl> = BTreeMap::new();
    let mut comp_order: Vec<String> = Vec::new();
    let mut all_nets: Vec<&NetDecl> = Vec::new();

    for circuit in &project.circuits {
        for comp in &circuit.components {
            if comp_map.insert(comp.id.clone(), comp).is_some() {
                diagnostics.push(
                    Diagnostic::error(
                        "E_DUP_COMPONENT",
                        "semantics",
                        format!("Duplicate component designator '{}'", comp.id),
                        Some(comp.span.clone()),
                    )
                    .with_entities(vec![comp.id.clone()]),
                );
            } else {
                comp_order.push(comp.id.clone());
            }
        }
        for net in &circuit.nets {
            all_nets.push(net);
        }
    }

    // 1. Validate pin references & build functional connectivity graph
    // Map pin key "Comp.Pin" -> list of net names it belongs to
    let mut pin_to_nets: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut net_pins: BTreeMap<String, Vec<(String, String, &PinDecl)>> = BTreeMap::new();
    let mut ground_nets: BTreeSet<String> = BTreeSet::new();

    for net in &all_nets {
        let entry = net_pins.entry(net.name.clone()).or_default();
        if net.name.eq_ignore_ascii_case("GND") || net.name.to_uppercase().contains("GND") {
            ground_nets.insert(net.name.clone());
        }
        for pref in &net.connects {
            let Some(comp) = comp_map.get(&pref.component) else {
                diagnostics.push(
                    Diagnostic::error(
                        "E_REF_UNKNOWN_COMPONENT",
                        "semantics",
                        format!(
                            "Net '{}' references undeclared component '{}'",
                            net.name, pref.component
                        ),
                        Some(pref.span.clone()),
                    )
                    .with_entities(vec![net.name.clone()]),
                );
                continue;
            };
            let Some(pin) = comp.pins.iter().find(|p| p.name == pref.pin) else {
                diagnostics.push(
                    Diagnostic::error(
                        "E_REF_UNKNOWN_PIN",
                        "semantics",
                        format!(
                            "Net '{}' references undeclared pin '{}.{}'",
                            net.name, pref.component, pref.pin
                        ),
                        Some(pref.span.clone()),
                    )
                    .with_entities(vec![net.name.clone(), pref.component.clone()]),
                );
                continue;
            };
            if pin.role == PinRole::Ground {
                ground_nets.insert(net.name.clone());
            }
            pin_to_nets
                .entry((pref.component.clone(), pref.pin.clone()))
                .or_default()
                .push(net.name.clone());
            entry.push((pref.component.clone(), pref.pin.clone(), pin));
        }
    }

    // Check if any pin was accidentally attached to two different nets (net merge conflict)
    for ((cid, pid), nets) in &pin_to_nets {
        let unique: BTreeSet<&str> = nets.iter().map(|s| s.as_str()).collect();
        if unique.len() > 1 {
            diagnostics.push(
                Diagnostic::error(
                    "E_PIN_MULTI_NET",
                    "topology",
                    format!(
                        "Pin '{cid}.{pid}' is connected to multiple distinct nets: [{}]",
                        unique.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                    comp_map.get(cid).map(|c| c.span.clone()),
                )
                .with_entities(vec![cid.clone()]),
            );
        }
    }

    // Build functional relations (strictly separate from drawn routes)
    let mut relations: Vec<RelationIr> = Vec::new();
    let mut net_driven_voltage: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();

    for net in &all_nets {
        let pins = net_pins.get(&net.name).cloned().unwrap_or_default();
        let mut drivers: Vec<(String, f64)> = Vec::new();
        for (cid, pid, pdecl) in &pins {
            if pdecl.role == PinRole::PowerOut {
                if let Some(VoltageSpec::Exact { volts }) = pdecl.voltage {
                    drivers.push((format!("{cid}.{pid}"), volts));
                }
            }
        }
        let driven_v = drivers.first().map(|(_, v)| *v);
        net_driven_voltage.insert(net.name.clone(), drivers);
        relations.push(RelationIr {
            id: net.name.clone(),
            kind: "elec.net".to_string(),
            connects: net
                .connects
                .iter()
                .map(|p| format!("{}.{}", p.component, p.pin))
                .collect(),
            driven_voltage_v: driven_v,
            is_ground: ground_nets.contains(&net.name),
            span: Some(net.span.clone()),
        });
    }

    // 2. Run Electrical Rule Checks (ERC) -> Evidence Ledger
    let mut rules = project.verifications.clone();
    if rules.is_empty() {
        rules.push(VerifyRule::AllRequiredPinsConnected {
            span: project.span.clone(),
        });
        rules.push(VerifyRule::NoPowerGroundShort {
            span: project.span.clone(),
        });
        rules.push(VerifyRule::NoMultiplePowerDrivers {
            span: project.span.clone(),
        });
        rules.push(VerifyRule::VoltageCompatibility {
            span: project.span.clone(),
        });
        rules.push(VerifyRule::NoSinglePinNets {
            span: project.span.clone(),
        });
    }

    for rule in &rules {
        match rule {
            VerifyRule::AllRequiredPinsConnected { .. } => {
                let mut unconnected: Vec<String> = Vec::new();
                for cid in &comp_order {
                    let comp = comp_map[cid];
                    for pin in &comp.pins {
                        if pin.role == PinRole::Nc {
                            continue;
                        }
                        if !pin_to_nets.contains_key(&(cid.clone(), pin.name.clone())) {
                            let full = format!("{cid}.{}", pin.name);
                            unconnected.push(full.clone());
                            diagnostics.push(
                                Diagnostic::error(
                                    "E_ERC_UNCONNECTED_PIN",
                                    "erc",
                                    format!(
                                        "Required {} pin '{}' is not connected to any net",
                                        pin.role.as_str(),
                                        full
                                    ),
                                    Some(pin.span.clone()),
                                )
                                .with_entities(vec![cid.clone(), full.clone()])
                                .with_suggestion(format!(
                                    "Connect '{full}' to an appropriate net (or mark pin as 'nc' if intentionally unused)"
                                )),
                            );
                        }
                    }
                }
                evidence.push(EvidenceRecord {
                    check_id: "E1_all_required_pins_connected".to_string(),
                    status: if unconnected.is_empty() {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_pin_net_incidence_erc_v0.3".to_string(),
                    tolerance_mm: None,
                    subjects: comp_order.clone(),
                    details: if unconnected.is_empty() {
                        format!("All required pins across {} component(s) are connected", comp_order.len())
                    } else {
                        format!(
                            "{} unconnected required pin(s): {}",
                            unconnected.len(),
                            unconnected.join(", ")
                        )
                    },
                });
            }

            VerifyRule::NoPowerGroundShort { .. } => {
                let mut shorts: Vec<String> = Vec::new();
                for net in &all_nets {
                    let pins = net_pins.get(&net.name).cloned().unwrap_or_default();
                    let has_ground = pins.iter().any(|(_, _, p)| p.role == PinRole::Ground);
                    let has_power = pins
                        .iter()
                        .any(|(_, _, p)| p.role == PinRole::PowerOut || p.role == PinRole::PowerIn);
                    if has_ground && has_power {
                        shorts.push(net.name.clone());
                        let affected: Vec<String> = pins
                            .iter()
                            .filter(|(_, _, p)| {
                                matches!(p.role, PinRole::Ground | PinRole::PowerOut | PinRole::PowerIn)
                            })
                            .map(|(c, _, _)| c.clone())
                            .collect();
                        diagnostics.push(
                            Diagnostic::error(
                                "E_ERC_POWER_GROUND_SHORT",
                                "erc",
                                format!(
                                    "Net '{}' shorts power and ground pins together!",
                                    net.name
                                ),
                                Some(net.span.clone()),
                            )
                            .with_entities([vec![net.name.clone()], affected].concat())
                            .with_suggestion(format!(
                                "Separate power pins and ground pins onto distinct nets instead of '{}'",
                                net.name
                            )),
                        );
                    }
                }
                evidence.push(EvidenceRecord {
                    check_id: "E2_no_power_ground_short".to_string(),
                    status: if shorts.is_empty() {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_pin_role_net_check_v0.3".to_string(),
                    tolerance_mm: None,
                    subjects: all_nets.iter().map(|n| n.name.clone()).collect(),
                    details: if shorts.is_empty() {
                        format!("0 power-to-ground shorts across {} net(s)", all_nets.len())
                    } else {
                        format!("Power-ground short on net(s): {}", shorts.join(", "))
                    },
                });
            }

            VerifyRule::NoMultiplePowerDrivers { .. } => {
                let mut conflicts: Vec<String> = Vec::new();
                for net in &all_nets {
                    let pins = net_pins.get(&net.name).cloned().unwrap_or_default();
                    let drivers: Vec<String> = pins
                        .iter()
                        .filter(|(_, _, p)| p.role == PinRole::PowerOut)
                        .map(|(c, p, _)| format!("{c}.{p}"))
                        .collect();
                    if drivers.len() > 1 {
                        conflicts.push(net.name.clone());
                        diagnostics.push(
                            Diagnostic::error(
                                "E_ERC_MULTIPLE_POWER_DRIVERS",
                                "erc",
                                format!(
                                    "Net '{}' has {} conflicting power_out drivers: [{}]",
                                    net.name,
                                    drivers.len(),
                                    drivers.join(", ")
                                ),
                                Some(net.span.clone()),
                            )
                            .with_entities(vec![net.name.clone()])
                            .with_suggestion("Each power rail should have at most one active power_out driver"),
                        );
                    }
                }
                evidence.push(EvidenceRecord {
                    check_id: "E3_no_multiple_power_drivers".to_string(),
                    status: if conflicts.is_empty() {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_power_driver_uniqueness_v0.3".to_string(),
                    tolerance_mm: None,
                    subjects: all_nets.iter().map(|n| n.name.clone()).collect(),
                    details: if conflicts.is_empty() {
                        format!("0 multi-driver power conflicts across {} net(s)", all_nets.len())
                    } else {
                        format!("Multiple power drivers on net(s): {}", conflicts.join(", "))
                    },
                });
            }

            VerifyRule::VoltageCompatibility { .. } => {
                let mut mismatches = 0usize;
                let mut checked_rails = 0usize;
                for net in &all_nets {
                    let pins = net_pins.get(&net.name).cloned().unwrap_or_default();
                    let drivers = net_driven_voltage.get(&net.name).cloned().unwrap_or_default();
                    let Some((driver_pin, v_supply)) = drivers.first() else {
                        continue;
                    };
                    for (cid, pid, pdecl) in &pins {
                        if pdecl.role != PinRole::PowerIn {
                            continue;
                        }
                        let Some(spec) = pdecl.voltage else {
                            continue;
                        };
                        checked_rails += 1;
                        let (ok, min_v, max_v) = match spec {
                            VoltageSpec::Exact { volts } => {
                                // Allow +/- 5% standard supply tolerance on exact nominal ratings
                                let tol = (volts.abs() * 0.05).max(0.05);
                                ((v_supply - volts).abs() <= tol, volts - tol, volts + tol)
                            }
                            VoltageSpec::Range { min_v, max_v } => {
                                (*v_supply >= min_v - 1e-6 && *v_supply <= max_v + 1e-6, min_v, max_v)
                            }
                        };
                        if !ok {
                            mismatches += 1;
                            let target_pin = format!("{cid}.{pid}");
                            diagnostics.push(
                                Diagnostic::error(
                                    "E_ERC_VOLTAGE_MISMATCH",
                                    "erc",
                                    format!(
                                        "Net '{}' supplies {:.2}V (from {}) to '{}' which is rated for {:.2}V..{:.2}V",
                                        net.name, v_supply, driver_pin, target_pin, min_v, max_v
                                    ),
                                    Some(net.span.clone()),
                                )
                                .with_entities(vec![net.name.clone(), cid.clone()])
                                .with_measurement(*v_supply, max_v, "V")
                                .with_suggestion(format!(
                                    "Connect '{target_pin}' to a regulated rail within {min_v:.2}V..{max_v:.2}V instead of '{}'",
                                    net.name
                                )),
                            );
                        }
                    }
                }
                evidence.push(EvidenceRecord {
                    check_id: "E4_voltage_compatibility".to_string(),
                    status: if mismatches == 0 {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_supply_voltage_range_check_v0.3".to_string(),
                    tolerance_mm: None,
                    subjects: all_nets.iter().map(|n| n.name.clone()).collect(),
                    details: if mismatches == 0 {
                        format!("All {checked_rails} rated power_in pin connection(s) within voltage limits")
                    } else {
                        format!("{mismatches} supply voltage compatibility violation(s) detected")
                    },
                });
            }

            VerifyRule::NoSinglePinNets { .. } => {
                let mut dangling: Vec<String> = Vec::new();
                for net in &all_nets {
                    if net.connects.len() < 2 {
                        dangling.push(net.name.clone());
                        diagnostics.push(
                            Diagnostic::error(
                                "E_ERC_SINGLE_PIN_NET",
                                "erc",
                                format!(
                                    "Net '{}' has only {} pin(s) (minimum 2 required for a connection)",
                                    net.name,
                                    net.connects.len()
                                ),
                                Some(net.span.clone()),
                            )
                            .with_entities(vec![net.name.clone()])
                            .with_suggestion(format!(
                                "Connect another pin to net '{}' or remove the dangling net",
                                net.name
                            )),
                        );
                    }
                }
                evidence.push(EvidenceRecord {
                    check_id: "E5_no_single_pin_nets".to_string(),
                    status: if dangling.is_empty() {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_net_degree_check_v0.3".to_string(),
                    tolerance_mm: None,
                    subjects: all_nets.iter().map(|n| n.name.clone()).collect(),
                    details: if dangling.is_empty() {
                        format!("All {} net(s) connect >= 2 pins", all_nets.len())
                    } else {
                        format!("Dangling single-pin net(s): {}", dangling.join(", "))
                    },
                });
            }
            _ => {}
        }
    }

    // 3. Deterministic Grouped Schematic Layout (KiCad 2.54mm grid)
    let schem = project.schematics.first();
    let group_gap_mm = snap_grid(
        schem
            .and_then(|s| s.layout.as_ref())
            .and_then(|l| l.group_gap)
            .and_then(|q| q.to_mm().ok())
            .unwrap_or(30.48),
    )
    .max(25.4);
    let member_gap_mm = snap_grid(
        schem
            .and_then(|s| s.layout.as_ref())
            .and_then(|l| l.member_gap)
            .and_then(|q| q.to_mm().ok())
            .unwrap_or(20.32),
    )
    .max(15.24);

    // Order groups
    let mut ordered_groups: Vec<(String, Vec<String>)> = Vec::new();
    let mut assigned: BTreeSet<String> = BTreeSet::new();

    if let Some(s) = schem {
        let gmap: BTreeMap<&str, &SchematicGroupDecl> =
            s.groups.iter().map(|g| (g.name.as_str(), g)).collect();
        let arrange_order: Vec<String> = s
            .layout
            .as_ref()
            .filter(|l| !l.arrange.is_empty())
            .map(|l| l.arrange.clone())
            .unwrap_or_else(|| s.groups.iter().map(|g| g.name.clone()).collect());

        for gname in arrange_order {
            if let Some(g) = gmap.get(gname.as_str()) {
                let mut valid_members = Vec::new();
                for m in &g.members {
                    if comp_map.contains_key(m) && assigned.insert(m.clone()) {
                        valid_members.push(m.clone());
                    }
                }
                if !valid_members.is_empty() {
                    ordered_groups.push((g.name.clone(), valid_members));
                }
            }
        }
    }
    // Any unassigned components go in a fallback column
    let leftover: Vec<String> = comp_order
        .iter()
        .filter(|id| !assigned.contains(*id))
        .cloned()
        .collect();
    if !leftover.is_empty() {
        ordered_groups.push(("Components".to_string(), leftover));
    }

    // Reserve top channel area for horizontal signal/power buses
    let non_gnd_nets: Vec<&NetDecl> = all_nets
        .iter()
        .copied()
        .filter(|n| !ground_nets.contains(&n.name))
        .collect();
    let bus_top_y = snap_grid(30.48);
    let bus_tracks_h = (non_gnd_nets.len().max(1) as f64) * 5.08;
    let comp_start_y = snap_grid(bus_top_y + bus_tracks_h + 15.24);
    let mut cur_x = snap_grid(35.56);

    let mut entities: Vec<EntityIr> = Vec::new();
    let mut schematic_groups: Vec<SchematicGroupIr> = Vec::new();
    // Keep pin lookup for orthogonal routing: (comp_id, pin_name) -> (PlacedPinIr, col_index, member_idx, col_len)
    struct RoutablePin {
        pin: PlacedPinIr,
        col_left_x: f64,
        col_right_x: f64,
        is_top_in_col: bool,
        is_bottom_in_col: bool,
    }
    let mut placed_pins: BTreeMap<(String, String), RoutablePin> = BTreeMap::new();
    let mut max_comp_y = comp_start_y + 25.4;
    let mut max_comp_x = cur_x + 50.8;

    for (gname, members) in &ordered_groups {
        // Compute max body width in this column so all components align cleanly
        let mut col_body_w = 15.24_f64;
        let mut sizes: Vec<(f64, f64, Vec<(&PinDecl, Side)>)> = Vec::new();
        for cid in members {
            let comp = comp_map[cid];
            let assigned_sides = assign_pin_sides(comp, &pin_to_nets, &ground_nets);
            let (bw, bh) = compute_symbol_size(comp, &assigned_sides);
            col_body_w = col_body_w.max(bw);
            sizes.push((bw, bh, assigned_sides));
        }

        let col_x0 = snap_grid(cur_x);
        let col_x1 = snap_grid(col_x0 + col_body_w);
        let mut cur_y = comp_start_y;
        let g_min_x = col_x0 - PIN_LEN_MM - 5.08;
        let g_max_x = col_x1 + PIN_LEN_MM + 5.08;
        let g_min_y = cur_y - PIN_LEN_MM - 5.08;

        for (m_idx, (cid, (bw, bh, assigned_sides))) in members.iter().zip(sizes.into_iter()).enumerate() {
            let comp = comp_map[cid];
            let bx0 = snap_grid(col_x0 + (col_body_w - bw) * 0.5);
            let by0 = snap_grid(cur_y);
            let body = Rect2::new(bx0, by0, snap_grid(bx0 + bw), snap_grid(by0 + bh));

            let placed = place_pins_on_body(comp, &assigned_sides, body);
            for p in &placed {
                placed_pins.insert(
                    (cid.clone(), p.name.clone()),
                    RoutablePin {
                        pin: p.clone(),
                        col_left_x: snap_grid(col_x0 - PIN_LEN_MM - 5.08),
                        col_right_x: snap_grid(col_x1 + PIN_LEN_MM + 5.08),
                        is_top_in_col: m_idx == 0,
                        is_bottom_in_col: m_idx + 1 == members.len(),
                    },
                );
            }

            let mut props = BTreeMap::new();
            props.insert("group".to_string(), json!(gname));
            props.insert("category".to_string(), json!(comp.category));
            props.insert("value".to_string(), json!(comp.value));
            if let Some(ref fp) = comp.footprint {
                props.insert("footprint".to_string(), json!(fp));
            }

            entities.push(EntityIr {
                id: cid.clone(),
                kind: "elec.component".to_string(),
                frame: default_frame.clone(),
                span: Some(comp.span.clone()),
                props,
                geometry: EntityGeometryIr::Component {
                    category: comp.category.clone(),
                    value: comp.value.clone(),
                    footprint: comp.footprint.clone(),
                    body_rect: body,
                    pins: placed,
                },
            });

            cur_y = snap_grid(body.max_y + member_gap_mm + PIN_LEN_MM);
            max_comp_y = max_comp_y.max(body.max_y + PIN_LEN_MM);
        }

        let g_max_y = cur_y - member_gap_mm + 5.08;
        schematic_groups.push(SchematicGroupIr {
            name: gname.clone(),
            members: members.clone(),
            bbox: Rect2::new(g_min_x, g_min_y, g_max_x, g_max_y),
        });

        max_comp_x = max_comp_x.max(col_x1 + PIN_LEN_MM + 10.16);
        cur_x = snap_grid(col_x1 + group_gap_mm + 2.0 * PIN_LEN_MM);
    }

    // 4. Orthogonal Manhattan Wire Routing (strictly zero wire-body crossings, 2.54mm grid!)
    let gnd_rail_y = snap_grid(max_comp_y + 15.24);
    let mut schematic_routes: Vec<SchematicRouteIr> = Vec::new();
    let mut track_idx = 0usize;
    let mut gnd_track_idx = 0usize;
    let mut sig_net_idx = 0usize;

    for net in &all_nets {
        let is_gnd = ground_nets.contains(&net.name);
        let is_pwr = net_driven_voltage
            .get(&net.name)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || net.name.to_uppercase().starts_with('V');

        let mut net_endpoints: Vec<&RoutablePin> = Vec::new();
        for pref in &net.connects {
            if let Some(rp) = placed_pins.get(&(pref.component.clone(), pref.pin.clone())) {
                net_endpoints.push(rp);
            }
        }
        if net_endpoints.is_empty() {
            continue;
        }

        let trunk_y = if is_gnd {
            let y = snap_grid(gnd_rail_y + (gnd_track_idx as f64) * 5.08);
            gnd_track_idx += 1;
            y
        } else {
            let y = snap_grid(bus_top_y + (track_idx as f64) * 5.08);
            track_idx += 1;
            y
        };

        // Unique per-net stagger so two distinct nets never share a vertical gutter line
        let gutter_stagger = if is_gnd {
            0.0
        } else {
            let s = ((sig_net_idx % 5) as f64) * GRID_MM;
            sig_net_idx += 1;
            s
        };

        // Collect all vertical risers grouped by column gutter X so multiple taps on the same X
        // connect cleanly without overshooting unnecessarily.
        let mut segments: Vec<Segment2> = Vec::new();
        let mut risers: Vec<(f64, f64)> = Vec::new(); // (x, y_entry_on_riser)

        for rp in &net_endpoints {
            let tip = rp.pin.tip_point;
            match rp.pin.side {
                Side::South if is_gnd && rp.is_bottom_in_col => {
                    risers.push((tip.x, tip.y));
                }
                Side::North if !is_gnd && rp.is_top_in_col => {
                    risers.push((tip.x, tip.y));
                }
                Side::West => {
                    let gx = snap_grid(rp.col_left_x - gutter_stagger);
                    add_ortho_seg(&mut segments, tip, Point2::new(gx, tip.y));
                    risers.push((gx, tip.y));
                }
                Side::East => {
                    let gx = snap_grid(rp.col_right_x + gutter_stagger);
                    add_ortho_seg(&mut segments, tip, Point2::new(gx, tip.y));
                    risers.push((gx, tip.y));
                }
                Side::North => {
                    let step_y = snap_grid(tip.y - GRID_MM);
                    let gx = snap_grid(rp.col_right_x + gutter_stagger);
                    add_ortho_seg(&mut segments, tip, Point2::new(tip.x, step_y));
                    add_ortho_seg(&mut segments, Point2::new(tip.x, step_y), Point2::new(gx, step_y));
                    risers.push((gx, step_y));
                }
                Side::South => {
                    // Upper member GND drop uses LEFT gutter so it never crosses right-gutter signal/power lines!
                    let step_y = snap_grid(tip.y + GRID_MM);
                    let gx = if is_gnd {
                        snap_grid(rp.col_left_x - GRID_MM)
                    } else {
                        snap_grid(rp.col_right_x + gutter_stagger)
                    };
                    add_ortho_seg(&mut segments, tip, Point2::new(tip.x, step_y));
                    add_ortho_seg(&mut segments, Point2::new(tip.x, step_y), Point2::new(gx, step_y));
                    risers.push((gx, step_y));
                }
            }
        }

        // Group risers by distinct X (exact grid columns)
        let mut by_x: BTreeMap<i64, (f64, Vec<f64>)> = BTreeMap::new();
        for (rx, ry) in risers {
            let key = (rx * 100.0).round() as i64;
            let entry = by_x.entry(key).or_insert_with(|| (rx, Vec::new()));
            entry.1.push(ry);
        }

        let mut junctions: Vec<Point2> = Vec::new();
        let label_pos: Option<Point2>;

        if by_x.len() == 1 {
            // All pins of this net live on a single vertical line (e.g. R1.B -> D1.A):
            // connect them directly with zero overshoot up to the top bus!
            let (_, (x, mut ys)) = by_x.into_iter().next().unwrap();
            ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
            ys.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);
            if ys.len() == 1 {
                // Single-pin dangling net: short stub toward trunk_y so it's visible
                let stub_y = if is_gnd {
                    snap_grid(ys[0] + 10.16)
                } else {
                    snap_grid(ys[0] - 10.16)
                };
                add_ortho_seg(&mut segments, Point2::new(x, ys[0]), Point2::new(x, stub_y));
                label_pos = Some(Point2::new(x, stub_y));
            } else {
                for pair in ys.windows(2) {
                    add_ortho_seg(&mut segments, Point2::new(x, pair[0]), Point2::new(x, pair[1]));
                }
                for &y_mid in &ys[1..ys.len() - 1] {
                    junctions.push(Point2::new(x, y_mid));
                }
                label_pos = Some(Point2::new(x, snap_grid(0.5 * (ys[0] + *ys.last().unwrap()))));
            }
        } else {
            // Multiple columns: route each column vertically to `trunk_y` and join along `trunk_y`
            let mut trunk_xs: Vec<f64> = Vec::new();
            for (_, (x, mut ys)) in by_x {
                ys.push(trunk_y);
                ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
                ys.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);
                for pair in ys.windows(2) {
                    add_ortho_seg(&mut segments, Point2::new(x, pair[0]), Point2::new(x, pair[1]));
                }
                // Interior T-taps along this vertical riser get explicit junction dots!
                for &y_mid in &ys[1..ys.len() - 1] {
                    if (y_mid - trunk_y).abs() > EPS_MM {
                        junctions.push(Point2::new(x, y_mid));
                    }
                }
                trunk_xs.push(x);
            }
            trunk_xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for pair in trunk_xs.windows(2) {
                add_ortho_seg(
                    &mut segments,
                    Point2::new(pair[0], trunk_y),
                    Point2::new(pair[1], trunk_y),
                );
            }
            for &x_mid in &trunk_xs[1..trunk_xs.len() - 1] {
                junctions.push(Point2::new(x_mid, trunk_y));
            }
            label_pos = Some(Point2::new(
                snap_grid(0.5 * (trunk_xs[0] + *trunk_xs.last().unwrap())),
                trunk_y,
            ));
        }

        schematic_routes.push(SchematicRouteIr {
            net_id: net.name.clone(),
            is_power: is_pwr,
            is_ground: is_gnd,
            segments,
            junctions,
            label_pos,
        });
    }

    let sheet_w = max_comp_x.max(254.0);
    let sheet_h = (gnd_rail_y + (gnd_track_idx as f64) * 5.08 + 20.32).max(177.8);
    let sheet_bounds = Rect2::new(0.0, 0.0, snap_grid(sheet_w), snap_grid(sheet_h));

    let views = vec![ViewIr {
        id: schem.map(|s| s.name.clone()).unwrap_or_else(|| "Main".to_string()),
        kind: "SchematicView".to_string(),
        frame: default_frame,
        sheet: schem
            .map(|s| s.sheet.clone())
            .unwrap_or_else(|| "A4 landscape".to_string()),
        scale: "1:1".to_string(),
        show_dimensions: false,
        show_labels: true,
    }];

    let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

    CanonicalIr {
        ir_version: "0.3.0".to_string(),
        project: ProjectSummaryIr {
            name: project.name.clone(),
            domain: "electronics".to_string(),
            default_unit: project.default_unit.to_string(),
            exterior_footprint_mm: Some([round4(sheet_bounds.width()), round4(sheet_bounds.depth())]),
            exterior_bounds: Some(sheet_bounds),
            total_clear_area_m2: 0.0,
            room_count: 0,
            opening_count: 0,
            fixture_count: 0,
            component_count: comp_order.len(),
            net_count: all_nets.len(),
            remaining_dof: Vec::new(),
            has_errors,
        },
        frames,
        entities,
        relations,
        derived_walls: Vec::new(),
        wall_solids: Vec::new(),
        schematic_groups,
        schematic_routes,
        constraints: Vec::new(),
        views,
        evidence,
        diagnostics,
    }
}

fn add_ortho_seg(segs: &mut Vec<Segment2>, a: Point2, b: Point2) {
    if a.dist(b) > EPS_MM {
        segs.push(Segment2::new(a, b));
    }
}

fn assign_pin_sides<'a>(
    comp: &'a ComponentDecl,
    pin_to_nets: &BTreeMap<(String, String), Vec<String>>,
    ground_nets: &BTreeSet<String>,
) -> Vec<(&'a PinDecl, Side)> {
    // Check 2-pin passives first: if one pin is connected to a GND net, orient vertically (TOP + BOTTOM)!
    if comp.pins.len() == 2 && comp.pins.iter().all(|p| p.role == PinRole::Passive) {
        let p0_gnd = pin_to_nets
            .get(&(comp.id.clone(), comp.pins[0].name.clone()))
            .map(|ns| ns.iter().any(|n| ground_nets.contains(n)))
            .unwrap_or(false);
        let p1_gnd = pin_to_nets
            .get(&(comp.id.clone(), comp.pins[1].name.clone()))
            .map(|ns| ns.iter().any(|n| ground_nets.contains(n)))
            .unwrap_or(false);
        if p1_gnd && !p0_gnd {
            return vec![(&comp.pins[0], Side::North), (&comp.pins[1], Side::South)];
        }
        if p0_gnd && !p1_gnd {
            return vec![(&comp.pins[0], Side::South), (&comp.pins[1], Side::North)];
        }
        return vec![(&comp.pins[0], Side::West), (&comp.pins[1], Side::East)];
    }

    let mut out = Vec::new();
    let mut passive_idx = 0usize;
    for pin in &comp.pins {
        let on_gnd = pin_to_nets
            .get(&(comp.id.clone(), pin.name.clone()))
            .map(|ns| ns.iter().any(|n| ground_nets.contains(n)))
            .unwrap_or(false);
        let side = match pin.role {
            PinRole::Ground => Side::South,
            PinRole::PowerIn | PinRole::Input => Side::West,
            PinRole::PowerOut | PinRole::Output | PinRole::Bidir => Side::East,
            PinRole::Passive if on_gnd => Side::South,
            PinRole::Passive => {
                let s = if passive_idx % 2 == 0 { Side::West } else { Side::East };
                passive_idx += 1;
                s
            }
            PinRole::Nc => Side::East,
        };
        out.push((pin, side));
    }
    out
}

fn compute_symbol_size(comp: &ComponentDecl, assigned: &[(&PinDecl, Side)]) -> (f64, f64) {
    let n_west = assigned.iter().filter(|(_, s)| *s == Side::West).count();
    let n_east = assigned.iter().filter(|(_, s)| *s == Side::East).count();
    let n_north = assigned.iter().filter(|(_, s)| *s == Side::North).count();
    let n_south = assigned.iter().filter(|(_, s)| *s == Side::South).count();

    let max_lr = n_west.max(n_east).max(1);
    let max_tb = n_north.max(n_south).max(1);

    // Compact sizing for 2-pin passives, wider boxes for ICs/regulators/connectors
    let is_small_passive = comp.pins.len() <= 2
        && matches!(
            comp.category.as_str(),
            "resistor" | "capacitor" | "inductor" | "diode" | "led"
        );

    let min_w = if is_small_passive { 10.16 } else { 20.32 };
    let min_h = if is_small_passive { 10.16 } else { 15.24 };

    let w = snap_grid(((max_tb as f64 + 1.0) * PIN_STEP_MM).max(min_w));
    let h = snap_grid(((max_lr as f64 + 1.0) * PIN_STEP_MM).max(min_h));
    (w, h)
}

fn place_pins_on_body(
    comp: &ComponentDecl,
    assigned: &[(&PinDecl, Side)],
    body: Rect2,
) -> Vec<PlacedPinIr> {
    let mut by_side: BTreeMap<u8, Vec<(usize, &PinDecl)>> = BTreeMap::new();
    for (idx, (pin, side)) in assigned.iter().enumerate() {
        let key = match side {
            Side::West => 0,
            Side::East => 1,
            Side::North => 2,
            Side::South => 3,
        };
        by_side.entry(key).or_default().push((idx + 1, *pin));
    }

    let mut placed = Vec::new();
    let _ = comp;
    for (key, pins) in by_side {
        let side = match key {
            0 => Side::West,
            1 => Side::East,
            2 => Side::North,
            _ => Side::South,
        };
        let count = pins.len();
        for (i, (pin_num, pin)) in pins.into_iter().enumerate() {
            let (body_pt, tip_pt) = match side {
                Side::West => {
                    let y = snap_grid(body.min_y + ((i + 1) as f64) * (body.depth() / (count + 1) as f64));
                    (Point2::new(body.min_x, y), Point2::new(body.min_x - PIN_LEN_MM, y))
                }
                Side::East => {
                    let y = snap_grid(body.min_y + ((i + 1) as f64) * (body.depth() / (count + 1) as f64));
                    (Point2::new(body.max_x, y), Point2::new(body.max_x + PIN_LEN_MM, y))
                }
                Side::North => {
                    let x = snap_grid(body.min_x + ((i + 1) as f64) * (body.width() / (count + 1) as f64));
                    (Point2::new(x, body.min_y), Point2::new(x, body.min_y - PIN_LEN_MM))
                }
                Side::South => {
                    let x = snap_grid(body.min_x + ((i + 1) as f64) * (body.width() / (count + 1) as f64));
                    (Point2::new(x, body.max_y), Point2::new(x, body.max_y + PIN_LEN_MM))
                }
            };
            placed.push(PlacedPinIr {
                name: pin.name.clone(),
                number: pin_num,
                role: pin.role,
                voltage: pin.voltage,
                side,
                body_point: body_pt,
                tip_point: tip_pt,
            });
        }
    }
    placed
}
