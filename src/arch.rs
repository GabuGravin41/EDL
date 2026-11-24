use crate::ast::*;
use crate::geom::{Arc2, Point2, QuarterDisk2, Rect2, Segment2, EPS_MM};
use crate::ir::*;
use crate::solver::solve_rooms;
use crate::span::{Diagnostic, Severity};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone)]
struct ResolvedWallSegment {
    id: String,
    kind: String, // "arch.wall.exterior" | "arch.wall.interior"
    derived_from: Vec<String>,
    is_horizontal: bool,
    /// Clear segment along the interior face (start < end along X or Y).
    clear_segment: Segment2,
    thickness_mm: f64,
    rect: Rect2,
    room_low: Option<String>,
    room_high: Option<String>,
}

pub fn compile_project(project: &ProjectAst) -> CanonicalIr {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut evidence: Vec<EvidenceRecord> = Vec::new();

    // 1. Validate frames & defaults
    let default_frame = project
        .frames
        .first()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "FloorPlan".to_string());

    let frames: Vec<FrameIr> = project
        .frames
        .iter()
        .map(|f| FrameIr {
            id: f.name.clone(),
            kind: f.kind.clone(),
            dim: 2,
            handedness: "right".to_string(),
        })
        .collect();

    let ext_wall_mm = match project.arch_defaults.exterior_wall.to_mm() {
        Ok(v) if v > 0.0 => v,
        Ok(v) => {
            diagnostics.push(Diagnostic::error(
                "E_WALL_THICKNESS",
                "semantics",
                format!("exterior_wall thickness must be > 0mm (got {v}mm)"),
                project.arch_defaults.span.clone(),
            ));
            200.0
        }
        Err(msg) => {
            diagnostics.push(Diagnostic::error(
                "E_UNIT_MISMATCH",
                "units",
                format!("exterior_wall: {msg}"),
                project.arch_defaults.span.clone(),
            ));
            200.0
        }
    };

    let int_wall_mm = match project.arch_defaults.interior_wall.to_mm() {
        Ok(v) if v > 0.0 => v,
        Ok(v) => {
            diagnostics.push(Diagnostic::error(
                "E_WALL_THICKNESS",
                "semantics",
                format!("interior_wall thickness must be > 0mm (got {v}mm)"),
                project.arch_defaults.span.clone(),
            ));
            120.0
        }
        Err(msg) => {
            diagnostics.push(Diagnostic::error(
                "E_UNIT_MISMATCH",
                "units",
                format!("interior_wall: {msg}"),
                project.arch_defaults.span.clone(),
            ));
            120.0
        }
    };

    // 2. Solve Tier 1 + Tier 2 room linear constraints
    let solve = solve_rooms(project);
    diagnostics.extend(solve.diagnostics.clone());

    let solver_status = if solve.diagnostics.iter().any(|d| d.severity == Severity::Error) {
        VerificationStatus::Violated
    } else {
        VerificationStatus::ExactlyChecked
    };
    evidence.push(EvidenceRecord {
        check_id: "T2_linear_layout_solve".to_string(),
        status: solver_status,
        method: "rref_linear_solver_with_provenance_v0.1".to_string(),
        tolerance_mm: Some(1e-7),
        subjects: project.rooms.iter().map(|r| r.id.clone()).collect(),
        details: if solve.remaining_dof.is_empty() && solver_status == VerificationStatus::ExactlyChecked {
            format!(
                "All {} room(s) uniquely determined (0 remaining DOF, {} equations)",
                solve.room_rects.len(),
                solve.equations.len()
            )
        } else if !solve.remaining_dof.is_empty() {
            format!(
                "Under-constrained: {} DOF ({})",
                solve.remaining_dof.len(),
                solve.remaining_dof.join(", ")
            )
        } else {
            "Constraint contradiction detected".to_string()
        },
    });

    // 3. Build room entities
    let mut entities: Vec<EntityIr> = Vec::new();
    let mut total_clear_area_m2 = 0.0;
    for r in &project.rooms {
        if let Some(&rect) = solve.room_rects.get(&r.id) {
            total_clear_area_m2 += rect.area_m2();
            let mut props = BTreeMap::new();
            props.insert(
                "measurement_basis".to_string(),
                json!(project.arch_defaults.measurement_basis),
            );
            props.insert("area_m2".to_string(), json!(round4(rect.area_m2())));
            if let Some(role) = r.role {
                props.insert("role".to_string(), json!(role.as_str()));
            }
            entities.push(EntityIr {
                id: r.id.clone(),
                kind: "arch.room".to_string(),
                frame: default_frame.clone(),
                span: Some(r.span.clone()),
                props,
                geometry: EntityGeometryIr::Rect {
                    min: [round4(rect.min_x), round4(rect.min_y)],
                    max: [round4(rect.max_x), round4(rect.max_y)],
                    width_mm: round4(rect.width()),
                    depth_mm: round4(rect.depth()),
                    area_m2: round4(rect.area_m2()),
                },
            });
        }
    }

    // 4. Derive semantic walls (exterior + shared interior)
    let wall_segments = derive_semantic_walls(&solve.room_rects, ext_wall_mm, int_wall_mm);

    let derived_walls: Vec<DerivedWallIr> = wall_segments
        .iter()
        .map(|w| DerivedWallIr {
            id: w.id.clone(),
            kind: w.kind.clone(),
            derived_from: w.derived_from.clone(),
            thickness_mm: round4(w.thickness_mm),
            clear_segment: w.clear_segment,
            clear_length_mm: round4(w.clear_segment.length()),
            rect: w.rect,
        })
        .collect();

    // 5. Resolve openings onto host walls
    struct ResolvedOpening {
        id: String,
        kind: OpeningKind,
        host: WallHost,
        span: crate::span::Span,
        host_wall_id: String,
        start_margin_mm: f64,
        end_margin_mm: f64,
        cutout_rect: Rect2,
        swing_into: Option<String>,
        quarter_disk: Option<QuarterDisk2>,
    }

    let mut resolved_openings: Vec<ResolvedOpening> = Vec::new();
    let mut opening_cutouts: Vec<Rect2> = Vec::new();

    for op in &project.openings {
        let width_mm = match op.width.to_mm() {
            Ok(v) if v > 0.0 => v,
            Ok(v) => {
                diagnostics.push(
                    Diagnostic::error(
                        "E_OPENING_WIDTH",
                        "semantics",
                        format!("Opening '{}' width must be > 0mm (got {v}mm)", op.id),
                        Some(op.span.clone()),
                    )
                    .with_entities(vec![op.id.clone()]),
                );
                continue;
            }
            Err(msg) => {
                diagnostics.push(
                    Diagnostic::error(
                        "E_UNIT_MISMATCH",
                        "units",
                        format!("Opening '{}' width: {msg}", op.id),
                        Some(op.span.clone()),
                    )
                    .with_entities(vec![op.id.clone()]),
                );
                continue;
            }
        };

        let host_wall = find_host_wall(&op.host, &wall_segments);
        let Some(wall) = host_wall else {
            diagnostics.push(
                Diagnostic::error(
                    "E_HOST_WALL_NOT_FOUND",
                    "topology",
                    format!(
                        "Opening '{}' could not resolve host wall {:?} (rooms may not share a wall or be solved)",
                        op.id, op.host
                    ),
                    Some(op.span.clone()),
                )
                .with_entities(vec![op.id.clone()]),
            );
            continue;
        };

        let seg_len = wall.clear_segment.length();
        let offset_from_start = match &op.placement {
            OpeningPlacement::Center => (seg_len - width_mm) * 0.5,
            OpeningPlacement::Offset { distance, from } => match distance.to_mm() {
                Ok(d) => match from {
                    WallEnd::Start => d,
                    WallEnd::End => seg_len - d - width_mm,
                },
                Err(msg) => {
                    diagnostics.push(
                        Diagnostic::error(
                            "E_UNIT_MISMATCH",
                            "units",
                            format!("Opening '{}' offset: {msg}", op.id),
                            Some(op.span.clone()),
                        )
                        .with_entities(vec![op.id.clone()]),
                    );
                    continue;
                }
            },
        };

        let start_margin = offset_from_start;
        let end_margin = seg_len - (offset_from_start + width_mm);

        let (cutout_rect, opening_segment) = if wall.is_horizontal {
            let x0 = wall.clear_segment.p0.x + offset_from_start;
            let x1 = x0 + width_mm;
            let y_mid = 0.5 * (wall.rect.min_y + wall.rect.max_y);
            (
                Rect2::new(x0, wall.rect.min_y, x1, wall.rect.max_y),
                Segment2::new(Point2::new(x0, y_mid), Point2::new(x1, y_mid)),
            )
        } else {
            let y0 = wall.clear_segment.p0.y + offset_from_start;
            let y1 = y0 + width_mm;
            let x_mid = 0.5 * (wall.rect.min_x + wall.rect.max_x);
            (
                Rect2::new(wall.rect.min_x, y0, wall.rect.max_x, y1),
                Segment2::new(Point2::new(x_mid, y0), Point2::new(x_mid, y1)),
            )
        };

        opening_cutouts.push(cutout_rect);

        let mut props = BTreeMap::new();
        props.insert("host_wall".to_string(), json!(wall.id));
        props.insert("width_mm".to_string(), json!(round4(width_mm)));
        props.insert("start_margin_mm".to_string(), json!(round4(start_margin)));
        props.insert("end_margin_mm".to_string(), json!(round4(end_margin)));

        match op.kind {
            OpeningKind::Window => {
                entities.push(EntityIr {
                    id: op.id.clone(),
                    kind: "arch.window".to_string(),
                    frame: default_frame.clone(),
                    span: Some(op.span.clone()),
                    props,
                    geometry: EntityGeometryIr::Window {
                        cutout_rect,
                        opening_segment,
                    },
                });
                resolved_openings.push(ResolvedOpening {
                    id: op.id.clone(),
                    kind: OpeningKind::Window,
                    host: op.host.clone(),
                    span: op.span.clone(),
                    host_wall_id: wall.id.clone(),
                    start_margin_mm: start_margin,
                    end_margin_mm: end_margin,
                    cutout_rect,
                    swing_into: None,
                    quarter_disk: None,
                });
            }
            OpeningKind::Passage => {
                entities.push(EntityIr {
                    id: op.id.clone(),
                    kind: "arch.passage".to_string(),
                    frame: default_frame.clone(),
                    span: Some(op.span.clone()),
                    props,
                    geometry: EntityGeometryIr::Passage {
                        cutout_rect,
                        opening_segment,
                    },
                });
                resolved_openings.push(ResolvedOpening {
                    id: op.id.clone(),
                    kind: OpeningKind::Passage,
                    host: op.host.clone(),
                    span: op.span.clone(),
                    host_wall_id: wall.id.clone(),
                    start_margin_mm: start_margin,
                    end_margin_mm: end_margin,
                    cutout_rect,
                    swing_into: None,
                    quarter_disk: None,
                });
            }
            OpeningKind::Door => {
                let hinge = op.hinge.unwrap_or(WallEnd::Start);
                let Some(ref swing_room) = op.swing_into else {
                    diagnostics.push(
                        Diagnostic::error(
                            "E_DOOR_MISSING_SWING",
                            "semantics",
                            format!("Door '{}' requires 'swing: into <Room>'", op.id),
                            Some(op.span.clone()),
                        )
                        .with_entities(vec![op.id.clone()]),
                    );
                    continue;
                };
                let Some(&target_rect) = solve.room_rects.get(swing_room) else {
                    diagnostics.push(
                        Diagnostic::error(
                            "E_REF_UNKNOWN_ROOM",
                            "semantics",
                            format!("Door '{}' swing references unknown room '{swing_room}'", op.id),
                            Some(op.span.clone()),
                        )
                        .with_entities(vec![op.id.clone()]),
                    );
                    continue;
                };

                let arc = compute_door_arc(
                    wall,
                    offset_from_start,
                    width_mm,
                    hinge,
                    swing_room,
                    target_rect,
                );
                let Some(arc) = arc else {
                    diagnostics.push(
                        Diagnostic::error(
                            "E_DOOR_SWING_TARGET",
                            "topology",
                            format!(
                                "Door '{}' cannot swing into '{}' because '{}' is not adjacent to host wall '{}'",
                                op.id, swing_room, swing_room, wall.id
                            ),
                            Some(op.span.clone()),
                        )
                        .with_entities(vec![op.id.clone(), swing_room.clone()]),
                    );
                    continue;
                };

                let leaf_open = Segment2::new(arc.center, arc.end_point());
                let qd = QuarterDisk2::from_arc(arc);

                props.insert("swing_into".to_string(), json!(swing_room));
                props.insert(
                    "hinge".to_string(),
                    json!(match hinge {
                        WallEnd::Start => "start",
                        WallEnd::End => "end",
                    }),
                );

                entities.push(EntityIr {
                    id: op.id.clone(),
                    kind: "arch.door".to_string(),
                    frame: default_frame.clone(),
                    span: Some(op.span.clone()),
                    props,
                    geometry: EntityGeometryIr::Door {
                        cutout_rect,
                        opening_segment,
                        leaf_open,
                        swing_arc: arc,
                    },
                });

                resolved_openings.push(ResolvedOpening {
                    id: op.id.clone(),
                    kind: OpeningKind::Door,
                    host: op.host.clone(),
                    span: op.span.clone(),
                    host_wall_id: wall.id.clone(),
                    start_margin_mm: start_margin,
                    end_margin_mm: end_margin,
                    cutout_rect,
                    swing_into: Some(swing_room.clone()),
                    quarter_disk: Some(qd),
                });
            }
        }
    }

    // 6. Resolve fixtures inside rooms
    struct ResolvedFixture {
        id: String,
        room: String,
        span: crate::span::Span,
        body_rect: Rect2,
        clearance_rect: Option<Rect2>,
    }

    let mut resolved_fixtures: Vec<ResolvedFixture> = Vec::new();
    for fx in &project.fixtures {
        let Some(&room_rect) = solve.room_rects.get(&fx.room) else {
            diagnostics.push(
                Diagnostic::error(
                    "E_REF_UNKNOWN_ROOM",
                    "semantics",
                    format!("Fixture '{}' references unknown or unsolved room '{}'", fx.id, fx.room),
                    Some(fx.span.clone()),
                )
                .with_entities(vec![fx.id.clone()]),
            );
            continue;
        };

        let w_mm = match fx.size.0.to_mm() {
            Ok(v) if v > 0.0 => v,
            Ok(v) => {
                diagnostics.push(Diagnostic::error(
                    "E_FIXTURE_SIZE",
                    "semantics",
                    format!("Fixture '{}' width must be > 0mm (got {v}mm)", fx.id),
                    Some(fx.span.clone()),
                ));
                continue;
            }
            Err(msg) => {
                diagnostics.push(Diagnostic::error(
                    "E_UNIT_MISMATCH",
                    "units",
                    format!("Fixture '{}' width: {msg}", fx.id),
                    Some(fx.span.clone()),
                ));
                continue;
            }
        };
        let d_mm = match fx.size.1.to_mm() {
            Ok(v) if v > 0.0 => v,
            Ok(v) => {
                diagnostics.push(Diagnostic::error(
                    "E_FIXTURE_SIZE",
                    "semantics",
                    format!("Fixture '{}' depth must be > 0mm (got {v}mm)", fx.id),
                    Some(fx.span.clone()),
                ));
                continue;
            }
            Err(msg) => {
                diagnostics.push(Diagnostic::error(
                    "E_UNIT_MISMATCH",
                    "units",
                    format!("Fixture '{}' depth: {msg}", fx.id),
                    Some(fx.span.clone()),
                ));
                continue;
            }
        };
        let clear_mm = match fx.clearance_front {
            None => 0.0,
            Some(q) => match q.to_mm() {
                Ok(v) if v >= 0.0 => v,
                Ok(v) => {
                    diagnostics.push(Diagnostic::error(
                        "E_FIXTURE_CLEARANCE",
                        "semantics",
                        format!("Fixture '{}' clearance_front must be >= 0mm (got {v}mm)", fx.id),
                        Some(fx.span.clone()),
                    ));
                    0.0
                }
                Err(msg) => {
                    diagnostics.push(Diagnostic::error(
                        "E_UNIT_MISMATCH",
                        "units",
                        format!("Fixture '{}' clearance_front: {msg}", fx.id),
                        Some(fx.span.clone()),
                    ));
                    0.0
                }
            },
        };

        let clear_all_mm = match fx.clearance_all {
            None => 0.0,
            Some(q) => match q.to_mm() {
                Ok(v) if v >= 0.0 => v,
                Ok(v) => {
                    diagnostics.push(Diagnostic::error(
                        "E_FIXTURE_CLEARANCE",
                        "semantics",
                        format!("Fixture '{}' clearance_all must be >= 0mm (got {v}mm)", fx.id),
                        Some(fx.span.clone()),
                    ));
                    0.0
                }
                Err(msg) => {
                    diagnostics.push(Diagnostic::error(
                        "E_UNIT_MISMATCH",
                        "units",
                        format!("Fixture '{}' clearance_all: {msg}", fx.id),
                        Some(fx.span.clone()),
                    ));
                    0.0
                }
            },
        };

        let (against_opt, body_rect, clearance_rect) = match &fx.position {
            FixturePos::AgainstWall { side, placement } => {
                let wall_span_len = match side {
                    Side::North | Side::South => room_rect.width(),
                    Side::East | Side::West => room_rect.depth(),
                };
                let off = match placement {
                    OpeningPlacement::Center => (wall_span_len - w_mm) * 0.5,
                    OpeningPlacement::Offset { distance, from } => match distance.to_mm() {
                        Ok(dist) => match from {
                            WallEnd::Start => dist,
                            WallEnd::End => wall_span_len - dist - w_mm,
                        },
                        Err(msg) => {
                            diagnostics.push(Diagnostic::error(
                                "E_UNIT_MISMATCH",
                                "units",
                                format!("Fixture '{}' offset: {msg}", fx.id),
                                Some(fx.span.clone()),
                            ));
                            continue;
                        }
                    },
                };
                let (b, c) = match side {
                    Side::North => {
                        let x0 = room_rect.min_x + off;
                        let x1 = x0 + w_mm;
                        let y1 = room_rect.max_y;
                        let y0 = y1 - d_mm;
                        let clr = if clear_mm > EPS_MM {
                            Some(Rect2::new(x0, y0 - clear_mm, x1, y0))
                        } else {
                            None
                        };
                        (Rect2::new(x0, y0, x1, y1), clr)
                    }
                    Side::South => {
                        let x0 = room_rect.min_x + off;
                        let x1 = x0 + w_mm;
                        let y0 = room_rect.min_y;
                        let y1 = y0 + d_mm;
                        let clr = if clear_mm > EPS_MM {
                            Some(Rect2::new(x0, y1, x1, y1 + clear_mm))
                        } else {
                            None
                        };
                        (Rect2::new(x0, y0, x1, y1), clr)
                    }
                    Side::West => {
                        let y0 = room_rect.min_y + off;
                        let y1 = y0 + w_mm;
                        let x0 = room_rect.min_x;
                        let x1 = x0 + d_mm;
                        let clr = if clear_mm > EPS_MM {
                            Some(Rect2::new(x1, y0, x1 + clear_mm, y1))
                        } else {
                            None
                        };
                        (Rect2::new(x0, y0, x1, y1), clr)
                    }
                    Side::East => {
                        let y0 = room_rect.min_y + off;
                        let y1 = y0 + w_mm;
                        let x1 = room_rect.max_x;
                        let x0 = x1 - d_mm;
                        let clr = if clear_mm > EPS_MM {
                            Some(Rect2::new(x0 - clear_mm, y0, x0, y1))
                        } else {
                            None
                        };
                        (Rect2::new(x0, y0, x1, y1), clr)
                    }
                };
                (Some(*side), b, c)
            }
            FixturePos::FreestandingCenter => {
                let cx = room_rect.center().x;
                let cy = room_rect.center().y;
                let b = Rect2::new(cx - w_mm * 0.5, cy - d_mm * 0.5, cx + w_mm * 0.5, cy + d_mm * 0.5);
                let pad = clear_all_mm.max(clear_mm);
                let clr = if pad > EPS_MM {
                    Some(Rect2::new(b.min_x - pad, b.min_y - pad, b.max_x + pad, b.max_y + pad))
                } else {
                    None
                };
                (None, b, clr)
            }
            FixturePos::FreestandingAt { x, y } => {
                let x_mm = match x.to_mm() {
                    Ok(v) => v,
                    Err(msg) => {
                        diagnostics.push(Diagnostic::error(
                            "E_UNIT_MISMATCH",
                            "units",
                            format!("Fixture '{}' at.x: {msg}", fx.id),
                            Some(fx.span.clone()),
                        ));
                        continue;
                    }
                };
                let y_mm = match y.to_mm() {
                    Ok(v) => v,
                    Err(msg) => {
                        diagnostics.push(Diagnostic::error(
                            "E_UNIT_MISMATCH",
                            "units",
                            format!("Fixture '{}' at.y: {msg}", fx.id),
                            Some(fx.span.clone()),
                        ));
                        continue;
                    }
                };
                let x0 = room_rect.min_x + x_mm;
                let y0 = room_rect.min_y + y_mm;
                let b = Rect2::new(x0, y0, x0 + w_mm, y0 + d_mm);
                let pad = clear_all_mm.max(clear_mm);
                let clr = if pad > EPS_MM {
                    Some(Rect2::new(b.min_x - pad, b.min_y - pad, b.max_x + pad, b.max_y + pad))
                } else {
                    None
                };
                (None, b, clr)
            }
        };

        let mut props = BTreeMap::new();
        props.insert("room".to_string(), json!(fx.room));
        props.insert("category".to_string(), json!(fx.category));
        if let Some(side) = against_opt {
            props.insert("against".to_string(), json!(side.as_str()));
        } else {
            props.insert("placement".to_string(), json!("freestanding"));
        }
        props.insert("width_mm".to_string(), json!(round4(w_mm)));
        props.insert("depth_mm".to_string(), json!(round4(d_mm)));
        if clear_mm > EPS_MM {
            props.insert("clearance_front_mm".to_string(), json!(round4(clear_mm)));
        }
        if clear_all_mm > EPS_MM {
            props.insert("clearance_all_mm".to_string(), json!(round4(clear_all_mm)));
        }

        entities.push(EntityIr {
            id: fx.id.clone(),
            kind: "arch.fixture".to_string(),
            frame: default_frame.clone(),
            span: Some(fx.span.clone()),
            props,
            geometry: EntityGeometryIr::Fixture {
                category: fx.category.clone(),
                against: against_opt,
                body_rect,
                clearance_rect,
            },
        });

        resolved_fixtures.push(ResolvedFixture {
            id: fx.id.clone(),
            room: fx.room.clone(),
            span: fx.span.clone(),
            body_rect,
            clearance_rect,
        });
    }

    // 7. Grid-CSG exact non-overlapping wall solids (handles rectangles & L-shapes!)
    let (wall_solids, exterior_bounds) = build_wall_solids(
        &solve.room_rects,
        &opening_cutouts,
        ext_wall_mm,
        int_wall_mm,
    );

    // 8. Execute declared verification rules & populate evidence ledger
    let mut rules_to_run = project.verifications.clone();
    if rules_to_run.is_empty() {
        rules_to_run.push(VerifyRule::NoRoomOverlap {
            span: project.span.clone(),
        });
    }

    for rule in &rules_to_run {
        match rule {
            VerifyRule::NoRoomOverlap { span } => {
                let names: Vec<String> = solve.room_rects.keys().cloned().collect();
                let mut overlap_pairs = Vec::new();
                for i in 0..names.len() {
                    for j in (i + 1)..names.len() {
                        let a = solve.room_rects[&names[i]];
                        let b = solve.room_rects[&names[j]];
                        if a.overlaps_strict(b) {
                            let area = a.overlap_area(b);
                            overlap_pairs.push((names[i].clone(), names[j].clone(), area));
                            diagnostics.push(
                                Diagnostic::error(
                                    "E_VERIFY_ROOM_OVERLAP",
                                    "verification",
                                    format!(
                                        "Rooms '{}' and '{}' overlap by {:.0} mm^2",
                                        names[i], names[j], area
                                    ),
                                    Some(span.clone()),
                                )
                                .with_entities(vec![names[i].clone(), names[j].clone()])
                                .with_measurement(area, 0.0, "mm^2"),
                            );
                        }
                    }
                }
                evidence.push(EvidenceRecord {
                    check_id: "V1_no_room_overlap".to_string(),
                    status: if overlap_pairs.is_empty() {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_aabb_interior_overlap_v0.1".to_string(),
                    tolerance_mm: Some(EPS_MM),
                    subjects: names,
                    details: if overlap_pairs.is_empty() {
                        "0 room overlaps detected".to_string()
                    } else {
                        format!("{} room overlap pair(s) detected", overlap_pairs.len())
                    },
                });
            }

            VerifyRule::OpeningJambMarginMin { min, span } => {
                let min_mm = match min.to_mm() {
                    Ok(v) => v,
                    Err(msg) => {
                        diagnostics.push(Diagnostic::error(
                            "E_UNIT_MISMATCH",
                            "units",
                            format!("opening_jamb_margin: {msg}"),
                            Some(span.clone()),
                        ));
                        continue;
                    }
                };
                let mut violations = 0usize;
                for rop in &resolved_openings {
                    if rop.start_margin_mm < min_mm - EPS_MM {
                        violations += 1;
                        let deficit = min_mm - rop.start_margin_mm;
                        diagnostics.push(
                            Diagnostic::error(
                                "E_VERIFY_JAMB_MARGIN",
                                "verification",
                                format!(
                                    "Opening '{}' on '{}' violates start jamb margin: {:.1}mm < {:.1}mm (short by {:.1}mm)",
                                    rop.id, rop.host_wall_id, rop.start_margin_mm, min_mm, deficit
                                ),
                                Some(rop.span.clone()),
                            )
                            .with_entities(vec![rop.id.clone(), rop.host_wall_id.clone()])
                            .with_measurement(round4(rop.start_margin_mm), round4(min_mm), "mm")
                            .with_suggestion(format!(
                                "Increase '{}' start offset by at least {:.0}mm (to >= {:.0}mm)",
                                rop.id, deficit, min_mm
                            )),
                        );
                    }
                    if rop.end_margin_mm < min_mm - EPS_MM {
                        violations += 1;
                        let deficit = min_mm - rop.end_margin_mm;
                        diagnostics.push(
                            Diagnostic::error(
                                "E_VERIFY_JAMB_MARGIN",
                                "verification",
                                format!(
                                    "Opening '{}' on '{}' violates end jamb margin: {:.1}mm < {:.1}mm (short by {:.1}mm)",
                                    rop.id, rop.host_wall_id, rop.end_margin_mm, min_mm, deficit
                                ),
                                Some(rop.span.clone()),
                            )
                            .with_entities(vec![rop.id.clone(), rop.host_wall_id.clone()])
                            .with_measurement(round4(rop.end_margin_mm), round4(min_mm), "mm")
                            .with_suggestion(format!(
                                "Shift '{}' toward start by at least {:.0}mm (or reduce width)",
                                rop.id, deficit
                            )),
                        );
                    }
                }
                evidence.push(EvidenceRecord {
                    check_id: "V2_opening_jamb_margin".to_string(),
                    status: if violations == 0 {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_host_segment_margin_v0.1".to_string(),
                    tolerance_mm: Some(EPS_MM),
                    subjects: resolved_openings.iter().map(|o| o.id.clone()).collect(),
                    details: if violations == 0 {
                        format!(
                            "All {} opening(s) satisfy jamb margin >= {:.1}mm",
                            resolved_openings.len(),
                            min_mm
                        )
                    } else {
                        format!("{violations} jamb margin violation(s) (< {:.1}mm)", min_mm)
                    },
                });
            }

            VerifyRule::NoDoorSwingCollision { .. } => {
                let doors: Vec<&ResolvedOpening> = resolved_openings
                    .iter()
                    .filter(|o| o.kind == OpeningKind::Door && o.quarter_disk.is_some())
                    .collect();
                let mut collisions = 0usize;

                // (a) Door vs Door exact quarter-disk overlap
                for i in 0..doors.len() {
                    for j in (i + 1)..doors.len() {
                        let d1 = doors[i];
                        let d2 = doors[j];
                        let q1 = d1.quarter_disk.unwrap();
                        let q2 = d2.quarter_disk.unwrap();
                        if q1.overlaps_quarter_disk(q2) {
                            collisions += 1;
                            let hinge_dist = q1.arc.center.dist(q2.arc.center);
                            diagnostics.push(
                                Diagnostic::error(
                                    "E_VERIFY_DOOR_SWING_COLLISION",
                                    "verification",
                                    format!(
                                        "Door swing arcs of '{}' and '{}' collide (hinge distance {:.1}mm)",
                                        d1.id, d2.id, hinge_dist
                                    ),
                                    Some(d2.span.clone()),
                                )
                                .with_entities(vec![d1.id.clone(), d2.id.clone()])
                                .with_measurement(
                                    round4(hinge_dist),
                                    round4(q1.arc.radius + q2.arc.radius),
                                    "mm",
                                )
                                .with_suggestion(format!(
                                    "Flip swing direction, change hinge side, or increase separation between '{}' and '{}'",
                                    d1.id, d2.id
                                )),
                            );
                        }
                    }
                }

                // (b) Door vs Target Room Boundary / Wall Solids / Fixture Bodies
                for d in &doors {
                    let qd = d.quarter_disk.unwrap();
                    if let Some(ref room_id) = d.swing_into {
                        if let Some(&room_rect) = solve.room_rects.get(room_id) {
                            if !room_rect.contains_rect(qd.bbox) {
                                collisions += 1;
                                diagnostics.push(
                                    Diagnostic::error(
                                        "E_VERIFY_DOOR_SWING_WALL",
                                        "verification",
                                        format!(
                                            "Door '{}' swing arc (radius {:.0}mm) exceeds clear bounds of target room '{}'",
                                            d.id, qd.arc.radius, room_id
                                        ),
                                        Some(d.span.clone()),
                                    )
                                    .with_entities(vec![d.id.clone(), room_id.clone()]),
                                );
                            }
                        }
                        // Check against fixture bodies in the target room
                        for fx in &resolved_fixtures {
                            if fx.room == *room_id && qd.overlaps_rect(fx.body_rect) {
                                collisions += 1;
                                diagnostics.push(
                                    Diagnostic::error(
                                        "E_VERIFY_DOOR_SWING_FIXTURE",
                                        "verification",
                                        format!(
                                            "Door '{}' swing arc collides with fixture '{}' in room '{}'",
                                            d.id, fx.id, room_id
                                        ),
                                        Some(fx.span.clone()),
                                    )
                                    .with_entities(vec![d.id.clone(), fx.id.clone()])
                                    .with_suggestion(format!(
                                        "Move fixture '{}' clear of '{}' swing arc or reverse '{}' hinge/swing",
                                        fx.id, d.id, d.id
                                    )),
                                );
                            }
                        }
                    }
                    for cell in &wall_solids {
                        if qd.overlaps_rect(cell.rect) {
                            collisions += 1;
                            diagnostics.push(
                                Diagnostic::error(
                                    "E_VERIFY_DOOR_SWING_WALL",
                                    "verification",
                                    format!(
                                        "Door '{}' swing arc intersects a {} wall solid",
                                        d.id, cell.kind
                                    ),
                                    Some(d.span.clone()),
                                )
                                .with_entities(vec![d.id.clone()]),
                            );
                            break;
                        }
                    }
                }

                evidence.push(EvidenceRecord {
                    check_id: "V3_no_door_swing_collision".to_string(),
                    status: if collisions == 0 {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_analytical_quarter_disk_intersection_v0.2".to_string(),
                    tolerance_mm: Some(EPS_MM),
                    subjects: doors.iter().map(|d| d.id.clone()).collect(),
                    details: if collisions == 0 {
                        format!("0 collisions across {} door swing arc(s)", doors.len())
                    } else {
                        format!("{collisions} door swing collision(s) detected")
                    },
                });
            }

            VerifyRule::AllRoomsReachable { span } => {
                let all_rooms: Vec<String> = solve.room_rects.keys().cloned().collect();
                let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for r in &all_rooms {
                    adj.entry(r.clone()).or_default();
                }

                let mut entry_roots: Vec<String> = Vec::new();
                for rop in &resolved_openings {
                    if rop.kind != OpeningKind::Door && rop.kind != OpeningKind::Passage {
                        continue;
                    }
                    match &rop.host {
                        WallHost::Exterior { room, .. } => {
                            if rop.kind == OpeningKind::Door {
                                entry_roots.push(room.clone());
                            }
                        }
                        WallHost::SharedWall { room_a, room_b } => {
                            adj.entry(room_a.clone()).or_default().push(room_b.clone());
                            adj.entry(room_b.clone()).or_default().push(room_a.clone());
                        }
                    }
                }

                let mut visited: BTreeSet<String> = BTreeSet::new();
                let mut queue: VecDeque<String> = VecDeque::new();
                if entry_roots.is_empty() {
                    if let Some(first) = all_rooms.first() {
                        queue.push_back(first.clone());
                        visited.insert(first.clone());
                    }
                } else {
                    for r in &entry_roots {
                        if visited.insert(r.clone()) {
                            queue.push_back(r.clone());
                        }
                    }
                }

                while let Some(u) = queue.pop_front() {
                    if let Some(neighbors) = adj.get(&u) {
                        for v in neighbors {
                            if visited.insert(v.clone()) {
                                queue.push_back(v.clone());
                            }
                        }
                    }
                }

                let unreachable: Vec<String> = all_rooms
                    .iter()
                    .filter(|r| !visited.contains(*r))
                    .cloned()
                    .collect();

                if !unreachable.is_empty() {
                    diagnostics.push(
                        Diagnostic::error(
                            "E_VERIFY_UNREACHABLE_ROOM",
                            "topology",
                            format!(
                                "Unreachable room(s) from entry: [{}]",
                                unreachable.join(", ")
                            ),
                            Some(span.clone()),
                        )
                        .with_entities(unreachable.clone())
                        .with_suggestion(format!(
                            "Add a door or passage connecting {} to an adjacent reachable room",
                            unreachable.join(", ")
                        )),
                    );
                }

                evidence.push(EvidenceRecord {
                    check_id: "V4_all_rooms_reachable".to_string(),
                    status: if unreachable.is_empty() {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_bfs_room_connectivity_graph_v0.2".to_string(),
                    tolerance_mm: None,
                    subjects: all_rooms,
                    details: if unreachable.is_empty() {
                        format!(
                            "All {} room(s) reachable from entry root(s) [{}]",
                            visited.len(),
                            if entry_roots.is_empty() {
                                "default".to_string()
                            } else {
                                entry_roots.join(", ")
                            }
                        )
                    } else {
                        format!("{} unreachable room(s): {}", unreachable.len(), unreachable.join(", "))
                    },
                });
            }

            VerifyRule::NoFixtureCollision { .. } => {
                let mut violations = 0usize;

                // (a) Fixture body & clearance inside host room
                for fx in &resolved_fixtures {
                    if let Some(&room_rect) = solve.room_rects.get(&fx.room) {
                        if !room_rect.contains_rect(fx.body_rect) {
                            violations += 1;
                            diagnostics.push(
                                Diagnostic::error(
                                    "E_VERIFY_FIXTURE_BOUNDS",
                                    "verification",
                                    format!(
                                        "Fixture '{}' extends outside clear bounds of room '{}'",
                                        fx.id, fx.room
                                    ),
                                    Some(fx.span.clone()),
                                )
                                .with_entities(vec![fx.id.clone(), fx.room.clone()])
                                .with_suggestion(format!(
                                    "Reduce size or adjust offset of fixture '{}' to fit inside '{}'",
                                    fx.id, fx.room
                                )),
                            );
                        }
                        if let Some(clr) = fx.clearance_rect {
                            if !room_rect.contains_rect(clr) {
                                violations += 1;
                                diagnostics.push(
                                    Diagnostic::error(
                                        "E_VERIFY_FIXTURE_CLEARANCE",
                                        "verification",
                                        format!(
                                            "Fixture '{}' front clearance zone extends outside room '{}'",
                                            fx.id, fx.room
                                        ),
                                        Some(fx.span.clone()),
                                    )
                                    .with_entities(vec![fx.id.clone(), fx.room.clone()]),
                                );
                            }
                        }
                    }

                    // (b) Fixture body must not block any door/passage cutout on the room's perimeter
                    let expanded_fx = Rect2::new(
                        fx.body_rect.min_x - 2.0,
                        fx.body_rect.min_y - 2.0,
                        fx.body_rect.max_x + 2.0,
                        fx.body_rect.max_y + 2.0,
                    );
                    for rop in &resolved_openings {
                        if rop.kind == OpeningKind::Window {
                            continue;
                        }
                        if expanded_fx.overlaps_strict(rop.cutout_rect) {
                            violations += 1;
                            diagnostics.push(
                                Diagnostic::error(
                                    "E_VERIFY_FIXTURE_BLOCKS_OPENING",
                                    "verification",
                                    format!(
                                        "Fixture '{}' blocks opening '{}' in room '{}'",
                                        fx.id, rop.id, fx.room
                                    ),
                                    Some(fx.span.clone()),
                                )
                                .with_entities(vec![fx.id.clone(), rop.id.clone()])
                                .with_suggestion(format!(
                                    "Move fixture '{}' away from opening '{}'",
                                    fx.id, rop.id
                                )),
                            );
                        }
                    }
                }

                // (c) Fixture vs Fixture body overlap AND clearance-vs-body overlap
                for i in 0..resolved_fixtures.len() {
                    for j in (i + 1)..resolved_fixtures.len() {
                        let a = &resolved_fixtures[i];
                        let b = &resolved_fixtures[j];
                        if a.room != b.room {
                            continue;
                        }
                        if a.body_rect.overlaps_strict(b.body_rect) {
                            violations += 1;
                            diagnostics.push(
                                Diagnostic::error(
                                    "E_VERIFY_FIXTURE_OVERLAP",
                                    "verification",
                                    format!(
                                        "Fixtures '{}' and '{}' overlap in room '{}'",
                                        a.id, b.id, a.room
                                    ),
                                    Some(b.span.clone()),
                                )
                                .with_entities(vec![a.id.clone(), b.id.clone()]),
                            );
                        } else {
                            if let Some(clr_a) = a.clearance_rect {
                                if clr_a.overlaps_strict(b.body_rect) {
                                    violations += 1;
                                    diagnostics.push(
                                        Diagnostic::error(
                                            "E_VERIFY_FIXTURE_CLEARANCE",
                                            "verification",
                                            format!(
                                                "Fixture '{}' encroaches into '{}' front clearance zone in room '{}'",
                                                b.id, a.id, a.room
                                            ),
                                            Some(b.span.clone()),
                                        )
                                        .with_entities(vec![a.id.clone(), b.id.clone()]),
                                    );
                                }
                            }
                            if let Some(clr_b) = b.clearance_rect {
                                if clr_b.overlaps_strict(a.body_rect) {
                                    violations += 1;
                                    diagnostics.push(
                                        Diagnostic::error(
                                            "E_VERIFY_FIXTURE_CLEARANCE",
                                            "verification",
                                            format!(
                                                "Fixture '{}' encroaches into '{}' front clearance zone in room '{}'",
                                                a.id, b.id, a.room
                                            ),
                                            Some(a.span.clone()),
                                        )
                                        .with_entities(vec![a.id.clone(), b.id.clone()]),
                                    );
                                }
                            }
                        }
                    }
                }

                evidence.push(EvidenceRecord {
                    check_id: "V5_no_fixture_collision".to_string(),
                    status: if violations == 0 {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_fixture_aabb_and_clearance_v0.2".to_string(),
                    tolerance_mm: Some(EPS_MM),
                    subjects: resolved_fixtures.iter().map(|f| f.id.clone()).collect(),
                    details: if violations == 0 {
                        format!(
                            "0 collisions or clearance violations across {} fixture(s)",
                            resolved_fixtures.len()
                        )
                    } else {
                        format!("{violations} fixture/clearance violation(s) detected")
                    },
                });
            }

            VerifyRule::HabitableRoomsHaveWindow { span } => {
                let habitable_rooms: Vec<String> = project
                    .rooms
                    .iter()
                    .filter(|r| r.role == Some(RoomRole::Habitable))
                    .map(|r| r.id.clone())
                    .collect();

                let rooms_with_window: BTreeSet<String> = resolved_openings
                    .iter()
                    .filter(|o| o.kind == OpeningKind::Window)
                    .filter_map(|o| match &o.host {
                        WallHost::Exterior { room, .. } => Some(room.clone()),
                        WallHost::SharedWall { .. } => None,
                    })
                    .collect();

                let missing: Vec<String> = habitable_rooms
                    .iter()
                    .filter(|r| !rooms_with_window.contains(*r))
                    .cloned()
                    .collect();

                if !missing.is_empty() {
                    diagnostics.push(
                        Diagnostic::error(
                            "E_VERIFY_NATURAL_LIGHT",
                            "verification",
                            format!(
                                "Habitable room(s) missing exterior window: [{}]",
                                missing.join(", ")
                            ),
                            Some(span.clone()),
                        )
                        .with_entities(missing.clone())
                        .with_suggestion(format!(
                            "Add an exterior window opening to: {}",
                            missing.join(", ")
                        )),
                    );
                }

                evidence.push(EvidenceRecord {
                    check_id: "V6_habitable_rooms_have_window".to_string(),
                    status: if missing.is_empty() {
                        VerificationStatus::ExactlyChecked
                    } else {
                        VerificationStatus::Violated
                    },
                    method: "exact_exterior_window_membership_v0.2".to_string(),
                    tolerance_mm: None,
                    subjects: habitable_rooms.clone(),
                    details: if missing.is_empty() {
                        format!(
                            "All {} habitable room(s) have exterior natural light",
                            habitable_rooms.len()
                        )
                    } else {
                        format!("{} habitable room(s) lack a window: {}", missing.len(), missing.join(", "))
                    },
                });
            }
        }
    }

    let views: Vec<ViewIr> = project
        .views
        .iter()
        .map(|v| ViewIr {
            id: v.id.clone(),
            kind: v.kind.clone(),
            frame: if v.frame.is_empty() {
                default_frame.clone()
            } else {
                v.frame.clone()
            },
            sheet: v.sheet.clone(),
            scale: v.scale.clone(),
            show_dimensions: v.show_dimensions,
            show_labels: v.show_labels,
        })
        .collect();

    let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
    let exterior_footprint_mm = exterior_bounds.map(|b| [round4(b.width()), round4(b.depth())]);

    CanonicalIr {
        ir_version: "0.2.0".to_string(),
        project: ProjectSummaryIr {
            name: project.name.clone(),
            default_unit: project.default_unit.to_string(),
            exterior_footprint_mm,
            exterior_bounds,
            total_clear_area_m2: round4(total_clear_area_m2),
            room_count: solve.room_rects.len(),
            opening_count: resolved_openings.len(),
            fixture_count: resolved_fixtures.len(),
            remaining_dof: solve.remaining_dof,
            has_errors,
        },
        frames,
        entities,
        derived_walls,
        wall_solids,
        constraints: solve.equations,
        views,
        evidence,
        diagnostics,
    }
}

/// Derive semantic exterior and shared interior wall segments from solved room rectangles.
/// Cleanly handles both rectangular plans and L-shaped re-entrant exterior corners.
fn derive_semantic_walls(
    rooms: &BTreeMap<String, Rect2>,
    ext_wall: f64,
    int_wall: f64,
) -> Vec<ResolvedWallSegment> {
    let mut walls = Vec::new();
    let names: Vec<String> = rooms.keys().cloned().collect();

    // 1. Interior shared partitions between every pair of rooms
    for i in 0..names.len() {
        for j in (i + 1)..names.len() {
            let a_id = &names[i];
            let b_id = &names[j];
            let a = rooms[a_id];
            let b = rooms[b_id];

            for (west_id, west_r, east_id, east_r) in [(a_id, a, b_id, b), (b_id, b, a_id, a)] {
                let gap = east_r.min_x - west_r.max_x;
                if gap > EPS_MM && (gap - int_wall).abs() <= 2.0 * int_wall + EPS_MM && gap <= ext_wall * 2.0 {
                    let y0 = west_r.min_y.max(east_r.min_y);
                    let y1 = west_r.max_y.min(east_r.max_y);
                    if y1 - y0 > EPS_MM {
                        walls.push(ResolvedWallSegment {
                            id: format!("W_Shared_{west_id}_{east_id}"),
                            kind: "arch.wall.interior".to_string(),
                            derived_from: vec![west_id.clone(), east_id.clone()],
                            is_horizontal: false,
                            clear_segment: Segment2::new(
                                Point2::new(west_r.max_x, y0),
                                Point2::new(west_r.max_x, y1),
                            ),
                            thickness_mm: gap,
                            rect: Rect2::new(west_r.max_x, y0, east_r.min_x, y1),
                            room_low: Some(west_id.clone()),
                            room_high: Some(east_id.clone()),
                        });
                    }
                }
            }

            for (south_id, south_r, north_id, north_r) in [(a_id, a, b_id, b), (b_id, b, a_id, a)] {
                let gap = north_r.min_y - south_r.max_y;
                if gap > EPS_MM && (gap - int_wall).abs() <= 2.0 * int_wall + EPS_MM && gap <= ext_wall * 2.0 {
                    let x0 = south_r.min_x.max(north_r.min_x);
                    let x1 = south_r.max_x.min(north_r.max_x);
                    if x1 - x0 > EPS_MM {
                        walls.push(ResolvedWallSegment {
                            id: format!("W_Shared_{south_id}_{north_id}"),
                            kind: "arch.wall.interior".to_string(),
                            derived_from: vec![south_id.clone(), north_id.clone()],
                            is_horizontal: true,
                            clear_segment: Segment2::new(
                                Point2::new(x0, south_r.max_y),
                                Point2::new(x1, south_r.max_y),
                            ),
                            thickness_mm: gap,
                            rect: Rect2::new(x0, south_r.max_y, x1, north_r.min_y),
                            room_low: Some(south_id.clone()),
                            room_high: Some(north_id.clone()),
                        });
                    }
                }
            }
        }
    }

    // 2. Exterior walls: subtract any interval covered by an adjacent room OR an L-shape wing's exterior wall
    for (room_id, &r) in rooms {
        let covered_south = covered_intervals_horizontal(rooms, room_id, r, r.min_y, false, int_wall, ext_wall);
        for (idx, (x0, x1)) in subtract_intervals(r.min_x, r.max_x, &covered_south)
            .into_iter()
            .enumerate()
        {
            let suffix = if idx == 0 { String::new() } else { format!("_{idx}") };
            walls.push(ResolvedWallSegment {
                id: format!("W_Ext_{room_id}_south{suffix}"),
                kind: "arch.wall.exterior".to_string(),
                derived_from: vec![room_id.clone()],
                is_horizontal: true,
                clear_segment: Segment2::new(Point2::new(x0, r.min_y), Point2::new(x1, r.min_y)),
                thickness_mm: ext_wall,
                rect: Rect2::new(x0, r.min_y - ext_wall, x1, r.min_y),
                room_low: None,
                room_high: Some(room_id.clone()),
            });
        }

        let covered_north = covered_intervals_horizontal(rooms, room_id, r, r.max_y, true, int_wall, ext_wall);
        for (idx, (x0, x1)) in subtract_intervals(r.min_x, r.max_x, &covered_north)
            .into_iter()
            .enumerate()
        {
            let suffix = if idx == 0 { String::new() } else { format!("_{idx}") };
            walls.push(ResolvedWallSegment {
                id: format!("W_Ext_{room_id}_north{suffix}"),
                kind: "arch.wall.exterior".to_string(),
                derived_from: vec![room_id.clone()],
                is_horizontal: true,
                clear_segment: Segment2::new(Point2::new(x0, r.max_y), Point2::new(x1, r.max_y)),
                thickness_mm: ext_wall,
                rect: Rect2::new(x0, r.max_y, x1, r.max_y + ext_wall),
                room_low: Some(room_id.clone()),
                room_high: None,
            });
        }

        let covered_west = covered_intervals_vertical(rooms, room_id, r, r.min_x, false, int_wall, ext_wall);
        for (idx, (y0, y1)) in subtract_intervals(r.min_y, r.max_y, &covered_west)
            .into_iter()
            .enumerate()
        {
            let suffix = if idx == 0 { String::new() } else { format!("_{idx}") };
            walls.push(ResolvedWallSegment {
                id: format!("W_Ext_{room_id}_west{suffix}"),
                kind: "arch.wall.exterior".to_string(),
                derived_from: vec![room_id.clone()],
                is_horizontal: false,
                clear_segment: Segment2::new(Point2::new(r.min_x, y0), Point2::new(r.min_x, y1)),
                thickness_mm: ext_wall,
                rect: Rect2::new(r.min_x - ext_wall, y0, r.min_x, y1),
                room_low: None,
                room_high: Some(room_id.clone()),
            });
        }

        let covered_east = covered_intervals_vertical(rooms, room_id, r, r.max_x, true, int_wall, ext_wall);
        for (idx, (y0, y1)) in subtract_intervals(r.min_y, r.max_y, &covered_east)
            .into_iter()
            .enumerate()
        {
            let suffix = if idx == 0 { String::new() } else { format!("_{idx}") };
            walls.push(ResolvedWallSegment {
                id: format!("W_Ext_{room_id}_east{suffix}"),
                kind: "arch.wall.exterior".to_string(),
                derived_from: vec![room_id.clone()],
                is_horizontal: false,
                clear_segment: Segment2::new(Point2::new(r.max_x, y0), Point2::new(r.max_x, y1)),
                thickness_mm: ext_wall,
                rect: Rect2::new(r.max_x, y0, r.max_x + ext_wall, y1),
                room_low: Some(room_id.clone()),
                room_high: None,
            });
        }
    }

    walls
}

fn covered_intervals_horizontal(
    rooms: &BTreeMap<String, Rect2>,
    self_id: &str,
    self_r: Rect2,
    y_edge: f64,
    look_north: bool,
    int_wall: f64,
    ext_wall: f64,
) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for (oid, &or) in rooms {
        if oid == self_id {
            continue;
        }
        let gap = if look_north {
            or.min_y - y_edge
        } else {
            y_edge - or.max_y
        };
        if gap > -EPS_MM && gap <= int_wall * 2.5 + EPS_MM {
            // If `or` ends strictly inside self's X span (re-entrant L-corner), account for its exterior wall thickness
            let pad_left = if or.min_x > self_r.min_x + EPS_MM && or.min_x < self_r.max_x - EPS_MM {
                ext_wall
            } else {
                int_wall
            };
            let pad_right = if or.max_x > self_r.min_x + EPS_MM && or.max_x < self_r.max_x - EPS_MM {
                ext_wall
            } else {
                int_wall
            };
            out.push((or.min_x - pad_left, or.max_x + pad_right));
        }
    }
    out
}

fn covered_intervals_vertical(
    rooms: &BTreeMap<String, Rect2>,
    self_id: &str,
    self_r: Rect2,
    x_edge: f64,
    look_east: bool,
    int_wall: f64,
    ext_wall: f64,
) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for (oid, &or) in rooms {
        if oid == self_id {
            continue;
        }
        let gap = if look_east {
            or.min_x - x_edge
        } else {
            x_edge - or.max_x
        };
        if gap > -EPS_MM && gap <= int_wall * 2.5 + EPS_MM {
            let pad_bot = if or.min_y > self_r.min_y + EPS_MM && or.min_y < self_r.max_y - EPS_MM {
                ext_wall
            } else {
                int_wall
            };
            let pad_top = if or.max_y > self_r.min_y + EPS_MM && or.max_y < self_r.max_y - EPS_MM {
                ext_wall
            } else {
                int_wall
            };
            out.push((or.min_y - pad_bot, or.max_y + pad_top));
        }
    }
    out
}

fn subtract_intervals(a: f64, b: f64, cov: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut pieces = vec![(a, b)];
    for &(c0, c1) in cov {
        let mut next = Vec::new();
        for (p0, p1) in pieces {
            if c1 <= p0 + EPS_MM || c0 >= p1 - EPS_MM {
                next.push((p0, p1));
            } else {
                if c0 > p0 + EPS_MM {
                    next.push((p0, c0.min(p1)));
                }
                if c1 < p1 - EPS_MM {
                    next.push((c1.max(p0), p1));
                }
            }
        }
        pieces = next;
    }
    pieces
}

fn find_host_wall<'a>(
    host: &WallHost,
    walls: &'a [ResolvedWallSegment],
) -> Option<&'a ResolvedWallSegment> {
    match host {
        WallHost::Exterior { room, side } => {
            let prefix = format!("W_Ext_{room}_{}", side.as_str());
            walls
                .iter()
                .filter(|w| w.id == prefix || w.id.starts_with(&format!("{prefix}_")))
                .max_by(|a, b| {
                    a.clear_segment
                        .length()
                        .partial_cmp(&b.clear_segment.length())
                        .unwrap()
                })
        }
        WallHost::SharedWall { room_a, room_b } => walls.iter().find(|w| {
            w.kind == "arch.wall.interior"
                && ((w.derived_from.first().map(|s| s.as_str()) == Some(room_a.as_str())
                    && w.derived_from.get(1).map(|s| s.as_str()) == Some(room_b.as_str()))
                    || (w.derived_from.first().map(|s| s.as_str()) == Some(room_b.as_str())
                        && w.derived_from.get(1).map(|s| s.as_str()) == Some(room_a.as_str())))
        }),
    }
}

