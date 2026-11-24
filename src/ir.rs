use crate::ast::{PinRole, Side, VoltageSpec};
use crate::geom::{Arc2, Point2, Rect2, Segment2};
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
pub struct PlacedPinIr {
    pub name: String,
    pub number: usize,
    pub role: PinRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage: Option<VoltageSpec>,
    pub side: Side,
    /// Pin attachment point on symbol body edge (sheet mm).
    pub body_point: Point2,
    /// Pin wire-connection tip (sheet mm, snapped to 2.54mm grid).
    pub tip_point: Point2,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        against: Option<Side>,
        body_rect: Rect2,
        #[serde(skip_serializing_if = "Option::is_none")]
        clearance_rect: Option<Rect2>,
    },
    Component {
        category: String,
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        footprint: Option<String>,
        body_rect: Rect2,
        pins: Vec<PlacedPinIr>,
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

/// Functional connectivity relation (strictly separate from drawn wire routes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationIr {
    pub id: String,
    pub kind: String, // e.g., "elec.net"
    pub connects: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driven_voltage_v: Option<f64>,
    pub is_ground: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// Presentation-layer schematic group container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicGroupIr {
    pub name: String,
    pub members: Vec<String>,
    pub bbox: Rect2,
}

/// Presentation-layer orthogonal wire route for a functional net.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicRouteIr {
    pub net_id: String,
    pub is_power: bool,
    pub is_ground: bool,
    pub segments: Vec<Segment2>,
    pub junctions: Vec<Point2>,
    pub label_pos: Option<Point2>,
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
    pub domain: String, // "architecture" | "electronics"
    pub default_unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exterior_footprint_mm: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exterior_bounds: Option<Rect2>,
    pub total_clear_area_m2: f64,
    pub room_count: usize,
    pub opening_count: usize,
    pub fixture_count: usize,
    pub component_count: usize,
    pub net_count: usize,
    pub remaining_dof: Vec<String>,
    pub has_errors: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalIr {
    pub ir_version: String,
    pub project: ProjectSummaryIr,
    pub frames: Vec<FrameIr>,
    pub entities: Vec<EntityIr>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub relations: Vec<RelationIr>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub derived_walls: Vec<DerivedWallIr>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub wall_solids: Vec<WallSolidCell>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub schematic_groups: Vec<SchematicGroupIr>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub schematic_routes: Vec<SchematicRouteIr>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub constraints: Vec<EquationRecord>,
    pub views: Vec<ViewIr>,
    pub evidence: Vec<EvidenceRecord>,
    pub diagnostics: Vec<Diagnostic>,
}
