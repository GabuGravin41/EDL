//! Recursive 2D schematic zone layout engine (`hstack`, `vstack`, `subzone`) on the 2.54mm KiCad grid.
//!
//! Lets AI models and engineers compose realistic dense subcircuits (side-by-side decoupling caps,
//! parallel resistor banks, hub-and-spoke ICs, and dashed sub-zone cells like KiCad 9 production sheets)
//! with zero manual coordinate math.

use crate::core::geom::Rect2;
use crate::core::ir::SchematicSubZoneIr;
use crate::domains::elec::ast::LayoutNode;
use std::collections::BTreeMap;

const GRID_MM: f64 = 2.54;
const SUBZONE_PAD: f64 = 5.08;
const SUBZONE_FOOTER: f64 = 5.08;

/// Body-aware KiCad 2.54mm-grid cell padding: compact passives pack tightly (like hand-drafted
/// KiCad decoupling banks), while multi-pin ICs keep generous left/right gutters for global net flags.
fn cell_padding(bw: f64, bh: f64) -> (f64, f64, f64, f64) {
    // Returns (pad_left, pad_right, pad_top, pad_bot)
    if bw <= 10.5 && bh <= 10.5 {
        // Vertical 2-pin passives (Device:C / Device:R) place RefDes+Value cleanly to the right
        if bh >= bw {
            (5.08, 10.16, 7.62, 7.62)
        } else {
            (7.62, 7.62, 7.62, 7.62)
        }
    } else if bw <= 20.5 && bh <= 20.5 {
        (10.16, 10.16, 10.16, 10.16)
    } else {
        (15.24, 15.24, 10.16, 10.16)
    }
}

fn snap(v: f64) -> f64 {
    (v / GRID_MM).round() * GRID_MM
}

/// Measure the total 2D bounding box `(width_mm, height_mm)` of a `LayoutNode` tree.
pub fn measure_layout_node(
    node: &LayoutNode,
    body_size: &dyn Fn(&str) -> (f64, f64),
    h_gap: f64,
    v_gap: f64,
) -> (f64, f64) {
    match node {
        LayoutNode::Comp(cid) => {
            let (bw, bh) = body_size(cid);
            let (pl, pr, pt, pb) = cell_padding(bw, bh);
            (snap(bw + pl + pr), snap(bh + pt + pb))
        }
        LayoutNode::HStack(items) => {
            if items.is_empty() {
                return (15.24, 15.24);
            }
            let mut w = 0.0_f64;
            let mut h = 0.0_f64;
            for (i, it) in items.iter().enumerate() {
                let (iw, ih) = measure_layout_node(it, body_size, h_gap, v_gap);
                if i > 0 {
                    w += h_gap;
                }
                w += iw;
                h = h.max(ih);
            }
            (snap(w), snap(h))
        }
        LayoutNode::VStack(items) => {
            if items.is_empty() {
                return (15.24, 15.24);
            }
            let mut w = 0.0_f64;
            let mut h = 0.0_f64;
            for (i, it) in items.iter().enumerate() {
                let (iw, ih) = measure_layout_node(it, body_size, h_gap, v_gap);
                if i > 0 {
                    h += v_gap;
                }
                h += ih;
                w = w.max(iw);
            }
            (snap(w), snap(h))
        }
        LayoutNode::SubZone { title, items } => {
            let inner = if items.len() == 1 {
                measure_layout_node(&items[0], body_size, h_gap, v_gap)
            } else {
                let tmp = LayoutNode::HStack(items.clone());
                measure_layout_node(&tmp, body_size, h_gap, v_gap)
            };
            let title_min_w = (title.len() as f64) * 2.2 + 10.16;
            (
                snap(inner.0.max(title_min_w) + 2.0 * SUBZONE_PAD),
                snap(inner.1 + 2.0 * SUBZONE_PAD + SUBZONE_FOOTER),
            )
        }
    }
}

