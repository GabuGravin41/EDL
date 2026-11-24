use crate::core::geom::{Point2, Rect2, Side};
use crate::core::ir::PlacedPinIr;

/// Vector primitives for IEEE/IEC electronic schematic symbols,
/// expressed in world schematic mm (Y-down, same frame as `body_rect` and `PlacedPinIr`).
/// Both `elec/svg.rs` and `elec/kicad.rs` render directly from this shared geometry,
/// ensuring identical, grid-accurate symbol appearance with zero ERC drift.
#[derive(Debug, Clone)]
pub enum SymPrim {
    /// Open or closed polyline in world mm.
    Polyline {
        pts: Vec<Point2>,
        filled: bool,
        stroke_mm: f64,
    },
    /// Rectangle in world mm.
    Rect {
        rect: Rect2,
        filled: bool,
        stroke_mm: f64,
    },
    /// Circle in world mm.
    Circle {
        center: Point2,
        radius: f64,
        filled: bool,
        stroke_mm: f64,
    },
    /// Small symbol-internal glyph label (e.g. `+` / `-` on polarized cap or op-amp).
    Mark {
        pos: Point2,
        text: String,
    },
}

#[derive(Debug, Clone)]
pub struct SymbolGraphic {
    pub primitives: Vec<SymPrim>,
    /// True for 2-pin passives (R, C, L, D, LED, SW) where internal pin-name text would clutter the symbol.
    pub hide_pin_names: bool,
}

/// Generate standard IEEE/IEC vector symbol geometry cleanly inside `body_rect`,
/// connecting every pin's `body_point` to the symbol terminals without moving any pin tip.
pub fn build_symbol_graphic(
    category: &str,
    body_rect: Rect2,
    pins: &[PlacedPinIr],
) -> SymbolGraphic {
    let cat = category.to_lowercase();
    let cx = body_rect.center().x;
    let cy = body_rect.center().y;

    match cat.as_str() {
        "resistor" | "r" => build_resistor(body_rect, pins, cx, cy),
        "capacitor" | "c" => build_capacitor(body_rect, pins, cx, cy),
        "inductor" | "l" => build_inductor(body_rect, pins, cx, cy),
        "diode" | "d" | "zener" | "schottky" => build_diode(body_rect, pins, cx, cy, false),
        "led" => build_diode(body_rect, pins, cx, cy, true),
        "switch" | "button" | "sw" => build_switch(body_rect, pins, cx, cy),
        "opamp" | "op_amp" | "comparator" => build_opamp(body_rect, pins, cx, cy),
        _ => SymbolGraphic {
            primitives: vec![SymPrim::Rect {
                rect: body_rect,
                filled: true,
                stroke_mm: 0.30,
            }],
            hide_pin_names: false,
        },
    }
}

fn is_vertical_pair(pins: &[PlacedPinIr]) -> bool {
    let ns = pins
        .iter()
        .filter(|p| matches!(p.side, Side::North | Side::South))
        .count();
    let ew = pins
        .iter()
        .filter(|p| matches!(p.side, Side::East | Side::West))
        .count();
    ns > ew
}

fn line(p0: Point2, p1: Point2, stroke_mm: f64) -> SymPrim {
    SymPrim::Polyline {
        pts: vec![p0, p1],
        filled: false,
        stroke_mm,
    }
}

/// Connect an arbitrary pin `body_point` cleanly to an internal symbol terminal using an orthogonal stub.
fn connect_lead(prims: &mut Vec<SymPrim>, body_pt: Point2, term: Point2, side: Side) {
    if body_pt.dist(term) < 1e-4 {
        return;
    }
    if (body_pt.x - term.x).abs() < 1e-4 || (body_pt.y - term.y).abs() < 1e-4 {
        prims.push(line(body_pt, term, 0.28));
    } else {
        // L-shaped orthogonal internal lead respecting pin entry direction
        let elbow = match side {
            Side::West | Side::East => Point2::new(term.x, body_pt.y),
            Side::North | Side::South => Point2::new(body_pt.x, term.y),
        };
        prims.push(SymPrim::Polyline {
            pts: vec![body_pt, elbow, term],
            filled: false,
            stroke_mm: 0.28,
        });
    }
}

