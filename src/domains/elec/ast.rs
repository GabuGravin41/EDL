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
    /// Optional machine-managed `.edl.layout` lockfile path (never needed by AI intent authoring).
    pub layout_lock: Option<String>,
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
    /// True if `category` was written as a quoted KiCad symbol reference (e.g. `"Device:R"` or `"Logo_2"`).
    pub is_quoted_symbol: bool,
    pub value: String,
    pub footprint: Option<String>,
    pub datasheet: Option<String>,
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
    pub dnp: bool,
    pub in_bom: bool,
    pub on_board: bool,
    /// Arbitrary extra BOM/SPICE/KiCad properties (`props { "Voltage": "16V", ... }`).
    pub props: Vec<(String, String)>,
    /// Optional component voltage rating in Volts (`voltage_rating: 16V`) for E13 derating proof.
    pub voltage_rating_v: Option<f64>,
    /// Optional component power dissipation rating in Watts (`power_rating: 0.25W`) for E14 derating proof.
    pub power_rating_w: Option<f64>,
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
    /// Optional hierarchical module path segments (e.g. `"/Power/VOUT"` -> `["Power"]`).
    pub hierarchy: Vec<String>,
    pub connects: Vec<PinRef>,
    /// Optional cross-hierarchy net links inside `connect: [..., /PrimaryStage/V3V3]`.
    pub linked_nets: Vec<(String, Span)>,
    pub power_flag: bool,
    pub span: Span,
}

/// Endpoint in a circuit idiom (`series`, `divider`, `rc_filter`): either a net name or `Comp.Pin`.
#[derive(Debug, Clone)]
pub enum ChainEndpoint {
    Net(String),
    Pin(PinRef),
}

