//! Building Plumbing Overlay Domain (`mep.plumbing`) compiler & verifier (v1.4.0).
//!
//! Layers on top of solved `arch` floor plans (and coordinates with `mep.elec` when present):
//! - Reads solved room bounds, wet architectural fixtures (`toilet`, `vanity`/`sink`/`basin`,
//!   `shower`, `tub`, `washer`), and `mep.elec` devices directly from `CanonicalIr`.
//! - Routes Cold Water Supply (`CWS`), Hot Water Supply (`HWS`), and Drainage/Waste/Vent (`DWV`)
//!   pipes orthogonally with parallel system offsets so lines stay crisp and legible.
//! - Runs 6 exact EN 806 / EN 12056 / IPC engineering checks (`P1`–`P6`), including
//!   cross-discipline `mep.plumbing` <-> `mep.elec` clash verification!

use crate::core::geom::{Point2, Rect2, Side};
use crate::core::ir::*;
use crate::core::span::{Diagnostic, Severity, Span};
use crate::syntax::ast::ProjectAst;
use crate::syntax::ext::{ExtNode, ExtValue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlumbingNode {
    pub id: String,
    /// `cold_water` | `water_heater` | `soil_stack` | `vent_stack` | `isolation_valve` | `floor_drain`
    pub kind: String,
    /// `cold` | `hot` | `soil` | `waste` | `vent`
    pub system: String,
    pub room: String,
    pub host_wall: Option<String>,
    pub pos: [f64; 2],
    pub facing: Option<String>,
    pub dia_mm: f64,
    pub capacity_l: Option<f64>,
    pub serves: Option<String>,
    pub vented: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeRun {
    pub id: String,
    /// `cold` | `hot` | `soil` | `waste` | `vent`
    pub system: String,
    pub from_id: String,
    pub to_id: String,
    pub dia_mm: f64,
    pub slope_ratio: Option<String>,
    pub slope_pct: Option<f64>,
    pub length_mm: f64,
    pub flow_l_s: f64,
    pub velocity_m_s: f64,
    #[serde(default)]
    pub dfu: f64,
    #[serde(default)]
    pub is_header: bool,
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlumbingModel {
    pub name: String,
    pub standard: String,
    pub supply_pressure_bar: f64,
    pub nodes: Vec<PlumbingNode>,
    pub pipes: Vec<PipeRun>,
    pub connected_fixtures: Vec<String>,
}

struct RoomInfo {
    rect: Rect2,
}

struct FixtureInfo {
    subtype: String,
    rect: Rect2,
    against: Option<Side>,
}

/// Compile all `building_plumbing` blocks onto an already-solved architectural (+ optional electrical) IR.
pub fn compile_building_plumbing(ast: &ProjectAst, ir: &mut CanonicalIr) {
    let blocks: Vec<_> = ast
        .extensions
        .iter()
        .filter(|b| b.keyword == "building_plumbing")
        .collect();
    if blocks.is_empty() {
        return;
    }

    // Index solved rooms, architectural fixtures, and electrical devices from the shared IR
    let mut rooms: BTreeMap<String, RoomInfo> = BTreeMap::new();
    let mut fixtures: BTreeMap<String, FixtureInfo> = BTreeMap::new();
    let mut elec_devices: Vec<(String, String, Point2)> = Vec::new();

    for e in &ir.entities {
        match &e.geometry {
            EntityGeometryIr::Rect { min, max, .. } if e.kind == "arch.room" => {
                rooms.insert(
                    e.id.clone(),
                    RoomInfo {
                        rect: Rect2::new(min[0], min[1], max[0], max[1]),
                    },
                );
            }
            EntityGeometryIr::Fixture {
                category,
                against,
                body_rect,
                ..
            } => {
                fixtures.insert(
                    e.id.clone(),
                    FixtureInfo {
                        subtype: category.clone(),
                        rect: *body_rect,
                        against: *against,
                    },
                );
            }
            EntityGeometryIr::MepDevice {
                device_kind, pos, ..
            } => {
                elec_devices.push((e.id.clone(), device_kind.clone(), *pos));
            }
            _ => {}
        }
    }

    let default_frame = ir
        .frames
        .first()
        .map(|f| f.id.clone())
        .unwrap_or_else(|| "Plan".to_string());

    for block in blocks {
        let tree = match block.parse_tree() {
            Ok(t) => t,
            Err(d) => {
                ir.diagnostics.push(d);
                ir.project.has_errors = true;
                continue;
            }
        };

        let model_name = block
            .name()
            .unwrap_or_else(|| "BuildingPlumbing".to_string());
        let standard = tree
            .ident("standard")
            .unwrap_or("EN 806 / EN 12056")
            .to_string();
        let supply_pressure_bar = tree.num("supply_pressure_bar").unwrap_or(3.0);

        let mut nodes: BTreeMap<String, PlumbingNode> = BTreeMap::new();
        let mut node_spans: BTreeMap<String, Span> = BTreeMap::new();
        let mut pipes: Vec<PipeRun> = Vec::new();
        let mut pipe_spans: Vec<Span> = Vec::new();

        // 1. Parse risers, water heaters, soil/vent stacks, valves, floor drains, cleanouts
        for child in &tree.children {
            match child.kind.as_str() {
                "riser" | "heater" | "stack" | "valve" | "floor_drain" | "cleanout" => {
                    let id = child
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("PL_{}", nodes.len() + 1));
                    let subtype = child
                        .subtype
                        .clone()
                        .unwrap_or_else(|| match child.kind.as_str() {
                            "riser" => "cold_water".to_string(),
                            "heater" => "water_heater".to_string(),
                            "stack" => "soil_stack".to_string(),
                            "valve" => "isolation_valve".to_string(),
                            "floor_drain" => "floor_drain".to_string(),
                            "cleanout" => "cleanout".to_string(),
                            _ => child.kind.clone(),
                        });
                    let system = child
                        .ident("system")
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| match subtype.as_str() {
                            "cold_water" => "cold".to_string(),
                            "water_heater" => "hot".to_string(),
                            "soil_stack" => "soil".to_string(),
                            "vent_stack" => "vent".to_string(),
                            "floor_drain" => "waste".to_string(),
                            "cleanout" => "soil".to_string(),
                            _ => "cold".to_string(),
                        });

                    let Some((room, host_wall, pos, facing)) =
                        resolve_node_position(child, &rooms, ir)
                    else {
                        continue;
                    };

                    let default_dia = match subtype.as_str() {
                        "cold_water" => 25.0,
                        "water_heater" => 20.0,
                        "soil_stack" => 110.0,
                        "vent_stack" => 50.0,
                        "floor_drain" => 50.0,
                        _ => 20.0,
                    };
                    let dia_mm = child
                        .mm("dia")
                        .or_else(|| child.mm("hot_dia"))
                        .unwrap_or(default_dia);
                    let capacity_l = child.num("capacity_l");
                    let serves = child.ident("serves").map(|s| s.to_string());
                    let vented = child
                        .ident("vented")
                        .map(|s| s == "true")
                        .unwrap_or(subtype == "soil_stack" || subtype == "vent_stack");

                    nodes.insert(
                        id.clone(),
                        PlumbingNode {
                            id: id.clone(),
                            kind: subtype,
                            system,
                            room,
                            host_wall,
                            pos: [pos.x, pos.y],
                            facing: facing.map(|f| format!("{f:?}").to_lowercase()),
                            dia_mm,
                            capacity_l,
                            serves,
                            vented,
                        },
                    );
                    node_spans.insert(id, child.span.clone());
                }
                _ => {}
            }
        }

        // 1b. Parse Commercial Collector Headers (`header <Name>: soil_header|waste_header|cold_header|hot_header|vent_header`)
        struct HeaderInfo {
            system: String,
            p0: Point2,
            p1: Point2,
            pipe_idx: usize,
            dia_mm: f64,
            dfu_sum: f64,
            span: Span,
        }
        let mut headers: BTreeMap<String, HeaderInfo> = BTreeMap::new();
        for child in tree.children_of("header") {
            let Some(hid) = child.name.clone() else {
                continue;
            };
            let sub = child.subtype.as_deref().unwrap_or("soil_header");
            let sys = match sub {
                "cold_header" | "cold" => "cold",
                "hot_header" | "hot" => "hot",
                "waste_header" | "waste" => "waste",
                "vent_header" | "vent" => "vent",
                _ => "soil",
            };
            let Some(from_id) = child.ident("from") else { continue };
            let Some(to_id) = child.ident("to") else { continue };
            let (Some(n0), Some(n1)) = (nodes.get(from_id), nodes.get(to_id)) else {
                ir.diagnostics.push(Diagnostic::error(
                    "E_PLUMB_UNKNOWN_NODE",
                    "plumbing",
                    format!("Header '{hid}' references unknown endpoint node '{from_id}' or '{to_id}'"),
                    Some(child.span.clone()),
                ));
                ir.project.has_errors = true;
                continue;
            };
            let p0 = Point2::new(n0.pos[0], n0.pos[1]);
            let p1 = Point2::new(n1.pos[0], n1.pos[1]);
            let default_dia = match sys {
                "soil" => 110.0,
                "waste" => 100.0,
                "vent" => 50.0,
                "cold" => 32.0,
                _ => 25.0,
            };
            let dia = child.mm("dia").unwrap_or(default_dia);
            let slope_str = child.ident("slope").map(|s| s.to_string()).or_else(|| {
                if sys == "soil" {
                    Some("1:100".to_string())
                } else if sys == "waste" {
                    Some("1:50".to_string())
                } else {
                    None
                }
            });
            let slope_pct = slope_str.as_deref().and_then(parse_slope_pct);
            let pts = vec![p0, p1];
            let len = p0.dist(p1);
            let pipe_idx = pipes.len();
            pipes.push(PipeRun {
                id: hid.clone(),
                system: sys.to_string(),
                from_id: from_id.to_string(),
                to_id: to_id.to_string(),
                dia_mm: dia,
                slope_ratio: slope_str,
                slope_pct,
                length_mm: len,
                flow_l_s: if sys == "cold" || sys == "hot" { 0.65 } else { 2.0 },
                velocity_m_s: calc_velocity_m_s(if sys == "cold" || sys == "hot" { 0.65 } else { 2.0 }, dia),
                dfu: 0.0,
                is_header: true,
                points: pts.iter().map(|p| [p.x, p.y]).collect(),
            });
            pipe_spans.push(child.span.clone());
            headers.insert(
                hid,
                HeaderInfo {
                    system: sys.to_string(),
                    p0,
                    p1,
                    pipe_idx,
                    dia_mm: dia,
                    dfu_sum: 0.0,
                    span: child.span.clone(),
                },
            );
        }

        // 2. Auto-route cold water feeds to water heaters (`cold_in: ColdMain`)
        //    and drains from floor drains (`to: SoilMain` or `to: WasteHeader`)
        for child in &tree.children {
            if child.kind == "heater" {
                let Some(hid) = child.name.as_deref() else {
                    continue;
                };
                if let Some(src_id) = child.ident("cold_in") {
                    if let (Some(src), Some(dst)) = (nodes.get(src_id), nodes.get(hid)) {
                        let dia = child.mm("cold_dia").unwrap_or(20.0);
                        let pts = route_orthogonal(
                            Point2::new(src.pos[0], src.pos[1]),
                            Point2::new(dst.pos[0], dst.pos[1]),
                            "cold",
                            &rooms,
                        );
                        let len = polyline_len(&pts);
                        let flow = 0.35; // Heater trunk feed (L/s)
                        let vel = calc_velocity_m_s(flow, dia);
                        pipes.push(PipeRun {
                            id: format!("CWS_{src_id}_to_{hid}"),
                            system: "cold".to_string(),
                            from_id: src_id.to_string(),
                            to_id: hid.to_string(),
                            dia_mm: dia,
                            slope_ratio: None,
                            slope_pct: None,
                            length_mm: len,
                            flow_l_s: flow,
                            velocity_m_s: vel,
                            dfu: 0.0,
                            is_header: false,
                            points: pts.iter().map(|p| [p.x, p.y]).collect(),
                        });
                        pipe_spans.push(child.span.clone());
                    }
                }
            } else if child.kind == "floor_drain" {
                let Some(fd_id) = child.name.as_deref() else {
                    continue;
                };
                if let Some(dst_id) = child.ident("to") {
                    if let Some(fd) = nodes.get(fd_id) {
                        let fd_pt = Point2::new(fd.pos[0], fd.pos[1]);
                        let target_pt = if let Some(hdr) = headers.get_mut(dst_id) {
                            hdr.dfu_sum += 2.0; // IPC Floor Drain = 2 DFU
                            Some(project_to_segment(fd_pt, hdr.p0, hdr.p1))
                        } else {
                            nodes.get(dst_id).map(|n| Point2::new(n.pos[0], n.pos[1]))
                        };
                        if let Some(tpt) = target_pt {
                            let dia = child.mm("dia").unwrap_or(50.0);
                            let slope_str = child.ident("slope").unwrap_or("1:50").to_string();
                            let slope_pct = parse_slope_pct(&slope_str);
                            let pts = route_orthogonal(fd_pt, tpt, "waste", &rooms);
                            let len = polyline_len(&pts);
                            pipes.push(PipeRun {
                                id: format!("DWV_{fd_id}_to_{dst_id}"),
                                system: "waste".to_string(),
                                from_id: fd_id.to_string(),
                                to_id: dst_id.to_string(),
                                dia_mm: dia,
                                slope_ratio: Some(slope_str),
                                slope_pct,
                                length_mm: len,
                                flow_l_s: 0.4,
                                velocity_m_s: calc_velocity_m_s(0.4, dia),
                                dfu: 2.0,
                                is_header: false,
                                points: pts.iter().map(|p| [p.x, p.y]).collect(),
                            });
                            pipe_spans.push(child.span.clone());
                        }
                    }
                }
            }
        }

        // 3. Parse fixture connections (`connect FixtureName { cold: ..., hot: ..., soil/waste: ..., ... }`)
        let mut fixture_has_cold: BTreeSet<String> = BTreeSet::new();
        let mut fixture_has_hot: BTreeSet<String> = BTreeSet::new();
        let mut fixture_has_drain: BTreeSet<String> = BTreeSet::new();
        let mut connected_fixtures: BTreeSet<String> = BTreeSet::new();
        let mut pipe_vented_override: BTreeMap<String, bool> = BTreeMap::new();

        for child in tree.children_of("connect") {
            let Some(fx_id) = child.name.clone() else {
                continue;
            };
            let Some(fx) = fixtures.get(&fx_id) else {
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_PLUMB_UNKNOWN_FIXTURE",
                        "plumbing",
                        format!("Plumbing connection references unknown architectural fixture '{fx_id}'"),
                        Some(child.span.clone()),
                    )
                    .with_entities(vec![fx_id.clone()]),
                );
                ir.project.has_errors = true;
                continue;
            };
            connected_fixtures.insert(fx_id.clone());
            let fx_pt = fixture_plumbing_anchor(fx);
            let flow_cold = fixture_design_flow_l_s(&fx.subtype, false);
            let flow_hot = fixture_design_flow_l_s(&fx.subtype, true);
            let aux_vented = child.ident("aux_vent").is_some()
                || child.ident("vent").is_some()
                || child.ident("vented").map(|s| s == "true").unwrap_or(false);

            // Cold supply branch (can tap either a riser node OR a collector header!)
            if let Some(src_id) = child.ident("cold") {
                let src_pt = if let Some(hdr) = headers.get(src_id) {
                    Some(project_to_segment(fx_pt, hdr.p0, hdr.p1))
                } else {
                    nodes.get(src_id).map(|n| Point2::new(n.pos[0], n.pos[1]))
                };
                if let Some(spt) = src_pt {
                    let dia = child.mm("cold_dia").unwrap_or(15.0);
                    let pts = route_orthogonal(spt, fx_pt, "cold", &rooms);
                    let len = polyline_len(&pts);
                    let vel = calc_velocity_m_s(flow_cold, dia);
                    pipes.push(PipeRun {
                        id: format!("CWS_{fx_id}"),
                        system: "cold".to_string(),
                        from_id: src_id.to_string(),
                        to_id: fx_id.clone(),
                        dia_mm: dia,
                        slope_ratio: None,
                        slope_pct: None,
                        length_mm: len,
                        flow_l_s: flow_cold,
                        velocity_m_s: vel,
                        dfu: 0.0,
                        is_header: false,
                        points: pts.iter().map(|p| [p.x, p.y]).collect(),
                    });
                    pipe_spans.push(child.span.clone());
                    fixture_has_cold.insert(fx_id.clone());
                } else {
                    ir.diagnostics.push(Diagnostic::error(
                        "E_PLUMB_UNKNOWN_NODE",
                        "plumbing",
                        format!("Cold supply for '{fx_id}' references unknown riser/header '{src_id}'"),
                        Some(child.span.clone()),
                    ));
                    ir.project.has_errors = true;
                }
            }

            // Hot supply branch (can tap either a heater node OR a hot collector header!)
            if let Some(src_id) = child.ident("hot") {
                let src_pt = if let Some(hdr) = headers.get(src_id) {
                    Some(project_to_segment(fx_pt, hdr.p0, hdr.p1))
                } else {
                    nodes.get(src_id).map(|n| Point2::new(n.pos[0], n.pos[1]))
                };
                if let Some(spt) = src_pt {
                    let dia = child.mm("hot_dia").unwrap_or(15.0);
                    let pts = route_orthogonal(spt, fx_pt, "hot", &rooms);
                    let len = polyline_len(&pts);
                    let vel = calc_velocity_m_s(flow_hot, dia);
                    pipes.push(PipeRun {
                        id: format!("HWS_{fx_id}"),
                        system: "hot".to_string(),
                        from_id: src_id.to_string(),
                        to_id: fx_id.clone(),
                        dia_mm: dia,
                        slope_ratio: None,
                        slope_pct: None,
                        length_mm: len,
                        flow_l_s: flow_hot,
                        velocity_m_s: vel,
                        dfu: 0.0,
                        is_header: false,
                        points: pts.iter().map(|p| [p.x, p.y]).collect(),
                    });
                    pipe_spans.push(child.span.clone());
                    fixture_has_hot.insert(fx_id.clone());
                } else {
                    ir.diagnostics.push(Diagnostic::error(
                        "E_PLUMB_UNKNOWN_NODE",
                        "plumbing",
                        format!("Hot supply for '{fx_id}' references unknown heater/header '{src_id}'"),
                        Some(child.span.clone()),
                    ));
                    ir.project.has_errors = true;
                }
            }

            // Optional explicit Vent Pipe branch (`vent: VentHeader | VentStack  vent_dia: 50mm`)
            if let Some(v_id) = child.ident("vent") {
                let dst_pt = if let Some(hdr) = headers.get(v_id) {
                    Some(project_to_segment(fx_pt, hdr.p0, hdr.p1))
                } else {
                    nodes.get(v_id).map(|n| Point2::new(n.pos[0], n.pos[1]))
                };
                if let Some(vpt) = dst_pt {
                    let dia = child.mm("vent_dia").unwrap_or(50.0);
                    let pts = route_orthogonal(fx_pt, vpt, "vent", &rooms);
                    let len = polyline_len(&pts);
                    pipes.push(PipeRun {
                        id: format!("VP_{fx_id}"),
                        system: "vent".to_string(),
                        from_id: fx_id.clone(),
                        to_id: v_id.to_string(),
                        dia_mm: dia,
                        slope_ratio: None,
                        slope_pct: None,
                        length_mm: len,
                        flow_l_s: 0.0,
                        velocity_m_s: 0.0,
                        dfu: 0.0,
                        is_header: false,
                        points: pts.iter().map(|p| [p.x, p.y]).collect(),
                    });
                    pipe_spans.push(child.span.clone());
                }
            }

            // Soil or waste drainage branch (can tap either a soil stack OR a collector header!)
            let drain_pair = child
                .ident("soil")
                .map(|s| ("soil", s))
                .or_else(|| child.ident("waste").map(|s| ("waste", s)));
            if let Some((sys, dst_id)) = drain_pair {
                let fx_dfu = fixture_dfu(&fx.subtype, &fx_id);
                let dst_pt = if let Some(hdr) = headers.get_mut(dst_id) {
                    hdr.dfu_sum += fx_dfu;
                    Some(project_to_segment(fx_pt, hdr.p0, hdr.p1))
                } else {
                    nodes.get(dst_id).map(|n| Point2::new(n.pos[0], n.pos[1]))
                };
                if let Some(dpt) = dst_pt {
                    let default_dia = if sys == "soil" { 110.0 } else { 50.0 };
                    let dia = child.mm("drain_dia").unwrap_or(default_dia);
                    let slope_str = child
                        .ident("slope")
                        .unwrap_or(if sys == "soil" { "1:60" } else { "1:40" })
                        .to_string();
                    let slope_pct = parse_slope_pct(&slope_str);
                    let pts = route_orthogonal(fx_pt, dpt, sys, &rooms);
                    let len = polyline_len(&pts);
                    let pid = format!("DWV_{fx_id}");
                    pipe_vented_override.insert(pid.clone(), aux_vented);
                    pipes.push(PipeRun {
                        id: pid,
                        system: sys.to_string(),
                        from_id: fx_id.clone(),
                        to_id: dst_id.to_string(),
                        dia_mm: dia,
                        slope_ratio: Some(slope_str),
                        slope_pct,
                        length_mm: len,
                        flow_l_s: if sys == "soil" { 2.0 } else { 0.5 },
                        velocity_m_s: calc_velocity_m_s(if sys == "soil" { 2.0 } else { 0.5 }, dia),
                        dfu: fx_dfu,
                        is_header: false,
                        points: pts.iter().map(|p| [p.x, p.y]).collect(),
                    });
                    pipe_spans.push(child.span.clone());
                    fixture_has_drain.insert(fx_id.clone());
                } else {
                    ir.diagnostics.push(Diagnostic::error(
                        "E_PLUMB_UNKNOWN_NODE",
                        "plumbing",
                        format!("Drain for '{fx_id}' references unknown stack/header '{dst_id}'"),
                        Some(child.span.clone()),
                    ));
                    ir.project.has_errors = true;
                }
            }
        }

        // Write accumulated DFUs back onto header trunk PipeRuns & run P7 DFU capacity check!
        for hdr in headers.values() {
            if let Some(p) = pipes.get_mut(hdr.pipe_idx) {
                p.dfu = hdr.dfu_sum;
            }
        }

        // =========================================================================
        // 4. EXACT ENGINEERING VERIFICATION RULES (P1 – P6)
        // =========================================================================

        // P1: Every wet architectural fixture has required cold (+ hot where applicable) + drainage
        let mut p1_missing: Vec<String> = Vec::new();
        let mut wet_fixtures: Vec<String> = Vec::new();
        for (fx_id, fx) in &fixtures {
            let (needs_cold, needs_hot, needs_drain) = wet_fixture_requirements(&fx.subtype, fx_id);
            if !needs_cold && !needs_hot && !needs_drain {
                continue;
            }
            wet_fixtures.push(fx_id.clone());
            let mut gaps = Vec::new();
            if needs_cold && !fixture_has_cold.contains(fx_id) {
                gaps.push("cold");
            }
            if needs_hot && !fixture_has_hot.contains(fx_id) {
                gaps.push("hot");
            }
            if needs_drain && !fixture_has_drain.contains(fx_id) {
                gaps.push("drain");
            }
            if !gaps.is_empty() {
                p1_missing.push(format!("{fx_id} ({})", gaps.join("/")));
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_PLUMB_MISSING_SUPPLY",
                        "plumbing",
                        format!(
                            "Wet fixture '{}' is missing required plumbing connection(s): {}",
                            fx_id,
                            gaps.join(", ")
                        ),
                        Some(tree.span.clone()),
                    )
                    .with_entities(vec![fx_id.clone()])
                    .with_suggestion(format!(
                        "Add `connect {fx_id} {{ cold: <Riser> {} ... }}` inside `building_plumbing`",
                        if needs_hot { "hot: <Heater>" } else { "" }
                    )),
                );
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "P1_wet_fixture_supply".to_string(),
            status: if p1_missing.is_empty() {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "en806_fixture_coverage_audit_v1.4".to_string(),
            tolerance_mm: None,
            subjects: wet_fixtures.clone(),
            details: if p1_missing.is_empty() {
                format!(
                    "All {} wet architectural fixture(s) have verified CWS/HWS supply & DWV drainage",
                    wet_fixtures.len()
                )
            } else {
                format!("Missing connections: {}", p1_missing.join("; "))
            },
        });

        // P2: Drainage pipe sizing and gravity slope check (EN 12056 / IPC)
        let mut p2_errs: Vec<String> = Vec::new();
        let mut drain_ids: Vec<String> = Vec::new();
        for (idx, pipe) in pipes.iter().enumerate() {
            if pipe.system != "soil" && pipe.system != "waste" {
                continue;
            }
            drain_ids.push(pipe.id.clone());
            let sp = pipe_spans.get(idx).cloned();
            if pipe.system == "soil" {
                if pipe.dia_mm < 100.0 {
                    p2_errs.push(format!("{} Ø{:.0}mm < 100mm", pipe.id, pipe.dia_mm));
                    ir.diagnostics.push(
                        Diagnostic::error(
                            "E_PLUMB_UNDERSIZED_DRAIN",
                            "plumbing",
                            format!(
                                "Soil pipe '{}' has diameter {:.0}mm, below 100mm minimum for WC discharge",
                                pipe.id, pipe.dia_mm
                            ),
                            sp.clone(),
                        )
                        .with_entities(vec![pipe.from_id.clone()]),
                    );
                }
                if let Some(pct) = pipe.slope_pct {
                    if !(1.0..=2.5).contains(&pct) {
                        p2_errs.push(format!("{} slope {:.2}%", pipe.id, pct));
                        ir.diagnostics.push(
                            Diagnostic::error(
                                "E_PLUMB_INVALID_SLOPE",
                                "plumbing",
                                format!(
                                    "Soil pipe '{}' slope {:.2}% ({}) outside EN 12056 range 1.0%..2.5% (1:100..1:40)",
                                    pipe.id,
                                    pct,
                                    pipe.slope_ratio.as_deref().unwrap_or("")
                                ),
                                sp,
                            )
                            .with_entities(vec![pipe.from_id.clone()]),
                        );
                    }
                }
            } else {
                // Waste pipe (basin/shower/sink/washer/floor_drain)
                let min_dia = if pipe.from_id.to_lowercase().contains("basin")
                    || pipe.from_id.to_lowercase().contains("vanity")
                {
                    40.0
                } else {
                    50.0
                };
                if pipe.dia_mm < min_dia {
                    p2_errs.push(format!("{} Ø{:.0}mm < {:.0}mm", pipe.id, pipe.dia_mm, min_dia));
                    ir.diagnostics.push(
                        Diagnostic::error(
                            "E_PLUMB_UNDERSIZED_DRAIN",
                            "plumbing",
                            format!(
                                "Waste pipe '{}' has diameter {:.0}mm, below {:.0}mm minimum",
                                pipe.id, pipe.dia_mm, min_dia
                            ),
                            sp.clone(),
                        )
                        .with_entities(vec![pipe.from_id.clone()]),
                    );
                }
                if let Some(pct) = pipe.slope_pct {
                    if !(2.0..=5.0).contains(&pct) {
                        p2_errs.push(format!("{} slope {:.2}%", pipe.id, pct));
                        ir.diagnostics.push(
                            Diagnostic::error(
                                "E_PLUMB_INVALID_SLOPE",
                                "plumbing",
                                format!(
                                    "Waste pipe '{}' slope {:.2}% ({}) outside self-cleansing range 2.0%..5.0% (1:50..1:20)",
                                    pipe.id,
                                    pct,
                                    pipe.slope_ratio.as_deref().unwrap_or("")
                                ),
                                sp,
                            )
                            .with_entities(vec![pipe.from_id.clone()]),
                        );
                    }
                }
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "P2_drainage_sizing_and_slope".to_string(),
            status: if p2_errs.is_empty() {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "en12056_gravity_drainage_check_v1.4".to_string(),
            tolerance_mm: None,
            subjects: drain_ids.clone(),
            details: if p2_errs.is_empty() {
                format!(
                    "All {} DWV soil/waste run(s) meet minimum diameter & self-cleansing gravity slope",
                    drain_ids.len()
                )
            } else {
                format!("Drainage violations: {}", p2_errs.join("; "))
            },
        });

        // P3: Unvented trap-arm maximum distance check (prevents trap siphonage!)
        let mut p3_errs: Vec<String> = Vec::new();
        let mut max_ratio = 0.0_f64;
        for (idx, pipe) in pipes.iter().enumerate() {
            if (pipe.system != "soil" && pipe.system != "waste") || pipe.is_header {
                continue;
            }
            let aux_vented = *pipe_vented_override.get(&pipe.id).unwrap_or(&false);
            if aux_vented {
                continue;
            }
            let max_len_mm = if pipe.dia_mm >= 100.0 {
                4500.0
            } else if pipe.dia_mm >= 50.0 {
                3500.0
            } else {
                2500.0
            };
            max_ratio = max_ratio.max(pipe.length_mm / max_len_mm);
            if pipe.length_mm > max_len_mm {
                p3_errs.push(format!(
                    "{} {:.2}m > {:.2}m",
                    pipe.id,
                    pipe.length_mm / 1000.0,
                    max_len_mm / 1000.0
                ));
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_PLUMB_TRAP_ARM_EXCEEDED",
                        "plumbing",
                        format!(
                            "Unvented trap arm '{}' is {:.2}m long, exceeding {:.2}m limit for Ø{:.0}mm pipe (trap siphonage risk)",
                            pipe.id,
                            pipe.length_mm / 1000.0,
                            max_len_mm / 1000.0,
                            pipe.dia_mm
                        ),
                        pipe_spans.get(idx).cloned(),
                    )
                    .with_entities(vec![pipe.from_id.clone()])
                    .with_suggestion(format!(
                        "Place a closer soil stack or add `vented: true` (auxiliary vent) to `connect {}`",
                        pipe.from_id
                    )),
                );
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "P3_trap_arm_distance".to_string(),
            status: if p3_errs.is_empty() {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "ipc_en12056_trap_arm_check_v1.4".to_string(),
            tolerance_mm: Some(1.0),
            subjects: drain_ids,
            details: if p3_errs.is_empty() {
                format!(
                    "All unvented trap arms within siphonage limits (peak utilization {:.0}%)",
                    max_ratio * 100.0
                )
            } else {
                format!("Trap-arm violations: {}", p3_errs.join("; "))
            },
        });

        // P4: Supply water velocity check (v <= 2.0 m/s to prevent water hammer & noise)
        let mut p4_errs: Vec<String> = Vec::new();
        let mut max_v = 0.0_f64;
        let mut supply_ids = Vec::new();
        for (idx, pipe) in pipes.iter().enumerate() {
            if pipe.system != "cold" && pipe.system != "hot" {
                continue;
            }
            supply_ids.push(pipe.id.clone());
            max_v = max_v.max(pipe.velocity_m_s);
            if pipe.velocity_m_s > 2.0 {
                p4_errs.push(format!("{} {:.2}m/s", pipe.id, pipe.velocity_m_s));
                ir.diagnostics.push(
                    Diagnostic::error(
                        "E_PLUMB_HIGH_VELOCITY",
                        "plumbing",
                        format!(
                            "Supply pipe '{}' peak velocity {:.2} m/s exceeds 2.0 m/s acoustic/erosion limit (Ø{:.0}mm at {:.2} L/s)",
                            pipe.id, pipe.velocity_m_s, pipe.dia_mm, pipe.flow_l_s
                        ),
                        pipe_spans.get(idx).cloned(),
                    )
                    .with_entities(vec![pipe.to_id.clone()])
                    .with_suggestion("Increase supply pipe diameter to 20mm or 25mm"),
                );
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "P4_supply_velocity_and_sizing".to_string(),
            status: if p4_errs.is_empty() {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "en806_hydraulic_velocity_check_v1.4".to_string(),
            tolerance_mm: None,
            subjects: supply_ids,
            details: if p4_errs.is_empty() {
                format!(
                    "Peak supply velocity {:.2} m/s <= 2.00 m/s across all CWS/HWS branches",
                    max_v
                )
            } else {
                format!("Velocity violations: {}", p4_errs.join("; "))
            },
        });

        // P5: Isolation shutoff valves on main cold supply & hot water heater
        let has_cold_valve = nodes
            .values()
            .any(|n| n.kind == "isolation_valve" && n.system == "cold");
        let has_heater = nodes.values().any(|n| n.kind == "water_heater");
        let has_hot_valve = nodes
            .values()
            .any(|n| n.kind == "isolation_valve" && n.system == "hot");
        let mut p5_errs = Vec::new();
        if !has_cold_valve {
            p5_errs.push("main cold supply");
            ir.diagnostics.push(Diagnostic::error(
                "E_PLUMB_MISSING_VALVE",
                "plumbing",
                "No cold-water isolation shutoff valve declared in `building_plumbing`",
                Some(tree.span.clone()),
            ));
        }
        if has_heater && !has_hot_valve {
            p5_errs.push("hot water heater");
            ir.diagnostics.push(Diagnostic::error(
                "E_PLUMB_MISSING_VALVE",
                "plumbing",
                "Water heater declared without a hot-water isolation valve",
                Some(tree.span.clone()),
            ));
        }
        let valve_ids: Vec<String> = nodes
            .values()
            .filter(|n| n.kind == "isolation_valve")
            .map(|n| n.id.clone())
            .collect();
        ir.evidence.push(EvidenceRecord {
            check_id: "P5_isolation_valves".to_string(),
            status: if p5_errs.is_empty() {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "en806_valve_audit_v1.4".to_string(),
            tolerance_mm: None,
            subjects: valve_ids.clone(),
            details: if p5_errs.is_empty() {
                format!(
                    "{} isolation shutoff valve(s) verified on cold main & hot supply",
                    valve_ids.len()
                )
            } else {
                format!("Missing isolation valve(s): {}", p5_errs.join(", "))
            },
        });

        // P6: Cross-Discipline Clash Verification (mep.plumbing <-> mep.elec)
        let mut p6_clashes: Vec<String> = Vec::new();
        let mut min_clearance_mm = f64::INFINITY;
        for (nid, node) in &nodes {
            if node.kind == "floor_drain" {
                continue;
            }
            let np = Point2::new(node.pos[0], node.pos[1]);
            for (eid, ekind, ep) in &elec_devices {
                if ekind == "light" {
                    continue; // Ceiling-mounted lights do not clash with wall/floor plumbing nodes
                }
                let dist = np.dist(*ep);
                min_clearance_mm = min_clearance_mm.min(dist);
                let req = if ekind == "panel" { 300.0 } else { 200.0 };
                if dist < req {
                    p6_clashes.push(format!("{nid}<->{eid} ({:.0}mm < {:.0}mm)", dist, req));
                    ir.diagnostics.push(
                        Diagnostic::error(
                            "E_PLUMB_ELEC_CLASH",
                            "coordination",
                            format!(
                                "Cross-discipline MEP clash: plumbing {} '{}' is {:.0}mm from electrical {} '{}' (minimum {:.0}mm required)",
                                node.kind, nid, dist, ekind, eid, req
                            ),
                            node_spans.get(nid).cloned(),
                        )
                        .with_entities(vec![nid.clone(), eid.clone()])
                        .with_suggestion(format!(
                            "Relocate '{}' or '{}' along its host wall by at least {:.0}mm",
                            nid, eid, req - dist + 50.0
                        )),
                    );
                }
            }
        }
        ir.evidence.push(EvidenceRecord {
            check_id: "P6_cross_discipline_clearance".to_string(),
            status: if p6_clashes.is_empty() {
                VerificationStatus::ExactlyChecked
            } else {
                VerificationStatus::Violated
            },
            method: "mep_plumbing_elec_clash_check_v1.4".to_string(),
            tolerance_mm: Some(1.0),
            subjects: nodes.keys().cloned().collect(),
            details: if p6_clashes.is_empty() {
                if elec_devices.is_empty() {
                    "No co-resident electrical devices to clash with".to_string()
                } else {
                    format!(
                        "Zero plumbing<->electrical wall/panel clashes (minimum separation {:.0}mm across {} electrical device(s))",
                        min_clearance_mm,
                        elec_devices.len()
                    )
                }
            } else {
                format!("MEP clashes: {}", p6_clashes.join("; "))
            },
        });

        // P7: Commercial Collector Header DFU Capacity (IPC Table 710.1(1) / EN 12056)
        let drain_headers: Vec<(&String, &HeaderInfo)> = headers
            .iter()
            .filter(|(_, h)| h.system == "soil" || h.system == "waste")
            .collect();
        if !drain_headers.is_empty() {
            let mut p7_errs = Vec::new();
            let mut total_dfu = 0.0;
            for (hid, h) in &drain_headers {
                total_dfu += h.dfu_sum;
                let max_dfu = if h.dia_mm >= 150.0 {
                    480.0
                } else if h.dia_mm >= 100.0 {
                    160.0
                } else if h.dia_mm >= 75.0 {
                    42.0
                } else {
                    12.0
                };
                if h.dfu_sum > max_dfu {
                    p7_errs.push(format!("{hid} {:.0} DFU > {:.0} DFU", h.dfu_sum, max_dfu));
                    ir.diagnostics.push(
                        Diagnostic::error(
                            "E_PLUMB_HEADER_DFU_OVERLOAD",
                            "plumbing",
                            format!(
                                "Collector header '{}' (Ø{:.0}mm) carries {:.1} DFU, exceeding IPC/EN 12056 capacity {:.0} DFU",
                                hid, h.dia_mm, h.dfu_sum, max_dfu
                            ),
                            Some(h.span.clone()),
                        )
                        .with_entities(vec![(*hid).clone()]),
                    );
                }
            }
            ir.evidence.push(EvidenceRecord {
                check_id: "P7_dfu_header_capacity".to_string(),
                status: if p7_errs.is_empty() {
                    VerificationStatus::ExactlyChecked
                } else {
                    VerificationStatus::Violated
                },
                method: "ipc710_dfu_collector_header_capacity_v1.8".to_string(),
                tolerance_mm: None,
                subjects: drain_headers.iter().map(|(id, _)| (*id).clone()).collect(),
                details: if p7_errs.is_empty() {
                    format!(
                        "All {} drainage collector header(s) within IPC/EN 12056 DFU capacity (total {:.0} DFU)",
                        drain_headers.len(),
                        total_dfu
                    )
                } else {
                    format!("Header DFU overloads: {}", p7_errs.join("; "))
                },
            });
        }

        // 5. Emit CanonicalIr entities (full plumbing model + individual nodes for query/diff)
        let model = PlumbingModel {
            name: model_name.clone(),
            standard,
            supply_pressure_bar,
            nodes: nodes.values().cloned().collect(),
            pipes: pipes.clone(),
            connected_fixtures: connected_fixtures.into_iter().collect(),
        };

        ir.entities.push(EntityIr {
            id: model_name.clone(),
            kind: "mep.plumbing.model".to_string(),
            frame: default_frame.clone(),
            props: BTreeMap::from([
                ("node_count".into(), serde_json::json!(model.nodes.len())),
                ("pipe_count".into(), serde_json::json!(model.pipes.len())),
            ]),
            geometry: EntityGeometryIr::Custom {
                shape: "plumbing_model".to_string(),
                data: serde_json::to_value(&model).unwrap_or_default(),
            },
            span: Some(tree.span.clone()),
            provenance: None,
        });

        for n in &model.nodes {
            ir.entities.push(EntityIr {
                id: n.id.clone(),
                kind: format!("mep.plumbing.{}", n.kind),
                frame: default_frame.clone(),
                props: BTreeMap::from([
                    ("system".into(), serde_json::json!(n.system)),
                    ("room".into(), serde_json::json!(n.room)),
                    ("dia_mm".into(), serde_json::json!(n.dia_mm)),
                ]),
                geometry: EntityGeometryIr::Custom {
                    shape: n.kind.clone(),
                    data: serde_json::json!({ "pos": n.pos, "room": n.room, "system": n.system }),
                },
                span: node_spans.get(&n.id).cloned(),
                provenance: None,
            });
        }

        if !ir.project.domain.contains("mep.plumbing") {
            if ir.project.domain.is_empty() || ir.project.domain == "architecture" {
                ir.project.domain = "arch+mep.plumbing".to_string();
            } else {
                ir.project.domain = format!("{}+mep.plumbing", ir.project.domain);
            }
        }
        if ir.diagnostics.iter().any(|d| d.severity == Severity::Error) {
            ir.project.has_errors = true;
        }
    }
}

