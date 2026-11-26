//! Pin-anchored relative spatial constraint solver for AI-native coordinate-free schematic layout (v3.0).
//!
//! Solves `left_of`, `right_of`, `above`, `below`, `offset`, `at_angle`/`dist`, `align_y`, `align_x`,
//! and `array` constraints directly against real `.kicad_sym` pin tip coordinates on the 2.54mm KiCad
//! grid, with:
//! - Automatic pin-outward-side awareness (never places a satellite *into* its anchor IC's body!)
//! - Topological dependency ordering + cycle detection (`E_LAYOUT_CYCLE`)
//! - Guaranteed collision-free push-apart (including RefDes/Value/power-symbol breathing room)
//! - Inter-group horizontal repack so expanded satellite groups never invade neighboring zone frames

use crate::core::geom::{Point2, Rect2, Side};
use crate::core::ir::{EntityGeometryIr, EntityIr, PlacedPinIr, SchematicGroupIr, SymPrim};
use crate::core::span::Diagnostic;
use crate::domains::elec::ast::{AnchorRef, RelDir, RelativePlacementDecl, RelativePlacementKind};
use std::collections::{BTreeMap, BTreeSet};

const GRID_MM: f64 = 2.54;
const PIN_PAD_MM: f64 = 5.08;

fn snap(v: f64) -> f64 {
    (v / GRID_MM).round() * GRID_MM
}

/// Shift a placed `EntityIr::Component` by `(dx, dy)` while keeping every pin tip and symbol primitive exact.
pub fn translate_entity(ent: &mut EntityIr, dx: f64, dy: f64) {
    if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
        return;
    }
    if let EntityGeometryIr::Component {
        ref mut body_rect,
        ref mut pins,
        ref mut symbol_prims,
        ..
    } = ent.geometry
    {
        body_rect.min_x += dx;
        body_rect.max_x += dx;
        body_rect.min_y += dy;
        body_rect.max_y += dy;
        for p in pins.iter_mut() {
            p.body_point.x += dx;
            p.body_point.y += dy;
            p.tip_point.x += dx;
            p.tip_point.y += dy;
        }
        if let Some(prims) = symbol_prims.as_mut() {
            for pr in prims.iter_mut() {
                match pr {
                    SymPrim::Polyline { pts, .. } => {
                        for pt in pts.iter_mut() {
                            pt.x += dx;
                            pt.y += dy;
                        }
                    }
                    SymPrim::Rect { rect, .. } => {
                        rect.min_x += dx;
                        rect.max_x += dx;
                        rect.min_y += dy;
                        rect.max_y += dy;
                    }
                    SymPrim::Circle { center, .. } => {
                        center.x += dx;
                        center.y += dy;
                    }
                    SymPrim::Mark { pos, .. } => {
                        pos.x += dx;
                        pos.y += dy;
                    }
                }
            }
        }
    }
}

fn entity_body_and_pins(ent: &EntityIr) -> Option<(Rect2, &[PlacedPinIr])> {
    match &ent.geometry {
        EntityGeometryIr::Component {
            body_rect, pins, ..
        } => Some((*body_rect, pins.as_slice())),
        _ => None,
    }
}

fn find_pin<'a>(pins: &'a [PlacedPinIr], key: &str) -> Option<&'a PlacedPinIr> {
    pins.iter().find(|p| {
        p.name == key
            || p.pin_code.as_deref() == Some(key)
            || p.number.to_string() == key
            || p.name.eq_ignore_ascii_case(key)
    })
}