/// High-density AI-native electrical circuit idioms inside `circuit { ... }`.
/// Each 1-line idiom expands into real components, nets, AND pin-anchored spatial layout hints.
#[derive(Debug, Clone)]
pub enum CircuitIdiomDecl {
    /// `decouple U1.VDD with [C1: "100nF", C2: "10uF"] [to: GND] [footprint: "..."]`
    Decouple {
        target: PinRef,
        caps: Vec<(String, String)>,
        ground: String,
        footprint: Option<String>,
        span: Span,
    },
    /// `pullup U1.SDA with R1: "4.7k" to V3V3 [footprint: "..."]`
    /// `pulldown U1.EN with R2: "10k" [to: GND] [footprint: "..."]`
    PullResistor {
        is_up: bool,
        target: PinRef,
        ref_id: String,
        value: String,
        rail: String,
        footprint: Option<String>,
        span: Span,
    },
    /// `divider VIN -> R1: "10k" -> FB [at: U1.FB] -> R2: "3.3k" -> GND [footprint: "..."]`
    Divider {
        top_net: String,
        r_top: (String, String),
        tap: ChainEndpoint,
        tap_net_name: Option<String>,
        r_bot: (String, String),
        bot_net: String,
        footprint: Option<String>,
        span: Span,
    },
    /// `series J1.D_P -> R1: "22" -> [L1: "10uH" ->] U1.USB_DP [footprint: "..."]`
    SeriesChain {
        from: ChainEndpoint,
        stages: Vec<(String, String)>,
        to: ChainEndpoint,
        footprint: Option<String>,
        span: Span,
    },
    /// `rc_filter VIN -> R1: "1k" -> FILT [at: U1.ADC] -> C1: "100nF" -> GND [footprint: "..."]`
    RcFilter {
        from: ChainEndpoint,
        r_id: String,
        r_val: String,
        tap: ChainEndpoint,
        tap_net_name: Option<String>,
        c_id: String,
        c_val: String,
        ground: String,
        footprint: Option<String>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct CircuitDecl {
    pub name: String,
    pub components: Vec<ComponentDecl>,
    pub nets: Vec<NetDecl>,
    pub idioms: Vec<CircuitIdiomDecl>,
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
    pub color: Option<String>,
    /// Optional explicit sheet frame rectangle `(x0, y0, x1, y1)` — obeyed literally, never re-packed.
    pub rect: Option<(Quantity, Quantity, Quantity, Quantity)>,
    pub members: Vec<String>,
    pub layout_tree: Option<LayoutNode>,
    pub notes: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SchematicDividerDecl {
    pub from: (Quantity, Quantity),
    pub to: (Quantity, Quantity),
    pub label: Option<String>,
    pub color: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SchematicLayoutDecl {
    pub arrange: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub direction: String,
    /// Optional packing mode: `"skyline"` (2D sheet-aware bin packing) or `"rows"` (default explicit rows).
    pub pack: Option<String>,
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
    pub explicit_mode: bool,
    pub default_style: Option<NetRoutingStyle>,
    pub power_style: Option<NetRoutingStyle>,
    pub ground_style: Option<NetRoutingStyle>,
    pub net_overrides: Vec<(String, NetRoutingStyle, Span)>,
}

#[derive(Debug, Clone)]
pub struct ExplicitFieldStyleDecl {
    pub name: String,
    pub value: Option<String>,
    pub hide: bool,
    pub x: f64,
    pub y: f64,
    pub orient_deg: i32,
    pub font_size: Option<f64>,
    pub justify: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExplicitPlacementDecl {
    pub component: String,
    pub unit: Option<usize>,
    pub x: Quantity,
    pub y: Quantity,
    /// When true (`place X center (x, y)`), `(x, y)` is the KiCad symbol center;
    /// when false (`place X at: (x, y)`), `(x, y)` is the body top-left (used by `edl sync`).
    pub is_center: bool,
    pub orient_deg: Option<i32>,
    pub mirror: Option<crate::domains::elec::kicad_sym::SymMirror>,
    /// Optional per-hierarchy-path RefDes overrides for reused sub-sheets (e.g. `"/ch1": "C201"`).
    pub instances: Vec<(String, String)>,
    pub ref_at: Option<(f64, f64, i32)>,
    pub val_at: Option<(f64, f64, i32)>,
    /// Exact visual field overrides (visibility, position, rotation, font size, justify).
    pub fields: Vec<ExplicitFieldStyleDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExplicitWireDecl {
    pub pts: Vec<(Quantity, Quantity)>,
    pub is_bus: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExplicitBusEntryDecl {
    pub x: Quantity,
    pub y: Quantity,
    pub dx: Quantity,
    pub dy: Quantity,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExplicitPointDecl {
    pub x: Quantity,
    pub y: Quantity,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExplicitPowerDecl {
    pub lib_id: String,
    pub value: String,
    pub reference: Option<String>,
    pub x: Quantity,
    pub y: Quantity,
    pub orient_deg: i32,
    pub mirror: Option<crate::domains::elec::kicad_sym::SymMirror>,
    pub val_at: Option<(f64, f64, i32)>,
    pub val_hide: bool,
    pub val_justify: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExplicitLabelDecl {
    pub kind: String, // "label" | "global_label" | "hierarchical_label" | "text"
    pub text: String,
    pub shape: Option<String>,
    pub x: Quantity,
    pub y: Quantity,
    pub orient_deg: i32,
    pub justify: Option<String>,
    pub font_size: Option<f64>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExplicitTextBoxDecl {
    pub text: String,
    pub x: Quantity,
    pub y: Quantity,
    pub w: Quantity,
    pub h: Quantity,
    pub orient_deg: i32,
    pub font_size: Option<f64>,
    pub justify: Option<String>,
    pub stroke_type: Option<String>,
    pub stroke_width: Option<f64>,
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExplicitGraphicDecl {
    pub kind: String, // "polyline" | "rectangle"
    pub pts: Vec<(Quantity, Quantity)>,
    pub stroke_type: Option<String>,
    pub stroke_width: Option<f64>,
    pub stroke_color: Option<String>,
    pub fill_color: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExplicitSheetPinDecl {
    pub name: String,
    pub shape: String,
    pub x: Quantity,
    pub y: Quantity,
    pub orient_deg: i32,
    pub justify: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExplicitSheetBlockDecl {
    pub name: String,
    pub file: String,
    pub x: Quantity,
    pub y: Quantity,
    pub w: Quantity,
    pub h: Quantity,
    pub name_at: Option<(f64, f64, i32)>,
    pub file_at: Option<(f64, f64, i32)>,
    pub file_justify: Option<String>,
    pub fill_color: Option<String>,
    pub pins: Vec<ExplicitSheetPinDecl>,
    pub span: Span,
}

/// Cardinal relative placement direction for AI-native coordinate-free schematic layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelDir {
    LeftOf,
    RightOf,
    Above,
    Below,
}

/// Anchor target for a relative placement: either a component (`U1`) or an exact pin (`U1.VDD`).
#[derive(Debug, Clone)]
pub struct AnchorRef {
    pub component: String,
    pub pin: Option<String>,
}

/// Relative geometric constraint in Intent Mode (zero global coordinates!).
#[derive(Debug, Clone)]
pub enum RelativePlacementKind {
    /// `place C1 below U1.VDD [gap: 5.08mm] [align_pin: "1"]`
    Directional {
        dir: RelDir,
        anchor: AnchorRef,
        gap: Option<Quantity>,
        align_pin: Option<String>,
    },
    /// `place C2 offset: (10.16mm, -5.08mm) from U1.AVDD`
    Offset {
        dx: Quantity,
        dy: Quantity,
        anchor: AnchorRef,
    },
    /// `place P2 at_angle: 45deg dist: 15.24mm from P1`
    Polar {
        angle_deg: f64,
        dist: Quantity,
        anchor: AnchorRef,
    },
    /// `align_y J1.D_P == U1.USB_DP` or `align_x R2.1 == R1.2`
    AlignAxis {
        horizontal: bool, // true = same Y (horizontal wire), false = same X (vertical wire)
        my_pin: Option<String>,
        anchor: AnchorRef,
    },
    /// `array [USB1, USB2, USB3, USB4] vertical [pitch: 25.4mm]`
    Array {
        members: Vec<String>,
        vertical: bool,
        pitch: Option<Quantity>,
    },
}

#[derive(Debug, Clone)]
pub struct RelativePlacementDecl {
    pub component: String,
    pub kind: RelativePlacementKind,
    pub orient_deg: Option<i32>,
    pub mirror: Option<crate::domains::elec::kicad_sym::SymMirror>,
    /// True for auto-generated idiom layout hints (compiler may auto-orient/push-apart);
    /// false for explicit user/AI `place` statements (obeyed literally and checked by E11/E12).
    pub is_auto: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SchematicNoteDecl {
    pub text: String,
    pub near: Option<String>, // optional group or component name
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SchematicDecl {
    pub name: String,
    pub sheet: String,
    pub sheet_file: Option<String>,
    pub title: Option<String>,
    pub rev: Option<String>,
    pub author: Option<String>,
    pub groups: Vec<SchematicGroupDecl>,
    pub dividers: Vec<SchematicDividerDecl>,
    pub layout: Option<SchematicLayoutDecl>,
    pub routing: Option<SchematicRoutingDecl>,
    pub placements: Vec<ExplicitPlacementDecl>,
    pub relative_placements: Vec<RelativePlacementDecl>,
    pub standalone_notes: Vec<SchematicNoteDecl>,
    pub explicit_wires: Vec<ExplicitWireDecl>,
    pub explicit_bus_entries: Vec<ExplicitBusEntryDecl>,
    pub explicit_junctions: Vec<ExplicitPointDecl>,
    pub explicit_no_connects: Vec<ExplicitPointDecl>,
    pub explicit_powers: Vec<ExplicitPowerDecl>,
    pub explicit_labels: Vec<ExplicitLabelDecl>,
    pub explicit_text_boxes: Vec<ExplicitTextBoxDecl>,
    pub explicit_graphics: Vec<ExplicitGraphicDecl>,
    pub explicit_sheets: Vec<ExplicitSheetBlockDecl>,
    pub bus_aliases: Vec<(String, Vec<String>)>,
    pub span: Span,
}