fn resolve_node_position(
    node: &ExtNode,
    rooms: &BTreeMap<String, RoomInfo>,
    ir: &mut CanonicalIr,
) -> Option<(String, Option<String>, Point2, Option<Side>)> {
    // Form A: `host: Room.wall  at: 600mm`
    if let Some(host) = node.ident("host") {
        let Some((room_id, wall_str)) = host.split_once('.') else {
            ir.diagnostics.push(Diagnostic::error(
                "E_PLUMB_BAD_HOST",
                "plumbing",
                format!("Invalid plumbing host '{host}' — expected `Room.north|south|east|west`"),
                Some(node.span.clone()),
            ));
            ir.project.has_errors = true;
            return None;
        };
        let Some(room) = rooms.get(room_id) else {
            ir.diagnostics.push(Diagnostic::error(
                "E_PLUMB_UNKNOWN_ROOM",
                "plumbing",
                format!("Plumbing node references unknown room '{room_id}'"),
                Some(node.span.clone()),
            ));
            ir.project.has_errors = true;
            return None;
        };
        let at_mm = node.mm("at").unwrap_or(500.0);
        let inset = 110.0; // Sit neatly inside the room face of the wall
        let (pt, facing) = match wall_str {
            "north" => (
                Point2::new((room.rect.min_x + at_mm).clamp(room.rect.min_x + 120.0, room.rect.max_x - 120.0), room.rect.max_y - inset),
                Side::South,
            ),
            "south" => (
                Point2::new((room.rect.min_x + at_mm).clamp(room.rect.min_x + 120.0, room.rect.max_x - 120.0), room.rect.min_y + inset),
                Side::North,
            ),
            "west" => (
                Point2::new(room.rect.min_x + inset, (room.rect.min_y + at_mm).clamp(room.rect.min_y + 120.0, room.rect.max_y - 120.0)),
                Side::East,
            ),
            "east" => (
                Point2::new(room.rect.max_x - inset, (room.rect.min_y + at_mm).clamp(room.rect.min_y + 120.0, room.rect.max_y - 120.0)),
                Side::West,
            ),
            _ => {
                ir.diagnostics.push(Diagnostic::error(
                    "E_PLUMB_BAD_HOST",
                    "plumbing",
                    format!("Unknown wall side '{wall_str}' on '{host}'"),
                    Some(node.span.clone()),
                ));
                ir.project.has_errors = true;
                return None;
            }
        };
        return Some((room_id.to_string(), Some(host.to_string()), pt, Some(facing)));
    }

    // Form B: `in: Room  at: (x, y)` (e.g. floor drains)
    if let Some(room_id) = node.ident("in") {
        let Some(room) = rooms.get(room_id) else {
            ir.diagnostics.push(Diagnostic::error(
                "E_PLUMB_UNKNOWN_ROOM",
                "plumbing",
                format!("Plumbing node references unknown room '{room_id}'"),
                Some(node.span.clone()),
            ));
            ir.project.has_errors = true;
            return None;
        };
        let (dx, dy) = match node.prop("at") {
            Some(ExtValue::Tuple(vals)) if vals.len() == 2 => (
                vals[0].as_mm().unwrap_or(room.rect.width() * 0.5),
                vals[1].as_mm().unwrap_or(room.rect.depth() * 0.5),
            ),
            _ => (room.rect.width() * 0.5, room.rect.depth() * 0.5),
        };
        let pt = Point2::new(
            (room.rect.min_x + dx).clamp(room.rect.min_x + 150.0, room.rect.max_x - 150.0),
            (room.rect.min_y + dy).clamp(room.rect.min_y + 150.0, room.rect.max_y - 150.0),
        );
        return Some((room_id.to_string(), None, pt, None));
    }

    ir.diagnostics.push(Diagnostic::error(
        "E_PLUMB_NO_LOCATION",
        "plumbing",
        format!(
            "Plumbing node '{}' needs `host: Room.wall  at: <dist>` or `in: Room  at: (x, y)`",
            node.name.as_deref().unwrap_or("?")
        ),
        Some(node.span.clone()),
    ));
    ir.project.has_errors = true;
    None
}

