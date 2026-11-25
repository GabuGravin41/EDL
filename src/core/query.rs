use crate::core::geom::{Rect2, EPS_MM};
use crate::core::ir::{CanonicalIr, EntityGeometryIr, EntityIr, VerificationStatus};
use crate::core::span::Severity;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Compact, token-efficient structural overview of any EDL project for AI agents and CLI `edl inspect`.
pub fn inspect_summary(ir: &CanonicalIr) -> Value {
    let mut modules_used: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut entities_compact = Vec::new();

    for e in &ir.entities {
        if let Some(ref p) = e.provenance {
            modules_used
                .entry(p.module.clone())
                .or_default()
                .insert(p.instance.clone());
        }
        let bbox = entity_bbox(e);
        let mut entry = json!({
            "id": e.id,
            "kind": e.kind,
        });
        if let Some(ref p) = e.provenance {
            entry["module"] = json!(p.module);
            entry["instance"] = json!(p.instance);
        }
        if let Some(r) = bbox {
            entry["bounds_mm"] = json!({
                "min_x": round2(r.min_x),
                "min_y": round2(r.min_y),
                "max_x": round2(r.max_x),
                "max_y": round2(r.max_y),
                "width": round2(r.width()),
                "depth": round2(r.depth()),
            });
        }
        if let EntityGeometryIr::Rect {
            area_m2,
            notch_rect,
            ..
        } = &e.geometry
        {
            entry["area_m2"] = json!(round2(*area_m2));
            if notch_rect.is_some() {
                entry["shape"] = json!("notched_l");
            }
        }
        if let EntityGeometryIr::Component {
            category,
            value,
            pins,
            ..
        } = &e.geometry
        {
            entry["category"] = json!(category);
            entry["value"] = json!(value);
            entry["pins"] = json!(pins.iter().map(|p| p.name.as_str()).collect::<Vec<_>>());
        }
        entities_compact.push(entry);
    }

    let nets_compact: Vec<Value> = ir
        .relations
        .iter()
        .map(|rel| {
            let route = ir.schematic_routes.iter().find(|r| r.net_id == rel.id);
            let wire_len_mm: f64 = route
                .map(|r| r.segments.iter().map(|s| s.length()).sum())
                .unwrap_or(0.0);
            json!({
                "id": rel.id,
                "pins": rel.connects,
                "is_ground": rel.is_ground,
                "driven_voltage_v": rel.driven_voltage_v,
                "routing_style": route.map(|r| r.style.as_str()).unwrap_or("none"),
                "wire_length_mm": round2(wire_len_mm),
            })
        })
        .collect();

    let evidence_compact: Vec<Value> = ir
        .evidence
        .iter()
        .map(|ev| {
            json!({
                "check_id": ev.check_id,
                "status": ev.status,
                "details": ev.details,
            })
        })
        .collect();

    let violations_count = ir
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();

    let is_elec = ir.project.domain == "electronics";
    let mut project_obj = json!({
        "name": ir.project.name,
        "domain": ir.project.domain,
        "has_errors": ir.project.has_errors,
        "error_count": violations_count,
        "exterior_footprint_mm": ir.project.exterior_footprint_mm,
    });
    if is_elec {
        project_obj["counts"] = json!({
            "components": ir.project.component_count,
            "nets": ir.project.net_count,
        });
    } else {
        project_obj["total_clear_area_m2"] = json!(round2(ir.project.total_clear_area_m2));
        project_obj["remaining_dof"] = json!(ir.project.remaining_dof);
        project_obj["counts"] = json!({
            "rooms": ir.project.room_count,
            "openings": ir.project.opening_count,
            "fixtures": ir.project.fixture_count,
        });
    }

    let mut out = json!({
        "project": project_obj,
        "modules": modules_used
            .into_iter()
            .map(|(m, insts)| (m, insts.into_iter().collect::<Vec<_>>()))
            .collect::<BTreeMap<_, _>>(),
        "entities": entities_compact,
        "evidence": evidence_compact,
    });
    if is_elec {
        out["nets"] = json!(nets_compact);
    }
    out
}