fn build_resistor(body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64) -> SymbolGraphic {
    let mut prims = Vec::new();
    let vert = is_vertical_pair(pins);
    if vert {
        let half_len = (body.depth() * 0.34).clamp(2.8, 4.2);
        let amp = (body.width() * 0.22).clamp(1.2, 1.8);
        let y0 = cy - half_len;
        let y1 = cy + half_len;
        let step = (y1 - y0) / 12.0;
        let pts = vec![
            Point2::new(cx, y0),
            Point2::new(cx + amp, y0 + 1.0 * step),
            Point2::new(cx - amp, y0 + 3.0 * step),
            Point2::new(cx + amp, y0 + 5.0 * step),
            Point2::new(cx - amp, y0 + 7.0 * step),
            Point2::new(cx + amp, y0 + 9.0 * step),
            Point2::new(cx - amp, y0 + 11.0 * step),
            Point2::new(cx, y1),
        ];
        prims.push(SymPrim::Polyline {
            pts,
            filled: false,
            stroke_mm: 0.32,
        });
        let t_top = Point2::new(cx, y0);
        let t_bot = Point2::new(cx, y1);
        for (i, p) in pins.iter().enumerate() {
            let term = match p.side {
                Side::North => t_top,
                Side::South => t_bot,
                _ => if i == 0 { t_top } else { t_bot },
            };
            connect_lead(&mut prims, p.body_point, term, p.side);
        }
    } else {
        let half_len = (body.width() * 0.34).clamp(2.8, 4.4);
        let amp = (body.depth() * 0.22).clamp(1.2, 1.8);
        let x0 = cx - half_len;
        let x1 = cx + half_len;
        let step = (x1 - x0) / 12.0;
        let pts = vec![
            Point2::new(x0, cy),
            Point2::new(x0 + 1.0 * step, cy - amp),
            Point2::new(x0 + 3.0 * step, cy + amp),
            Point2::new(x0 + 5.0 * step, cy - amp),
            Point2::new(x0 + 7.0 * step, cy + amp),
            Point2::new(x0 + 9.0 * step, cy - amp),
            Point2::new(x0 + 11.0 * step, cy + amp),
            Point2::new(x1, cy),
        ];
        prims.push(SymPrim::Polyline {
            pts,
            filled: false,
            stroke_mm: 0.32,
        });
        let t_l = Point2::new(x0, cy);
        let t_r = Point2::new(x1, cy);
        for (i, p) in pins.iter().enumerate() {
            let term = match p.side {
                Side::West => t_l,
                Side::East => t_r,
                _ => if i == 0 { t_l } else { t_r },
            };
            connect_lead(&mut prims, p.body_point, term, p.side);
        }
    }
    SymbolGraphic {
        primitives: prims,
        hide_pin_names: pins.len() <= 2,
    }
}

fn build_capacitor(body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64) -> SymbolGraphic {
    let mut prims = Vec::new();
    let vert = is_vertical_pair(pins);
    let has_pos = pins
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case("POS") || p.name == "+");

    if vert {
        let gap = 0.95;
        let half_w = (body.width() * 0.36).clamp(2.2, 3.6);
        let y_top = cy - gap;
        let y_bot = cy + gap;
        prims.push(line(Point2::new(cx - half_w, y_top), Point2::new(cx + half_w, y_top), 0.42));
        prims.push(line(Point2::new(cx - half_w, y_bot), Point2::new(cx + half_w, y_bot), 0.42));
        if has_pos {
            prims.push(SymPrim::Mark {
                pos: Point2::new(cx - half_w + 0.6, y_top - 1.4),
                text: "+".to_string(),
            });
        }
        let t_top = Point2::new(cx, y_top);
        let t_bot = Point2::new(cx, y_bot);
        for (i, p) in pins.iter().enumerate() {
            let term = match p.side {
                Side::North => t_top,
                Side::South => t_bot,
                _ => if i == 0 { t_top } else { t_bot },
            };
            connect_lead(&mut prims, p.body_point, term, p.side);
        }
    } else {
        let gap = 0.95;
        let half_h = (body.depth() * 0.36).clamp(2.2, 3.6);
        let x_l = cx - gap;
        let x_r = cx + gap;
        prims.push(line(Point2::new(x_l, cy - half_h), Point2::new(x_l, cy + half_h), 0.42));
        prims.push(line(Point2::new(x_r, cy - half_h), Point2::new(x_r, cy + half_h), 0.42));
        if has_pos {
            prims.push(SymPrim::Mark {
                pos: Point2::new(x_l - 1.5, cy - half_h + 0.6),
                text: "+".to_string(),
            });
        }
        let t_l = Point2::new(x_l, cy);
        let t_r = Point2::new(x_r, cy);
        for (i, p) in pins.iter().enumerate() {
            let term = match p.side {
                Side::West => t_l,
                Side::East => t_r,
                _ => if i == 0 { t_l } else { t_r },
            };
            connect_lead(&mut prims, p.body_point, term, p.side);
        }
    }

    SymbolGraphic {
        primitives: prims,
        hide_pin_names: pins.len() <= 2,
    }
}