/// Anchor plumbing connection at the back (wall-mounted) edge of a wet fixture!
fn fixture_plumbing_anchor(fx: &FixtureInfo) -> Point2 {
    let c = fx.rect.center();
    match fx.against {
        Some(Side::North) => Point2::new(c.x, fx.rect.max_y - 40.0),
        Some(Side::South) => Point2::new(c.x, fx.rect.min_y + 40.0),
        Some(Side::West) => Point2::new(fx.rect.min_x + 40.0, c.y),
        Some(Side::East) => Point2::new(fx.rect.max_x - 40.0, c.y),
        None => c,
    }
}

/// Returns `(needs_cold, needs_hot, needs_drain)` for an architectural fixture.
fn wet_fixture_requirements(subtype: &str, id: &str) -> (bool, bool, bool) {
    let s = format!("{} {}", subtype.to_lowercase(), id.to_lowercase());
    if s.contains("toilet") || s.contains("wc") {
        (true, false, true)
    } else if s.contains("shower")
        || s.contains("tub")
        || s.contains("bath")
        || s.contains("vanity")
        || s.contains("basin")
        || s.contains("sink")
    {
        (true, true, true)
    } else if s.contains("washer") || s.contains("laundry") || s.contains("dishwasher") {
        (true, false, true)
    } else {
        (false, false, false)
    }
}

