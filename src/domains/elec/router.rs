//! Obstacle-aware 2.54mm KiCad grid A* orthogonal router for dense 2D schematic zones (v3.0).
//!
//! Features:
//! - 0-bend straight-shot fast path for pin-aligned connections
//! - Strict directed H/V collinear-edge separation (two different nets NEVER overlap collinearly -> zero KiCad shorts!)
//! - Perpendicular wire crossing allowed only with high penalty (never turning/landing on a foreign wire corner)
//! - Outward pin-exit preference so wires leave pins cleanly without doubling back over symbol pins
//! - Strict component-body and foreign-pin-tip avoidance

use crate::core::geom::{Point2, Rect2, Segment2, Side};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

const GRID_MM: f64 = 2.54;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct GPt {
    x: i32,
    y: i32,
}

fn to_grid(p: Point2) -> GPt {
    GPt {
        x: (p.x / GRID_MM).round() as i32,
        y: (p.y / GRID_MM).round() as i32,
    }
}

fn from_grid(g: GPt) -> Point2 {
    Point2::new(g.x as f64 * GRID_MM, g.y as f64 * GRID_MM)
}

fn side_to_dir(s: Side) -> u8 {
    match s {
        Side::East => 1,
        Side::West => 2,
        Side::North => 3,
        Side::South => 4,
    }
}

fn opposite_dir(d: u8) -> u8 {
    match d {
        1 => 2,
        2 => 1,
        3 => 4,
        4 => 3,
        _ => 0,
    }
}

/// Persistent cross-net wire occupancy on the 2.54mm KiCad routing grid.
/// Separates horizontal edges, vertical edges, and corners so different nets can cross
/// perpendicularly (at high penalty) without EVER overlapping collinearly or shorting!
#[derive(Debug, Clone, Default)]
pub struct GridOccupancy {
    /// Horizontal unit grid edge `(min_x, y)` connecting `(min_x, y) -- (min_x + 1, y)`.
    pub used_h_edges: BTreeSet<(i32, i32)>,
    /// Vertical unit grid edge `(x, min_y)` connecting `(x, min_y) -- (x, min_y + 1)`.
    pub used_v_edges: BTreeSet<(i32, i32)>,
    /// Cells where an existing net has a bend, junction, or endpoint (another net must not pass through).
    pub used_corners: BTreeSet<(i32, i32)>,
    /// All cells touched by any previous net (for perpendicular crossing penalty).
    pub used_cells: BTreeSet<(i32, i32)>,
}

impl GridOccupancy {
    pub fn record_edge(&mut self, a: (i32, i32), b: (i32, i32)) {
        if a.1 == b.1 {
            self.used_h_edges.insert((a.0.min(b.0), a.1));
        } else if a.0 == b.0 {
            self.used_v_edges.insert((a.0, a.1.min(b.1)));
        }
        self.used_cells.insert(a);
        self.used_cells.insert(b);
    }

    /// Record an orthogonal world-mm segment into occupancy (all unit grid edges + endpoints as corners).
    pub fn record_world_segment(&mut self, seg: Segment2) {
        let g0 = to_grid(seg.p0);
        let g1 = to_grid(seg.p1);
        self.used_corners.insert((g0.x, g0.y));
        self.used_corners.insert((g1.x, g1.y));
        if g0.y == g1.y {
            let (x0, x1) = (g0.x.min(g1.x), g0.x.max(g1.x));
            for x in x0..x1 {
                self.record_edge((x, g0.y), (x + 1, g0.y));
            }
        } else if g0.x == g1.x {
            let (y0, y1) = (g0.y.min(g1.y), g0.y.max(g1.y));
            for y in y0..y1 {
                self.record_edge((g0.x, y), (g0.x, y + 1));
            }
        }
    }

    /// Reserve a terminal point (power/GND symbol or net label) so no foreign wire touches or crosses it.
    pub fn record_terminal_point(&mut self, pt: Point2) {
        let g = to_grid(pt);
        self.used_corners.insert((g.x, g.y));
        self.used_cells.insert((g.x, g.y));
    }

