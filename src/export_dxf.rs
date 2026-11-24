use crate::geom::Rect2;
use crate::ir::*;
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
    let _ = writeln!(out, "0\nTABLE\n2\nLAYER\n70\n8");
    for (name, color) in [
        ("WALLS_EXT", 7),
        ("WALLS_INT", 8),
        ("ROOMS", 5),
        ("DOORS", 3),
        ("WINDOWS", 4),
        ("PASSAGES", 9),
        ("FIXTURES", 6),
        ("ANNOTATIONS", 2),
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
            } if e.kind == "arch.room" => {
                let r = Rect2::new(min[0], min[1], max[0], max[1]);
                write_rect_lines(&mut out, "ROOMS", r);
                let c = r.center();
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
            _ => {}
        }
    }

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