fn fixture_design_flow_l_s(subtype: &str, is_hot: bool) -> f64 {
    let s = subtype.to_lowercase();
    if s.contains("shower") || s.contains("tub") || s.contains("bath") {
        0.15
    } else if s.contains("sink") || s.contains("kitchen") {
        0.15
    } else if s.contains("toilet") || s.contains("wc") {
        if is_hot { 0.0 } else { 0.13 }
    } else if s.contains("washer") {
        0.20
    } else {
        0.10 // Basin / vanity
    }
}

fn calc_velocity_m_s(flow_l_s: f64, dia_mm: f64) -> f64 {
    // Approximate internal bore (wall thickness ~1.5mm for copper/PEX)
    let id_m = ((dia_mm - 2.5).max(8.0)) / 1000.0;
    let area_m2 = std::f64::consts::PI * 0.25 * id_m * id_m;
    let q_m3_s = flow_l_s / 1000.0;
    q_m3_s / area_m2
}

fn parse_slope_pct(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if let Some((num, den)) = trimmed.split_once(':') {
        let n: f64 = num.trim().parse().ok()?;
        let d: f64 = den.trim().parse().ok()?;
        if d > 0.0 {
            return Some((n / d) * 100.0);
        }
    } else if let Some(p) = trimmed.strip_suffix('%') {
        return p.trim().parse().ok();
    }
    None
}

