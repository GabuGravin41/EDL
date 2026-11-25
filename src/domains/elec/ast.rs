use crate::core::geom::Side;
use crate::core::ir::ModuleProvenanceIr;
pub use crate::core::ir::PortRole as PinRole;
use crate::core::span::Span;
use crate::core::units::{Quantity, VoltageSpec};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct ElecProjectData {
    pub symbol_libs: Vec<String>,
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
    /// Orientation in degrees (0, 90, 180, 270; KiCad CCW convention).
    pub orient_deg: i32,
    /// Optional mirroring ("x" | "y").
    pub mirror: Option<crate::domains::elec::kicad_sym::SymMirror>,
    /// Optional unit index (1-based) for multi-unit library symbols (e.g. LM358 unit 1/2/3).
    pub unit: Option<usize>,
    /// Pins explicitly flagged No-Connect (`no_connect: [INT, NC, 7]`).
    pub no_connect: Vec<String>,
    /// True if this component is a galvanic isolation barrier (transformer / optocoupler).
    pub isolated: bool,
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

/// Declarative 2D relative layout tree inside a schematic `group` or `zone` (zero coordinate math!).
#[derive(Debug, Clone)]
pub enum LayoutNode {
    Comp(String),
    HStack(Vec<LayoutNode>),
    VStack(Vec<LayoutNode>),
    SubZone {
        title: String,
        items: Vec<LayoutNode>,
    },
}

impl LayoutNode {
    pub fn collect_components(&self, out: &mut Vec<String>) {
        match self {
            LayoutNode::Comp(c) => out.push(c.clone()),
            LayoutNode::HStack(v) | LayoutNode::VStack(v) | LayoutNode::SubZone { items: v, .. } => {
                for child in v {
                    child.collect_components(out);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SchematicGroupDecl {
    pub name: String,
    pub title: Option<String>,
    pub border: String, // "dashed" | "solid"
    pub members: Vec<String>,
    pub layout_tree: Option<LayoutNode>,
    pub notes: Vec<String>,
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
    ZoneLocal,
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
    pub title: Option<String>,
    pub rev: Option<String>,
    pub author: Option<String>,
    pub groups: Vec<SchematicGroupDecl>,
    pub layout: Option<SchematicLayoutDecl>,
    pub routing: Option<SchematicRoutingDecl>,
    pub placements: Vec<ExplicitPlacementDecl>,
    pub span: Span,
}
