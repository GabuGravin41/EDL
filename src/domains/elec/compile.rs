use crate::core::geom::{Point2, Rect2, Segment2, Side, EPS_MM};
use crate::core::ir::*;
use crate::core::span::{Diagnostic, Severity};
use crate::core::units::VoltageSpec;
use crate::domains::elec::ast::*;
use crate::syntax::ast::{ProjectAst, VerifyRule};
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

fn style_name(s: NetRoutingStyle) -> &'static str {
    match s {
        NetRoutingStyle::Direct => "direct",
        NetRoutingStyle::Bus => "bus",
        NetRoutingStyle::Rail => "rail",
        NetRoutingStyle::Symbol => "symbol",
        NetRoutingStyle::LabelStub => "label_stub",
    }
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

    // Collect components & nets across circuit blocks
    let mut comp_map: BTreeMap<String, &ComponentDecl> = BTreeMap::new();
    let mut comp_order: Vec<String> = Vec::new();
    let mut all_nets: Vec<&NetDecl> = Vec::new();

    for circuit in &project.elec.circuits {
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

    // 2. Run Electrical Rule Checks (ERC) — 100% invariant to presentation/routing style!
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
                                format!("Net '{}' shorts power and ground pins together!", net.name),
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

    // 3. Resolve routing policies per net (layered: per-net override > category policy > smart default)
    let schem = project.elec.schematics.first();
    let routing_cfg = schem.and_then(|s| s.routing.as_ref());
    let mut net_override_map: BTreeMap<String, NetRoutingStyle> = BTreeMap::new();
    if let Some(rcfg) = routing_cfg {
        for (nname, st, span) in &rcfg.net_overrides {
            if !all_nets.iter().any(|n| n.name == *nname) {
                diagnostics.push(Diagnostic::error(
                    "E_REF_UNKNOWN_NET",
                    "semantics",
                    format!("Routing override references undeclared net '{nname}'"),
                    Some(span.clone()),
                ));
            }
            net_override_map.insert(nname.clone(), *st);
        }
    }

    let resolve_style = |net_name: &str, is_gnd: bool, is_pwr: bool| -> NetRoutingStyle {
        if let Some(&ov) = net_override_map.get(net_name) {
            return ov;
        }
        if is_gnd {
            return routing_cfg
                .and_then(|r| r.ground_style)
                .unwrap_or(NetRoutingStyle::Rail);
        }
        if is_pwr {
            return routing_cfg
                .and_then(|r| r.power_style)
                .unwrap_or(NetRoutingStyle::Direct);
        }
        routing_cfg
            .and_then(|r| r.default_style)
            .unwrap_or(NetRoutingStyle::Direct)
    };

    // Explicit placement overrides (`place <Comp> at: (x, y)`)
    let mut explicit_pos: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    if let Some(s) = schem {
        for pl in &s.placements {
            match (pl.x.to_mm(), pl.y.to_mm()) {
                (Ok(x), Ok(y)) => {
                    explicit_pos.insert(pl.component.clone(), (snap_grid(x), snap_grid(y)));
                }
                (Err(m), _) | (_, Err(m)) => {
                    diagnostics.push(Diagnostic::error(
                        "E_UNIT_MISMATCH",
                        "units",
                        format!("Explicit placement for '{}': {m}", pl.component),
                        Some(pl.span.clone()),
                    ));
                }
            }
        }
    }

    // 4. Deterministic Grouped Schematic Layout (KiCad 2.54mm grid)
    let group_gap_mm = snap_grid(
        schem
            .and_then(|s| s.layout.as_ref())
            .and_then(|l| l.group_gap)
            .and_then(|q| q.to_mm().ok())
            .unwrap_or(25.4),
    )
    .max(15.24);
    let any_symbol_mode = all_nets.iter().any(|n| {
        let is_g = ground_nets.contains(&n.name);
        let is_p = net_driven_voltage
            .get(&n.name)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || n.name.to_uppercase().starts_with('V');
        resolve_style(&n.name, is_g, is_p) == NetRoutingStyle::Symbol
    });
    let min_member_gap = if any_symbol_mode { 20.32 } else { 12.70 };
    let member_gap_mm = snap_grid(
        schem
            .and_then(|s| s.layout.as_ref())
            .and_then(|l| l.member_gap)
            .and_then(|q| q.to_mm().ok())
            .unwrap_or(min_member_gap),
    )
    .max(min_member_gap);

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
    let leftover: Vec<String> = comp_order
        .iter()
        .filter(|id| !assigned.contains(*id))
        .cloned()
        .collect();
    if !leftover.is_empty() {
        ordered_groups.push(("Components".to_string(), leftover));
    }

    // Build comp -> col_idx map so we can predict how many nets actually need the overhead bus
    let mut comp_col_map: BTreeMap<String, usize> = BTreeMap::new();
    for (c_idx, (_, members)) in ordered_groups.iter().enumerate() {
        for m in members {
            comp_col_map.insert(m.clone(), c_idx);
        }
    }

    // Pre-assign pin sides for every component (honoring explicit `pin ... side:` overrides!)
    let mut comp_pin_sides: BTreeMap<String, Vec<(&PinDecl, Side)>> = BTreeMap::new();
    for cid in &comp_order {
        let comp = comp_map[cid];
        comp_pin_sides.insert(cid.clone(), assign_pin_sides(comp, &pin_to_nets, &ground_nets));
    }

    // Count how many nets will actually use the top bus so we only reserve exact space needed!
    let mut predicted_bus_nets = 0usize;
    for net in &all_nets {
        let is_gnd = ground_nets.contains(&net.name);
        let is_pwr = net_driven_voltage
            .get(&net.name)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || net.name.to_uppercase().starts_with('V');
        let st = resolve_style(&net.name, is_gnd, is_pwr);
        if matches!(st, NetRoutingStyle::Bus) {
            predicted_bus_nets += 1;
        } else if st == NetRoutingStyle::Direct && !is_gnd {
            let cols: BTreeSet<usize> = net
                .connects
                .iter()
                .filter_map(|p| comp_col_map.get(&p.component).copied())
                .collect();
            if cols.len() > 2 || (cols.len() == 2 && !can_direct_bridge_gap(net, &comp_col_map, &comp_pin_sides)) {
                predicted_bus_nets += 1;
            }
        }
    }

    // Compact inter-column gap: wide enough for direct local lanes & any bus risers without bloating!
    let local_gutter_budget = snap_grid(((predicted_bus_nets + 3).min(7) as f64) * 2.0 * GRID_MM);
    let effective_group_gap = group_gap_mm.max(local_gutter_budget);

    let bus_top_y = snap_grid(25.4);
    let bus_tracks_h = if predicted_bus_nets == 0 {
        0.0
    } else {
        (predicted_bus_nets as f64) * 5.08 + 7.62
    };
    let comp_start_y = snap_grid(bus_top_y + bus_tracks_h + 12.70);
    let mut cur_x = snap_grid(30.48 + ((predicted_bus_nets.min(4) + 1) as f64) * GRID_MM);

    let mut entities: Vec<EntityIr> = Vec::new();
    let mut schematic_groups: Vec<SchematicGroupIr> = Vec::new();
    let mut col_bounds: Vec<(f64, f64)> = Vec::new(); // (col_x0, col_x1)

    #[derive(Clone)]
    struct RoutablePin {
        pin: PlacedPinIr,
        col_idx: usize,
        col_left_x: f64,
        col_right_x: f64,
        body_min_y: f64,
        is_top_in_col: bool,
        is_bottom_in_col: bool,
    }
    let mut placed_pins: BTreeMap<(String, String), RoutablePin> = BTreeMap::new();
    let mut max_comp_y = comp_start_y + 25.4;
    let mut max_comp_x = cur_x + 50.8;

    for (col_idx, (gname, members)) in ordered_groups.iter().enumerate() {
        let mut col_body_w = 15.24_f64;
        let mut sizes: Vec<(f64, f64)> = Vec::new();
        for cid in members {
            let comp = comp_map[cid];
            let assigned_sides = &comp_pin_sides[cid];
            let (bw, bh) = compute_symbol_size(comp, assigned_sides);
            col_body_w = col_body_w.max(bw);
            sizes.push((bw, bh));
        }

        let col_x0 = snap_grid(cur_x);
        let col_x1 = snap_grid(col_x0 + col_body_w);
        col_bounds.push((col_x0, col_x1));

        let mut cur_y = comp_start_y;
        let g_min_x = col_x0 - PIN_LEN_MM - 5.08;
        let g_max_x = col_x1 + PIN_LEN_MM + 5.08;
        let mut g_min_y = cur_y - PIN_LEN_MM - 5.08;
        let mut g_max_y = cur_y + 20.32;

        for (m_idx, (cid, (bw, bh))) in members.iter().zip(sizes.into_iter()).enumerate() {
            let comp = comp_map[cid];
            let assigned_sides = &comp_pin_sides[cid];

            let (bx0, by0) = if let Some(&(ex, ey)) = explicit_pos.get(cid) {
                (ex, ey)
            } else {
                (snap_grid(col_x0 + (col_body_w - bw) * 0.5), snap_grid(cur_y))
            };
            let body = Rect2::new(bx0, by0, snap_grid(bx0 + bw), snap_grid(by0 + bh));
            let placed = place_pins_on_body(comp, assigned_sides, body);

            let local_left = snap_grid(col_x0.min(bx0) - PIN_LEN_MM - 2.54);
            let local_right = snap_grid(col_x1.max(body.max_x) + PIN_LEN_MM + 2.54);

            for p in &placed {
                placed_pins.insert(
                    (cid.clone(), p.name.clone()),
                    RoutablePin {
                        pin: p.clone(),
                        col_idx,
                        col_left_x: local_left,
                        col_right_x: local_right,
                        body_min_y: body.min_y,
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
                provenance: comp.provenance.clone(),
                props,
                geometry: EntityGeometryIr::Component {
                    category: comp.category.clone(),
                    value: comp.value.clone(),
                    footprint: comp.footprint.clone(),
                    body_rect: body,
                    pins: placed,
                },
            });

            if !explicit_pos.contains_key(cid) {
                cur_y = snap_grid(body.max_y + member_gap_mm + PIN_LEN_MM);
            }
            g_min_y = g_min_y.min(body.min_y - PIN_LEN_MM - 5.08);
            g_max_y = g_max_y.max(body.max_y + PIN_LEN_MM + 5.08);
            max_comp_y = max_comp_y.max(body.max_y + PIN_LEN_MM);
            max_comp_x = max_comp_x.max(body.max_x + PIN_LEN_MM + 12.70);
        }

        schematic_groups.push(SchematicGroupIr {
            name: gname.clone(),
            members: members.clone(),
            bbox: Rect2::new(g_min_x, g_min_y, g_max_x, g_max_y),
        });

        cur_x = snap_grid(col_x1 + effective_group_gap + 2.0 * PIN_LEN_MM);
    }

    // 5. Layered Multi-Style Router (direct | rail | bus | symbol | label_stub)
    // Per-column local lane allocators so lanes stay tight and NEVER collide across nets!
    let mut left_lane_next: BTreeMap<usize, usize> = BTreeMap::new();
    let mut right_lane_next: BTreeMap<usize, usize> = BTreeMap::new();
    // Reserve lane 0 on left gutters for GND rail drops whenever ground uses rail
    let any_gnd_rail = all_nets.iter().any(|n| {
        let is_g = ground_nets.contains(&n.name);
        is_g && resolve_style(&n.name, true, false) == NetRoutingStyle::Rail
    });
    if any_gnd_rail {
        for c in 0..ordered_groups.len() {
            left_lane_next.insert(c, 1); // lane 0 reserved exclusively for GND rail drops
        }
    }

    let gnd_rail_y = snap_grid(max_comp_y + 12.70);
    let mut schematic_routes: Vec<SchematicRouteIr> = Vec::new();
    let mut top_bus_track = 0usize;
    let mut gnd_rail_track = 0usize;

    for net in &all_nets {
        let is_gnd = ground_nets.contains(&net.name);
        let is_pwr = net_driven_voltage
            .get(&net.name)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || net.name.to_uppercase().starts_with('V');

        let style = resolve_style(&net.name, is_gnd, is_pwr);

        let mut net_endpoints: Vec<&RoutablePin> = Vec::new();
        for pref in &net.connects {
            if let Some(rp) = placed_pins.get(&(pref.component.clone(), pref.pin.clone())) {
                net_endpoints.push(rp);
            }
        }
        if net_endpoints.is_empty() {
            continue;
        }

        // --- MODE A: Local Symbol (GND triangles / upward power flags) ---
        if style == NetRoutingStyle::Symbol {
            let mut segments = Vec::new();
            let mut terminals = Vec::new();
            for rp in &net_endpoints {
                let tip = rp.pin.tip_point;
                if is_gnd {
                    let end = match rp.pin.side {
                        Side::South => {
                            let e = Point2::new(tip.x, snap_grid(tip.y + GRID_MM));
                            add_ortho_seg(&mut segments, tip, e);
                            e
                        }
                        Side::West => {
                            let mid = Point2::new(snap_grid(tip.x - GRID_MM), tip.y);
                            let e = Point2::new(mid.x, snap_grid(mid.y + GRID_MM));
                            add_ortho_seg(&mut segments, tip, mid);
                            add_ortho_seg(&mut segments, mid, e);
                            e
                        }
                        Side::East => {
                            let mid = Point2::new(snap_grid(tip.x + GRID_MM), tip.y);
                            let e = Point2::new(mid.x, snap_grid(mid.y + GRID_MM));
                            add_ortho_seg(&mut segments, tip, mid);
                            add_ortho_seg(&mut segments, mid, e);
                            e
                        }
                        Side::North => {
                            let e = Point2::new(tip.x, snap_grid(tip.y - GRID_MM));
                            add_ortho_seg(&mut segments, tip, e);
                            e
                        }
                    };
                    terminals.push(NetTerminalIr {
                        kind: "gnd_symbol".to_string(),
                        pos: end,
                        dir: Side::South,
                        label: net.name.clone(),
                    });
                } else {
                    let end = match rp.pin.side {
                        Side::North => {
                            let e = Point2::new(tip.x, snap_grid(tip.y - GRID_MM));
                            add_ortho_seg(&mut segments, tip, e);
                            e
                        }
                        Side::West => {
                            let mid = Point2::new(snap_grid(tip.x - GRID_MM), tip.y);
                            let e = Point2::new(mid.x, snap_grid(mid.y - GRID_MM));
                            add_ortho_seg(&mut segments, tip, mid);
                            add_ortho_seg(&mut segments, mid, e);
                            e
                        }
                        Side::East => {
                            let mid = Point2::new(snap_grid(tip.x + GRID_MM), tip.y);
                            let e = Point2::new(mid.x, snap_grid(mid.y - GRID_MM));
                            add_ortho_seg(&mut segments, tip, mid);
                            add_ortho_seg(&mut segments, mid, e);
                            e
                        }
                        Side::South => {
                            let e = Point2::new(tip.x, snap_grid(tip.y + GRID_MM));
                            add_ortho_seg(&mut segments, tip, e);
                            e
                        }
                    };
                    terminals.push(NetTerminalIr {
                        kind: "power_symbol".to_string(),
                        pos: end,
                        dir: Side::North,
                        label: net.name.clone(),
                    });
                }
            }
            schematic_routes.push(SchematicRouteIr {
                net_id: net.name.clone(),
                style: style_name(style).to_string(),
                is_power: is_pwr,
                is_ground: is_gnd,
                segments,
                junctions: Vec::new(),
                terminals,
                label_pos: None,
            });
            continue;
        }

        // --- MODE B: Local Named Net-Label Stubs (zero cross-sheet wires!) ---
        if style == NetRoutingStyle::LabelStub {
            let mut segments = Vec::new();
            let mut terminals = Vec::new();
            for rp in &net_endpoints {
                let tip = rp.pin.tip_point;
                let end = match rp.pin.side {
                    Side::West => Point2::new(snap_grid(tip.x - 5.08), tip.y),
                    Side::East => Point2::new(snap_grid(tip.x + 5.08), tip.y),
                    Side::North => Point2::new(tip.x, snap_grid(tip.y - 5.08)),
                    Side::South => Point2::new(tip.x, snap_grid(tip.y + 5.08)),
                };
                add_ortho_seg(&mut segments, tip, end);
                terminals.push(NetTerminalIr {
                    kind: "label_stub".to_string(),
                    pos: end,
                    dir: rp.pin.side,
                    label: net.name.clone(),
                });
            }
            schematic_routes.push(SchematicRouteIr {
                net_id: net.name.clone(),
                style: style_name(style).to_string(),
                is_power: is_pwr,
                is_ground: is_gnd,
                segments,
                junctions: Vec::new(),
                terminals,
                label_pos: None,
            });
            continue;
        }

        // --- MODE C: Direct Local Routing (single column OR adjacent-column facing gap!) ---
        if style == NetRoutingStyle::Direct && !is_gnd {
            let cols: BTreeSet<usize> = net_endpoints.iter().map(|rp| rp.col_idx).collect();

            // Subcase C1: Single-column net (e.g., LED_A: R2.B -> D1.A) -> tight local gutter lane!
            if cols.len() == 1 {
                let col = *cols.iter().next().unwrap();
                let all_west = net_endpoints.iter().all(|rp| rp.pin.side == Side::West);

                let (gx, mut segments, mut ys) = if all_west {
                    let lane = *left_lane_next.entry(col).or_insert(0);
                    *left_lane_next.get_mut(&col).unwrap() += 1;
                    let lx = snap_grid(net_endpoints[0].col_left_x - (lane as f64) * GRID_MM);
                    let mut segs = Vec::new();
                    let mut ylist = Vec::new();
                    for rp in &net_endpoints {
                        let tip = rp.pin.tip_point;
                        add_ortho_seg(&mut segs, tip, Point2::new(lx, tip.y));
                        ylist.push(tip.y);
                    }
                    (lx, segs, ylist)
                } else {
                    let lane = *right_lane_next.entry(col).or_insert(0);
                    *right_lane_next.get_mut(&col).unwrap() += 1;
                    let rx = snap_grid(net_endpoints[0].col_right_x + (lane as f64) * GRID_MM);

                    let mut segs = Vec::new();
                    let mut ylist = Vec::new();
                    for rp in &net_endpoints {
                        let tip = rp.pin.tip_point;
                        match rp.pin.side {
                            Side::East => {
                                add_ortho_seg(&mut segs, tip, Point2::new(rx, tip.y));
                                ylist.push(tip.y);
                            }
                            Side::North => {
                                let sy = snap_grid(tip.y - GRID_MM);
                                add_ortho_seg(&mut segs, tip, Point2::new(tip.x, sy));
                                add_ortho_seg(&mut segs, Point2::new(tip.x, sy), Point2::new(rx, sy));
                                ylist.push(sy);
                            }
                            Side::South => {
                                let sy = snap_grid(tip.y + GRID_MM);
                                add_ortho_seg(&mut segs, tip, Point2::new(tip.x, sy));
                                add_ortho_seg(&mut segs, Point2::new(tip.x, sy), Point2::new(rx, sy));
                                ylist.push(sy);
                            }
                            Side::West => {
                                // Route cleanly out the left gutter, up into the inter-component clearance channel,
                                // and across to the right lane without ever crossing a component body!
                                let llane = *left_lane_next.entry(col).or_insert(0);
                                *left_lane_next.get_mut(&col).unwrap() += 1;
                                let wx = snap_grid(rp.col_left_x - (llane as f64) * GRID_MM);
                                let by = snap_grid(rp.body_min_y - 3.0 * GRID_MM);
                                add_ortho_seg(&mut segs, tip, Point2::new(wx, tip.y));
                                add_ortho_seg(&mut segs, Point2::new(wx, tip.y), Point2::new(wx, by));
                                add_ortho_seg(&mut segs, Point2::new(wx, by), Point2::new(rx, by));
                                ylist.push(by);
                            }
                        }
                    }
                    (rx, segs, ylist)
                };
                ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
                ys.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);
                let mut junctions = Vec::new();
                let label_pos = if ys.len() == 1 {
                    let stub_y = snap_grid(ys[0] - 5.08);
                    add_ortho_seg(&mut segments, Point2::new(gx, ys[0]), Point2::new(gx, stub_y));
                    Some(Point2::new(gx, stub_y))
                } else {
                    for pair in ys.windows(2) {
                        add_ortho_seg(&mut segments, Point2::new(gx, pair[0]), Point2::new(gx, pair[1]));
                    }
                    for &ym in &ys[1..ys.len() - 1] {
                        junctions.push(Point2::new(gx, ym));
                    }
                    Some(Point2::new(gx, snap_grid(0.5 * (ys[0] + *ys.last().unwrap()))))
                };

                schematic_routes.push(SchematicRouteIr {
                    net_id: net.name.clone(),
                    style: "direct".to_string(),
                    is_power: is_pwr,
                    is_ground: false,
                    segments,
                    junctions,
                    terminals: Vec::new(),
                    label_pos,
                });
                continue;
            }

            // Subcase C2: Adjacent-column direct bridge! (e.g. VBUS: J1.VBUS + C1.POS -> U1.IN, or GPIO_LED: U2 -> R2)
            if cols.len() == 2 && can_direct_bridge_gap(net, &comp_col_map, &comp_pin_sides) {
                let mut it = cols.iter().copied();
                let c_left = it.next().unwrap();
                let lane = *right_lane_next.entry(c_left).or_insert(0);
                *right_lane_next.get_mut(&c_left).unwrap() += 1;
                let left_anchor_x = net_endpoints
                    .iter()
                    .find(|rp| rp.col_idx == c_left)
                    .unwrap()
                    .col_right_x;
                let gx = snap_grid(left_anchor_x + (lane as f64) * GRID_MM);

                let mut segments = Vec::new();
                let mut ys: Vec<f64> = Vec::new();
                let mut y_counts: BTreeMap<i64, usize> = BTreeMap::new();

                for rp in &net_endpoints {
                    let tip = rp.pin.tip_point;
                    let y_entry = match rp.pin.side {
                        Side::East | Side::West => {
                            add_ortho_seg(&mut segments, tip, Point2::new(gx, tip.y));
                            tip.y
                        }
                        Side::North => {
                            let sy = snap_grid(tip.y - GRID_MM);
                            add_ortho_seg(&mut segments, tip, Point2::new(tip.x, sy));
                            add_ortho_seg(&mut segments, Point2::new(tip.x, sy), Point2::new(gx, sy));
                            sy
                        }
                        Side::South => {
                            let sy = snap_grid(tip.y + GRID_MM);
                            add_ortho_seg(&mut segments, tip, Point2::new(tip.x, sy));
                            add_ortho_seg(&mut segments, Point2::new(tip.x, sy), Point2::new(gx, sy));
                            sy
                        }
                    };
                    ys.push(y_entry);
                    *y_counts.entry((y_entry * 100.0).round() as i64).or_insert(0) += 1;
                }

                ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
                ys.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);
                let mut junctions = Vec::new();
                for pair in ys.windows(2) {
                    add_ortho_seg(&mut segments, Point2::new(gx, pair[0]), Point2::new(gx, pair[1]));
                }
                // Any y on `gx` that has >= 3 incident segments gets a clean junction dot!
                for (idx, &y_val) in ys.iter().enumerate() {
                    let key = (y_val * 100.0).round() as i64;
                    let horiz_deg = y_counts.get(&key).copied().unwrap_or(0);
                    let vert_deg = if ys.len() == 1 {
                        0
                    } else if idx == 0 || idx + 1 == ys.len() {
                        1
                    } else {
                        2
                    };
                    if horiz_deg + vert_deg >= 3 {
                        junctions.push(Point2::new(gx, y_val));
                    }
                }

                // Prefer placing label on the longest horizontal segment of this direct bridge
                let label_pos = segments
                    .iter()
                    .filter(|s| (s.p0.y - s.p1.y).abs() <= EPS_MM)
                    .max_by(|a, b| a.length().partial_cmp(&b.length()).unwrap())
                    .map(|s| Point2::new(snap_grid(0.5 * (s.p0.x + s.p1.x)), s.p0.y))
                    .or_else(|| Some(Point2::new(gx, ys[0])));

                schematic_routes.push(SchematicRouteIr {
                    net_id: net.name.clone(),
                    style: "direct".to_string(),
                    is_power: is_pwr,
                    is_ground: false,
                    segments,
                    junctions,
                    terminals: Vec::new(),
                    label_pos,
                });
                continue;
            }
        }

        // --- MODE D: Bus / Bottom Rail Routing (uses per-column local lanes!) ---
        let trunk_y = if is_gnd || style == NetRoutingStyle::Rail {
            let y = snap_grid(gnd_rail_y + (gnd_rail_track as f64) * 5.08);
            gnd_rail_track += 1;
            y
        } else {
            let y = snap_grid(bus_top_y + (top_bus_track as f64) * 5.08);
            top_bus_track += 1;
            y
        };

        // Allocate at most one local left lane and one local right lane per column touched by THIS net
        let mut net_left_x: BTreeMap<usize, f64> = BTreeMap::new();
        let mut net_right_x: BTreeMap<usize, f64> = BTreeMap::new();

        let mut segments: Vec<Segment2> = Vec::new();
        let mut risers: Vec<(f64, f64)> = Vec::new();

        for rp in &net_endpoints {
            let tip = rp.pin.tip_point;
            match rp.pin.side {
                Side::South if (is_gnd || style == NetRoutingStyle::Rail) && rp.is_bottom_in_col => {
                    risers.push((tip.x, tip.y));
                }
                Side::North if !is_gnd && rp.is_top_in_col => {
                    risers.push((tip.x, tip.y));
                }
                Side::West => {
                    let gx = *net_left_x.entry(rp.col_idx).or_insert_with(|| {
                        let lane = *left_lane_next.entry(rp.col_idx).or_insert(0);
                        *left_lane_next.get_mut(&rp.col_idx).unwrap() += 1;
                        snap_grid(rp.col_left_x - (lane as f64) * GRID_MM)
                    });
                    add_ortho_seg(&mut segments, tip, Point2::new(gx, tip.y));
                    risers.push((gx, tip.y));
                }
                Side::East => {
                    let gx = *net_right_x.entry(rp.col_idx).or_insert_with(|| {
                        let lane = *right_lane_next.entry(rp.col_idx).or_insert(0);
                        *right_lane_next.get_mut(&rp.col_idx).unwrap() += 1;
                        snap_grid(rp.col_right_x + (lane as f64) * GRID_MM)
                    });
                    add_ortho_seg(&mut segments, tip, Point2::new(gx, tip.y));
                    risers.push((gx, tip.y));
                }
                Side::North => {
                    let step_y = snap_grid(tip.y - GRID_MM);
                    let gx = *net_right_x.entry(rp.col_idx).or_insert_with(|| {
                        let lane = *right_lane_next.entry(rp.col_idx).or_insert(0);
                        *right_lane_next.get_mut(&rp.col_idx).unwrap() += 1;
                        snap_grid(rp.col_right_x + (lane as f64) * GRID_MM)
                    });
                    add_ortho_seg(&mut segments, tip, Point2::new(tip.x, step_y));
                    add_ortho_seg(&mut segments, Point2::new(tip.x, step_y), Point2::new(gx, step_y));
                    risers.push((gx, step_y));
                }
                Side::South => {
                    let step_y = snap_grid(tip.y + GRID_MM);
                    let gx = if is_gnd {
                        snap_grid(rp.col_left_x) // lane 0 reserved exclusively for GND rail!
                    } else {
                        *net_right_x.entry(rp.col_idx).or_insert_with(|| {
                            let lane = *right_lane_next.entry(rp.col_idx).or_insert(0);
                            *right_lane_next.get_mut(&rp.col_idx).unwrap() += 1;
                            snap_grid(rp.col_right_x + (lane as f64) * GRID_MM)
                        })
                    };
                    add_ortho_seg(&mut segments, tip, Point2::new(tip.x, step_y));
                    add_ortho_seg(&mut segments, Point2::new(tip.x, step_y), Point2::new(gx, step_y));
                    risers.push((gx, step_y));
                }
            }
        }

        let mut by_x: BTreeMap<i64, (f64, Vec<f64>)> = BTreeMap::new();
        for (rx, ry) in risers {
            let key = (rx * 100.0).round() as i64;
            let entry = by_x.entry(key).or_insert_with(|| (rx, Vec::new()));
            entry.1.push(ry);
        }

        let mut junctions: Vec<Point2> = Vec::new();
        let label_pos: Option<Point2>;

        if by_x.len() == 1 {
            let (_, (x, mut ys)) = by_x.into_iter().next().unwrap();
            ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
            ys.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);
            if ys.len() == 1 {
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
            let mut trunk_xs: Vec<f64> = Vec::new();
            for (_, (x, mut ys)) in by_x {
                ys.push(trunk_y);
                ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
                ys.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);
                for pair in ys.windows(2) {
                    add_ortho_seg(&mut segments, Point2::new(x, pair[0]), Point2::new(x, pair[1]));
                }
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

        let effective_style_str = if is_gnd || style == NetRoutingStyle::Rail {
            "rail"
        } else {
            "bus"
        };
        schematic_routes.push(SchematicRouteIr {
            net_id: net.name.clone(),
            style: effective_style_str.to_string(),
            is_power: is_pwr,
            is_ground: is_gnd,
            segments,
            junctions,
            terminals: Vec::new(),
            label_pos,
        });
    }

    // 5b. Post-pass: guarantee NO net label ever lands on a perpendicular crossing of another net!
    let route_count = schematic_routes.len();
    for i in 0..route_count {
        if schematic_routes[i].label_pos.is_none() {
            continue;
        }
        let mut chosen: Option<Point2> = None;
        let segs_i = schematic_routes[i].segments.clone();
        let mut candidates: Vec<Point2> = Vec::new();
        if let Some(lp) = schematic_routes[i].label_pos {
            candidates.push(lp);
        }
        for pass in 0..2 {
            for seg in &segs_i {
                let is_horiz = (seg.p0.y - seg.p1.y).abs() <= EPS_MM;
                if (pass == 0 && !is_horiz) || (pass == 1 && is_horiz) {
                    continue;
                }
                let steps = (seg.length() / GRID_MM).round() as i64;
                if steps <= 0 {
                    continue;
                }
                let mid = steps / 2;
                for delta in 0..=steps {
                    for s in [mid + delta, mid - delta] {
                        if s < 0 || s > steps {
                            continue;
                        }
                        let t = (s as f64) / (steps as f64);
                        candidates.push(Point2::new(
                            snap_grid(seg.p0.x + t * (seg.p1.x - seg.p0.x)),
                            snap_grid(seg.p0.y + t * (seg.p1.y - seg.p0.y)),
                        ));
                    }
                }
            }
        }
        for cand in candidates {
            let collides_other = (0..route_count).any(|j| {
                if j == i {
                    return false;
                }
                schematic_routes[j]
                    .segments
                    .iter()
                    .any(|s| point_on_ortho_segment(cand, *s))
            });
            if !collides_other {
                chosen = Some(cand);
                break;
            }
        }
        if let Some(pt) = chosen {
            schematic_routes[i].label_pos = Some(pt);
        }
    }

    // Compute tight actual sheet bounds from all routed coordinates
    let mut max_x = max_comp_x;
    let mut max_y = max_comp_y + 15.24;
    for r in &schematic_routes {
        for s in &r.segments {
            max_x = max_x.max(s.p0.x.max(s.p1.x) + 15.24);
            max_y = max_y.max(s.p0.y.max(s.p1.y) + 15.24);
        }
        for t in &r.terminals {
            max_x = max_x.max(t.pos.x + 15.24);
            max_y = max_y.max(t.pos.y + 15.24);
        }
    }
    let sheet_bounds = Rect2::new(0.0, 0.0, snap_grid(max_x.max(228.6)), snap_grid(max_y.max(152.4)));

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
    let _ = col_bounds;

    CanonicalIr {
        ir_version: "0.3.2".to_string(),
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

/// True iff all pins of `net` live in two adjacent columns (c, c+1) and face each other across the gap
/// between c and c+1 (col c uses East/North/South; col c+1 uses West).
fn can_direct_bridge_gap(
    net: &NetDecl,
    comp_col: &BTreeMap<String, usize>,
    comp_pin_sides: &BTreeMap<String, Vec<(&PinDecl, Side)>>,
) -> bool {
    let mut cols: Vec<usize> = net
        .connects
        .iter()
        .filter_map(|p| comp_col.get(&p.component).copied())
        .collect();
    cols.sort();
    cols.dedup();
    if cols.len() != 2 || cols[1] != cols[0] + 1 {
        return false;
    }
    let c_left = cols[0];
    let c_right = cols[1];

    for pref in &net.connects {
        let Some(&col) = comp_col.get(&pref.component) else {
            return false;
        };
        let Some(sides) = comp_pin_sides.get(&pref.component) else {
            return false;
        };
        let Some((_, side)) = sides.iter().find(|(p, _)| p.name == pref.pin) else {
            return false;
        };
        if col == c_left && *side == Side::West {
            return false;
        }
        if col == c_right && *side != Side::West {
            return false;
        }
    }
    true
}

fn point_on_ortho_segment(p: Point2, seg: Segment2) -> bool {
    let min_x = seg.p0.x.min(seg.p1.x) - 0.1;
    let max_x = seg.p0.x.max(seg.p1.x) + 0.1;
    let min_y = seg.p0.y.min(seg.p1.y) - 0.1;
    let max_y = seg.p0.y.max(seg.p1.y) + 0.1;
    p.x >= min_x && p.x <= max_x && p.y >= min_y && p.y <= max_y
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
    // If ALL pins are 2-pin passives with no explicit side override, orient vertically when 1 pin is GND
    if comp.pins.len() == 2
        && comp.pins.iter().all(|p| p.role == PinRole::Passive && p.side.is_none())
    {
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
        // Explicit `side:` override ALWAYS wins!
        if let Some(explicit_side) = pin.side {
            out.push((pin, explicit_side));
            continue;
        }
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

    let is_small_passive = comp.pins.len() <= 2
        && matches!(
            comp.category.as_str(),
            "resistor" | "capacitor" | "inductor" | "diode" | "led"
        );

    let max_west_len = assigned
        .iter()
        .filter(|(_, s)| *s == Side::West)
        .map(|(p, _)| p.name.len())
        .max()
        .unwrap_or(0);
    let max_east_len = assigned
        .iter()
        .filter(|(_, s)| *s == Side::East)
        .map(|(p, _)| p.name.len())
        .max()
        .unwrap_or(0);

    let min_w = if is_small_passive {
        if comp.value.len() >= 7 {
            15.24
        } else {
            12.70
        }
    } else {
        let pin_text_w = ((max_west_len + max_east_len) as f64) * 1.6 + 10.16;
        let val_w = (comp.value.len() as f64) * 1.8 + 7.62;
        pin_text_w.max(val_w).max(20.32)
    };
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