fn compute_door_arc(
    wall: &ResolvedWallSegment,
    offset_from_start: f64,
    width_mm: f64,
    hinge: WallEnd,
    swing_room: &str,
    target_rect: Rect2,
) -> Option<Arc2> {
    let is_high = wall.room_high.as_deref() == Some(swing_room);
    let is_low = wall.room_low.as_deref() == Some(swing_room);
    if !is_high && !is_low {
        return None;
    }

    if wall.is_horizontal {
        let x_lo = wall.clear_segment.p0.x + offset_from_start;
        let x_hi = x_lo + width_mm;
        let y_face = if is_high { target_rect.min_y } else { target_rect.max_y };
        let (hx, start_deg, sweep_deg) = match (is_high, hinge) {
            (true, WallEnd::Start) => (x_lo, 0.0, 90.0),
            (true, WallEnd::End) => (x_hi, 180.0, -90.0),
            (false, WallEnd::Start) => (x_lo, 0.0, -90.0),
            (false, WallEnd::End) => (x_hi, 180.0, 90.0),
        };
        Some(Arc2 {
            center: Point2::new(hx, y_face),
            radius: width_mm,
            start_deg,
            sweep_deg,
        })
    } else {
        let y_lo = wall.clear_segment.p0.y + offset_from_start;
        let y_hi = y_lo + width_mm;
        let x_face = if is_high { target_rect.min_x } else { target_rect.max_x };
        let (hy, start_deg, sweep_deg) = match (is_high, hinge) {
            (true, WallEnd::Start) => (y_lo, 90.0, -90.0),
            (true, WallEnd::End) => (y_hi, 270.0, 90.0),
            (false, WallEnd::Start) => (y_lo, 90.0, 90.0),
            (false, WallEnd::End) => (y_hi, 270.0, -90.0),
        };
        Some(Arc2 {
            center: Point2::new(x_face, hy),
            radius: width_mm,
            start_deg,
            sweep_deg,
        })
    }
}

