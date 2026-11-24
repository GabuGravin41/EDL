use crate::span::Span;
use crate::units::{Quantity, Unit};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ProjectAst {
    pub name: String,
    pub span: Span,
    pub default_unit: Unit,
    pub frames: Vec<FrameDecl>,
    pub arch_defaults: ArchDefaults,
    pub rooms: Vec<RoomDecl>,
    pub layouts: Vec<LayoutBlock>,
    pub openings: Vec<OpeningDecl>,
    pub fixtures: Vec<FixtureDecl>,
    pub verifications: Vec<VerifyRule>,
    pub views: Vec<ViewDecl>,
}

#[derive(Debug, Clone)]
pub struct FrameDecl {
    pub name: String,
    pub kind: String, // e.g., "Frame2"
    pub span: Span,
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
pub enum Side {
    West,
    East,
    South,
    North,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::West => "west",
            Side::East => "east",
            Side::South => "south",
            Side::North => "north",
        }
    }
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
pub struct RoomDecl {
    pub id: String,
    pub role: Option<RoomRole>,
    pub clear_size: Option<(Quantity, Quantity)>,
    pub anchor: Option<AnchorSpec>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintStrength {
    Require,
    Prefer,
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
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FixtureDecl {
    pub id: String,
    pub category: String, // bed, sofa, counter, table, toilet, vanity, shower, wardrobe, desk, etc.
    pub room: String,
    /// (width along anchored wall, depth projecting into room)
    pub size: (Quantity, Quantity),
    pub against: Side,
    pub placement: OpeningPlacement,
    pub clearance_front: Option<Quantity>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum VerifyRule {
    NoRoomOverlap { span: Span },
    OpeningJambMarginMin { min: Quantity, span: Span },
    NoDoorSwingCollision { span: Span },
    AllRoomsReachable { span: Span },
    NoFixtureCollision { span: Span },
    HabitableRoomsHaveWindow { span: Span },
}

#[derive(Debug, Clone)]
pub struct ViewDecl {
    pub id: String,
    pub kind: String,
    pub frame: String,
    pub sheet: String,
    pub scale: String,
    pub show_dimensions: bool,
    pub show_labels: bool,
    pub span: Span,
}