/// Clean orthogonal pipe routing with system-specific parallel track offsets so
/// Cold Water (`+60mm`), Hot Water (`-60mm`), and Drainage (`0mm` + 45° corner bevel)
/// never visually collide on top of one another!
fn route_orthogonal(
    from: Point2,
    to: Point2,
    system: &str,
    _rooms: &BTreeMap<String, RoomInfo>,
) -> Vec<Point2> {
    let offset = match system {
        "cold" => 55.0,
        "hot" => -55.0,
        _ => 0.0,
    };
    let dx = (to.x - from.x).abs();
    let dy = (to.y - from.y).abs();
    if dx < 20.0 || dy < 20.0 {
        return vec![from, to];
    }

    // Pick L-corner with clean parallel lane offset so CWS & HWS run side-by-side neatly
    if system == "soil" || system == "waste" {
        // Drainage runs with a realistic 45-degree wye bend (industry plumbing drafting convention!)
        let mid = Point2::new(from.x, to.y);
        let bevel = (dx.min(dy) * 0.28).clamp(80.0, 260.0);
        let sy = if to.y > from.y { 1.0 } else { -1.0 };
        let sx = if to.x > from.x { 1.0 } else { -1.0 };
        let p1 = Point2::new(mid.x, mid.y - sy * bevel);
        let p2 = Point2::new(mid.x + sx * bevel, mid.y);
        vec![from, p1, p2, to]
    } else {
        // Supply runs: route horizontally then vertically (or vice-versa) with clean offset lane
        let elbow_y = if dy > 220.0 {
            to.y + offset
        } else {
            to.y
        };
        let elbow_x = if dx > 220.0 {
            from.x + offset
        } else {
            from.x
        };
        if offset.abs() > 1.0 && dx > 260.0 && dy > 260.0 {
            vec![
                from,
                Point2::new(elbow_x, from.y),
                Point2::new(elbow_x, elbow_y),
                Point2::new(to.x, elbow_y),
                to,
            ]
        } else {
            vec![from, Point2::new(to.x, from.y), to]
        }
    }
}

fn polyline_len(pts: &[Point2]) -> f64 {
    pts.windows(2).map(|w| w[0].dist(w[1])).sum()
}

fn fixture_dfu(subtype: &str, id: &str) -> f64 {
    let s = format!("{} {}", subtype.to_lowercase(), id.to_lowercase());
    if s.contains("toilet") || s.contains("wc") {
        4.0
    } else if s.contains("shower") || s.contains("tub") || s.contains("bath") || s.contains("sink") || s.contains("washer") {
        2.0
    } else {
        1.0 // Lavatory / basin / vanity
    }
}

fn project_to_segment(p: Point2, a: Point2, b: Point2) -> Point2 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let len2 = abx * abx + aby * aby;
    if len2 < 1e-6 {
        return a;
    }
    let t = (((p.x - a.x) * abx + (p.y - a.y) * aby) / len2).clamp(0.05, 0.95);
    Point2::new(a.x + t * abx, a.y + t * aby)
}
