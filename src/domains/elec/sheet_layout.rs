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
const CELL_PAD_X: f64 = 17.78; // 7 grids left/right for pin stubs + wide shaped net flags
const CELL_PAD_TOP: f64 = 12.70; // 5 grids above for North power rail + RefDes
const CELL_PAD_BOT: f64 = 15.24; // 6 grids below for South GND symbol + Value
const SUBZONE_PAD: f64 = 5.08;
const SUBZONE_FOOTER: f64 = 7.62;

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
            (
                snap(bw + 2.0 * CELL_PAD_X),
                snap(bh + CELL_PAD_TOP + CELL_PAD_BOT),
            )
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
            let bx = snap(x0 + CELL_PAD_X);
            let by = snap(y0 + CELL_PAD_TOP);
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
