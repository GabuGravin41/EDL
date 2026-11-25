use crate::core::geom::{Point2, Rect2, Side};
use crate::core::ir::PlacedPinIr;
pub use crate::core::ir::SymPrim;

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
        "npn" | "bjt_npn" => build_bjt(body_rect, pins, cx, cy, true),
        "pnp" | "bjt_pnp" => build_bjt(body_rect, pins, cx, cy, false),
        "nmos" | "mosfet_n" => build_mosfet(body_rect, pins, cx, cy, true),
        "pmos" | "mosfet_p" => build_mosfet(body_rect, pins, cx, cy, false),
        "fuse" | "f" => build_fuse(body_rect, pins, cx, cy),
        "crystal" | "xtal" => build_crystal(body_rect, pins, cx, cy),
        "jumper" | "solder_jumper" | "jp" => build_jumper(body_rect, pins, cx, cy),
        "potentiometer" | "pot" | "trimmer" => build_potentiometer(body_rect, pins, cx, cy),
        "transformer" | "xfmr" => build_transformer(body_rect, pins, cx, cy),
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
    // Prioritize explicit `A`/`ANODE` and `K`/`CATHODE` names before falling back to role/order
    // so shunt TVS diodes (pin K: north, pin A: ground south) orient accurately!
    let explicit_anode = pins
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("A") || p.name.eq_ignore_ascii_case("ANODE"));
    let explicit_cathode = pins
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("K") || p.name.eq_ignore_ascii_case("CATHODE"));

    let anode_pin = explicit_anode.or_else(|| {
        pins.iter()
            .find(|p| explicit_cathode.map(|c| c.number != p.number).unwrap_or(true))
    });
    let cathode_pin = explicit_cathode.or_else(|| {
        pins.iter()
            .find(|p| {
                anode_pin.map(|a| a.number != p.number).unwrap_or(true)
                    && p.role == crate::domains::elec::ast::PinRole::Ground
            })
            .or_else(|| {
                pins.iter()
                    .find(|p| anode_pin.map(|a| a.number != p.number).unwrap_or(true))
            })
    });

    let vert = is_vertical_pair(pins);
    if vert {
        let half_h = 2.0;
        let half_w = 2.2;
        // Point UP if anode is physically below cathode (Y-down: anode.y > cathode.y), else DOWN!
        let point_up = match (anode_pin, cathode_pin) {
            (Some(ap), Some(kp)) => ap.body_point.y > kp.body_point.y,
            _ => false,
        };
        let (y_a, y_k) = if point_up {
            (cy + half_h, cy - half_h)
        } else {
            (cy - half_h, cy + half_h)
        };
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
        // Point LEFT if anode is physically to the right of cathode (anode.x > cathode.x), else RIGHT!
        let point_left = match (anode_pin, cathode_pin) {
            (Some(ap), Some(kp)) => ap.body_point.x > kp.body_point.x + 0.5,
            _ => false,
        };
        let (x_a, x_k) = if point_left {
            (cx + half_w, cx - half_w)
        } else {
            (cx - half_w, cx + half_w)
        };
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
            // If cathode is on South (common in LED-to-GND), route neatly out then down to South body_point
            if kp.side == Side::South {
                let drop_x = if point_left {
                    (x_k - 1.2).max(body.min_x + 0.6)
                } else {
                    (x_k + 1.2).min(body.max_x - 0.6)
                };
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

fn build_bjt(body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64, is_npn: bool) -> SymbolGraphic {
    let mut prims = Vec::new();
    let r = (body.width().min(body.depth()) * 0.45).clamp(2.5, 4.2);
    prims.push(SymPrim::Circle {
        center: Point2::new(cx, cy),
        radius: r,
        filled: false,
        stroke_mm: 0.25,
    });
    let bar_x = cx - r * 0.32;
    prims.push(line(Point2::new(bar_x, cy - r * 0.62), Point2::new(bar_x, cy + r * 0.62), 0.50));
    let c_pt = Point2::new(cx + r * 0.45, cy - r * 0.72);
    let e_pt = Point2::new(cx + r * 0.45, cy + r * 0.72);
    prims.push(line(Point2::new(bar_x, cy - r * 0.28), c_pt, 0.28));
    prims.push(line(Point2::new(bar_x, cy + r * 0.28), e_pt, 0.28));
    // Emitter arrow (outward for NPN, inward for PNP)
    let (a_tip, a_b1, a_b2) = if is_npn {
        (
            Point2::new(bar_x + r * 0.62, cy + r * 0.60),
            Point2::new(bar_x + r * 0.32, cy + r * 0.56),
            Point2::new(bar_x + r * 0.46, cy + r * 0.34),
        )
    } else {
        (
            Point2::new(bar_x + r * 0.14, cy + r * 0.34),
            Point2::new(bar_x + r * 0.44, cy + r * 0.36),
            Point2::new(bar_x + r * 0.30, cy + r * 0.58),
        )
    };
    prims.push(SymPrim::Polyline {
        pts: vec![a_tip, a_b1, a_b2, a_tip],
        filled: true,
        stroke_mm: 0.20,
    });
    for p in pins {
        let n = p.name.to_uppercase();
        let target = if n == "B" || n.contains("BASE") || p.side == Side::West {
            Point2::new(bar_x, cy)
        } else if n == "C" || n.contains("COL") || p.side == Side::North {
            c_pt
        } else {
            e_pt
        };
        connect_lead(&mut prims, p.body_point, target, p.side);
    }
    SymbolGraphic {
        primitives: prims,
        hide_pin_names: true,
    }
}

fn build_mosfet(body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64, is_nmos: bool) -> SymbolGraphic {
    let mut prims = Vec::new();
    let r = (body.width().min(body.depth()) * 0.45).clamp(2.5, 4.2);
    prims.push(SymPrim::Circle {
        center: Point2::new(cx, cy),
        radius: r,
        filled: false,
        stroke_mm: 0.25,
    });
    let gx = cx - r * 0.38;
    let ch_x = cx - r * 0.10;
    let dx = cx + r * 0.45;
    prims.push(line(Point2::new(gx, cy - r * 0.55), Point2::new(gx, cy + r * 0.55), 0.35));
    // Split channel segments
    for dy in [-r * 0.48, 0.0, r * 0.48] {
        prims.push(line(Point2::new(ch_x, cy + dy - 0.45), Point2::new(ch_x, cy + dy + 0.45), 0.38));
    }
    let d_pt = Point2::new(dx, cy - r * 0.55);
    let s_pt = Point2::new(dx, cy + r * 0.55);
    prims.push(line(Point2::new(ch_x, cy - r * 0.48), d_pt, 0.26));
    prims.push(line(Point2::new(ch_x, cy + r * 0.48), s_pt, 0.26));
    prims.push(line(Point2::new(ch_x, cy), Point2::new(dx, cy), 0.24));
    prims.push(line(Point2::new(dx, cy), s_pt, 0.24));
    let arrow_x = if is_nmos { ch_x + 0.3 } else { dx - 0.3 };
    let arrow_base = if is_nmos { ch_x + 1.1 } else { dx - 1.1 };
    prims.push(SymPrim::Polyline {
        pts: vec![
            Point2::new(arrow_x, cy),
            Point2::new(arrow_base, cy - 0.45),
            Point2::new(arrow_base, cy + 0.45),
            Point2::new(arrow_x, cy),
        ],
        filled: true,
        stroke_mm: 0.18,
    });
    for p in pins {
        let n = p.name.to_uppercase();
        let target = if n == "G" || n.contains("GATE") || p.side == Side::West {
            Point2::new(gx, cy)
        } else if n == "D" || n.contains("DRAIN") || p.side == Side::North {
            d_pt
        } else {
            s_pt
        };
        connect_lead(&mut prims, p.body_point, target, p.side);
    }
    SymbolGraphic {
        primitives: prims,
        hide_pin_names: true,
    }
}

fn build_fuse(body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64) -> SymbolGraphic {
    let mut prims = Vec::new();
    let vertical = is_vertical_pair(pins);
    if vertical {
        let h = (body.depth() * 0.65).clamp(4.0, 7.62);
        let w = 2.6;
        prims.push(SymPrim::Rect {
            rect: Rect2::new(cx - w * 0.5, cy - h * 0.5, cx + w * 0.5, cy + h * 0.5),
            filled: false,
            stroke_mm: 0.28,
        });
        prims.push(line(Point2::new(cx, body.min_y), Point2::new(cx, body.max_y), 0.26));
    } else {
        let w = (body.width() * 0.65).clamp(4.0, 7.62);
        let h = 2.6;
        prims.push(SymPrim::Rect {
            rect: Rect2::new(cx - w * 0.5, cy - h * 0.5, cx + w * 0.5, cy + h * 0.5),
            filled: false,
            stroke_mm: 0.28,
        });
        prims.push(line(Point2::new(body.min_x, cy), Point2::new(body.max_x, cy), 0.26));
    }
    SymbolGraphic {
        primitives: prims,
        hide_pin_names: pins.len() <= 2,
    }
}

fn build_crystal(_body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64) -> SymbolGraphic {
    let mut prims = Vec::new();
    let vertical = is_vertical_pair(pins);
    if vertical {
        prims.push(SymPrim::Rect {
            rect: Rect2::new(cx - 2.0, cy - 0.9, cx + 2.0, cy + 0.9),
            filled: false,
            stroke_mm: 0.30,
        });
        prims.push(line(Point2::new(cx - 2.2, cy - 1.8), Point2::new(cx + 2.2, cy - 1.8), 0.32));
        prims.push(line(Point2::new(cx - 2.2, cy + 1.8), Point2::new(cx + 2.2, cy + 1.8), 0.32));
        for p in pins {
            let t = if p.side == Side::North { Point2::new(cx, cy - 1.8) } else { Point2::new(cx, cy + 1.8) };
            connect_lead(&mut prims, p.body_point, t, p.side);
        }
    } else {
        prims.push(SymPrim::Rect {
            rect: Rect2::new(cx - 0.9, cy - 2.0, cx + 0.9, cy + 2.0),
            filled: false,
            stroke_mm: 0.30,
        });
        prims.push(line(Point2::new(cx - 1.8, cy - 2.2), Point2::new(cx - 1.8, cy + 2.2), 0.32));
        prims.push(line(Point2::new(cx + 1.8, cy - 2.2), Point2::new(cx + 1.8, cy + 2.2), 0.32));
        for p in pins {
            let t = if p.side == Side::West { Point2::new(cx - 1.8, cy) } else { Point2::new(cx + 1.8, cy) };
            connect_lead(&mut prims, p.body_point, t, p.side);
        }
    }
    SymbolGraphic {
        primitives: prims,
        hide_pin_names: pins.len() <= 2,
    }
}

fn build_jumper(body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64) -> SymbolGraphic {
    let mut prims = Vec::new();
    let r = 0.7;
    let dx = (body.width() * 0.26).clamp(1.2, 2.5);
    prims.push(SymPrim::Circle { center: Point2::new(cx - dx, cy), radius: r, filled: true, stroke_mm: 0.25 });
    prims.push(SymPrim::Circle { center: Point2::new(cx + dx, cy), radius: r, filled: true, stroke_mm: 0.25 });
    prims.push(SymPrim::Polyline {
        pts: vec![
            Point2::new(cx - dx, cy - 0.4),
            Point2::new(cx, cy - 1.5),
            Point2::new(cx + dx, cy - 0.4),
        ],
        filled: false,
        stroke_mm: 0.28,
    });
    for (i, p) in pins.iter().enumerate() {
        let t = if p.side == Side::West || i == 0 {
            Point2::new(cx - dx, cy)
        } else {
            Point2::new(cx + dx, cy)
        };
        connect_lead(&mut prims, p.body_point, t, p.side);
    }
    SymbolGraphic {
        primitives: prims,
        hide_pin_names: pins.len() <= 2,
    }
}

fn build_potentiometer(body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64) -> SymbolGraphic {
    let mut g = build_resistor(body, pins, cx, cy);
    // Add wiper arrow pointing into the resistor center
    let wiper_pin = pins.iter().find(|p| {
        let n = p.name.to_uppercase();
        n == "W" || n.contains("WIPER") || n == "2"
    });
    if let Some(wp) = wiper_pin {
        let target = Point2::new(cx, cy);
        connect_lead(&mut g.primitives, wp.body_point, target, wp.side);
    }
    g
}

fn build_transformer(body: Rect2, pins: &[PlacedPinIr], cx: f64, cy: f64) -> SymbolGraphic {
    let mut prims = Vec::new();
    // Dual magnetic core bars in center separating primary (West) and secondary (East)
    let h = body.depth() * 0.78;
    prims.push(line(Point2::new(cx - 0.6, cy - h * 0.5), Point2::new(cx - 0.6, cy + h * 0.5), 0.34));
    prims.push(line(Point2::new(cx + 0.6, cy - h * 0.5), Point2::new(cx + 0.6, cy + h * 0.5), 0.34));
    // Draw coil scallops on West and East faces
    for side_x in [cx - 2.4, cx + 2.4] {
        let is_west = side_x < cx;
        let dir = if is_west { 1.0 } else { -1.0 };
        let loops = 4usize;
        let y0 = cy - h * 0.45;
        let step = (h * 0.90) / loops as f64;
        for i in 0..loops {
            let ya = y0 + i as f64 * step;
            let ym = ya + step * 0.5;
            let yb = ya + step;
            prims.push(SymPrim::Polyline {
                pts: vec![
                    Point2::new(side_x, ya),
                    Point2::new(side_x + dir * 1.1, ya + step * 0.22),
                    Point2::new(side_x + dir * 1.3, ym),
                    Point2::new(side_x + dir * 1.1, yb - step * 0.22),
                    Point2::new(side_x, yb),
                ],
                filled: false,
                stroke_mm: 0.28,
            });
        }
    }
    for p in pins {
        let tx = if p.side == Side::West { cx - 2.4 } else { cx + 2.4 };
        connect_lead(&mut prims, p.body_point, Point2::new(tx, p.body_point.y), p.side);
    }
    SymbolGraphic {
        primitives: prims,
        hide_pin_names: false,
    }
}
