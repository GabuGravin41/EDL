//! SVG Overlay, AutoCAD R12 Layered DXF, and Plumbing Takeoff Schedule for `mep.plumbing` (v1.4.0).

use crate::core::ir::*;
use crate::core::span::Severity;
use crate::domains::mep_plumbing::compile::PlumbingModel;
use crate::domains::registry::ViewTransform;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

pub fn extract_plumbing_models(ir: &CanonicalIr) -> Vec<PlumbingModel> {
    ir.entities
        .iter()
        .filter_map(|e| {
            if e.kind == "mep.plumbing.model" {
                if let EntityGeometryIr::Custom { data, .. } = &e.geometry {
                    return serde_json::from_value::<PlumbingModel>(data.clone()).ok();
                }
            }
            None
        })
        .collect()
}

/// Render the `mep.plumbing` SVG overlay in project model coordinates through `view`.
pub fn render_plumbing_overlay(ir: &CanonicalIr, view: &ViewTransform) -> Option<String> {
    let models = extract_plumbing_models(ir);
    if models.is_empty() {
        return None;
    }
    let violated: BTreeSet<&str> = ir
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .flat_map(|d| d.entities.iter().map(|s| s.as_str()))
        .collect();

    let mut out = String::new();
    let _ = writeln!(
        out,
        r##"    <style>
      .pl-cold {{ fill: none; stroke: #0284c7; stroke-width: 1.7; stroke-linejoin: round; stroke-linecap: round; }}
      .pl-hot  {{ fill: none; stroke: #dc2626; stroke-width: 1.6; stroke-dasharray: 6 3; stroke-linejoin: round; stroke-linecap: round; }}
      .pl-soil {{ fill: none; stroke: #7c2d12; stroke-width: 2.7; stroke-linejoin: round; stroke-linecap: round; }}
      .pl-waste {{ fill: none; stroke: #b45309; stroke-width: 1.9; stroke-linejoin: round; stroke-linecap: round; }}
      .pl-vent {{ fill: none; stroke: #15803d; stroke-width: 1.4; stroke-dasharray: 3 3; }}
      .pl-arrow {{ fill: #7c2d12; stroke: none; }}
      .pl-arrow-w {{ fill: #b45309; stroke: none; }}
      .pl-riser-c {{ fill: #0284c7; stroke: #0c4a6e; stroke-width: 1.3; }}
      .pl-stack {{ fill: #fef3c7; stroke: #7c2d12; stroke-width: 1.8; }}
      .pl-stack-in {{ fill: #7c2d12; stroke: none; }}
      .pl-heater {{ fill: #fee2e2; stroke: #b91c1c; stroke-width: 1.6; }}
      .pl-valve-c {{ fill: #bae6fd; stroke: #0369a1; stroke-width: 1.2; }}
      .pl-valve-h {{ fill: #fecaca; stroke: #b91c1c; stroke-width: 1.2; }}
      .pl-fd {{ fill: #fef3c7; stroke: #92400e; stroke-width: 1.3; }}
      .pl-err {{ fill: #fecaca; stroke: #dc2626; stroke-width: 2.4; }}
      .pl-lbl {{ font-family: monospace; font-size: 7.8px; font-weight: 700; fill: #1e293b; text-anchor: middle; }}
      .pl-lbl-bg {{ fill: #ffffff; fill-opacity: 0.86; stroke: none; }}
      .pl-callout {{ font-family: monospace; font-size: 7.4px; font-weight: 700; fill: #7c2d12; text-anchor: middle; }}
      .pl-leg-txt {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 8.5px; font-weight: 600; fill: #334155; }}
    </style>"##
    );

    for model in &models {
        // 1. Draw pipes: drainage first (underneath), then cold, then hot
        for layer_sys in ["soil", "waste", "vent", "cold", "hot"] {
            for pipe in model.pipes.iter().filter(|p| p.system == layer_sys) {
                if pipe.points.len() < 2 {
                    continue;
                }
                let cls = match pipe.system.as_str() {
                    "cold" => "pl-cold",
                    "hot" => "pl-hot",
                    "soil" => "pl-soil",
                    "waste" => "pl-waste",
                    _ => "pl-vent",
                };
                let mut d_attr = String::new();
                for (i, pt) in pipe.points.iter().enumerate() {
                    let (px, py) = view.map(pt[0], pt[1]);
                    if i == 0 {
                        let _ = write!(d_attr, "M {px:.2} {py:.2}");
                    } else {
                        let _ = write!(d_attr, " L {px:.2} {py:.2}");
                    }
                }
                let _ = writeln!(
                    out,
                    r#"    <path id="pipe-{}" class="{cls}" d="{d_attr}"/>"#,
                    pipe.id
                );

                // For drainage pipes (soil/waste), place a crisp flow-direction arrowhead & Ø/slope badge
                // on the longest segment!
                if pipe.system == "soil" || pipe.system == "waste" {
                    if let Some((p0, p1)) = longest_segment(&pipe.points) {
                        let (x0, y0) = view.map(p0[0], p0[1]);
                        let (x1, y1) = view.map(p1[0], p1[1]);
                        let (dx, dy) = (x1 - x0, y1 - y0);
                        let seg_len = (dx * dx + dy * dy).sqrt();
                        if seg_len > 22.0 {
                            let (ux, uy) = (dx / seg_len, dy / seg_len);
                            let (mx, my) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
                            // Arrowhead pointing in flow direction (from fixture -> stack)
                            let ax = mx + ux * 5.0;
                            let ay = my + uy * 5.0;
                            let bx = mx - ux * 4.0 - uy * 3.8;
                            let by = my - uy * 4.0 + ux * 3.8;
                            let cx = mx - ux * 4.0 + uy * 3.8;
                            let cy = my - uy * 4.0 - ux * 3.8;
                            let acls = if pipe.system == "soil" { "pl-arrow" } else { "pl-arrow-w" };
                            let _ = writeln!(
                                out,
                                r#"    <polygon class="{acls}" points="{ax:.1},{ay:.1} {bx:.1},{by:.1} {cx:.1},{cy:.1}"/>"#
                            );
                            if seg_len > 48.0 {
                                let label = if let Some(ref sl) = pipe.slope_ratio {
                                    format!("Ø{:.0} {sl}", pipe.dia_mm)
                                } else {
                                    format!("Ø{:.0}", pipe.dia_mm)
                                };
                                // Offset badge perpendicular to the pipe (into room space when stack has wall facing)
                                let dest_facing = model
                                    .nodes
                                    .iter()
                                    .find(|n| n.id == pipe.to_id)
                                    .and_then(|n| n.facing.as_deref());
                                let (nx, ny) = if ux.abs() < 0.3 {
                                    match dest_facing {
                                        Some("west") => (-24.0, 0.0),
                                        Some("east") => (24.0, 0.0),
                                        _ => (-uy * 14.0, 0.0),
                                    }
                                } else {
                                    (0.0, if ux > 0.0 { -10.0 } else { 12.0 })
                                };
                                let lx = mx + nx;
                                let ly = my + ny;
                                let bw = (label.len() as f64) * 4.6 + 6.0;
                                let _ = writeln!(
                                    out,
                                    r#"    <rect class="pl-lbl-bg" x="{:.1}" y="{:.1}" width="{bw:.1}" height="10" rx="2"/>"#,
                                    lx - bw * 0.5,
                                    ly - 7.5
                                );
                                let _ = writeln!(
                                    out,
                                    r#"    <text class="pl-callout" x="{lx:.1}" y="{ly:.1}">{label}</text>"#
                                );
                            }
                        }
                    }
                }
            }
        }

        // 2. Draw plumbing equipment & symbols (risers, water heaters, soil stacks, valves, floor drains)
        for n in &model.nodes {
            let (px, py) = view.map(n.pos[0], n.pos[1]);
            let is_err = violated.contains(n.id.as_str());
            match n.kind.as_str() {
                "cold_water" => {
                    let cls = if is_err { "pl-err" } else { "pl-riser-c" };
                    let _ = writeln!(out, r#"    <circle id="pl-{}" class="{cls}" cx="{px:.2}" cy="{py:.2}" r="6.0"/>"#, n.id);
                    let _ = writeln!(out, r##"    <circle fill="none" stroke="#ffffff" stroke-width="1.1" cx="{px:.2}" cy="{py:.2}" r="2.5"/>"##);
                    draw_pill_label(&mut out, px, py - 10.0, &format!("CW Ø{:.0}", n.dia_mm));
                }
                "soil_stack" | "vent_stack" => {
                    let cls = if is_err { "pl-err" } else { "pl-stack" };
                    let _ = writeln!(out, r#"    <circle id="pl-{}" class="{cls}" cx="{px:.2}" cy="{py:.2}" r="7.5"/>"#, n.id);
                    let _ = writeln!(out, r#"    <circle class="pl-stack-in" cx="{px:.2}" cy="{py:.2}" r="3.0"/>"#);
                    let tag = if n.kind == "soil_stack" { "SS" } else { "VS" };
                    let lx = match n.facing.as_deref() {
                        Some("west") => px - 14.0,
                        Some("east") => px + 14.0,
                        _ => px,
                    };
                    draw_pill_label(&mut out, lx, py - 11.5, &format!("{tag} Ø{:.0}", n.dia_mm));
                }
                "water_heater" => {
                    let cls = if is_err { "pl-err" } else { "pl-heater" };
                    let _ = writeln!(
                        out,
                        r#"    <rect id="pl-{}" class="{cls}" x="{:.2}" y="{:.2}" width="24" height="16" rx="6"/>"#,
                        n.id,
                        px - 12.0,
                        py - 8.0
                    );
                    let cap = n
                        .capacity_l
                        .map(|c| format!("{} {}L", n.id, c as i64))
                        .unwrap_or_else(|| n.id.clone());
                    draw_pill_label(&mut out, px, py - 12.0, &cap);
                }
                "isolation_valve" => {
                    let cls = if is_err {
                        "pl-err"
                    } else if n.system == "hot" {
                        "pl-valve-h"
                    } else {
                        "pl-valve-c"
                    };
                    // Standard ISO bowtie isolation valve symbol
                    let _ = writeln!(
                        out,
                        r#"    <polygon id="pl-{}" class="{cls}" points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}"/>"#,
                        n.id,
                        px - 5.5, py - 4.0,
                        px + 5.5, py + 4.0,
                        px + 5.5, py - 4.0,
                        px - 5.5, py + 4.0
                    );
                }
                "floor_drain" => {
                    let cls = if is_err { "pl-err" } else { "pl-fd" };
                    let _ = writeln!(
                        out,
                        r#"    <rect id="pl-{}" class="{cls}" x="{:.2}" y="{:.2}" width="11" height="11" rx="1.5"/>"#,
                        n.id,
                        px - 5.5,
                        py - 5.5
                    );
                    let _ = writeln!(out, r#"    <circle class="{cls}" cx="{px:.2}" cy="{py:.2}" r="3.0"/>"#);
                    draw_pill_label(&mut out, px, py + 14.5, "FD");
                }
                _ => {}
            }
        }
    }

    // 3. Compact Plumbing Legend bar in the top-left sheet header margin
    let lx = 32.0;
    let ly = 33.0;
    let _ = writeln!(
        out,
        r##"    <g id="plumbing-legend">
      <rect x="{:.1}" y="{:.1}" width="372" height="22" rx="4" fill="#ffffff" fill-opacity="0.93" stroke="#cbd5e1" stroke-width="1"/>
      <line class="pl-cold" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>
      <text class="pl-leg-txt" x="{:.1}" y="{:.1}">CWS (Cold)</text>
      <line class="pl-hot" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>
      <text class="pl-leg-txt" x="{:.1}" y="{:.1}">HWS (Hot)</text>
      <line class="pl-soil" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>
      <text class="pl-leg-txt" x="{:.1}" y="{:.1}">DWV Soil Ø110</text>
      <line class="pl-waste" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>
      <text class="pl-leg-txt" x="{:.1}" y="{:.1}">Waste Ø40-50</text>
    </g>"##,
        lx - 8.0, ly - 13.0,
        lx, ly - 2.0, lx + 18.0, ly - 2.0,
        lx + 23.0, ly + 1.0,
        lx + 86.0, ly - 2.0, lx + 104.0, ly - 2.0,
        lx + 109.0, ly + 1.0,
        lx + 168.0, ly - 2.0, lx + 186.0, ly - 2.0,
        lx + 191.0, ly + 1.0,
        lx + 272.0, ly - 2.0, lx + 290.0, ly - 2.0,
        lx + 295.0, ly + 1.0,
    );

    Some(out)
}

fn draw_pill_label(out: &mut String, x: f64, y: f64, text: &str) {
    let w = (text.len() as f64) * 4.9 + 6.0;
    let _ = writeln!(
        out,
        r#"    <rect class="pl-lbl-bg" x="{:.1}" y="{:.1}" width="{w:.1}" height="10" rx="2"/>"#,
        x - w * 0.5,
        y - 7.8
    );
    let _ = writeln!(out, r#"    <text class="pl-lbl" x="{x:.1}" y="{y:.1}">{text}</text>"#);
}

fn longest_segment(pts: &[[f64; 2]]) -> Option<([f64; 2], [f64; 2])> {
    pts.windows(2)
        .max_by(|a, b| {
            let la = (a[1][0] - a[0][0]).hypot(a[1][1] - a[0][1]);
            let lb = (b[1][0] - b[0][0]).hypot(b[1][1] - b[0][1]);
            la.partial_cmp(&lb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|w| (w[0], w[1]))
}

/// Append layered AutoCAD R12 DXF entities (`PLUMB_COLD`, `PLUMB_HOT`, `PLUMB_DRAIN`, `PLUMB_VENT`, `PLUMB_EQUIP`).
pub fn append_plumbing_dxf(ir: &CanonicalIr, out: &mut String) {
    let models = extract_plumbing_models(ir);
    for model in &models {
        for pipe in &model.pipes {
            let layer = match pipe.system.as_str() {
                "cold" => "PLUMB_COLD",
                "hot" => "PLUMB_HOT",
                "soil" | "waste" => "PLUMB_DRAIN",
                _ => "PLUMB_VENT",
            };
            for seg in pipe.points.windows(2) {
                let _ = writeln!(
                    out,
                    "0\nLINE\n8\n{layer}\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n11\n{:.4}\n21\n{:.4}\n31\n0.0",
                    seg[0][0], seg[0][1], seg[1][0], seg[1][1]
                );
            }
        }
        for n in &model.nodes {
            let r = match n.kind.as_str() {
                "soil_stack" => 90.0,
                "water_heater" => 180.0,
                "floor_drain" => 75.0,
                _ => 65.0,
            };
            let _ = writeln!(
                out,
                "0\nCIRCLE\n8\nPLUMB_EQUIP\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n40\n{r:.2}",
                n.pos[0], n.pos[1]
            );
            let _ = writeln!(
                out,
                "0\nTEXT\n8\nPLUMB_EQUIP\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n40\n75.00\n1\n{}",
                n.pos[0],
                n.pos[1] + r + 40.0,
                n.id
            );
        }
    }
}

/// Format a plumbing pipe & equipment takeoff schedule (table or CSV).
pub fn format_plumbing_schedule(ir: &CanonicalIr, csv: bool) -> String {
    let models = extract_plumbing_models(ir);
    if models.is_empty() {
        return if csv {
            "system,diameter_mm,runs,total_length_m,max_flow_l_s,peak_velocity_m_s,slope\n".to_string()
        } else {
            "No building_plumbing model present in this project.\n".to_string()
        };
    }

    // Aggregate pipe takeoffs by (system, diameter_mm, slope)
    let mut agg: BTreeMap<(String, i64, String), (usize, f64, f64, f64)> = BTreeMap::new();
    let mut nodes_all = Vec::new();
    for m in &models {
        for p in &m.pipes {
            let key = (
                p.system.to_uppercase(),
                p.dia_mm.round() as i64,
                p.slope_ratio.clone().unwrap_or_else(|| "-".to_string()),
            );
            let e = agg.entry(key).or_insert((0, 0.0, 0.0, 0.0));
            e.0 += 1;
            e.1 += p.length_mm / 1000.0;
            e.2 = e.2.max(p.flow_l_s);
            e.3 = e.3.max(p.velocity_m_s);
        }
        nodes_all.extend(m.nodes.clone());
    }

    let mut out = String::new();
    if csv {
        let _ = writeln!(
            out,
            "system,diameter_mm,runs,total_length_m,max_flow_l_s,peak_velocity_m_s,slope"
        );
        for ((sys, dia, slope), (runs, len_m, q, v)) in &agg {
            let _ = writeln!(out, "{sys},{dia},{runs},{len_m:.2},{q:.2},{v:.2},{slope}");
        }
    } else {
        let _ = writeln!(out, "PLUMBING PIPE & EQUIPMENT TAKEOFF SCHEDULE");
        let _ = writeln!(
            out,
            "{:<8} {:>8} {:>6} {:>10} {:>10} {:>10} {:>8}",
            "SYSTEM", "DIA(mm)", "RUNS", "LENGTH(m)", "FLOW(L/s)", "VEL(m/s)", "SLOPE"
        );
        let _ = writeln!(out, "{}", "-".repeat(66));
        let mut total_m = 0.0;
        for ((sys, dia, slope), (runs, len_m, q, v)) in &agg {
            total_m += len_m;
            let _ = writeln!(
                out,
                "{:<8} {:>8} {:>6} {:>10.2} {:>10.2} {:>10.2} {:>8}",
                sys, dia, runs, len_m, q, v, slope
            );
        }
        let _ = writeln!(out, "{}", "-".repeat(66));
        let _ = writeln!(
            out,
            "Total pipe length: {:.2} m | Equipment & valves: {}",
            total_m,
            nodes_all.len()
        );
    }
    out
}
