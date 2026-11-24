use crate::core::geom::{Rect2, Side};
use crate::core::ir::{ConstraintStrength, EquationRecord};
use crate::core::span::{Diagnostic, Span};
use crate::domains::arch::ast::*;
use crate::syntax::ast::ProjectAst;
use std::collections::{BTreeMap, BTreeSet};

const SOLVE_EPS: f64 = 1e-7;

#[derive(Debug, Clone)]
pub struct SolveOutput {
    pub room_rects: BTreeMap<String, Rect2>,
    pub equations: Vec<EquationRecord>,
    pub remaining_dof: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
struct LinearEq {
    id: String,
    strength: ConstraintStrength,
    expr: String,
    span: Option<Span>,
    /// Coefficients per variable index (lhs - rhs = 0 => sum(c_i * x_i) = rhs_const)
    coeffs: Vec<f64>,
    rhs: f64,
}

/// Exact Tier 1 + Tier 2 linear room solver using Gauss-Jordan RREF with row provenance.
/// Reports:
/// - exact solved room rectangles when uniquely determined,
/// - exact free variables (DOF) when under-constrained,
/// - exact conflicting equation IDs & residuals when over-constrained/inconsistent.
pub fn solve_rooms(project: &ProjectAst) -> SolveOutput {
    let mut diagnostics = Vec::new();
    let mut room_index: BTreeMap<String, usize> = BTreeMap::new();
    let mut var_names: Vec<String> = Vec::new();

    for (idx, r) in project.arch.rooms.iter().enumerate() {
        if room_index.insert(r.id.clone(), idx).is_some() {
            diagnostics.push(Diagnostic::error(
                "E_DUP_ROOM",
                "semantics",
                format!("Duplicate room declaration '{}'", r.id),
                Some(r.span.clone()),
            ));
        }
        var_names.push(format!("{}.west", r.id));
        var_names.push(format!("{}.east", r.id));
        var_names.push(format!("{}.south", r.id));
        var_names.push(format!("{}.north", r.id));
    }

    let nvars = var_names.len();
    let mut req_eqs: Vec<LinearEq> = Vec::new();
    let mut pref_eqs: Vec<LinearEq> = Vec::new();
    let mut eq_counter = 0usize;

    let var_of = |room: &str, side: Side, room_index: &BTreeMap<String, usize>| -> Option<usize> {
        let base = *room_index.get(room)? * 4;
        let off = match side {
            Side::West => 0,
            Side::East => 1,
            Side::South => 2,
            Side::North => 3,
        };
        Some(base + off)
    };

    // 1. Equations from room clear_size and anchor
    for r in &project.arch.rooms {
        if let Some((w_q, d_q)) = r.clear_size {
            match w_q.to_mm() {
                Ok(w_mm) => {
                    if w_mm <= 0.0 {
                        diagnostics.push(
                            Diagnostic::error(
                                "E_ROOM_NONPOSITIVE",
                                "semantics",
                                format!("Room '{}' width must be > 0mm (got {}mm)", r.id, w_mm),
                                Some(r.span.clone()),
                            )
                            .with_entities(vec![r.id.clone()]),
                        );
                    }
                    let mut c = vec![0.0; nvars];
                    c[var_of(&r.id, Side::East, &room_index).unwrap()] = 1.0;
                    c[var_of(&r.id, Side::West, &room_index).unwrap()] = -1.0;
                    eq_counter += 1;
                    req_eqs.push(LinearEq {
                        id: format!("C{:02}", eq_counter),
                        strength: ConstraintStrength::Require,
                        expr: format!("{}.east - {}.west == {}mm", r.id, r.id, w_mm),
                        span: Some(r.span.clone()),
                        coeffs: c,
                        rhs: w_mm,
                    });
                }
                Err(msg) => diagnostics.push(Diagnostic::error(
                    "E_UNIT_MISMATCH",
                    "units",
                    format!("Room '{}' width: {msg}", r.id),
                    Some(r.span.clone()),
                )),
            }

            match d_q.to_mm() {
                Ok(d_mm) => {
                    if d_mm <= 0.0 {
                        diagnostics.push(
                            Diagnostic::error(
                                "E_ROOM_NONPOSITIVE",
                                "semantics",
                                format!("Room '{}' depth must be > 0mm (got {}mm)", r.id, d_mm),
                                Some(r.span.clone()),
                            )
                            .with_entities(vec![r.id.clone()]),
                        );
                    }
                    let mut c = vec![0.0; nvars];
                    c[var_of(&r.id, Side::North, &room_index).unwrap()] = 1.0;
                    c[var_of(&r.id, Side::South, &room_index).unwrap()] = -1.0;
                    eq_counter += 1;
                    req_eqs.push(LinearEq {
                        id: format!("C{:02}", eq_counter),
                        strength: ConstraintStrength::Require,
                        expr: format!("{}.north - {}.south == {}mm", r.id, r.id, d_mm),
                        span: Some(r.span.clone()),
                        coeffs: c,
                        rhs: d_mm,
                    });
                }
                Err(msg) => diagnostics.push(Diagnostic::error(
                    "E_UNIT_MISMATCH",
                    "units",
                    format!("Room '{}' depth: {msg}", r.id),
                    Some(r.span.clone()),
                )),
            }
        }

        if let Some(ref anchor) = r.anchor {
            if !project.frames.iter().any(|f| f.name == anchor.frame) {
                diagnostics.push(Diagnostic::error(
                    "E_FRAME_UNKNOWN",
                    "frames",
                    format!(
                        "Room '{}' anchor references undeclared frame '{}'",
                        r.id, anchor.frame
                    ),
                    Some(anchor.span.clone()),
                ));
            }
            if anchor.target != "origin" {
                diagnostics.push(Diagnostic::error(
                    "E_ANCHOR_TARGET",
                    "semantics",
                    format!("Unsupported anchor target '{}' (expected 'origin')", anchor.target),
                    Some(anchor.span.clone()),
                ));
            }
            let (x_side, y_side) = match anchor.corner {
                Corner::Southwest => (Side::West, Side::South),
                Corner::Southeast => (Side::East, Side::South),
                Corner::Northwest => (Side::West, Side::North),
                Corner::Northeast => (Side::East, Side::North),
            };
            let mut cx = vec![0.0; nvars];
            cx[var_of(&r.id, x_side, &room_index).unwrap()] = 1.0;
            eq_counter += 1;
            req_eqs.push(LinearEq {
                id: format!("C{:02}", eq_counter),
                strength: ConstraintStrength::Require,
                expr: format!("{}.{} == 0mm (anchor)", r.id, x_side.as_str()),
                span: Some(anchor.span.clone()),
                coeffs: cx,
                rhs: 0.0,
            });

            let mut cy = vec![0.0; nvars];
            cy[var_of(&r.id, y_side, &room_index).unwrap()] = 1.0;
            eq_counter += 1;
            req_eqs.push(LinearEq {
                id: format!("C{:02}", eq_counter),
                strength: ConstraintStrength::Require,
                expr: format!("{}.{} == 0mm (anchor)", r.id, y_side.as_str()),
                span: Some(anchor.span.clone()),
                coeffs: cy,
                rhs: 0.0,
            });
        }
    }

    // Build parameter lookup map (built-in wall thicknesses + declared parameters)
    let mut params_mm: BTreeMap<String, f64> = BTreeMap::new();
    if let Ok(v) = project.arch.defaults.interior_wall.to_mm() {
        params_mm.insert("interior_wall".to_string(), v);
    }
    if let Ok(v) = project.arch.defaults.exterior_wall.to_mm() {
        params_mm.insert("exterior_wall".to_string(), v);
    }
    for p in &project.parameters {
        match p.value.to_mm() {
            Ok(v) => {
                params_mm.insert(p.name.clone(), v);
            }
            Err(msg) => {
                diagnostics.push(Diagnostic::error(
                    "E_UNIT_MISMATCH",
                    "units",
                    format!("Parameter '{}': {msg}", p.name),
                    Some(p.span.clone()),
                ));
            }
        }
    }

    // 2. Equations from layout blocks
    for layout in &project.arch.layouts {
        if !project.frames.iter().any(|f| f.name == layout.frame) {
            diagnostics.push(Diagnostic::error(
                "E_FRAME_UNKNOWN",
                "frames",
                format!("Layout block references undeclared frame '{}'", layout.frame),
                Some(layout.span.clone()),
            ));
        }
        for c in &layout.constraints {
            let mut coeffs = vec![0.0; nvars];
            let mut constant = 0.0;
            let ok_l = eval_linear_expr(
                &c.lhs,
                1.0,
                &room_index,
                &params_mm,
                &mut coeffs,
                &mut constant,
                &mut diagnostics,
            );
            let ok_r = eval_linear_expr(
                &c.rhs,
                -1.0,
                &room_index,
                &params_mm,
                &mut coeffs,
                &mut constant,
                &mut diagnostics,
            );
            if ok_l && ok_r {
                eq_counter += 1;
                let leq = LinearEq {
                    id: format!("C{:02}", eq_counter),
                    strength: c.strength,
                    expr: c.raw_text.clone(),
                    span: Some(c.span.clone()),
                    coeffs,
                    rhs: -constant,
                };
                match c.strength {
                    ConstraintStrength::Require => req_eqs.push(leq),
                    ConstraintStrength::Prefer => pref_eqs.push(leq),
                }
            }
        }
    }

    // 3. Incremental Gaussian Elimination with provenance over `require` equations
    let mut basis: Vec<PivotRow> = Vec::new();
    let mut eq_records: Vec<EquationRecord> = Vec::new();

    for eq in &req_eqs {
        match insert_equation(&mut basis, eq) {
            InsertOutcome::Independent => {
                eq_records.push(EquationRecord {
                    id: eq.id.clone(),
                    tier: 2,
                    strength: eq.strength,
                    expr: eq.expr.clone(),
                    span: eq.span.clone(),
                    status: "EXACTLY_CHECKED".to_string(),
                    residual_mm: Some(0.0),
                });
            }
            InsertOutcome::Redundant => {
                eq_records.push(EquationRecord {
                    id: eq.id.clone(),
                    tier: 2,
                    strength: eq.strength,
                    expr: eq.expr.clone(),
                    span: eq.span.clone(),
                    status: "EXACTLY_CHECKED".to_string(),
                    residual_mm: Some(0.0),
                });
            }
            InsertOutcome::Contradiction {
                residual,
                involved_ids,
            } => {
                eq_records.push(EquationRecord {
                    id: eq.id.clone(),
                    tier: 2,
                    strength: eq.strength,
                    expr: eq.expr.clone(),
                    span: eq.span.clone(),
                    status: "VIOLATED".to_string(),
                    residual_mm: Some(residual.abs()),
                });
                let involved_list = involved_ids.join(", ");
                diagnostics.push(
                    Diagnostic::error(
                        "E_SOLVER_CONTRADICTION",
                        "solver",
                        format!(
                            "Over-constrained / contradictory linear constraint {} ('{}'): conflict residual {:.3}mm with [{}]",
                            eq.id,
                            eq.expr,
                            residual.abs(),
                            involved_list
                        ),
                        eq.span.clone(),
                    )
                    .with_measurement(residual.abs(), 0.0, "mm")
                    .with_suggestion(format!(
                        "Remove or relax one of the conflicting constraints: {involved_list}"
                    )),
                );
            }
        }
    }

    // 4. Apply `prefer` equations only if they are independent (never contradict `require`)
    for eq in &pref_eqs {
        match insert_equation(&mut basis, eq) {
            InsertOutcome::Independent | InsertOutcome::Redundant => {
                eq_records.push(EquationRecord {
                    id: eq.id.clone(),
                    tier: 2,
                    strength: eq.strength,
                    expr: eq.expr.clone(),
                    span: eq.span.clone(),
                    status: "EXACTLY_CHECKED".to_string(),
                    residual_mm: Some(0.0),
                });
            }
            InsertOutcome::Contradiction { residual, .. } => {
                eq_records.push(EquationRecord {
                    id: eq.id.clone(),
                    tier: 2,
                    strength: eq.strength,
                    expr: eq.expr.clone(),
                    span: eq.span.clone(),
                    status: "VIOLATED".to_string(),
                    residual_mm: Some(residual.abs()),
                });
            }
        }
    }

    // 5. Back-substitute (full RREF) to inspect solved vs free variables
    reduce_to_rref(&mut basis);
    let pivot_cols: BTreeSet<usize> = basis.iter().map(|r| r.pivot_col).collect();
    let remaining_dof: Vec<String> = (0..nvars)
        .filter(|col| !pivot_cols.contains(col))
        .map(|col| var_names[col].clone())
        .collect();

    if !remaining_dof.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "E_SOLVER_UNDERCONSTRAINED",
                "solver",
                format!(
                    "Model is under-constrained: {} remaining degree(s) of freedom: [{}]",
                    remaining_dof.len(),
                    remaining_dof.join(", ")
                ),
                None,
            )
            .with_suggestion(format!(
                "Add clear_size, anchor, or layout require constraints fixing: {}",
                remaining_dof.join(", ")
            )),
        );
    }

    // Extract uniquely determined variables (rows with no nonzero non-pivot entries)
    let mut solved_vals: Vec<Option<f64>> = vec![None; nvars];
    for row in &basis {
        let has_free = row
            .coeffs
            .iter()
            .enumerate()
            .any(|(j, &c)| j != row.pivot_col && c.abs() > SOLVE_EPS);
        if !has_free {
            solved_vals[row.pivot_col] = Some(row.rhs);
        }
    }

    let mut room_rects = BTreeMap::new();
    for r in &project.arch.rooms {
        let base = room_index[&r.id] * 4;
        if let (Some(w), Some(e), Some(s), Some(n)) = (
            solved_vals[base],
            solved_vals[base + 1],
            solved_vals[base + 2],
            solved_vals[base + 3],
        ) {
            if e <= w + SOLVE_EPS || n <= s + SOLVE_EPS {
                diagnostics.push(
                    Diagnostic::error(
                        "E_ROOM_DEGENERATE",
                        "geometry",
                        format!(
                            "Room '{}' resolved to non-positive dimensions ({:.1}mm x {:.1}mm)",
                            r.id,
                            e - w,
                            n - s
                        ),
                        Some(r.span.clone()),
                    )
                    .with_entities(vec![r.id.clone()]),
                );
            } else {
                room_rects.insert(r.id.clone(), Rect2::new(w, s, e, n));
            }
        }
    }

    SolveOutput {
        room_rects,
        equations: eq_records,
        remaining_dof,
        diagnostics,
    }
}