/// Targeted semantic queries over Canonical IR (`summary`, `entity`, `net`, `room`, `module`, `violations`).
pub fn query_ir(ir: &CanonicalIr, kind: &str, target: Option<&str>) -> Result<Value, String> {
    match kind {
        "summary" | "inspect" => Ok(inspect_summary(ir)),

        "violations" | "errors" => {
            let failing_evidence: Vec<Value> = ir
                .evidence
                .iter()
                .filter(|e| e.status == VerificationStatus::Violated)
                .map(|e| {
                    json!({
                        "check_id": e.check_id,
                        "status": e.status,
                        "method": e.method,
                        "subjects": e.subjects,
                        "details": e.details,
                    })
                })
                .collect();

            let repair_plan: Vec<Value> = ir
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(|d| {
                    json!({
                        "code": d.code,
                        "category": d.category,
                        "message": d.message,
                        "entities": d.entities,
                        "span": d.span,
                        "measured": d.measured,
                        "required": d.required,
                        "unit": d.unit,
                        "repair": d.inferred_repair(),
                    })
                })
                .collect();

            Ok(json!({
                "project": ir.project.name,
                "has_errors": ir.project.has_errors,
                "violation_count": repair_plan.len(),
                "failing_evidence": failing_evidence,
                "repair_plan": repair_plan,
            }))
        }

        "entity" => {
            let id = target.ok_or_else(|| "query 'entity' requires a target entity ID".to_string())?;
            let ent = ir
                .entities
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| format!("Entity '{id}' not found in project '{}'", ir.project.name))?;

            let connected_nets: Vec<String> = ir
                .relations
                .iter()
                .filter(|r| r.connects.iter().any(|p| p.starts_with(&format!("{id}."))))
                .map(|r| r.id.clone())
                .collect();

            let related_diags: Vec<Value> = ir
                .diagnostics
                .iter()
                .filter(|d| d.entities.iter().any(|x| x == id || x.starts_with(&format!("{id}."))))
                .map(|d| {
                    json!({
                        "code": d.code,
                        "severity": d.severity,
                        "message": d.message,
                        "repair": d.inferred_repair(),
                    })
                })
                .collect();

            Ok(json!({
                "entity": ent,
                "connected_nets": connected_nets,
                "diagnostics": related_diags,
            }))
        }

        "net" => {
            let name = target.ok_or_else(|| "query 'net' requires a target net name".to_string())?;
            let rel = ir
                .relations
                .iter()
                .find(|r| r.id == name)
                .ok_or_else(|| format!("Net '{name}' not found in project '{}'", ir.project.name))?;
            let route = ir.schematic_routes.iter().find(|r| r.net_id == name);
            let wire_len_mm: f64 = route
                .map(|r| r.segments.iter().map(|s| s.length()).sum())
                .unwrap_or(0.0);

            let related_diags: Vec<Value> = ir
                .diagnostics
                .iter()
                .filter(|d| d.entities.iter().any(|x| x == name))
                .map(|d| json!({ "code": d.code, "message": d.message, "repair": d.inferred_repair() }))
                .collect();

            Ok(json!({
                "net": rel,
                "routing_style": route.map(|r| r.style.as_str()).unwrap_or("none"),
                "wire_length_mm": round2(wire_len_mm),
                "segment_count": route.map(|r| r.segments.len()).unwrap_or(0),
                "junction_count": route.map(|r| r.junctions.len()).unwrap_or(0),
                "terminal_count": route.map(|r| r.terminals.len()).unwrap_or(0),
                "diagnostics": related_diags,
            }))
        }

        "room" => {
            let id = target.ok_or_else(|| "query 'room' requires a target room ID".to_string())?;
            let room = ir
                .entities
                .iter()
                .find(|e| e.kind == "arch.room" && e.id == id)
                .ok_or_else(|| format!("Room '{id}' not found in project '{}'", ir.project.name))?;

            let adjacent_rooms: Vec<Value> = ir
                .derived_walls
                .iter()
                .filter(|w| w.kind == "arch.wall.interior" && w.derived_from.iter().any(|r| r == id))
                .map(|w| {
                    let other = w
                        .derived_from
                        .iter()
                        .find(|r| r.as_str() != id)
                        .cloned()
                        .unwrap_or_default();
                    let axis = if (w.clear_segment.p0.y - w.clear_segment.p1.y).abs() <= EPS_MM {
                        "horizontal"
                    } else {
                        "vertical"
                    };
                    json!({
                        "room": other,
                        "wall_id": w.id,
                        "axis": axis,
                        "shared_length_mm": round2(w.clear_length_mm),
                        "thickness_mm": round2(w.thickness_mm),
                    })
                })
                .collect();

            // Match openings hosted on any derived wall whose `derived_from` includes this room ID
            let room_wall_ids: BTreeSet<&str> = ir
                .derived_walls
                .iter()
                .filter(|w| w.derived_from.iter().any(|r| r == id))
                .map(|w| w.id.as_str())
                .collect();

            let hosted_openings: Vec<String> = ir
                .entities
                .iter()
                .filter(|e| {
                    matches!(e.kind.as_str(), "arch.door" | "arch.window" | "arch.passage")
                        && e.props
                            .get("host_wall")
                            .and_then(|v| v.as_str())
                            .map(|hw| room_wall_ids.contains(hw))
                            .unwrap_or(false)
                })
                .map(|e| e.id.clone())
                .collect();

            let fixtures_inside: Vec<String> = ir
                .entities
                .iter()
                .filter(|e| {
                    e.kind == "arch.fixture"
                        && e.props.get("room").and_then(|v| v.as_str()) == Some(id)
                })
                .map(|e| e.id.clone())
                .collect();

            Ok(json!({
                "room": room,
                "adjacent_rooms": adjacent_rooms,
                "hosted_openings": hosted_openings,
                "fixtures": fixtures_inside,
            }))
        }

        "module" => {
            let mname = target.ok_or_else(|| "query 'module' requires a target module name".to_string())?;
            let mut instances: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for e in &ir.entities {
                if let Some(ref p) = e.provenance {
                    if p.module == mname {
                        instances
                            .entry(p.instance.clone())
                            .or_default()
                            .push(e.id.clone());
                    }
                }
            }
            if instances.is_empty() {
                return Err(format!("No instances of module '{mname}' found in '{}'", ir.project.name));
            }
            Ok(json!({
                "module": mname,
                "instance_count": instances.len(),
                "instances": instances,
            }))
        }

        other => Err(format!(
            "Unknown query kind '{other}' (expected: summary, entity, net, room, module, or violations)"
        )),
    }
}