/// Recursively place all components inside `node` starting at `(x0, y0)` (snapped to 2.54mm grid),
/// populating `positions` (`comp_id -> (body_min_x, body_min_y)`) and `subzones`.
pub fn place_layout_node(
    node: &LayoutNode,
    x0: f64,
    y0: f64,
    body_size: &dyn Fn(&str) -> (f64, f64),
    h_gap: f64,
    v_gap: f64,
    positions: &mut BTreeMap<String, (f64, f64)>,
    subzones: &mut Vec<SchematicSubZoneIr>,
) -> Rect2 {
    let (total_w, total_h) = measure_layout_node(node, body_size, h_gap, v_gap);
    let x0 = snap(x0);
    let y0 = snap(y0);

    match node {
        LayoutNode::Comp(cid) => {
            let (bw, bh) = body_size(cid);
            let (pl, _pr, pt, _pb) = cell_padding(bw, bh);
            let bx = snap(x0 + pl);
            let by = snap(y0 + pt);
            positions.insert(cid.clone(), (bx, by));
            Rect2::new(x0, y0, snap(x0 + total_w), snap(y0 + total_h))
        }
        LayoutNode::HStack(items) => {
            let mut cx = x0;
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    cx = snap(cx + h_gap);
                }
                let (iw, ih) = measure_layout_node(it, body_size, h_gap, v_gap);
                // Vertically center children inside the HStack band
                let cy = snap(y0 + (total_h - ih) * 0.5);
                place_layout_node(it, cx, cy, body_size, h_gap, v_gap, positions, subzones);
                cx = snap(cx + iw);
            }
            Rect2::new(x0, y0, snap(x0 + total_w), snap(y0 + total_h))
        }
        LayoutNode::VStack(items) => {
            let mut cy = y0;
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    cy = snap(cy + v_gap);
                }
                let (iw, ih) = measure_layout_node(it, body_size, h_gap, v_gap);
                let cx = snap(x0 + (total_w - iw) * 0.5);
                place_layout_node(it, cx, cy, body_size, h_gap, v_gap, positions, subzones);
                cy = snap(cy + ih);
            }
            Rect2::new(x0, y0, snap(x0 + total_w), snap(y0 + total_h))
        }
        LayoutNode::SubZone { title, items } => {
            let inner_x = snap(x0 + SUBZONE_PAD);
            let inner_y = snap(y0 + SUBZONE_PAD);
            if items.len() == 1 {
                place_layout_node(&items[0], inner_x, inner_y, body_size, h_gap, v_gap, positions, subzones);
            } else {
                let tmp = LayoutNode::HStack(items.clone());
                place_layout_node(&tmp, inner_x, inner_y, body_size, h_gap, v_gap, positions, subzones);
            }
            let rect = Rect2::new(x0, y0, snap(x0 + total_w), snap(y0 + total_h));
            subzones.push(SchematicSubZoneIr {
                title: title.clone(),
                bbox: rect,
            });
            rect
        }
    }
}

