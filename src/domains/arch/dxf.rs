use crate::core::geom::Rect2;
use crate::core::ir::*;
use std::fmt::Write;

/// Export a clean, standards-compliant AutoCAD R12 ASCII DXF with dedicated layers:
/// - WALLS_EXT (ACI 7)
/// - WALLS_INT (ACI 8)
/// - ROOMS     (ACI 5)
/// - DOORS     (ACI 3, exact analytical ARC + LINE leaf)
/// - WINDOWS   (ACI 4)
/// - PASSAGES  (ACI 9)
/// - FIXTURES  (ACI 6)
/// - ANNOTATIONS (ACI 2)
pub fn export_dxf(ir: &CanonicalIr) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "0\nSECTION\n2\nHEADER");
    let _ = writeln!(out, "9\n$ACADVER\n1\nAC1009");
    let _ = writeln!(out, "9\n$INSUNITS\n70\n4");
    let _ = writeln!(out, "0\nENDSEC");

    let _ = writeln!(out, "0\nSECTION\n2\nTABLES");
    // Registered XDATA application so AutoCAD preserves the EDL_ID tags through edits.
    let _ = writeln!(out, "0\nTABLE\n2\nAPPID\n70\n1\n0\nAPPID\n2\nEDL\n70\n0\n0\nENDTAB");
    let _ = writeln!(out, "0\nTABLE\n2\nLAYER\n70\n22");
    for (name, color) in [
        ("WALLS_EXT", 7),
        ("WALLS_INT", 8),
        ("ROOMS", 5),
        ("DOORS", 3),
        ("WINDOWS", 4),
        ("PASSAGES", 9),
        ("FIXTURES", 6),
        ("ANNOTATIONS", 2),
        ("GRID_AXES", 8),
        ("GRID_BUBBLES", 7),
        ("COLUMNS", 1),
        ("ELEC_PANEL", 1),
        ("ELEC_LIGHTING", 2),
        ("ELEC_SWITCHES", 30),
        ("ELEC_POWER", 4),
        ("ELEC_WIRING", 6),
        ("PLUMB_COLD", 5),
        ("PLUMB_HOT", 1),
        ("PLUMB_DRAIN", 32),
        ("PLUMB_VENT", 3),
        ("PLUMB_EQUIP", 4),
        ("EDL_ANCHORS", 8),
    ] {
        let _ = writeln!(
            out,
            "0\nLAYER\n2\n{name}\n70\n0\n62\n{color}\n6\nCONTINUOUS"
        );
    }
    let _ = writeln!(out, "0\nENDTAB\n0\nENDSEC");

    let _ = writeln!(out, "0\nSECTION\n2\nENTITIES");

    for cell in &ir.wall_solids {
        let layer = if cell.kind == "exterior" {
            "WALLS_EXT"
        } else {
            "WALLS_INT"
        };
        write_rect_lines(&mut out, layer, cell.rect);
    }

    for e in &ir.entities {
        match &e.geometry {
            EntityGeometryIr::Rect {
                min,
                max,
                width_mm,
                depth_mm,
                area_m2,
                notch_rect,
            } if e.kind == "arch.room" => {
                let r = Rect2::new(min[0], min[1], max[0], max[1]);
                let subs = crate::domains::arch::compile::decompose_room_rect(r, *notch_rect);
                for sr in &subs {
                    write_rect_lines(&mut out, "ROOMS", *sr);
                }
                let best = subs
                    .iter()
                    .max_by(|a, b| a.area_m2().partial_cmp(&b.area_m2()).unwrap())
                    .copied()
                    .unwrap_or(r);
                let c = best.center();
                write_text(&mut out, "ANNOTATIONS", c.x, c.y + 120.0, 160.0, &e.id);
                write_text(
                    &mut out,
                    "ANNOTATIONS",
                    c.x,
                    c.y - 100.0,
                    110.0,
                    &format!(
                        "{:.2}m x {:.2}m ({:.2} m2)",
                        width_mm / 1000.0,
                        depth_mm / 1000.0,
                        area_m2
                    ),
                );
            }
            EntityGeometryIr::Door {
                leaf_open,
                swing_arc,
                ..
            } => {
                write_line(
                    &mut out,
                    "DOORS",
                    leaf_open.p0.x,
                    leaf_open.p0.y,
                    leaf_open.p1.x,
                    leaf_open.p1.y,
                );
                let (a_start, a_end) = if swing_arc.sweep_deg >= 0.0 {
                    (swing_arc.start_deg, swing_arc.start_deg + swing_arc.sweep_deg)
                } else {
                    (swing_arc.start_deg + swing_arc.sweep_deg, swing_arc.start_deg)
                };
                write_arc(
                    &mut out,
                    "DOORS",
                    swing_arc.center.x,
                    swing_arc.center.y,
                    swing_arc.radius,
                    normalize_deg(a_start),
                    normalize_deg(a_end),
                );
            }
            EntityGeometryIr::Window {
                cutout_rect,
                opening_segment,
            } => {
                write_rect_lines(&mut out, "WINDOWS", *cutout_rect);
                write_line(
                    &mut out,
                    "WINDOWS",
                    opening_segment.p0.x,
                    opening_segment.p0.y,
                    opening_segment.p1.x,
                    opening_segment.p1.y,
                );
            }
            EntityGeometryIr::Passage { opening_segment, .. } => {
                write_line(
                    &mut out,
                    "PASSAGES",
                    opening_segment.p0.x,
                    opening_segment.p0.y,
                    opening_segment.p1.x,
                    opening_segment.p1.y,
                );
            }
            EntityGeometryIr::Fixture { body_rect, .. } => {
                write_rect_lines(&mut out, "FIXTURES", *body_rect);
                let c = body_rect.center();
                write_text(&mut out, "ANNOTATIONS", c.x, c.y, 90.0, &e.id);
            }
            EntityGeometryIr::MepDevice {
                device_kind, pos, ..
            } => {
                let layer = match device_kind.as_str() {
                    "panel" => "ELEC_PANEL",
                    "light" => "ELEC_LIGHTING",
                    "switch" => "ELEC_SWITCHES",
                    _ => "ELEC_POWER",
                };
                if device_kind == "panel" {
                    write_rect_lines(
                        &mut out,
                        layer,
                        Rect2::new(pos.x - 150.0, pos.y - 75.0, pos.x + 150.0, pos.y + 75.0),
                    );
                } else {
                    write_arc(&mut out, layer, pos.x, pos.y, 85.0, 0.0, 359.9);
                    if device_kind == "light" {
                        write_line(&mut out, layer, pos.x - 60.0, pos.y - 60.0, pos.x + 60.0, pos.y + 60.0);
                        write_line(&mut out, layer, pos.x - 60.0, pos.y + 60.0, pos.x + 60.0, pos.y - 60.0);
                    }
                }
                write_text(&mut out, layer, pos.x, pos.y + 110.0, 75.0, &e.id);
            }
            _ => {}
        }
    }

    // Stable identity anchors for bidirectional sync (`edl sync --from-dxf`): one tagged POINT per movable entity,
    // placed inside its footprint so a normal window-select + move in AutoCAD carries it along.
    for e in &ir.entities {
        if let Some((kind, x, y)) = crate::core::sync::dxf_anchor(e) {
            let _ = writeln!(
                out,
                "0\nPOINT\n8\nEDL_ANCHORS\n10\n{x:.4}\n20\n{y:.4}\n30\n0.0\n1001\nEDL\n1000\n{}\n1000\n{kind}",
                e.id
            );
        }
    }

    for w in &ir.mep_wires {
        let layer = if w.run_kind == "lighting" {
            "ELEC_LIGHTING"
        } else if w.run_kind == "switch_leg" {
            "ELEC_SWITCHES"
        } else {
            "ELEC_WIRING"
        };
        for seg in &w.segments {
            write_line(&mut out, layer, seg.p0.x, seg.p0.y, seg.p1.x, seg.p1.y);
        }
    }

    if let Some(grid) = crate::domains::arch::grid::extract_grid_from_ir(ir) {
        let bounds = ir
            .project
            .exterior_bounds
            .unwrap_or(Rect2::new(0.0, 0.0, 5000.0, 4000.0));
        let pad = 1200.0;
        for xa in &grid.x_axes {
            write_line(&mut out, "GRID_AXES", xa.coord_mm, bounds.min_y - pad, xa.coord_mm, bounds.max_y + pad);
            write_arc(&mut out, "GRID_BUBBLES", xa.coord_mm, bounds.min_y - pad - 250.0, 220.0, 0.0, 359.9);
            write_text(&mut out, "GRID_BUBBLES", xa.coord_mm - 70.0, bounds.min_y - pad - 320.0, 160.0, &xa.label);
        }
        for ya in &grid.y_axes {
            write_line(&mut out, "GRID_AXES", bounds.min_x - pad, ya.coord_mm, bounds.max_x + pad, ya.coord_mm);
            write_arc(&mut out, "GRID_BUBBLES", bounds.min_x - pad - 250.0, ya.coord_mm, 220.0, 0.0, 359.9);
            write_text(&mut out, "GRID_BUBBLES", bounds.min_x - pad - 320.0, ya.coord_mm - 70.0, 160.0, &ya.label);
        }
        for col in &grid.columns {
            write_rect_lines(&mut out, "COLUMNS", crate::domains::arch::grid::column_rect(col));
        }
    }

    crate::domains::mep_plumbing::render::append_plumbing_dxf(ir, &mut out);

    let _ = writeln!(out, "0\nENDSEC\n0\nEOF");
    out
}

