//! High-density AI-native circuit idiom expander (`decouple`, `pullup`, `pulldown`, `divider`, `series`, `rc_filter`).
//!
//! Expands 1-line engineering idioms inside `circuit { ... }` into real components (auto-hydrated
//! from KiCad 9 `.kicad_sym` libraries), exact electrical nets, and pin-anchored relative layout
//! hints so AI models express complex sub-circuits in ~5–10x fewer lines with zero coordinates.

use crate::core::units::Quantity;
use crate::domains::elec::ast::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub struct ExpandedIdioms {
    pub extra_components: Vec<ComponentDecl>,
    pub extra_nets: Vec<NetDecl>,
    pub auto_relative_placements: Vec<RelativePlacementDecl>,
    /// Generated component ID -> anchor component ID (inherits anchor's sheet & group automatically).
    pub component_anchor: BTreeMap<String, String>,
}

/// Infer standard KiCad 9 `Device:*` symbol, default SMD footprint, and whether default `0deg` orientation is vertical.
fn infer_passive_symbol(ref_id: &str) -> (&'static str, &'static str, bool) {
    let up = ref_id.to_ascii_uppercase();
    if up.starts_with("FB") {
        ("Device:FerriteBead", "Inductor_SMD:L_0603_1608Metric", true)
    } else if up.starts_with('C') {
        ("Device:C", "Capacitor_SMD:C_0603_1608Metric", true)
    } else if up.starts_with('L') {
        ("Device:L", "Inductor_SMD:L_0603_1608Metric", true)
    } else if up.starts_with('F') {
        ("Device:Fuse", "Fuse:Fuse_1206_3216Metric", true)
    } else if up.starts_with('D') {
        ("Device:D", "Diode_SMD:D_SOD-123", false)
    } else {
        ("Device:R", "Resistor_SMD:R_0603_1608Metric", true)
    }
}

fn make_passive_comp(
    id: String,
    value: String,
    footprint_override: Option<String>,
    horizontal: bool,
    span: crate::core::span::Span,
) -> ComponentDecl {
    let (lib_id, def_fp, default_is_vertical) = infer_passive_symbol(&id);
    // In KiCad 9, Device:R/C/L/Fuse are vertical at 0deg (pin 1 North, pin 2 South),
    // and horizontal at 90deg CCW (pin 1 West, pin 2 East).
    let orient_deg = if horizontal && default_is_vertical {
        90
    } else if !horizontal && !default_is_vertical {
        90
    } else {
        0
    };
    ComponentDecl {
        id,
        category: lib_id.to_string(),
        is_quoted_symbol: true,
        value,
        footprint: footprint_override.or_else(|| Some(def_fp.to_string())),
        datasheet: None,
        orient_deg,
        mirror: None,
        unit: None,
        no_connect: Vec::new(),
        isolated: false,
        dnp: false,
        in_bom: true,
        on_board: true,
        props: Vec::new(),
        voltage_rating_v: None,
        power_rating_w: None,
        pins: Vec::new(),
        provenance: None,
        span,
    }
}

fn append_to_net(
    net_map: &mut BTreeMap<String, Vec<PinRef>>,
    net_order: &mut Vec<String>,
    net_name: &str,
    pin_ref: PinRef,
) {
    if !net_map.contains_key(net_name) {
        net_order.push(net_name.to_string());
    }
    let list = net_map.entry(net_name.to_string()).or_default();
    if !list
        .iter()
        .any(|p| p.component == pin_ref.component && p.pin == pin_ref.pin)
    {
        list.push(pin_ref);
    }
}