fn build_inductor(body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64) -> SymbolGraphic {
    let mut prims = Vec::new();
    let vert = is_vertical_pair(pins);
    let lobes = 4usize;

    if vert {
        let half_len = (body.depth() * 0.36).clamp(3.0, 4.4);
        let y0 = cy - half_len;
        let y1 = cy + half_len;
        let lobe_h = (y1 - y0) / (lobes as f64);
        let r = lobe_h * 0.5;
        let mut pts = Vec::new();
        for i in 0..lobes {
            let ly_c = y0 + (i as f64 + 0.5) * lobe_h;
            for step in 0..=8 {
                let ang = -std::f64::consts::FRAC_PI_2 + (step as f64 / 8.0) * std::f64::consts::PI;
                pts.push(Point2::new(cx + r * 1.15 * ang.cos(), ly_c + r * ang.sin()));
            }
        }
        prims.push(SymPrim::Polyline { pts, filled: false, stroke_mm: 0.32 });
        let t_top = Point2::new(cx, y0);
        let t_bot = Point2::new(cx, y1);
        for (i, p) in pins.iter().enumerate() {
            let term = match p.side {
                Side::North => t_top,
                Side::South => t_bot,
                _ => if i == 0 { t_top } else { t_bot },
            };
            connect_lead(&mut prims, p.body_point, term, p.side);
        }
    } else {
        let half_len = (body.width() * 0.36).clamp(3.0, 4.4);
        let x0 = cx - half_len;
        let x1 = cx + half_len;
        let lobe_w = (x1 - x0) / (lobes as f64);
        let r = lobe_w * 0.5;
        let mut pts = Vec::new();
        for i in 0..lobes {
            let lx_c = x0 + (i as f64 + 0.5) * lobe_w;
            for step in 0..=8 {
                let ang = std::f64::consts::PI - (step as f64 / 8.0) * std::f64::consts::PI;
                pts.push(Point2::new(lx_c + r * ang.cos(), cy - r * 1.15 * ang.sin()));
            }
        }
        prims.push(SymPrim::Polyline { pts, filled: false, stroke_mm: 0.32 });
        let t_l = Point2::new(x0, cy);
        let t_r = Point2::new(x1, cy);
        for (i, p) in pins.iter().enumerate() {
            let term = match p.side {
                Side::West => t_l,
                Side::East => t_r,
                _ => if i == 0 { t_l } else { t_r },
            };
            connect_lead(&mut prims, p.body_point, term, p.side);
        }
    }

    SymbolGraphic {
        primitives: prims,
        hide_pin_names: pins.len() <= 2,
    }
}

