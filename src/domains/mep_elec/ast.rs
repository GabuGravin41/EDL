use crate::core::geom::Side;
use crate::core::span::Span;
use crate::core::units::Quantity;
use crate::domains::arch::ast::{OpeningPlacement, WallEnd};

#[derive(Debug, Clone, Default)]
pub struct MepElecProjectData {
    pub installations: Vec<BuildingElecDecl>,
}

#[derive(Debug, Clone)]
pub struct BuildingElecDecl {
    pub name: String,
    pub supply_v: Quantity,
    pub frequency_hz: Quantity,
    pub ceiling_height: Quantity,
    pub panels: Vec<PanelDecl>,
    pub breakers: Vec<BreakerDecl>,
    pub lights: Vec<LightDecl>,
    pub switches: Vec<SwitchDecl>,
    pub outlets: Vec<OutletDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum WallMountSpec {
    /// Mounted on a room's cardinal wall face (interior side of that wall).
    RoomWall {
        room: String,
        side: Side,
        placement: OpeningPlacement,
    },
    /// Mounted on the latch (or hinge) side beside a named door opening inside `room`!
    BesideDoor {
        door_id: String,
        latch_side: bool,
        offset: Quantity,
    },
}

#[derive(Debug, Clone)]
pub struct PanelDecl {
    pub id: String,
    pub mount: WallMountSpec,
    pub main_breaker: Quantity,
    pub height: Quantity,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct BreakerDecl {
    pub id: String,
    pub kind: String, // "lighting" | "socket" | "appliance" | "dedicated"
    pub from_panel: String,
    pub rating: Quantity,
    pub cable: Quantity,
    pub rcd: Option<Quantity>,
    pub max_voltage_drop_pct: Option<f64>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum RoomPointSpec {
    Center,
    OffsetXY { x: Quantity, y: Quantity },
}

#[derive(Debug, Clone)]
pub struct LightDecl {
    pub id: String,
    pub subtype: String, // "ceiling" | "downlight" | "wall_sconce" | "linear"
    pub room: String,
    pub position: RoomPointSpec,
    pub load: Quantity,
    pub circuit: String,
    pub ip_rating: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SwitchDecl {
    pub id: String,
    pub subtype: String, // "1gang" | "2gang" | "2way" | "dimmer"
    pub room: String,
    pub mount: WallMountSpec,
    pub controls: Vec<String>,
    pub circuit: String,
    pub height: Quantity,
    pub ip_rating: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct OutletDecl {
    pub id: String,
    pub subtype: String, // "duplex" | "single" | "cooker" | "shaver" | "usb_duplex"
    pub mount: WallMountSpec,
    pub height: Quantity,
    pub load: Quantity,
    pub circuit: String,
    pub ip_rating: Option<String>,
    pub span: Span,
}

#[allow(dead_code)]
pub fn default_wall_end() -> WallEnd {
    WallEnd::Start
}