fn pick_facing_pin<'a>(
    pins: &'a [PlacedPinIr],
    dir: RelDir,
    explicit: Option<&str>,
) -> Option<&'a PlacedPinIr> {
    if let Some(k) = explicit {
        if let Some(p) = find_pin(pins, k) {
            return Some(p);
        }
    }
    match dir {
        RelDir::LeftOf => pins
            .iter()
            .max_by(|a, b| a.tip_point.x.partial_cmp(&b.tip_point.x).unwrap()),
        RelDir::RightOf => pins
            .iter()
            .min_by(|a, b| a.tip_point.x.partial_cmp(&b.tip_point.x).unwrap()),
        RelDir::Below => pins
            .iter()
            .min_by(|a, b| a.tip_point.y.partial_cmp(&b.tip_point.y).unwrap()),
        RelDir::Above => pins
            .iter()
            .max_by(|a, b| a.tip_point.y.partial_cmp(&b.tip_point.y).unwrap()),
    }
}

fn resolve_anchor(
    entities: &[EntityIr],
    idx_map: &BTreeMap<String, usize>,
    anchor: &AnchorRef,
) -> Option<(Point2, Rect2, Option<Side>, bool)> {
    let &idx = idx_map.get(&anchor.component)?;
    let (body, pins) = entity_body_and_pins(&entities[idx])?;
    let is_multi_pin_ic = pins.len() >= 3;
    if let Some(ref pname) = anchor.pin {
        if let Some(p) = find_pin(pins, pname) {
            return Some((p.tip_point, body, Some(p.side), is_multi_pin_ic));
        }
    }
    Some((body.center(), body, None, is_multi_pin_ic))
}

/// Padded clearance envelope around a component (covers body + pins + power/GND symbols + RefDes/Value text).
fn full_envelope(ent: &EntityIr) -> Rect2 {
    let Some((body, pins)) = entity_body_and_pins(ent) else {
        return Rect2::new(0.0, 0.0, 0.0, 0.0);
    };
    let mut r = body;
    for p in pins {
        r.min_x = r.min_x.min(p.tip_point.x);
        r.max_x = r.max_x.max(p.tip_point.x);
        r.min_y = r.min_y.min(p.tip_point.y);
        r.max_y = r.max_y.max(p.tip_point.y);
    }
    // Tight pin-tip + RefDes clearance (does not falsely trigger on intentional 5.08mm stacked chains like dividers!)
    let (px_l, px_r, py_t, py_b) = if pins.len() <= 2 {
        (2.54, 5.08, 2.0, 2.0)
    } else {
        (3.81, 3.81, 3.81, 3.81)
    };
    Rect2::new(r.min_x - px_l, r.min_y - py_t, r.max_x + px_r, r.max_y + py_b)
}

fn rects_overlap(a: Rect2, b: Rect2) -> bool {
    a.min_x < b.max_x - 0.1
        && a.max_x > b.min_x + 0.1
        && a.min_y < b.max_y - 0.1
        && a.max_y > b.min_y + 0.1
}

/// If a directional hint would push a satellite *into* a multi-pin IC's body (e.g. `Below` a North pin,
/// `LeftOf` an East pin, `Above` an East pin), redirect it cleanly along the pin's outward normal!
fn outward_safe_dir(requested: RelDir, pin_side: Option<Side>, anchor_is_ic: bool) -> RelDir {
    if !anchor_is_ic {
        return requested;
    }
    let Some(ps) = pin_side else {
        return requested;
    };
    match (requested, ps) {
        // Never go inward through the IC body!
        (RelDir::Below, Side::North) => RelDir::Above,
        (RelDir::Above, Side::South) => RelDir::Below,
        (RelDir::LeftOf, Side::East) => RelDir::RightOf,
        (RelDir::RightOf, Side::West) => RelDir::LeftOf,
        // Vertical pullup/decouple attached to horizontal East/West IC pins -> place cleanly outboard!
        (RelDir::Above | RelDir::Below, Side::East) => RelDir::RightOf,
        (RelDir::Above | RelDir::Below, Side::West) => RelDir::LeftOf,
        _ => requested,
    }
}