/// Expand all `CircuitIdiomDecl`s across every `circuit { ... }` in a project.
pub fn expand_circuit_idioms(circuits: &[CircuitDecl]) -> ExpandedIdioms {
    let mut out = ExpandedIdioms::default();

    // Index existing declared components and pin->net assignments so idioms attach to existing nets cleanly!
    let mut existing_comps: BTreeSet<String> = BTreeSet::new();
    let mut pin_to_net: BTreeMap<(String, String), String> = BTreeMap::new();
    for c in circuits {
        for comp in &c.components {
            existing_comps.insert(comp.id.clone());
        }
        for net in &c.nets {
            for pr in &net.connects {
                pin_to_net
                    .entry((pr.component.clone(), pr.pin.clone()))
                    .or_insert_with(|| net.name.clone());
            }
        }
    }

    let mut net_additions: BTreeMap<String, Vec<PinRef>> = BTreeMap::new();
    let mut net_order: Vec<String> = Vec::new();
    let mut net_spans: BTreeMap<String, crate::core::span::Span> = BTreeMap::new();

    let resolve_pin_net = |pr: &PinRef,
                           preferred: Option<&str>,
                           pin_to_net: &mut BTreeMap<(String, String), String>,
                           net_additions: &mut BTreeMap<String, Vec<PinRef>>,
                           net_order: &mut Vec<String>,
                           net_spans: &mut BTreeMap<String, crate::core::span::Span>|
     -> String {
        if let Some(existing) = pin_to_net.get(&(pr.component.clone(), pr.pin.clone())) {
            return existing.clone();
        }
        let nname = preferred
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}_{}", pr.component, pr.pin));
        pin_to_net.insert((pr.component.clone(), pr.pin.clone()), nname.clone());
        net_spans.entry(nname.clone()).or_insert_with(|| pr.span.clone());
        append_to_net(net_additions, net_order, &nname, pr.clone());
        nname
    };

    for circuit in circuits {
        for idiom in &circuit.idioms {
            match idiom {
                CircuitIdiomDecl::Decouple {
                    target,
                    caps,
                    ground,
                    footprint,
                    span,
                } => {
                    let rail_net = resolve_pin_net(
                        target,
                        None,
                        &mut pin_to_net,
                        &mut net_additions,
                        &mut net_order,
                        &mut net_spans,
                    );
                    net_spans.entry(ground.clone()).or_insert_with(|| span.clone());

                    let mut prev_cap: Option<String> = None;
                    for (cid, cval) in caps {
                        if existing_comps.insert(cid.clone()) {
                            out.extra_components.push(make_passive_comp(
                                cid.clone(),
                                cval.clone(),
                                footprint.clone(),
                                false, // vertical shunt cap
                                span.clone(),
                            ));
                        }
                        append_to_net(
                            &mut net_additions,
                            &mut net_order,
                            &rail_net,
                            PinRef {
                                component: cid.clone(),
                                pin: "1".to_string(),
                                span: span.clone(),
                            },
                        );
                        append_to_net(
                            &mut net_additions,
                            &mut net_order,
                            ground,
                            PinRef {
                                component: cid.clone(),
                                pin: "2".to_string(),
                                span: span.clone(),
                            },
                        );
                        out.component_anchor
                            .insert(cid.clone(), target.component.clone());

                        if let Some(ref prev) = prev_cap {
                            out.auto_relative_placements.push(RelativePlacementDecl {
                                component: cid.clone(),
                                kind: RelativePlacementKind::Directional {
                                    dir: RelDir::RightOf,
                                    anchor: AnchorRef {
                                        component: prev.clone(),
                                        pin: None,
                                    },
                                    gap: Some(Quantity::mm(7.62)),
                                    align_pin: None,
                                },
                                orient_deg: Some(0),
                                mirror: None,
                                is_auto: true,
                                span: span.clone(),
                            });
                        } else {
                            out.auto_relative_placements.push(RelativePlacementDecl {
                                component: cid.clone(),
                                kind: RelativePlacementKind::Directional {
                                    dir: RelDir::Below,
                                    anchor: AnchorRef {
                                        component: target.component.clone(),
                                        pin: Some(target.pin.clone()),
                                    },
                                    gap: Some(Quantity::mm(7.62)),
                                    align_pin: Some("1".to_string()),
                                },
                                orient_deg: Some(0),
                                mirror: None,
                                is_auto: true,
                                span: span.clone(),
                            });
                        }
                        prev_cap = Some(cid.clone());
                    }
                }

                CircuitIdiomDecl::PullResistor {
                    is_up,
                    target,
                    ref_id,
                    value,
                    rail,
                    footprint,
                    span,
                } => {
                    let sig_net = resolve_pin_net(
                        target,
                        None,
                        &mut pin_to_net,
                        &mut net_additions,
                        &mut net_order,
                        &mut net_spans,
                    );
                    net_spans.entry(rail.clone()).or_insert_with(|| span.clone());
                    if existing_comps.insert(ref_id.clone()) {
                        out.extra_components.push(make_passive_comp(
                            ref_id.clone(),
                            value.clone(),
                            footprint.clone(),
                            false, // vertical pull resistor
                            span.clone(),
                        ));
                    }
                    let (top_net, bot_net) = if *is_up {
                        (rail.as_str(), sig_net.as_str())
                    } else {
                        (sig_net.as_str(), rail.as_str())
                    };
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        top_net,
                        PinRef {
                            component: ref_id.clone(),
                            pin: "1".to_string(),
                            span: span.clone(),
                        },
                    );
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        bot_net,
                        PinRef {
                            component: ref_id.clone(),
                            pin: "2".to_string(),
                            span: span.clone(),
                        },
                    );
                    out.component_anchor
                        .insert(ref_id.clone(), target.component.clone());
                    out.auto_relative_placements.push(RelativePlacementDecl {
                        component: ref_id.clone(),
                        kind: RelativePlacementKind::Directional {
                            dir: if *is_up { RelDir::Above } else { RelDir::Below },
                            anchor: AnchorRef {
                                component: target.component.clone(),
                                pin: Some(target.pin.clone()),
                            },
                            gap: Some(Quantity::mm(7.62)),
                            align_pin: Some(if *is_up { "2" } else { "1" }.to_string()),
                        },
                        orient_deg: Some(0),
                        mirror: None,
                        is_auto: true,
                        span: span.clone(),
                    });
                }

                CircuitIdiomDecl::Divider {
                    top_net,
                    r_top: (rt_id, rt_val),
                    tap,
                    tap_net_name,
                    r_bot: (rb_id, rb_val),
                    bot_net,
                    footprint,
                    span,
                } => {
                    net_spans.entry(top_net.clone()).or_insert_with(|| span.clone());
                    net_spans.entry(bot_net.clone()).or_insert_with(|| span.clone());
                    if existing_comps.insert(rt_id.clone()) {
                        out.extra_components.push(make_passive_comp(
                            rt_id.clone(),
                            rt_val.clone(),
                            footprint.clone(),
                            false,
                            span.clone(),
                        ));
                    }
                    if existing_comps.insert(rb_id.clone()) {
                        out.extra_components.push(make_passive_comp(
                            rb_id.clone(),
                            rb_val.clone(),
                            footprint.clone(),
                            false,
                            span.clone(),
                        ));
                    }
                    let tap_net = match tap {
                        ChainEndpoint::Net(n) => {
                            net_spans.entry(n.clone()).or_insert_with(|| span.clone());
                            n.clone()
                        }
                        ChainEndpoint::Pin(pr) => resolve_pin_net(
                            pr,
                            tap_net_name.as_deref(),
                            &mut pin_to_net,
                            &mut net_additions,
                            &mut net_order,
                            &mut net_spans,
                        ),
                    };
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        top_net,
                        PinRef { component: rt_id.clone(), pin: "1".to_string(), span: span.clone() },
                    );
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        &tap_net,
                        PinRef { component: rt_id.clone(), pin: "2".to_string(), span: span.clone() },
                    );
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        &tap_net,
                        PinRef { component: rb_id.clone(), pin: "1".to_string(), span: span.clone() },
                    );
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        bot_net,
                        PinRef { component: rb_id.clone(), pin: "2".to_string(), span: span.clone() },
                    );

                    if let ChainEndpoint::Pin(pr) = tap {
                        out.component_anchor.insert(rt_id.clone(), pr.component.clone());
                        out.component_anchor.insert(rb_id.clone(), pr.component.clone());
                        // Align R_top.2 horizontally with the target feedback pin, then stack R_bot directly below R_top!
                        out.auto_relative_placements.push(RelativePlacementDecl {
                            component: rt_id.clone(),
                            kind: RelativePlacementKind::Directional {
                                dir: RelDir::LeftOf,
                                anchor: AnchorRef {
                                    component: pr.component.clone(),
                                    pin: Some(pr.pin.clone()),
                                },
                                gap: Some(Quantity::mm(10.16)),
                                align_pin: Some("2".to_string()),
                            },
                            orient_deg: Some(0),
                            mirror: None,
                            is_auto: true,
                            span: span.clone(),
                        });
                    }
                    out.auto_relative_placements.push(RelativePlacementDecl {
                        component: rb_id.clone(),
                        kind: RelativePlacementKind::Directional {
                            dir: RelDir::Below,
                            anchor: AnchorRef {
                                component: rt_id.clone(),
                                pin: Some("2".to_string()),
                            },
                            gap: Some(Quantity::mm(5.08)),
                            align_pin: Some("1".to_string()),
                        },
                        orient_deg: Some(0),
                        mirror: None,
                        is_auto: true,
                        span: span.clone(),
                    });
                }

                CircuitIdiomDecl::SeriesChain {
                    from,
                    stages,
                    to,
                    footprint,
                    span,
                } => {
                    if stages.is_empty() {
                        continue;
                    }
                    for (sid, sval) in stages {
                        if existing_comps.insert(sid.clone()) {
                            out.extra_components.push(make_passive_comp(
                                sid.clone(),
                                sval.clone(),
                                footprint.clone(),
                                true, // horizontal inline series element!
                                span.clone(),
                            ));
                        }
                    }
                    let start_net = match from {
                        ChainEndpoint::Net(n) => {
                            net_spans.entry(n.clone()).or_insert_with(|| span.clone());
                            n.clone()
                        }
                        ChainEndpoint::Pin(pr) => resolve_pin_net(
                            pr,
                            None,
                            &mut pin_to_net,
                            &mut net_additions,
                            &mut net_order,
                            &mut net_spans,
                        ),
                    };
                    let end_net = match to {
                        ChainEndpoint::Net(n) => {
                            net_spans.entry(n.clone()).or_insert_with(|| span.clone());
                            n.clone()
                        }
                        ChainEndpoint::Pin(pr) => resolve_pin_net(
                            pr,
                            None,
                            &mut pin_to_net,
                            &mut net_additions,
                            &mut net_order,
                            &mut net_spans,
                        ),
                    };

                    // Connect start -> stage[0].1
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        &start_net,
                        PinRef { component: stages[0].0.clone(), pin: "1".to_string(), span: span.clone() },
                    );
                    // Connect intermediate stage[i].2 -> stage[i+1].1
                    for w in stages.windows(2) {
                        let mid_net = format!("{}_TO_{}", w[0].0, w[1].0);
                        net_spans.entry(mid_net.clone()).or_insert_with(|| span.clone());
                        append_to_net(
                            &mut net_additions,
                            &mut net_order,
                            &mid_net,
                            PinRef { component: w[0].0.clone(), pin: "2".to_string(), span: span.clone() },
                        );
                        append_to_net(
                            &mut net_additions,
                            &mut net_order,
                            &mid_net,
                            PinRef { component: w[1].0.clone(), pin: "1".to_string(), span: span.clone() },
                        );
                    }
                    // Connect stage[last].2 -> end
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        &end_net,
                        PinRef {
                            component: stages.last().unwrap().0.clone(),
                            pin: "2".to_string(),
                            span: span.clone(),
                        },
                    );

                    // Pin-anchored horizontal layout: anchor to destination pin (going left) or source pin (going right)!
                    if let ChainEndpoint::Pin(tp) = to {
                        for (sid, _) in stages {
                            out.component_anchor.insert(sid.clone(), tp.component.clone());
                        }
                        let last_id = stages.last().unwrap().0.clone();
                        out.auto_relative_placements.push(RelativePlacementDecl {
                            component: last_id,
                            kind: RelativePlacementKind::Directional {
                                dir: RelDir::LeftOf,
                                anchor: AnchorRef {
                                    component: tp.component.clone(),
                                    pin: Some(tp.pin.clone()),
                                },
                                gap: Some(Quantity::mm(7.62)),
                                align_pin: Some("2".to_string()),
                            },
                            orient_deg: Some(90),
                            mirror: None,
                            is_auto: true,
                            span: span.clone(),
                        });
                        for i in (0..stages.len().saturating_sub(1)).rev() {
                            out.auto_relative_placements.push(RelativePlacementDecl {
                                component: stages[i].0.clone(),
                                kind: RelativePlacementKind::Directional {
                                    dir: RelDir::LeftOf,
                                    anchor: AnchorRef {
                                        component: stages[i + 1].0.clone(),
                                        pin: Some("1".to_string()),
                                    },
                                    gap: Some(Quantity::mm(5.08)),
                                    align_pin: Some("2".to_string()),
                                },
                                orient_deg: Some(90),
                                mirror: None,
                                is_auto: true,
                                span: span.clone(),
                            });
                        }
                    } else if let ChainEndpoint::Pin(fp) = from {
                        for (sid, _) in stages {
                            out.component_anchor.insert(sid.clone(), fp.component.clone());
                        }
                        out.auto_relative_placements.push(RelativePlacementDecl {
                            component: stages[0].0.clone(),
                            kind: RelativePlacementKind::Directional {
                                dir: RelDir::RightOf,
                                anchor: AnchorRef {
                                    component: fp.component.clone(),
                                    pin: Some(fp.pin.clone()),
                                },
                                gap: Some(Quantity::mm(7.62)),
                                align_pin: Some("1".to_string()),
                            },
                            orient_deg: Some(90),
                            mirror: None,
                            is_auto: true,
                            span: span.clone(),
                        });
                        for i in 1..stages.len() {
                            out.auto_relative_placements.push(RelativePlacementDecl {
                                component: stages[i].0.clone(),
                                kind: RelativePlacementKind::Directional {
                                    dir: RelDir::RightOf,
                                    anchor: AnchorRef {
                                        component: stages[i - 1].0.clone(),
                                        pin: Some("2".to_string()),
                                    },
                                    gap: Some(Quantity::mm(5.08)),
                                    align_pin: Some("1".to_string()),
                                },
                                orient_deg: Some(90),
                                mirror: None,
                                is_auto: true,
                                span: span.clone(),
                            });
                        }
                    }
                }

                CircuitIdiomDecl::RcFilter {
                    from,
                    r_id,
                    r_val,
                    tap,
                    tap_net_name,
                    c_id,
                    c_val,
                    ground,
                    footprint,
                    span,
                } => {
                    if existing_comps.insert(r_id.clone()) {
                        out.extra_components.push(make_passive_comp(
                            r_id.clone(),
                            r_val.clone(),
                            footprint.clone(),
                            true, // horizontal R
                            span.clone(),
                        ));
                    }
                    if existing_comps.insert(c_id.clone()) {
                        out.extra_components.push(make_passive_comp(
                            c_id.clone(),
                            c_val.clone(),
                            None, // capacitor default footprint
                            false, // vertical shunt C
                            span.clone(),
                        ));
                    }
                    let in_net = match from {
                        ChainEndpoint::Net(n) => {
                            net_spans.entry(n.clone()).or_insert_with(|| span.clone());
                            n.clone()
                        }
                        ChainEndpoint::Pin(pr) => resolve_pin_net(
                            pr,
                            None,
                            &mut pin_to_net,
                            &mut net_additions,
                            &mut net_order,
                            &mut net_spans,
                        ),
                    };
                    let tap_net = match tap {
                        ChainEndpoint::Net(n) => {
                            net_spans.entry(n.clone()).or_insert_with(|| span.clone());
                            n.clone()
                        }
                        ChainEndpoint::Pin(pr) => resolve_pin_net(
                            pr,
                            tap_net_name.as_deref(),
                            &mut pin_to_net,
                            &mut net_additions,
                            &mut net_order,
                            &mut net_spans,
                        ),
                    };
                    net_spans.entry(ground.clone()).or_insert_with(|| span.clone());
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        &in_net,
                        PinRef { component: r_id.clone(), pin: "1".to_string(), span: span.clone() },
                    );
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        &tap_net,
                        PinRef { component: r_id.clone(), pin: "2".to_string(), span: span.clone() },
                    );
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        &tap_net,
                        PinRef { component: c_id.clone(), pin: "1".to_string(), span: span.clone() },
                    );
                    append_to_net(
                        &mut net_additions,
                        &mut net_order,
                        ground,
                        PinRef { component: c_id.clone(), pin: "2".to_string(), span: span.clone() },
                    );

                    if let ChainEndpoint::Pin(tp) = tap {
                        out.component_anchor.insert(r_id.clone(), tp.component.clone());
                        out.component_anchor.insert(c_id.clone(), tp.component.clone());
                        out.auto_relative_placements.push(RelativePlacementDecl {
                            component: r_id.clone(),
                            kind: RelativePlacementKind::Directional {
                                dir: RelDir::LeftOf,
                                anchor: AnchorRef {
                                    component: tp.component.clone(),
                                    pin: Some(tp.pin.clone()),
                                },
                                gap: Some(Quantity::mm(10.16)),
                                align_pin: Some("2".to_string()),
                            },
                            orient_deg: Some(90),
                            mirror: None,
                            is_auto: true,
                            span: span.clone(),
                        });
                    } else if let ChainEndpoint::Pin(fp) = from {
                        out.component_anchor.insert(r_id.clone(), fp.component.clone());
                        out.component_anchor.insert(c_id.clone(), fp.component.clone());
                        out.auto_relative_placements.push(RelativePlacementDecl {
                            component: r_id.clone(),
                            kind: RelativePlacementKind::Directional {
                                dir: RelDir::RightOf,
                                anchor: AnchorRef {
                                    component: fp.component.clone(),
                                    pin: Some(fp.pin.clone()),
                                },
                                gap: Some(Quantity::mm(7.62)),
                                align_pin: Some("1".to_string()),
                            },
                            orient_deg: Some(90),
                            mirror: None,
                            is_auto: true,
                            span: span.clone(),
                        });
                    }
                    out.auto_relative_placements.push(RelativePlacementDecl {
                        component: c_id.clone(),
                        kind: RelativePlacementKind::Directional {
                            dir: RelDir::Below,
                            anchor: AnchorRef {
                                component: r_id.clone(),
                                pin: Some("2".to_string()),
                            },
                            gap: Some(Quantity::mm(5.08)),
                            align_pin: Some("1".to_string()),
                        },
                        orient_deg: Some(0),
                        mirror: None,
                        is_auto: true,
                        span: span.clone(),
                    });
                }
            }
        }
    }

    for nname in net_order {
        if let Some(connects) = net_additions.remove(&nname) {
            let sp = net_spans
                .remove(&nname)
                .unwrap_or_else(|| crate::core::span::Span::new("", 0, 0, 0, 0));
            out.extra_nets.push(NetDecl {
                name: nname,
                hierarchy: Vec::new(),
                connects,
                linked_nets: Vec::new(),
                power_flag: false,
                span: sp,
            });
        }
    }

    out
}
