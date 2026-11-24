use serde::{Deserialize, Serialize};

pub const EPS_MM: f64 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn dist(self, other: Point2) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Segment2 {
    pub p0: Point2,
    pub p1: Point2,
}

impl Segment2 {
    pub fn new(p0: Point2, p1: Point2) -> Self {
        Self { p0, p1 }
    }

    pub fn length(self) -> f64 {
        self.p0.dist(self.p1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect2 {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Rect2 {
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn depth(self) -> f64 {
        self.max_y - self.min_y
    }

    pub fn area_mm2(self) -> f64 {
        self.width().max(0.0) * self.depth().max(0.0)
    }

    pub fn area_m2(self) -> f64 {
        self.area_mm2() / 1_000_000.0
    }

    pub fn center(self) -> Point2 {
        Point2::new((self.min_x + self.max_x) * 0.5, (self.min_y + self.max_y) * 0.5)
    }

    /// Strict interior overlap (touching edges do NOT count as overlap).
    pub fn overlaps_strict(self, other: Rect2) -> bool {
        let ox = self.max_x.min(other.max_x) - self.min_x.max(other.min_x);
        let oy = self.max_y.min(other.max_y) - self.min_y.max(other.min_y);
        ox > EPS_MM && oy > EPS_MM
    }

    /// Overlap area in mm^2.
    pub fn overlap_area(self, other: Rect2) -> f64 {
        let ox = (self.max_x.min(other.max_x) - self.min_x.max(other.min_x)).max(0.0);
        let oy = (self.max_y.min(other.max_y) - self.min_y.max(other.min_y)).max(0.0);
        ox * oy
    }

    pub fn contains_point_strict(self, p: Point2) -> bool {
        p.x > self.min_x + EPS_MM
            && p.x < self.max_x - EPS_MM
            && p.y > self.min_y + EPS_MM
            && p.y < self.max_y - EPS_MM
    }

    pub fn contains_rect(self, inner: Rect2) -> bool {
        inner.min_x >= self.min_x - EPS_MM
            && inner.max_x <= self.max_x + EPS_MM
            && inner.min_y >= self.min_y - EPS_MM
            && inner.max_y <= self.max_y + EPS_MM
    }
}

/// Exact analytical circular arc (authoritative — never tessellated in IR).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Arc2 {
    pub center: Point2,
    pub radius: f64,
    /// Start angle in degrees CCW from +X (east).
    pub start_deg: f64,
    /// Signed sweep angle in degrees (+90 = CCW, -90 = CW).
    pub sweep_deg: f64,
}

impl Arc2 {
    pub fn start_point(self) -> Point2 {
        let a = self.start_deg.to_radians();
        Point2::new(
            self.center.x + self.radius * a.cos(),
            self.center.y + self.radius * a.sin(),
        )
    }

    pub fn end_point(self) -> Point2 {
        let a = (self.start_deg + self.sweep_deg).to_radians();
        Point2::new(
            self.center.x + self.radius * a.cos(),
            self.center.y + self.radius * a.sin(),
        )
    }

    /// Axis-aligned bounding box of a 90-degree axis-aligned door-swing quarter-disk.
    pub fn quarter_disk_bbox(self) -> Rect2 {
        let p0 = self.center;
        let p1 = self.start_point();
        let p2 = self.end_point();
        Rect2::new(
            p0.x.min(p1.x).min(p2.x),
            p0.y.min(p1.y).min(p2.y),
            p0.x.max(p1.x).max(p2.x),
            p0.y.max(p1.y).max(p2.y),
        )
    }
}

/// Exact 90-degree axis-aligned quarter-disk swept by a door leaf.
/// A point (x, y) is strictly inside the quarter-disk iff:
///   (x, y) is strictly inside `bbox` AND (x - cx)^2 + (y - cy)^2 < r^2.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QuarterDisk2 {
    pub arc: Arc2,
    pub bbox: Rect2,
}

impl QuarterDisk2 {
    pub fn from_arc(arc: Arc2) -> Self {
        let bbox = arc.quarter_disk_bbox();
        Self { arc, bbox }
    }

    /// Exact analytical test: do two 90-degree axis-aligned quarter-disks have strictly positive overlap area?
    /// Uses exact critical-y sweep across the intersection of their bounding boxes and disk boundaries.
    pub fn overlaps_quarter_disk(self, other: QuarterDisk2) -> bool {
        let bx0 = self.bbox.min_x.max(other.bbox.min_x);
        let by0 = self.bbox.min_y.max(other.bbox.min_y);
        let bx1 = self.bbox.max_x.min(other.bbox.max_x);
        let by1 = self.bbox.max_y.min(other.bbox.max_y);
        if bx1 - bx0 <= EPS_MM || by1 - by0 <= EPS_MM {
            return false;
        }

        let c1 = self.arc.center;
        let r1 = self.arc.radius;
        let c2 = other.arc.center;
        let r2 = other.arc.radius;
        let d = c1.dist(c2);
        if d >= r1 + r2 - EPS_MM {
            return false;
        }

        let mut crit_y = vec![by0, by1];
        for (c, r) in [(c1, r1), (c2, r2)] {
            for y in [c.y - r, c.y, c.y + r] {
                if y > by0 + EPS_MM && y < by1 - EPS_MM {
                    crit_y.push(y);
                }
            }
            for bx in [bx0, bx1] {
                let rem = r * r - (bx - c.x).powi(2);
                if rem > 0.0 {
                    let sy = rem.sqrt();
                    for y in [c.y - sy, c.y + sy] {
                        if y > by0 + EPS_MM && y < by1 - EPS_MM {
                            crit_y.push(y);
                        }
                    }
                }
            }
        }
        // Circle-circle intersection y-coordinates
        if d > EPS_MM && d > (r1 - r2).abs() - EPS_MM {
            let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
            let h_sq = r1 * r1 - a * a;
            if h_sq >= 0.0 {
                let h = h_sq.sqrt();
                let ux = (c2.x - c1.x) / d;
                let uy = (c2.y - c1.y) / d;
                let my = c1.y + a * uy;
                for s in [-1.0, 1.0] {
                    let iy = my + s * h * ux;
                    if iy > by0 + EPS_MM && iy < by1 - EPS_MM {
                        crit_y.push(iy);
                    }
                }
            }
        }

        crit_y.sort_by(|a, b| a.partial_cmp(b).unwrap());
        crit_y.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);

        for pair in crit_y.windows(2) {
            let ym = 0.5 * (pair[0] + pair[1]);
            let Some((x1a, x1b)) = circle_x_interval(c1, r1, ym) else {
                continue;
            };
            let Some((x2a, x2b)) = circle_x_interval(c2, r2, ym) else {
                continue;
            };
            let lo = bx0.max(x1a).max(x2a);
            let hi = bx1.min(x1b).min(x2b);
            if hi - lo > EPS_MM {
                return true;
            }
        }
        false
    }

    /// Exact analytical test: does a quarter-disk strictly intersect an axis-aligned rectangle (e.g., a wall)?
    pub fn overlaps_rect(self, rect: Rect2) -> bool {
        let bx0 = self.bbox.min_x.max(rect.min_x);
        let by0 = self.bbox.min_y.max(rect.min_y);
        let bx1 = self.bbox.max_x.min(rect.max_x);
        let by1 = self.bbox.max_y.min(rect.max_y);
        if bx1 - bx0 <= EPS_MM || by1 - by0 <= EPS_MM {
            return false;
        }
        // Closest point in intersection box [bx0,bx1] x [by0,by1] to disk center
        let c = self.arc.center;
        let qx = c.x.clamp(bx0, bx1);
        let qy = c.y.clamp(by0, by1);
        let dist = Point2::new(qx, qy).dist(c);
        dist < self.arc.radius - EPS_MM
    }
}

fn circle_x_interval(c: Point2, r: f64, y: f64) -> Option<(f64, f64)> {
    let dy = y - c.y;
    let rem = r * r - dy * dy;
    if rem <= 0.0 {
        return None;
    }
    let dx = rem.sqrt();
    Some((c.x - dx, c.x + dx))
}
