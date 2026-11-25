use crate::core::geom::Side;
use crate::core::ir::ModuleProvenanceIr;
pub use crate::core::ir::PortRole as PinRole;
use crate::core::span::Span;
use crate::core::units::{Quantity, VoltageSpec};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct ElecProjectData {
    pub circuits: Vec<CircuitDecl>,
    pub schematics: Vec<SchematicDecl>,
}

#[derive(Debug, Clone)]
pub struct PinDecl {
    pub name: String,
    pub role: PinRole,
    pub voltage: Option<VoltageSpec>,
    pub side: Option<Side>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ComponentDecl {
    pub id: String,
    pub category: String,
    pub value: String,
    pub footprint: Option<String>,
    pub pins: Vec<PinDecl>,
    pub provenance: Option<ModuleProvenanceIr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PinRef {
    pub component: String,
    pub pin: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct NetDecl {
    pub name: String,
    pub connects: Vec<PinRef>,
    pub power_flag: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CircuitDecl {
    pub name: String,
    pub components: Vec<ComponentDecl>,
    pub nets: Vec<NetDecl>,
    pub uses: Vec<crate::syntax::ast::UseDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SchematicGroupDecl {
    pub name: String,
    pub members: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SchematicLayoutDecl {
    pub arrange: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub direction: String,
    pub group_gap: Option<Quantity>,
    pub member_gap: Option<Quantity>,
    pub row_gap: Option<Quantity>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NetRoutingStyle {
    Direct,
    Bus,
    Rail,
    Symbol,
    LabelStub,
}

#[derive(Debug, Clone, Default)]
pub struct SchematicRoutingDecl {
    pub default_style: Option<NetRoutingStyle>,
    pub power_style: Option<NetRoutingStyle>,
    pub ground_style: Option<NetRoutingStyle>,
    pub net_overrides: Vec<(String, NetRoutingStyle, Span)>,
}

#[derive(Debug, Clone)]
pub struct ExplicitPlacementDecl {
    pub component: String,
    pub x: Quantity,
    pub y: Quantity,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SchematicDecl {
    pub name: String,
    pub sheet: String,
    pub groups: Vec<SchematicGroupDecl>,
    pub layout: Option<SchematicLayoutDecl>,
    pub routing: Option<SchematicRoutingDecl>,
    pub placements: Vec<ExplicitPlacementDecl>,
    pub span: Span,
}