fn eval_linear_expr(
    expr: &LinearExprAst,
    sign: f64,
    room_index: &BTreeMap<String, usize>,
    params_mm: &BTreeMap<String, f64>,
    coeffs: &mut [f64],
    constant: &mut f64,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    match expr {
        LinearExprAst::Param { name, span } => {
            if let Some(&mm) = params_mm.get(name) {
                *constant += sign * mm;
                true
            } else {
                diagnostics.push(Diagnostic::error(
                    "E_REF_UNKNOWN_PARAM",
                    "semantics",
                    format!(
                        "Constraint references undeclared parameter '{name}' (built-ins: interior_wall, exterior_wall)"
                    ),
                    Some(span.clone()),
                ));
                false
            }
        }
        LinearExprAst::RoomEdge { room, side, span } => {
            let Some(&idx) = room_index.get(room) else {
                diagnostics.push(Diagnostic::error(
                    "E_REF_UNKNOWN_ROOM",
                    "semantics",
                    format!("Constraint references undeclared room '{room}'"),
                    Some(span.clone()),
                ));
                return false;
            };
            let off = match side {
                Side::West => 0,
                Side::East => 1,
                Side::South => 2,
                Side::North => 3,
            };
            coeffs[idx * 4 + off] += sign;
            true
        }
        LinearExprAst::RoomDim { room, dim, span } => {
            let Some(&idx) = room_index.get(room) else {
                diagnostics.push(Diagnostic::error(
                    "E_REF_UNKNOWN_ROOM",
                    "semantics",
                    format!("Constraint references undeclared room '{room}'"),
                    Some(span.clone()),
                ));
                return false;
            };
            match dim {
                RoomDimKind::Width => {
                    coeffs[idx * 4 + 1] += sign;
                    coeffs[idx * 4] -= sign;
                }
                RoomDimKind::Depth => {
                    coeffs[idx * 4 + 3] += sign;
                    coeffs[idx * 4 + 2] -= sign;
                }
            }
            true
        }
        LinearExprAst::Quantity(q, span) => match q.to_mm() {
            Ok(mm) => {
                *constant += sign * mm;
                true
            }
            Err(msg) => {
                diagnostics.push(Diagnostic::error(
                    "E_UNIT_MISMATCH",
                    "units",
                    format!("Invalid quantity in layout constraint: {msg}"),
                    Some(span.clone()),
                ));
                false
            }
        },
        LinearExprAst::Add(a, b) => {
            let ok_a = eval_linear_expr(a, sign, room_index, params_mm, coeffs, constant, diagnostics);
            let ok_b = eval_linear_expr(b, sign, room_index, params_mm, coeffs, constant, diagnostics);
            ok_a && ok_b
        }
        LinearExprAst::Sub(a, b) => {
            let ok_a = eval_linear_expr(a, sign, room_index, params_mm, coeffs, constant, diagnostics);
            let ok_b = eval_linear_expr(b, -sign, room_index, params_mm, coeffs, constant, diagnostics);
            ok_a && ok_b
        }
        LinearExprAst::Scale(k, inner) => {
            eval_linear_expr(inner, sign * k, room_index, params_mm, coeffs, constant, diagnostics)
        }
    }
}