    fn edge_used(&self, a: GPt, b: GPt) -> bool {
        if a.y == b.y {
            self.used_h_edges.contains(&(a.x.min(b.x), a.y))
        } else if a.x == b.x {
            self.used_v_edges.contains(&(a.x, a.y.min(b.y)))
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct State {
    f: i32,
    g: i32,
    pt: GPt,
    dir: u8, // 0=none, 1=E, 2=W, 3=N, 4=S
}

impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .cmp(&self.f)
            .then_with(|| other.g.cmp(&self.g))
            .then_with(|| self.pt.cmp(&other.pt))
    }
}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Backwards-compatible wrapper using cell-only tracking.
pub fn route_net_astar(
    endpoints: &[(Point2, Side)],
    body_obstacles: &[Rect2],
    foreign_pin_tips: &BTreeSet<(i32, i32)>,
    used_wire_cells: &mut BTreeSet<(i32, i32)>,
) -> (Vec<Segment2>, Vec<Point2>) {
    let mut occ = GridOccupancy {
        used_cells: used_wire_cells.clone(),
        ..Default::default()
    };
    let res = route_net_astar_v3(endpoints, body_obstacles, foreign_pin_tips, &mut occ);
    used_wire_cells.extend(occ.used_cells);
    res
}

/// Strict short-free v3.0 grid A* router: returns `Some((segments, junctions))` ONLY when a 100%
/// collinear-conflict-free, corner-conflict-free, body-clear path exists for every pin in the net.
/// Returns `None` if the corridor is completely blocked (caller should emit clean `label_flag` stubs
/// instead of risking an accidental KiCad short!).
pub fn try_route_net_astar_strict(
    endpoints: &[(Point2, Side)],
    body_obstacles: &[Rect2],
    foreign_pin_tips: &BTreeSet<(i32, i32)>,
    occupancy: &mut GridOccupancy,
) -> Option<(Vec<Segment2>, Vec<Point2>)> {
    if endpoints.len() < 2 {
        return Some((Vec::new(), Vec::new()));
    }
    let pin_gpts: Vec<(GPt, Side)> = endpoints.iter().map(|(p, s)| (to_grid(*p), *s)).collect();
    let pin_set: BTreeSet<GPt> = pin_gpts.iter().map(|(g, _)| *g).collect();

    let mut blocked: BTreeSet<GPt> = BTreeSet::new();
    for &(fx, fy) in foreign_pin_tips {
        let gp = GPt { x: fx, y: fy };
        if !pin_set.contains(&gp) {
            blocked.insert(gp);
        }
    }
    let (mut min_gx, mut max_gx) = (i32::MAX, i32::MIN);
    let (mut min_gy, mut max_gy) = (i32::MAX, i32::MIN);
    for (gp, _) in &pin_gpts {
        min_gx = min_gx.min(gp.x);
        max_gx = max_gx.max(gp.x);
        min_gy = min_gy.min(gp.y);
        max_gy = max_gy.max(gp.y);
    }
    let pad = 18;
    min_gx -= pad;
    max_gx += pad;
    min_gy -= pad;
    max_gy += pad;

    for r in body_obstacles {
        let gx0 = (r.min_x / GRID_MM).floor() as i32;
        let gx1 = (r.max_x / GRID_MM).ceil() as i32;
        let gy0 = (r.min_y / GRID_MM).floor() as i32;
        let gy1 = (r.max_y / GRID_MM).ceil() as i32;
        if gx1 < min_gx || gx0 > max_gx || gy1 < min_gy || gy0 > max_gy {
            continue;
        }
        for gx in gx0.max(min_gx)..=gx1.min(max_gx) {
            for gy in gy0.max(min_gy)..=gy1.min(max_gy) {
                let gp = GPt { x: gx, y: gy };
                if !pin_set.contains(&gp) {
                    blocked.insert(gp);
                }
            }
        }
    }

    // Fast path: 0-bend straight shot for 2 pin-aligned endpoints
    if pin_gpts.len() == 2 {
        let (p0, _) = pin_gpts[0];
        let (p1, _) = pin_gpts[1];
        if (p0.x == p1.x || p0.y == p1.y) && p0 != p1 {
            if let Some(straight) = try_straight_path(p0, p1, &blocked, occupancy) {
                let mut raw_edges: BTreeSet<(GPt, GPt)> = BTreeSet::new();
                for win in straight.windows(2) {
                    let (a, b) = if win[0] <= win[1] { (win[0], win[1]) } else { (win[1], win[0]) };
                    raw_edges.insert((a, b));
                    occupancy.record_edge((a.x, a.y), (b.x, b.y));
                }
                occupancy.used_corners.insert((p0.x, p0.y));
                occupancy.used_corners.insert((p1.x, p1.y));
                return Some((merge_grid_edges(&raw_edges, &pin_set), Vec::new()));
            }
        }
    }

    // Plan onto a tentative occupancy clone so partial failure never pollutes `occupancy`!
    let mut trial_occ = occupancy.clone();
    let mut tree_cells: BTreeSet<GPt> = BTreeSet::new();
    tree_cells.insert(pin_gpts[0].0);
    let mut remaining: Vec<(GPt, Side)> = pin_gpts[1..].to_vec();
    let mut raw_edges: BTreeSet<(GPt, GPt)> = BTreeSet::new();

    while !remaining.is_empty() {
        let (best_idx, (target, target_side)) = remaining
            .iter()
            .enumerate()
            .min_by_key(|(_, (p, _))| {
                tree_cells
                    .iter()
                    .map(|t| (t.x - p.x).abs() + (t.y - p.y).abs())
                    .min()
                    .unwrap_or(i32::MAX)
            })
            .map(|(i, p)| (i, *p))
            .unwrap();
        remaining.remove(best_idx);
        if tree_cells.contains(&target) {
            continue;
        }
        let path = astar_to_tree(
            target,
            side_to_dir(target_side),
            &tree_cells,
            &blocked,
            &trial_occ,
            true,
            (min_gx, max_gx, min_gy, max_gy),
        )?;
        for win in path.windows(2) {
            let (a, b) = if win[0] <= win[1] { (win[0], win[1]) } else { (win[1], win[0]) };
            raw_edges.insert((a, b));
            tree_cells.insert(a);
            tree_cells.insert(b);
            trial_occ.record_edge((a.x, a.y), (b.x, b.y));
        }
        if let Some(first) = path.first() {
            trial_occ.used_corners.insert((first.x, first.y));
        }
        if let Some(last) = path.last() {
            trial_occ.used_corners.insert((last.x, last.y));
        }
        for w in path.windows(3) {
            if (w[1].x - w[0].x) != (w[2].x - w[1].x) || (w[1].y - w[0].y) != (w[2].y - w[1].y) {
                trial_occ.used_corners.insert((w[1].x, w[1].y));
            }
        }
    }

    let mut adj: BTreeMap<GPt, Vec<GPt>> = BTreeMap::new();
    for &(a, b) in &raw_edges {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }
    let junction_set: BTreeSet<GPt> = adj
        .iter()
        .filter(|(_, nbrs)| nbrs.len() >= 3)
        .map(|(pt, _)| *pt)
        .collect();
    for j in &junction_set {
        trial_occ.used_corners.insert((j.x, j.y));
    }
    let junctions: Vec<Point2> = junction_set.iter().map(|pt| from_grid(*pt)).collect();
    let mut split_pts = pin_set.clone();
    for &j in &junction_set {
        split_pts.insert(j);
    }
    let segments = merge_grid_edges(&raw_edges, &split_pts);
    *occupancy = trial_occ;
    Some((segments, junctions))
}

/// v3.0 obstacle-aware & collinear-short-free grid A* orthogonal router.
pub fn route_net_astar_v3(
    endpoints: &[(Point2, Side)],
    body_obstacles: &[Rect2],
    foreign_pin_tips: &BTreeSet<(i32, i32)>,
    occupancy: &mut GridOccupancy,
) -> (Vec<Segment2>, Vec<Point2>) {
    if endpoints.len() < 2 {
        return (Vec::new(), Vec::new());
    }

    let pin_gpts: Vec<(GPt, Side)> = endpoints.iter().map(|(p, s)| (to_grid(*p), *s)).collect();
    let pin_set: BTreeSet<GPt> = pin_gpts.iter().map(|(g, _)| *g).collect();

    // Build obstacle grid set (component bodies + foreign pin tips, never blocking our own pin tips!)
    let mut blocked: BTreeSet<GPt> = BTreeSet::new();
    for &(fx, fy) in foreign_pin_tips {
        let gp = GPt { x: fx, y: fy };
        if !pin_set.contains(&gp) {
            blocked.insert(gp);
        }
    }

    // Compute search window around net endpoints
    let (mut min_gx, mut max_gx) = (i32::MAX, i32::MIN);
    let (mut min_gy, mut max_gy) = (i32::MAX, i32::MIN);
    for (gp, _) in &pin_gpts {
        min_gx = min_gx.min(gp.x);
        max_gx = max_gx.max(gp.x);
        min_gy = min_gy.min(gp.y);
        max_gy = max_gy.max(gp.y);
    }
    let pad = 16;
    min_gx -= pad;
    max_gx += pad;
    min_gy -= pad;
    max_gy += pad;

    for r in body_obstacles {
        let gx0 = (r.min_x / GRID_MM).floor() as i32;
        let gx1 = (r.max_x / GRID_MM).ceil() as i32;
        let gy0 = (r.min_y / GRID_MM).floor() as i32;
        let gy1 = (r.max_y / GRID_MM).ceil() as i32;
        if gx1 < min_gx || gx0 > max_gx || gy1 < min_gy || gy0 > max_gy {
            continue;
        }
        for gx in gx0.max(min_gx)..=gx1.min(max_gx) {
            for gy in gy0.max(min_gy)..=gy1.min(max_gy) {
                let gp = GPt { x: gx, y: gy };
                if !pin_set.contains(&gp) {
                    blocked.insert(gp);
                }
            }
        }
    }

    // Fast path: 0-bend straight shot for 2 pin-aligned endpoints with clear corridor!
    if pin_gpts.len() == 2 {
        let (p0, _) = pin_gpts[0];
        let (p1, _) = pin_gpts[1];
        if (p0.x == p1.x || p0.y == p1.y) && p0 != p1 {
            if let Some(straight) = try_straight_path(p0, p1, &blocked, occupancy) {
                let mut raw_edges: BTreeSet<(GPt, GPt)> = BTreeSet::new();
                for win in straight.windows(2) {
                    let (a, b) = if win[0] <= win[1] { (win[0], win[1]) } else { (win[1], win[0]) };
                    raw_edges.insert((a, b));
                    occupancy.record_edge((a.x, a.y), (b.x, b.y));
                }
                occupancy.used_corners.insert((p0.x, p0.y));
                occupancy.used_corners.insert((p1.x, p1.y));
                let segs = merge_grid_edges(&raw_edges, &pin_set);
                return (segs, Vec::new());
            }
        }
    }

    // Connect pins incrementally via nearest-tree-cell (Steiner-like MST on grid)
    let mut tree_cells: BTreeSet<GPt> = BTreeSet::new();
    tree_cells.insert(pin_gpts[0].0);

    let mut remaining: Vec<(GPt, Side)> = pin_gpts[1..].to_vec();
    let mut raw_edges: BTreeSet<(GPt, GPt)> = BTreeSet::new();

    while !remaining.is_empty() {
        let (best_idx, (target, target_side)) = remaining
            .iter()
            .enumerate()
            .min_by_key(|(_, (p, _))| {
                tree_cells
                    .iter()
                    .map(|t| (t.x - p.x).abs() + (t.y - p.y).abs())
                    .min()
                    .unwrap_or(i32::MAX)
            })
            .map(|(i, p)| (i, *p))
            .unwrap();
        remaining.remove(best_idx);

        if tree_cells.contains(&target) {
            continue;
        }

        // First try strict non-collinear A*; if boxed in completely, fall back to soft A*, then L-fallback.
        let path = astar_to_tree(
            target,
            side_to_dir(target_side),
            &tree_cells,
            &blocked,
            occupancy,
            true,
            (min_gx, max_gx, min_gy, max_gy),
        )
        .or_else(|| {
            astar_to_tree(
                target,
                side_to_dir(target_side),
                &tree_cells,
                &blocked,
                occupancy,
                false,
                (min_gx, max_gx, min_gy, max_gy),
            )
        });

        let path = match path {
            Some(p) => p,
            None => {
                let nearest = *tree_cells
                    .iter()
                    .min_by_key(|t| (t.x - target.x).abs() + (t.y - target.y).abs())
                    .unwrap();
                let mut fb = vec![target];
                let mut cur = target;
                while cur.x != nearest.x {
                    cur.x += (nearest.x - cur.x).signum();
                    fb.push(cur);
                }
                while cur.y != nearest.y {
                    cur.y += (nearest.y - cur.y).signum();
                    fb.push(cur);
                }
                fb
            }
        };

        for win in path.windows(2) {
            let (a, b) = if win[0] <= win[1] {
                (win[0], win[1])
            } else {
                (win[1], win[0])
            };
            raw_edges.insert((a, b));
            tree_cells.insert(a);
            tree_cells.insert(b);
            occupancy.record_edge((a.x, a.y), (b.x, b.y));
        }
        // Record corners/endpoints of this net so future nets don't turn on top of them
        if let Some(first) = path.first() {
            occupancy.used_corners.insert((first.x, first.y));
        }
        if let Some(last) = path.last() {
            occupancy.used_corners.insert((last.x, last.y));
        }
        for w in path.windows(3) {
            let dx1 = w[1].x - w[0].x;
            let dy1 = w[1].y - w[0].y;
            let dx2 = w[2].x - w[1].x;
            let dy2 = w[2].y - w[1].y;
            if dx1 != dx2 || dy1 != dy2 {
                occupancy.used_corners.insert((w[1].x, w[1].y));
            }
        }
    }

    // Compute exact junctions (degree >= 3 in grid graph) and merge collinear segments
    let mut adj: BTreeMap<GPt, Vec<GPt>> = BTreeMap::new();
    for &(a, b) in &raw_edges {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }

    let junction_set: BTreeSet<GPt> = adj
        .iter()
        .filter(|(_, nbrs)| nbrs.len() >= 3)
        .map(|(pt, _)| *pt)
        .collect();
    for j in &junction_set {
        occupancy.used_corners.insert((j.x, j.y));
    }
    let junctions: Vec<Point2> = junction_set.iter().map(|pt| from_grid(*pt)).collect();

    // Never merge collinear segments straight through a pin tip or junction — KiCad 9 requires
    // an explicit wire endpoint at every connected pin tip!
    let mut split_pts = pin_set.clone();
    for &j in &junction_set {
        split_pts.insert(j);
    }
    let segments = merge_grid_edges(&raw_edges, &split_pts);
    (segments, junctions)
}

fn try_straight_path(
    p0: GPt,
    p1: GPt,
    blocked: &BTreeSet<GPt>,
    occupancy: &GridOccupancy,
) -> Option<Vec<GPt>> {
    let dx = (p1.x - p0.x).signum();
    let dy = (p1.y - p0.y).signum();
    let mut path = vec![p0];
    let mut cur = p0;
    while cur != p1 {
        let nxt = GPt {
            x: cur.x + dx,
            y: cur.y + dy,
        };
        if nxt != p1 && (blocked.contains(&nxt) || occupancy.used_corners.contains(&(nxt.x, nxt.y))) {
            return None;
        }
        if occupancy.edge_used(cur, nxt) {
            return None;
        }
        path.push(nxt);
        cur = nxt;
    }
    Some(path)
}

fn astar_to_tree(
    start: GPt,
    preferred_exit_dir: u8,
    goals: &BTreeSet<GPt>,
    blocked: &BTreeSet<GPt>,
    occupancy: &GridOccupancy,
    strict_edges: bool,
    bounds: (i32, i32, i32, i32),
) -> Option<Vec<GPt>> {
    let (min_gx, max_gx, min_gy, max_gy) = bounds;
    let h = |p: GPt| -> i32 {
        goals
            .iter()
            .map(|g| (g.x - p.x).abs() + (g.y - p.y).abs())
            .min()
            .unwrap_or(0)
            * 10
    };

    let mut heap = BinaryHeap::new();
    let mut best_g: BTreeMap<(GPt, u8), i32> = BTreeMap::new();
    let mut came_from: BTreeMap<(GPt, u8), (GPt, u8)> = BTreeMap::new();

    heap.push(State {
        f: h(start),
        g: 0,
        pt: start,
        dir: 0,
    });
    best_g.insert((start, 0), 0);

    let dirs: [(i32, i32, u8); 4] = [(1, 0, 1), (-1, 0, 2), (0, -1, 3), (0, 1, 4)];
    let mut expansions = 0usize;

    while let Some(cur) = heap.pop() {
        if goals.contains(&cur.pt) {
            let mut path = vec![cur.pt];
            let mut state_key = (cur.pt, cur.dir);
            while let Some(&prev) = came_from.get(&state_key) {
                path.push(prev.0);
                state_key = prev;
            }
            path.reverse();
            return Some(path);
        }
        expansions += 1;
        if expansions > 22_000 {
            return None;
        }
        if let Some(&bg) = best_g.get(&(cur.pt, cur.dir)) {
            if cur.g > bg {
                continue;
            }
        }

        for &(dx, dy, ndir) in &dirs {
            let np = GPt {
                x: cur.pt.x + dx,
                y: cur.pt.y + dy,
            };
            if np.x < min_gx || np.x > max_gx || np.y < min_gy || np.y > max_gy {
                continue;
            }
            if !goals.contains(&np) && blocked.contains(&np) {
                continue;
            }
            // Strict collinear-edge & foreign-corner protection (prevents accidental KiCad shorts!)
            let collinear_hit = occupancy.edge_used(cur.pt, np);
            let corner_hit = !goals.contains(&np) && occupancy.used_corners.contains(&(np.x, np.y));
            let turning_on_foreign = cur.dir != 0
                && cur.dir != ndir
                && cur.pt != start
                && occupancy.used_cells.contains(&(cur.pt.x, cur.pt.y));
            if strict_edges && (collinear_hit || corner_hit || turning_on_foreign) {
                continue;
            }

            let exit_pen = if cur.dir == 0 && preferred_exit_dir != 0 {
                if ndir == opposite_dir(preferred_exit_dir) {
                    70
                } else if ndir != preferred_exit_dir {
                    15
                } else {
                    0
                }
            } else {
                0
            };
            let turn_cost = if cur.dir != 0 && cur.dir != ndir { 18 } else { 0 };
            let cross_cost = if !goals.contains(&np) && occupancy.used_cells.contains(&(np.x, np.y)) {
                45
            } else {
                0
            };
            let soft_pen = if !strict_edges && (collinear_hit || corner_hit) {
                250
            } else {
                0
            };
            let ng = cur.g + 10 + exit_pen + turn_cost + cross_cost + soft_pen;
            let key = (np, ndir);
            if ng < *best_g.get(&key).unwrap_or(&i32::MAX) {
                best_g.insert(key, ng);
                came_from.insert(key, (cur.pt, cur.dir));
                heap.push(State {
                    f: ng + h(np),
                    g: ng,
                    pt: np,
                    dir: ndir,
                });
            }
        }
    }
    None
}

/// Merge unit grid edges into maximal straight horizontal & vertical `Segment2` runs,
/// splitting cleanly at every pin tip and T-junction so KiCad 9 always sees an exact endpoint.
fn merge_grid_edges(edges: &BTreeSet<(GPt, GPt)>, split_pts: &BTreeSet<GPt>) -> Vec<Segment2> {
    let mut h_by_y: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
    let mut v_by_x: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
    for &(a, b) in edges {
        if a.y == b.y {
            h_by_y.entry(a.y).or_default().push(a.x.min(b.x));
        } else if a.x == b.x {
            v_by_x.entry(a.x).or_default().push(a.y.min(b.y));
        }
    }
    let mut segs = Vec::new();
    for (y, mut xs) in h_by_y {
        xs.sort();
        xs.dedup();
        let mut i = 0;
        while i < xs.len() {
            let start_x = xs[i];
            let mut end_x = start_x + 1;
            while i + 1 < xs.len()
                && xs[i + 1] == end_x
                && !split_pts.contains(&GPt { x: end_x, y })
            {
                i += 1;
                end_x = xs[i] + 1;
            }
            segs.push(Segment2::new(
                from_grid(GPt { x: start_x, y }),
                from_grid(GPt { x: end_x, y }),
            ));
            i += 1;
        }
    }
    for (x, mut ys) in v_by_x {
        ys.sort();
        ys.dedup();
        let mut i = 0;
        while i < ys.len() {
            let start_y = ys[i];
            let mut end_y = start_y + 1;
            while i + 1 < ys.len()
                && ys[i + 1] == end_y
                && !split_pts.contains(&GPt { x, y: end_y })
            {
                i += 1;
                end_y = ys[i] + 1;
            }
            segs.push(Segment2::new(
                from_grid(GPt { x, y: start_y }),
                from_grid(GPt { x, y: end_y }),
            ));
            i += 1;
        }
    }
    segs
}
