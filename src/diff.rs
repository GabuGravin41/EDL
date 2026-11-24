use crate::ir::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityChange {
    pub id: String,
    pub kind: String,
    pub change: String, // "added" | "removed" | "modified"
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationChange {
    pub check_id: String,
    pub before: Option<VerificationStatus>,
    pub after: Option<VerificationStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDiff {
    pub project_a: String,
    pub project_b: String,
    pub footprint_before_mm: Option<[f64; 2]>,
    pub footprint_after_mm: Option<[f64; 2]>,
    pub area_before_m2: f64,
    pub area_after_m2: f64,
    pub unchanged_entities: Vec<String>,
    pub changed_entities: Vec<EntityChange>,
    pub verification_changes: Vec<VerificationChange>,
}

pub fn diff_ir(a: &CanonicalIr, b: &CanonicalIr) -> SemanticDiff {
    let map_a: BTreeMap<&str, &EntityIr> = a.entities.iter().map(|e| (e.id.as_str(), e)).collect();
    let map_b: BTreeMap<&str, &EntityIr> = b.entities.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut unchanged_entities = Vec::new();
    let mut changed_entities = Vec::new();

    for (&id, &ea) in &map_a {
        match map_b.get(id) {
            None => changed_entities.push(EntityChange {
                id: id.to_string(),
                kind: ea.kind.clone(),
                change: "removed".to_string(),
                details: format!("Removed entity '{id}'"),
            }),
            Some(&eb) => {
                let ga = serde_json::to_value(&ea.geometry).unwrap();
                let gb = serde_json::to_value(&eb.geometry).unwrap();
                if ga == gb {
                    unchanged_entities.push(id.to_string());
                } else {
                    changed_entities.push(EntityChange {
                        id: id.to_string(),
                        kind: eb.kind.clone(),
                        change: "modified".to_string(),
                        details: describe_entity_delta(ea, eb),
                    });
                }
            }
        }
    }

    for (&id, &eb) in &map_b {
        if !map_a.contains_key(id) {
            changed_entities.push(EntityChange {
                id: id.to_string(),
                kind: eb.kind.clone(),
                change: "added".to_string(),
                details: format!("Added entity '{id}'"),
            });
        }
    }

    let ev_a: BTreeMap<&str, VerificationStatus> = a
        .evidence
        .iter()
        .map(|e| (e.check_id.as_str(), e.status))
        .collect();
    let ev_b: BTreeMap<&str, VerificationStatus> = b
        .evidence
        .iter()
        .map(|e| (e.check_id.as_str(), e.status))
        .collect();

    let mut verification_changes = Vec::new();
    for (&cid, &sa) in &ev_a {
        let sb = ev_b.get(cid).copied();
        if sb != Some(sa) {
            verification_changes.push(VerificationChange {
                check_id: cid.to_string(),
                before: Some(sa),
                after: sb,
            });
        }
    }
    for (&cid, &sb) in &ev_b {
        if !ev_a.contains_key(cid) {
            verification_changes.push(VerificationChange {
                check_id: cid.to_string(),
                before: None,
                after: Some(sb),
            });
        }
    }

    SemanticDiff {
        project_a: a.project.name.clone(),
        project_b: b.project.name.clone(),
        footprint_before_mm: a.project.exterior_footprint_mm,
        footprint_after_mm: b.project.exterior_footprint_mm,
        area_before_m2: a.project.total_clear_area_m2,
        area_after_m2: b.project.total_clear_area_m2,
        unchanged_entities,
        changed_entities,
        verification_changes,
    }
}

fn describe_entity_delta(a: &EntityIr, b: &EntityIr) -> String {
    match (&a.geometry, &b.geometry) {
        (
            EntityGeometryIr::Rect {
                min: min_a,
                width_mm: wa,
                depth_mm: da,
                ..
            },
            EntityGeometryIr::Rect {
                min: min_b,
                width_mm: wb,
                depth_mm: db,
                ..
            },
        ) => {
            let dx = min_b[0] - min_a[0];
            let dy = min_b[1] - min_a[1];
            let dw = wb - wa;
            let dd = db - da;
            let mut parts = Vec::new();
            if dx.abs() > 1e-4 || dy.abs() > 1e-4 {
                parts.push(format!("shifted ({dx:+.0}mm, {dy:+.0}mm)"));
            }
            if dw.abs() > 1e-4 || dd.abs() > 1e-4 {
                parts.push(format!("size {wa:.0}x{da:.0}mm -> {wb:.0}x{db:.0}mm"));
            }
            if parts.is_empty() {
                "properties changed".to_string()
            } else {
                parts.join("; ")
            }
        }
        (
            EntityGeometryIr::Window {
                opening_segment: sa,
                ..
            },
            EntityGeometryIr::Window {
                opening_segment: sb,
                ..
            },
        ) => {
            let dx = sb.p0.x - sa.p0.x;
            let dy = sb.p0.y - sa.p0.y;
            format!(
                "width {:.0}mm -> {:.0}mm; start shifted ({dx:+.0}mm, {dy:+.0}mm)",
                sa.length(),
                sb.length()
            )
        }
        (
            EntityGeometryIr::Door { swing_arc: aa, .. },
            EntityGeometryIr::Door { swing_arc: ab, .. },
        ) => {
            let dx = ab.center.x - aa.center.x;
            let dy = ab.center.y - aa.center.y;
            format!(
                "hinge shifted ({dx:+.0}mm, {dy:+.0}mm); radius {:.0}mm -> {:.0}mm",
                aa.radius, ab.radius
            )
        }
        (
            EntityGeometryIr::Fixture { body_rect: ra, .. },
            EntityGeometryIr::Fixture { body_rect: rb, .. },
        ) => {
            let dx = rb.min_x - ra.min_x;
            let dy = rb.min_y - ra.min_y;
            format!(
                "fixture shifted ({dx:+.0}mm, {dy:+.0}mm); size {:.0}x{:.0}mm -> {:.0}x{:.0}mm",
                ra.width(),
                ra.depth(),
                rb.width(),
                rb.depth()
            )
        }
        _ => "geometry updated".to_string(),
    }
}