#[derive(Debug, Clone)]
struct PivotRow {
    pivot_col: usize,
    coeffs: Vec<f64>,
    rhs: f64,
    provenance: BTreeSet<String>,
}

enum InsertOutcome {
    Independent,
    Redundant,
    Contradiction {
        residual: f64,
        involved_ids: Vec<String>,
    },
}

fn insert_equation(basis: &mut Vec<PivotRow>, eq: &LinearEq) -> InsertOutcome {
    let mut c = eq.coeffs.clone();
    let mut rhs = eq.rhs;
    let mut prov = BTreeSet::new();
    prov.insert(eq.id.clone());

    for row in basis.iter() {
        let factor = c[row.pivot_col];
        if factor.abs() > SOLVE_EPS {
            for j in 0..c.len() {
                c[j] -= factor * row.coeffs[j];
            }
            rhs -= factor * row.rhs;
            for p in &row.provenance {
                prov.insert(p.clone());
            }
        }
    }

    // Find leading nonzero column
    if let Some((pcol, &pval)) = c
        .iter()
        .enumerate()
        .find(|(_, v)| v.abs() > SOLVE_EPS)
    {
        for v in &mut c {
            *v /= pval;
        }
        rhs /= pval;
        basis.push(PivotRow {
            pivot_col: pcol,
            coeffs: c,
            rhs,
            provenance: prov,
        });
        InsertOutcome::Independent
    } else if rhs.abs() <= SOLVE_EPS {
        InsertOutcome::Redundant
    } else {
        InsertOutcome::Contradiction {
            residual: rhs,
            involved_ids: prov.into_iter().collect(),
        }
    }
}

fn reduce_to_rref(basis: &mut Vec<PivotRow>) {
    basis.sort_by_key(|r| r.pivot_col);
    let m = basis.len();
    for i in (0..m).rev() {
        let pcol = basis[i].pivot_col;
        let pi_coeffs = basis[i].coeffs.clone();
        let pi_rhs = basis[i].rhs;
        for k in 0..i {
            let factor = basis[k].coeffs[pcol];
            if factor.abs() > SOLVE_EPS {
                for j in 0..pi_coeffs.len() {
                    basis[k].coeffs[j] -= factor * pi_coeffs[j];
                }
                basis[k].rhs -= factor * pi_rhs;
            }
        }
    }
}
