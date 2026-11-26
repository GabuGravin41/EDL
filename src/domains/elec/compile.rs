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

    // Expand high-density v3.0 circuit idioms (`decouple`, `pullup`, `pulldown`, `divider`, `series`, `rc_filter`)
    let expanded_idioms = crate::domains::elec::idioms::expand_circuit_idioms(&project.elec.circuits);

    // Collect explicit orientation/mirror overrides declared on `place ... left_of/...` relative constraints
    let mut rel_orient_overrides: BTreeMap<String, (Option<i32>, Option<crate::domains::elec::kicad_sym::SymMirror>)> =
        BTreeMap::new();
    for sc in &project.elec.schematics {
        for rp in &sc.relative_placements {
            if rp.orient_deg.is_some() || rp.mirror.is_some() {
                rel_orient_overrides.insert(rp.component.clone(), (rp.orient_deg, rp.mirror));
            }
        }
    }

    // Collect components (auto-hydrating native `.kicad_sym` library symbols!) & merged nets across circuit blocks + idioms
    let mut resolved_comps: BTreeMap<String, ComponentDecl> = BTreeMap::new();
    let mut loaded_syms: BTreeMap<String, crate::domains::elec::kicad_sym::KiCadLibSymbol> =
        BTreeMap::new();
    // Alias map: (comp_id, user_ref_name_or_number) -> Vec<internal_unique_pin_name>
    let mut pin_aliases: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    // Pin display metadata for library pins: (comp_id, internal_pin_name) -> (display_name, pin_code)
    let mut pin_meta: BTreeMap<(String, String), (String, String)> = BTreeMap::new();
    let mut comp_order: Vec<String> = Vec::new();
    let mut merged_nets_owned: Vec<NetDecl> = Vec::new();
    let mut net_idx_by_name: BTreeMap<String, usize> = BTreeMap::new();

    let mut raw_comp_stream: Vec<&ComponentDecl> = Vec::new();
    for circuit in &project.elec.circuits {
        for rc in &circuit.components {
            raw_comp_stream.push(rc);
        }
        for net in &circuit.nets {
            if let Some(&ni) = net_idx_by_name.get(&net.name) {
                if net.power_flag {
                    merged_nets_owned[ni].power_flag = true;
                }
                for pr in &net.connects {
                    if !merged_nets_owned[ni]
                        .connects
                        .iter()
                        .any(|e| e.component == pr.component && e.pin == pr.pin)
                    {
                        merged_nets_owned[ni].connects.push(pr.clone());
                    }
                }
                for ln in &net.linked_nets {
                    if !merged_nets_owned[ni].linked_nets.iter().any(|(n, _)| n == &ln.0) {
                        merged_nets_owned[ni].linked_nets.push(ln.clone());
                    }
                }
            } else {
                net_idx_by_name.insert(net.name.clone(), merged_nets_owned.len());
                merged_nets_owned.push(net.clone());
            }
        }
    }
    for rc in &expanded_idioms.extra_components {
        raw_comp_stream.push(rc);
    }
    for net in &expanded_idioms.extra_nets {
        if let Some(&ni) = net_idx_by_name.get(&net.name) {
            for pr in &net.connects {
                if !merged_nets_owned[ni]
                    .connects
                    .iter()
                    .any(|e| e.component == pr.component && e.pin == pr.pin)
                {
                    merged_nets_owned[ni].connects.push(pr.clone());
                }
            }
        } else {
            net_idx_by_name.insert(net.name.clone(), merged_nets_owned.len());
            merged_nets_owned.push(net.clone());
        }
    }

    // Resolve cross-hierarchy net links (`connect: [..., /PrimaryStage/V3V3]`) via union-find.
    // Preserves the target hierarchical net name & hierarchy while unifying all pins and power_flag.
    if merged_nets_owned.iter().any(|n| !n.linked_nets.is_empty()) {
        let n_nets = merged_nets_owned.len();
        let mut parent: Vec<usize> = (0..n_nets).collect();
        fn uf_find(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        for i in 0..n_nets {
            let links = merged_nets_owned[i].linked_nets.clone();
            let src_name = merged_nets_owned[i].name.clone();
            for (target_name, lspan) in links {
                if let Some(&ti) = net_idx_by_name.get(&target_name) {
                    let ri = uf_find(&mut parent, i);
                    let rt = uf_find(&mut parent, ti);
                    if ri != rt {
                        // Prefer the referenced target (typically the canonical `/Module/Net`) as root
                        parent[ri] = rt;
                    }
                } else {
                    diagnostics.push(
                        Diagnostic::error(
                            "E_REF_UNKNOWN_NET",
                            "semantics",
                            format!(
                                "Net '{}' references undeclared hierarchical net '{}'",
                                src_name, target_name
                            ),
                            Some(lspan),
                        )
                        .with_entities(vec![src_name.clone(), target_name.clone()])
                        .with_suggestion(format!(
                            "Declare `net {target_name} {{ ... }}` in a circuit block or fix the hierarchical net path"
                        )),
                    );
                }
            }
        }
        let mut absorbed = vec![false; n_nets];
        for i in 0..n_nets {
            let root = uf_find(&mut parent, i);
            if root != i {
                absorbed[i] = true;
                let extra_pf = merged_nets_owned[i].power_flag;
                let extra_connects = merged_nets_owned[i].connects.clone();
                if extra_pf {
                    merged_nets_owned[root].power_flag = true;
                }
                for pr in extra_connects {
                    if !merged_nets_owned[root]
                        .connects
                        .iter()
                        .any(|e| e.component == pr.component && e.pin == pr.pin)
                    {
                        merged_nets_owned[root].connects.push(pr);
                    }
                }
            }
        }
        let mut idx = 0usize;
        merged_nets_owned.retain(|_| {
            let keep = !absorbed[idx];
            idx += 1;
            keep
        });
    }

    for raw_comp in raw_comp_stream {
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
        if let Some(&(o_ov, m_ov)) = rel_orient_overrides.get(&comp.id) {
            if let Some(o) = o_ov {
                comp.orient_deg = o;
            }
            if let Some(m) = m_ov {
                comp.mirror = Some(m);
            }
        }

            // If category contains ':' OR was written as a quoted KiCad symbol reference, resolve it from `.kicad_sym` libraries!
            if comp.is_quoted_symbol || comp.category.contains(':') {
                match crate::domains::elec::kicad_sym::load_symbol(&comp.category, &extra_libs) {
                    Ok(sym) => {
                        let all_units = comp.unit.is_none() && sym.unit_count > 1;
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

                        // When a multi-unit IC is declared once in `circuit {}` (`unit: None`) and placed across
                        // multiple units in `schematic {}`, expose all units' pins (deduped by pin number).
                        let mut seen_pin_nums: BTreeSet<String> = BTreeSet::new();
                        let unit_pins: Vec<_> = sym
                            .pins
                            .iter()
                            .filter(|p| {
                                let unit_ok = all_units || p.unit == 0 || p.unit == target_unit;
                                let vis_ok = !p.hide || matches!(p.role, PinRole::PowerIn | PinRole::PowerOut | PinRole::Ground);
                                unit_ok && vis_ok && seen_pin_nums.insert(p.number.clone())
                            })
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
    let all_nets: Vec<&NetDecl> = merged_nets_owned.iter().collect();

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
            hierarchy: net.hierarchy.clone(),
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
                let repair_suggestions = if unconnected.is_empty() {
                    Vec::new()
                } else {
                    vec![
                        RepairSuggestion {
                            fix_id: 1,
                            description: format!(
                                "Connect unconnected pin(s) [{}] to their intended signal, power, or ground nets.",
                                unconnected.join(", ")
                            ),
                            auto_fixable: false,
                        },
                        RepairSuggestion {
                            fix_id: 2,
                            description: "For intentionally unused pins, add them to the component's `no_connect: [...]` list."
                                .to_string(),
                            auto_fixable: true,
                        },
                    ]
                };
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
                    repair_suggestions,
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
                let repair_suggestions: Vec<RepairSuggestion> = shorts
                    .iter()
                    .enumerate()
                    .map(|(i, n)| RepairSuggestion {
                        fix_id: i + 1,
                        description: format!(
                            "Separate power pins and ground pins currently shorted together on net '{n}'"
                        ),
                        auto_fixable: false,
                    })
                    .collect();
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
                    repair_suggestions,
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
                let repair_suggestions: Vec<RepairSuggestion> = conflicts
                    .iter()
                    .enumerate()
                    .map(|(i, n)| RepairSuggestion {
                        fix_id: i + 1,
                        description: format!(
                            "Isolate conflicting power_out drivers on net '{n}' using OR-ing diodes, load switches, or separate supply rails"
                        ),
                        auto_fixable: false,
                    })
                    .collect();
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
                    repair_suggestions,
                });
            }

            VerifyRule::VoltageCompatibility { .. } => {
                let mut mismatches = 0usize;
                let mut checked_rails = 0usize;
                let mut repair_suggestions: Vec<RepairSuggestion> = Vec::new();
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
                            repair_suggestions.push(RepairSuggestion {
                                fix_id: mismatches,
                                description: format!(
                                    "Connect '{target_pin}' to a regulated rail within {min_v:.2}V..{max_v:.2}V instead of '{}' ({v_supply:.2}V)",
                                    net.name
                                ),
                                auto_fixable: false,
                            });
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
                    repair_suggestions,
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
                        repair_suggestions: Vec::new(),
                    });
                } else {
                    evidence.push(EvidenceRecord {
                        check_id,
                        status: VerificationStatus::ExactlyChecked,
                        method: "exact_conductive_graph_bfs_v1.1".to_string(),
                        tolerance_mm: None,
                        subjects: vec![net_a.clone(), net_b.clone()],
                        details: format!("0 conductive paths between '{}' and '{}' (galvanic barrier intact)", net_a, net_b),
                        repair_suggestions: Vec::new(),
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
                let repair_suggestions: Vec<RepairSuggestion> = dangling
                    .iter()
                    .enumerate()
                    .map(|(i, n)| RepairSuggestion {
                        fix_id: i + 1,
                        description: format!(
                            "Connect a second pin to net '{n}', link it to a hierarchical net, or remove the dangling net"
                        ),
                        auto_fixable: false,
                    })
                    .collect();
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
                    repair_suggestions,
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
            for rp in &sc.relative_placements {
                comp_to_sheet.entry(rp.component.clone()).or_insert(s_idx);
            }
        }
    }
    // Idiom-generated passive satellites inherit their anchor IC's sheet automatically!
    for (sat_id, anchor_id) in &expanded_idioms.component_anchor {
        if !comp_to_sheet.contains_key(sat_id) {
            if let Some(&s_idx) = comp_to_sheet.get(anchor_id) {
                comp_to_sheet.insert(sat_id.clone(), s_idx);
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
    let mut schematic_dividers: Vec<SchematicDividerIr> = Vec::new();
    let mut schematic_routes: Vec<SchematicRouteIr> = Vec::new();
    let mut explicit_sheets: Vec<ExplicitSchematicSheetIr> = Vec::new();
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

        // --- EXPLICIT GEOMETRIC MODE (`routing { mode: explicit }`) ---
        // Preserves 100% lossless human/KiCad schematic geometry alongside EDL's semantic `circuit {}`!
        if let Some(sc) = schem {
            if sc.routing.as_ref().map(|r| r.explicit_mode).unwrap_or(false) {
                let mut raw_lib_symbols: BTreeMap<String, String> = BTreeMap::new();
                let mut exp_symbols: Vec<ExplicitSymbolInstIr> = Vec::new();
                let mut exp_powers: Vec<ExplicitPowerInstIr> = Vec::new();
                let mut exp_wires: Vec<Segment2> = Vec::new();
                let mut exp_buses: Vec<Segment2> = Vec::new();
                let mut exp_bus_entries: Vec<ExplicitBusEntryIr> = Vec::new();
                let mut exp_junctions: Vec<Point2> = Vec::new();
                let mut exp_ncs: Vec<Point2> = Vec::new();
                let mut exp_labels: Vec<ExplicitLabelIr> = Vec::new();
                let mut exp_text_boxes: Vec<ExplicitTextBoxIr> = Vec::new();
                let mut exp_graphics: Vec<ExplicitGraphicIr> = Vec::new();
                let mut exp_sheet_blocks: Vec<ExplicitSheetBlockIr> = Vec::new();

                let mut s_max_x = 297.0_f64;
                let mut s_max_y = 210.0_f64;

                for pl in &sc.placements {
                    let x = pl.x.to_mm().unwrap_or(0.0);
                    let y = pl.y.to_mm().unwrap_or(0.0);
                    s_max_x = s_max_x.max(x + 20.0);
                    s_max_y = s_max_y.max(y + 20.0);
                    let Some(comp) = comp_map.get(&pl.component) else {
                        continue;
                    };
                    let unit = pl.unit.or(comp.unit).unwrap_or(1);
                    let orient_deg = pl.orient_deg.unwrap_or(comp.orient_deg);
                    let mirror_opt = pl.mirror.or(comp.mirror);
                    let mirror_str = mirror_opt.map(|m| match m {
                        crate::domains::elec::kicad_sym::SymMirror::X => "x".to_string(),
                        crate::domains::elec::kicad_sym::SymMirror::Y => "y".to_string(),
                    });
                    let lib_id = comp.category.clone();
                    if !raw_lib_symbols.contains_key(&lib_id) {
                        if let Some(sym) = loaded_syms.get(&comp.id) {
                            if let Some(ref raw) = sym.raw_sexpr {
                                raw_lib_symbols.insert(lib_id.clone(), raw.clone());
                            }
                        } else if let Some(raw) =
                            crate::domains::elec::kicad_sym::load_raw_symbol_sexpr(&lib_id, &extra_libs)
                        {
                            raw_lib_symbols.insert(lib_id.clone(), raw);
                        }
                    }
                    let pin_numbers: Vec<String> = if let Some(sym) = loaded_syms.get(&comp.id) {
                        let mut seen = BTreeSet::new();
                        sym.pins
                            .iter()
                            .filter(|p| p.unit == 0 || p.unit == unit)
                            .filter(|p| seen.insert(p.number.clone()))
                            .map(|p| p.number.clone())
                            .collect()
                    } else {
                        (1..=comp.pins.len()).map(|n| n.to_string()).collect()
                    };
                    exp_symbols.push(ExplicitSymbolInstIr {
                        ref_id: comp.id.clone(),
                        lib_id: lib_id.clone(),
                        value: comp.value.clone(),
                        footprint: comp.footprint.clone(),
                        datasheet: comp.datasheet.clone(),
                        unit,
                        x,
                        y,
                        orient_deg,
                        mirror: mirror_str.clone(),
                        dnp: comp.dnp,
                        in_bom: comp.in_bom,
                        on_board: comp.on_board,
                        props: comp.props.clone(),
                        pin_numbers,
                        instances: pl.instances.clone(),
                        ref_at: pl.ref_at.map(|(rx, ry, rr)| [rx, ry, rr as f64]),
                        val_at: pl.val_at.map(|(vx, vy, vr)| [vx, vy, vr as f64]),
                        fields: pl
                            .fields
                            .iter()
                            .map(|f| ExplicitFieldStyleIr {
                                name: f.name.clone(),
                                value: f.value.clone(),
                                hide: f.hide,
                                x: f.x,
                                y: f.y,
                                orient_deg: f.orient_deg,
                                font_size: f.font_size,
                                justify: f.justify.clone(),
                            })
                            .collect(),
                    });

                    // Also emit EntityIr (once per logical component) for BOM/MNA/SVG compatibility
                    if !entities.iter().any(|e| e.id == comp.id) {
                        let mut props = BTreeMap::new();
                        props.insert("category".to_string(), json!(comp.category));
                        props.insert("value".to_string(), json!(comp.value));
                        if let Some(ref fp) = comp.footprint {
                            props.insert("footprint".to_string(), json!(fp));
                        }
                        if let Some(vr) = comp.voltage_rating_v {
                            props.insert("voltage_rating_v".to_string(), json!(vr));
                        }
                        if let Some(pr) = comp.power_rating_w {
                            props.insert("power_rating_w".to_string(), json!(pr));
                        }
                        if let Some(ref st) = sheet_tag {
                            props.insert("sheet".to_string(), json!(st));
                        }
                        let (body, placed_vec, hide_pin_names, symbol_prims) =
                            if let Some(sym) = loaded_syms.get(&comp.id) {
                                let lb = crate::domains::elec::kicad_sym::symbol_unit_body_bounds(
                                    sym, unit, orient_deg, mirror_opt,
                                );
                                let body = Rect2::new(x + lb.min_x, y + lb.min_y, x + lb.max_x, y + lb.max_y);
                                let gr = crate::domains::elec::kicad_sym::build_kicad_symbol_graphic(
                                    sym, unit, x, y, orient_deg, mirror_opt,
                                );
                                let pvec: Vec<PlacedPinIr> = sym
                                    .pins
                                    .iter()
                                    .filter(|p| (p.unit == 0 || p.unit == unit) && !p.hide)
                                    .enumerate()
                                    .map(|(pidx, lp)| {
                                        let tip = crate::domains::elec::kicad_sym::local_to_world(
                                            x, y, lp.x, lp.y, orient_deg, mirror_opt,
                                        );
                                        let rad = lp.angle.to_radians();
                                        let bpt = crate::domains::elec::kicad_sym::local_to_world(
                                            x,
                                            y,
                                            lp.x + lp.length * rad.cos(),
                                            lp.y + lp.length * rad.sin(),
                                            orient_deg,
                                            mirror_opt,
                                        );
                                        let side = crate::domains::elec::kicad_sym::pin_outward_side(
                                            lp.angle, orient_deg, mirror_opt,
                                        );
                                        PlacedPinIr {
                                            name: lp.name.clone(),
                                            number: lp.number.parse().unwrap_or(pidx + 1),
                                            pin_code: Some(lp.number.clone()),
                                            role: lp.role,
                                            voltage: None,
                                            no_connect: comp.no_connect.iter().any(|n| n == &lp.number || n == &lp.name),
                                            side,
                                            body_point: bpt,
                                            tip_point: tip,
                                        }
                                    })
                                    .collect();
                                (body, pvec, gr.hide_pin_names, Some(gr.primitives))
                            } else {
                                (Rect2::new(x - 5.0, y - 5.0, x + 5.0, y + 5.0), Vec::new(), false, None)
                            };
                        entities.push(EntityIr {
                            id: comp.id.clone(),
                            kind: "elec.component".to_string(),
                            frame: default_frame.clone(),
                            span: Some(comp.span.clone()),
                            provenance: comp.provenance.clone(),
                            props,
                            geometry: EntityGeometryIr::Component {
                                category: comp.category.clone(),
                                value: comp.value.clone(),
                                footprint: comp.footprint.clone(),
                                lib_id: Some(lib_id),
                                orient_deg,
                                mirror: mirror_str,
                                unit: Some(unit),
                                hide_pin_names,
                                symbol_prims,
                                symbol_origin: Some(Point2::new(x, y)),
                                body_rect: body,
                                pins: placed_vec,
                            },
                        });
                    }
                }

                for (pidx, pw) in sc.explicit_powers.iter().enumerate() {
                    let x = pw.x.to_mm().unwrap_or(0.0);
                    let y = pw.y.to_mm().unwrap_or(0.0);
                    if !raw_lib_symbols.contains_key(&pw.lib_id) {
                        if let Some(raw) =
                            crate::domains::elec::kicad_sym::load_raw_symbol_sexpr(&pw.lib_id, &extra_libs)
                        {
                            raw_lib_symbols.insert(pw.lib_id.clone(), raw);
                        }
                    }
                    let reference = pw
                        .reference
                        .clone()
                        .unwrap_or_else(|| format!("#PWR{:02}{:03}", s_idx + 1, pidx + 1));
                    exp_powers.push(ExplicitPowerInstIr {
                        lib_id: pw.lib_id.clone(),
                        value: pw.value.clone(),
                        reference,
                        x,
                        y,
                        orient_deg: pw.orient_deg,
                        mirror: pw.mirror.map(|m| match m {
                            crate::domains::elec::kicad_sym::SymMirror::X => "x".to_string(),
                            crate::domains::elec::kicad_sym::SymMirror::Y => "y".to_string(),
                        }),
                        val_at: pw.val_at.map(|(vx, vy, vr)| [vx, vy, vr as f64]),
                        val_hide: pw.val_hide,
                        val_justify: pw.val_justify.clone(),
                    });
                }

                for w in &sc.explicit_wires {
                    let pts: Vec<Point2> = w
                        .pts
                        .iter()
                        .map(|(qx, qy)| Point2::new(qx.to_mm().unwrap_or(0.0), qy.to_mm().unwrap_or(0.0)))
                        .collect();
                    for win in pts.windows(2) {
                        let seg = Segment2::new(win[0], win[1]);
                        if w.is_bus {
                            exp_buses.push(seg);
                        } else {
                            exp_wires.push(seg);
                        }
                    }
                }
                for be in &sc.explicit_bus_entries {
                    exp_bus_entries.push(ExplicitBusEntryIr {
                        x: be.x.to_mm().unwrap_or(0.0),
                        y: be.y.to_mm().unwrap_or(0.0),
                        dx: be.dx.to_mm().unwrap_or(2.54),
                        dy: be.dy.to_mm().unwrap_or(2.54),
                    });
                }
                for j in &sc.explicit_junctions {
                    exp_junctions.push(Point2::new(j.x.to_mm().unwrap_or(0.0), j.y.to_mm().unwrap_or(0.0)));
                }
                for nc in &sc.explicit_no_connects {
                    exp_ncs.push(Point2::new(nc.x.to_mm().unwrap_or(0.0), nc.y.to_mm().unwrap_or(0.0)));
                }
                for lb in &sc.explicit_labels {
                    exp_labels.push(ExplicitLabelIr {
                        kind: lb.kind.clone(),
                        text: lb.text.clone(),
                        shape: lb.shape.clone(),
                        x: lb.x.to_mm().unwrap_or(0.0),
                        y: lb.y.to_mm().unwrap_or(0.0),
                        orient_deg: lb.orient_deg,
                        justify: lb.justify.clone(),
                        font_size: lb.font_size,
                    });
                }
                for tb in &sc.explicit_text_boxes {
                    exp_text_boxes.push(ExplicitTextBoxIr {
                        text: tb.text.clone(),
                        x: tb.x.to_mm().unwrap_or(0.0),
                        y: tb.y.to_mm().unwrap_or(0.0),
                        w: tb.w.to_mm().unwrap_or(40.0),
                        h: tb.h.to_mm().unwrap_or(20.0),
                        orient_deg: tb.orient_deg,
                        font_size: tb.font_size,
                        justify: tb.justify.clone(),
                        stroke_type: tb.stroke_type.clone(),
                        stroke_width: tb.stroke_width,
                        stroke_color: tb.stroke_color.clone(),
                        fill_color: tb.fill_color.clone(),
                    });
                }
                for gr in &sc.explicit_graphics {
                    exp_graphics.push(ExplicitGraphicIr {
                        kind: gr.kind.clone(),
                        pts: gr
                            .pts
                            .iter()
                            .map(|(qx, qy)| Point2::new(qx.to_mm().unwrap_or(0.0), qy.to_mm().unwrap_or(0.0)))
                            .collect(),
                        stroke_type: gr.stroke_type.clone(),
                        stroke_width: gr.stroke_width,
                        stroke_color: gr.stroke_color.clone(),
                        fill_color: gr.fill_color.clone(),
                    });
                }
                for sb in &sc.explicit_sheets {
                    exp_sheet_blocks.push(ExplicitSheetBlockIr {
                        name: sb.name.clone(),
                        file: sb.file.clone(),
                        x: sb.x.to_mm().unwrap_or(0.0),
                        y: sb.y.to_mm().unwrap_or(0.0),
                        w: sb.w.to_mm().unwrap_or(40.0),
                        h: sb.h.to_mm().unwrap_or(25.0),
                        name_at: sb.name_at.map(|(nx, ny, nr)| [nx, ny, nr as f64]),
                        file_at: sb.file_at.map(|(fx, fy, fr)| [fx, fy, fr as f64]),
                        file_justify: sb.file_justify.clone(),
                        fill_color: sb.fill_color.clone(),
                        pins: sb
                            .pins
                            .iter()
                            .map(|sp| ExplicitSheetPinIr {
                                name: sp.name.clone(),
                                shape: sp.shape.clone(),
                                x: sp.x.to_mm().unwrap_or(0.0),
                                y: sp.y.to_mm().unwrap_or(0.0),
                                orient_deg: sp.orient_deg,
                                justify: sp.justify.clone(),
                            })
                            .collect(),
                    });
                }

                // Also emit a single SchematicRouteIr carrying explicit wires + junctions for EDL SVG preview
                if !exp_wires.is_empty() {
                    schematic_routes.push(SchematicRouteIr {
                        net_id: format!("__explicit_{sheet_name_str}"),
                        sheet: sheet_tag.clone(),
                        style: "direct".to_string(),
                        is_power: false,
                        is_ground: false,
                        segments: exp_wires.clone(),
                        junctions: exp_junctions.clone(),
                        terminals: Vec::new(),
                        label_pos: None,
                    });
                }

                explicit_sheets.push(ExplicitSchematicSheetIr {
                    sheet_id: sheet_name_str.clone(),
                    sheet_file: sc.sheet_file.clone(),
                    paper: sc.sheet.clone(),
                    title: sc.title.clone(),
                    author: sc.author.clone(),
                    rev: sc.rev.clone(),
                    raw_lib_symbols,
                    symbols: exp_symbols,
                    powers: exp_powers,
                    wires: exp_wires,
                    buses: exp_buses,
                    bus_entries: exp_bus_entries,
                    junctions: exp_junctions,
                    no_connects: exp_ncs,
                    labels: exp_labels,
                    text_boxes: exp_text_boxes,
                    graphics: exp_graphics,
                    sheet_blocks: exp_sheet_blocks,
                    bus_aliases: sc.bus_aliases.clone(),
                });

                if s_idx == 0 {
                    sheet_bounds = Rect2::new(0.0, 0.0, s_max_x, s_max_y);
                }
                views.push(ViewIr {
                    id: sheet_name_str,
                    kind: "SchematicView".to_string(),
                    frame: default_frame.clone(),
                    sheet: sc.sheet.clone(),
                    scale: "1:1".to_string(),
                    show_dimensions: false,
                    show_labels: true,
                    title: sc.title.clone(),
                    rev: sc.rev.clone(),
                    author: sc.author.clone(),
                    sheet_index: if total_sheets > 1 { Some(s_idx + 1) } else { None },
                    sheet_total: if total_sheets > 1 { Some(total_sheets) } else { None },
                });
                continue;
            }
        }
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

    // Explicit placement overrides (`place <Comp> at: (x, y)` or `place <Comp> center (x, y) [orient: ...]`)
    let mut explicit_pos: BTreeMap<
        String,
        (f64, f64, bool, Option<i32>, Option<crate::domains::elec::kicad_sym::SymMirror>),
    > = BTreeMap::new();
    if let Some(s) = schem {
        for pl in &s.placements {
            match (pl.x.to_mm(), pl.y.to_mm()) {
                (Ok(x), Ok(y)) => {
                    explicit_pos.insert(
                        pl.component.clone(),
                        (snap_grid(x), snap_grid(y), pl.is_center, pl.orient_deg, pl.mirror),
                    );
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
    let pinned_explicit: BTreeSet<String> = explicit_pos.keys().cloned().collect();
    let mut explicit_group_rects: BTreeMap<String, Rect2> = BTreeMap::new();
    let mut pinned_groups: BTreeSet<String> = BTreeSet::new();
    if let Some(s) = schem {
        for g in &s.groups {
            if let Some((x0, y0, x1, y1)) = g.rect {
                if let (Ok(rx0), Ok(ry0), Ok(rx1), Ok(ry1)) = (x0.to_mm(), y0.to_mm(), x1.to_mm(), y1.to_mm()) {
                    explicit_group_rects.insert(
                        g.name.clone(),
                        Rect2::new(rx0.min(rx1), ry0.min(ry1), rx0.max(rx1), ry0.max(ry1)),
                    );
                    pinned_groups.insert(g.name.clone());
                }
            }
        }
    }
    let mut local_dividers: Vec<SchematicDividerIr> = Vec::new();
    if let Some(s) = schem {
        for dv in &s.dividers {
            if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
                dv.from.0.to_mm(),
                dv.from.1.to_mm(),
                dv.to.0.to_mm(),
                dv.to.1.to_mm(),
            ) {
                local_dividers.push(SchematicDividerIr {
                    sheet: sheet_tag.clone(),
                    from: Point2::new(x0, y0),
                    to: Point2::new(x1, y1),
                    label: dv.label.clone(),
                    color: dv.color.clone(),
                });
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
    // Auto-attach idiom-generated satellites to their anchor IC's group if not already explicitly grouped!
    for (sat_id, anchor_id) in &expanded_idioms.component_anchor {
        if !assigned.contains(sat_id) && sheet_comps.contains(sat_id) {
            if let Some(grp) = ordered_groups
                .iter_mut()
                .find(|(_, _, _, ms)| ms.iter().any(|m| m == anchor_id))
            {
                grp.3.push(sat_id.clone());
                assigned.insert(sat_id.clone());
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
    let effective_group_gap = group_gap_mm.max(local_gutter_budget).max(27.94);

    let bus_top_y = snap_grid(25.4);
    let bus_tracks_h = if predicted_bus_nets == 0 || is_multi_row {
        0.0
    } else {
        (predicted_bus_nets as f64) * 5.08 + 7.62
    };
    let start_x = snap_grid(38.10 + ((predicted_bus_nets.min(4) + 1) as f64) * GRID_MM);
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

    // Optional 2D Sheet-Aware Packing (`layout { pack: skyline }` or `layout { pack: order }`):
    // `pack: order` preserves exact declaration order (Stage 1 -> Stage 2 -> Stage 3 -> ...);
    // `pack: skyline` sorts tallest-first for tightest 2D packing.
    // Groups with explicit `rect: (x0, y0, x1, y1)` are never moved by auto-packing!
    let pack_mode = schem
        .and_then(|s| s.layout.as_ref())
        .and_then(|l| l.pack.as_deref());
    let use_skyline = matches!(pack_mode, Some("skyline") | Some("order"))
        && pinned_groups.is_empty();
    let preserve_order = pack_mode == Some("order");
    let skyline_origins: BTreeMap<usize, (f64, f64)> = if use_skyline {
        let target_sheet_str = schem
            .map(|s| s.sheet.as_str())
            .unwrap_or("A2 landscape");
        let (target_sw, target_sh) = sheet_dimensions_mm(target_sheet_str).unwrap_or((594.0, 420.0));
        let mut group_dims: Vec<(usize, f64, f64)> = Vec::new();
        for (gidx, &(_, _, ref gname, ref members)) in ordered_groups.iter().enumerate() {
            let mut smap: BTreeMap<String, (f64, f64)> = BTreeMap::new();
            let mut col_w = 15.24_f64;
            let mut col_h = 0.0_f64;
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
                    compute_symbol_size(comp, &comp_pin_sides[cid])
                };
                col_w = col_w.max(bw);
                col_h += bh + member_gap_mm + PIN_LEN_MM;
                smap.insert(cid.clone(), (bw, bh));
            }
            let gdecl_opt = grp_decl_map.get(gname.as_str()).copied();
            let title_h = if gdecl_opt.and_then(|g| g.title.as_ref()).is_some() { 10.16 } else { 2.54 };
            let note_h = gdecl_opt
                .map(|g| if g.notes.is_empty() { 0.0 } else { (g.notes.len() as f64) * 6.35 + 5.08 })
                .unwrap_or(0.0);
            let (gw, gh) = if let Some(lt) = gdecl_opt.and_then(|g| g.layout_tree.as_ref()) {
                let sz_fn = |cid: &str| smap.get(cid).copied().unwrap_or((15.24, 10.16));
                let (tw, th) = crate::domains::elec::sheet_layout::measure_layout_node(lt, &sz_fn, 7.62, 7.62);
                (snap_grid(tw + 15.24), snap_grid(th + title_h + note_h + 15.24))
            } else {
                (snap_grid(col_w + 2.0 * PIN_LEN_MM + 15.24), snap_grid(col_h + title_h + note_h + 15.24))
            };
            group_dims.push((gidx, gw, gh));
        }
        crate::domains::elec::sheet_layout::pack_groups_skyline(
            &group_dims,
            target_sw,
            target_sh,
            snap_grid(20.32),
            snap_grid(17.78),
            12.70,
            12.70,
            preserve_order,
        )
    } else {
        BTreeMap::new()
    };

    for (col_idx, &(r_idx, _c_in_row, ref gname, ref members)) in ordered_groups.iter().enumerate() {
        let explicit_rect_opt = explicit_group_rects.get(gname).copied();
        if let Some(er) = explicit_rect_opt {
            cur_x = snap_grid(er.min_x + 12.70);
            row_start_y = snap_grid(er.min_y + 15.24);
        } else if let Some(&(sx, sy)) = skyline_origins.get(&col_idx) {
            cur_x = sx;
            row_start_y = sy;
        } else if r_idx != cur_row_idx {
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
            let explicit_entry = explicit_pos.get(cid).copied();
            let eff_orient = explicit_entry.and_then(|e| e.3).unwrap_or(comp.orient_deg);
            let eff_mirror = explicit_entry.and_then(|e| e.4).or(comp.mirror);

            let (bx0, by0) = if let Some((ex, ey, _is_c, _, _)) = explicit_entry {
                (ex, ey)
            } else if let Some(&(tx, ty)) = tree_pos.get(cid) {
                (tx, ty)
            } else {
                (snap_grid(col_x0 + (col_body_w - bw) * 0.5), snap_grid(cur_y))
            };
            let (body, placed, lib_id_opt, hide_pin_names, symbol_prims, symbol_origin, routing_keys): (
                Rect2,
                Vec<PlacedPinIr>,
                Option<String>,
                bool,
                Option<Vec<SymPrim>>,
                Option<Point2>,
                Vec<String>,
            ) = if let Some(sym) = loaded_syms.get(cid) {
                let u = comp.unit.unwrap_or(1);
                let lb = crate::domains::elec::kicad_sym::symbol_unit_body_bounds(
                    sym,
                    u,
                    eff_orient,
                    eff_mirror,
                );
                let (mut cx, mut cy) = if let Some((ex, ey, true, _, _)) = explicit_entry {
                    (snap_grid(ex), snap_grid(ey))
                } else {
                    (snap_grid(bx0 - lb.min_x), snap_grid(by0 - lb.min_y))
                };
                // If a KiCad library symbol's pins sit at half-grid offsets (e.g. Device:R/C at ±3.81mm = 1.5*2.54mm),
                // offset the symbol center by 1.27mm so EVERY pin tip lands cleanly on the 2.54mm grid with exact pin length!
                let first_unit_pin = sym.pins.iter().find(|p| (p.unit == 0 || p.unit == u) && !p.hide);
                if let Some(fp) = first_unit_pin {
                    let (rx, ry) = crate::domains::elec::kicad_sym::transform_local_yup(
                        fp.x,
                        fp.y,
                        eff_orient,
                        eff_mirror,
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
                    eff_orient,
                    eff_mirror,
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
                        eff_orient,
                        eff_mirror,
                    );
                    let rad = lp.angle.to_radians();
                    let body_pt = crate::domains::elec::kicad_sym::local_to_world(
                        cx,
                        cy,
                        lp.x + lp.length * rad.cos(),
                        lp.y + lp.length * rad.sin(),
                        eff_orient,
                        eff_mirror,
                    );
                    let side = crate::domains::elec::kicad_sym::pin_outward_side(
                        lp.angle,
                        eff_orient,
                        eff_mirror,
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
                    Some(Point2::new(cx, cy)),
                    rkeys,
                )
            } else {
                let body = Rect2::new(bx0, by0, snap_grid(bx0 + bw), snap_grid(by0 + bh));
                let placed = place_pins_on_body(comp, assigned_sides, body);
                let rkeys = placed.iter().map(|p| p.name.clone()).collect();
                (body, placed, None, false, None, None, rkeys)
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
            if let Some(vr) = comp.voltage_rating_v {
                props.insert("voltage_rating_v".to_string(), json!(vr));
            }
            if let Some(pr) = comp.power_rating_w {
                props.insert("power_rating_w".to_string(), json!(pr));
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
                    orient_deg: eff_orient,
                    mirror: eff_mirror.map(|m| match m {
                        crate::domains::elec::kicad_sym::SymMirror::X => "x".to_string(),
                        crate::domains::elec::kicad_sym::SymMirror::Y => "y".to_string(),
                    }),
                    unit: comp.unit,
                    hide_pin_names,
                    symbol_prims,
                    symbol_origin,
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
        if explicit_rect_opt.is_some() || g_min_x.is_finite() {
            let final_bbox = explicit_rect_opt
                .unwrap_or_else(|| Rect2::new(g_min_x, g_min_y, g_max_x, g_max_y));
            local_groups.push(SchematicGroupIr {
                name: gname.clone(),
                sheet: sheet_tag.clone(),
                title: gdecl_opt.and_then(|g| g.title.clone()),
                border: gdecl_opt.map(|g| g.border.clone()),
                color: gdecl_opt.and_then(|g| g.color.clone()),
                members: members.clone(),
                bbox: final_bbox,
                notes: g_notes,
                subzones: g_subzones,
            });
        }

        cur_x = snap_grid(col_x1 + effective_group_gap + 2.0 * PIN_LEN_MM);
    }

    // 4b. Attach standalone `note "..." [near: Target]` declarations to their target group!
    if let Some(sc) = schem {
        for sn in &sc.standalone_notes {
            let target_grp = sn.near.as_deref().and_then(|t| {
                local_groups
                    .iter_mut()
                    .find(|g| g.name == t || g.members.iter().any(|m| m == t))
            });
            if let Some(g) = target_grp {
                g.notes.push(sn.text.clone());
            } else if let Some(first_g) = local_groups.first_mut() {
                first_g.notes.push(sn.text.clone());
            }
        }
    }

    // 4c. Pin-Anchored Relative Placement Constraint Solver (v3.0)!
    // Solves idiom hints (`decouple`, `pullup`, `divider`, `series`, `rc_filter`) + explicit
    // `place ... left_of/right_of/above/below/offset/at_angle`, `align_y`/`align_x`, `array`!
    let mut sheet_rel_constraints = expanded_idioms.auto_relative_placements.clone();
    if let Some(sc) = schem {
        sheet_rel_constraints.extend(sc.relative_placements.clone());
    }
    let moved_satellites = crate::domains::elec::rel_layout::apply_relative_placements(
        &mut local_entities,
        &mut local_groups,
        &sheet_rel_constraints,
        &pinned_explicit,
        &pinned_groups,
        &mut diagnostics,
    );
    if !moved_satellites.is_empty() {
        // Refresh placed_pins and sheet bounds tightly from the post-solver entity coordinates!
        max_comp_x = 0.0;
        max_comp_y = 0.0;
        for ent in &local_entities {
            if let EntityGeometryIr::Component {
                body_rect, pins, ..
            } = &ent.geometry
            {
                max_comp_x = max_comp_x.max(body_rect.max_x + PIN_LEN_MM + 12.70);
                max_comp_y = max_comp_y.max(body_rect.max_y + PIN_LEN_MM);
                let has_north = pins.iter().any(|p| p.side == Side::North);
                for p in pins {
                    // Update every routing key pointing to this pin (matched by pin_code or name)
                    for ((cid, _rkey), rp) in placed_pins.iter_mut() {
                        if cid == &ent.id
                            && (rp.pin.pin_code == p.pin_code && rp.pin.name == p.name
                                && rp.pin.number == p.number)
                        {
                            rp.pin = p.clone();
                            rp.body_center_x = body_rect.center().x;
                            rp.body_min_y = body_rect.min_y;
                            rp.comp_has_north_pin = has_north;
                            rp.col_left_x = rp.col_left_x.min(snap_grid(body_rect.min_x - PIN_LEN_MM - 2.54));
                            rp.col_right_x = rp.col_right_x.max(snap_grid(body_rect.max_x + PIN_LEN_MM + 2.54));
                        }
                    }
                }
            }
        }
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
    let mut astar_occ = crate::domains::elec::router::GridOccupancy::default();
    let mut pending_astar_nets: Vec<(String, bool, bool, Vec<RoutablePin>)> = Vec::new();
    let all_sheet_pin_tips: BTreeSet<(i32, i32)> = placed_pins
        .values()
        .map(|rp| {
            (
                (rp.pin.tip_point.x / GRID_MM).round() as i32,
                (rp.pin.tip_point.y / GRID_MM).round() as i32,
            )
        })
        .collect();
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
                let gcell = |p: Point2| -> (i32, i32) {
                    ((p.x / GRID_MM).round() as i32, (p.y / GRID_MM).round() as i32)
                };
                let can_cluster = (is_gnd && rpi.pin.side == Side::South)
                    || (!is_gnd && rpi.pin.side == Side::North);
                if can_cluster {
                    let ys = rpi.pin.tip_point.y;
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
                                && (rpj.pin.tip_point.y - ys).abs() <= 20.32
                                && cluster.iter().any(|&k| {
                                    let xk = net_endpoints[k].pin.tip_point.x;
                                    let xj = rpj.pin.tip_point.x;
                                    (xj - xk).abs() <= 90.0
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
                        let cluster_tips: BTreeSet<(i32, i32)> = cluster
                            .iter()
                            .map(|&idx| gcell(net_endpoints[idx].pin.tip_point))
                            .collect();
                        let ref_y = if is_gnd {
                            cluster
                                .iter()
                                .map(|&idx| net_endpoints[idx].pin.tip_point.y)
                                .fold(f64::NEG_INFINITY, f64::max)
                        } else {
                            cluster
                                .iter()
                                .map(|&idx| net_endpoints[idx].pin.tip_point.y)
                                .fold(f64::INFINITY, f64::min)
                        };
                        let mut pts_sorted: Vec<Point2> = cluster
                            .iter()
                            .map(|&idx| net_endpoints[idx].pin.tip_point)
                            .collect();
                        pts_sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                        let min_x = pts_sorted.first().unwrap().x;
                        let max_x = pts_sorted.last().unwrap().x;
                        let seg_clear = |p0: Point2, p1: Point2, occ: &crate::domains::elec::router::GridOccupancy| -> bool {
                            let a = gcell(p0);
                            let b = gcell(p1);
                            if a.1 == b.1 {
                                let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                                for x in x0..=x1 {
                                    let c = (x, a.1);
                                    if !cluster_tips.contains(&c)
                                        && (all_sheet_pin_tips.contains(&c) || occ.used_cells.contains(&c))
                                    {
                                        return false;
                                    }
                                }
                            } else if a.0 == b.0 {
                                let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                                for y in y0..=y1 {
                                    let c = (a.0, y);
                                    if !cluster_tips.contains(&c)
                                        && (all_sheet_pin_tips.contains(&c) || occ.used_cells.contains(&c))
                                    {
                                        return false;
                                    }
                                }
                            }
                            let (bx0, bx1) = (p0.x.min(p1.x), p0.x.max(p1.x));
                            let (by0, by1) = (p0.y.min(p1.y), p0.y.max(p1.y));
                            !all_bodies.iter().any(|b| {
                                by1 >= b.min_y - 0.1
                                    && by0 <= b.max_y + 0.1
                                    && bx1 >= b.min_x - 0.1
                                    && bx0 <= b.max_x + 0.1
                            })
                        };
                        let mut chosen_bridge: Option<f64> = None;
                        for step in [2.0_f64, 1.0, 3.0, 4.0] {
                            let by = if is_gnd {
                                snap_grid(ref_y + step * GRID_MM)
                            } else {
                                snap_grid(ref_y - step * GRID_MM)
                            };
                            let horiz_ok = seg_clear(Point2::new(min_x, by), Point2::new(max_x, by), &astar_occ);
                            let drops_ok = pts_sorted
                                .iter()
                                .all(|&pt| seg_clear(pt, Point2::new(pt.x, by), &astar_occ));
                            let sym_x = pts_sorted[pts_sorted.len() / 2].x;
                            let sym_y = if is_gnd {
                                snap_grid(by + GRID_MM)
                            } else {
                                snap_grid(by - GRID_MM)
                            };
                            let sym_ok = seg_clear(Point2::new(sym_x, by), Point2::new(sym_x, sym_y), &astar_occ);
                            if horiz_ok && drops_ok && sym_ok {
                                chosen_bridge = Some(by);
                                break;
                            }
                        }
                        if let Some(bridge_y) = chosen_bridge {
                            for &idx in &cluster {
                                handled[idx] = true;
                            }
                            let mut xs: Vec<f64> = Vec::new();
                            for pt in &pts_sorted {
                                let d0 = *pt;
                                let d1 = Point2::new(pt.x, bridge_y);
                                add_ortho_seg(&mut segments, d0, d1);
                                astar_occ.record_world_segment(Segment2::new(d0, d1));
                                if xs.last().map(|lx| (pt.x - *lx).abs() > EPS_MM).unwrap_or(true) {
                                    xs.push(pt.x);
                                }
                            }
                            for pair in xs.windows(2) {
                                let h0 = Point2::new(pair[0], bridge_y);
                                let h1 = Point2::new(pair[1], bridge_y);
                                add_ortho_seg(&mut segments, h0, h1);
                                astar_occ.record_world_segment(Segment2::new(h0, h1));
                            }
                            // Place single shared power/GND symbol cleanly on median pin of the trunk
                            let sym_x = xs[xs.len() / 2];
                            let sym_y = if is_gnd {
                                snap_grid(bridge_y + GRID_MM)
                            } else {
                                snap_grid(bridge_y - GRID_MM)
                            };
                            let s0 = Point2::new(sym_x, bridge_y);
                            let s1 = Point2::new(sym_x, sym_y);
                            add_ortho_seg(&mut segments, s0, s1);
                            astar_occ.record_world_segment(Segment2::new(s0, s1));
                            astar_occ.record_terminal_point(s1);
                            for &xj in &xs {
                                // Every interior T-junction plus the shared symbol tee gets a junction dot
                                let is_edge = (xj - *xs.first().unwrap()).abs() <= EPS_MM
                                    || (xj - *xs.last().unwrap()).abs() <= EPS_MM;
                                if !is_edge || (xj - sym_x).abs() <= EPS_MM {
                                    junctions.push(Point2::new(xj, bridge_y));
                                }
                            }
                            terminals.push(NetTerminalIr {
                                kind: if is_gnd { "gnd_symbol" } else { "power_symbol" }.to_string(),
                                pos: s1,
                                dir: if is_gnd { Side::South } else { Side::North },
                                label: net.name.clone(),
                            });
                            continue;
                        }
                    }
                }
                handled[i] = true;
                let tip = rpi.pin.tip_point;
                let tip_g = gcell(tip);
                // Helper: verify candidate polyline AND the power/GND symbol's own vertical footprint
                // never touch any foreign pin tip or existing stub (prevents orient:180 connector shorts!).
                let path_clear = |pts: &[Point2], occ: &crate::domains::elec::router::GridOccupancy| -> bool {
                    for win in pts.windows(2) {
                        let a = gcell(win[0]);
                        let b = gcell(win[1]);
                        if a.1 == b.1 {
                            let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                            for x in x0..=x1 {
                                let c = (x, a.1);
                                if c != tip_g && (all_sheet_pin_tips.contains(&c) || occ.used_cells.contains(&c)) {
                                    return false;
                                }
                            }
                        } else if a.0 == b.0 {
                            let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                            for y in y0..=y1 {
                                let c = (a.0, y);
                                if c != tip_g && (all_sheet_pin_tips.contains(&c) || occ.used_cells.contains(&c)) {
                                    return false;
                                }
                            }
                        }
                    }
                    if let Some(&end_pt) = pts.last() {
                        let eg = gcell(end_pt);
                        let sym_body = if is_gnd { (eg.0, eg.1 + 1) } else { (eg.0, eg.1 - 1) };
                        if sym_body != tip_g
                            && (all_sheet_pin_tips.contains(&sym_body) || occ.used_cells.contains(&sym_body))
                        {
                            return false;
                        }
                        // Don't terminate an elbow directly into an adjacent pin's horizontal exit lane!
                        let same_row_pin_near = (-3..=3)
                            .any(|dx| (eg.0 + dx, eg.1) != tip_g && all_sheet_pin_tips.contains(&(eg.0 + dx, eg.1)));
                        if eg.1 != tip_g.1 && same_row_pin_near {
                            return false;
                        }
                    }
                    true
                };
                let pick_horiz_stub = |sign_x: f64, sign_y: f64| -> Vec<Point2> {
                    // First try classic L-elbow (`tip -> mid -> mid + sign_y`)
                    for step in 2..=6 {
                        let mid = Point2::new(snap_grid(tip.x + sign_x * (step as f64) * GRID_MM), tip.y);
                        let e = Point2::new(mid.x, snap_grid(mid.y + sign_y * GRID_MM));
                        let cand = vec![tip, mid, e];
                        if path_clear(&cand, &astar_occ) {
                            return cand;
                        }
                    }
                    // Fallback: staggered pure-horizontal stub (`power:GND` / `power:+24V` already draw
                    // their own vertical stem, so this avoids crossing an adjacent pin's lane!).
                    for step in [2, 4, 3, 5, 6, 1, 7] {
                        let e = Point2::new(snap_grid(tip.x + sign_x * (step as f64) * GRID_MM), tip.y);
                        let cand = vec![tip, e];
                        if path_clear(&cand, &astar_occ) {
                            return cand;
                        }
                    }
                    vec![
                        tip,
                        Point2::new(snap_grid(tip.x + sign_x * 2.0 * GRID_MM), tip.y),
                    ]
                };
                let chosen_pts: Vec<Point2> = if is_gnd {
                    match rpi.pin.side {
                        Side::South => {
                            let mut res = vec![tip, Point2::new(tip.x, snap_grid(tip.y + GRID_MM))];
                            for step in 1..=5 {
                                let cand = vec![tip, Point2::new(tip.x, snap_grid(tip.y + (step as f64) * GRID_MM))];
                                if path_clear(&cand, &astar_occ) {
                                    res = cand;
                                    break;
                                }
                            }
                            res
                        }
                        Side::West => pick_horiz_stub(-1.0, 1.0),
                        Side::East => pick_horiz_stub(1.0, 1.0),
                        Side::North => {
                            let mut res = vec![
                                tip,
                                Point2::new(tip.x, snap_grid(tip.y - GRID_MM)),
                                Point2::new(snap_grid(tip.x - 2.0 * GRID_MM), snap_grid(tip.y - GRID_MM)),
                                Point2::new(snap_grid(tip.x - 2.0 * GRID_MM), snap_grid(tip.y)),
                            ];
                            for step in 2..=6 {
                                let up = Point2::new(tip.x, snap_grid(tip.y - GRID_MM));
                                let side_pt = Point2::new(snap_grid(tip.x - (step as f64) * GRID_MM), up.y);
                                let e = Point2::new(side_pt.x, snap_grid(side_pt.y + GRID_MM));
                                let cand = vec![tip, up, side_pt, e];
                                if path_clear(&cand, &astar_occ) {
                                    res = cand;
                                    break;
                                }
                            }
                            res
                        }
                    }
                } else {
                    match rpi.pin.side {
                        Side::North => {
                            let mut res = vec![tip, Point2::new(tip.x, snap_grid(tip.y - GRID_MM))];
                            for step in 1..=5 {
                                let cand = vec![tip, Point2::new(tip.x, snap_grid(tip.y - (step as f64) * GRID_MM))];
                                if path_clear(&cand, &astar_occ) {
                                    res = cand;
                                    break;
                                }
                            }
                            res
                        }
                        Side::West => pick_horiz_stub(-1.0, -1.0),
                        Side::East => pick_horiz_stub(1.0, -1.0),
                        Side::South => {
                            let mut res = vec![
                                tip,
                                Point2::new(tip.x, snap_grid(tip.y + GRID_MM)),
                                Point2::new(snap_grid(tip.x + 2.0 * GRID_MM), snap_grid(tip.y + GRID_MM)),
                                Point2::new(snap_grid(tip.x + 2.0 * GRID_MM), snap_grid(tip.y)),
                            ];
                            for step in 2..=6 {
                                let dn = Point2::new(tip.x, snap_grid(tip.y + GRID_MM));
                                let side_pt = Point2::new(snap_grid(tip.x + (step as f64) * GRID_MM), dn.y);
                                let e = Point2::new(side_pt.x, snap_grid(side_pt.y - GRID_MM));
                                let cand = vec![tip, dn, side_pt, e];
                                if path_clear(&cand, &astar_occ) {
                                    res = cand;
                                    break;
                                }
                            }
                            res
                        }
                    }
                };
                for win in chosen_pts.windows(2) {
                    add_ortho_seg(&mut segments, win[0], win[1]);
                    astar_occ.record_world_segment(Segment2::new(win[0], win[1]));
                }
                let end = *chosen_pts.last().unwrap();
                astar_occ.record_terminal_point(end);
                // Also reserve the symbol's own vertical stem cell so a neighboring opposite-polarity
                // stub cannot cross it!
                let sym_stem_pt = Point2::new(
                    end.x,
                    if is_gnd { snap_grid(end.y + GRID_MM) } else { snap_grid(end.y - GRID_MM) },
                );
                astar_occ.record_terminal_point(sym_stem_pt);
                terminals.push(NetTerminalIr {
                    kind: if is_gnd { "gnd_symbol" } else { "power_symbol" }.to_string(),
                    pos: end,
                    dir: if is_gnd { Side::South } else { Side::North },
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
                junctions,
                terminals,
                label_pos: None,
            });
            continue;
        }

        // --- MODE A2: Cross-Sheet Global Flags & Zone-Local Obstacle-Aware A* Routing ---
        let is_cross_sheet = cross_sheet_nets.contains(&net.name);
        let touches_rel_sat = !moved_satellites.is_empty()
            && net
                .connects
                .iter()
                .any(|p| moved_satellites.contains(&p.component));
        let touches_explicit = !pinned_explicit.is_empty()
            && net
                .connects
                .iter()
                .any(|p| pinned_explicit.contains(&p.component));
        let cols_in_net_pre: BTreeSet<usize> = net_endpoints.iter().map(|rp| rp.col_idx).collect();
        let use_local_astar = !is_cross_sheet
            && !is_gnd
            && !is_pwr
            && cols_in_net_pre.len() == 1
            && net_endpoints.len() >= 2
            && (style == NetRoutingStyle::ZoneLocal
                || ((touches_rel_sat || touches_explicit) && style == NetRoutingStyle::Direct));

        if use_local_astar {
            // Defer intra-zone A* routing to Pass 2 so all power/GND symbols and label stubs
            // reserve their grid edges & terminal cells FIRST (zero accidental shorts!).
            pending_astar_nets.push((
                net.name.clone(),
                is_pwr,
                is_gnd,
                net_endpoints.into_iter().cloned().collect(),
            ));
            continue;
        }
        if (style == NetRoutingStyle::ZoneLocal || is_cross_sheet) && !is_gnd && !is_pwr {
            // Cross-zone or Cross-sheet net -> crisp KiCad red-outlined `global_flag` box!
            let term_kind = if is_cross_sheet || cols_in_net_pre.len() > 1 {
                "global_flag"
            } else {
                "label_flag"
            };
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

    // 5a-Pass2. Route deferred intra-zone A* nets AFTER all power/GND symbols & flag stubs have reserved occupancy!
    if !pending_astar_nets.is_empty() {
        for r in &local_routes {
            for &seg in &r.segments {
                astar_occ.record_world_segment(seg);
            }
            for t in &r.terminals {
                astar_occ.record_terminal_point(t.pos);
            }
        }
        let body_obs: Vec<Rect2> = local_entities
            .iter()
            .filter_map(|e| match &e.geometry {
                EntityGeometryIr::Component { body_rect, .. } => Some(*body_rect),
                _ => None,
            })
            .collect();
        let foreign_tips: BTreeSet<(i32, i32)> = placed_pins
            .values()
            .map(|rp| {
                (
                    (rp.pin.tip_point.x / GRID_MM).round() as i32,
                    (rp.pin.tip_point.y / GRID_MM).round() as i32,
                )
            })
            .collect();

        // Sort shortest-span nets first so direct adjacent/pin-aligned connections always get 0-bend wires!
        pending_astar_nets.sort_by(|a, b| {
            let span_of = |rps: &[RoutablePin]| -> i64 {
                let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
                for rp in rps {
                    x0 = x0.min(rp.pin.tip_point.x);
                    x1 = x1.max(rp.pin.tip_point.x);
                    y0 = y0.min(rp.pin.tip_point.y);
                    y1 = y1.max(rp.pin.tip_point.y);
                }
                (((x1 - x0) + (y1 - y0)) * 100.0) as i64
            };
            span_of(&a.3).cmp(&span_of(&b.3)).then_with(|| a.0.cmp(&b.0))
        });

        for (net_name, is_pwr, is_gnd, rps) in pending_astar_nets {
            let eps: Vec<(Point2, Side)> = rps
                .iter()
                .map(|rp| (rp.pin.tip_point, rp.pin.side))
                .collect();
            if let Some((segs, juncs)) = crate::domains::elec::router::try_route_net_astar_strict(
                &eps,
                &body_obs,
                &foreign_tips,
                &mut astar_occ,
            ) {
                local_routes.push(SchematicRouteIr {
                    net_id: net_name,
                    sheet: sheet_tag.clone(),
                    style: "zone_local".to_string(),
                    is_power: is_pwr,
                    is_ground: is_gnd,
                    segments: segs,
                    junctions: juncs,
                    terminals: Vec::new(),
                    label_pos: None,
                });
            } else {
                // Corridor completely congested: emit clean local `label_flag` stubs — ALWAYS checking
                // `astar_occ` so a fallback stub NEVER terminates on top of a passing A* wire!
                let mut segments = Vec::new();
                let mut terminals = Vec::new();
                let to_g = |p: Point2| -> (i32, i32) {
                    ((p.x / GRID_MM).round() as i32, (p.y / GRID_MM).round() as i32)
                };
                for rp in &rps {
                    let tip = rp.pin.tip_point;
                    let (end, term_dir) = match rp.pin.side {
                        Side::West => {
                            let e = Point2::new(snap_grid(tip.x - GRID_MM), tip.y);
                            let eg = to_g(e);
                            if !astar_occ.used_cells.contains(&eg) && !foreign_tips.contains(&eg) {
                                add_ortho_seg(&mut segments, tip, e);
                                (e, Side::West)
                            } else {
                                // Direct zero-length pin label: 100% valid in KiCad 9, mathematically short-free!
                                (tip, Side::West)
                            }
                        }
                        Side::East => {
                            let e = Point2::new(snap_grid(tip.x + GRID_MM), tip.y);
                            let eg = to_g(e);
                            if !astar_occ.used_cells.contains(&eg) && !foreign_tips.contains(&eg) {
                                add_ortho_seg(&mut segments, tip, e);
                                (e, Side::East)
                            } else {
                                (tip, Side::East)
                            }
                        }
                        Side::North => {
                            let go_left = tip.x < rp.body_center_x - 0.1;
                            let mid = Point2::new(tip.x, snap_grid(tip.y - GRID_MM));
                            let (e, d) = if go_left {
                                (Point2::new(snap_grid(tip.x - GRID_MM), mid.y), Side::West)
                            } else {
                                (Point2::new(snap_grid(tip.x + GRID_MM), mid.y), Side::East)
                            };
                            let mg = to_g(mid);
                            let eg = to_g(e);
                            if !astar_occ.used_cells.contains(&mg)
                                && !astar_occ.used_cells.contains(&eg)
                                && !foreign_tips.contains(&mg)
                                && !foreign_tips.contains(&eg)
                            {
                                add_ortho_seg(&mut segments, tip, mid);
                                add_ortho_seg(&mut segments, mid, e);
                                (e, d)
                            } else {
                                (tip, Side::North)
                            }
                        }
                        Side::South => {
                            let go_left = tip.x < rp.body_center_x - 0.1;
                            let mid = Point2::new(tip.x, snap_grid(tip.y + GRID_MM));
                            let (e, d) = if go_left {
                                (Point2::new(snap_grid(tip.x - GRID_MM), mid.y), Side::West)
                            } else {
                                (Point2::new(snap_grid(tip.x + GRID_MM), mid.y), Side::East)
                            };
                            let mg = to_g(mid);
                            let eg = to_g(e);
                            if !astar_occ.used_cells.contains(&mg)
                                && !astar_occ.used_cells.contains(&eg)
                                && !foreign_tips.contains(&mg)
                                && !foreign_tips.contains(&eg)
                            {
                                add_ortho_seg(&mut segments, tip, mid);
                                add_ortho_seg(&mut segments, mid, e);
                                (e, d)
                            } else {
                                (tip, Side::South)
                            }
                        }
                    };
                    terminals.push(NetTerminalIr {
                        kind: "label_flag".to_string(),
                        pos: end,
                        dir: term_dir,
                        label: net_name.clone(),
                    });
                }
                for &seg in &segments {
                    astar_occ.record_world_segment(seg);
                }
                for t in &terminals {
                    astar_occ.record_terminal_point(t.pos);
                }
                local_routes.push(SchematicRouteIr {
                    net_id: net_name,
                    sheet: sheet_tag.clone(),
                    style: "zone_local".to_string(),
                    is_power: is_pwr,
                    is_ground: is_gnd,
                    segments,
                    junctions: Vec::new(),
                    terminals,
                    label_pos: None,
                });
            }
        }
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

    // 5c. Post-routing Zone Enclosure & Clearance Pass:
    //     Expand every zone frame (`local_groups`) to strictly enclose all member pin tips,
    //     GND triangles, upward power arrows, and global/local label pills with guaranteed
    //     visual breathing margin so borders NEVER clip pins, power symbols, or labels!
    {
        let clearance_mm = 3.81_f64;
        let title_band_mm = 6.35_f64;
        // Precompute member body + pin bounds per group
        let entity_by_id: BTreeMap<&str, &EntityIr> = local_entities
            .iter()
            .map(|e| (e.id.as_str(), e))
            .collect();
        // Track current group content bounds (excluding note footer)
        let mut content_bounds: Vec<Rect2> = local_groups
            .iter()
            .map(|g| {
                let note_h = if g.notes.is_empty() {
                    0.0
                } else {
                    (g.notes.len() as f64) * 6.35 + 5.08
                };
                Rect2::new(g.bbox.min_x, g.bbox.min_y, g.bbox.max_x, g.bbox.max_y - note_h)
            })
            .collect();

        // Include every member component's pin tips & label bounds
        for (gi, g) in local_groups.iter().enumerate() {
            let cb = &mut content_bounds[gi];
            for mid in &g.members {
                if let Some(ent) = entity_by_id.get(mid.as_str()) {
                    if let EntityGeometryIr::Component { body_rect, pins, .. } = &ent.geometry {
                        cb.min_x = cb.min_x.min(body_rect.min_x - clearance_mm);
                        cb.max_x = cb.max_x.max(body_rect.max_x + clearance_mm);
                        cb.min_y = cb.min_y.min(body_rect.min_y - title_band_mm);
                        cb.max_y = cb.max_y.max(body_rect.max_y + clearance_mm);
                        for p in pins {
                            cb.min_x = cb.min_x.min(p.tip_point.x - clearance_mm);
                            cb.max_x = cb.max_x.max(p.tip_point.x + clearance_mm);
                            cb.min_y = cb.min_y.min(p.tip_point.y - title_band_mm);
                            cb.max_y = cb.max_y.max(p.tip_point.y + clearance_mm);
                        }
                    }
                }
            }
        }

        // Associate every routed terminal (GND symbol, power arrow, global/local label pill)
        // with its owning zone (nearest group whose pre-pass bbox contains or is closest to it)
        // and expand that zone's content bounds to enclose the full rendered symbol/text!
        for route in &local_routes {
            for term in &route.terminals {
                if local_groups.is_empty() {
                    break;
                }
                let best_gi = local_groups
                    .iter()
                    .enumerate()
                    .min_by(|(_, ga), (_, gb)| {
                        let da = dist_point_to_rect(term.pos, ga.bbox);
                        let db = dist_point_to_rect(term.pos, gb.bbox);
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i)
                    .unwrap();

                let tw = (term.label.len() as f64) * 1.55;
                let (tx0, ty0, tx1, ty1) = match term.kind.as_str() {
                    "gnd_symbol" => {
                        let hw = (tw * 0.5).max(2.0) + 1.5;
                        (term.pos.x - hw, term.pos.y - 1.0, term.pos.x + hw, term.pos.y + 5.8)
                    }
                    "power_symbol" => {
                        let hw = (tw * 0.5).max(2.0) + 1.5;
                        (term.pos.x - hw, term.pos.y - 5.8, term.pos.x + hw, term.pos.y + 1.0)
                    }
                    _ => {
                        // global_flag / label_flag / label_stub pill extends in `term.dir`
                        let pill_w = tw + 4.8;
                        match term.dir {
                            Side::West => (term.pos.x - pill_w, term.pos.y - 2.2, term.pos.x + 0.5, term.pos.y + 2.2),
                            Side::East => (term.pos.x - 0.5, term.pos.y - 2.2, term.pos.x + pill_w, term.pos.y + 2.2),
                            Side::North => (term.pos.x - 2.2, term.pos.y - pill_w, term.pos.x + 2.2, term.pos.y + 0.5),
                            Side::South => (term.pos.x - 2.2, term.pos.y - 0.5, term.pos.x + 2.2, term.pos.y + pill_w),
                        }
                    }
                };

                let cb = &mut content_bounds[best_gi];
                cb.min_x = cb.min_x.min(tx0 - clearance_mm);
                cb.max_x = cb.max_x.max(tx1 + clearance_mm);
                cb.min_y = cb.min_y.min(ty0 - title_band_mm);
                cb.max_y = cb.max_y.max(ty1 + clearance_mm);
            }
        }

        // Write final snapped zone boxes (preserving note footer room cleanly below content)
        for (gi, g) in local_groups.iter_mut().enumerate() {
            let note_h = if g.notes.is_empty() {
                0.0
            } else {
                (g.notes.len() as f64) * 4.5 + 4.5
            };
            let cb = content_bounds[gi];
            g.bbox = Rect2::new(
                snap_grid(cb.min_x).max(15.24),
                snap_grid(cb.min_y).max(15.24),
                snap_grid(cb.max_x),
                snap_grid(cb.max_y + note_h),
            );
        }

        // Ensure sibling zones on the same row never overlap horizontally after left/right expansion
        for i in 0..local_groups.len() {
            for j in (i + 1)..local_groups.len() {
                let a = local_groups[i].bbox;
                let b = local_groups[j].bbox;
                let y_overlap = a.min_y < b.max_y && b.min_y < a.max_y;
                if y_overlap && a.min_x < b.min_x && a.max_x > b.min_x - 2.54 {
                    let mid = snap_grid(0.5 * (a.max_x + b.min_x));
                    local_groups[i].bbox.max_x = (mid - 1.27).max(a.min_x + 10.16);
                    local_groups[j].bbox.min_x = (mid + 1.27).min(b.max_x - 10.16);
                }
            }
        }
    }

    // Compute tight actual sheet bounds from all routed coordinates & expanded zone frames
    let mut max_x = max_comp_x;
    let mut max_y = max_comp_y + 15.24;
    for g in &local_groups {
        max_x = max_x.max(g.bbox.max_x + 15.24);
        max_y = max_y.max(g.bbox.max_y + 15.24);
    }
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
        // Real KiCad printable interior (10.16mm border margin all around) + bottom-right title block keepout
        let frame_min_x = 10.16_f64;
        let frame_min_y = 10.16_f64;
        let frame_max_x = sw - 10.16;
        let frame_max_y = sh - 10.16;
        let tb_x0 = sw - 118.0;
        let tb_y0 = sh - 40.0;

        let mut frame_offenders: Vec<String> = Vec::new();
        let mut title_block_offenders: Vec<String> = Vec::new();
        let mut max_overflow_x = 0.0_f64;
        let mut max_overflow_y = 0.0_f64;

        for e in &local_entities {
            if let EntityGeometryIr::Component { body_rect, pins, .. } = &e.geometry {
                let mut r = *body_rect;
                for p in pins {
                    r.min_x = r.min_x.min(p.tip_point.x);
                    r.max_x = r.max_x.max(p.tip_point.x);
                    r.min_y = r.min_y.min(p.tip_point.y);
                    r.max_y = r.max_y.max(p.tip_point.y);
                }
                if r.min_x < frame_min_x || r.max_x > frame_max_x || r.min_y < frame_min_y || r.max_y > frame_max_y {
                    frame_offenders.push(e.id.clone());
                    max_overflow_x = max_overflow_x.max((r.max_x - frame_max_x).max(0.0));
                    max_overflow_y = max_overflow_y.max((r.max_y - frame_max_y).max(0.0));
                }
                if r.max_x > tb_x0 && r.max_y > tb_y0 {
                    title_block_offenders.push(e.id.clone());
                }
            }
        }
        let any_zone_overflow = local_groups.iter().any(|g| {
            g.bbox.min_x < frame_min_x
                || g.bbox.max_x > frame_max_x
                || g.bbox.min_y < frame_min_y
                || g.bbox.max_y > frame_max_y
        });
        let overflowed = !frame_offenders.is_empty()
            || !title_block_offenders.is_empty()
            || any_zone_overflow
            || this_bounds.width() > sw - 12.0
            || this_bounds.depth() > sh - 12.0;

        let util_w_pct = (this_bounds.width() / frame_max_x * 100.0).min(999.0);
        let util_h_pct = (this_bounds.depth() / frame_max_y * 100.0).min(999.0);
        let unused_w_mm = (frame_max_x - this_bounds.width()).max(0.0);

        if overflowed {
            let offenders_str = if !frame_offenders.is_empty() {
                let sample: Vec<&str> = frame_offenders.iter().take(8).map(|s| s.as_str()).collect();
                format!(
                    " ({} component(s) outside frame: [{}]{}; +{:.1}mm X / +{:.1}mm Y)",
                    frame_offenders.len(),
                    sample.join(", "),
                    if frame_offenders.len() > 8 { ", ..." } else { "" },
                    max_overflow_x,
                    max_overflow_y
                )
            } else if !title_block_offenders.is_empty() {
                format!(
                    " ({} component(s) collide with bottom-right title block: [{}])",
                    title_block_offenders.len(),
                    title_block_offenders.join(", ")
                )
            } else {
                String::new()
            };
            let space_hint = if unused_w_mm > 120.0 && max_overflow_y > 0.0 {
                format!(
                    "Sheet has {:.0}mm of unused horizontal space on the right ({:.0}% W × {:.0}% H) — use `layout {{ pack: skyline }}` to pack across the full width, split across sheets, or increase `sheet:`",
                    unused_w_mm, util_w_pct, util_h_pct
                )
            } else {
                "Use `layout { pack: skyline }`, split groups across multiple `schematic` sheets, or increase `sheet:` (e.g., `A2 landscape` / `A1 landscape`)".to_string()
            };
            diagnostics.push(
                Diagnostic::error(
                    "E_SHEET_FRAME_OVERFLOW",
                    "layout",
                    format!(
                        "Schematic '{}' bounds ({:.1}mm × {:.1}mm) exceed declared sheet '{}' ({:.0}mm × {:.0}mm){offenders_str}",
                        sheet_name_str,
                        this_bounds.width(),
                        this_bounds.depth(),
                        sheet_str,
                        sw,
                        sh
                    ),
                    schem.map(|s| s.span.clone()),
                )
                .with_entities(frame_offenders.clone())
                .with_suggestion(space_hint.clone()),
            );
            evidence.push(EvidenceRecord {
                check_id: format!("E11_sheet_frame_fit_{sheet_name_str}"),
                status: VerificationStatus::Violated,
                method: "exact_kicad_frame_and_titleblock_bounds_v3.1".to_string(),
                tolerance_mm: Some(10.16),
                subjects: frame_offenders,
                details: format!(
                    "Overflow on '{}' ({:.0}×{:.0}mm): utilization {:.0}% W × {:.0}% H{offenders_str}",
                    sheet_str, sw, sh, util_w_pct, util_h_pct
                ),
                repair_suggestions: vec![
                    RepairSuggestion {
                        fix_id: 1,
                        description: "Add `layout { pack: skyline }` to pack groups cleanly across the full sheet width.".to_string(),
                        auto_fixable: true,
                    },
                    RepairSuggestion {
                        fix_id: 2,
                        description: "Increase `sheet:` size (e.g. `A3 landscape` -> `A2 landscape` or `A1 landscape`).".to_string(),
                        auto_fixable: true,
                    },
                    RepairSuggestion {
                        fix_id: 3,
                        description: "Split functional groups across multiple `schematic` sheets.".to_string(),
                        auto_fixable: false,
                    },
                ],
            });
        } else {
            evidence.push(EvidenceRecord {
                check_id: format!("E11_sheet_frame_fit_{sheet_name_str}"),
                status: VerificationStatus::ExactlyChecked,
                method: "exact_kicad_frame_and_titleblock_bounds_v3.1".to_string(),
                tolerance_mm: Some(10.16),
                subjects: vec![sheet_name_str.clone()],
                details: format!(
                    "All {} component(s) & {} zone(s) fit cleanly inside '{}' ({:.0}×{:.0}mm) clear of title block; utilization {:.0}% W × {:.0}% H",
                    local_entities.len(),
                    local_groups.len(),
                    sheet_str,
                    sw,
                    sh,
                    util_w_pct,
                    util_h_pct
                ),
                repair_suggestions: Vec::new(),
            });
        }
    }

    // E12: Non-negotiable Component Body Overlap Check (`E_COMPONENT_OVERLAP`)!
    // Engineers/AI have full creative freedom over placement, but two physical symbol bodies must NEVER collide.
    {
        let mut comp_boxes: Vec<(&str, Rect2, Option<crate::core::span::Span>)> = Vec::new();
        for e in &local_entities {
            if let EntityGeometryIr::Component { body_rect, .. } = &e.geometry {
                comp_boxes.push((e.id.as_str(), *body_rect, e.span.clone()));
            }
        }
        let mut overlap_pairs: Vec<String> = Vec::new();
        let mut overlap_subjects: Vec<String> = Vec::new();
        for i in 0..comp_boxes.len() {
            for j in (i + 1)..comp_boxes.len() {
                let (id_a, ra, sp_a) = &comp_boxes[i];
                let (id_b, rb, _) = &comp_boxes[j];
                let ov_x = ra.max_x.min(rb.max_x) - ra.min_x.max(rb.min_x);
                let ov_y = ra.max_y.min(rb.max_y) - ra.min_y.max(rb.min_y);
                if ov_x > 0.2 && ov_y > 0.2 {
                    overlap_pairs.push(format!("{id_a}↔{id_b} ({ov_x:.2}×{ov_y:.2}mm)"));
                    overlap_subjects.push((*id_a).to_string());
                    overlap_subjects.push((*id_b).to_string());
                    diagnostics.push(
                        Diagnostic::error(
                            "E_COMPONENT_OVERLAP",
                            "layout",
                            format!(
                                "Components '{id_a}' and '{id_b}' overlap by {:.2}mm × {:.2}mm on sheet '{sheet_name_str}'",
                                ov_x, ov_y
                            ),
                            sp_a.clone(),
                        )
                        .with_entities(vec![(*id_a).to_string(), (*id_b).to_string()])
                        .with_suggestion(format!(
                            "Increase relative gap/offset between '{id_a}' and '{id_b}' (minimum {:.2}mm separation) or remove conflicting explicit placement",
                            ov_x.max(ov_y) + 5.08
                        )),
                    );
                }
            }
        }
        overlap_subjects.sort();
        overlap_subjects.dedup();
        if overlap_pairs.is_empty() {
            evidence.push(EvidenceRecord {
                check_id: format!("E12_no_component_overlap_{sheet_name_str}"),
                status: VerificationStatus::ExactlyChecked,
                method: "exact_symbol_body_aabb_collision_v3.1".to_string(),
                tolerance_mm: Some(0.2),
                subjects: vec![sheet_name_str.clone()],
                details: format!(
                    "0 component body overlaps across {} placed component(s) on '{sheet_name_str}'",
                    comp_boxes.len()
                ),
                repair_suggestions: Vec::new(),
            });
        } else {
            evidence.push(EvidenceRecord {
                check_id: format!("E12_no_component_overlap_{sheet_name_str}"),
                status: VerificationStatus::Violated,
                method: "exact_symbol_body_aabb_collision_v3.1".to_string(),
                tolerance_mm: Some(0.2),
                subjects: overlap_subjects,
                details: format!(
                    "{} component overlap(s) on '{sheet_name_str}': {}",
                    overlap_pairs.len(),
                    overlap_pairs.join(", ")
                ),
                repair_suggestions: vec![
                    RepairSuggestion {
                        fix_id: 1,
                        description: format!(
                            "Increase `gap:` or `offset:` on the overlapping pair(s): {}",
                            overlap_pairs.join(", ")
                        ),
                        auto_fixable: true,
                    },
                    RepairSuggestion {
                        fix_id: 2,
                        description: "Move one of the colliding components to an adjacent stack (`hstack`/`vstack`) or group.".to_string(),
                        auto_fixable: false,
                    },
                ],
            });
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
    schematic_dividers.extend(local_dividers);
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
        schematic_dividers,
        schematic_routes,
        explicit_sheets,
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
            repair_suggestions: Vec::new(),
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
                            repair_suggestions: Vec::new(),
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
                            repair_suggestions: Vec::new(),
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

    // 8. MNA-Backed Component Voltage & Power Derating Proofs (E13 / E14):
    //    Enforces 20% safety derating (operating <= 80% of rated) whenever components declare
    //    `voltage_rating:` / `power_rating:` (or `props { "voltage_rating": "...", "power_rating": "..." }`).
    run_derating_checks(
        &comp_order,
        &comp_map,
        &pin_to_nets,
        &ground_nets,
        &net_driven_voltage,
        &mut ir,
    );

    ir.project.has_errors = ir.diagnostics.iter().any(|d| d.severity == Severity::Error);
    ir
}

fn parse_rating_volts(comp: &ComponentDecl) -> Option<f64> {
    if let Some(v) = comp.voltage_rating_v {
        return Some(v);
    }
    for (k, v) in &comp.props {
        let kl = k.to_lowercase();
        if matches!(kl.as_str(), "voltage_rating" | "rated_voltage" | "max_voltage" | "voltage") {
            let trimmed = v.trim();
            let stripped = trimmed
                .trim_end_matches("VDC")
                .trim_end_matches("Vdc")
                .trim_end_matches('V')
                .trim_end_matches('v')
                .trim();
            if let Ok(num) = stripped.parse::<f64>() {
                if num > 0.0 {
                    return Some(num);
                }
            }
        }
    }
    None
}

fn parse_rating_watts(comp: &ComponentDecl) -> Option<f64> {
    if let Some(w) = comp.power_rating_w {
        return Some(w);
    }
    for (k, v) in &comp.props {
        let kl = k.to_lowercase();
        if matches!(kl.as_str(), "power_rating" | "rated_power" | "max_power" | "power") {
            let trimmed = v.trim();
            if let Some(mw) = trimmed.strip_suffix("mW").or_else(|| trimmed.strip_suffix("mw")) {
                if let Ok(num) = mw.trim().parse::<f64>() {
                    if num > 0.0 {
                        return Some(num * 1e-3);
                    }
                }
            } else if let Some(w) = trimmed.strip_suffix('W').or_else(|| trimmed.strip_suffix('w')) {
                if let Ok(num) = w.trim().parse::<f64>() {
                    if num > 0.0 {
                        return Some(num);
                    }
                }
            }
        }
    }
    None
}

fn next_standard_voltage_tier(min_v: f64) -> f64 {
    const TIERS: &[f64] = &[6.3, 10.0, 16.0, 25.0, 35.0, 50.0, 63.0, 100.0, 250.0, 400.0, 630.0, 1000.0];
    TIERS.iter().copied().find(|&t| t >= min_v - 1e-6).unwrap_or(min_v.ceil())
}

fn next_standard_power_tier(min_w: f64) -> f64 {
    const TIERS: &[f64] = &[0.0625, 0.1, 0.125, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 25.0];
    TIERS.iter().copied().find(|&t| t >= min_w - 1e-9).unwrap_or(min_w.ceil())
}

fn run_derating_checks(
    comp_order: &[String],
    comp_map: &BTreeMap<String, &ComponentDecl>,
    pin_to_nets: &BTreeMap<(String, String), Vec<String>>,
    ground_nets: &BTreeSet<String>,
    net_driven_voltage: &BTreeMap<String, Vec<(String, f64)>>,
    ir: &mut CanonicalIr,
) {
    let rated_v_comps: Vec<(String, &ComponentDecl, f64)> = comp_order
        .iter()
        .filter_map(|cid| {
            let c = *comp_map.get(cid)?;
            Some((cid.clone(), c, parse_rating_volts(c)?))
        })
        .collect();
    let rated_w_comps: Vec<(String, &ComponentDecl, f64)> = comp_order
        .iter()
        .filter_map(|cid| {
            let c = *comp_map.get(cid)?;
            Some((cid.clone(), c, parse_rating_watts(c)?))
        })
        .collect();

    if rated_v_comps.is_empty() && rated_w_comps.is_empty() {
        return;
    }

    // Solve real MNA DC operating point for exact node voltages & branch power dissipation
    let sim = crate::domains::elec::spice::simulate_dc_and_rc(ir);
    let net_voltage = |net: &str| -> Option<f64> {
        if ground_nets.contains(net) {
            return Some(0.0);
        }
        if let Some(&v) = sim.node_voltages_v.get(net) {
            return Some(v);
        }
        net_driven_voltage
            .get(net)
            .and_then(|v| v.first())
            .map(|(_, volts)| *volts)
    };

    // E13: Component Voltage Derating (require rated_v >= operating_v * 1.25 => <=80% stress)
    if !rated_v_comps.is_empty() {
        const DERATING_FACTOR: f64 = 1.25;
        let mut violations: Vec<String> = Vec::new();
        let mut repair_suggestions: Vec<RepairSuggestion> = Vec::new();
        let mut subjects: Vec<String> = Vec::new();

        for (cid, comp, rated_v) in &rated_v_comps {
            subjects.push(cid.clone());
            // Measure max voltage difference across any pair of pins (or max pin voltage vs GND)
            let mut pin_vs: Vec<f64> = Vec::new();
            for pin in &comp.pins {
                if let Some(nets) = pin_to_nets.get(&(cid.clone(), pin.name.clone())) {
                    for n in nets {
                        if let Some(v) = net_voltage(n) {
                            pin_vs.push(v);
                        }
                    }
                }
            }
            if pin_vs.is_empty() {
                continue;
            }
            let min_v = pin_vs.iter().copied().fold(f64::INFINITY, f64::min);
            let max_v = pin_vs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let operating_v = if pin_vs.len() >= 2 {
                (max_v - min_v).abs().max(max_v.abs())
            } else {
                max_v.abs()
            };
            let min_required_rating = operating_v * DERATING_FACTOR;
            if *rated_v + 1e-6 < min_required_rating {
                let rec_tier = next_standard_voltage_tier(min_required_rating);
                let stress_pct = if *rated_v > 1e-9 {
                    (operating_v / *rated_v) * 100.0
                } else {
                    999.0
                };
                let severity_note = if operating_v > *rated_v {
                    "EXCEEDS absolute maximum rating"
                } else {
                    "violates 20% voltage derating margin (max 80% stress)"
                };
                violations.push(format!("{cid}({operating_v:.2}V/{rated_v:.1}V)"));
                let fix_desc = format!(
                    "Upgrade '{cid}' voltage_rating from {rated_v:.1}V to at least {rec_tier:.1}V (operating {operating_v:.2}V requires >= {min_required_rating:.2}V with 20% derating)"
                );
                repair_suggestions.push(RepairSuggestion {
                    fix_id: violations.len(),
                    description: fix_desc.clone(),
                    auto_fixable: false,
                });
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_VOLTAGE_DERATING",
                        "verification",
                        format!(
                            "Component '{cid}' ({}) rated {rated_v:.1}V sees {operating_v:.2}V ({stress_pct:.0}% stress) — {severity_note}",
                            comp.value
                        ),
                        Some(comp.span.clone()),
                    )
                    .with_entities(vec![cid.clone()])
                    .with_measurement(operating_v, *rated_v / DERATING_FACTOR, "V")
                    .with_suggestion(fix_desc),
                );
            }
        }

        ir.evidence.push(EvidenceRecord {
            check_id: "E13_voltage_derating".to_string(),
            status: if violations.is_empty() {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "mna_dc_voltage_derating_80pct_v3.2".to_string(),
            tolerance_mm: None,
            subjects,
            details: if violations.is_empty() {
                format!(
                    "All {} voltage-rated component(s) within 80% DC voltage derating limit",
                    rated_v_comps.len()
                )
            } else {
                format!(
                    "{} voltage derating violation(s): {}",
                    violations.len(),
                    violations.join(", ")
                )
            },
            repair_suggestions,
        });
    }

    // E14: Component Power Dissipation Derating (require rated_w >= dissipated_w * 1.25 => <=80% thermal stress)
    if !rated_w_comps.is_empty() {
        const DERATING_FACTOR: f64 = 1.25;
        let mut violations: Vec<String> = Vec::new();
        let mut repair_suggestions: Vec<RepairSuggestion> = Vec::new();
        let mut subjects: Vec<String> = Vec::new();

        for (cid, comp, rated_w) in &rated_w_comps {
            subjects.push(cid.clone());
            // Prefer exact MNA branch power; fallback to V^2/R for resistors if branch wasn't listed
            let dissipated_w = sim
                .branches
                .iter()
                .find(|b| b.component == *cid)
                .map(|b| (b.power_mw / 1000.0).abs())
                .or_else(|| {
                    let r_ohm = crate::domains::elec::spice::parse_si_component_value(&comp.value, "R")?;
                    if r_ohm <= 1e-9 || comp.pins.len() < 2 {
                        return None;
                    }
                    let v0 = pin_to_nets
                        .get(&(cid.clone(), comp.pins[0].name.clone()))
                        .and_then(|ns| ns.iter().find_map(|n| net_voltage(n)))?;
                    let v1 = pin_to_nets
                        .get(&(cid.clone(), comp.pins[1].name.clone()))
                        .and_then(|ns| ns.iter().find_map(|n| net_voltage(n)))?;
                    let dv = v0 - v1;
                    Some((dv * dv) / r_ohm)
                });
            let Some(p_w) = dissipated_w else {
                continue;
            };
            let min_required_w = p_w * DERATING_FACTOR;
            if *rated_w + 1e-9 < min_required_w {
                let rec_tier = next_standard_power_tier(min_required_w);
                let stress_pct = if *rated_w > 1e-9 {
                    (p_w / *rated_w) * 100.0
                } else {
                    999.0
                };
                violations.push(format!("{cid}({:.1}mW/{:.1}mW)", p_w * 1000.0, rated_w * 1000.0));
                let fix_desc = format!(
                    "Upgrade '{cid}' power_rating from {rated_w:.3}W to at least {rec_tier:.3}W (dissipating {:.1}mW requires >= {:.1}mW with 20% thermal derating)",
                    p_w * 1000.0,
                    min_required_w * 1000.0
                );
                repair_suggestions.push(RepairSuggestion {
                    fix_id: violations.len(),
                    description: fix_desc.clone(),
                    auto_fixable: false,
                });
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_POWER_DERATING",
                        "verification",
                        format!(
                            "Component '{cid}' ({}) rated {rated_w:.3}W dissipates {p_w:.3}W ({stress_pct:.0}% thermal stress) — exceeds 80% power derating limit",
                            comp.value
                        ),
                        Some(comp.span.clone()),
                    )
                    .with_entities(vec![cid.clone()])
                    .with_measurement(p_w, *rated_w / DERATING_FACTOR, "W")
                    .with_suggestion(fix_desc),
                );
            }
        }

        ir.evidence.push(EvidenceRecord {
            check_id: "E14_power_derating".to_string(),
            status: if violations.is_empty() {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "mna_dc_power_derating_80pct_v3.2".to_string(),
            tolerance_mm: None,
            subjects,
            details: if violations.is_empty() {
                format!(
                    "All {} power-rated component(s) within 80% thermal power derating limit",
                    rated_w_comps.len()
                )
            } else {
                format!(
                    "{} power derating violation(s): {}",
                    violations.len(),
                    violations.join(", ")
                )
            },
            repair_suggestions,
        });
    }
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
                repair_suggestions: Vec::new(),
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
                repair_suggestions: Vec::new(),
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
                    let load_coil_nets: Option<(String, String)> = two_pin_nets(l_id, load).or_else(|| {
                        // Multi-pin relays (e.g. `Relay:Relay_SPDT` with coil + COM/NO/NC contacts):
                        // locate the switched coil pin and its paired supply/ground coil pin!
                        let mut distinct: Vec<String> = Vec::new();
                        for p in &load.pins {
                            if let Some(ns) = pin_to_nets.get(&(l_id.clone(), p.name.clone())) {
                                for n in ns {
                                    if !distinct.contains(n) {
                                        distinct.push(n.clone());
                                    }
                                }
                            }
                        }
                        if distinct.contains(&sw_net) {
                            let peer = distinct.iter().find(|n| {
                                *n != &sw_net
                                    && (is_power_like_net(n)
                                        || ground_nets.contains(*n)
                                        || n.to_uppercase().contains("24V")
                                        || n.to_uppercase().contains("12V")
                                        || n.to_uppercase().contains("5V"))
                            });
                            peer.map(|p| (sw_net.clone(), p.clone()))
                        } else {
                            None
                        }
                    });
                    let Some((na, nb)) = load_coil_nets else {
                        continue;
                    };
                    let other_net = if na == sw_net {
                        &nb
                    } else if nb == sw_net {
                        &na
                    } else {
                        continue;
                    };
                    if !is_power_like_net(other_net)
                        && !ground_nets.contains(other_net)
                        && !other_net.to_uppercase().contains("24V")
                        && !other_net.to_uppercase().contains("12V")
                    {
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
                repair_suggestions: Vec::new(),
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
                repair_suggestions: Vec::new(),
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
    } else if low.contains("a0") {
        (1189.0, 841.0)
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

fn dist_point_to_rect(p: Point2, r: Rect2) -> f64 {
    let dx = if p.x < r.min_x {
        r.min_x - p.x
    } else if p.x > r.max_x {
        p.x - r.max_x
    } else {
        0.0
    };
    let dy = if p.y < r.min_y {
        r.min_y - p.y
    } else if p.y > r.max_y {
        p.y - r.max_y
    } else {
        0.0
    };
    (dx * dx + dy * dy).sqrt()
}

fn is_power_rail_name(name: &str) -> bool {
    let leaf = name.rsplit('/').next().unwrap_or(name);
    let u = leaf.to_uppercase();
    if u.contains("_ADC") || u.contains("_SENSE") || u.contains("_SNS") || u.contains("_SIG") || u.contains("_DIV") {
        return false;
    }
    u.starts_with('V') || u.starts_with('+') || u.starts_with('-')
}