/// 2D Sheet-Aware Skyline Group Packer (v3.0).
///
/// Packs schematic zone groups `(group_idx, width_mm, height_mm)` cleanly across the full
/// printable landscape width of the target sheet (minimizing dead negative space AND keeping
/// every group strictly clear of the outer KiCad frame and bottom-right title block!).
pub fn pack_groups_skyline(
    group_dims: &[(usize, f64, f64)],
    sheet_w: f64,
    sheet_h: f64,
    start_x: f64,
    start_y: f64,
    h_gap: f64,
    v_gap: f64,
) -> BTreeMap<usize, (f64, f64)> {
    let mut result = BTreeMap::new();
    if group_dims.is_empty() {
        return result;
    }

    // Sort groups tallest-first (then widest-first) so tall anchor blocks anchor clean columns
    // and shorter blocks tuck neatly into the remaining open skyline slots!
    let mut order: Vec<(usize, f64, f64)> = group_dims.to_vec();
    order.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap()
            .then_with(|| b.1.partial_cmp(&a.1).unwrap())
            .then_with(|| a.0.cmp(&b.0))
    });

    let try_pack = |cand_w: f64, cand_h: f64, hg: f64, vg: f64| -> (BTreeMap<usize, (f64, f64)>, f64, f64) {
        let margin_r = 15.24_f64;
        let max_x = snap((cand_w - margin_r).max(start_x + 120.0));
        let start_gx = (start_x / GRID_MM).round() as i32;
        let max_gx = (max_x / GRID_MM).round() as i32;
        let n_bins = (max_gx - start_gx + 1).max(1) as usize;
        let mut skyline: Vec<f64> = vec![snap(start_y); n_bins];

        // KiCad bottom-right title-block keepout zone
        let tb_x0 = cand_w - 122.0;
        let tb_y0 = cand_h - 44.0;

        let h_gap_g = (snap(hg) / GRID_MM).round().max(3.0) as usize;
        let v_gap_mm = snap(vg).max(10.16);

        let mut placed: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
        let mut used_max_x = start_x;
        let mut used_max_y = start_y;

        for &(gidx, gw, gh) in &order {
            let gw_s = snap(gw).max(20.32);
            let gh_s = snap(gh).max(20.32);
            let w_bins = (gw_s / GRID_MM).ceil().max(1.0) as usize;

            let mut best: Option<(f64, f64, usize)> = None; // (bottom_y, top_y, bin_x)
            let max_start_bin = n_bins.saturating_sub(w_bins);
            // Sample every 2 grid steps (5.08mm) for fast, crisp alignment
            let mut bx = 0usize;
            while bx <= max_start_bin {
                let mut top_y = snap(start_y);
                for k in bx..(bx + w_bins).min(n_bins) {
                    top_y = top_y.max(skyline[k]);
                }
                let x_mm = snap(start_x + (bx as f64) * GRID_MM);
                let bot_y = snap(top_y + gh_s);
                let right_x = x_mm + gw_s;

                // Strictly avoid bottom-right title block!
                let hits_title_block = right_x > tb_x0 && bot_y > tb_y0 && top_y < cand_h;
                if !hits_title_block {
                    let key = (bot_y, top_y, bx);
                    if best.map(|b| key < b).unwrap_or(true) {
                        best = Some(key);
                    }
                }
                bx += 2;
            }

            let (chosen_x, chosen_y, chosen_bx) = if let Some((_, ty, bx)) = best {
                (snap(start_x + (bx as f64) * GRID_MM), ty, bx)
            } else {
                // Fallback if group wider than sheet: place at left edge below current skyline
                let top_y = skyline.iter().copied().fold(snap(start_y), f64::max);
                (snap(start_x), top_y, 0)
            };

            let new_top = snap(chosen_y + gh_s + v_gap_mm);
            let fill_end = (chosen_bx + w_bins + h_gap_g).min(n_bins);
            for k in chosen_bx..fill_end {
                skyline[k] = skyline[k].max(new_top);
            }

            placed.insert(gidx, (chosen_x, chosen_y));
            used_max_x = used_max_x.max(chosen_x + gw_s);
            used_max_y = used_max_y.max(chosen_y + gh_s);
        }
        (placed, used_max_x, used_max_y)
    };

    // First try the declared target sheet size (normal gap, then compact gap) before ever escalating ISO size!
    for &(hg, vg) in &[(h_gap, v_gap), (10.16, 10.16)] {
        let (p, ux, uy) = try_pack(sheet_w, sheet_h, hg, vg);
        if ux <= sheet_w - 14.0 && uy <= sheet_h - 14.0 {
            return p;
        }
    }
    let iso_candidates: &[(f64, f64)] = &[
        (420.0, 297.0),
        (594.0, 420.0),
        (841.0, 594.0),
        (1189.0, 841.0),
    ];
    let mut best_pack = None;
    for &(cw, ch) in iso_candidates {
        if cw < sheet_w - 1.0 && ch < sheet_h - 1.0 {
            continue;
        }
        let (p, ux, uy) = try_pack(cw, ch, h_gap, v_gap);
        let fits = ux <= cw - 14.0 && uy <= ch - 14.0;
        best_pack = Some(p);
        if fits {
            break;
        }
    }
    result = best_pack.unwrap_or_default();
    result
}
