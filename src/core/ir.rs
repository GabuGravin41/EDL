use crate::core::geom::{Arc2, Point2, Rect2, Segment2, Side};
use crate::core::span::{Diagnostic, Span};
use crate::core::units::VoltageSpec;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintStrength {
    Require,
    Prefer,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EquationRecord {
    pub id: String,
    pub tier: u8,
    pub strength: ConstraintStrength,
    pub expr: String,
    pub span: Option<Span>,
    pub status: String, // "EXACTLY_CHECKED" | "REDUNDANT" | "VIOLATED" | "UNVERIFIED"
    pub residual_mm: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PortRole {
    PowerOut,
    PowerIn,
    Ground,
    Input,
    Output,
    Bidir,
    Passive,
    Nc,
}

impl PortRole {
    pub fn as_str(self) -> &'static str {
        match self {
            PortRole::PowerOut => "power_out",
            PortRole::PowerIn => "power_in",
            PortRole::Ground => "ground",
            PortRole::Input => "input",
            PortRole::Output => "output",
            PortRole::Bidir => "bidir",
            PortRole::Passive => "passive",
            PortRole::Nc => "nc",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RepairSuggestion {
    pub fix_id: usize,
    pub description: String,
    /// Whether this fix can be applied automatically (true) or requires manual intervention (false).
    pub auto_fixable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceRecord {
    pub check_id: String,
    pub status: VerificationStatus,
    pub method: String,
    pub tolerance_mm: Option<f64>,
    pub subjects: Vec<String>,
    pub details: String,
    /// Actionable repair suggestions for violated/unverified checks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repair_suggestions: Vec<RepairSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FrameIr {
    pub id: String,
    pub kind: String,
    pub dim: u8,
    pub handedness: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleProvenanceIr {
    pub module: String,
    pub instance: String,
}

/// Vector primitive for electronic/industrial schematic symbols in world mm (Y-down).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "prim", rename_all = "snake_case")]
pub enum SymPrim {
    /// Open or closed polyline in world mm.
    Polyline {
        pts: Vec<Point2>,
        filled: bool,
        stroke_mm: f64,
    },
    /// Rectangle in world mm.
    Rect {
        rect: Rect2,
        filled: bool,
        stroke_mm: f64,
    },
    /// Circle in world mm.
    Circle {
        center: Point2,
        radius: f64,
        filled: bool,
        stroke_mm: f64,
    },
    /// Small symbol-internal glyph label (e.g. `+` / `-` on polarized cap or op-amp).
    Mark {
        pos: Point2,
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlacedPinIr {
    pub name: String,
    pub number: usize,
    /// Optional exact library pin code (e.g. `"A1"`, `"7"`, `"EP"`); defaults to `number` if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_code: Option<String>,
    pub role: PortRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage: Option<VoltageSpec>,
    /// True when explicitly flagged as No-Connect (`no_connect: [...]` or `role: nc`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub no_connect: bool,
    pub side: Side,
    /// Pin attachment point on symbol body edge (sheet mm).
    pub body_point: Point2,
    /// Pin wire-connection tip (sheet mm, snapped to 2.54mm grid).
    pub tip_point: Point2,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EntityGeometryIr {
    Rect {
        min: [f64; 2],
        max: [f64; 2],
        width_mm: f64,
        depth_mm: f64,
        area_m2: f64,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        notch_rect: Option<Rect2>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lib_id: Option<String>,
        #[serde(default, skip_serializing_if = "is_zero_i32")]
        orient_deg: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mirror: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<usize>,
        #[serde(default, skip_serializing_if = "is_false")]
        hide_pin_names: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        symbol_prims: Option<Vec<SymPrim>>,
        body_rect: Rect2,
        pins: Vec<PlacedPinIr>,
    },
    MepDevice {
        device_kind: String, // "panel" | "light" | "switch" | "outlet"
        subtype: String,     // e.g. "ceiling" | "1gang" | "duplex" | "shaver" | "main_db"
        pos: Point2,
        #[serde(skip_serializing_if = "Option::is_none")]
        facing: Option<Side>, // wall normal direction into room (for wall-mounted devices)
        room: String,
        height_mm: f64,
        load_w: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        circuit: Option<String>,
    },
    /// Open-ended geometry for externally registered domains (plumbing, hvac, pcb, ...). `shape` names the
    /// payload schema owned by the domain; `data` is arbitrary JSON in project model coordinates (mm).
    Custom {
        shape: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EntityIr {
    pub id: String,
    pub kind: String,
    pub frame: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ModuleProvenanceIr>,
    pub props: BTreeMap<String, serde_json::Value>,
    pub geometry: EntityGeometryIr,
}

/// Functional connectivity relation (strictly separate from drawn wire routes).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RelationIr {
    pub id: String,
    pub kind: String, // e.g., "elec.net"
    pub name: String,
    /// Hierarchical path segments for multi-sheet schematics (e.g., vec!["PowerModule", "VOUT"]).
    /// Empty vec indicates root-level/global net. Used for generating cross-sheet reference labels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hierarchy: Vec<String>,
    pub connects: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driven_voltage_v: Option<f64>,
    pub is_ground: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// Sub-zone divider inside a framed schematic zone (e.g., "DECOUPLING CAPACITORS" or "ADDRESS SWITCHING").
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SchematicSubZoneIr {
    pub title: String,
    pub bbox: Rect2,
}

/// Presentation-layer schematic group / zone container.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SchematicGroupIr {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>, // "solid" | "dashed"
    pub members: Vec<String>,
    pub bbox: Rect2,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subzones: Vec<SchematicSubZoneIr>,
}

/// Local schematic terminal graphics (GND triangle, upward power bar, named net-label stub, or global flag).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetTerminalIr {
    pub kind: String, // "gnd_symbol" | "power_symbol" | "label_stub" | "global_flag"
    pub pos: Point2,
    pub dir: Side,
    pub label: String,
}

/// Presentation-layer orthogonal wire route (or symbol/label-stub termination) for a functional net.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SchematicRouteIr {
    pub net_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    pub style: String, // "direct" | "bus" | "rail" | "symbol" | "label_stub" | "zone_local"
    pub is_power: bool,
    pub is_ground: bool,
    pub segments: Vec<Segment2>,
    pub junctions: Vec<Point2>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub terminals: Vec<NetTerminalIr>,
    pub label_pos: Option<Point2>,
}

/// Building electrical breaker circuit summary & computed IEC/NEC metrics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MepCircuitIr {
    pub id: String,
    pub kind: String, // "lighting" | "socket" | "appliance" | "dedicated"
    pub panel_id: String,
    pub supply_v: f64,
    pub rating_a: f64,
    pub cable_mm2: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rcd_ma: Option<f64>,
    pub max_voltage_drop_pct: f64,
    pub connected_devices: Vec<String>,
    pub total_load_w: f64,
    pub design_current_a: f64,
    pub run_length_m: f64,
    pub voltage_drop_pct: f64,
    pub status: String, // "OK" | "OVERLOAD" | "VDROP_EXCEEDED" | "RCD_MISSING"
}

/// Physical conduit/wire run or switch-control line overlaid on the architectural floor plan.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MepWireRunIr {
    pub circuit_id: String,
    pub run_kind: String, // "power" | "lighting" | "switch_leg"
    pub from_id: String,
    pub to_id: String,
    pub segments: Vec<Segment2>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WallSolidCell {
    pub kind: String, // "exterior" | "interior"
    pub rect: Rect2,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ViewIr {
    pub id: String,
    pub kind: String,
    pub frame: String,
    pub sheet: String,
    pub scale: String,
    pub show_dimensions: bool,
    pub show_labels: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet_total: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

/// Exact lossless explicit schematic sheet representation (used when `routing { mode: explicit }`
/// preserves human/KiCad-authored geometry alongside EDL's semantic `circuit {}` netlist).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitFieldStyleIr {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub hide: bool,
    pub x: f64,
    pub y: f64,
    pub orient_deg: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justify: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitSymbolInstIr {
    pub ref_id: String,
    pub lib_id: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheet: Option<String>,
    pub unit: usize,
    pub x: f64,
    pub y: f64,
    pub orient_deg: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<String>,
    #[serde(default)]
    pub dnp: bool,
    pub in_bom: bool,
    pub on_board: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pin_numbers: Vec<String>,
    /// Optional per-hierarchy-path RefDes overrides for reused sub-sheets (`"/sheet1": "C201"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_at: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub val_at: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ExplicitFieldStyleIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitPowerInstIr {
    pub lib_id: String,
    pub value: String,
    pub reference: String,
    pub x: f64,
    pub y: f64,
    pub orient_deg: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub val_at: Option<[f64; 3]>,
    #[serde(default)]
    pub val_hide: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub val_justify: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitLabelIr {
    pub kind: String, // "label" | "global_label" | "hierarchical_label" | "text"
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    pub x: f64,
    pub y: f64,
    pub orient_deg: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justify: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitTextBoxIr {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub orient_deg: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justify: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitBusEntryIr {
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitGraphicIr {
    pub kind: String, // "polyline" | "rectangle"
    pub pts: Vec<Point2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitSheetPinIr {
    pub name: String,
    pub shape: String,
    pub x: f64,
    pub y: f64,
    pub orient_deg: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justify: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitSheetBlockIr {
    pub name: String,
    pub file: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_at: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_at: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_justify: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pins: Vec<ExplicitSheetPinIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExplicitSchematicSheetIr {
    pub sheet_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet_file: Option<String>,
    pub paper: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    /// Verbatim `(symbol "<lib_id>" ...)` S-expressions required by this sheet.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw_lib_symbols: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<ExplicitSymbolInstIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub powers: Vec<ExplicitPowerInstIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wires: Vec<Segment2>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buses: Vec<Segment2>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bus_entries: Vec<ExplicitBusEntryIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub junctions: Vec<Point2>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_connects: Vec<Point2>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<ExplicitLabelIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_boxes: Vec<ExplicitTextBoxIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphics: Vec<ExplicitGraphicIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sheet_blocks: Vec<ExplicitSheetBlockIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bus_aliases: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    pub explicit_sheets: Vec<ExplicitSchematicSheetIr>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub mep_circuits: Vec<MepCircuitIr>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub mep_wires: Vec<MepWireRunIr>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub constraints: Vec<EquationRecord>,
    pub views: Vec<ViewIr>,
    pub evidence: Vec<EvidenceRecord>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Generate the formal JSON Schema (Draft 7) for [`CanonicalIr`], derived directly from the Rust IR types.
pub fn canonical_ir_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(CanonicalIr);
    serde_json::to_value(&schema).expect("serialize CanonicalIr JSON Schema")
}