/// Exact analytical measurements for a single entity/net OR between any two entities.
pub fn measure_ir(ir: &CanonicalIr, target_a: &str, target_b: Option<&str>) -> Result<Value, String> {
    if target_b.is_none() {
        // Check if target_a is a schematic net
        if let Some(route) = ir.schematic_routes.iter().find(|r| r.net_id == target_a) {
            let total_mm: f64 = route.segments.iter().map(|s| s.length()).sum();
            let rel = ir.relations.iter().find(|r| r.id == target_a);
            return Ok(json!({
                "kind": "net_route",
                "net": target_a,
                "style": route.style,
                "total_wire_length_mm": round2(total_mm),
                "segment_count": route.segments.len(),
                "junction_count": route.junctions.len(),
                "terminal_count": route.terminals.len(),
                "pin_count": rel.map(|r| r.connects.len()).unwrap_or(0),
            }));
        }

        // Otherwise measure single entity
        let ea = ir
            .entities
            .iter()
            .find(|e| e.id == target_a)
            .ok_or_else(|| format!("Entity or net '{target_a}' not found"))?;
        let bbox = entity_bbox(ea).ok_or_else(|| format!("Entity '{target_a}' has no bounding box"))?;
        let mut res = json!({
            "kind": "entity",
            "id": ea.id,
            "entity_kind": ea.kind,
            "width_mm": round2(bbox.width()),
            "depth_mm": round2(bbox.depth()),
            "perimeter_mm": round2(2.0 * (bbox.width() + bbox.depth())),
            "bbox_area_m2": round2(bbox.area_m2()),
            "center_mm": [round2(bbox.center().x), round2(bbox.center().y)],
        });
        if let EntityGeometryIr::Rect { area_m2, .. } = &ea.geometry {
            res["clear_area_m2"] = json!(round2(*area_m2));
        }
        return Ok(res);
    }

    let tb = target_b.unwrap();
    let ea = ir
        .entities
        .iter()
        .find(|e| e.id == target_a)
        .ok_or_else(|| format!("Entity '{target_a}' not found"))?;
    let eb = ir
        .entities
        .iter()
        .find(|e| e.id == tb)
        .ok_or_else(|| format!("Entity '{tb}' not found"))?;

    let ba = entity_bbox(ea).ok_or_else(|| format!("Entity '{target_a}' has no geometry"))?;
    let bb = entity_bbox(eb).ok_or_else(|| format!("Entity '{tb}' has no geometry"))?;
    let subs_a = entity_subrects(ea);
    let subs_b = entity_subrects(eb);

    let ca = ba.center();
    let cb = bb.center();
    let dx = cb.x - ca.x;
    let dy = cb.y - ca.y;
    let center_dist = (dx * dx + dy * dy).sqrt();

    // Compute exact minimum clear gap across decomposed sub-rectangles (handles L-shaped notched rooms!)
    let mut best_gap = f64::INFINITY;
    let mut best_gx = 0.0_f64;
    let mut best_gy = 0.0_f64;
    let mut any_overlap = false;
    for &ra in &subs_a {
        for &rb in &subs_b {
            if ra.overlaps_strict(rb) {
                any_overlap = true;
            }
            let gx = (rb.min_x - ra.max_x).max(ra.min_x - rb.max_x).max(0.0);
            let gy = (rb.min_y - ra.max_y).max(ra.min_y - rb.max_y).max(0.0);
            let g = (gx * gx + gy * gy).sqrt();
            if g < best_gap {
                best_gap = g;
                best_gx = gx;
                best_gy = gy;
            }
        }
    }
    let (clear_gap, gap_x, gap_y) = if best_gap.is_finite() {
        (best_gap, best_gx, best_gy)
    } else {
        (0.0, 0.0, 0.0)
    };

    let shared_walls: Vec<Value> = ir
        .derived_walls
        .iter()
        .filter(|w| {
            w.kind == "arch.wall.interior"
                && w.derived_from.iter().any(|r| r == target_a)
                && w.derived_from.iter().any(|r| r == tb)
        })
        .map(|w| {
            let axis = if (w.clear_segment.p0.y - w.clear_segment.p1.y).abs() <= EPS_MM {
                "horizontal"
            } else {
                "vertical"
            };
            json!({
                "wall_id": w.id,
                "axis": axis,
                "clear_length_mm": round2(w.clear_length_mm),
                "thickness_mm": round2(w.thickness_mm),
            })
        })
        .collect();

    let shared_nets: Vec<String> = ir
        .relations
        .iter()
        .filter(|r| {
            r.connects.iter().any(|p| p.starts_with(&format!("{target_a}.")))
                && r.connects.iter().any(|p| p.starts_with(&format!("{tb}.")))
        })
        .map(|r| r.id.clone())
        .collect();

    let mut out = json!({
        "kind": "pair",
        "target_a": target_a,
        "target_b": tb,
        "center_distance_mm": round2(center_dist),
        "delta_mm": [round2(dx), round2(dy)],
        "clear_gap_mm": round2(clear_gap),
        "gap_x_mm": round2(gap_x),
        "gap_y_mm": round2(gap_y),
        "bboxes_overlap": any_overlap,
    });
    if ir.project.domain == "electronics" {
        out["shared_nets"] = json!(shared_nets);
    } else {
        out["shared_walls"] = json!(shared_walls);
    }
    Ok(out)
}

fn entity_subrects(e: &EntityIr) -> Vec<Rect2> {
    match &e.geometry {
        EntityGeometryIr::Rect {
            min,
            max,
            notch_rect,
            ..
        } => crate::domains::arch::compile::decompose_room_rect(
            Rect2::new(min[0], min[1], max[0], max[1]),
            *notch_rect,
        ),
        _ => entity_bbox(e).into_iter().collect(),
    }
}

fn entity_bbox(e: &EntityIr) -> Option<Rect2> {
    match &e.geometry {
        EntityGeometryIr::Rect { min, max, .. } => Some(Rect2::new(min[0], min[1], max[0], max[1])),
        EntityGeometryIr::Door { cutout_rect, .. }
        | EntityGeometryIr::Window { cutout_rect, .. }
        | EntityGeometryIr::Passage { cutout_rect, .. } => Some(*cutout_rect),
        EntityGeometryIr::Fixture { body_rect, .. }
        | EntityGeometryIr::Component { body_rect, .. } => Some(*body_rect),
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
