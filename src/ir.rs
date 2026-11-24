use crate::ast::Side;
use crate::geom::{Arc2, Rect2, Segment2};
use crate::solver::EquationRecord;
use crate::span::{Diagnostic, Span};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationStatus {
    FormallyChecked,
    ExactlyChecked,
    NumericallyChecked,
    Assumed,
    NotModeled,
    Unverified,
    Violated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub check_id: String,
    pub status: VerificationStatus,
    pub method: String,
    pub tolerance_mm: Option<f64>,
    pub subjects: Vec<String>,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameIr {
    pub id: String,
    pub kind: String,
    pub dim: u8,
    pub handedness: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EntityGeometryIr {
    Rect {
        min: [f64; 2],
        max: [f64; 2],
        width_mm: f64,
        depth_mm: f64,
        area_m2: f64,
    },
    Door {
        cutout_rect: Rect2,
        opening_segment: Segment2,
        leaf_open: Segment2,
        swing_arc: Arc2,
    },
    Window {
        cutout_rect: Rect2,
        opening_segment: Segment2,
    },
    Passage {
        cutout_rect: Rect2,
        opening_segment: Segment2,
    },
    Fixture {
        category: String,
        against: Side,
        body_rect: Rect2,
        #[serde(skip_serializing_if = "Option::is_none")]
        clearance_rect: Option<Rect2>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityIr {
    pub id: String,
    pub kind: String,
    pub frame: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    pub props: BTreeMap<String, serde_json::Value>,
    pub geometry: EntityGeometryIr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedWallIr {
    pub id: String,
    pub kind: String, // "arch.wall.exterior" | "arch.wall.interior"
    pub derived_from: Vec<String>,
    pub thickness_mm: f64,
    pub clear_segment: Segment2,
    pub clear_length_mm: f64,
    pub rect: Rect2,
}

/// Exact non-overlapping wall poché cell (after corner joins, T-junctions, and opening cutouts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallSolidCell {
    pub kind: String, // "exterior" | "interior"
    pub rect: Rect2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewIr {
    pub id: String,
    pub kind: String,
    pub frame: String,
    pub sheet: String,
    pub scale: String,
    pub show_dimensions: bool,
    pub show_labels: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummaryIr {
    pub name: String,
    pub default_unit: String,
    pub exterior_footprint_mm: Option<[f64; 2]>,
    pub exterior_bounds: Option<Rect2>,
    pub total_clear_area_m2: f64,
    pub room_count: usize,
    pub opening_count: usize,
    pub fixture_count: usize,
    pub remaining_dof: Vec<String>,
    pub has_errors: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalIr {
    pub ir_version: String,
    pub project: ProjectSummaryIr,
    pub frames: Vec<FrameIr>,
    pub entities: Vec<EntityIr>,
    pub derived_walls: Vec<DerivedWallIr>,
    pub wall_solids: Vec<WallSolidCell>,
    pub constraints: Vec<EquationRecord>,
    pub views: Vec<ViewIr>,
    pub evidence: Vec<EvidenceRecord>,
    pub diagnostics: Vec<Diagnostic>,
}
