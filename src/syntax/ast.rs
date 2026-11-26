use crate::core::span::Span;
use crate::core::units::{Quantity, Unit};
use crate::domains::arch::ast::ArchProjectData;
use crate::domains::elec::ast::{ComponentDecl, ElecProjectData, NetDecl, PinRef};
use crate::domains::mep_elec::ast::MepElecProjectData;
use crate::syntax::lexer::Token;

#[derive(Debug, Clone)]
pub enum ArgValue {
    Quantity(Quantity),
    String(String),
    Ident(String),
}

#[derive(Debug, Clone)]
pub struct ParamDecl {
    pub name: String,
    pub value: Quantity,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ModuleParamSpec {
    pub name: String,
    pub default: Option<ArgValue>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PortBindDecl {
    pub port: String,
    pub target: PinRef,
    pub span: Span,
}

/// Reusable parameterized module (supports both electronics sub-circuits and architectural room units!).
#[derive(Debug, Clone)]
pub struct ModuleDecl {
    pub name: String,
    pub file: String,
    pub params: Vec<ModuleParamSpec>,
    pub body_tokens: Vec<Token>,
    pub span: Span,
}

/// Parsed body of a module after substituting concrete parameter values.
#[derive(Debug, Clone, Default)]
pub struct ModuleBody {
    pub components: Vec<ComponentDecl>,
    pub nets: Vec<NetDecl>,
    pub port_binds: Vec<PortBindDecl>,
    pub arch: ArchProjectData,
}

/// Instantiation of a declared or imported module: `use StatusLed = LedIndicator(r: 470ohm, color: "Blue")`
#[derive(Debug, Clone)]
pub struct UseDecl {
    pub instance: String,
    pub module: String,
    pub args: Vec<(String, ArgValue, Span)>,
    /// Optional `{ PORT: NET, ... }` net-wiring bindings attached directly to the `use` statement.
    pub port_nets: Vec<(String, String, Span)>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ProjectAst {
    pub name: String,
    pub span: Span,
    pub default_unit: Unit,
    pub imports: Vec<ImportDecl>,
    /// Optional base project files overlaid by this project (`overlay "clinic_arch.edl"`).
    pub overlays: Vec<ImportDecl>,
    pub modules: Vec<ModuleDecl>,
    pub uses: Vec<UseDecl>,
    pub frames: Vec<FrameDecl>,
    pub parameters: Vec<ParamDecl>,
    pub arch: ArchProjectData,
    pub elec: ElecProjectData,
    pub mep_elec: MepElecProjectData,
    /// Declarations owned by externally registered domains (see `syntax::ext`).
    pub extensions: Vec<crate::syntax::ext::ExtensionBlock>,
    pub verifications: Vec<VerifyRule>,
    pub views: Vec<ViewDecl>,
}

/// Top-level parsed file (either a full `project "..." { ... }` or a standalone library file of `module`s).
#[derive(Debug, Clone)]
pub struct FileAst {
    pub imports: Vec<ImportDecl>,
    pub modules: Vec<ModuleDecl>,
    pub project: Option<ProjectAst>,
}

#[derive(Debug, Clone)]
pub struct FrameDecl {
    pub name: String,
    pub kind: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum VerifyRule {
    // Architectural rules
    NoRoomOverlap { span: Span },
    OpeningJambMarginMin { min: Quantity, span: Span },
    NoDoorSwingCollision { span: Span },
    AllRoomsReachable { span: Span },
    NoFixtureCollision { span: Span },
    HabitableRoomsHaveWindow { span: Span },
    // Electrical ERC rules
    AllRequiredPinsConnected { span: Span },
    NoPowerGroundShort { span: Span },
    NoMultiplePowerDrivers { span: Span },
    VoltageCompatibility { span: Span },
    NoSinglePinNets { span: Span },
    GalvanicIsolation {
        net_a: String,
        net_b: String,
        span: Span,
    },
    // Senior-engineer circuit design rules (E7-E10)
    IcDecoupling { span: Span },
    I2cPullups { span: Span },
    InductiveFlyback { span: Span },
    BomManufacturability { span: Span },
    // Electrical SPICE/MNA simulation assertions
    DcVoltageRange {
        net: String,
        min: Quantity,
        max: Quantity,
        span: Span,
    },
    BranchCurrentRange {
        component: String,
        min: Quantity,
        max: Quantity,
        span: Span,
    },
    // Building electrical (mep.elec) rules layered on top of architecture
    BreakerLoadOk { span: Span },
    VoltageDropOk { span: Span },
    WetZoneClearance { span: Span },
    SwitchAccessible { span: Span },
    ElectricalDeviceClear { span: Span },
    WetCircuitsRcdProtected { span: Span },
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