fn build_diode(
    body: Rect2,
    pins: &[PlacedPinIr],
    cx: f64,
    cy: f64,
    is_led: bool,
) -> SymbolGraphic {
    let mut prims = Vec::new();
    // Detect cathode pin (`K`, `CATHODE`, or Ground-role pin) vs anode pin (`A`, `ANODE`, or first pin)
    let anode_pin = pins
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("A") || p.name.eq_ignore_ascii_case("ANODE"))
        .or_else(|| pins.first());
    let cathode_pin = pins
        .iter()
        .find(|p| {
            p.name.eq_ignore_ascii_case("K")
                || p.name.eq_ignore_ascii_case("CATHODE")
                || p.role == crate::domains::elec::ast::PinRole::Ground
        })
        .or_else(|| pins.get(1));

    let vert = is_vertical_pair(pins);
    if vert {
        let half_h = 2.0;
        let half_w = 2.2;
        let y_a = cy - half_h;
        let y_k = cy + half_h;
        // Triangle pointing down (Y-down: anode top, cathode bottom)
        prims.push(SymPrim::Polyline {
            pts: vec![
                Point2::new(cx - half_w, y_a),
                Point2::new(cx + half_w, y_a),
                Point2::new(cx, y_k),
                Point2::new(cx - half_w, y_a),
            ],
            filled: true,
            stroke_mm: 0.32,
        });
        // Cathode bar
        prims.push(line(Point2::new(cx - half_w, y_k), Point2::new(cx + half_w, y_k), 0.40));
        if is_led {
            add_led_arrows(&mut prims, Point2::new(cx + half_w + 0.6, cy - 0.6), 1.0, -0.7);
            add_led_arrows(&mut prims, Point2::new(cx + half_w + 0.6, cy + 1.0), 1.0, -0.7);
        }
        if let Some(ap) = anode_pin {
            connect_lead(&mut prims, ap.body_point, Point2::new(cx, y_a), ap.side);
        }
        if let Some(kp) = cathode_pin {
            connect_lead(&mut prims, kp.body_point, Point2::new(cx, y_k), kp.side);
        }
    } else {
        let half_w = 2.0;
        let half_h = 2.1;
        let x_a = cx - half_w;
        let x_k = cx + half_w;
        // Triangle pointing right (anode left, cathode right)
        prims.push(SymPrim::Polyline {
            pts: vec![
                Point2::new(x_a, cy - half_h),
                Point2::new(x_a, cy + half_h),
                Point2::new(x_k, cy),
                Point2::new(x_a, cy - half_h),
            ],
            filled: true,
            stroke_mm: 0.32,
        });
        // Cathode bar
        prims.push(line(Point2::new(x_k, cy - half_h), Point2::new(x_k, cy + half_h), 0.40));
        if is_led {
            add_led_arrows(&mut prims, Point2::new(cx - 0.5, cy - half_h - 0.5), 0.75, -0.95);
            add_led_arrows(&mut prims, Point2::new(cx + 1.1, cy - half_h - 0.5), 0.75, -0.95);
        }
        if let Some(ap) = anode_pin {
            connect_lead(&mut prims, ap.body_point, Point2::new(x_a, cy), ap.side);
        }
        if let Some(kp) = cathode_pin {
            // If cathode is on South (common in LED-to-GND), route neatly out right then down to South body_point
            if kp.side == Side::South {
                let drop_x = (x_k + 1.2).min(body.max_x - 0.6);
                let drop_y = (body.max_y - 1.0).max(cy + 1.2);
                prims.push(SymPrim::Polyline {
                    pts: vec![
                        Point2::new(x_k, cy),
                        Point2::new(drop_x, cy),
                        Point2::new(drop_x, drop_y),
                        Point2::new(kp.body_point.x, drop_y),
                        kp.body_point,
                    ],
                    filled: false,
                    stroke_mm: 0.28,
                });
            } else {
                connect_lead(&mut prims, kp.body_point, Point2::new(x_k, cy), kp.side);
            }
        }
    }

    SymbolGraphic {
        primitives: prims,
        hide_pin_names: pins.len() <= 2,
    }
}

fn add_led_arrows(prims: &mut Vec<SymPrim>, start: Point2, dx: f64, dy: f64) {
    let len = 2.0;
    let norm = (dx * dx + dy * dy).sqrt().max(1e-6);
    let ux = dx / norm;
    let uy = dy / norm;
    let tip = Point2::new(start.x + ux * len, start.y + uy * len);
    prims.push(line(start, tip, 0.22));
    // Two tiny arrowhead wings
    let px = -uy;
    let py = ux;
    let back = Point2::new(tip.x - ux * 0.75, tip.y - uy * 0.75);
    let w1 = Point2::new(back.x + px * 0.42, back.y + py * 0.42);
    let w2 = Point2::new(back.x - px * 0.42, back.y - py * 0.42);
    prims.push(SymPrim::Polyline {
        pts: vec![w1, tip, w2],
        filled: false,
        stroke_mm: 0.22,
    });
}

