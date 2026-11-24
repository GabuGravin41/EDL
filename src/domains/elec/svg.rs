use crate::core::geom::{Rect2, Side};
use crate::core::ir::*;
use crate::core::span::Severity;
use crate::domains::elec::symbols::{build_symbol_graphic, SymPrim};
use std::collections::BTreeSet;
use std::fmt::Write;

/// Render an EDA-grade schematic SVG directly from Canonical IR (orthogonal routes, junctions,
/// typed pins, group boxes, colored power/GND/signal nets, and red ERC violation overlays).
pub fn render_svg(ir: &CanonicalIr, annotated: bool) -> String {
    let bounds = ir
        .project
        .exterior_bounds
        .unwrap_or(Rect2::new(0.0, 0.0, 254.0, 177.8));

    let pad_mm = 15.0;
    let footer_mm = 22.0;
    let world_w = bounds.width() + 2.0 * pad_mm;
    let world_h = bounds.depth() + 2.0 * pad_mm + footer_mm;
    let px_per_mm = (1200.0 / world_w.max(world_h)).clamp(3.2, 5.2);

    let svg_w = (world_w * px_per_mm).round();
    let svg_h = (world_h * px_per_mm).round();

    let sx = |x: f64| -> f64 { (x - bounds.min_x + pad_mm) * px_per_mm };
    let sy = |y: f64| -> f64 { (y - bounds.min_y + pad_mm) * px_per_mm };
    let sl = |len: f64| -> f64 { len * px_per_mm };

    let violated_entities: BTreeSet<&str> = ir
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .flat_map(|d| d.entities.iter().map(|s| s.as_str()))
        .collect();

    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {svg_w:.1} {svg_h:.1}" width="{svg_w:.0}" height="{svg_h:.0}">"#
    );
    let _ = writeln!(
        out,
        r##"  <defs>
    <style>
      .bg {{ fill: #f8fafc; }}
      .sheet {{ fill: #ffffff; stroke: #cbd5e1; stroke-width: 1.5; }}
      .grp-box {{ fill: #f8fafc; fill-opacity: 0.7; stroke: #94a3b8; stroke-width: 1.0; stroke-dasharray: 5 4; }}
      .grp-title {{ font-family: 'Inter', system-ui, sans-serif; font-size: 11px; font-weight: 700; fill: #64748b; text-anchor: middle; }}
      .sym-body {{ fill: #fef9c3; stroke: #854d0e; stroke-width: 1.6; }}
      .sym-body-err {{ fill: #fee2e2; stroke: #dc2626; stroke-width: 2.2; }}
      .sym-stroke {{ fill: none; stroke: #854d0e; stroke-linecap: round; stroke-linejoin: round; }}
      .sym-stroke-err {{ fill: none; stroke: #dc2626; stroke-linecap: round; stroke-linejoin: round; }}
      .sym-fill {{ fill: #fef08a; stroke: #854d0e; stroke-linejoin: round; }}
      .sym-fill-err {{ fill: #fecaca; stroke: #dc2626; stroke-linejoin: round; }}
      .sym-mark {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #854d0e; text-anchor: middle; }}
      .sym-ref {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #0f172a; }}
      .sym-val {{ font-family: 'Inter', system-ui, sans-serif; font-size: 10px; fill: #334155; }}
      .pin-line {{ stroke: #475569; stroke-width: 1.5; }}
      .pin-line-err {{ stroke: #dc2626; stroke-width: 2.2; }}
      .pin-dot {{ fill: #ffffff; stroke: #334155; stroke-width: 1.1; }}
      .pin-dot-err {{ fill: #fecaca; stroke: #dc2626; stroke-width: 1.6; }}
      .pin-name {{ font-family: monospace; font-size: 9px; fill: #334155; }}
      .wire-sig {{ stroke: #1d4ed8; stroke-width: 1.7; fill: none; stroke-linecap: round; }}
      .wire-pwr {{ stroke: #dc2626; stroke-width: 2.0; fill: none; stroke-linecap: round; }}
      .wire-gnd {{ stroke: #15803d; stroke-width: 2.0; fill: none; stroke-linecap: round; }}
      .wire-err {{ stroke: #dc2626; stroke-width: 2.8; stroke-dasharray: 6 3; fill: none; }}
      .junc-sig {{ fill: #1d4ed8; }}
      .junc-pwr {{ fill: #dc2626; }}
      .junc-gnd {{ fill: #15803d; }}
      .net-lbl {{ font-family: monospace; font-size: 10px; font-weight: 700; }}
      .net-lbl-pwr {{ fill: #b91c1c; }}
      .net-lbl-gnd {{ fill: #15803d; }}
      .net-lbl-sig {{ fill: #1d4ed8; }}
      .title-text {{ font-family: 'Inter', system-ui, sans-serif; font-size: 15px; font-weight: 700; fill: #0f172a; }}
      .meta-text {{ font-family: 'Inter', system-ui, sans-serif; font-size: 11px; fill: #475569; }}
      .status-ok {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #15803d; text-anchor: end; }}
      .status-err {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #dc2626; text-anchor: end; }}
      .status-err-sub {{ font-family: monospace; font-size: 10px; font-weight: 600; fill: #b91c1c; text-anchor: end; }}
    </style>
  </defs>"##
    );

    let _ = writeln!(out, r#"  <rect class="bg" x="0" y="0" width="{svg_w:.1}" height="{svg_h:.1}"/>"#);
    let _ = writeln!(
        out,
        r#"  <rect class="sheet" x="16" y="16" width="{:.1}" height="{:.1}" rx="6"/>"#,
        svg_w - 32.0,
        svg_h - 32.0
    );

    // 1. Functional stage group boxes
    let _ = writeln!(out, r#"  <g id="groups">"#);
    for grp in &ir.schematic_groups {
        let x = sx(grp.bbox.min_x);
        let y = sy(grp.bbox.min_y);
        let w = sl(grp.bbox.width());
        let h = sl(grp.bbox.depth());
        let _ = writeln!(
            out,
            r#"    <rect class="grp-box" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" rx="6"/>"#
        );
        let _ = writeln!(
            out,
            r#"    <text class="grp-title" x="{:.2}" y="{:.2}">{}</text>"#,
            x + w * 0.5,
            y - 5.0,
            escape_xml(&grp.name)
        );
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 2. Orthogonal wire routes, junctions, and net labels
    let _ = writeln!(out, r#"  <g id="wires">"#);
    for route in &ir.schematic_routes {
        let is_err = violated_entities.contains(route.net_id.as_str());
        let w_cls = if is_err {
            "wire-err"
        } else if route.is_ground {
            "wire-gnd"
        } else if route.is_power {
            "wire-pwr"
        } else {
            "wire-sig"
        };
        let j_cls = if route.is_ground {
            "junc-gnd"
        } else if route.is_power {
            "junc-pwr"
        } else {
            "junc-sig"
        };
        let l_cls = if route.is_ground {
            "net-lbl net-lbl-gnd"
        } else if route.is_power {
            "net-lbl net-lbl-pwr"
        } else {
            "net-lbl net-lbl-sig"
        };

        for seg in &route.segments {
            let _ = writeln!(
                out,
                r#"    <line class="{w_cls}" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                sx(seg.p0.x),
                sy(seg.p0.y),
                sx(seg.p1.x),
                sy(seg.p1.y)
            );
        }
        for jpt in &route.junctions {
            let _ = writeln!(
                out,
                r#"    <circle class="{j_cls}" cx="{:.2}" cy="{:.2}" r="3.2"/>"#,
                sx(jpt.x),
                sy(jpt.y)
            );
        }
        if let Some(lp) = route.label_pos {
            let y_off = if route.is_ground { 13.0 } else { -5.0 };
            let _ = writeln!(
                out,
                r#"    <text class="{l_cls}" x="{:.2}" y="{:.2}" text-anchor="middle">{}</text>"#,
                sx(lp.x),
                sy(lp.y) + y_off,
                escape_xml(&route.net_id)
            );
        }
        // Render local terminals (GND triangles, upward power bars, or named net-label flags)
        for term in &route.terminals {
            let tx = sx(term.pos.x);
            let ty = sy(term.pos.y);
            match term.kind.as_str() {
                "gnd_symbol" => {
                    // Classic 3-bar EDA ground symbol + small GND label
                    let _ = writeln!(out, r#"    <line class="wire-gnd" x1="{:.2}" y1="{ty:.2}" x2="{:.2}" y2="{ty:.2}"/>"#, tx - 8.0, tx + 8.0);
                    let _ = writeln!(out, r#"    <line class="wire-gnd" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#, tx - 5.0, ty + 3.5, tx + 5.0, ty + 3.5);
                    let _ = writeln!(out, r#"    <line class="wire-gnd" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#, tx - 2.2, ty + 7.0, tx + 2.2, ty + 7.0);
                    let _ = writeln!(
                        out,
                        r#"    <text class="net-lbl net-lbl-gnd" x="{tx:.2}" y="{:.2}" text-anchor="middle">{}</text>"#,
                        ty + 17.0,
                        escape_xml(&term.label)
                    );
                }
                "power_symbol" => {
                    // Classic upward power rail bar + label
                    let _ = writeln!(out, r#"    <line class="wire-pwr" x1="{:.2}" y1="{ty:.2}" x2="{:.2}" y2="{ty:.2}"/>"#, tx - 8.0, tx + 8.0);
                    let _ = writeln!(
                        out,
                        r#"    <text class="net-lbl net-lbl-pwr" x="{tx:.2}" y="{:.2}" text-anchor="middle">{}</text>"#,
                        ty - 4.0,
                        escape_xml(&term.label)
                    );
                }
                _ => {
                    // Named net-label stub badge (clean horizontal flag)
                    let (lx, ly, anchor) = match term.dir {
                        Side::West => (tx - 3.0, ty + 3.0, "end"),
                        Side::East => (tx + 3.0, ty + 3.0, "start"),
                        Side::North => (tx, ty - 4.0, "middle"),
                        Side::South => (tx, ty + 11.0, "middle"),
                    };
                    let _ = writeln!(
                        out,
                        r#"    <text class="{l_cls}" x="{lx:.2}" y="{ly:.2}" text-anchor="{anchor}">{}</text>"#,
                        escape_xml(&term.label)
                    );
                }
            }
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 3. Components & typed pins
    let _ = writeln!(out, r#"  <g id="components">"#);
    for e in &ir.entities {
        let EntityGeometryIr::Component {
            category,
            value,
            body_rect,
            pins,
            ..
        } = &e.geometry
        else {
            continue;
        };
        let is_err = violated_entities.contains(e.id.as_str());
        let b_cls = if is_err { "sym-body-err" } else { "sym-body" };
        let s_cls = if is_err { "sym-stroke-err" } else { "sym-stroke" };
        let f_cls = if is_err { "sym-fill-err" } else { "sym-fill" };
        let x = sx(body_rect.min_x);
        let y = sy(body_rect.min_y);
        let w = sl(body_rect.width());
        let h = sl(body_rect.depth());

        let graphic = build_symbol_graphic(category, *body_rect, pins);
        let _ = writeln!(out, r#"    <g id="comp-{id}">"#, id = e.id);
        for prim in &graphic.primitives {
            match prim {
                SymPrim::Rect { rect, filled, stroke_mm } => {
                    let cls = if *filled { b_cls } else { s_cls };
                    let _ = writeln!(
                        out,
                        r#"      <rect class="{cls}" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="3" stroke-width="{:.2}"/>"#,
                        sx(rect.min_x),
                        sy(rect.min_y),
                        sl(rect.width()),
                        sl(rect.depth()),
                        (stroke_mm * px_per_mm).clamp(1.2, 2.6)
                    );
                }
                SymPrim::Polyline { pts, filled, stroke_mm } => {
                    let cls = if *filled { f_cls } else { s_cls };
                    let pts_str = pts
                        .iter()
                        .map(|p| format!("{:.2},{:.2}", sx(p.x), sy(p.y)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let sw = (stroke_mm * px_per_mm).clamp(1.2, 2.8);
                    if *filled {
                        let _ = writeln!(
                            out,
                            r#"      <polygon class="{cls}" points="{pts_str}" stroke-width="{sw:.2}"/>"#
                        );
                    } else {
                        let _ = writeln!(
                            out,
                            r#"      <polyline class="{cls}" points="{pts_str}" stroke-width="{sw:.2}"/>"#
                        );
                    }
                }
                SymPrim::Circle { center, radius, filled, stroke_mm } => {
                    let cls = if *filled { f_cls } else { s_cls };
                    let sw = (stroke_mm * px_per_mm).clamp(1.2, 2.6);
                    let _ = writeln!(
                        out,
                        r#"      <circle class="{cls}" cx="{:.2}" cy="{:.2}" r="{:.2}" stroke-width="{sw:.2}"/>"#,
                        sx(center.x),
                        sy(center.y),
                        sl(*radius)
                    );
                }
                SymPrim::Mark { pos, text } => {
                    let _ = writeln!(
                        out,
                        r#"      <text class="sym-mark" x="{:.2}" y="{:.2}">{}</text>"#,
                        sx(pos.x),
                        sy(pos.y),
                        escape_xml(text)
                    );
                }
            }
        }
        let _ = writeln!(out, r#"    </g>"#);
        let has_north = pins.iter().any(|p| p.side == Side::North);
        let has_south = pins.iter().any(|p| p.side == Side::South);
        let (rx, r_anchor) = if has_north {
            (x + w * 0.5 + 8.0, "start")
        } else {
            (x + w * 0.5, "middle")
        };
        let (vx, v_anchor) = if has_south {
            (x + w * 0.5 + 8.0, "start")
        } else {
            (x + w * 0.5, "middle")
        };
        let _ = writeln!(
            out,
            r#"    <text class="sym-ref" x="{rx:.2}" y="{:.2}" text-anchor="{r_anchor}">{}</text>"#,
            y - 4.0,
            escape_xml(&e.id)
        );
        let _ = writeln!(
            out,
            r#"    <text class="sym-val" x="{vx:.2}" y="{:.2}" text-anchor="{v_anchor}">{}</text>"#,
            y + h + 11.0,
            escape_xml(value)
        );

        for p in pins {
            let full_pin = format!("{}.{}", e.id, p.name);
            let p_err = violated_entities.contains(full_pin.as_str());
            let pl_cls = if p_err { "pin-line-err" } else { "pin-line" };
            let pd_cls = if p_err { "pin-dot-err" } else { "pin-dot" };

            let bx = sx(p.body_point.x);
            let by = sy(p.body_point.y);
            let tx = sx(p.tip_point.x);
            let ty = sy(p.tip_point.y);

            let _ = writeln!(
                out,
                r#"    <line class="{pl_cls}" x1="{bx:.2}" y1="{by:.2}" x2="{tx:.2}" y2="{ty:.2}"/>"#
            );
            let _ = writeln!(
                out,
                r#"    <circle class="{pd_cls}" cx="{tx:.2}" cy="{ty:.2}" r="{}"/>"#,
                if p_err { "4.2" } else { "2.4" }
            );
            if (!graphic.hide_pin_names || annotated) || p_err {
                let (lx, ly, anchor) = match p.side {
                    Side::West => (bx + 3.0, by + 3.0, "start"),
                    Side::East => (bx - 3.0, by + 3.0, "end"),
                    Side::North => (bx, by + 10.0, "middle"),
                    Side::South => (bx, by - 3.0, "middle"),
                };
                let _ = writeln!(
                    out,
                    r#"    <text class="pin-name" x="{lx:.2}" y="{ly:.2}" text-anchor="{anchor}">{}</text>"#,
                    escape_xml(&p.name)
                );
            }
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 4. Title block & ERC status footer
    let footer_y = svg_h - 62.0;
    let _ = writeln!(
        out,
        r##"  <line x1="28" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#e2e8f0" stroke-width="1"/>"##,
        footer_y - 16.0,
        svg_w - 28.0,
        footer_y - 16.0
    );
    let _ = writeln!(
        out,
        r#"  <text class="title-text" x="34" y="{footer_y:.1}">EDL Schematic: {}</text>"#,
        escape_xml(&ir.project.name)
    );
    let sheet_txt = ir
        .views
        .first()
        .map(|v| v.sheet.clone())
        .unwrap_or_else(|| "A4 landscape".to_string());
    let _ = writeln!(
        out,
        r#"  <text class="meta-text" x="34" y="{:.1}">{} | Components: {} | Nets: {} | Grid: 2.54mm (100 mil)</text>"#,
        footer_y + 18.0,
        sheet_txt,
        ir.project.component_count,
        ir.project.net_count
    );

    if ir.project.has_errors {
        let err_count = ir
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let _ = writeln!(
            out,
            r#"  <text class="status-err" x="{:.1}" y="{footer_y:.1}">ERC FAILED ({err_count} error(s))</text>"#,
            svg_w - 34.0
        );
        if let Some(first_err) = ir.diagnostics.iter().find(|d| d.severity == Severity::Error) {
            let mut msg = first_err.message.clone();
            if msg.len() > 86 {
                msg.truncate(83);
                msg.push_str("...");
            }
            let _ = writeln!(
                out,
                r#"  <text class="status-err-sub" x="{:.1}" y="{:.1}">{}</text>"#,
                svg_w - 34.0,
                footer_y + 18.0,
                escape_xml(&msg)
            );
        }
    } else {
        let _ = writeln!(
            out,
            r#"  <text class="status-ok" x="{:.1}" y="{footer_y:.1}">ALL ERC CHECKS EXACTLY_CHECKED ({} rules)</text>"#,
            svg_w - 34.0,
            ir.evidence.len()
        );
    }

    let _ = writeln!(out, "</svg>");
    out
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
