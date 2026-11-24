use crate::ast::Side;
use crate::ir::*;
use crate::span::Severity;
use std::collections::BTreeSet;
use std::fmt::Write;

/// Render an architectural-quality SVG floor plan directly from the Canonical IR.
/// - Auto-fits canvas to ~1150px wide/tall
/// - Emits exact analytical SVG `<path ... A ...>` door-swing arcs (never polygonized)
/// - Renders walls (L-shapes & T-junctions), passages, windows, doors, fixtures + front clearances,
///   room dimensions, overall dimensions, and red verification violation overlays.
pub fn render_svg(ir: &CanonicalIr, annotated: bool) -> String {
    if ir.project.domain == "electronics" {
        return render_schematic_svg(ir, annotated);
    }
    let bounds = ir
        .project
        .exterior_bounds
        .unwrap_or(crate::geom::Rect2::new(0.0, 0.0, 5000.0, 4000.0));

    let margin_mm = 1150.0;
    let title_h_mm = 700.0;
    let world_w = bounds.width() + 2.0 * margin_mm;
    let world_h = bounds.depth() + 2.0 * margin_mm + title_h_mm;

    // Auto-scale so canvas fits cleanly around ~1150px max side
    let max_world = world_w.max(world_h);
    let scale_px_per_mm = (1150.0 / max_world).clamp(0.075, 0.13);

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
      .room-fill-err {{ fill: #fef2f2; stroke: #dc2626; stroke-width: 1.5; }}
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
            width_mm,
            depth_mm,
            ..
        } = e.geometry
        {
            let x = sx(min[0]);
            let y = sy(max[1]);
            let w = sl(width_mm);
            let h = sl(depth_mm);
            let is_violated = violated_entities.contains(e.id.as_str());
            let role = e.props.get("role").and_then(|v| v.as_str()).unwrap_or("");
            let lower = e.id.to_lowercase();
            let cls = if is_violated {
                "room-fill-err"
            } else if role == "wet" || lower.contains("bath") || lower.contains("wc") {
                "room-fill-bath"
            } else if role == "service" || lower.contains("kitchen") {
                "room-fill-alt"
            } else if role == "circulation" || lower.contains("hall") || lower.contains("corridor") {
                "room-fill-circ"
            } else {
                "room-fill"
            };
            let _ = writeln!(
                out,
                r#"    <rect id="room-{id}" class="{cls}" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}"/>"#,
                id = e.id
            );
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

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
            }

            // Label fixture if box is big enough
            if w >= 24.0 && h >= 14.0 {
                let _ = writeln!(
                    out,
                    r#"    <text class="fx-label" x="{:.2}" y="{:.2}">{}</text>"#,
                    x + w * 0.5,
                    y + h * 0.5 + 3.0,
                    e.id
                );
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

    // 5. Room labels & clear dimensions (rendered on top so never obscured)
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
        } = e.geometry
        {
            let x = sx(min[0]);
            let y = sy(max[1]);
            let w = sl(width_mm);
            let h = sl(depth_mm);
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            let _ = writeln!(
                out,
                r#"    <text class="room-title" x="{cx:.2}" y="{:.2}">{}</text>"#,
                cy - 7.0,
                e.id
            );
            let _ = writeln!(
                out,
                r#"    <text class="room-sub" x="{cx:.2}" y="{:.2}">{:.2}m × {:.2}m</text>"#,
                cy + 8.0,
                width_mm / 1000.0,
                depth_mm / 1000.0
            );
            let _ = writeln!(
                out,
                r#"    <text class="room-sub" x="{cx:.2}" y="{:.2}">{:.2} m²</text>"#,
                cy + 21.0,
                area_m2
            );
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 6. Overall exterior dimension lines (top + right)
    let _ = writeln!(out, r#"  <g id="dimensions">"#);
    let dim_off = 460.0;
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

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render an EDA-grade schematic SVG directly from Canonical IR (orthogonal routes, junctions,
/// typed pins, group boxes, colored power/GND/signal nets, and red ERC violation overlays).
fn render_schematic_svg(ir: &CanonicalIr, annotated: bool) -> String {
    let bounds = ir
        .project
        .exterior_bounds
        .unwrap_or(crate::geom::Rect2::new(0.0, 0.0, 254.0, 177.8));

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
      .net-lbl {{ font-family: monospace; font-size: 10px; font-weight: 700; text-anchor: middle; }}
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
                r#"    <text class="{l_cls}" x="{:.2}" y="{:.2}">{}</text>"#,
                sx(lp.x),
                sy(lp.y) + y_off,
                escape_xml(&route.net_id)
            );
        }
    }
    let _ = writeln!(out, r#"  </g>"#);

    // 3. Components & typed pins
    let _ = writeln!(out, r#"  <g id="components">"#);
    for e in &ir.entities {
        let EntityGeometryIr::Component {
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
        let x = sx(body_rect.min_x);
        let y = sy(body_rect.min_y);
        let w = sl(body_rect.width());
        let h = sl(body_rect.depth());

        let _ = writeln!(
            out,
            r#"    <rect id="comp-{id}" class="{b_cls}" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" rx="3"/>"#,
            id = e.id
        );
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
            if annotated || p_err {
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