/// Apply all relative placement constraints (`left_of`, `right_of`, `above`, `below`, `offset`,
/// `at_angle`/`dist`, `align_y`, `align_x`, `array`) to `entities`, resolve any collisions cleanly,
/// and repack groups horizontally so expanded zones never overlap!
pub fn apply_relative_placements(
    entities: &mut [EntityIr],
    groups: &mut [SchematicGroupIr],
    constraints: &[RelativePlacementDecl],
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    let mut moved_sats: BTreeSet<String> = BTreeSet::new();
    if constraints.is_empty() || entities.is_empty() {
        return moved_sats;
    }

    let idx_map: BTreeMap<String, usize> = entities
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i))
        .collect();

    let active: Vec<&RelativePlacementDecl> = constraints
        .iter()
        .filter(|c| match &c.kind {
            RelativePlacementKind::Array { members, .. } => {
                members.iter().any(|m| idx_map.contains_key(m))
            }
            _ => idx_map.contains_key(&c.component),
        })
        .collect();
    if active.is_empty() {
        return moved_sats;
    }

    // 1. Cycle detection
    let mut dep_graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in &active {
        let dep = match &c.kind {
            RelativePlacementKind::Directional { anchor, .. }
            | RelativePlacementKind::Offset { anchor, .. }
            | RelativePlacementKind::Polar { anchor, .. }
            | RelativePlacementKind::AlignAxis { anchor, .. } => Some(anchor.component.clone()),
            RelativePlacementKind::Array { .. } => None,
        };
        if let Some(d) = dep {
            if d != c.component {
                dep_graph.entry(c.component.clone()).or_default().push(d);
            }
        }
    }
    let mut state: BTreeMap<String, u8> = BTreeMap::new();
    fn has_cycle(
        node: &str,
        g: &BTreeMap<String, Vec<String>>,
        st: &mut BTreeMap<String, u8>,
    ) -> bool {
        if st.get(node) == Some(&1) {
            return true;
        }
        if st.get(node) == Some(&2) {
            return false;
        }
        st.insert(node.to_string(), 1);
        if let Some(nbrs) = g.get(node) {
            for n in nbrs {
                if has_cycle(n, g, st) {
                    return true;
                }
            }
        }
        st.insert(node.to_string(), 2);
        false
    }
    let mut cyclic_nodes: BTreeSet<String> = BTreeSet::new();
    for c in &active {
        if !cyclic_nodes.contains(&c.component) && has_cycle(&c.component, &dep_graph, &mut state) {
            cyclic_nodes.insert(c.component.clone());
            diagnostics.push(
                Diagnostic::error(
                    "E_LAYOUT_CYCLE",
                    "layout",
                    format!(
                        "Cyclic relative placement dependency involving component '{}'",
                        c.component
                    ),
                    Some(c.span.clone()),
                )
                .with_entities(vec![c.component.clone()]),
            );
        }
    }

    // Track primary push direction ONLY for auto-generated idiom satellites (`is_auto == true`).
    // Explicit user/AI `place` statements (`is_auto == false`) are obeyed literally and verified
    // strictly by E11 (sheet frame) & E12 (component overlap) — never silently hijacked!
    let mut sat_push_dir: BTreeMap<String, RelDir> = BTreeMap::new();
    let mut auto_sats: BTreeSet<String> = BTreeSet::new();
    let explicit_sats: BTreeSet<String> = active
        .iter()
        .filter(|c| !c.is_auto)
        .map(|c| c.component.clone())
        .collect();

    // 2. Multi-pass topological resolution
    for _pass in 0..8 {
        let mut any_moved = false;
        for c in &active {
            if cyclic_nodes.contains(&c.component) {
                continue;
            }
            match &c.kind {
                RelativePlacementKind::Directional {
                    dir,
                    anchor,
                    gap,
                    align_pin,
                } => {
                    let Some(&my_idx) = idx_map.get(&c.component) else {
                        continue;
                    };
                    let Some((anchor_pt, anchor_body, anchor_pin_side, anchor_is_ic)) =
                        resolve_anchor(entities, &idx_map, anchor)
                    else {
                        continue;
                    };
                    let eff_dir = if c.is_auto {
                        outward_safe_dir(*dir, anchor_pin_side, anchor_is_ic)
                    } else {
                        *dir
                    };
                    let gap_mm = snap(gap.and_then(|q| q.to_mm().ok()).unwrap_or(7.62)).max(2.54);
                    let Some((my_body, my_pins)) = entity_body_and_pins(&entities[my_idx]) else {
                        continue;
                    };

                    let (dx, dy) = if anchor.pin.is_some() {
                        // If we redirected `Above/Below` to `LeftOf/RightOf` for an auto idiom on an East/West IC pin,
                        // match the target pin (`align_pin` if specified, else facing pin) cleanly!
                        let use_pin = if eff_dir != *dir {
                            align_pin
                                .as_deref()
                                .and_then(|k| find_pin(my_pins, k))
                                .or_else(|| pick_facing_pin(my_pins, eff_dir, None))
                        } else {
                            pick_facing_pin(my_pins, eff_dir, align_pin.as_deref())
                        };
                        if let Some(mp) = use_pin {
                            let (tx, ty) = match eff_dir {
                                RelDir::LeftOf => (anchor_pt.x - gap_mm, anchor_pt.y),
                                RelDir::RightOf => (anchor_pt.x + gap_mm, anchor_pt.y),
                                RelDir::Below => (anchor_pt.x, anchor_pt.y + gap_mm),
                                RelDir::Above => (anchor_pt.x, anchor_pt.y - gap_mm),
                            };
                            let mut sdx = snap(tx - mp.tip_point.x);
                            let mut sdy = snap(ty - mp.tip_point.y);
                            if c.is_auto {
                                if eff_dir == RelDir::LeftOf && my_body.max_x + sdx > anchor_body.min_x - 7.62 {
                                    sdx = snap((anchor_body.min_x - 7.62) - my_body.max_x);
                                } else if eff_dir == RelDir::RightOf && my_body.min_x + sdx < anchor_body.max_x + 7.62 {
                                    sdx = snap((anchor_body.max_x + 7.62) - my_body.min_x);
                                } else if eff_dir == RelDir::Above && my_body.max_y + sdy > anchor_body.min_y - 7.62 {
                                    sdy = snap((anchor_body.min_y - 7.62) - my_body.max_y);
                                } else if eff_dir == RelDir::Below && my_body.min_y + sdy < anchor_body.max_y + 7.62 {
                                    sdy = snap((anchor_body.max_y + 7.62) - my_body.min_y);
                                }
                            }
                            (sdx, sdy)
                        } else {
                            (0.0, 0.0)
                        }
                    } else {
                        let (tx, ty) = match eff_dir {
                            RelDir::LeftOf => (
                                anchor_body.min_x - gap_mm - PIN_PAD_MM - my_body.width() * 0.5,
                                anchor_body.center().y,
                            ),
                            RelDir::RightOf => (
                                anchor_body.max_x + gap_mm + PIN_PAD_MM + my_body.width() * 0.5,
                                anchor_body.center().y,
                            ),
                            RelDir::Below => (
                                anchor_body.center().x,
                                anchor_body.max_y + gap_mm + PIN_PAD_MM + my_body.depth() * 0.5,
                            ),
                            RelDir::Above => (
                                anchor_body.center().x,
                                anchor_body.min_y - gap_mm - PIN_PAD_MM - my_body.depth() * 0.5,
                            ),
                        };
                        let mc = my_body.center();
                        (snap(tx - mc.x), snap(ty - mc.y))
                    };

                    if dx.abs() > 0.01 || dy.abs() > 0.01 {
                        translate_entity(&mut entities[my_idx], dx, dy);
                        any_moved = true;
                    }
                    moved_sats.insert(c.component.clone());
                    if c.is_auto && !explicit_sats.contains(&c.component) {
                        auto_sats.insert(c.component.clone());
                        sat_push_dir.insert(c.component.clone(), eff_dir);
                    }
                }

                RelativePlacementKind::Offset { dx, dy, anchor } => {
                    let Some(&my_idx) = idx_map.get(&c.component) else {
                        continue;
                    };
                    let Some((anchor_pt, _, _, _)) = resolve_anchor(entities, &idx_map, anchor) else {
                        continue;
                    };
                    let ox = dx.to_mm().unwrap_or(0.0);
                    let oy = dy.to_mm().unwrap_or(0.0);
                    let Some((my_body, _)) = entity_body_and_pins(&entities[my_idx]) else {
                        continue;
                    };
                    let mc = my_body.center();
                    let sdx = snap((anchor_pt.x + ox) - mc.x);
                    let sdy = snap((anchor_pt.y + oy) - mc.y);
                    if sdx.abs() > 0.01 || sdy.abs() > 0.01 {
                        translate_entity(&mut entities[my_idx], sdx, sdy);
                        any_moved = true;
                    }
                    moved_sats.insert(c.component.clone());
                }

                RelativePlacementKind::Polar {
                    angle_deg,
                    dist,
                    anchor,
                } => {
                    let Some(&my_idx) = idx_map.get(&c.component) else {
                        continue;
                    };
                    let Some((anchor_pt, _, _, _)) = resolve_anchor(entities, &idx_map, anchor) else {
                        continue;
                    };
                    let d = dist.to_mm().unwrap_or(15.24);
                    let rad = angle_deg.to_radians();
                    let ox = d * rad.cos();
                    let oy = -d * rad.sin();
                    let Some((my_body, _)) = entity_body_and_pins(&entities[my_idx]) else {
                        continue;
                    };
                    let mc = my_body.center();
                    let sdx = snap((anchor_pt.x + ox) - mc.x);
                    let sdy = snap((anchor_pt.y + oy) - mc.y);
                    if sdx.abs() > 0.01 || sdy.abs() > 0.01 {
                        translate_entity(&mut entities[my_idx], sdx, sdy);
                        any_moved = true;
                    }
                    moved_sats.insert(c.component.clone());
                }

                RelativePlacementKind::AlignAxis {
                    horizontal,
                    my_pin,
                    anchor,
                } => {
                    let Some(&my_idx) = idx_map.get(&c.component) else {
                        continue;
                    };
                    let Some((anchor_pt, _, _, _)) = resolve_anchor(entities, &idx_map, anchor) else {
                        continue;
                    };
                    let Some((my_body, my_pins)) = entity_body_and_pins(&entities[my_idx]) else {
                        continue;
                    };
                    let my_pt = my_pin
                        .as_deref()
                        .and_then(|k| find_pin(my_pins, k))
                        .map(|p| p.tip_point)
                        .unwrap_or_else(|| my_body.center());
                    let (sdx, sdy) = if *horizontal {
                        (0.0, snap(anchor_pt.y - my_pt.y))
                    } else {
                        (snap(anchor_pt.x - my_pt.x), 0.0)
                    };
                    if sdx.abs() > 0.01 || sdy.abs() > 0.01 {
                        translate_entity(&mut entities[my_idx], sdx, sdy);
                        any_moved = true;
                    }
                    moved_sats.insert(c.component.clone());
                }

                RelativePlacementKind::Array {
                    members,
                    vertical,
                    pitch,
                } => {
                    let present: Vec<usize> = members
                        .iter()
                        .filter_map(|m| idx_map.get(m).copied())
                        .collect();
                    if present.len() < 2 {
                        continue;
                    }
                    let base_body = entity_body_and_pins(&entities[present[0]])
                        .map(|(b, _)| b)
                        .unwrap_or_else(|| Rect2::new(0.0, 0.0, 10.16, 10.16));
                    let auto_pitch = if *vertical {
                        snap(base_body.depth() + 15.24)
                    } else {
                        snap(base_body.width() + 15.24)
                    };
                    let step = snap(pitch.and_then(|q| q.to_mm().ok()).unwrap_or(auto_pitch)).max(5.08);
                    let base_c = base_body.center();
                    for (k, &mi) in present.iter().enumerate().skip(1) {
                        let Some((mb, _)) = entity_body_and_pins(&entities[mi]) else {
                            continue;
                        };
                        let mc = mb.center();
                        let (tx, ty) = if *vertical {
                            (base_c.x, base_c.y + (k as f64) * step)
                        } else {
                            (base_c.x + (k as f64) * step, base_c.y)
                        };
                        let sdx = snap(tx - mc.x);
                        let sdy = snap(ty - mc.y);
                        if sdx.abs() > 0.01 || sdy.abs() > 0.01 {
                            translate_entity(&mut entities[mi], sdx, sdy);
                            any_moved = true;
                        }
                        moved_sats.insert(entities[mi].id.clone());
                    }
                }
            }
        }
        if !any_moved {
            break;
        }
    }

    // 3. Deterministic Collision Push-Apart Pass ONLY for auto-generated idiom satellites!
    //    Explicit user/AI `place` statements are NOT silently moved — any real overlap is caught as
    //    a hard `E_COMPONENT_OVERLAP` compile error by E12!
    let sat_indices: Vec<usize> = auto_sats
        .iter()
        .filter_map(|id| idx_map.get(id).copied())
        .collect();
    for _iter in 0..32 {
        let mut pushed_any = false;
        for &si in &sat_indices {
            let env_s = full_envelope(&entities[si]);
            let mut hit_anchor: Option<Rect2> = None;
            let mut hit_sibling: Option<(usize, Rect2)> = None;
            for (oi, other) in entities.iter().enumerate() {
                if oi == si {
                    continue;
                }
                let env_o = full_envelope(other);
                if rects_overlap(env_s, env_o) {
                    if moved_sats.contains(&other.id) {
                        hit_sibling = Some((oi, env_o));
                    } else {
                        hit_anchor = Some(env_o);
                        break;
                    }
                }
            }
            if let Some(col) = hit_anchor {
                // Push cleanly outward away from anchor body along primary placement direction
                let dir = sat_push_dir.get(&entities[si].id).copied();
                let (pdx, pdy) = match dir {
                    Some(RelDir::LeftOf) => (snap(col.min_x - env_s.max_x - GRID_MM).min(-GRID_MM), 0.0),
                    Some(RelDir::RightOf) => (snap(col.max_x - env_s.min_x + GRID_MM).max(GRID_MM), 0.0),
                    Some(RelDir::Above) => (0.0, snap(col.min_y - env_s.max_y - GRID_MM).min(-GRID_MM)),
                    Some(RelDir::Below) => (0.0, snap(col.max_y - env_s.min_y + GRID_MM).max(GRID_MM)),
                    None => (GRID_MM * 2.0, 0.0),
                };
                translate_entity(&mut entities[si], pdx, pdy);
                pushed_any = true;
            } else if let Some((oi, col)) = hit_sibling {
                // Only push the higher-indexed sibling so the pair separates symmetrically without chasing!
                if si > oi {
                    let dir = sat_push_dir.get(&entities[si].id).copied();
                    let (pdx, pdy) = match dir {
                        // Adjacent RightOf/LeftOf satellites (e.g. R1/R2/R3 pullups on PC4/PC5/RESET) fan horizontally outward!
                        Some(RelDir::RightOf) => (snap(col.max_x - env_s.min_x + GRID_MM).max(GRID_MM * 2.0), 0.0),
                        Some(RelDir::LeftOf) => (snap(col.min_x - env_s.max_x - GRID_MM).min(-GRID_MM * 2.0), 0.0),
                        // Adjacent Above/Below decoupling caps fan rightward in a neat horizontal row!
                        Some(RelDir::Above) | Some(RelDir::Below) => {
                            (snap(col.max_x - env_s.min_x + GRID_MM).max(GRID_MM * 2.0), 0.0)
                        }
                        None => (0.0, snap(col.max_y - env_s.min_y + GRID_MM).max(GRID_MM * 2.0)),
                    };
                    translate_entity(&mut entities[si], pdx, pdy);
                    pushed_any = true;
                }
            }
        }
        if !pushed_any {
            break;
        }
    }

    // 4. Inter-Group Horizontal Repack Pass:
    //    If satellites expanded Group i rightward into Group i+1 (same row band), shift Group i+1 and all
    //    its members rightward cleanly so zone frames NEVER collide!
    if groups.len() > 1 {
        let mut g_bounds: Vec<(f64, f64, f64, f64)> = Vec::new();
        for g in groups.iter() {
            let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
            for m in &g.members {
                if let Some(&ei) = idx_map.get(m) {
                    let env = full_envelope(&entities[ei]);
                    x0 = x0.min(env.min_x);
                    x1 = x1.max(env.max_x);
                    y0 = y0.min(env.min_y);
                    y1 = y1.max(env.max_y);
                }
            }
            g_bounds.push((x0, x1, y0, y1));
        }
        let min_group_gap = 25.4_f64;
        for i in 0..groups.len() {
            let (ax0, ax1, ay0, ay1) = g_bounds[i];
            if !ax0.is_finite() {
                continue;
            }
            for j in (i + 1)..groups.len() {
                let (bx0, _bx1, by0, by1) = g_bounds[j];
                if !bx0.is_finite() {
                    continue;
                }
                // Check vertical band overlap (same row)
                let y_overlap = ay0 < by1 + 10.0 && ay1 > by0 - 10.0;
                if y_overlap && bx0 < ax1 + min_group_gap {
                    let dx = snap((ax1 + min_group_gap) - bx0);
                    if dx > 0.01 {
                        for m in &groups[j].members {
                            if let Some(&ei) = idx_map.get(m) {
                                translate_entity(&mut entities[ei], dx, 0.0);
                            }
                        }
                        g_bounds[j].0 += dx;
                        g_bounds[j].1 += dx;
                        groups[j].bbox.min_x += dx;
                        groups[j].bbox.max_x += dx;
                    }
                }
            }
        }
    }

    // 5. Keep entire sheet cleanly inside positive printable coordinates (min_x >= 30.48mm, min_y >= 30.48mm)
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for e in entities.iter() {
        let env = full_envelope(e);
        min_x = min_x.min(env.min_x);
        min_y = min_y.min(env.min_y);
    }
    let shift_x = if min_x < 30.48 { snap(30.48 - min_x) } else { 0.0 };
    let shift_y = if min_y < 30.48 { snap(30.48 - min_y) } else { 0.0 };
    if shift_x > 0.0 || shift_y > 0.0 {
        for e in entities.iter_mut() {
            translate_entity(e, shift_x, shift_y);
        }
        for g in groups.iter_mut() {
            g.bbox.min_x += shift_x;
            g.bbox.max_x += shift_x;
            g.bbox.min_y += shift_y;
            g.bbox.max_y += shift_y;
        }
    }

    // 6. Recompute each group's bbox tightly from the actual post-solver member envelopes
    //    (eliminates stale pre-solver empty vertical space left behind when satellites snap to pins!).
    for g in groups.iter_mut() {
        let (mut gx0, mut gx1, mut gy0, mut gy1) =
            (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for m in &g.members {
            if let Some(&ei) = idx_map.get(m) {
                let env = full_envelope(&entities[ei]);
                gx0 = gx0.min(env.min_x - 2.54);
                gx1 = gx1.max(env.max_x + 2.54);
                gy0 = gy0.min(env.min_y - 5.08);
                gy1 = gy1.max(env.max_y + 2.54);
            }
        }
        if gx0.is_finite() {
            let note_h = if g.notes.is_empty() {
                0.0
            } else {
                (g.notes.len() as f64) * 6.35 + 5.08
            };
            g.bbox = Rect2::new(snap(gx0), snap(gy0), snap(gx1), snap(gy1 + note_h));
        }
    }

    moved_sats
}
