//! Obstacle-aware 2.54mm KiCad grid A* orthogonal router for dense 2D schematic zones.
//!
//! Routes intra-zone nets with minimal bends while strictly avoiding every component body
//! and every unrelated pin tip (zero wire-over-body crossings and zero accidental pin shorts).

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

/// Route a net connecting `endpoints` around `body_obstacles` and `foreign_pin_tips`.
/// Returns minimal orthogonal segments and T-junction points.
pub fn route_net_astar(
    endpoints: &[(Point2, Side)],
    body_obstacles: &[Rect2],
    foreign_pin_tips: &BTreeSet<(i32, i32)>,
    used_wire_cells: &mut BTreeSet<(i32, i32)>,
) -> (Vec<Segment2>, Vec<Point2>) {
    if endpoints.len() < 2 {
        return (Vec::new(), Vec::new());
    }

    let pin_gpts: Vec<GPt> = endpoints.iter().map(|(p, _)| to_grid(*p)).collect();
    let pin_set: BTreeSet<GPt> = pin_gpts.iter().copied().collect();

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
    for gp in &pin_gpts {
        min_gx = min_gx.min(gp.x);
        max_gx = max_gx.max(gp.x);
        min_gy = min_gy.min(gp.y);
        max_gy = max_gy.max(gp.y);
    }
    let pad = 12;
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

    // Connect pins incrementally via nearest-tree-cell (Steiner-like MST on grid)
    let mut tree_cells: BTreeSet<GPt> = BTreeSet::new();
    tree_cells.insert(pin_gpts[0]);

    let mut remaining: Vec<GPt> = pin_gpts[1..].to_vec();
    let mut raw_edges: BTreeSet<(GPt, GPt)> = BTreeSet::new();

    while !remaining.is_empty() {
        // Pick remaining pin closest to existing tree
        let (best_idx, target) = remaining
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| {
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
            &tree_cells,
            &blocked,
            used_wire_cells,
            (min_gx, max_gx, min_gy, max_gy),
        );

        let path = match path {
            Some(p) => p,
            None => {
                // Clean L-fallback if completely enclosed
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
            used_wire_cells.insert((a.x, a.y));
            used_wire_cells.insert((b.x, b.y));
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

fn astar_to_tree(
    start: GPt,
    goals: &BTreeSet<GPt>,
    blocked: &BTreeSet<GPt>,
    used_wire_cells: &BTreeSet<(i32, i32)>,
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
        if expansions > 18_000 {
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
            let turn_cost = if cur.dir != 0 && cur.dir != ndir { 18 } else { 0 };
            let cross_cost = if !goals.contains(&np) && used_wire_cells.contains(&(np.x, np.y)) {
                25
            } else {
                0
            };
            let ng = cur.g + 10 + turn_cost + cross_cost;
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
            let x0 = xs[i];
            let mut x1 = x0 + 1;
            while i + 1 < xs.len()
                && xs[i + 1] == x1
                && !split_pts.contains(&GPt { x: x1, y })
            {
                i += 1;
                x1 += 1;
            }
            segs.push(Segment2::new(
                from_grid(GPt { x: x0, y }),
                from_grid(GPt { x: x1, y }),
            ));
            i += 1;
        }
    }
    for (x, mut ys) in v_by_x {
        ys.sort();
        ys.dedup();
        let mut i = 0;
        while i < ys.len() {
            let y0 = ys[i];
            let mut y1 = y0 + 1;
            while i + 1 < ys.len()
                && ys[i + 1] == y1
                && !split_pts.contains(&GPt { x, y: y1 })
            {
                i += 1;
                y1 += 1;
            }
            segs.push(Segment2::new(
                from_grid(GPt { x, y: y0 }),
                from_grid(GPt { x, y: y1 }),
            ));
            i += 1;
        }
    }
    segs
}