/// Exact 2D grid-CSG wall solid derivation:
/// Produces exact, non-overlapping exterior and interior wall poché rectangles with clean
/// L-corners, re-entrant L-shape wings, T-junctions, and door/window/passage cutouts.
fn build_wall_solids(
    rooms: &BTreeMap<String, Rect2>,
    cutouts: &[Rect2],
    ext_wall: f64,
    int_wall: f64,
) -> (Vec<WallSolidCell>, Option<Rect2>) {
    if rooms.is_empty() {
        return (Vec::new(), None);
    }

    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for &r in rooms.values() {
        for x in [r.min_x - ext_wall, r.min_x, r.max_x, r.max_x + ext_wall] {
            xs.push(x);
        }
        for y in [r.min_y - ext_wall, r.min_y, r.max_y, r.max_y + ext_wall] {
            ys.push(y);
        }
        min_x = min_x.min(r.min_x - ext_wall);
        min_y = min_y.min(r.min_y - ext_wall);
        max_x = max_x.max(r.max_x + ext_wall);
        max_y = max_y.max(r.max_y + ext_wall);
    }
    for &c in cutouts {
        xs.push(c.min_x);
        xs.push(c.max_x);
        ys.push(c.min_y);
        ys.push(c.max_y);
    }

    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.dedup_by(|a, b| (*a - *b).abs() <= EPS_MM);

    let room_list: Vec<Rect2> = rooms.values().copied().collect();

    // Helper: is point strictly inside any room's exterior shell?
    let in_building_footprint = |p: Point2| -> bool {
        room_list.iter().any(|r| {
            Rect2::new(
                r.min_x - ext_wall,
                r.min_y - ext_wall,
                r.max_x + ext_wall,
                r.max_y + ext_wall,
            )
            .contains_point_strict(p)
        })
    };

    let mut raw_cells: Vec<WallSolidCell> = Vec::new();
    for iy in 0..ys.len().saturating_sub(1) {
        let y0 = ys[iy];
        let y1 = ys[iy + 1];
        if y1 - y0 <= EPS_MM {
            continue;
        }
        let mut row_cells: Vec<WallSolidCell> = Vec::new();
        for ix in 0..xs.len().saturating_sub(1) {
            let x0 = xs[ix];
            let x1 = xs[ix + 1];
            if x1 - x0 <= EPS_MM {
                continue;
            }
            let mid = Point2::new(0.5 * (x0 + x1), 0.5 * (y0 + y1));

            if room_list.iter().any(|r| r.contains_point_strict(mid)) {
                continue;
            }
            if cutouts.iter().any(|c| c.contains_point_strict(mid)) {
                continue;
            }
            if !in_building_footprint(mid) {
                continue;
            }

            // A wall cell is INTERIOR iff:
            // (a) it lies within `int_wall` of >= 2 rooms, AND
            // (b) stepping `ext_wall` in all 4 cardinal directions does not immediately expose it
            //     as an outer exterior face — specifically, check whether the cell is strictly
            //     between rooms along X or Y (or at a T/cross junction of interior partitions).
            let near_rooms = room_list
                .iter()
                .filter(|r| {
                    Rect2::new(
                        r.min_x - int_wall,
                        r.min_y - int_wall,
                        r.max_x + int_wall,
                        r.max_y + int_wall,
                    )
                    .contains_point_strict(mid)
                })
                .count();

            // Check if this cell touches outside air (within ext_wall) on any cardinal side
            let step = ext_wall * 0.6;
            let touches_outside = !in_building_footprint(Point2::new(mid.x - step, mid.y))
                || !in_building_footprint(Point2::new(mid.x + step, mid.y))
                || !in_building_footprint(Point2::new(mid.x, mid.y - step))
                || !in_building_footprint(Point2::new(mid.x, mid.y + step));

            let is_interior = near_rooms >= 2 && !touches_outside;
            let kind = if is_interior { "interior" } else { "exterior" }.to_string();
            let rect = Rect2::new(x0, y0, x1, y1);

            if let Some(last) = row_cells.last_mut() {
                if last.kind == kind && (last.rect.max_x - rect.min_x).abs() <= EPS_MM {
                    last.rect.max_x = rect.max_x;
                    continue;
                }
            }
            row_cells.push(WallSolidCell { kind, rect });
        }
        raw_cells.extend(row_cells);
    }

    // Merge vertically adjacent cells with identical X span & kind
    let mut merged: Vec<WallSolidCell> = Vec::new();
    let mut used: BTreeSet<usize> = BTreeSet::new();
    for i in 0..raw_cells.len() {
        if used.contains(&i) {
            continue;
        }
        let mut cur = raw_cells[i].clone();
        loop {
            let mut extended = false;
            for j in (i + 1)..raw_cells.len() {
                if used.contains(&j) {
                    continue;
                }
                let cand = &raw_cells[j];
                if cand.kind == cur.kind
                    && (cand.rect.min_x - cur.rect.min_x).abs() <= EPS_MM
                    && (cand.rect.max_x - cur.rect.max_x).abs() <= EPS_MM
                    && (cand.rect.min_y - cur.rect.max_y).abs() <= EPS_MM
                {
                    cur.rect.max_y = cand.rect.max_y;
                    used.insert(j);
                    extended = true;
                }
            }
            if !extended {
                break;
            }
        }
        merged.push(cur);
    }

    (merged, Some(Rect2::new(min_x, min_y, max_x, max_y)))
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
