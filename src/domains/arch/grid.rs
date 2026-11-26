//! Structural Axis Grid, Columns, and Floor Elevation Tags for Complex Architecture (v1.5.0).
//!
//! Parses `structural_grid <Name> { ... }` extension blocks so large buildings (multi-unit
//! apartments, stepped villas, commercial halls, and multi-bay warehouses) can snap rooms
//! directly to structural grid axes (`grid_x_1`, `grid_y_A`, ...) with zero cascading chains!

use crate::core::geom::{Point2, Rect2};
use crate::core::ir::*;
use crate::core::span::{Diagnostic, Span};
use crate::syntax::ast::ProjectAst;
use crate::syntax::ext::ExtValue;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridAxis {
    pub label: String,
    pub coord_mm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridColumn {
    pub id: String,
    pub center: [f64; 2],
    pub width_mm: f64,
    pub depth_mm: f64,
    /// `"concrete"` | `"steel_i"`
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralGridModel {
    pub name: String,
    pub x_axes: Vec<GridAxis>,
    pub y_axes: Vec<GridAxis>,
    pub columns: Vec<GridColumn>,
    /// Optional architectural floor elevation tags per room id (e.g. `"±0.000"`, `"-0.030"`).
    pub room_elevations: BTreeMap<String, String>,
}

/// Extract the first `structural_grid` block (if any) into a typed model + solver parameter map.
pub fn parse_structural_grid(
    project: &ProjectAst,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(StructuralGridModel, Span)> {
    let block = project
        .extensions
        .iter()
        .find(|b| b.keyword == "structural_grid")?;

    let tree = match block.parse_tree() {
        Ok(t) => t,
        Err(d) => {
            diagnostics.push(d);
            return None;
        }
    };

    let name = block.name().unwrap_or_else(|| "Grid".to_string());
    let (ox, oy) = match tree.prop("origin") {
        Some(ExtValue::Tuple(vs)) if vs.len() == 2 => (
            vs[0].as_mm().unwrap_or(0.0),
            vs[1].as_mm().unwrap_or(0.0),
        ),
        _ => (0.0, 0.0),
    };

    let x_bays = extract_mm_list(tree.prop("x_bays"));
    let y_bays = extract_mm_list(tree.prop("y_bays"));
    let x_labels = extract_str_list(tree.prop("x_labels"));
    let y_labels = extract_str_list(tree.prop("y_labels"));

    let mut x_axes = Vec::new();
    let mut cur_x = ox;
    for i in 0..=x_bays.len() {
        let lbl = x_labels
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("{}", i + 1));
        x_axes.push(GridAxis {
            label: lbl,
            coord_mm: cur_x,
        });
        if i < x_bays.len() {
            cur_x += x_bays[i];
        }
    }

    let mut y_axes = Vec::new();
    let mut cur_y = oy;
    for i in 0..=y_bays.len() {
        let lbl = y_labels.get(i).cloned().unwrap_or_else(|| {
            let ch = (b'A' + (i as u8 % 26)) as char;
            ch.to_string()
        });
        y_axes.push(GridAxis {
            label: lbl,
            coord_mm: cur_y,
        });
        if i < y_bays.len() {
            cur_y += y_bays[i];
        }
    }

    // Parse structural column configuration
    let col_mode = tree.ident("columns").unwrap_or("none");
    let (cw, cd) = match tree.prop("column_size") {
        Some(ExtValue::Tuple(vs)) if vs.len() == 2 => (
            vs[0].as_mm().unwrap_or(350.0),
            vs[1].as_mm().unwrap_or(350.0),
        ),
        _ => (350.0, 350.0),
    };
    let col_kind = tree.ident("column_kind").unwrap_or("concrete").to_string();
    let exclude: BTreeSet<String> = extract_str_list(tree.prop("exclude_columns"))
        .into_iter()
        .collect();
    let explicit_list: BTreeSet<String> = match tree.prop("columns") {
        Some(ExtValue::List(_)) => extract_str_list(tree.prop("columns")).into_iter().collect(),
        _ => BTreeSet::new(),
    };

    let mut columns = Vec::new();
    let nx = x_axes.len();
    let ny = y_axes.len();
    for (xi, xa) in x_axes.iter().enumerate() {
        for (yi, ya) in y_axes.iter().enumerate() {
            let tag_xy = format!("{}{}", xa.label, ya.label);
            let tag_yx = format!("{}{}", ya.label, xa.label);
            if exclude.contains(&tag_xy) || exclude.contains(&tag_yx) {
                continue;
            }
            let include = if !explicit_list.is_empty() {
                explicit_list.contains(&tag_xy) || explicit_list.contains(&tag_yx)
            } else {
                match col_mode {
                    "all_intersections" | "all" => true,
                    "perimeter" => xi == 0 || xi + 1 == nx || yi == 0 || yi + 1 == ny,
                    _ => false,
                }
            };
            if include {
                columns.push(GridColumn {
                    id: format!("Col_{}_{}", xa.label, ya.label),
                    center: [xa.coord_mm, ya.coord_mm],
                    width_mm: cw,
                    depth_mm: cd,
                    kind: col_kind.clone(),
                });
            }
        }
    }

    // Optional architectural floor elevation benchmarks (`levels { Living: "±0.000"  TerraceW: "-0.030" }`)
    let mut room_elevations = BTreeMap::new();
    for lv in tree.children_of("levels") {
        for (k, (v, _)) in &lv.props {
            if let Some(s) = v.as_ident() {
                room_elevations.insert(k.clone(), s.to_string());
            } else if let Some(n) = v.as_f64() {
                room_elevations.insert(
                    k.clone(),
                    if n.abs() < 1e-6 {
                        "±0.000".to_string()
                    } else if n > 0.0 {
                        format!("+{n:.3}")
                    } else {
                        format!("{n:.3}")
                    },
                );
            }
        }
    }

    Some((
        StructuralGridModel {
            name,
            x_axes,
            y_axes,
            columns,
            room_elevations,
        },
        tree.span,
    ))
}

/// Inject all `grid_x_<label>` and `grid_y_<label>` axis coordinates into `params_mm`
/// so rooms and layout constraints can reference structural grid axes directly!
pub fn inject_grid_solver_params(project: &ProjectAst, params_mm: &mut BTreeMap<String, f64>) {
    let mut scratch = Vec::new();
    let Some((grid, _)) = parse_structural_grid(project, &mut scratch) else {
        return;
    };
    for xa in &grid.x_axes {
        params_mm.insert(format!("grid_x_{}", xa.label), xa.coord_mm);
        params_mm.insert(format!("gx_{}", xa.label), xa.coord_mm);
    }
    for ya in &grid.y_axes {
        params_mm.insert(format!("grid_y_{}", ya.label), ya.coord_mm);
        params_mm.insert(format!("gy_{}", ya.label), ya.coord_mm);
    }
}

pub fn extract_grid_from_ir(ir: &CanonicalIr) -> Option<StructuralGridModel> {
    ir.entities.iter().find_map(|e| {
        if e.kind == "arch.structural_grid" {
            if let EntityGeometryIr::Custom { data, .. } = &e.geometry {
                return serde_json::from_value::<StructuralGridModel>(data.clone()).ok();
            }
        }
        None
    })
}

pub fn column_rect(c: &GridColumn) -> Rect2 {
    Rect2::new(
        c.center[0] - c.width_mm * 0.5,
        c.center[1] - c.depth_mm * 0.5,
        c.center[0] + c.width_mm * 0.5,
        c.center[1] + c.depth_mm * 0.5,
    )
}

pub fn grid_bounds(g: &StructuralGridModel) -> Option<Rect2> {
    let min_x = g.x_axes.iter().map(|a| a.coord_mm).reduce(f64::min)?;
    let max_x = g.x_axes.iter().map(|a| a.coord_mm).reduce(f64::max)?;
    let min_y = g.y_axes.iter().map(|a| a.coord_mm).reduce(f64::min)?;
    let max_y = g.y_axes.iter().map(|a| a.coord_mm).reduce(f64::max)?;
    Some(Rect2::new(min_x, min_y, max_x, max_y))
}

/// Check that no structural grid column collides with a door swing or opening cutout.
pub fn verify_column_clearances(
    grid: &StructuralGridModel,
    door_arcs: &[(String, crate::core::geom::QuarterDisk2)],
    opening_cutouts: &[(String, Rect2)],
    span: &Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> EvidenceRecord {
    let mut clashes = Vec::new();
    for col in &grid.columns {
        let cr = column_rect(col);
        for (did, qd) in door_arcs {
            if qd.overlaps_rect(cr) {
                clashes.push(format!("{}<->{}", col.id, did));
                diagnostics.push(
                    Diagnostic::error(
                        "E_COLUMN_DOOR_CLASH",
                        "verification",
                        format!(
                            "Structural column '{}' collides with door '{}' swing arc",
                            col.id, did
                        ),
                        Some(span.clone()),
                    )
                    .with_entities(vec![col.id.clone(), did.clone()]),
                );
            }
        }
        for (oid, orect) in opening_cutouts {
            if cr.overlaps_strict(*orect) {
                clashes.push(format!("{}<->{}", col.id, oid));
                diagnostics.push(
                    Diagnostic::error(
                        "E_COLUMN_OPENING_CLASH",
                        "verification",
                        format!(
                            "Structural column '{}' blocks opening '{}'",
                            col.id, oid
                        ),
                        Some(span.clone()),
                    )
                    .with_entities(vec![col.id.clone(), oid.clone()]),
                );
            }
        }
    }
    EvidenceRecord {
        check_id: "V7_structural_column_clearance".to_string(),
        status: if clashes.is_empty() {
            VerificationStatus::ExactlyChecked
        } else {
            VerificationStatus::Violated
        },
        method: "exact_column_opening_door_clearance_v1.5".to_string(),
        tolerance_mm: Some(1e-6),
        subjects: grid.columns.iter().map(|c| c.id.clone()).collect(),
        details: if clashes.is_empty() {
            format!(
                "All {} structural column(s) clear of door swings & opening cutouts",
                grid.columns.len()
            )
        } else {
            format!("Column clashes: {}", clashes.join(", "))
        },
        repair_suggestions: Vec::new(),
    }
}

fn extract_mm_list(val: Option<&ExtValue>) -> Vec<f64> {
    match val {
        Some(ExtValue::List(vs)) => vs.iter().filter_map(|v| v.as_mm()).collect(),
        _ => Vec::new(),
    }
}

fn extract_str_list(val: Option<&ExtValue>) -> Vec<String> {
    match val {
        Some(ExtValue::List(vs)) => vs
            .iter()
            .filter_map(|v| match v {
                ExtValue::Str(s) | ExtValue::Ident(s) => Some(s.clone()),
                ExtValue::Number { value, .. } => Some(format!("{}", *value as i64)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[allow(dead_code)]
pub fn point_on_grid(grid: &StructuralGridModel, x_lbl: &str, y_lbl: &str) -> Option<Point2> {
    let x = grid.x_axes.iter().find(|a| a.label == x_lbl)?.coord_mm;
    let y = grid.y_axes.iter().find(|a| a.label == y_lbl)?.coord_mm;
    Some(Point2::new(x, y))
}
