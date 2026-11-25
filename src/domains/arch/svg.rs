use crate::core::geom::{Rect2, Side};
use crate::core::ir::*;
use crate::core::span::Severity;
use std::collections::BTreeSet;
use std::fmt::Write;

/// Render an architectural-quality SVG floor plan directly from the Canonical IR.
/// - Auto-fits canvas to ~1150px wide/tall
/// - Emits exact analytical SVG `<path ... A ...>` door-swing arcs (never polygonized)
/// - Renders walls (L-shapes & T-junctions), passages, windows, doors, fixtures + front clearances,
///   room dimensions, overall dimensions, and red verification violation overlays.
pub fn render_svg(ir: &CanonicalIr, annotated: bool) -> String {
    render_svg_filtered(ir, annotated, true)
}

/// Render an architectural floor plan with optional electrical overlay (`show_elec`).
pub fn render_svg_filtered(ir: &CanonicalIr, annotated: bool, show_elec: bool) -> String {
    let grid = crate::domains::arch::grid::extract_grid_from_ir(ir);
    let mut bounds = ir
        .project
        .exterior_bounds
        .unwrap_or(Rect2::new(0.0, 0.0, 5000.0, 4000.0));
    if let Some(ref g) = grid {
        if let Some(gb) = crate::domains::arch::grid::grid_bounds(g) {
            bounds = Rect2::new(
                bounds.min_x.min(gb.min_x),
                bounds.min_y.min(gb.min_y),
                bounds.max_x.max(gb.max_x),
                bounds.max_y.max(gb.max_y),
            );
        }
    }

    let span_max = bounds.width().max(bounds.depth());
    // Compute initial scale, then guarantee generous pixel margins for grid bubbles, dim chains & title bar
    let est_scale = (1280.0 / (span_max * 1.24)).clamp(0.015, 0.13);
    let min_margin_px = if grid.is_some() { 112.0 } else { 88.0 };
    let margin_mm = (min_margin_px / est_scale).max(1150.0);
    let title_h_mm = (86.0 / est_scale).max(700.0);
    let world_w = bounds.width() + 2.0 * margin_mm;
    let world_h = bounds.depth() + 2.0 * margin_mm + title_h_mm;

    // Auto-scale so canvas fits cleanly around ~1280px max side (supports 4m studios up to 70m warehouses!)
    let max_world = world_w.max(world_h);
    let scale_px_per_mm = (1280.0 / max_world).clamp(0.015, 0.13);

    let svg_w = (world_w * scale_px_per_mm).round();
    let svg_h = (world_h * scale_px_per_mm).round();

    let sx = |x: f64| -> f64 { (x - bounds.min_x + margin_mm) * scale_px_per_mm };
    let sy = |y: f64| -> f64 { (bounds.max_y - y + margin_mm) * scale_px_per_mm };
    let sl = |len: f64| -> f64 { len * scale_px_per_mm };

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
    // Model->pixel mapping (px = ox + kx*x_mm, py = oy + ky*y_mm) so registered overlay domains can draw in project coordinates.
    let _ = writeln!(
        out,
        "<!--EDL_VIEW ox={:.6} oy={:.6} kx={:.9} ky={:.9}-->",
        (margin_mm - bounds.min_x) * scale_px_per_mm,
        (bounds.max_y + margin_mm) * scale_px_per_mm,
        scale_px_per_mm,
        -scale_px_per_mm
    );
    let _ = writeln!(
        out,
        r##"  <defs>
    <style>
      .bg {{ fill: #f8f9fb; }}
      .sheet {{ fill: #ffffff; stroke: #cbd5e1; stroke-width: 1.5; }}
      .room-fill {{ fill: #f1f5f9; stroke: #94a3b8; stroke-width: 0.7; }}
      .room-fill-alt {{ fill: #eef6ff; stroke: #94a3b8; stroke-width: 0.7; }}
      .room-fill-bath {{ fill: #ecfeff; stroke: #94a3b8; stroke-width: 0.7; }}
      .room-fill-circ {{ fill: #f8fafc; stroke: #94a3b8; stroke-width: 0.7; }}
      .room-fill-outdoor {{ fill: #fefce8; stroke: #a16207; stroke-width: 1.0; stroke-dasharray: 5 3; }}
      .room-fill-err {{ fill: #fef2f2; stroke: #dc2626; stroke-width: 1.5; }}
      .grid-line {{ stroke: #94a3b8; stroke-width: 0.75; stroke-dasharray: 8 4 2 4; stroke-opacity: 0.75; }}
      .grid-bubble {{ fill: #ffffff; stroke: #334155; stroke-width: 1.3; }}
      .grid-bubble-txt {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 10px; font-weight: 700; fill: #0f172a; text-anchor: middle; }}
      .grid-bay-dim {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 9px; fill: #475569; text-anchor: middle; }}
      .col-concrete {{ fill: #0f172a; stroke: #020617; stroke-width: 0.8; }}
      .col-steel {{ fill: none; stroke: #0f172a; stroke-width: 2.0; stroke-linecap: square; }}
      .elev-box {{ fill: #ffffff; fill-opacity: 0.9; stroke: #475569; stroke-width: 0.8; }}
      .elev-txt {{ font-family: monospace; font-size: 8.5px; font-weight: 700; fill: #0f172a; text-anchor: middle; }}
      .stair-tread {{ stroke: #334155; stroke-width: 0.85; }}
      .stair-arrow {{ stroke: #0f172a; stroke-width: 1.25; fill: none; }}
      .elev-x {{ stroke: #475569; stroke-width: 1.2; }}
      .wall-ext {{ fill: #1e293b; stroke: #0f172a; stroke-width: 0.4; }}
      .wall-int {{ fill: #475569; stroke: #334155; stroke-width: 0.4; }}
      .door-leaf {{ stroke: #0f172a; stroke-width: 2.0; stroke-linecap: round; }}
      .door-arc {{ fill: #3b82f6; fill-opacity: 0.10; stroke: #2563eb; stroke-width: 1.25; stroke-dasharray: 4 3; }}
      .door-arc-err {{ fill: #ef4444; fill-opacity: 0.22; stroke: #dc2626; stroke-width: 2.0; stroke-dasharray: 5 3; }}
      .win-frame {{ fill: #e0f2fe; stroke: #0284c7; stroke-width: 1.3; }}
      .win-glass {{ stroke: #0369a1; stroke-width: 1.1; }}
      .passage-line {{ stroke: #64748b; stroke-width: 1.1; stroke-dasharray: 4 3; }}
      .passage-jamb {{ stroke: #334155; stroke-width: 1.8; }}
      .fx-body {{ fill: #e2e8f0; fill-opacity: 0.85; stroke: #475569; stroke-width: 1.2; }}
      .fx-detail {{ fill: none; stroke: #64748b; stroke-width: 0.85; }}
      .fx-body-err {{ fill: #fecaca; fill-opacity: 0.85; stroke: #dc2626; stroke-width: 1.8; }}
      .fx-clear {{ fill: #22c55e; fill-opacity: 0.08; stroke: #16a34a; stroke-width: 0.9; stroke-dasharray: 3 3; }}
      .fx-clear-err {{ fill: #ef4444; fill-opacity: 0.16; stroke: #dc2626; stroke-width: 1.4; stroke-dasharray: 3 3; }}
      .fx-label {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 9px; font-weight: 600; fill: #334155; text-anchor: middle; }}
      .dim-line {{ stroke: #64748b; stroke-width: 1.0; }}
      .dim-text {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 11px; fill: #334155; text-anchor: middle; }}
      .room-dim {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 9.5px; fill: #64748b; text-anchor: middle; }}
      .room-title {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 13px; font-weight: 700; fill: #0f172a; text-anchor: middle; }}
      .room-sub {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 10.5px; fill: #475569; text-anchor: middle; }}
      .badge {{ font-family: monospace; font-size: 9.5px; font-weight: 600; fill: #1d4ed8; text-anchor: middle; }}
      .badge-err {{ font-family: monospace; font-size: 9.5px; font-weight: 700; fill: #dc2626; text-anchor: middle; }}
      .title-text {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 15px; font-weight: 700; fill: #0f172a; }}
      .meta-text {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 11px; fill: #475569; }}
      .status-ok {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #15803d; text-anchor: end; }}
      .status-err {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #dc2626; text-anchor: end; }}
      .status-err-sub {{ font-family: monospace; font-size: 10px; font-weight: 600; fill: #b91c1c; text-anchor: end; }}
      .mep-wire-light {{ fill: none; stroke: #0284c7; stroke-width: 1.3; stroke-opacity: 0.72; }}
      .mep-wire-power {{ fill: none; stroke: #ea580c; stroke-width: 1.4; stroke-opacity: 0.72; }}
      .mep-wire-swleg {{ fill: none; stroke: #9333ea; stroke-width: 1.1; stroke-dasharray: 4 3; stroke-opacity: 0.85; }}
      .mep-panel {{ fill: #dc2626; stroke: #7f1d1d; stroke-width: 1.3; }}
      .mep-light {{ fill: #fef08a; stroke: #ca8a04; stroke-width: 1.6; }}
      .mep-switch {{ fill: #ffffff; stroke: #7e22ce; stroke-width: 1.6; }}
      .mep-outlet {{ fill: #ffedd5; stroke: #c2410c; stroke-width: 1.5; }}
      .mep-err {{ fill: #fecaca; stroke: #dc2626; stroke-width: 2.2; }}
      .mep-lbl {{ font-family: monospace; font-size: 8.5px; font-weight: 700; fill: #1e293b; text-anchor: middle; }}
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

    // 1. Rooms (fills first, text drawn after fixtures so labels stay crisp)
    let _ = writeln!(out, r#"  <g id="rooms">"#);
    for e in &ir.entities {
        if e.kind != "arch.room" {
            continue;
        }
        if let EntityGeometryIr::Rect {
            min,
            max,
            notch_rect,
            ..
        } = e.geometry
        {
            let full_r = Rect2::new(min[0], min[1], max[0], max[1]);
            let is_violated = violated_entities.contains(e.id.as_str());
            let role = e.props.get("role").and_then(|v| v.as_str()).unwrap_or("");
            let lower = e.id.to_lowercase();
            let cls = if is_violated {
                "room-fill-err"
            } else if role == "outdoor" || lower.contains("terrace") || lower.contains("balcony") || lower.contains("loggia") {
                "room-fill-outdoor"
            } else if role == "wet" || lower.contains("bath") || lower.contains("wc") {
                "room-fill-bath"
            } else if role == "service" || lower.contains("kitchen") {
                "room-fill-alt"
            } else if role == "circulation" || lower.contains("hall") || lower.contains("corridor") {
                "room-fill-circ"
            } else {
                "room-fill"
            };
            if let Some(nr) = notch_rect {
                // Seamless 6-vertex L-shaped polygon (zero internal seam line!)
                let poly = notched_room_polygon(full_r, nr);
                let pts = poly
                    .iter()
                    .map(|p| format!("{:.2},{:.2}", sx(p.x), sy(p.y)))
                    .collect::<Vec<_>>()
                    .join(" ");
                let _ = writeln!(
                    out,
                    r#"    <polygon id="room-{id}" class="{cls}" points="{pts}"/>"#,
                    id = e.id
                );
            } else {
                let x = sx(full_r.min_x);
                let y = sy(full_r.max_y);
                let w = sl(full_r.width());
                let h = sl(full_r.depth());
                let _ = writeln!(
                    out,
                    r#"    <rect id="room-{id}" class="{cls}" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}"/>"#,
                    id = e.id
                );
            }
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 1b. Structural Axis Grid lines, circled axis bubbles & bay dimensions (v1.5.0)
    if let Some(ref g) = grid {
        let _ = writeln!(out, r#"  <g id="structural-grid">"#);
        let bubble_r = 11.5;
        let y_top = (sy(bounds.max_y) - 48.0).max(26.0);
        let y_bot = (sy(bounds.min_y) + 56.0).min(svg_h - 110.0);
        let x_l = (sx(bounds.min_x) - 56.0).max(40.0);
        let x_r = (sx(bounds.max_x) + 46.0).min(svg_w - 28.0);

        // Vertical grid axes (X: 1, 2, 3...) + circled bubbles at bottom
        for xa in &g.x_axes {
            let px = sx(xa.coord_mm);
            let _ = writeln!(out, r#"    <line class="grid-line" x1="{px:.2}" y1="{y_top:.2}" x2="{px:.2}" y2="{y_bot:.2}"/>"#);
            let by = y_bot + bubble_r;
            let _ = writeln!(out, r#"    <circle class="grid-bubble" cx="{px:.2}" cy="{by:.2}" r="{bubble_r:.1}"/>"#);
            let _ = writeln!(out, r#"    <text class="grid-bubble-txt" x="{px:.2}" y="{:.2}">{}</text>"#, by + 3.5, xa.label);
        }
        // Horizontal bay dimensions between X axes right above the bottom bubbles
        for pair in g.x_axes.windows(2) {
            let x0 = sx(pair[0].coord_mm);
            let x1 = sx(pair[1].coord_mm);
            let bay = pair[1].coord_mm - pair[0].coord_mm;
            let yd = y_bot - 5.0;
            if (x1 - x0) > 28.0 {
                let _ = writeln!(
                    out,
                    r#"    <text class="grid-bay-dim" x="{:.2}" y="{yd:.2}">{:.0}</text>"#,
                    0.5 * (x0 + x1),
                    bay
                );
            }
        }

        // Horizontal grid axes (Y: A, B, C...) + circled bubbles at left
        for ya in &g.y_axes {
            let py = sy(ya.coord_mm);
            let _ = writeln!(out, r#"    <line class="grid-line" x1="{x_l:.2}" y1="{py:.2}" x2="{x_r:.2}" y2="{py:.2}"/>"#);
            let bx = x_l - bubble_r;
            let _ = writeln!(out, r#"    <circle class="grid-bubble" cx="{bx:.2}" cy="{py:.2}" r="{bubble_r:.1}"/>"#);
            let _ = writeln!(out, r#"    <text class="grid-bubble-txt" x="{bx:.2}" y="{:.2}">{}</text>"#, py + 3.5, ya.label);
        }
        // Vertical bay dimensions between Y axes right beside left bubbles
        for pair in g.y_axes.windows(2) {
            let y0 = sy(pair[0].coord_mm);
            let y1 = sy(pair[1].coord_mm);
            let bay = pair[1].coord_mm - pair[0].coord_mm;
            let xd = x_l + 6.0;
            let my = 0.5 * (y0 + y1);
            if (y0 - y1).abs() > 26.0 {
                let _ = writeln!(
                    out,
                    r#"    <text class="grid-bay-dim" x="{xd:.2}" y="{my:.2}" transform="rotate(-90 {xd:.2} {my:.2})">{:.0}</text>"#,
                    bay
                );
            }
        }
        let _ = writeln!(out, r#"  </g>"#);
    }

    // 2. Fixtures & front clearance zones (below walls & door arcs)
    let _ = writeln!(out, r#"  <g id="fixtures">"#);
    for e in &ir.entities {
        if let EntityGeometryIr::Fixture {
            category,
            against,
            body_rect,
            clearance_rect,
        } = &e.geometry
        {
            let is_violated = violated_entities.contains(e.id.as_str());
            if let Some(clr) = clearance_rect {
                let c_cls = if is_violated { "fx-clear-err" } else { "fx-clear" };
                let _ = writeln!(
                    out,
                    r#"    <rect id="fx-clear-{id}" class="{c_cls}" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"/>"#,
                    sx(clr.min_x),
                    sy(clr.max_y),
                    sl(clr.width()),
                    sl(clr.depth()),
                    id = e.id
                );
            }
            let b_cls = if is_violated { "fx-body-err" } else { "fx-body" };
            let x = sx(body_rect.min_x);
            let y = sy(body_rect.max_y);
            let w = sl(body_rect.width());
            let h = sl(body_rect.depth());
            let _ = writeln!(
                out,
                r#"    <rect id="fx-{id}" class="{b_cls}" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" rx="2"/>"#,
                id = e.id
            );

            // Subtle architectural detail line by category
            if category == "bed" {
                if let Some(wall_side) = against {
                    let p_thick = (w.min(h) * 0.22).min(24.0);
                    let (px, py, pw, ph) = match wall_side {
                        Side::North => (x + 4.0, y + 4.0, (w - 8.0).max(2.0), p_thick),
                        Side::South => (x + 4.0, y + h - p_thick - 4.0, (w - 8.0).max(2.0), p_thick),
                        Side::West => (x + 4.0, y + 4.0, p_thick, (h - 8.0).max(2.0)),
                        Side::East => (x + w - p_thick - 4.0, y + 4.0, p_thick, (h - 8.0).max(2.0)),
                    };
                    let _ = writeln!(
                        out,
                        r#"    <rect class="fx-detail" x="{px:.2}" y="{py:.2}" width="{pw:.2}" height="{ph:.2}" rx="2"/>"#
                    );
                }
            } else if category == "shower" {
                let _ = writeln!(
                    out,
                    r#"    <line class="fx-detail" x1="{x:.2}" y1="{y:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                    x + w,
                    y + h
                );
                let _ = writeln!(
                    out,
                    r#"    <line class="fx-detail" x1="{:.2}" y1="{y:.2}" x2="{x:.2}" y2="{:.2}"/>"#,
                    x + w,
                    y + h
                );
            } else if category == "stair" || category == "stair_u" || category == "stair_straight" {
                // Real architectural staircase treads + center stringer (for U-turn) + UP arrow
                if category == "stair_u" || (category == "stair" && w.min(h) >= 18.0 && (w / h).clamp(0.5, 2.0) == w / h) {
                    if w >= h {
                        let land = w * 0.30;
                        let mid_y = y + h * 0.5;
                        let _ = writeln!(out, r#"    <line class="stair-tread" x1="{:.2}" y1="{mid_y:.2}" x2="{:.2}" y2="{mid_y:.2}"/>"#, x, x + w - land);
                        let n = ((w - land) / 5.5).floor().clamp(4.0, 12.0) as usize;
                        for i in 1..n {
                            let tx = x + (i as f64) * (w - land) / (n as f64);
                            let _ = writeln!(out, r#"    <line class="stair-tread" x1="{tx:.2}" y1="{y:.2}" x2="{tx:.2}" y2="{:.2}"/>"#, y + h);
                        }
                        let _ = writeln!(out, r#"    <path class="stair-arrow" d="M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}"/>"#,
                            x + 4.0, y + h * 0.75, x + w - land * 0.5, y + h * 0.75, x + w - land * 0.5, y + h * 0.25, x + 6.0, y + h * 0.25);
                    } else {
                        let land = h * 0.30;
                        let mid_x = x + w * 0.5;
                        let _ = writeln!(out, r#"    <line class="stair-tread" x1="{mid_x:.2}" y1="{:.2}" x2="{mid_x:.2}" y2="{:.2}"/>"#, y + land, y + h);
                        let n = ((h - land) / 5.5).floor().clamp(4.0, 12.0) as usize;
                        for i in 1..n {
                            let ty = y + land + (i as f64) * (h - land) / (n as f64);
                            let _ = writeln!(out, r#"    <line class="stair-tread" x1="{x:.2}" y1="{ty:.2}" x2="{:.2}" y2="{ty:.2}"/>"#, x + w);
                        }
                        let _ = writeln!(out, r#"    <path class="stair-arrow" d="M {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}"/>"#,
                            x + w * 0.25, y + h - 4.0, x + w * 0.25, y + land * 0.5, x + w * 0.75, y + land * 0.5, x + w * 0.75, y + h - 6.0);
                    }
                } else {
                    // Straight stair flight
                    if w >= h {
                        let n = (w / 5.5).floor().clamp(4.0, 14.0) as usize;
                        for i in 1..n {
                            let tx = x + (i as f64) * w / (n as f64);
                            let _ = writeln!(out, r#"    <line class="stair-tread" x1="{tx:.2}" y1="{y:.2}" x2="{tx:.2}" y2="{:.2}"/>"#, y + h);
                        }
                        let _ = writeln!(out, r#"    <line class="stair-arrow" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#, x + 4.0, y + h * 0.5, x + w - 4.0, y + h * 0.5);
                    } else {
                        let n = (h / 5.5).floor().clamp(4.0, 14.0) as usize;
                        for i in 1..n {
                            let ty = y + (i as f64) * h / (n as f64);
                            let _ = writeln!(out, r#"    <line class="stair-tread" x1="{x:.2}" y1="{ty:.2}" x2="{:.2}" y2="{ty:.2}"/>"#, x + w);
                        }
                        let _ = writeln!(out, r#"    <line class="stair-arrow" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#, x + w * 0.5, y + h - 4.0, x + w * 0.5, y + 4.0);
                    }
                }
            } else if category == "elevator" || category == "lift" || category == "shaft" || category == "chase" {
                let _ = writeln!(out, r#"    <line class="elev-x" x1="{x:.2}" y1="{y:.2}" x2="{:.2}" y2="{:.2}"/>"#, x + w, y + h);
                if category == "elevator" || category == "lift" {
                    let _ = writeln!(out, r#"    <line class="elev-x" x1="{:.2}" y1="{y:.2}" x2="{x:.2}" y2="{:.2}"/>"#, x + w, y + h);
                }
            } else if category == "column" {
                let _ = writeln!(out, r#"    <rect class="col-concrete" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}"/>"#);
            }

            // Label fixture if box is big enough (strip redundant module instance prefix for clean display)
            if w >= 24.0 && h >= 14.0 {
                let display_label = if let Some(ref prov) = e.provenance {
                    let prefix = format!("{}_", prov.instance);
                    e.id.strip_prefix(&prefix).unwrap_or(&e.id)
                } else {
                    e.id.as_str()
                };
                let cx = x + w * 0.5;
                let cy = y + h * 0.5;
                let est_w = (display_label.len() as f64) * 5.6;
                if h > w * 1.35 && est_w > w - 6.0 {
                    let _ = writeln!(
                        out,
                        r#"    <text class="fx-label" x="{cx:.2}" y="{:.2}" transform="rotate(-90 {cx:.2} {cy:.2})">{display_label}</text>"#,
                        cy + 3.0
                    );
                } else {
                    let _ = writeln!(
                        out,
                        r#"    <text class="fx-label" x="{cx:.2}" y="{:.2}">{display_label}</text>"#,
                        cy + 3.0
                    );
                }
            }
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 3. Solid wall poché (exterior + interior)
    let _ = writeln!(out, r#"  <g id="walls">"#);
    for cell in &ir.wall_solids {
        let cls = if cell.kind == "exterior" {
            "wall-ext"
        } else {
            "wall-int"
        };
        let x = sx(cell.rect.min_x);
        let y = sy(cell.rect.max_y);
        let w = sl(cell.rect.width());
        let h = sl(cell.rect.depth());
        let _ = writeln!(
            out,
            r#"    <rect class="{cls}" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}"/>"#
        );
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 3b. Structural Grid Columns (concrete squares or steel I-beams, drawn on top of walls!)
    if let Some(ref g) = grid {
        if !g.columns.is_empty() {
            let _ = writeln!(out, r#"  <g id="structural-columns">"#);
            for col in &g.columns {
                let cr = crate::domains::arch::grid::column_rect(col);
                let x = sx(cr.min_x);
                let y = sy(cr.max_y);
                let w = sl(cr.width()).max(4.5);
                let h = sl(cr.depth()).max(4.5);
                if col.kind == "steel_i" {
                    // Crisp structural steel H/I-beam cross-section
                    let cx = x + w * 0.5;
                    let _ = writeln!(out, r#"    <line class="col-steel" x1="{x:.2}" y1="{y:.2}" x2="{:.2}" y2="{y:.2}"/>"#, x + w);
                    let _ = writeln!(out, r#"    <line class="col-steel" x1="{x:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#, y + h, x + w, y + h);
                    let _ = writeln!(out, r#"    <line class="col-steel" x1="{cx:.2}" y1="{y:.2}" x2="{cx:.2}" y2="{:.2}"/>"#, y + h);
                } else {
                    let _ = writeln!(out, r#"    <rect id="col-{}" class="col-concrete" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}"/>"#, col.id);
                }
            }
            let _ = writeln!(out, r#"  </g>"#);
        }
    }

    // 4. Openings (windows, passages, and doors with exact analytical SVG arcs)
    let _ = writeln!(out, r#"  <g id="openings">"#);
    for e in &ir.entities {
        let is_violated = violated_entities.contains(e.id.as_str());
        match &e.geometry {
            EntityGeometryIr::Window {
                cutout_rect,
                opening_segment,
            } => {
                let x = sx(cutout_rect.min_x);
                let y = sy(cutout_rect.max_y);
                let w = sl(cutout_rect.width());
                let h = sl(cutout_rect.depth());
                let _ = writeln!(
                    out,
                    r#"    <rect id="win-{id}" class="win-frame" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}"/>"#,
                    id = e.id
                );
                let _ = writeln!(
                    out,
                    r#"    <line class="win-glass" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                    sx(opening_segment.p0.x),
                    sy(opening_segment.p0.y),
                    sx(opening_segment.p1.x),
                    sy(opening_segment.p1.y)
                );
                if annotated || is_violated {
                    let is_horiz = cutout_rect.width() >= cutout_rect.depth();
                    let (lx, ly) = if is_horiz {
                        (sx(cutout_rect.center().x), sy(cutout_rect.max_y) - 6.0)
                    } else {
                        // Place vertical window badge cleanly inside the room side to avoid outer dim line
                        (sx(cutout_rect.min_x) - 28.0, sy(cutout_rect.center().y) + 3.0)
                    };
                    let cls = if is_violated { "badge-err" } else { "badge" };
                    let _ = writeln!(out, r#"    <text class="{cls}" x="{lx:.2}" y="{ly:.2}">{}</text>"#, e.id);
                }
            }
            EntityGeometryIr::Passage {
                cutout_rect,
                opening_segment,
            } => {
                // Clean dashed threshold + end jamb ticks
                let _ = writeln!(
                    out,
                    r#"    <line id="passage-{id}" class="passage-line" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                    sx(opening_segment.p0.x),
                    sy(opening_segment.p0.y),
                    sx(opening_segment.p1.x),
                    sy(opening_segment.p1.y),
                    id = e.id
                );
                let is_horiz = cutout_rect.width() >= cutout_rect.depth();
                if is_horiz {
                    let _ = writeln!(
                        out,
                        r#"    <line class="passage-jamb" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                        sx(cutout_rect.min_x), sy(cutout_rect.min_y), sx(cutout_rect.min_x), sy(cutout_rect.max_y)
                    );
                    let _ = writeln!(
                        out,
                        r#"    <line class="passage-jamb" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                        sx(cutout_rect.max_x), sy(cutout_rect.min_y), sx(cutout_rect.max_x), sy(cutout_rect.max_y)
                    );
                } else {
                    let _ = writeln!(
                        out,
                        r#"    <line class="passage-jamb" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                        sx(cutout_rect.min_x), sy(cutout_rect.min_y), sx(cutout_rect.max_x), sy(cutout_rect.min_y)
                    );
                    let _ = writeln!(
                        out,
                        r#"    <line class="passage-jamb" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                        sx(cutout_rect.min_x), sy(cutout_rect.max_y), sx(cutout_rect.max_x), sy(cutout_rect.max_y)
                    );
                }
                if annotated || is_violated {
                    let cls = if is_violated { "badge-err" } else { "badge" };
                    let _ = writeln!(
                        out,
                        r#"    <text class="{cls}" x="{:.2}" y="{:.2}">{}</text>"#,
                        sx(cutout_rect.center().x),
                        sy(cutout_rect.center().y) - 4.0,
                        e.id
                    );
                }
            }
            EntityGeometryIr::Door {
                cutout_rect,
                leaf_open,
                swing_arc,
                ..
            } => {
                let c = swing_arc.center;
                let p_start = swing_arc.start_point();
                let p_end = swing_arc.end_point();
                let r_px = sl(swing_arc.radius);
                let sweep_flag = if swing_arc.sweep_deg > 0.0 { 0 } else { 1 };
                let arc_cls = if is_violated { "door-arc-err" } else { "door-arc" };

                let _ = writeln!(
                    out,
                    r#"    <path id="door-arc-{id}" class="{arc_cls}" d="M {cx:.2} {cy:.2} L {x1:.2} {y1:.2} A {r:.2} {r:.2} 0 0 {sf} {x2:.2} {y2:.2} Z"/>"#,
                    id = e.id,
                    cx = sx(c.x),
                    cy = sy(c.y),
                    x1 = sx(p_start.x),
                    y1 = sy(p_start.y),
                    r = r_px,
                    sf = sweep_flag,
                    x2 = sx(p_end.x),
                    y2 = sy(p_end.y)
                );
                let _ = writeln!(
                    out,
                    r#"    <line class="door-leaf" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                    sx(leaf_open.p0.x),
                    sy(leaf_open.p0.y),
                    sx(leaf_open.p1.x),
                    sy(leaf_open.p1.y)
                );
                if is_violated {
                    let _ = writeln!(
                        out,
                        r##"    <rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="none" stroke="#dc2626" stroke-width="2.2"/>"##,
                        sx(cutout_rect.min_x),
                        sy(cutout_rect.max_y),
                        sl(cutout_rect.width()),
                        sl(cutout_rect.depth())
                    );
                }
                if annotated || is_violated {
                    let mid_angle = (swing_arc.start_deg + 0.5 * swing_arc.sweep_deg).to_radians();
                    let mx = c.x + 0.52 * swing_arc.radius * mid_angle.cos();
                    let my = c.y + 0.52 * swing_arc.radius * mid_angle.sin();
                    let cls = if is_violated { "badge-err" } else { "badge" };
                    let _ = writeln!(
                        out,
                        r#"    <text class="{cls}" x="{:.2}" y="{:.2}">{}</text>"#,
                        sx(mx),
                        sy(my) + 3.0,
                        e.id
                    );
                }
            }
            _ => {}
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 5. Room labels & clear dimensions (smart collision-free placement away from fixtures & door arcs!)
    let mut label_obstacles: Vec<Rect2> = Vec::new();
    for e in &ir.entities {
        match &e.geometry {
            EntityGeometryIr::Fixture { body_rect, .. } => label_obstacles.push(*body_rect),
            // Electrical devices (esp. ceiling lights at room centers) must not collide with room titles
            EntityGeometryIr::MepDevice { device_kind, pos, .. } if show_elec => {
                let r = if device_kind == "light" { 650.0 } else { 300.0 };
                label_obstacles.push(Rect2::new(pos.x - r, pos.y - r, pos.x + r, pos.y + r));
            }
            EntityGeometryIr::Door { swing_arc, .. } => {
                // Approximate quarter-disk bbox around door swing so labels stay clear of door leaves
                let p0 = swing_arc.start_point();
                let p1 = swing_arc.end_point();
                let c = swing_arc.center;
                let min_x = c.x.min(p0.x).min(p1.x);
                let max_x = c.x.max(p0.x).max(p1.x);
                let min_y = c.y.min(p0.y).min(p1.y);
                let max_y = c.y.max(p0.y).max(p1.y);
                label_obstacles.push(Rect2::new(min_x, min_y, max_x, max_y));
            }
            _ => {}
        }
    }

    let _ = writeln!(out, r#"  <g id="room-labels">"#);
    for e in &ir.entities {
        if e.kind != "arch.room" {
            continue;
        }
        if let EntityGeometryIr::Rect {
            min,
            max,
            width_mm,
            depth_mm,
            area_m2,
            notch_rect,
        } = e.geometry
        {
            let full_r = Rect2::new(min[0], min[1], max[0], max[1]);
            let subs = crate::domains::arch::compile::decompose_room_rect(full_r, notch_rect);
            let best_pt = pick_clear_room_label_pos(&subs, &label_obstacles, e.id.len());
            let cx = sx(best_pt.x);
            let cy = sy(best_pt.y);
            let _ = writeln!(
                out,
                r#"    <text class="room-title" x="{cx:.2}" y="{:.2}">{}</text>"#,
                cy - 7.0,
                e.id
            );
            let _ = writeln!(
                out,
                r#"    <text class="room-sub" x="{cx:.2}" y="{:.2}">{:.2}m × {:.2}m{}</text>"#,
                cy + 8.0,
                width_mm / 1000.0,
                depth_mm / 1000.0,
                if notch_rect.is_some() { " (L)" } else { "" }
            );
            let _ = writeln!(
                out,
                r#"    <text class="room-sub" x="{cx:.2}" y="{:.2}">{:.2} m²</text>"#,
                cy + 21.0,
                area_m2
            );
            if let Some(ref g) = grid {
                if let Some(elev) = g.room_elevations.get(&e.id) {
                    let bw = (elev.len() as f64) * 5.2 + 8.0;
                    let _ = writeln!(
                        out,
                        r#"    <rect class="elev-box" x="{:.2}" y="{:.2}" width="{bw:.1}" height="12" rx="1.5"/>"#,
                        cx - bw * 0.5,
                        cy + 26.0
                    );
                    let _ = writeln!(
                        out,
                        r#"    <text class="elev-txt" x="{cx:.2}" y="{:.2}">{elev}</text>"#,
                        cy + 35.0
                    );
                }
            }
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 5b. Building Electrical Overlay (mep.elec) — layered directly on top of the floor plan!
    let has_mep = show_elec
        && (!ir.mep_wires.is_empty()
            || ir
                .entities
                .iter()
                .any(|e| matches!(e.geometry, EntityGeometryIr::MepDevice { .. })));
    if has_mep {
        let _ = writeln!(out, r#"  <g id="mep-electrical">"#);
        // Conduit / circuit runs & switch legs
        for w in &ir.mep_wires {
            let cls = match w.run_kind.as_str() {
                "lighting" => "mep-wire-light",
                "switch_leg" => "mep-wire-swleg",
                _ => "mep-wire-power",
            };
            for seg in &w.segments {
                if w.run_kind == "switch_leg" {
                    // Drafting convention: switch legs are drawn as shallow arcs, never straight diagonals,
                    // so they stay visually distinct from circuit conduit runs.
                    let (x0, y0, x1, y1) = (sx(seg.p0.x), sy(seg.p0.y), sx(seg.p1.x), sy(seg.p1.y));
                    let (dx, dy) = (x1 - x0, y1 - y0);
                    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                    let bow = (len * 0.12).min(40.0);
                    let (cx, cy) = ((x0 + x1) * 0.5 - dy / len * bow, (y0 + y1) * 0.5 + dx / len * bow);
                    let _ = writeln!(
                        out,
                        r#"    <path class="{cls}" d="M {x0:.2} {y0:.2} Q {cx:.2} {cy:.2} {x1:.2} {y1:.2}"/>"#
                    );
                    continue;
                }
                let _ = writeln!(
                    out,
                    r#"    <line class="{cls}" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                    sx(seg.p0.x),
                    sy(seg.p0.y),
                    sx(seg.p1.x),
                    sy(seg.p1.y)
                );
            }
        }
        // Standard IEC/AutoCAD electrical symbols
        for e in &ir.entities {
            let EntityGeometryIr::MepDevice {
                device_kind,
                pos,
                facing,
                ..
            } = &e.geometry
            else {
                continue;
            };
            let is_err = violated_entities.contains(e.id.as_str());
            let px = sx(pos.x);
            let py = sy(pos.y);
            // Offset wall-mounted symbols slightly inward from wall face so they stand clear of poché
            let (nx, ny) = match facing {
                Some(Side::North) => (0.0, -1.0), // SVG Y-down: North is -Y
                Some(Side::South) => (0.0, 1.0),
                Some(Side::East) => (1.0, 0.0),
                Some(Side::West) => (-1.0, 0.0),
                None => (0.0, 0.0),
            };
            match device_kind.as_str() {
                "panel" => {
                    let cls = if is_err { "mep-err" } else { "mep-panel" };
                    let cx = px + nx * 6.0;
                    let cy = py + ny * 6.0;
                    let (rw, rh) = if ny.abs() > 0.5 { (20.0, 8.0) } else { (8.0, 20.0) };
                    let _ = writeln!(
                        out,
                        r#"    <rect id="mep-{id}" class="{cls}" x="{:.2}" y="{:.2}" width="{rw:.1}" height="{rh:.1}" rx="1.5"/>"#,
                        cx - rw * 0.5,
                        cy - rh * 0.5,
                        id = e.id
                    );
                    let _ = writeln!(
                        out,
                        r#"    <text class="mep-lbl" x="{:.2}" y="{:.2}">{}</text>"#,
                        cx + nx * 13.0,
                        cy + ny * 12.0 + 3.0,
                        e.id
                    );
                }
                "light" => {
                    let cls = if is_err { "mep-err" } else { "mep-light" };
                    let r = 7.5;
                    let _ = writeln!(out, r#"    <circle id="mep-{id}" class="{cls}" cx="{px:.2}" cy="{py:.2}" r="{r:.1}"/>"#, id = e.id);
                    let d = r * 0.7;
                    let _ = writeln!(out, r#"    <line class="{cls}" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#, px - d, py - d, px + d, py + d);
                    let _ = writeln!(out, r#"    <line class="{cls}" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#, px - d, py + d, px + d, py - d);
                    let _ = writeln!(out, r#"    <text class="mep-lbl" x="{px:.2}" y="{:.2}">{}</text>"#, py - r - 3.0, e.id);
                }
                "switch" => {
                    let cls = if is_err { "mep-err" } else { "mep-switch" };
                    let cx = px + nx * 5.0;
                    let cy = py + ny * 5.0;
                    let _ = writeln!(out, r#"    <circle id="mep-{id}" class="{cls}" cx="{cx:.2}" cy="{cy:.2}" r="4.0"/>"#, id = e.id);
                    let _ = writeln!(out, r#"    <line class="{cls}" x1="{cx:.2}" y1="{cy:.2}" x2="{:.2}" y2="{:.2}"/>"#, cx + 6.5, cy - 6.5);
                    let _ = writeln!(out, r#"    <line class="{cls}" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#, cx + 6.5, cy - 6.5, cx + 9.0, cy - 4.0);
                    if annotated || is_err {
                        let _ = writeln!(out, r#"    <text class="mep-lbl" x="{:.2}" y="{:.2}">{}</text>"#, cx + nx * 10.0, cy + ny * 10.0 + 3.0, e.id);
                    }
                }
                "outlet" => {
                    let cls = if is_err { "mep-err" } else { "mep-outlet" };
                    let cx = px + nx * 5.5;
                    let cy = py + ny * 5.5;
                    let _ = writeln!(out, r#"    <circle id="mep-{id}" class="{cls}" cx="{cx:.2}" cy="{cy:.2}" r="4.8"/>"#, id = e.id);
                    // Twin prongs pointing inward from wall
                    let tx = -ny;
                    let ty = nx;
                    for sign in [-1.0, 1.0] {
                        let bx = cx + tx * sign * 2.0;
                        let by = cy + ty * sign * 2.0;
                        let _ = writeln!(
                            out,
                            r#"    <line class="{cls}" x1="{bx:.2}" y1="{by:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                            bx + nx * 5.0,
                            by + ny * 5.0
                        );
                    }
                    if annotated || is_err {
                        let _ = writeln!(out, r#"    <text class="mep-lbl" x="{:.2}" y="{:.2}">{}</text>"#, cx + nx * 13.0, cy + ny * 12.0 + 3.0, e.id);
                    }
                }
                _ => {}
            }
        }
        let _ = writeln!(out, r#"  </g>"#);
        // Compact Electrical Legend bar in top-right sheet header margin
        let ex = svg_w - 392.0;
        let ey = 33.0;
        let _ = writeln!(
            out,
            r##"  <g id="electrical-legend">
    <rect x="{:.1}" y="{:.1}" width="368" height="22" rx="4" fill="#ffffff" fill-opacity="0.93" stroke="#cbd5e1" stroke-width="1"/>
    <line class="mep-wire-light" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>
    <text class="meta-text" style="font-size:8.5px;font-weight:600;" x="{:.1}" y="{:.1}">Lighting</text>
    <line class="mep-wire-power" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>
    <text class="meta-text" style="font-size:8.5px;font-weight:600;" x="{:.1}" y="{:.1}">Power</text>
    <line class="mep-wire-swleg" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>
    <text class="meta-text" style="font-size:8.5px;font-weight:600;" x="{:.1}" y="{:.1}">Switch Leg</text>
    <rect class="mep-panel" x="{:.1}" y="{:.1}" width="10" height="7" rx="1"/>
    <text class="meta-text" style="font-size:8.5px;font-weight:600;" x="{:.1}" y="{:.1}">DB</text>
    <circle class="mep-light" cx="{:.1}" cy="{:.1}" r="4.5"/>
    <text class="meta-text" style="font-size:8.5px;font-weight:600;" x="{:.1}" y="{:.1}">Light</text>
    <circle class="mep-outlet" cx="{:.1}" cy="{:.1}" r="4.0"/>
    <text class="meta-text" style="font-size:8.5px;font-weight:600;" x="{:.1}" y="{:.1}">Socket</text>
  </g>"##,
            ex, ey - 13.0,
            ex + 8.0, ey - 2.0, ex + 24.0, ey - 2.0,
            ex + 28.0, ey + 1.0,
            ex + 74.0, ey - 2.0, ex + 90.0, ey - 2.0,
            ex + 94.0, ey + 1.0,
            ex + 132.0, ey - 2.0, ex + 148.0, ey - 2.0,
            ex + 152.0, ey + 1.0,
            ex + 210.0, ey - 5.5,
            ex + 224.0, ey + 1.0,
            ex + 252.0, ey - 2.0,
            ex + 260.0, ey + 1.0,
            ex + 298.0, ey - 2.0,
            ex + 306.0, ey + 1.0,
        );
    }

    // 6. Overall exterior + interior room dimension chains
    let show_dims = ir.views.first().map(|v| v.show_dimensions).unwrap_or(true);
    let _ = writeln!(out, r#"  <g id="dimensions">"#);
    if show_dims {
        let dim_off = (36.0 / scale_px_per_mm).max(560.0);
        let chain_off = (16.0 / scale_px_per_mm).max(250.0);

        // Overall Top dimension
        let y_dim = sy(bounds.max_y + dim_off);
        let x_l = sx(bounds.min_x);
        let x_r = sx(bounds.max_x);
        let _ = writeln!(out, r#"    <line class="dim-line" x1="{x_l:.2}" y1="{y_dim:.2}" x2="{x_r:.2}" y2="{y_dim:.2}"/>"#);
        let _ = writeln!(out, r#"    <line class="dim-line" x1="{x_l:.2}" y1="{:.2}" x2="{x_l:.2}" y2="{:.2}"/>"#, y_dim - 6.0, sy(bounds.max_y) - 4.0);
        let _ = writeln!(out, r#"    <line class="dim-line" x1="{x_r:.2}" y1="{:.2}" x2="{x_r:.2}" y2="{:.2}"/>"#, y_dim - 6.0, sy(bounds.max_y) - 4.0);
        let _ = writeln!(
            out,
            r#"    <text class="dim-text" x="{:.2}" y="{:.2}">{:.0} mm ({:.2} m)</text>"#,
            0.5 * (x_l + x_r),
            y_dim - 6.0,
            bounds.width(),
            bounds.width() / 1000.0
        );

        // Overall Right dimension
        let x_dim = sx(bounds.max_x + dim_off);
        let y_t = sy(bounds.max_y);
        let y_b = sy(bounds.min_y);
        let _ = writeln!(out, r#"    <line class="dim-line" x1="{x_dim:.2}" y1="{y_t:.2}" x2="{x_dim:.2}" y2="{y_b:.2}"/>"#);
        let _ = writeln!(out, r#"    <line class="dim-line" x1="{:.2}" y1="{y_t:.2}" x2="{:.2}" y2="{y_t:.2}"/>"#, sx(bounds.max_x) + 4.0, x_dim + 6.0);
        let _ = writeln!(out, r#"    <line class="dim-line" x1="{:.2}" y1="{y_b:.2}" x2="{:.2}" y2="{y_b:.2}"/>"#, sx(bounds.max_x) + 4.0, x_dim + 6.0);
        let mid_y = 0.5 * (y_t + y_b);
        let _ = writeln!(
            out,
            r#"    <text class="dim-text" x="{:.2}" y="{mid_y:.2}" transform="rotate(90 {:.2} {mid_y:.2})">{:.0} mm ({:.2} m)</text>"#,
            x_dim + 14.0,
            x_dim + 14.0,
            bounds.depth(),
            bounds.depth() / 1000.0
        );

        // Interior room dimension chains derived directly from exterior-facing walls!
        // This accurately includes recessed wings (non-rectangular footprints) AND keeps
        // unbroken exterior walls of notched rooms as single continuous spans!
        let mut raw_top: Vec<(f64, f64, f64)> = Vec::new();
        let mut raw_bot: Vec<(f64, f64, f64)> = Vec::new();
        let mut raw_right: Vec<(f64, f64, f64)> = Vec::new();
        let mut raw_left: Vec<(f64, f64, f64)> = Vec::new();

        for w in &ir.derived_walls {
            if w.kind != "arch.wall.exterior" {
                continue;
            }
            let x0 = w.clear_segment.p0.x.min(w.clear_segment.p1.x);
            let x1 = w.clear_segment.p0.x.max(w.clear_segment.p1.x);
            let y0 = w.clear_segment.p0.y.min(w.clear_segment.p1.y);
            let y1 = w.clear_segment.p0.y.max(w.clear_segment.p1.y);
            if w.id.contains("_north") {
                raw_top.push((x0, x1, y0)); // higher y wins if overlapping in X
            } else if w.id.contains("_south") {
                raw_bot.push((x0, x1, -y0)); // lower y wins if overlapping in X
            } else if w.id.contains("_east") {
                raw_right.push((y0, y1, x0)); // higher x wins if overlapping in Y
            } else if w.id.contains("_west") {
                raw_left.push((y0, y1, -x0)); // lower x wins if overlapping in Y
            }
        }

        let top_spans = project_outermost_spans(raw_top);
        let bot_spans = project_outermost_spans(raw_bot);
        let right_spans = project_outermost_spans(raw_right);
        let left_spans = project_outermost_spans(raw_left);

        if true {

            // Top interior chain
            if top_spans.len() >= 1 {
                let yc = sy(bounds.max_y + chain_off);
                for &(a, b) in &top_spans {
                    let xa = sx(a);
                    let xb = sx(b);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{xa:.2}" y1="{yc:.2}" x2="{xb:.2}" y2="{yc:.2}"/>"#);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{xa:.2}" y1="{:.2}" x2="{xa:.2}" y2="{:.2}"/>"#, yc - 4.0, yc + 4.0);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{xb:.2}" y1="{:.2}" x2="{xb:.2}" y2="{:.2}"/>"#, yc - 4.0, yc + 4.0);
                    let _ = writeln!(
                        out,
                        r#"    <text class="dim-text" x="{:.2}" y="{:.2}">{:.2} m</text>"#,
                        0.5 * (xa + xb),
                        yc - 5.0,
                        (b - a) / 1000.0
                    );
                }
            }
            // Bottom interior chain (only if partition differs from top!)
            if !spans_equal(&top_spans, &bot_spans) && !bot_spans.is_empty() {
                let yc = sy(bounds.min_y - chain_off);
                for &(a, b) in &bot_spans {
                    let xa = sx(a);
                    let xb = sx(b);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{xa:.2}" y1="{yc:.2}" x2="{xb:.2}" y2="{yc:.2}"/>"#);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{xa:.2}" y1="{:.2}" x2="{xa:.2}" y2="{:.2}"/>"#, yc - 4.0, yc + 4.0);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{xb:.2}" y1="{:.2}" x2="{xb:.2}" y2="{:.2}"/>"#, yc - 4.0, yc + 4.0);
                    let _ = writeln!(
                        out,
                        r#"    <text class="dim-text" x="{:.2}" y="{:.2}">{:.2} m</text>"#,
                        0.5 * (xa + xb),
                        yc + 12.0,
                        (b - a) / 1000.0
                    );
                }
            }
            // Right interior chain
            if !right_spans.is_empty() {
                let xc = sx(bounds.max_x + chain_off);
                for &(a, b) in &right_spans {
                    let ya = sy(a);
                    let yb = sy(b);
                    let my = 0.5 * (ya + yb);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{xc:.2}" y1="{ya:.2}" x2="{xc:.2}" y2="{yb:.2}"/>"#);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{:.2}" y1="{ya:.2}" x2="{:.2}" y2="{ya:.2}"/>"#, xc - 4.0, xc + 4.0);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{:.2}" y1="{yb:.2}" x2="{:.2}" y2="{yb:.2}"/>"#, xc - 4.0, xc + 4.0);
                    let _ = writeln!(
                        out,
                        r#"    <text class="dim-text" x="{:.2}" y="{my:.2}" transform="rotate(90 {:.2} {my:.2})">{:.2} m</text>"#,
                        xc + 11.0,
                        xc + 11.0,
                        (b - a) / 1000.0
                    );
                }
            }
            // Left interior chain (only if partition differs from right!)
            if !spans_equal(&right_spans, &left_spans) && !left_spans.is_empty() {
                let xc = sx(bounds.min_x - chain_off);
                for &(a, b) in &left_spans {
                    let ya = sy(a);
                    let yb = sy(b);
                    let my = 0.5 * (ya + yb);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{xc:.2}" y1="{ya:.2}" x2="{xc:.2}" y2="{yb:.2}"/>"#);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{:.2}" y1="{ya:.2}" x2="{:.2}" y2="{ya:.2}"/>"#, xc - 4.0, xc + 4.0);
                    let _ = writeln!(out, r#"    <line class="dim-line" x1="{:.2}" y1="{yb:.2}" x2="{:.2}" y2="{yb:.2}"/>"#, xc - 4.0, xc + 4.0);
                    let _ = writeln!(
                        out,
                        r#"    <text class="dim-text" x="{:.2}" y="{my:.2}" transform="rotate(-90 {:.2} {my:.2})">{:.2} m</text>"#,
                        xc - 11.0,
                        xc - 11.0,
                        (b - a) / 1000.0
                    );
                }
            }
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 7. Title block & verification status bar
    let footer_y = svg_h - 68.0;
    let _ = writeln!(
        out,
        r##"  <line x1="28" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#e2e8f0" stroke-width="1"/>"##,
        footer_y - 16.0,
        svg_w - 28.0,
        footer_y - 16.0
    );
    let _ = writeln!(
        out,
        r#"  <text class="title-text" x="34" y="{footer_y:.1}">EDL Plan: {}</text>"#,
        ir.project.name
    );
    let scale_txt = ir
        .views
        .first()
        .map(|v| format!("{} | Scale {}", v.sheet, v.scale))
        .unwrap_or_else(|| "A3 landscape | Scale 1:50".to_string());
    let _ = writeln!(
        out,
        r#"  <text class="meta-text" x="34" y="{:.1}">{} | Rooms: {} | Clear Area: {:.2} m² | Openings: {} | Fixtures: {} | DOF: {}</text>"#,
        footer_y + 18.0,
        scale_txt,
        ir.project.room_count,
        ir.project.total_clear_area_m2,
        ir.project.opening_count,
        ir.project.fixture_count,
        ir.project.remaining_dof.len()
    );

    if ir.project.has_errors {
        let err_count = ir
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let _ = writeln!(
            out,
            r#"  <text class="status-err" x="{:.1}" y="{footer_y:.1}">VERIFICATION FAILED ({err_count} error(s))</text>"#,
            svg_w - 34.0
        );
        if let Some(first_err) = ir.diagnostics.iter().find(|d| d.severity == Severity::Error) {
            let mut msg = first_err.message.clone();
            if msg.len() > 82 {
                msg.truncate(79);
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
            r#"  <text class="status-ok" x="{:.1}" y="{footer_y:.1}">ALL CHECKS EXACTLY_CHECKED ({} rules)</text>"#,
            svg_w - 34.0,
            ir.evidence.len()
        );
    }

    let _ = writeln!(out, "</svg>");
    out
}

/// Compute the exact 6-vertex CCW polygon of a room rectangle minus a corner notch.
fn notched_room_polygon(r: Rect2, n: Rect2) -> Vec<crate::core::geom::Point2> {
    use crate::core::geom::Point2;
    let eps = 1e-3;
    let at_east = (n.max_x - r.max_x).abs() <= eps;
    let at_north = (n.max_y - r.max_y).abs() <= eps;
    match (at_east, at_north) {
        // Northeast notch
        (true, true) => vec![
            Point2::new(r.min_x, r.min_y),
            Point2::new(r.max_x, r.min_y),
            Point2::new(r.max_x, n.min_y),
            Point2::new(n.min_x, n.min_y),
            Point2::new(n.min_x, r.max_y),
            Point2::new(r.min_x, r.max_y),
        ],
        // Northwest notch
        (false, true) => vec![
            Point2::new(r.min_x, r.min_y),
            Point2::new(r.max_x, r.min_y),
            Point2::new(r.max_x, r.max_y),
            Point2::new(n.max_x, r.max_y),
            Point2::new(n.max_x, n.min_y),
            Point2::new(r.min_x, n.min_y),
        ],
        // Southeast notch
        (true, false) => vec![
            Point2::new(r.min_x, r.min_y),
            Point2::new(n.min_x, r.min_y),
            Point2::new(n.min_x, n.max_y),
            Point2::new(r.max_x, n.max_y),
            Point2::new(r.max_x, r.max_y),
            Point2::new(r.min_x, r.max_y),
        ],
        // Southwest notch
        (false, false) => vec![
            Point2::new(n.max_x, r.min_y),
            Point2::new(r.max_x, r.min_y),
            Point2::new(r.max_x, r.max_y),
            Point2::new(r.min_x, r.max_y),
            Point2::new(r.min_x, n.max_y),
            Point2::new(n.max_x, n.max_y),
        ],
    }
}

/// Choose the clearest room label position inside `subs` that avoids fixtures and door swing arcs,
/// breaking ties by proximity to the largest sub-rectangle's center.
fn pick_clear_room_label_pos(
    subs: &[Rect2],
    obstacles: &[Rect2],
    title_len: usize,
) -> crate::core::geom::Point2 {
    use crate::core::geom::Point2;
    let primary = subs
        .iter()
        .max_by(|a, b| a.area_m2().partial_cmp(&b.area_m2()).unwrap())
        .copied()
        .unwrap_or_else(|| Rect2::new(0.0, 0.0, 1000.0, 1000.0));
    let anchor = primary.center();

    let half_w = ((title_len as f64) * 52.0).clamp(520.0, 680.0);
    let half_h = 280.0_f64;

    let mut best_pt = anchor;
    let mut best_score = f64::INFINITY;

    for &sr in subs {
        let pad_x = half_w.min(sr.width() * 0.35);
        let pad_y = half_h.min(sr.depth() * 0.35);
        let x_lo = sr.min_x + pad_x;
        let x_hi = (sr.max_x - pad_x).max(x_lo);
        let y_lo = sr.min_y + pad_y;
        let y_hi = (sr.max_y - pad_y).max(y_lo);

        for ix in 0..=6 {
            let fx = ix as f64 / 6.0;
            let cx = x_lo + fx * (x_hi - x_lo);
            for iy in 0..=6 {
                let fy = iy as f64 / 6.0;
                let cy = y_lo + fy * (y_hi - y_lo);
                let lbox = Rect2::new(cx - half_w, cy - half_h, cx + half_w, cy + half_h);
                let mut overlap_pen = 0.0_f64;
                for &ob in obstacles {
                    if lbox.overlaps_strict(ob) {
                        overlap_pen += lbox.overlap_area(ob);
                    }
                }
                let dist = ((cx - anchor.x).powi(2) + (cy - anchor.y).powi(2)).sqrt();
                // Prioritize zero-overlap strongly, then stay close to room center
                let score = overlap_pen * 1000.0 + dist;
                if score < best_score {
                    best_score = score;
                    best_pt = Point2::new(cx, cy);
                }
            }
        }
    }
    best_pt
}

/// Project exterior-facing wall segments onto an axis, keeping the outermost segment wherever
/// two wings overlap in projection, so recessed wings (e.g. Hall beside Study & Kitchen) are
/// always preserved in the interior dimension chain!
fn project_outermost_spans(mut segs: Vec<(f64, f64, f64)>) -> Vec<(f64, f64)> {
    // Sort by outer priority descending (outermost walls claim their intervals first)
    segs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    let mut claimed: Vec<(f64, f64)> = Vec::new();
    for (a, b, _) in segs {
        if b - a < 100.0 {
            continue;
        }
        // Subtract already-claimed intervals from (a, b)
        let mut pieces = vec![(a, b)];
        for &(c0, c1) in &claimed {
            let mut next = Vec::new();
            for (p0, p1) in pieces {
                if c1 <= p0 + 1.0 || c0 >= p1 - 1.0 {
                    next.push((p0, p1));
                } else {
                    if c0 > p0 + 1.0 {
                        next.push((p0, c0.min(p1)));
                    }
                    if c1 < p1 - 1.0 {
                        next.push((c1.max(p0), p1));
                    }
                }
            }
            pieces = next;
        }
        for (p0, p1) in pieces {
            if p1 - p0 >= 150.0 {
                claimed.push((p0, p1));
            }
        }
    }
    claimed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    claimed
}

fn spans_equal(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| (x.0 - y.0).abs() < 2.0 && (x.1 - y.1).abs() < 2.0)
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

