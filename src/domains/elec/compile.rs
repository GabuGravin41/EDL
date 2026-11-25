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
        NetRoutingStyle::ZoneLocal => "zone_local",
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

    // Resolve extra project-local `.kicad_sym` libraries relative to source file directory
    let base_dir = std::path::Path::new(&project.span.file)
        .parent()
        .map(|p| p.to_path_buf());
    let extra_libs: Vec<std::path::PathBuf> = project
        .elec
        .symbol_libs
        .iter()
        .map(|rel| {
            let p = std::path::PathBuf::from(rel);
            if p.is_relative() {
                if let Some(ref b) = base_dir {
                    return b.join(p);
                }
            }
            p
        })
        .collect();

    // Collect components (auto-hydrating native `.kicad_sym` library symbols!) & nets across circuit blocks
    let mut resolved_comps: BTreeMap<String, ComponentDecl> = BTreeMap::new();
    let mut loaded_syms: BTreeMap<String, crate::domains::elec::kicad_sym::KiCadLibSymbol> =
        BTreeMap::new();
    // Alias map: (comp_id, user_ref_name_or_number) -> Vec<internal_unique_pin_name>
    let mut pin_aliases: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    // Pin display metadata for library pins: (comp_id, internal_pin_name) -> (display_name, pin_code)
    let mut pin_meta: BTreeMap<(String, String), (String, String)> = BTreeMap::new();
    let mut comp_order: Vec<String> = Vec::new();
    let mut all_nets: Vec<&NetDecl> = Vec::new();

    for circuit in &project.elec.circuits {
        for raw_comp in &circuit.components {
            if resolved_comps.contains_key(&raw_comp.id) {
                diagnostics.push(
                    Diagnostic::error(
                        "E_DUP_COMPONENT",
                        "semantics",
                        format!("Duplicate component designator '{}'", raw_comp.id),
                        Some(raw_comp.span.clone()),
                    )
                    .with_entities(vec![raw_comp.id.clone()]),
                );
                continue;
            }
            let mut comp = raw_comp.clone();

            // If category contains ':', resolve it from KiCad `.kicad_sym` libraries!
            if comp.category.contains(':') {
                match crate::domains::elec::kicad_sym::load_symbol(&comp.category, &extra_libs) {
                    Ok(sym) => {
                        let target_unit = comp.unit.unwrap_or(1);
                        if comp.value.is_empty() {
                            comp.value = sym.default_value.clone();
                        }
                        if comp.footprint.is_none() {
                            comp.footprint = sym.default_footprint.clone();
                        }
                        let user_overrides: BTreeMap<String, PinDecl> = comp
                            .pins
                            .drain(..)
                            .map(|p| (p.name.clone(), p))
                            .collect();

                        let unit_pins: Vec<_> = sym
                            .pins
                            .iter()
                            .filter(|p| (p.unit == 0 || p.unit == target_unit) && !p.hide)
                            .collect();
                        let mut name_counts: BTreeMap<&str, usize> = BTreeMap::new();
                        for lp in &unit_pins {
                            *name_counts.entry(lp.name.as_str()).or_insert(0) += 1;
                        }
                        for lp in unit_pins {
                            let dup = name_counts.get(lp.name.as_str()).copied().unwrap_or(0) > 1;
                            let unique_name = if dup {
                                format!("{}#{}", lp.name, lp.number)
                            } else {
                                lp.name.clone()
                            };
                            let ov = user_overrides
                                .get(&lp.number)
                                .or_else(|| user_overrides.get(&lp.name));
                            let is_nc = comp.no_connect.iter().any(|n| n == &lp.name || n == &lp.number)
                                || lp.role == PinRole::Nc;
                            let role = if is_nc {
                                PinRole::Nc
                            } else {
                                ov.map(|o| o.role).unwrap_or(lp.role)
                            };
                            let voltage = ov.and_then(|o| o.voltage);
                            comp.pins.push(PinDecl {
                                name: unique_name.clone(),
                                role,
                                voltage,
                                side: None,
                                span: comp.span.clone(),
                            });
                            pin_meta.insert(
                                (comp.id.clone(), unique_name.clone()),
                                (lp.name.clone(), lp.number.clone()),
                            );
                            pin_aliases
                                .entry((comp.id.clone(), lp.name.clone()))
                                .or_default()
                                .push(unique_name.clone());
                            if lp.number != lp.name {
                                pin_aliases
                                    .entry((comp.id.clone(), lp.number.clone()))
                                    .or_default()
                                    .push(unique_name.clone());
                            }
                            if unique_name != lp.name && unique_name != lp.number {
                                pin_aliases
                                    .entry((comp.id.clone(), unique_name.clone()))
                                    .or_default()
                                    .push(unique_name);
                            }
                        }
                        loaded_syms.insert(comp.id.clone(), sym);
                    }
                    Err(msg) => {
                        diagnostics.push(
                            Diagnostic::error(
                                "E_SYMBOL_NOT_FOUND",
                                "symbols",
                                format!("Component '{}': {msg}", comp.id),
                                Some(comp.span.clone()),
                            )
                            .with_entities(vec![comp.id.clone()]),
                        );
                    }
                }
            } else {
                // Built-in/custom component: apply explicit `no_connect: [...]` & register numeric/name aliases
                for (idx, p) in comp.pins.iter_mut().enumerate() {
                    let num_s = format!("{}", idx + 1);
                    if comp.no_connect.iter().any(|n| n == &p.name || n == &num_s) {
                        p.role = PinRole::Nc;
                    }
                    pin_aliases
                        .entry((comp.id.clone(), p.name.clone()))
                        .or_default()
                        .push(p.name.clone());
                    if num_s != p.name {
                        pin_aliases
                            .entry((comp.id.clone(), num_s))
                            .or_default()
                            .push(p.name.clone());
                    }
                }
            }

            comp_order.push(comp.id.clone());
            resolved_comps.insert(comp.id.clone(), comp);
        }
        for net in &circuit.nets {
            all_nets.push(net);
        }
    }

    let comp_map: BTreeMap<String, &ComponentDecl> = resolved_comps
        .iter()
        .map(|(k, v)| (k.clone(), v))
        .collect();

    // 1. Validate pin references & build functional connectivity graph (supporting pin names AND pin numbers!)
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
            let matched_names: Vec<String> = pin_aliases
                .get(&(pref.component.clone(), pref.pin.clone()))
                .cloned()
                .unwrap_or_default();
            if matched_names.is_empty() {
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
            }
            for unique_pin_name in matched_names {
                let Some(pin) = comp.pins.iter().find(|p| p.name == unique_pin_name) else {
                    continue;
                };
                if pin.role == PinRole::Ground {
                    ground_nets.insert(net.name.clone());
                }
                pin_to_nets
                    .entry((pref.component.clone(), unique_pin_name.clone()))
                    .or_default()
                    .push(net.name.clone());
                entry.push((pref.component.clone(), unique_pin_name, pin));
            }
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

    // 2b. Series-passive DC rail propagation (ferrite beads, inductors, fuses, switches, <=10 ohm resistors)
    // so downstream `power_in` pins are accurately voltage-checked by E4 and driven cleanly in KiCad 9!
    let mut propagated_power_nets: BTreeSet<String> = BTreeSet::new();
    for net in &all_nets {
        if net.power_flag {
            propagated_power_nets.insert(net.name.clone());
        }
    }
    for _pass in 0..8 {
        let mut changed = false;
        for cid in &comp_order {
            let comp = comp_map[cid];
            if comp.pins.len() != 2 {
                continue;
            }
            let cat = comp.category.to_lowercase();
            let is_series_dc_short = matches!(
                cat.as_str(),
                "inductor" | "l" | "ferrite" | "fuse" | "switch" | "button" | "sw"
            ) || (matches!(cat.as_str(), "resistor" | "r")
                && crate::domains::elec::spice::parse_si_component_value(&comp.value, "R")
                    .map(|r| r <= 10.0)
                    .unwrap_or(false));
            if !is_series_dc_short {
                continue;
            }
            let n0 = pin_to_nets
                .get(&(cid.clone(), comp.pins[0].name.clone()))
                .and_then(|v| v.first())
                .cloned();
            let n1 = pin_to_nets
                .get(&(cid.clone(), comp.pins[1].name.clone()))
                .and_then(|v| v.first())
                .cloned();
            let (Some(net_a), Some(net_b)) = (n0, n1) else {
                continue;
            };
            if net_a == net_b || ground_nets.contains(&net_a) || ground_nets.contains(&net_b) {
                continue;
            }
            let va = net_driven_voltage
                .get(&net_a)
                .and_then(|d| d.first().map(|(_, v)| *v));
            let vb = net_driven_voltage
                .get(&net_b)
                .and_then(|d| d.first().map(|(_, v)| *v));
            if let (Some(v), None) = (va, vb) {
                net_driven_voltage
                    .entry(net_b.clone())
                    .or_default()
                    .push((format!("{cid}.{} (via {cid})", comp.pins[1].name), v));
                propagated_power_nets.insert(net_b.clone());
                if let Some(rel) = relations.iter_mut().find(|r| r.id == net_b) {
                    rel.driven_voltage_v = Some(v);
                }
                changed = true;
            } else if let (None, Some(v)) = (va, vb) {
                net_driven_voltage
                    .entry(net_a.clone())
                    .or_default()
                    .push((format!("{cid}.{} (via {cid})", comp.pins[0].name), v));
                propagated_power_nets.insert(net_a.clone());
                if let Some(rel) = relations.iter_mut().find(|r| r.id == net_a) {
                    rel.driven_voltage_v = Some(v);
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let _ = propagated_power_nets;

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

            VerifyRule::GalvanicIsolation { net_a, net_b, span } => {
                // Conductive net-to-net BFS across all NON-isolated components (transformers/optocouplers act as barriers!)
                let mut adj: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
                for (cid, comp) in &comp_map {
                    if comp.isolated {
                        continue;
                    }
                    let mut c_nets: BTreeSet<String> = BTreeSet::new();
                    for pin in &comp.pins {
                        if let Some(ns) = pin_to_nets.get(&(cid.clone(), pin.name.clone())) {
                            for n in ns {
                                c_nets.insert(n.clone());
                            }
                        }
                    }
                    let nvec: Vec<String> = c_nets.into_iter().collect();
                    for i in 0..nvec.len() {
                        for j in (i + 1)..nvec.len() {
                            adj.entry(nvec[i].clone())
                                .or_default()
                                .push((nvec[j].clone(), cid.clone()));
                            adj.entry(nvec[j].clone())
                                .or_default()
                                .push((nvec[i].clone(), cid.clone()));
                        }
                    }
                }
                let mut parent: BTreeMap<String, (String, String)> = BTreeMap::new();
                let mut q = std::collections::VecDeque::new();
                let mut visited: BTreeSet<String> = BTreeSet::new();
                visited.insert(net_a.clone());
                q.push_back(net_a.clone());
                let mut breached = false;
                while let Some(cur) = q.pop_front() {
                    if &cur == net_b {
                        breached = true;
                        break;
                    }
                    if let Some(nbrs) = adj.get(&cur) {
                        for (nxt, via) in nbrs {
                            if visited.insert(nxt.clone()) {
                                parent.insert(nxt.clone(), (cur.clone(), via.clone()));
                                q.push_back(nxt.clone());
                            }
                        }
                    }
                }
                let check_id = format!("E6_galvanic_isolation_{}_{}", net_a, net_b);
                if breached {
                    let mut path_comps = Vec::new();
                    let mut cur = net_b.clone();
                    while let Some((prev, via)) = parent.get(&cur) {
                        path_comps.push(via.clone());
                        cur = prev.clone();
                    }
                    path_comps.reverse();
                    diagnostics.push(
                        Diagnostic::error(
                            "E_ERC_ISOLATION_BREACH",
                            "erc",
                            format!(
                                "Galvanic isolation breached between '{}' and '{}' via non-isolated component(s): [{}]",
                                net_a,
                                net_b,
                                path_comps.join(" -> ")
                            ),
                            Some(span.clone()),
                        )
                        .with_entities(vec![net_a.clone(), net_b.clone()])
                        .with_suggestion("Route cross-barrier signals/power only through components marked `isolated: true` (e.g. transformers or optocouplers)"),
                    );
                    evidence.push(EvidenceRecord {
                        check_id,
                        status: VerificationStatus::Violated,
                        method: "exact_conductive_graph_bfs_v1.1".to_string(),
                        tolerance_mm: None,
                        subjects: vec![net_a.clone(), net_b.clone()],
                        details: format!("Conductive path found via [{}]", path_comps.join(" -> ")),
                    });
                } else {
                    evidence.push(EvidenceRecord {
                        check_id,
                        status: VerificationStatus::ExactlyChecked,
                        method: "exact_conductive_graph_bfs_v1.1".to_string(),
                        tolerance_mm: None,
                        subjects: vec![net_a.clone(), net_b.clone()],
                        details: format!("0 conductive paths between '{}' and '{}' (galvanic barrier intact)", net_a, net_b),
                    });
                }
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

    // 3. Multi-sheet partitioning & cross-sheet global net detection
    let sheet_list: Vec<Option<&SchematicDecl>> = if project.elec.schematics.is_empty() {
        vec![None]
    } else {
        project.elec.schematics.iter().map(Some).collect()
    };
    let total_sheets = sheet_list.len();
    let mut comp_to_sheet: BTreeMap<String, usize> = BTreeMap::new();
    for (s_idx, s_opt) in sheet_list.iter().enumerate() {
        if let Some(sc) = s_opt {
            for g in &sc.groups {
                for m in &g.members {
                    comp_to_sheet.entry(m.clone()).or_insert(s_idx);
                }
            }
            for pl in &sc.placements {
                comp_to_sheet.entry(pl.component.clone()).or_insert(s_idx);
            }
        }
    }
    for cid in &comp_order {
        comp_to_sheet.entry(cid.clone()).or_insert(0);
    }

    let mut cross_sheet_nets: BTreeSet<String> = BTreeSet::new();
    if total_sheets > 1 {
        for net in &all_nets {
            let sheets_hit: BTreeSet<usize> = net_pins
                .get(&net.name)
                .map(|ps| {
                    ps.iter()
                        .filter_map(|(cid, _, _)| comp_to_sheet.get(cid).copied())
                        .collect()
                })
                .unwrap_or_default();
            if sheets_hit.len() > 1 {
                cross_sheet_nets.insert(net.name.clone());
            }
        }
    }

    // Pre-assign pin sides for every component once across all sheets
    let mut comp_pin_sides: BTreeMap<String, Vec<(&PinDecl, Side)>> = BTreeMap::new();
    for cid in &comp_order {
        let comp = comp_map[cid];
        comp_pin_sides.insert(cid.clone(), assign_pin_sides(comp, &pin_to_nets, &ground_nets));
    }

    let mut entities: Vec<EntityIr> = Vec::new();
    let mut schematic_groups: Vec<SchematicGroupIr> = Vec::new();
    let mut schematic_routes: Vec<SchematicRouteIr> = Vec::new();
    let mut views: Vec<ViewIr> = Vec::new();
    let mut sheet_bounds = Rect2::new(0.0, 0.0, 228.6, 152.4);

    for (s_idx, schem) in sheet_list.iter().copied().enumerate() {
        let sheet_name_str = schem
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Main".to_string());
        let sheet_tag: Option<String> = if total_sheets > 1 {
            Some(sheet_name_str.clone())
        } else {
            None
        };
        let sheet_comps: Vec<String> = comp_order
            .iter()
            .filter(|cid| comp_to_sheet.get(*cid).copied().unwrap_or(0) == s_idx)
            .cloned()
            .collect();
    // 3. Resolve routing policies per net (layered: per-net override > category policy > smart default)
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
        let def_st = routing_cfg
            .and_then(|r| r.default_style)
            .unwrap_or(NetRoutingStyle::Direct);
        if is_gnd {
            return routing_cfg
                .and_then(|r| r.ground_style)
                .unwrap_or(if def_st == NetRoutingStyle::ZoneLocal {
                    NetRoutingStyle::Symbol
                } else {
                    NetRoutingStyle::Rail
                });
        }
        if is_pwr {
            return routing_cfg
                .and_then(|r| r.power_style)
                .unwrap_or(if def_st == NetRoutingStyle::ZoneLocal {
                    NetRoutingStyle::Symbol
                } else {
                    NetRoutingStyle::Direct
                });
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
            || is_power_rail_name(&n.name);
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

    // Build 2D row/column group layout (supports multi-row `row [...]` bands OR classic 1D `arrange`)
    let mut ordered_groups: Vec<(usize, usize, String, Vec<String>)> = Vec::new(); // (row_idx, col_in_row, name, members)
    let mut assigned: BTreeSet<String> = BTreeSet::new();

    let row_gap_mm = snap_grid(
        schem
            .and_then(|s| s.layout.as_ref())
            .and_then(|l| l.row_gap)
            .and_then(|q| q.to_mm().ok())
            .unwrap_or(25.4),
    )
    .max(17.78);

    if let Some(s) = schem {
        let gmap: BTreeMap<&str, &SchematicGroupDecl> =
            s.groups.iter().map(|g| (g.name.as_str(), g)).collect();
        let row_specs: Vec<Vec<String>> = match s.layout.as_ref() {
            Some(l) if !l.rows.is_empty() => l.rows.clone(),
            Some(l) if !l.arrange.is_empty() => vec![l.arrange.clone()],
            _ => vec![s.groups.iter().map(|g| g.name.clone()).collect()],
        };

        for (r_idx, r_names) in row_specs.into_iter().enumerate() {
            let mut c_in_row = 0usize;
            for gname in r_names {
                if let Some(g) = gmap.get(gname.as_str()) {
                    let mut valid_members = Vec::new();
                    for m in &g.members {
                        if comp_map.contains_key(m) && assigned.insert(m.clone()) {
                            valid_members.push(m.clone());
                        }
                    }
                    if !valid_members.is_empty() {
                        ordered_groups.push((r_idx, c_in_row, g.name.clone(), valid_members));
                        c_in_row += 1;
                    }
                }
            }
        }
    }
    let leftover: Vec<String> = sheet_comps
        .iter()
        .filter(|id| !assigned.contains(*id))
        .cloned()
        .collect();
    if !leftover.is_empty() {
        let next_col = ordered_groups
            .iter()
            .filter(|(r, _, _, _)| *r == 0)
            .count();
        ordered_groups.push((0, next_col, "Components".to_string(), leftover));
    }

    // Build comp -> (group_flat_idx, row_idx, col_in_row) maps
    let mut comp_col_map: BTreeMap<String, usize> = BTreeMap::new();
    let mut comp_row_map: BTreeMap<String, usize> = BTreeMap::new();
    for (g_idx, &(r_idx, _c_in_row, _, ref members)) in ordered_groups.iter().enumerate() {
        for m in members {
            comp_col_map.insert(m.clone(), g_idx);
            comp_row_map.insert(m.clone(), r_idx);
        }
    }



    let is_multi_row = ordered_groups.iter().any(|(r, _, _, _)| *r > 0);

    // Count how many nets will actually use the top bus so we only reserve exact space needed!
    let mut predicted_bus_nets = 0usize;
    for net in &all_nets {
        let is_gnd = ground_nets.contains(&net.name);
        let is_pwr = net_driven_voltage
            .get(&net.name)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || is_power_rail_name(&net.name);
        let st = resolve_style(&net.name, is_gnd, is_pwr);
        let rows_spanned: BTreeSet<usize> = net
            .connects
            .iter()
            .filter_map(|p| comp_row_map.get(&p.component).copied())
            .collect();
        if st == NetRoutingStyle::ZoneLocal || cross_sheet_nets.contains(&net.name) || (rows_spanned.len() > 1 && matches!(st, NetRoutingStyle::Direct | NetRoutingStyle::Bus)) {
            continue; // cross-row nets automatically use clean local label_stub terminals!
        }
        if matches!(st, NetRoutingStyle::Bus) {
            predicted_bus_nets += 1;
        } else if st == NetRoutingStyle::Direct && !is_gnd {
            let cols: BTreeSet<usize> = net
                .connects
                .iter()
                .filter_map(|p| comp_col_map.get(&p.component).copied())
                .collect();
            if cols.len() > 2 || (cols.len() == 2 && !can_direct_bridge_gap(net, &comp_col_map, &comp_row_map, &comp_pin_sides)) {
                predicted_bus_nets += 1;
            }
        }
    }

    // Compact inter-column gap: wide enough for direct local lanes & any bus risers without bloating!
    let local_gutter_budget = snap_grid(((predicted_bus_nets + 3).min(7) as f64) * 2.0 * GRID_MM);
    let effective_group_gap = group_gap_mm.max(local_gutter_budget);

    let bus_top_y = snap_grid(25.4);
    let bus_tracks_h = if predicted_bus_nets == 0 || is_multi_row {
        0.0
    } else {
        (predicted_bus_nets as f64) * 5.08 + 7.62
    };
    let start_x = snap_grid(30.48 + ((predicted_bus_nets.min(4) + 1) as f64) * GRID_MM);
    let mut cur_x = start_x;
    let mut row_start_y = snap_grid(bus_top_y + bus_tracks_h + 12.70);
    let mut cur_row_idx = 0usize;
    let mut cur_row_max_y = row_start_y + 25.4;

    let mut local_entities: Vec<EntityIr> = Vec::new();
    let mut local_groups: Vec<SchematicGroupIr> = Vec::new();
    let mut col_bounds: Vec<(f64, f64)> = Vec::new(); // (col_x0, col_x1)

    #[derive(Clone)]
    struct RoutablePin {
        pin: PlacedPinIr,
        row_idx: usize,
        col_idx: usize,
        col_left_x: f64,
        col_right_x: f64,
        body_center_x: f64,
        body_min_y: f64,
        comp_has_north_pin: bool,
        is_top_in_col: bool,
        is_bottom_in_col: bool,
    }
    let mut placed_pins: BTreeMap<(String, String), RoutablePin> = BTreeMap::new();
    let mut max_comp_y = row_start_y + 25.4;
    let mut max_comp_x = cur_x + 50.8;

    let grp_decl_map: BTreeMap<&str, &SchematicGroupDecl> = schem
        .map(|s| s.groups.iter().map(|g| (g.name.as_str(), g)).collect())
        .unwrap_or_default();

    for (col_idx, &(r_idx, _c_in_row, ref gname, ref members)) in ordered_groups.iter().enumerate() {
        if r_idx != cur_row_idx {
            cur_row_idx = r_idx;
            cur_x = start_x;
            row_start_y = snap_grid(cur_row_max_y + row_gap_mm);
            cur_row_max_y = row_start_y + 25.4;
        }
        let mut col_body_w = 15.24_f64;
        let mut sizes: Vec<(f64, f64)> = Vec::new();
        let mut size_map: BTreeMap<String, (f64, f64)> = BTreeMap::new();
        for cid in members {
            let comp = comp_map[cid];
            let (bw, bh) = if let Some(sym) = loaded_syms.get(cid) {
                let u = comp.unit.unwrap_or(1);
                let lb = crate::domains::elec::kicad_sym::symbol_unit_body_bounds(
                    sym,
                    u,
                    comp.orient_deg,
                    comp.mirror,
                );
                (snap_grid(lb.width().max(5.08)), snap_grid(lb.depth().max(5.08)))
            } else {
                let assigned_sides = &comp_pin_sides[cid];
                compute_symbol_size(comp, assigned_sides)
            };
            col_body_w = col_body_w.max(bw);
            size_map.insert(cid.clone(), (bw, bh));
            sizes.push((bw, bh));
        }

        // If this group/zone declares a 2D `hstack`/`vstack`/`subzone` tree, lay it out recursively!
        let gdecl_opt = grp_decl_map.get(gname.as_str()).copied();
        let mut tree_pos: BTreeMap<String, (f64, f64)> = BTreeMap::new();
        let mut g_subzones: Vec<SchematicSubZoneIr> = Vec::new();
        let mut tree_bbox_opt: Option<Rect2> = None;
        if let Some(lt) = gdecl_opt.and_then(|g| g.layout_tree.as_ref()) {
            let sz_fn = |cid: &str| size_map.get(cid).copied().unwrap_or((15.24, 10.16));
            let title_offset = if gdecl_opt.and_then(|g| g.title.as_ref()).is_some() {
                7.62
            } else {
                0.0
            };
            let tb = crate::domains::elec::sheet_layout::place_layout_node(
                lt,
                cur_x,
                snap_grid(row_start_y + title_offset),
                &sz_fn,
                7.62,
                7.62,
                &mut tree_pos,
                &mut g_subzones,
            );
            col_body_w = col_body_w.max(tb.width());
            tree_bbox_opt = Some(tb);
        }

        let col_x0 = snap_grid(cur_x);
        let col_x1 = snap_grid(col_x0 + col_body_w);
        col_bounds.push((col_x0, col_x1));

        let mut cur_y = row_start_y;
        let mut g_min_x = f64::INFINITY;
        let mut g_max_x = f64::NEG_INFINITY;
        let mut g_min_y = f64::INFINITY;
        let mut g_max_y = f64::NEG_INFINITY;

        for (m_idx, (cid, (bw, bh))) in members.iter().zip(sizes.into_iter()).enumerate() {
            let comp = comp_map[cid];
            let assigned_sides = &comp_pin_sides[cid];

            let (bx0, by0) = if let Some(&(ex, ey)) = explicit_pos.get(cid) {
                (ex, ey)
            } else if let Some(&(tx, ty)) = tree_pos.get(cid) {
                (tx, ty)
            } else {
                (snap_grid(col_x0 + (col_body_w - bw) * 0.5), snap_grid(cur_y))
            };
            let (body, placed, lib_id_opt, hide_pin_names, symbol_prims, routing_keys): (
                Rect2,
                Vec<PlacedPinIr>,
                Option<String>,
                bool,
                Option<Vec<SymPrim>>,
                Vec<String>,
            ) = if let Some(sym) = loaded_syms.get(cid) {
                let u = comp.unit.unwrap_or(1);
                let lb = crate::domains::elec::kicad_sym::symbol_unit_body_bounds(
                    sym,
                    u,
                    comp.orient_deg,
                    comp.mirror,
                );
                let mut cx = snap_grid(bx0 - lb.min_x);
                let mut cy = snap_grid(by0 - lb.min_y);
                // If a KiCad library symbol's pins sit at half-grid offsets (e.g. Device:R/C at ±3.81mm = 1.5*2.54mm),
                // offset the symbol center by 1.27mm so EVERY pin tip lands cleanly on the 2.54mm grid with exact pin length!
                let first_unit_pin = sym.pins.iter().find(|p| (p.unit == 0 || p.unit == u) && !p.hide);
                if let Some(fp) = first_unit_pin {
                    let (rx, ry) = crate::domains::elec::kicad_sym::transform_local_yup(
                        fp.x,
                        fp.y,
                        comp.orient_deg,
                        comp.mirror,
                    );
                    if (rx - snap_grid(rx)).abs() > 0.5 {
                        cx += GRID_MM * 0.5;
                    }
                    if (ry - snap_grid(ry)).abs() > 0.5 {
                        cy += GRID_MM * 0.5;
                    }
                }
                let body = Rect2::new(
                    cx + lb.min_x,
                    cy + lb.min_y,
                    cx + lb.max_x,
                    cy + lb.max_y,
                );
                let graphic = crate::domains::elec::kicad_sym::build_kicad_symbol_graphic(
                    sym,
                    u,
                    cx,
                    cy,
                    comp.orient_deg,
                    comp.mirror,
                );
                let unit_pins: Vec<_> = sym
                    .pins
                    .iter()
                    .filter(|p| (p.unit == 0 || p.unit == u) && !p.hide)
                    .collect();
                let mut placed_vec = Vec::new();
                let mut rkeys = Vec::new();
                for (pidx, (lp, cpin)) in unit_pins.iter().zip(comp.pins.iter()).enumerate() {
                    let tip = crate::domains::elec::kicad_sym::local_to_world(
                        cx,
                        cy,
                        lp.x,
                        lp.y,
                        comp.orient_deg,
                        comp.mirror,
                    );
                    let rad = lp.angle.to_radians();
                    let body_pt = crate::domains::elec::kicad_sym::local_to_world(
                        cx,
                        cy,
                        lp.x + lp.length * rad.cos(),
                        lp.y + lp.length * rad.sin(),
                        comp.orient_deg,
                        comp.mirror,
                    );
                    let side = crate::domains::elec::kicad_sym::pin_outward_side(
                        lp.angle,
                        comp.orient_deg,
                        comp.mirror,
                    );
                    let (disp_name, code_str) = pin_meta
                        .get(&(cid.clone(), cpin.name.clone()))
                        .cloned()
                        .unwrap_or_else(|| (lp.name.clone(), lp.number.clone()));
                    let num_usize = code_str.parse::<usize>().unwrap_or(pidx + 1);
                    let is_nc = cpin.role == PinRole::Nc;
                    placed_vec.push(PlacedPinIr {
                        name: disp_name,
                        number: num_usize,
                        pin_code: Some(code_str),
                        role: cpin.role,
                        voltage: cpin.voltage,
                        no_connect: is_nc,
                        side,
                        body_point: body_pt,
                        tip_point: Point2::new(snap_grid(tip.x), snap_grid(tip.y)),
                    });
                    rkeys.push(cpin.name.clone());
                }
                (
                    body,
                    placed_vec,
                    Some(sym.lib_id.clone()),
                    graphic.hide_pin_names,
                    Some(graphic.primitives),
                    rkeys,
                )
            } else {
                let body = Rect2::new(bx0, by0, snap_grid(bx0 + bw), snap_grid(by0 + bh));
                let placed = place_pins_on_body(comp, assigned_sides, body);
                let rkeys = placed.iter().map(|p| p.name.clone()).collect();
                (body, placed, None, false, None, rkeys)
            };

            let local_left = snap_grid(col_x0.min(body.min_x) - PIN_LEN_MM - 2.54);
            let local_right = snap_grid(col_x1.max(body.max_x) + PIN_LEN_MM + 2.54);

            let has_north = placed.iter().any(|p| p.side == Side::North);
            for (p, rkey) in placed.iter().zip(routing_keys.into_iter()) {
                placed_pins.insert(
                    (cid.clone(), rkey),
                    RoutablePin {
                        pin: p.clone(),
                        row_idx: r_idx,
                        col_idx,
                        col_left_x: local_left,
                        col_right_x: local_right,
                        body_center_x: body.center().x,
                        body_min_y: body.min_y,
                        comp_has_north_pin: has_north,
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
            if let Some(ref lid) = lib_id_opt {
                props.insert("lib_id".to_string(), json!(lid));
            }
            if let Some(ref st) = sheet_tag {
                props.insert("sheet".to_string(), json!(st));
            }
            local_entities.push(EntityIr {
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
                    lib_id: lib_id_opt,
                    orient_deg: comp.orient_deg,
                    mirror: comp.mirror.map(|m| match m {
                        crate::domains::elec::kicad_sym::SymMirror::X => "x".to_string(),
                        crate::domains::elec::kicad_sym::SymMirror::Y => "y".to_string(),
                    }),
                    unit: comp.unit,
                    hide_pin_names,
                    symbol_prims,
                    body_rect: body,
                    pins: placed,
                },
            });

            if !explicit_pos.contains_key(cid) {
                cur_y = snap_grid(body.max_y + member_gap_mm + PIN_LEN_MM);
            }
            let top_pad = if has_north { PIN_LEN_MM + 11.43 } else { PIN_LEN_MM + 7.62 };
            g_min_x = g_min_x.min(body.min_x - PIN_LEN_MM - 5.08);
            g_max_x = g_max_x.max(body.max_x + PIN_LEN_MM + 5.08);
            g_min_y = g_min_y.min(body.min_y - top_pad);
            g_max_y = g_max_y.max(body.max_y + PIN_LEN_MM + 5.08);
            cur_row_max_y = cur_row_max_y.max(body.max_y + PIN_LEN_MM + 5.08);
            max_comp_y = max_comp_y.max(body.max_y + PIN_LEN_MM);
            max_comp_x = max_comp_x.max(body.max_x + PIN_LEN_MM + 12.70);
        }

        if let Some(tb) = tree_bbox_opt {
            let title_hdr = if gdecl_opt.and_then(|g| g.title.as_ref()).is_some() {
                7.62
            } else {
                2.54
            };
            g_min_x = g_min_x.min(tb.min_x - 2.54);
            g_max_x = g_max_x.max(tb.max_x + 2.54);
            g_min_y = g_min_y.min(tb.min_y - title_hdr);
            g_max_y = g_max_y.max(tb.max_y + 2.54);
            cur_row_max_y = cur_row_max_y.max(g_max_y);
            max_comp_x = max_comp_x.max(g_max_x + 7.62);
            max_comp_y = max_comp_y.max(g_max_y);
        }
        let g_notes = gdecl_opt.map(|g| g.notes.clone()).unwrap_or_default();
        if !g_notes.is_empty() && g_max_y.is_finite() {
            let note_h = (g_notes.len() as f64) * 6.35 + 5.08;
            g_max_y = snap_grid(g_max_y + note_h);
            cur_row_max_y = cur_row_max_y.max(g_max_y);
            max_comp_y = max_comp_y.max(g_max_y);
        }
        if g_min_x.is_finite() {
            local_groups.push(SchematicGroupIr {
                name: gname.clone(),
                sheet: sheet_tag.clone(),
                title: gdecl_opt.and_then(|g| g.title.clone()),
                border: gdecl_opt.map(|g| g.border.clone()),
                members: members.clone(),
                bbox: Rect2::new(g_min_x, g_min_y, g_max_x, g_max_y),
                notes: g_notes,
                subzones: g_subzones,
            });
        }

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
    // Reserve gutter lanes occupied by East/West `symbol` (1 grid) or `label_stub` (2 grids)
    // so vertical wire trunks NEVER cross a local power symbol or net-label stub!
    for net in &all_nets {
        let is_g = ground_nets.contains(&net.name);
        let is_p = net_driven_voltage
            .get(&net.name)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || is_power_rail_name(&net.name);
        let st = resolve_style(&net.name, is_g, is_p);
        let min_lane = match st {
            NetRoutingStyle::Symbol => 1,
            NetRoutingStyle::LabelStub => 2,
            _ => 0,
        };
        if min_lane == 0 {
            continue;
        }
        if let Some(npins) = net_pins.get(&net.name) {
            for (cid, pname, _) in npins {
                if let Some(rp) = placed_pins.get(&(cid.clone(), pname.clone())) {
                    match rp.pin.side {
                        Side::East => {
                            let e = right_lane_next.entry(rp.col_idx).or_insert(0);
                            if *e < min_lane {
                                *e = min_lane;
                            }
                        }
                        Side::West => {
                            let e = left_lane_next.entry(rp.col_idx).or_insert(0);
                            if *e < min_lane {
                                *e = min_lane;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let gnd_rail_y = snap_grid(max_comp_y + 12.70);
    let mut local_routes: Vec<SchematicRouteIr> = Vec::new();
    let mut astar_used_cells: BTreeSet<(i32, i32)> = BTreeSet::new();
    let mut top_bus_track = 0usize;
    let mut gnd_rail_track = 0usize;

    for net in &all_nets {
        let is_gnd = ground_nets.contains(&net.name);
        let net_cols_spanned: BTreeSet<usize> = net
            .connects
            .iter()
            .filter_map(|p| comp_col_map.get(&p.component).copied())
            .collect();
        let has_power_pin = net_pins
            .get(&net.name)
            .map(|ps| {
                ps.iter()
                    .any(|(_, _, p)| matches!(p.role, PinRole::PowerIn | PinRole::PowerOut))
            })
            .unwrap_or(false);
        let is_driven = net_driven_voltage
            .get(&net.name)
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        // Treat a net as a symbol-routed power distribution rail if it's a named rail (V3V3/VDD/VCC),
        // or a cross-zone / multi-drop driven power rail. Single-zone 2-3 pin series filter nodes
        // (like AC_L_FUSE -> F1 -> AC_NTC -> R10 -> L1) stay crisp direct wires!
        let is_pwr = is_power_rail_name(&net.name)
            || (net.power_flag && (net_cols_spanned.len() > 1 || net.connects.len() >= 4))
            || (is_driven && has_power_pin && (net_cols_spanned.len() > 1 || net.connects.len() >= 3));

        let style = resolve_style(&net.name, is_gnd, is_pwr);

        let mut net_endpoints: Vec<&RoutablePin> = Vec::new();
        if let Some(npins) = net_pins.get(&net.name) {
            for (cid, pname, _) in npins {
                if let Some(rp) = placed_pins.get(&(cid.clone(), pname.clone())) {
                    net_endpoints.push(rp);
                }
            }
        }
        if net_endpoints.is_empty() {
            continue;
        }

        // --- MODE A: Local Symbol (GND triangles / upward power flags; bridges adjacent same-zone pins!) ---
        if style == NetRoutingStyle::Symbol {
            let mut segments = Vec::new();
            let mut junctions = Vec::new();
            let mut terminals = Vec::new();
            let mut handled = vec![false; net_endpoints.len()];
            let all_bodies: Vec<Rect2> = local_entities
                .iter()
                .filter_map(|e| match &e.geometry {
                    EntityGeometryIr::Component { body_rect, .. } => Some(*body_rect),
                    _ => None,
                })
                .collect();

            for i in 0..net_endpoints.len() {
                if handled[i] {
                    continue;
                }
                let rpi = net_endpoints[i];
                let can_cluster = (is_gnd && rpi.pin.side == Side::South)
                    || (!is_gnd && rpi.pin.side == Side::North);
                if can_cluster {
                    let ys = rpi.pin.tip_point.y;
                    let bridge_y = if is_gnd {
                        snap_grid(ys + GRID_MM)
                    } else {
                        snap_grid(ys - GRID_MM)
                    };
                    let mut cluster = vec![i];
                    loop {
                        let mut added = false;
                        for j in 0..net_endpoints.len() {
                            if handled[j] || cluster.contains(&j) {
                                continue;
                            }
                            let rpj = net_endpoints[j];
                            if rpj.col_idx == rpi.col_idx
                                && rpj.pin.side == rpi.pin.side
                                && (rpj.pin.tip_point.y - ys).abs() < 0.1
                                && cluster.iter().any(|&k| {
                                    let xk = net_endpoints[k].pin.tip_point.x;
                                    let xj = rpj.pin.tip_point.x;
                                    let dist_ok = (xj - xk).abs() <= 30.48;
                                    let (bx0, bx1) = (xk.min(xj), xk.max(xj));
                                    let clear_of_bodies = !all_bodies.iter().any(|b| {
                                        bridge_y >= b.min_y - 0.2
                                            && bridge_y <= b.max_y + 0.2
                                            && bx1 >= b.min_x - 0.2
                                            && bx0 <= b.max_x + 0.2
                                    });
                                    dist_ok && clear_of_bodies
                                })
                            {
                                cluster.push(j);
                                added = true;
                            }
                        }
                        if !added {
                            break;
                        }
                    }
                    if cluster.len() >= 2 {
                        for &idx in &cluster {
                            handled[idx] = true;
                        }
                        let mut xs: Vec<f64> = cluster
                            .iter()
                            .map(|&idx| net_endpoints[idx].pin.tip_point.x)
                            .collect();
                        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        xs.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);
                        for &x in &xs {
                            add_ortho_seg(&mut segments, Point2::new(x, ys), Point2::new(x, bridge_y));
                        }
                        for pair in xs.windows(2) {
                            add_ortho_seg(
                                &mut segments,
                                Point2::new(pair[0], bridge_y),
                                Point2::new(pair[1], bridge_y),
                            );
                        }
                        let min_x = *xs.first().unwrap();
                        // Place single shared power/GND symbol cleanly on leftmost pin
                        let sym_x = min_x;
                        let sym_y = if is_gnd {
                            snap_grid(bridge_y + GRID_MM)
                        } else {
                            snap_grid(bridge_y - GRID_MM)
                        };
                        add_ortho_seg(
                            &mut segments,
                            Point2::new(sym_x, bridge_y),
                            Point2::new(sym_x, sym_y),
                        );
                        junctions.push(Point2::new(sym_x, bridge_y));
                        for &xm in &xs[1..xs.len().saturating_sub(1)] {
                            junctions.push(Point2::new(xm, bridge_y));
                        }
                        terminals.push(NetTerminalIr {
                            kind: if is_gnd { "gnd_symbol" } else { "power_symbol" }.to_string(),
                            pos: Point2::new(sym_x, sym_y),
                            dir: if is_gnd { Side::South } else { Side::North },
                            label: net.name.clone(),
                        });
                        continue;
                    }
                }
                handled[i] = true;
                let tip = rpi.pin.tip_point;
                if is_gnd {
                    let end = match rpi.pin.side {
                        Side::South => {
                            let e = Point2::new(tip.x, snap_grid(tip.y + GRID_MM));
                            add_ortho_seg(&mut segments, tip, e);
                            e
                        }
                        Side::West => {
                            let mid = Point2::new(snap_grid(tip.x - 2.0 * GRID_MM), tip.y);
                            let e = Point2::new(mid.x, snap_grid(mid.y + GRID_MM));
                            add_ortho_seg(&mut segments, tip, mid);
                            add_ortho_seg(&mut segments, mid, e);
                            e
                        }
                        Side::East => {
                            let mid = Point2::new(snap_grid(tip.x + 2.0 * GRID_MM), tip.y);
                            let e = Point2::new(mid.x, snap_grid(mid.y + GRID_MM));
                            add_ortho_seg(&mut segments, tip, mid);
                            add_ortho_seg(&mut segments, mid, e);
                            e
                        }
                        Side::North => {
                            // Elbow out left so downward GND triangle never overlaps component body!
                            let up = Point2::new(tip.x, snap_grid(tip.y - GRID_MM));
                            let side_pt = Point2::new(snap_grid(tip.x - 2.0 * GRID_MM), up.y);
                            let e = Point2::new(side_pt.x, snap_grid(side_pt.y + GRID_MM));
                            add_ortho_seg(&mut segments, tip, up);
                            add_ortho_seg(&mut segments, up, side_pt);
                            add_ortho_seg(&mut segments, side_pt, e);
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
                    let end = match rpi.pin.side {
                        Side::North => {
                            let e = Point2::new(tip.x, snap_grid(tip.y - GRID_MM));
                            add_ortho_seg(&mut segments, tip, e);
                            e
                        }
                        Side::West => {
                            let mid = Point2::new(snap_grid(tip.x - 2.0 * GRID_MM), tip.y);
                            let e = Point2::new(mid.x, snap_grid(mid.y - GRID_MM));
                            add_ortho_seg(&mut segments, tip, mid);
                            add_ortho_seg(&mut segments, mid, e);
                            e
                        }
                        Side::East => {
                            let mid = Point2::new(snap_grid(tip.x + 2.0 * GRID_MM), tip.y);
                            let e = Point2::new(mid.x, snap_grid(mid.y - GRID_MM));
                            add_ortho_seg(&mut segments, tip, mid);
                            add_ortho_seg(&mut segments, mid, e);
                            e
                        }
                        Side::South => {
                            // Elbow out right so upward power arrow never overlaps component body!
                            let dn = Point2::new(tip.x, snap_grid(tip.y + GRID_MM));
                            let side_pt = Point2::new(snap_grid(tip.x + 2.0 * GRID_MM), dn.y);
                            let e = Point2::new(side_pt.x, snap_grid(side_pt.y - GRID_MM));
                            add_ortho_seg(&mut segments, tip, dn);
                            add_ortho_seg(&mut segments, dn, side_pt);
                            add_ortho_seg(&mut segments, side_pt, e);
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
            local_routes.push(SchematicRouteIr {
                net_id: net.name.clone(),
                sheet: sheet_tag.clone(),
                style: style_name(style).to_string(),
                is_power: is_pwr,
                is_ground: is_gnd,
                segments,
                junctions,
                terminals,
                label_pos: None,
            });
            continue;
        }

        // --- MODE A2: Cross-Sheet Global Flags & Zone-Local Obstacle-Aware A* Routing ---
        let is_cross_sheet = cross_sheet_nets.contains(&net.name);
        if (style == NetRoutingStyle::ZoneLocal || is_cross_sheet) && !is_gnd && !is_pwr {
            let cols_in_net: BTreeSet<usize> = net_endpoints.iter().map(|rp| rp.col_idx).collect();
            // Intra-zone net on a single sheet -> route directly with obstacle-aware grid A*!
            if !is_cross_sheet && cols_in_net.len() == 1 && net_endpoints.len() >= 2 {
                let body_obs: Vec<Rect2> = local_entities
                    .iter()
                    .filter_map(|e| match &e.geometry {
                        EntityGeometryIr::Component { body_rect, .. } => Some(*body_rect),
                        _ => None,
                    })
                    .collect();
                let foreign_tips: BTreeSet<(i32, i32)> = placed_pins
                    .values()
                    .map(|rp| (
                        (rp.pin.tip_point.x / GRID_MM).round() as i32,
                        (rp.pin.tip_point.y / GRID_MM).round() as i32,
                    ))
                    .collect();
                let eps: Vec<(Point2, Side)> = net_endpoints
                    .iter()
                    .map(|rp| (rp.pin.tip_point, rp.pin.side))
                    .collect();
                let (segs, juncs) = crate::domains::elec::router::route_net_astar(
                    &eps,
                    &body_obs,
                    &foreign_tips,
                    &mut astar_used_cells,
                );
                local_routes.push(SchematicRouteIr {
                    net_id: net.name.clone(),
                    sheet: sheet_tag.clone(),
                    style: "zone_local".to_string(),
                    is_power: is_pwr,
                    is_ground: is_gnd,
                    segments: segs,
                    junctions: juncs,
                    terminals: Vec::new(),
                    label_pos: None,
                });
                continue;
            }
            // Cross-zone or Cross-sheet net -> crisp shaped flag terminals at each endpoint!
            let term_kind = if is_cross_sheet { "global_flag" } else { "label_flag" };
            let mut segments = Vec::new();
            let mut terminals = Vec::new();
            for rp in &net_endpoints {
                let tip = rp.pin.tip_point;
                let (end, term_dir) = match rp.pin.side {
                    Side::West => {
                        let e = Point2::new(snap_grid(tip.x - 2.0 * GRID_MM), tip.y);
                        add_ortho_seg(&mut segments, tip, e);
                        (e, Side::West)
                    }
                    Side::East => {
                        let e = Point2::new(snap_grid(tip.x + 2.0 * GRID_MM), tip.y);
                        add_ortho_seg(&mut segments, tip, e);
                        (e, Side::East)
                    }
                    Side::North => {
                        // Fan outward away from body center so adjacent North pins (e.g. VBAT/VCC) never short!
                        let go_left = tip.x < rp.body_center_x - 0.1;
                        let mid = Point2::new(tip.x, snap_grid(tip.y - GRID_MM));
                        let (e, d) = if go_left {
                            (Point2::new(snap_grid(tip.x - GRID_MM), mid.y), Side::West)
                        } else {
                            (Point2::new(snap_grid(tip.x + GRID_MM), mid.y), Side::East)
                        };
                        add_ortho_seg(&mut segments, tip, mid);
                        add_ortho_seg(&mut segments, mid, e);
                        (e, d)
                    }
                    Side::South => {
                        let go_left = tip.x < rp.body_center_x - 0.1;
                        let mid = Point2::new(tip.x, snap_grid(tip.y + GRID_MM));
                        let (e, d) = if go_left {
                            (Point2::new(snap_grid(tip.x - GRID_MM), mid.y), Side::West)
                        } else {
                            (Point2::new(snap_grid(tip.x + GRID_MM), mid.y), Side::East)
                        };
                        add_ortho_seg(&mut segments, tip, mid);
                        add_ortho_seg(&mut segments, mid, e);
                        (e, d)
                    }
                };
                terminals.push(NetTerminalIr {
                    kind: term_kind.to_string(),
                    pos: end,
                    dir: term_dir,
                    label: net.name.clone(),
                });
            }
            local_routes.push(SchematicRouteIr {
                net_id: net.name.clone(),
                sheet: sheet_tag.clone(),
                style: if is_cross_sheet { "global_flag" } else { "zone_local" }.to_string(),
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
        // Also automatically used for cross-row nets in 2D multi-row layouts so wires never cut through rows!
        let rows_in_net: BTreeSet<usize> = net_endpoints.iter().map(|rp| rp.row_idx).collect();
        let force_cross_row_stub = is_multi_row
            && !is_gnd
            && rows_in_net.len() > 1
            && matches!(style, NetRoutingStyle::Direct | NetRoutingStyle::Bus);
        if style == NetRoutingStyle::LabelStub || force_cross_row_stub {
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
            local_routes.push(SchematicRouteIr {
                net_id: net.name.clone(),
                sheet: sheet_tag.clone(),
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
                                // Clear both the component body AND any North pin + power symbol (`-7.62mm`)!
                                let clearance_grids = if rp.comp_has_north_pin { 4.0 } else { 3.0 };
                                let by = snap_grid(rp.body_min_y - clearance_grids * GRID_MM);
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

                local_routes.push(SchematicRouteIr {
                    net_id: net.name.clone(),
                    sheet: sheet_tag.clone(),
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

            // Subcase C2: Adjacent-column direct bridge within the SAME row!
            if cols.len() == 2 && can_direct_bridge_gap(net, &comp_col_map, &comp_row_map, &comp_pin_sides) {
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

                local_routes.push(SchematicRouteIr {
                    net_id: net.name.clone(),
                    sheet: sheet_tag.clone(),
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
        local_routes.push(SchematicRouteIr {
            net_id: net.name.clone(),
            sheet: sheet_tag.clone(),
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
    let route_count = local_routes.len();
    for i in 0..route_count {
        if local_routes[i].label_pos.is_none() {
            continue;
        }
        let mut chosen: Option<Point2> = None;
        let segs_i = local_routes[i].segments.clone();
        let mut candidates: Vec<Point2> = Vec::new();
        if let Some(lp) = local_routes[i].label_pos {
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
                local_routes[j]
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
            local_routes[i].label_pos = Some(pt);
        }
    }

    // Compute tight actual sheet bounds from all routed coordinates
    let mut max_x = max_comp_x;
    let mut max_y = max_comp_y + 15.24;
    for r in &local_routes {
        for s in &r.segments {
            max_x = max_x.max(s.p0.x.max(s.p1.x) + 15.24);
            max_y = max_y.max(s.p0.y.max(s.p1.y) + 15.24);
        }
        for t in &r.terminals {
            max_x = max_x.max(t.pos.x + 15.24);
            max_y = max_y.max(t.pos.y + 15.24);
        }
    }
    let this_bounds = Rect2::new(0.0, 0.0, snap_grid(max_x.max(228.6)), snap_grid(max_y.max(152.4)));
    if s_idx == 0 {
        sheet_bounds = this_bounds;
    }

    let sheet_str = schem
        .map(|s| s.sheet.clone())
        .unwrap_or_else(|| "A4 landscape".to_string());
    if let Some((sw, sh)) = sheet_dimensions_mm(&sheet_str) {
        if this_bounds.width() > sw - 12.0 || this_bounds.depth() > sh - 12.0 {
            diagnostics.push(
                Diagnostic::warning(
                    "W_SHEET_OVERFLOW",
                    "layout",
                    format!(
                        "Schematic '{}' bounds ({:.1}mm × {:.1}mm) exceed declared sheet '{}' ({:.0}mm × {:.0}mm)",
                        sheet_name_str,
                        this_bounds.width(),
                        this_bounds.depth(),
                        sheet_str,
                        sw,
                        sh
                    ),
                    schem.map(|s| s.span.clone()),
                )
                .with_suggestion(
                    "Split groups across multiple `schematic` sheets, `row [...]` bands, or increase `sheet:` (e.g., `A3 landscape`)",
                ),
            );
        }
    }

    views.push(ViewIr {
        id: sheet_name_str,
        kind: "SchematicView".to_string(),
        frame: default_frame.clone(),
        sheet: sheet_str,
        scale: "1:1".to_string(),
        show_dimensions: false,
        show_labels: true,
        title: schem.and_then(|s| s.title.clone()),
        rev: schem.and_then(|s| s.rev.clone()),
        author: schem.and_then(|s| s.author.clone()),
        sheet_index: if total_sheets > 1 { Some(s_idx + 1) } else { None },
        sheet_total: if total_sheets > 1 { Some(total_sheets) } else { None },
    });

    entities.extend(local_entities);
    schematic_groups.extend(local_groups);
    schematic_routes.extend(local_routes);
    let _ = col_bounds;
    } // end per-sheet loop

    let mut ir = CanonicalIr {
        ir_version: "0.6.0".to_string(),
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
            has_errors: false,
        },
        frames,
        entities,
        relations,
        derived_walls: Vec::new(),
        wall_solids: Vec::new(),
        schematic_groups,
        schematic_routes,
        mep_circuits: Vec::new(),
        mep_wires: Vec::new(),
        constraints: Vec::new(),
        views,
        evidence,
        diagnostics,
    };

    // 6. Optional declarative MNA DC simulation assertions (`dc_voltage` / `branch_current`)
    let has_sim_rules = project.verifications.iter().any(|r| {
        matches!(
            r,
            VerifyRule::DcVoltageRange { .. } | VerifyRule::BranchCurrentRange { .. }
        )
    });
    if has_sim_rules {
        let sim = crate::domains::elec::spice::simulate_dc_and_rc(&ir);
        ir.evidence.push(EvidenceRecord {
            check_id: "S1_dc_operating_point".to_string(),
            status: if sim.converged {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: sim.method.clone(),
            tolerance_mm: None,
            subjects: vec![ir.project.name.clone()],
            details: format!(
                "MNA DC operating point solved in {} iteration(s) ({} nets, {} branches, {} RC filter(s))",
                sim.iterations,
                sim.node_voltages_v.len(),
                sim.branches.len(),
                sim.rc_filters.len()
            ),
        });

        for rule in &project.verifications {
            match rule {
                VerifyRule::DcVoltageRange {
                    net,
                    min,
                    max,
                    span,
                } => {
                    if let (Ok(vmin), Ok(vmax)) = (min.to_volts(), max.to_volts()) {
                        let measured = sim.node_voltages_v.get(net).copied();
                        let ok = measured
                            .map(|v| v >= vmin - 1e-4 && v <= vmax + 1e-4)
                            .unwrap_or(false);
                        if !ok {
                            ir.diagnostics.push(
                                Diagnostic::error(
                                    "E_SIM_DC_VOLTAGE",
                                    "simulation",
                                    match measured {
                                        Some(v) => format!(
                                            "Simulated DC voltage on net '{net}' is {v:.3}V (required {vmin:.3}V..{vmax:.3}V)"
                                        ),
                                        None => format!("Net '{net}' not found in DC simulation"),
                                    },
                                    Some(span.clone()),
                                )
                                .with_entities(vec![net.clone()]),
                            );
                        }
                        ir.evidence.push(EvidenceRecord {
                            check_id: format!("S2_dc_voltage_{net}"),
                            status: if ok {
                                VerificationStatus::ExactlyChecked
                            } else {
                                VerificationStatus::Violated
                            },
                            method: sim.method.clone(),
                            tolerance_mm: None,
                            subjects: vec![net.clone()],
                            details: match measured {
                                Some(v) => format!("{net} = {v:.3}V (spec {vmin:.3}V..{vmax:.3}V)"),
                                None => format!("Net '{net}' missing"),
                            },
                        });
                    }
                }
                VerifyRule::BranchCurrentRange {
                    component,
                    min,
                    max,
                    span,
                } => {
                    if let (Ok(imin_a), Ok(imax_a)) = (min.to_amps(), max.to_amps()) {
                        let imin_ma = imin_a * 1000.0;
                        let imax_ma = imax_a * 1000.0;
                        let measured = sim
                            .branches
                            .iter()
                            .find(|b| b.component == *component)
                            .map(|b| b.current_ma.abs());
                        let ok = measured
                            .map(|i| i >= imin_ma - 1e-4 && i <= imax_ma + 1e-4)
                            .unwrap_or(false);
                        if !ok {
                            ir.diagnostics.push(
                                Diagnostic::error(
                                    "E_SIM_BRANCH_CURRENT",
                                    "simulation",
                                    match measured {
                                        Some(i) => format!(
                                            "Simulated DC current in '{component}' is {i:.3}mA (required {imin_ma:.3}mA..{imax_ma:.3}mA)"
                                        ),
                                        None => format!("Component '{component}' has no simulated branch current"),
                                    },
                                    Some(span.clone()),
                                )
                                .with_entities(vec![component.clone()]),
                            );
                        }
                        ir.evidence.push(EvidenceRecord {
                            check_id: format!("S3_branch_current_{component}"),
                            status: if ok {
                                VerificationStatus::ExactlyChecked
                            } else {
                                VerificationStatus::Violated
                            },
                            method: sim.method.clone(),
                            tolerance_mm: None,
                            subjects: vec![component.clone()],
                            details: match measured {
                                Some(i) => format!("{component} = {i:.3}mA (spec {imin_ma:.3}mA..{imax_ma:.3}mA)"),
                                None => format!("Component '{component}' missing"),
                            },
                        });
                    }
                }
                _ => {}
            }
        }
    }

    // 7. Senior-Engineer Design Rule Checks (E7-E10):
    //    Always run as advisory warnings on violations; promoted to hard Errors + EvidenceRecords
    //    when explicitly declared via `require ic_decoupling / i2c_pullups / inductive_flyback / bom_manufacturability`.
    run_senior_erc_checks(
        project,
        &comp_order,
        &comp_map,
        &all_nets,
        &pin_to_nets,
        &net_pins,
        &ground_nets,
        &net_driven_voltage,
        &mut ir,
    );

    ir.project.has_errors = ir.diagnostics.iter().any(|d| d.severity == Severity::Error);
    ir
}

#[allow(clippy::too_many_arguments)]
fn run_senior_erc_checks(
    project: &ProjectAst,
    comp_order: &[String],
    comp_map: &BTreeMap<String, &ComponentDecl>,
    all_nets: &[&NetDecl],
    pin_to_nets: &BTreeMap<(String, String), Vec<String>>,
    net_pins: &BTreeMap<String, Vec<(String, String, &PinDecl)>>,
    ground_nets: &BTreeSet<String>,
    net_driven_voltage: &BTreeMap<String, Vec<(String, f64)>>,
    ir: &mut CanonicalIr,
) {
    let req_decoupling = project
        .verifications
        .iter()
        .any(|r| matches!(r, VerifyRule::IcDecoupling { .. }));
    let req_i2c = project
        .verifications
        .iter()
        .any(|r| matches!(r, VerifyRule::I2cPullups { .. }));
    let req_flyback = project
        .verifications
        .iter()
        .any(|r| matches!(r, VerifyRule::InductiveFlyback { .. }));
    let req_bom = project
        .verifications
        .iter()
        .any(|r| matches!(r, VerifyRule::BomManufacturability { .. }));

    // Helper: get the 2 nets connected to a 2-pin passive component
    let two_pin_nets = |cid: &str, comp: &ComponentDecl| -> Option<(String, String)> {
        if comp.pins.len() != 2 {
            return None;
        }
        let n0 = pin_to_nets
            .get(&(cid.to_string(), comp.pins[0].name.clone()))
            .and_then(|v| v.first())
            .cloned()?;
        let n1 = pin_to_nets
            .get(&(cid.to_string(), comp.pins[1].name.clone()))
            .and_then(|v| v.first())
            .cloned()?;
        Some((n0, n1))
    };

    let is_power_like_net = |nname: &str| -> bool {
        if ground_nets.contains(nname) {
            return false;
        }
        net_driven_voltage
            .get(nname)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || is_power_rail_name(nname)
            || all_nets.iter().any(|n| n.name == nname && n.power_flag)
    };

    // ---- E7: IC Decoupling Capacitors ----
    {
        let mut checked_rails = 0usize;
        let mut missing: Vec<String> = Vec::new();
        for cid in comp_order {
            let comp = comp_map[cid];
            if !is_active_ic_comp(comp) {
                continue;
            }
            let mut supply_nets: BTreeSet<String> = BTreeSet::new();
            for pin in &comp.pins {
                if pin.role == PinRole::PowerIn {
                    if let Some(ns) = pin_to_nets.get(&(cid.clone(), pin.name.clone())) {
                        for n in ns {
                            if !ground_nets.contains(n) {
                                supply_nets.insert(n.clone());
                            }
                        }
                    }
                }
            }
            for snet in supply_nets {
                checked_rails += 1;
                let has_cap = comp_order.iter().any(|other_id| {
                    let other = comp_map[other_id];
                    if !is_capacitor_comp(other) {
                        return false;
                    }
                    let Some((na, nb)) = two_pin_nets(other_id, other) else {
                        return false;
                    };
                    let bridges = (na == snet && ground_nets.contains(&nb))
                        || (nb == snet && ground_nets.contains(&na));
                    if !bridges {
                        return false;
                    }
                    // Accept any capacitance >= 1nF (or unparseable custom cap string)
                    crate::domains::elec::spice::parse_si_component_value(&other.value, "C")
                        .map(|c| c >= 1e-9)
                        .unwrap_or(true)
                });
                if !has_cap {
                    let desc = format!("{cid} ({snet})");
                    missing.push(desc.clone());
                    let diag = if req_decoupling {
                        Diagnostic::error(
                            "E_ERC_MISSING_DECOUPLING",
                            "erc",
                            format!("Active IC '{cid}' supply rail '{snet}' has no decoupling capacitor to GND"),
                            Some(comp.span.clone()),
                        )
                    } else {
                        Diagnostic::warning(
                            "W_ERC_MISSING_DECOUPLING",
                            "erc",
                            format!("Active IC '{cid}' supply rail '{snet}' has no decoupling capacitor to GND"),
                            Some(comp.span.clone()),
                        )
                    };
                    ir.diagnostics.push(
                        diag.with_entities(vec![cid.clone(), snet.clone()])
                            .with_suggestion(format!(
                                "Add a 100nF ceramic decoupling capacitor between '{snet}' and GND near '{cid}'"
                            )),
                    );
                }
            }
        }
        if req_decoupling {
            ir.evidence.push(EvidenceRecord {
                check_id: "E7_ic_decoupling".to_string(),
                status: if missing.is_empty() {
                    VerificationStatus::ExactlyChecked
                } else {
                    VerificationStatus::Violated
                },
                method: "exact_ic_power_bypass_cap_check_v2.0".to_string(),
                tolerance_mm: None,
                subjects: comp_order
                    .iter()
                    .filter(|c| is_active_ic_comp(comp_map[*c]))
                    .cloned()
                    .collect(),
                details: if missing.is_empty() {
                    format!("All {checked_rails} active IC supply rail(s) have power-to-ground decoupling capacitor(s)")
                } else {
                    format!("Missing decoupling capacitor on: {}", missing.join(", "))
                },
            });
        }
    }

    // ---- E8: I2C Bus Pull-Up Resistors ----
    {
        let mut i2c_nets: Vec<&NetDecl> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        for net in all_nets {
            if !is_i2c_signal_net(&net.name) {
                continue;
            }
            i2c_nets.push(net);
            let has_pullup = comp_order.iter().any(|cid| {
                let comp = comp_map[cid];
                if !is_resistor_comp(comp) {
                    return false;
                }
                let Some((na, nb)) = two_pin_nets(cid, comp) else {
                    return false;
                };
                let other_net = if na == net.name {
                    &nb
                } else if nb == net.name {
                    &na
                } else {
                    return false;
                };
                if !is_power_like_net(other_net) {
                    return false;
                }
                crate::domains::elec::spice::parse_si_component_value(&comp.value, "R")
                    .map(|r| (470.0..=50_000.0).contains(&r))
                    .unwrap_or(true)
            });
            if !has_pullup {
                missing.push(net.name.clone());
                let diag = if req_i2c {
                    Diagnostic::error(
                        "E_ERC_MISSING_I2C_PULLUP",
                        "erc",
                        format!("I2C bus net '{}' has no pull-up resistor (470ohm..50kohm) to a logic supply rail", net.name),
                        Some(net.span.clone()),
                    )
                } else {
                    Diagnostic::warning(
                        "W_ERC_MISSING_I2C_PULLUP",
                        "erc",
                        format!("I2C bus net '{}' has no pull-up resistor (470ohm..50kohm) to a logic supply rail", net.name),
                        Some(net.span.clone()),
                    )
                };
                ir.diagnostics.push(
                    diag.with_entities(vec![net.name.clone()])
                        .with_suggestion(format!(
                            "Add a 2.2kohm..10kohm pull-up resistor from '{}' to your logic supply rail (e.g. V3V3)",
                            net.name
                        )),
                );
            }
        }
        if req_i2c {
            ir.evidence.push(EvidenceRecord {
                check_id: "E8_i2c_pullups".to_string(),
                status: if missing.is_empty() {
                    VerificationStatus::ExactlyChecked
                } else {
                    VerificationStatus::Violated
                },
                method: "exact_i2c_pullup_verification_v2.0".to_string(),
                tolerance_mm: None,
                subjects: i2c_nets.iter().map(|n| n.name.clone()).collect(),
                details: if missing.is_empty() {
                    format!("All {} I2C bus line(s) have valid pull-up resistor(s)", i2c_nets.len())
                } else {
                    format!("I2C bus line(s) missing pull-up resistor: {}", missing.join(", "))
                },
            });
        }
    }

    // ---- E9: Inductive Load Flyback Clamp Diodes ----
    {
        let mut checked_loads = 0usize;
        let mut unprotected: Vec<String> = Vec::new();
        for q_id in comp_order {
            let q = comp_map[q_id];
            if !is_transistor_switch_comp(q) {
                continue;
            }
            // Find collector/drain switched nets
            let sw_nets: Vec<String> = q
                .pins
                .iter()
                .filter(|p| {
                    let u = p.name.to_uppercase();
                    matches!(u.as_str(), "C" | "COL" | "COLLECTOR" | "D" | "DRAIN")
                })
                .filter_map(|p| pin_to_nets.get(&(q_id.clone(), p.name.clone())))
                .flatten()
                .cloned()
                .collect();

            for sw_net in sw_nets {
                for l_id in comp_order {
                    let load = comp_map[l_id];
                    if !is_inductive_load_comp(load) {
                        continue;
                    }
                    let Some((na, nb)) = two_pin_nets(l_id, load) else {
                        continue;
                    };
                    let other_net = if na == sw_net {
                        &nb
                    } else if nb == sw_net {
                        &na
                    } else {
                        continue;
                    };
                    if !is_power_like_net(other_net) && !ground_nets.contains(other_net) {
                        continue;
                    }
                    checked_loads += 1;
                    let has_clamp = comp_order.iter().any(|d_id| {
                        let d = comp_map[d_id];
                        if !is_diode_comp(d) {
                            return false;
                        }
                        let Some((da, db)) = two_pin_nets(d_id, d) else {
                            return false;
                        };
                        (da == sw_net && db == *other_net) || (db == sw_net && da == *other_net)
                    });
                    if !has_clamp {
                        let desc = format!("{l_id} (switched by {q_id} on {sw_net})");
                        unprotected.push(desc.clone());
                        let diag = if req_flyback {
                            Diagnostic::error(
                                "E_ERC_MISSING_FLYBACK",
                                "erc",
                                format!(
                                    "Inductive load '{l_id}' switched by '{q_id}' between '{sw_net}' and '{other_net}' lacks a flyback clamp diode"
                                ),
                                Some(load.span.clone()),
                            )
                        } else {
                            Diagnostic::warning(
                                "W_ERC_MISSING_FLYBACK",
                                "erc",
                                format!(
                                    "Inductive load '{l_id}' switched by '{q_id}' between '{sw_net}' and '{other_net}' lacks a flyback clamp diode"
                                ),
                                Some(load.span.clone()),
                            )
                        };
                        ir.diagnostics.push(
                            diag.with_entities(vec![l_id.clone(), q_id.clone(), sw_net.clone()])
                                .with_suggestion(format!(
                                    "Place a flyback diode (e.g. Device:D_Schottky SS24 or 1N4148) across '{l_id}' between '{sw_net}' and '{other_net}'"
                                )),
                        );
                    }
                }
            }
        }
        if req_flyback {
            ir.evidence.push(EvidenceRecord {
                check_id: "E9_inductive_flyback".to_string(),
                status: if unprotected.is_empty() {
                    VerificationStatus::ExactlyChecked
                } else {
                    VerificationStatus::Violated
                },
                method: "exact_switched_inductive_flyback_check_v2.0".to_string(),
                tolerance_mm: None,
                subjects: comp_order
                    .iter()
                    .filter(|c| is_inductive_load_comp(comp_map[*c]))
                    .cloned()
                    .collect(),
                details: if unprotected.is_empty() {
                    format!("All {checked_loads} transistor-switched inductive load(s) have flyback clamp diodes")
                } else {
                    format!("Unprotected inductive load(s): {}", unprotected.join(", "))
                },
            });
        }
    }

    // ---- E10: BOM Manufacturability (Cap Voltage Derating >= 1.25x & MNA Resistor Power Limits) ----
    {
        let sim = crate::domains::elec::spice::simulate_dc_and_rc(ir);
        let net_voltage_est = |nname: &str| -> Option<f64> {
            if ground_nets.contains(nname) {
                return Some(0.0);
            }
            if let Some(drvs) = net_driven_voltage.get(nname) {
                if let Some((_, v)) = drvs.first() {
                    return Some(*v);
                }
            }
            if let Some(&v) = sim.node_voltages_v.get(nname) {
                if v.abs() > 0.05 {
                    return Some(v);
                }
            }
            if let Some(ps) = net_pins.get(nname) {
                for (_, _, pdecl) in ps {
                    if pdecl.role == PinRole::PowerIn {
                        if let Some(spec) = pdecl.voltage {
                            return Some(match spec {
                                VoltageSpec::Exact { volts } => volts,
                                VoltageSpec::Range { min_v, max_v } => 0.5 * (min_v + max_v),
                            });
                        }
                    }
                }
            }
            None
        };

        let mut checked_caps = 0usize;
        let mut checked_res = 0usize;
        let mut violations: Vec<String> = Vec::new();

        for cid in comp_order {
            let comp = comp_map[cid];
            if is_capacitor_comp(comp) {
                if let Some(v_rated) = extract_cap_voltage_rating(&comp.value) {
                    if let Some((na, nb)) = two_pin_nets(cid, comp) {
                        if let (Some(va), Some(vb)) = (net_voltage_est(&na), net_voltage_est(&nb)) {
                            let v_op = (va - vb).abs();
                            if v_op > 0.5 {
                                checked_caps += 1;
                                let min_req = v_op * 1.25;
                                if v_rated + 1e-6 < min_req {
                                    violations.push(format!(
                                        "{cid}: rated {v_rated:.1}V < 1.25x operating {v_op:.1}V (need >= {min_req:.1}V)"
                                    ));
                                    let diag = if req_bom {
                                        Diagnostic::error(
                                            "E_ERC_CAP_VOLTAGE_MARGIN",
                                            "erc",
                                            format!(
                                                "Capacitor '{cid}' ({}) rated {v_rated:.1}V has insufficient derating on {v_op:.1}V DC rail (need >= {min_req:.1}V)",
                                                comp.value
                                            ),
                                            Some(comp.span.clone()),
                                        )
                                    } else {
                                        Diagnostic::warning(
                                            "W_ERC_CAP_VOLTAGE_MARGIN",
                                            "erc",
                                            format!(
                                                "Capacitor '{cid}' ({}) rated {v_rated:.1}V has insufficient derating on {v_op:.1}V DC rail (need >= {min_req:.1}V)",
                                                comp.value
                                            ),
                                            Some(comp.span.clone()),
                                        )
                                    };
                                    ir.diagnostics.push(
                                        diag.with_entities(vec![cid.clone()])
                                            .with_measurement(v_rated, min_req, "V")
                                            .with_suggestion(format!(
                                                "Upgrade '{cid}' voltage rating to at least {:.0}V (>= 1.25x rail voltage)",
                                                min_req.ceil()
                                            )),
                                    );
                                }
                            }
                        }
                    }
                }
            } else if is_resistor_comp(comp) {
                if let Some(br) = sim.branches.iter().find(|b| b.component == *cid) {
                    checked_res += 1;
                    let max_mw = resistor_package_max_mw(comp.footprint.as_deref());
                    if br.power_mw > max_mw + 1e-6 {
                        violations.push(format!(
                            "{cid}: dissipates {:.1}mW > package limit {:.1}mW",
                            br.power_mw, max_mw
                        ));
                        let diag = if req_bom {
                            Diagnostic::error(
                                "E_ERC_RESISTOR_POWER",
                                "erc",
                                format!(
                                    "Resistor '{cid}' ({}) dissipates {:.1}mW exceeding package rating {:.1}mW",
                                    comp.value, br.power_mw, max_mw
                                ),
                                Some(comp.span.clone()),
                            )
                        } else {
                            Diagnostic::warning(
                                "W_ERC_RESISTOR_POWER",
                                "erc",
                                format!(
                                    "Resistor '{cid}' ({}) dissipates {:.1}mW exceeding package rating {:.1}mW",
                                    comp.value, br.power_mw, max_mw
                                ),
                                Some(comp.span.clone()),
                            )
                        };
                        ir.diagnostics.push(
                            diag.with_entities(vec![cid.clone()])
                                .with_measurement(br.power_mw, max_mw, "mW")
                                .with_suggestion(format!(
                                    "Use a larger resistor package (e.g. 1206/2512) or higher resistance for '{cid}'"
                                )),
                        );
                    }
                }
            }
        }

        if req_bom {
            ir.evidence.push(EvidenceRecord {
                check_id: "E10_bom_manufacturability".to_string(),
                status: if violations.is_empty() {
                    VerificationStatus::ExactlyChecked
                } else {
                    VerificationStatus::Violated
                },
                method: "exact_bom_derating_and_mna_power_check_v2.0".to_string(),
                tolerance_mm: None,
                subjects: comp_order.to_vec(),
                details: if violations.is_empty() {
                    format!(
                        "BOM ratings OK ({checked_caps} rated cap(s) >= 1.25x rail V, {checked_res} resistor(s) within package power limit)"
                    )
                } else {
                    format!("BOM rating violation(s): {}", violations.join("; "))
                },
            });
        }
    }
}

fn is_capacitor_comp(comp: &ComponentDecl) -> bool {
    let cat = comp.category.to_lowercase();
    cat == "capacitor"
        || cat == "c"
        || cat == "device:c"
        || cat.starts_with("device:c_")
        || cat.ends_with(":c")
}

fn is_resistor_comp(comp: &ComponentDecl) -> bool {
    let cat = comp.category.to_lowercase();
    cat == "resistor"
        || cat == "r"
        || cat == "device:r"
        || cat.starts_with("device:r_")
        || cat.ends_with(":r")
}

fn is_diode_comp(comp: &ComponentDecl) -> bool {
    let cat = comp.category.to_lowercase();
    cat == "diode"
        || cat == "led"
        || cat == "d"
        || cat == "device:d"
        || cat.starts_with("device:d_")
        || cat.starts_with("device:led")
        || cat.starts_with("diode:")
}

fn is_transistor_switch_comp(comp: &ComponentDecl) -> bool {
    let cat = comp.category.to_lowercase();
    cat.contains("transistor")
        || cat.contains("mosfet")
        || cat.contains("bjt")
        || matches!(cat.as_str(), "npn" | "pnp" | "nmos" | "pmos" | "fet")
}

fn is_inductive_load_comp(comp: &ComponentDecl) -> bool {
    let cat = comp.category.to_lowercase();
    cat == "inductor"
        || cat == "relay"
        || cat == "solenoid"
        || cat == "buzzer"
        || cat == "motor"
        || cat.contains("buzzer")
        || cat.contains("relay")
        || cat.contains("solenoid")
        || cat.contains("speaker")
}

fn is_active_ic_comp(comp: &ComponentDecl) -> bool {
    if comp.pins.len() < 4 {
        return false;
    }
    let cat = comp.category.to_lowercase();
    if matches!(
        cat.as_str(),
        "connector" | "transformer" | "header" | "switch" | "jumper" | "terminal"
    ) || cat.starts_with("connector")
        || cat.starts_with("jumper")
        || cat.starts_with("transformer")
    {
        return false;
    }
    comp.pins.iter().any(|p| p.role == PinRole::PowerIn)
}

fn is_i2c_signal_net(name: &str) -> bool {
    let u = name.to_uppercase();
    if u.contains("_CS") || u.starts_with("AUX_") || u.contains("_EN") {
        return false;
    }
    u == "SDA"
        || u == "SCL"
        || u.starts_with("SDA")
        || u.starts_with("SCL")
        || u.ends_with("_SDA")
        || u.ends_with("_SCL")
        || u.contains("I2C_SDA")
        || u.contains("I2C_SCL")
}

fn extract_cap_voltage_rating(value: &str) -> Option<f64> {
    for tok in value.split(|c: char| c == '_' || c == '/' || c == ' ' || c == '-') {
        let up = tok.trim().to_uppercase();
        let Some(num_str) = up.strip_suffix("VDC").or_else(|| up.strip_suffix('V')) else {
            continue;
        };
        if !num_str.is_empty() && num_str.chars().all(|c| c.is_ascii_digit() || c == '.') {
            if let Ok(v) = num_str.parse::<f64>() {
                if v > 0.0 {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn resistor_package_max_mw(footprint: Option<&str>) -> f64 {
    let fp = footprint.unwrap_or("").to_uppercase();
    if fp.contains("0201") {
        50.0
    } else if fp.contains("0402") {
        62.5
    } else if fp.contains("0603") {
        100.0
    } else if fp.contains("0805") {
        125.0
    } else if fp.contains("1206") {
        250.0
    } else if fp.contains("1210") {
        500.0
    } else if fp.contains("2512") {
        1000.0
    } else {
        250.0
    }
}

fn sheet_dimensions_mm(sheet: &str) -> Option<(f64, f64)> {
    let low = sheet.to_lowercase();
    let portrait = low.contains("portrait");
    let (w, h) = if low.contains("a4") {
        (297.0, 210.0)
    } else if low.contains("a3") {
        (420.0, 297.0)
    } else if low.contains("a2") {
        (594.0, 420.0)
    } else if low.contains("a1") {
        (841.0, 594.0)
    } else {
        return None;
    };
    Some(if portrait { (h, w) } else { (w, h) })
}

/// True iff all pins of `net` live in two adjacent columns (c, c+1) in the SAME row and face each other.
fn can_direct_bridge_gap(
    net: &NetDecl,
    comp_col: &BTreeMap<String, usize>,
    comp_row: &BTreeMap<String, usize>,
    comp_pin_sides: &BTreeMap<String, Vec<(&PinDecl, Side)>>,
) -> bool {
    let rows: BTreeSet<usize> = net
        .connects
        .iter()
        .filter_map(|p| comp_row.get(&p.component).copied())
        .collect();
    if rows.len() != 1 {
        return false;
    }
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
        let mut pair = if p1_gnd && !p0_gnd {
            vec![(&comp.pins[0], Side::North), (&comp.pins[1], Side::South)]
        } else if p0_gnd && !p1_gnd {
            vec![(&comp.pins[0], Side::South), (&comp.pins[1], Side::North)]
        } else {
            vec![(&comp.pins[0], Side::West), (&comp.pins[1], Side::East)]
        };
        if comp.orient_deg != 0 {
            for (_, s) in &mut pair {
                *s = rotate_side_ccw(*s, comp.orient_deg);
            }
        }
        return pair;
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
    if comp.orient_deg != 0 {
        for (_, side) in &mut out {
            *side = rotate_side_ccw(*side, comp.orient_deg);
        }
    }
    out
}

fn rotate_side_ccw(side: Side, deg: i32) -> Side {
    let steps = (deg.rem_euclid(360) / 90) as usize;
    let mut cur = side;
    for _ in 0..steps {
        cur = match cur {
            Side::East => Side::North,
            Side::North => Side::West,
            Side::West => Side::South,
            Side::South => Side::East,
        };
    }
    cur
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
                pin_code: None,
                role: pin.role,
                voltage: pin.voltage,
                no_connect: pin.role == PinRole::Nc,
                side,
                body_point: body_pt,
                tip_point: tip_pt,
            });
        }
    }
    placed
}

fn is_power_rail_name(name: &str) -> bool {
    let u = name.to_uppercase();
    if u.contains("_ADC") || u.contains("_SENSE") || u.contains("_SNS") || u.contains("_SIG") || u.contains("_DIV") {
        return false;
    }
    u.starts_with('V')
}
