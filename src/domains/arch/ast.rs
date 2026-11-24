use crate::core::geom::Side;
use crate::core::ir::{ConstraintStrength, ModuleProvenanceIr};
use crate::core::span::Span;
use crate::core::units::Quantity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct ArchProjectData {
    pub defaults: ArchDefaults,
    pub rooms: Vec<RoomDecl>,
    pub layouts: Vec<LayoutBlock>,
    pub openings: Vec<OpeningDecl>,
    pub fixtures: Vec<FixtureDecl>,
}

#[derive(Debug, Clone)]
pub struct ArchDefaults {
    pub measurement_basis: String, // "clear_interior"
    pub exterior_wall: Quantity,
    pub interior_wall: Quantity,
    pub span: Option<Span>,
}

impl Default for ArchDefaults {
    fn default() -> Self {
        Self {
            measurement_basis: "clear_interior".to_string(),
            exterior_wall: Quantity::mm(200.0),
            interior_wall: Quantity::mm(120.0),
            span: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Corner {
    Southwest,
    Southeast,
    Northwest,
    Northeast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomRole {
    Habitable,
    Circulation,
    Wet,
    Service,
    Outdoor,
}

impl RoomRole {
    pub fn as_str(self) -> &'static str {
        match self {
            RoomRole::Habitable => "habitable",
            RoomRole::Circulation => "circulation",
            RoomRole::Wet => "wet",
            RoomRole::Service => "service",
            RoomRole::Outdoor => "outdoor",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnchorSpec {
    pub corner: Corner,
    pub frame: String,
    pub target: String, // "origin"
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RoomNotch {
    pub width: Quantity,
    pub depth: Quantity,
    pub corner: Corner,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RoomDecl {
    pub id: String,
    pub role: Option<RoomRole>,
    pub clear_size: Option<(Quantity, Quantity)>,
    pub anchor: Option<AnchorSpec>,
    pub notch: Option<RoomNotch>,
    pub provenance: Option<ModuleProvenanceIr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LayoutBlock {
    pub frame: String,
    pub constraints: Vec<LinearConstraintAst>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LinearConstraintAst {
    pub strength: ConstraintStrength,
    pub lhs: LinearExprAst,
    pub rhs: LinearExprAst,
    pub raw_text: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum LinearExprAst {
    RoomEdge { room: String, side: Side, span: Span },
    RoomDim { room: String, dim: RoomDimKind, span: Span },
    Param { name: String, span: Span },
    Quantity(Quantity, Span),
    Add(Box<LinearExprAst>, Box<LinearExprAst>),
    Sub(Box<LinearExprAst>, Box<LinearExprAst>),
    Scale(f64, Box<LinearExprAst>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomDimKind {
    Width,
    Depth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpeningKind {
    Door,
    Window,
    Passage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WallHost {
    Exterior { room: String, side: Side },
    SharedWall { room_a: String, room_b: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WallEnd {
    Start,
    End,
}

#[derive(Debug, Clone)]
pub enum OpeningPlacement {
    Center,
    Offset { distance: Quantity, from: WallEnd },
}

#[derive(Debug, Clone)]
pub struct OpeningDecl {
    pub id: String,
    pub kind: OpeningKind,
    pub host: WallHost,
    pub width: Quantity,
    pub placement: OpeningPlacement,
    pub hinge: Option<WallEnd>,
    pub swing_into: Option<String>,
    pub provenance: Option<ModuleProvenanceIr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum FixturePos {
    AgainstWall { side: Side, placement: OpeningPlacement },
    FreestandingCenter,
    FreestandingAt { x: Quantity, y: Quantity },
}

#[derive(Debug, Clone)]
pub struct FixtureDecl {
    pub id: String,
    pub category: String,
    pub room: String,
    pub size: (Quantity, Quantity),
    pub position: FixturePos,
    pub clearance_front: Option<Quantity>,
    pub clearance_all: Option<Quantity>,
    pub provenance: Option<ModuleProvenanceIr>,
    pub span: Span,
}