fn build_switch(_body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64) -> SymbolGraphic {
    let mut prims = Vec::new();
    let vert = is_vertical_pair(pins);
    if vert {
        let y0 = cy - 2.4;
        let y1 = cy + 2.4;
        prims.push(SymPrim::Circle { center: Point2::new(cx, y0), radius: 0.65, filled: false, stroke_mm: 0.30 });
        prims.push(SymPrim::Circle { center: Point2::new(cx, y1), radius: 0.65, filled: false, stroke_mm: 0.30 });
        // Open lever arm + push bar
        prims.push(line(Point2::new(cx, y0 + 0.65), Point2::new(cx + 2.0, y1 - 0.6), 0.32));
        prims.push(line(Point2::new(cx + 1.6, cy), Point2::new(cx + 3.0, cy), 0.25));
        prims.push(line(Point2::new(cx + 3.0, cy - 1.2), Point2::new(cx + 3.0, cy + 1.2), 0.30));
        for (i, p) in pins.iter().enumerate() {
            let term = match p.side {
                Side::North => Point2::new(cx, y0 - 0.65),
                Side::South => Point2::new(cx, y1 + 0.65),
                _ => if i == 0 { Point2::new(cx, y0 - 0.65) } else { Point2::new(cx, y1 + 0.65) },
            };
            connect_lead(&mut prims, p.body_point, term, p.side);
        }
    } else {
        let x0 = cx - 2.6;
        let x1 = cx + 2.6;
        prims.push(SymPrim::Circle { center: Point2::new(x0, cy), radius: 0.65, filled: false, stroke_mm: 0.30 });
        prims.push(SymPrim::Circle { center: Point2::new(x1, cy), radius: 0.65, filled: false, stroke_mm: 0.30 });
        prims.push(line(Point2::new(x0 + 0.65, cy), Point2::new(x1 - 0.4, cy - 2.0), 0.32));
        prims.push(line(Point2::new(cx, cy - 1.3), Point2::new(cx, cy - 2.8), 0.25));
        prims.push(line(Point2::new(cx - 1.4, cy - 2.8), Point2::new(cx + 1.4, cy - 2.8), 0.30));
        for (i, p) in pins.iter().enumerate() {
            let term = match p.side {
                Side::West => Point2::new(x0 - 0.65, cy),
                Side::East => Point2::new(x1 + 0.65, cy),
                _ => if i == 0 { Point2::new(x0 - 0.65, cy) } else { Point2::new(x1 + 0.65, cy) },
            };
            connect_lead(&mut prims, p.body_point, term, p.side);
        }
    }
    SymbolGraphic {
        primitives: prims,
        hide_pin_names: pins.len() <= 2,
    }
}

fn build_opamp(body: Rect2, pins: &[PlacedPinIr], _cx: f64, cy: f64) -> SymbolGraphic {
    let mut prims = Vec::new();
    // Classic rightward-pointing triangle spanning body_rect
    prims.push(SymPrim::Polyline {
        pts: vec![
            Point2::new(body.min_x, body.min_y),
            Point2::new(body.min_x, body.max_y),
            Point2::new(body.max_x, cy),
            Point2::new(body.min_x, body.min_y),
        ],
        filled: true,
        stroke_mm: 0.32,
    });
    // Place + / - marks near West input pins
    for p in pins {
        if p.side == Side::West {
            let n = p.name.to_uppercase();
            let mark = if n.contains("POS") || n.contains("INP") || n.contains('+') {
                Some("+")
            } else if n.contains("NEG") || n.contains("INN") || n.contains('-') {
                Some("-")
            } else {
                None
            };
            if let Some(m) = mark {
                prims.push(SymPrim::Mark {
                    pos: Point2::new(body.min_x + 1.8, p.body_point.y + 0.5),
                    text: m.to_string(),
                });
            }
        }
    }
    SymbolGraphic {
        primitives: prims,
        hide_pin_names: true,
    }
}