fn write_line(out: &mut String, layer: &str, x1: f64, y1: f64, x2: f64, y2: f64) {
    let _ = writeln!(
        out,
        "0\nLINE\n8\n{layer}\n10\n{x1:.4}\n20\n{y1:.4}\n30\n0.0\n11\n{x2:.4}\n21\n{y2:.4}\n31\n0.0"
    );
}

fn write_rect_lines(out: &mut String, layer: &str, r: Rect2) {
    write_line(out, layer, r.min_x, r.min_y, r.max_x, r.min_y);
    write_line(out, layer, r.max_x, r.min_y, r.max_x, r.max_y);
    write_line(out, layer, r.max_x, r.max_y, r.min_x, r.max_y);
    write_line(out, layer, r.min_x, r.max_y, r.min_x, r.min_y);
}

fn write_arc(
    out: &mut String,
    layer: &str,
    cx: f64,
    cy: f64,
    r: f64,
    start_deg: f64,
    end_deg: f64,
) {
    let _ = writeln!(
        out,
        "0\nARC\n8\n{layer}\n10\n{cx:.4}\n20\n{cy:.4}\n30\n0.0\n40\n{r:.4}\n50\n{start_deg:.4}\n51\n{end_deg:.4}"
    );
}

fn write_text(out: &mut String, layer: &str, x: f64, y: f64, height: f64, text: &str) {
    let _ = writeln!(
        out,
        "0\nTEXT\n8\n{layer}\n10\n{x:.4}\n20\n{y:.4}\n30\n0.0\n40\n{height:.2}\n1\n{text}"
    );
}

fn normalize_deg(deg: f64) -> f64 {
    let mut d = deg % 360.0;
    if d < 0.0 {
        d += 360.0;
    }
    d
}
